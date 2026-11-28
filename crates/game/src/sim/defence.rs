// The city's defence and the raids it answers (docs/plans/defence.md). Saved in `Simulation` and
// read through `SimContext::defence()`.
//
// A game has raids when its mission's `defence` block lists them (campaign::config::MissionDefenceDef),
// each scheduled from the mission's start, or when its custom game's Raids switch is on: then a raid
// comes every few years, drawn from the simulation's RNG (decision Q9). The state keeps the raids to
// come, those on the map, the log of those that ended, and each wall cell's condition.

use common::coords::Cell;
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::{
    campaign::config::MissionDefenceDef,
    config::DefenceRules,
    sim::resources::{RESOURCE_KIND_COUNT, per_kind_map},
    unit::UnitId,
};

// ----------------------------------------------
// WallTier
// ----------------------------------------------

// A wall's tier (GDD 5.16: wood, earth, then brick or stone; decision Q3), each its own tile and
// strength; a gatehouse takes its walls' tier.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WallTier {
    Palisade,
    Rampart,
    CityWall,
}

impl WallTier {
    pub const ALL: [WallTier; 3] = [WallTier::Palisade, WallTier::Rampart, WallTier::CityWall];

    // Its tile's name, and its name in data and automation.
    pub fn name(self) -> &'static str {
        match self {
            Self::Palisade => "palisade",
            Self::Rampart => "rampart",
            Self::CityWall => "city_wall",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|tier| tier.name() == name)
    }

    // What a new piece of it holds out against (DefenceRules).
    pub fn strength(self, rules: &DefenceRules) -> f32 {
        match self {
            Self::Palisade => rules.palisade_strength,
            Self::Rampart => rules.rampart_strength,
            Self::CityWall => rules.city_wall_strength,
        }
        .max(1.0)
    }
}

// ----------------------------------------------
// RaidKind, MapSide, RaidFrom
// ----------------------------------------------

// What a raid is after (decision Q6): raiders go for loot (stores, granaries, the treasury through a
// tax office) and houses; a war party for the palace too.
#[derive(Copy, Clone, Default, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RaidKind {
    #[default]
    Raiders,
    WarParty,
}

impl RaidKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Raiders => "raiders",
            Self::WarParty => "war_party",
        }
    }
}

// A side of the map, as the grid names them (north is +x, the upper-right edge on screen; east is -y).
#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MapSide {
    North,
    East,
    South,
    West,
}

impl MapSide {
    pub fn name(self) -> &'static str {
        match self {
            Self::North => "north",
            Self::East => "east",
            Self::South => "south",
            Self::West => "west",
        }
    }
}

// Where a raid comes in: a side of the map (its edge's middle, where raiders can stand) or a cell.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RaidFrom {
    Side(MapSide),
    Cell([i32; 2]),
}

// ----------------------------------------------
// ScheduledRaid, RaidUnderWay, RaidRecord
// ----------------------------------------------

// A raid to come: the mission's (or a custom game's random one), its ordinal months (year x 12 +
// month - 1) of arrival and of first warning, and the warnings sent so far.
#[derive(Clone, Serialize, Deserialize)]
pub struct ScheduledRaid {
    pub id: String,
    pub kind: RaidKind,
    pub from: RaidFrom,
    pub size: u32,
    // A Realm text key naming the enemy in its warnings; None: its kind's words.
    pub enemy: Option<String>,
    pub arrives: u32,
    pub warn_from: u32,
    pub warnings_sent: u32,
}

impl ScheduledRaid {
    // The months left until it arrives, at `now` (an ordinal month); 0 in its month.
    #[inline]
    pub fn months_left(&self, now: u32) -> u32 {
        self.arrives.saturating_sub(now)
    }
}

// What a raid cost the city, so far or in all.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct RaidLosses {
    // The city's walkers and militia killed (each a resident lost: decision Q16).
    pub walkers_killed: u32,
    pub militia_killed: u32,
    // All the residents lost: one for each death, all of a house's when a raider destroyed it.
    pub residents_lost: u32,
    pub buildings_destroyed: u32,
    pub houses_destroyed: u32,
    #[serde(with = "per_kind_map")]
    pub goods_taken: [u32; RESOURCE_KIND_COUNT],
    pub gold_taken: u32,
}

impl RaidLosses {
    // Whether the raid took nothing and destroyed nothing: repelled (docs/plans/defence.md, §2).
    pub fn is_none(&self) -> bool {
        self.buildings_destroyed == 0 && self.gold_taken == 0 && self.goods_taken.iter().all(|count| *count == 0)
    }
}

// A raid on the map: its raiders (those spawned so far: they come in over a few seconds), how many
// have fallen, and its losses so far.
#[derive(Clone, Serialize, Deserialize)]
pub struct RaidUnderWay {
    pub id: String,
    pub kind: RaidKind,
    pub entry: Cell,
    pub size: u32,
    pub arrived: u32,
    pub spawned: u32,
    pub raiders: Vec<UnitId>,
    pub fallen: u32,
    pub losses: RaidLosses,
}

// A raid that ended: everyone it sent left the map or fell.
#[derive(Clone, Serialize, Deserialize)]
pub struct RaidRecord {
    pub id: String,
    pub kind: RaidKind,
    pub size: u32,
    pub arrived: u32,
    pub ended: u32,
    pub fallen: u32,
    pub losses: RaidLosses,
    // The palace (or citadel) was destroyed (decision Q10).
    pub palace_lost: bool,
}

impl RaidRecord {
    // Repelled: it destroyed and took nothing (the `RaidsRepelled` goal, Security's driver).
    #[inline]
    pub fn repelled(&self) -> bool {
        self.losses.is_none() && !self.palace_lost
    }
}

// ----------------------------------------------
// WallCondition
// ----------------------------------------------

// A wall cell's condition, 0 to its tier's strength (docs/plans/defence.md, §1): raiders lower it, it
// breaches at 0, and a builders' guild walker beside it repairs it; `repaired` is the ordinal month
// one last did.
#[derive(Copy, Clone, Serialize, Deserialize)]
pub struct WallCondition {
    pub cell: Cell,
    pub tier: WallTier,
    pub condition: f32,
    pub repaired: Option<u32>,
}

impl WallCondition {
    #[inline]
    pub fn strength(&self, rules: &DefenceRules) -> f32 {
        self.tier.strength(rules)
    }

    // Below the rules' share of its strength: it shows its cracks.
    #[inline]
    pub fn is_damaged(&self, rules: &DefenceRules) -> bool {
        self.condition < self.strength(rules) * rules.damaged_below
    }
}

// ----------------------------------------------
// DefenceNotice
// ----------------------------------------------

// What the message log hears of the defence (GameLoop::notify_defence; runtime only).
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum DefenceNotice {
    // A wall piece gave way: rubble in its place.
    Breach { cell: Cell, tier: WallTier },
}

// ----------------------------------------------
// CityDefence
// ----------------------------------------------

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct CityDefence {
    // A custom game with its Raids switch on: raids come at random (decision Q9), the next arriving
    // in `next_random_raid` (an ordinal month).
    random_raids: bool,
    next_random_raid: Option<u32>,
    // The raids to come, by arrival (then by the mission's order).
    scheduled: Vec<ScheduledRaid>,
    under_way: Vec<RaidUnderWay>,
    log: Vec<RaidRecord>,
    // Each wall cell's condition, in row order.
    walls: Vec<WallCondition>,

    // For the message log (runtime only).
    #[serde(skip)]
    notices: Vec<DefenceNotice>,
}

impl CityDefence {
    pub fn new() -> Self {
        Self::default()
    }

    // A game starts at `now` (an ordinal month): its mission's raids, or a custom game's random ones
    // (the first drawn from `rng` only then, so other games draw nothing).
    pub fn start(
        &mut self,
        mission: Option<&MissionDefenceDef>,
        random_raids: bool,
        now: u32,
        rules: &DefenceRules,
        rng: &mut impl Rng,
    ) {
        // A new game's raids; the walls on its map stay as they are.
        let walls = std::mem::take(&mut self.walls);
        *self = Self::new();
        self.walls = walls;
        if let Some(mission) = mission {
            for raid in &mission.raids {
                let arrives = now + raid.month;
                let warning = raid.warning_months.unwrap_or(rules.warning_months);
                self.scheduled.push(ScheduledRaid {
                    id: raid.id.clone(),
                    kind: raid.kind,
                    from: raid.from,
                    size: raid.size,
                    enemy: raid.enemy.clone(),
                    arrives,
                    warn_from: arrives.saturating_sub(warning).max(now),
                    warnings_sent: 0,
                });
            }
            self.scheduled.sort_by_key(|raid| raid.arrives); // A stable sort keeps the mission's order.
        }
        self.set_random_raids(random_raids, now, rules, rng);
    }

    // Turns a custom game's random raids on (the next drawn from `now`) or off.
    pub fn set_random_raids(&mut self, on: bool, now: u32, rules: &DefenceRules, rng: &mut impl Rng) {
        self.random_raids = on;
        self.next_random_raid = on.then(|| now + random_raid_months(rules, rng));
    }

    #[inline]
    pub fn random_raids(&self) -> bool {
        self.random_raids
    }

    #[inline]
    pub fn next_random_raid(&self) -> Option<u32> {
        self.next_random_raid
    }

    #[inline]
    pub fn scheduled(&self) -> &[ScheduledRaid] {
        &self.scheduled
    }

    #[inline]
    pub fn under_way(&self) -> &[RaidUnderWay] {
        &self.under_way
    }

    #[inline]
    pub fn log(&self) -> &[RaidRecord] {
        &self.log
    }

    #[inline]
    pub fn walls(&self) -> &[WallCondition] {
        &self.walls
    }

    #[inline]
    pub fn has_raids(&self) -> bool {
        self.random_raids || !self.scheduled.is_empty() || !self.under_way.is_empty()
    }

    // Moves a scheduled raid to arrive in `arrives` (an ordinal month), its first warning as far
    // before it as it was (automation's `defence.set`). False if there's no such raid.
    pub fn set_raid_arrival(&mut self, id: &str, arrives: u32) -> bool {
        let Some(raid) = self.scheduled.iter_mut().find(|raid| raid.id == id) else {
            return false;
        };
        let warning = raid.arrives - raid.warn_from;
        raid.arrives = arrives;
        raid.warn_from = arrives.saturating_sub(warning);
        raid.warnings_sent = 0;
        self.scheduled.sort_by_key(|raid| raid.arrives);
        true
    }

    // ----------------------
    // Walls:
    // ----------------------

    // Matches the walls' records to the map's wall pieces (`walls`: each cell and its tier, any
    // order): a piece kept keeps its condition (at most its tier's strength), a new piece or one of
    // another tier (drawn over in a stronger one) starts at its tier's full strength, and a piece
    // gone takes its record with it.
    pub fn sync_walls(&mut self, mut walls: Vec<(Cell, WallTier)>, rules: &DefenceRules) {
        walls.sort_by_key(|(cell, _)| row_order(*cell));
        let old = std::mem::take(&mut self.walls);
        self.walls = walls
            .into_iter()
            .map(|(cell, tier)| match old.binary_search_by_key(&row_order(cell), |wall| row_order(wall.cell)) {
                Ok(index) if old[index].tier == tier => {
                    let wall = old[index];
                    WallCondition { condition: wall.condition.min(tier.strength(rules)), ..wall }
                }
                _ => WallCondition { cell, tier, condition: tier.strength(rules), repaired: None },
            })
            .collect();
    }

    #[inline]
    fn wall_index(&self, cell: Cell) -> Result<usize, usize> {
        self.walls.binary_search_by_key(&row_order(cell), |wall| row_order(wall.cell))
    }

    #[inline]
    pub fn wall_at(&self, cell: Cell) -> Option<&WallCondition> {
        self.wall_index(cell).ok().map(|index| &self.walls[index])
    }

    // Sets a piece's condition, within 0 and its strength; false where there's no wall.
    pub fn set_wall_condition(&mut self, cell: Cell, condition: f32, rules: &DefenceRules) -> bool {
        let Ok(index) = self.wall_index(cell) else {
            return false;
        };
        let wall = &mut self.walls[index];
        wall.condition = condition.clamp(0.0, wall.strength(rules));
        true
    }

    // Lowers a piece's condition; the condition left (None where there's no wall). At 0 the caller
    // breaches it (crate::defence::damage_wall).
    pub fn damage_wall(&mut self, cell: Cell, amount: f32) -> Option<f32> {
        let index = self.wall_index(cell).ok()?;
        let wall = &mut self.walls[index];
        wall.condition = (wall.condition - amount.max(0.0)).max(0.0);
        Some(wall.condition)
    }

    // Mends a piece by the rules' share of its strength, at `now` (an ordinal month), once a month at
    // most; false where there's no wall, it's whole, or it was mended this month.
    pub fn repair_wall(&mut self, cell: Cell, now: u32, rules: &DefenceRules) -> bool {
        let Ok(index) = self.wall_index(cell) else {
            return false;
        };
        let wall = &mut self.walls[index];
        let strength = wall.strength(rules);
        if wall.condition >= strength || wall.repaired == Some(now) {
            return false;
        }
        wall.condition = (wall.condition + strength * rules.repair_share).min(strength);
        wall.repaired = Some(now);
        true
    }

    // A breached piece's record goes, and the log hears of it.
    pub fn remove_wall(&mut self, cell: Cell) -> Option<WallCondition> {
        let index = self.wall_index(cell).ok()?;
        let wall = self.walls.remove(index);
        self.notices.push(DefenceNotice::Breach { cell, tier: wall.tier });
        Some(wall)
    }

    pub fn take_notices(&mut self) -> Vec<DefenceNotice> {
        std::mem::take(&mut self.notices)
    }

    // Raids that ended without the palace lost (the `SurviveAttacks` goal) and those repelled.
    pub fn survived(&self) -> u32 {
        self.log.iter().filter(|raid| !raid.palace_lost).count() as u32
    }

    pub fn repelled(&self) -> u32 {
        self.log.iter().filter(|raid| raid.repelled()).count() as u32
    }
}

#[inline]
fn row_order(cell: Cell) -> (i32, i32) {
    (cell.y, cell.x)
}

// Months until a custom game's next random raid: between DefenceRules' least and most years.
fn random_raid_months(rules: &DefenceRules, rng: &mut impl Rng) -> u32 {
    let min = rules.random_raid_min_years.max(1) * 12;
    let max = (rules.random_raid_max_years * 12).max(min);
    rng.random_range(min..=max)
}

// A random raid's size for a city of `population` (DefenceRules: a raider for so many residents,
// within its bounds).
pub fn random_raid_size(rules: &DefenceRules, population: u32) -> u32 {
    let min = rules.random_raid_min_size.max(1);
    (population / rules.residents_per_raider.max(1)).clamp(min, rules.random_raid_max_size.max(min))
}
