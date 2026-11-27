// Labour priorities (docs/plans/messages.md, M11 Phase 2; GDD 5.3): the houses' workers handed out
// city-wide to the employers by the sectors' order (sim::labour::CityLabour), in place of each
// employer's own search. Every `LabourRules::allocate_secs`, and at once when the order changes or
// a building is paused or resumed:
//   1. the supply: each finished house's workers, employed or not;
//   2. the demand: each employer that can work (finished, not paused, not burning, linked to a
//      road), by sector;
//   3. sector by sector in priority order, each building first gets its minimum (so as many as
//      possible run: one that can't reach it gets none), then the rest up to its maximum, from the
//      houses within `workers_search_radius` along the roads, nearest first; within a sector in
//      pool order, so the same city gives the same allocation;
//   4. the result replaces the employers' households and the houses' employers. It's worked out
//      afresh each time, so workers stay put while nothing changes.
// The `freeze_labour` cheat (the test harness's) stops it: workers stay where they are.

use std::collections::BTreeMap;

use serde::Serialize;
use strum::IntoEnumIterator;

use crate::{
    building::{Building, BuildingId, BuildingKind, BuildingKindAndId},
    cheats,
    config::GameConfigs,
    pathfind::NodeKind as PathNodeKind,
    sim::{
        SimContext,
        SimContextMut,
        labour::{LABOUR_SECTOR_COUNT, LabourSector},
    },
    tile::TileKind,
    world::object::GameObject,
};

// ----------------------------------------------
// Sectors
// ----------------------------------------------

// The sector a building counts under: its tile's (LabourSector::of_tile_sector).
pub fn sector_of(context: &SimContext, building: &Building) -> LabourSector {
    let sector = context
        .tile_map()
        .find_tile(building.base_cell(), TileKind::Building)
        .map(|tile| tile.tile_def().sector)
        .unwrap_or_default();
    LabourSector::of_tile_sector(sector)
}

// Whether an employer takes part in the allocation: finished, not paused, not burning or about
// to be destroyed, and linked to a road (its workers come along the roads).
pub fn can_work(building: &Building) -> bool {
    building.has_jobs()
        && !building.is_under_construction()
        && !building.is_paused()
        && !building.risks().is_burning()
        && !building.risks().is_doomed()
        && building.is_linked_to_road()
}

// ----------------------------------------------
// Allocation
// ----------------------------------------------

struct Job {
    building: BuildingKindAndId,
    road_link: common::coords::Cell,
    min: u32,
    max: u32,
    // The houses with workers within reach, nearest first; read when first needed.
    reach: Option<Vec<BuildingId>>,
    // What it's given: workers by house.
    given: BTreeMap<BuildingId, u32>,
    count: u32,
}

impl Job {
    fn reach(&mut self, context: &SimContext, supply: &BTreeMap<BuildingId, u32>) -> &[BuildingId] {
        let road_link = self.road_link;
        self.reach.get_or_insert_with(|| houses_in_reach(context, road_link, supply))
    }

    // Takes up to `wanted` workers from the houses in reach, nearest first.
    fn take(
        &mut self,
        context: &SimContext,
        supply: &BTreeMap<BuildingId, u32>,
        left: &mut BTreeMap<BuildingId, u32>,
        wanted: u32,
    ) {
        let mut wanted = wanted;
        let reach = self.reach(context, supply).to_vec();
        for house in reach {
            if wanted == 0 {
                break;
            }
            let Some(available) = left.get_mut(&house) else {
                continue;
            };
            let taken = (*available).min(wanted);
            if taken == 0 {
                continue;
            }
            *available -= taken;
            wanted -= taken;
            *self.given.entry(house).or_insert(0) += taken;
            self.count += taken;
        }
    }

    fn available(
        &mut self,
        context: &SimContext,
        supply: &BTreeMap<BuildingId, u32>,
        left: &BTreeMap<BuildingId, u32>,
    ) -> u32 {
        self.reach(context, supply).iter().map(|house| left.get(house).copied().unwrap_or(0)).sum()
    }
}

// The houses with workers whose road link is within `workers_search_radius` of `road_link` along
// the roads, nearest first.
fn houses_in_reach(
    context: &SimContext,
    road_link: common::coords::Cell,
    supply: &BTreeMap<BuildingId, u32>,
) -> Vec<BuildingId> {
    let radius = GameConfigs::get().sim.workers_search_radius;
    let mut houses = Vec::new();
    context.find_nearest_buildings(road_link, BuildingKind::House, PathNodeKind::Road, Some(radius), |house, _path| {
        let id = house.id();
        if supply.contains_key(&id) && !houses.contains(&id) {
            houses.push(id);
        }
        true // Go on: every house in reach.
    });
    houses
}

// Hands the houses' workers out to the employers by the sectors' priorities (above). Does nothing
// while the `freeze_labour` cheat is on.
pub fn allocate(context: &SimContextMut) {
    if cheats::get().freeze_labour {
        return;
    }

    // The supply: each finished house's workers.
    let mut supply: BTreeMap<BuildingId, u32> = BTreeMap::new();
    context.world().for_each_building(BuildingKind::House, |house| {
        if !house.is_under_construction()
            && let Some(pool) = house.workers().and_then(|workers| workers.as_household_worker_pool())
            && pool.total_workers() > 0
        {
            supply.insert(house.id(), pool.total_workers());
        }
        true
    });

    // The demand: each employer that can work, by sector, in pool order.
    let mut sectors: [Vec<Job>; LABOUR_SECTOR_COUNT] = Default::default();
    context.world().for_each_building_of_any_kind(|building| {
        if can_work(building)
            && let Some(employer) = building.workers().and_then(|workers| workers.as_employer())
        {
            sectors[sector_of(context, building) as usize].push(Job {
                building: building.kind_and_id(),
                road_link: building.road_link().unwrap(),
                min: employer.min_employees(),
                max: employer.max_employees(),
                reach: None,
                given: BTreeMap::new(),
                count: 0,
            });
        }
        true
    });

    // Sector by sector: minimums first, then the rest.
    let mut left = supply.clone();
    for sector in context.labour().order().to_vec() {
        let jobs = &mut sectors[sector as usize];
        if left.values().all(|count| *count == 0) {
            break;
        }
        for job in jobs.iter_mut() {
            if job.min > 0 && job.available(context, &supply, &left) >= job.min {
                job.take(context, &supply, &mut left, job.min);
            }
        }
        for job in jobs.iter_mut() {
            if job.count >= job.min && job.count < job.max {
                job.take(context, &supply, &mut left, job.max - job.count);
            }
        }
    }

    // The result: every employer's households (none for those left out) and every house's
    // employers.
    let mut given: BTreeMap<BuildingKindAndId, BTreeMap<BuildingId, u32>> = BTreeMap::new();
    let mut employers: BTreeMap<BuildingId, BTreeMap<BuildingKindAndId, u32>> = BTreeMap::new();
    for job in sectors.into_iter().flatten() {
        for (house, count) in &job.given {
            employers.entry(*house).or_default().insert(job.building, *count);
        }
        if !job.given.is_empty() {
            given.insert(job.building, job.given);
        }
    }
    context.world_mut().for_each_building_of_any_kind_mut(|building| {
        if building.is(BuildingKind::House) {
            building.assign_employers(employers.remove(&building.id()).unwrap_or_default());
        } else if building.workers().is_some_and(|workers| workers.is_employer()) {
            building.assign_employees(given.remove(&building.kind_and_id()).unwrap_or_default());
        }
        true
    });
}

// Pauses or resumes a workplace: paused, its workers go and it gets none (so it stops as an
// unstaffed building does; a store keeps its goods); resumed, it's staffed again. The workers are
// handed out again at once. Returns the building's paused state, or None when it has no jobs.
pub fn set_paused(context: &SimContextMut, handle: BuildingKindAndId, paused: bool) -> Option<bool> {
    let building = context.world_mut().find_building_mut(handle.kind, handle.id)?;
    if !building.has_jobs() {
        return None;
    }
    if building.is_paused() != paused {
        building.set_paused(paused);
        if paused {
            building.remove_all_workers(context);
        }
        allocate(context);
    }
    Some(paused)
}

// Moves a sector to a priority (from 1) and hands the workers out again at once. Returns whether
// the order changed.
pub fn set_priority(context: &SimContextMut, sector: LabourSector, priority: usize) -> bool {
    let changed = context.labour_mut().set_priority(sector, priority);
    if changed {
        allocate(context);
    }
    changed
}

// ----------------------------------------------
// Report
// ----------------------------------------------

// A workplace in a sector's report.
#[derive(Clone, Serialize)]
pub struct WorkplaceReport {
    pub building: BuildingKind,
    pub cell: common::coords::Cell,
    pub name: &'static str,
    pub min: u32,
    pub needed: u32,
    pub employed: u32,
    pub paused: bool,
    // Takes part in the allocation (can_work).
    pub working: bool,
}

// A sector on the Labour page: its priority, its jobs (those that can work: a paused building's
// aren't open) and their workers.
#[derive(Clone, Serialize)]
pub struct SectorReport {
    pub sector: LabourSector,
    pub priority: usize,
    pub needed: u32,
    pub employed: u32,
    pub short: u32,
    pub paused: u32,
    pub buildings: Vec<WorkplaceReport>,
}

#[derive(Clone, Serialize)]
pub struct LabourReport {
    // The sectors in priority order.
    pub sectors: Vec<SectorReport>,
    // The houses' workers, those employed and those looking for work.
    pub workforce: u32,
    pub employed: u32,
    pub looking_for_work: u32,
    // The open jobs no one fills.
    pub jobs_not_filled: u32,
}

impl LabourReport {
    pub fn sector(&self, sector: LabourSector) -> Option<&SectorReport> {
        self.sectors.iter().find(|report| report.sector == sector)
    }
}

pub fn report(context: &SimContext) -> LabourReport {
    let labour = context.labour();
    let mut sectors: Vec<SectorReport> = labour
        .order()
        .iter()
        .enumerate()
        .map(|(index, sector)| SectorReport {
            sector: *sector,
            priority: index + 1,
            needed: 0,
            employed: 0,
            short: 0,
            paused: 0,
            buildings: Vec::new(),
        })
        .collect();

    let (mut workforce, mut employed) = (0, 0);
    context.world().for_each_building_of_any_kind(|building| {
        let Some(workers) = building.workers() else {
            return true;
        };
        if let Some(pool) = workers.as_household_worker_pool() {
            if !building.is_under_construction() {
                workforce += pool.total_workers();
                employed += pool.employed_count();
            }
        } else if let Some(employer) = workers.as_employer()
            && employer.max_employees() > 0
            && !building.is_under_construction()
        {
            let sector = sector_of(context, building);
            let report = sectors.iter_mut().find(|report| report.sector == sector).unwrap();
            let working = can_work(building);
            if working {
                report.needed += employer.max_employees();
                report.employed += employer.employee_count();
            }
            if building.is_paused() {
                report.paused += 1;
            }
            report.buildings.push(WorkplaceReport {
                building: building.kind(),
                cell: building.base_cell(),
                name: building.name(),
                min: employer.min_employees(),
                needed: employer.max_employees(),
                employed: employer.employee_count(),
                paused: building.is_paused(),
                working,
            });
        }
        true
    });

    for report in &mut sectors {
        report.short = report.needed.saturating_sub(report.employed);
    }
    let jobs_not_filled = sectors.iter().map(|report| report.short).sum();
    LabourReport { sectors, workforce, employed, looking_for_work: workforce.saturating_sub(employed), jobs_not_filled }
}

// Every sector, in the mockup's order (a new game's).
pub fn all_sectors() -> impl Iterator<Item = LabourSector> {
    LabourSector::iter()
}
