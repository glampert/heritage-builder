use common::{Size, coords::Cell};
use game::{
    building::BuildingKindAndId,
    cheats,
    config::GameConfigs,
    debug::game_object_debug::GameObjectDebugVarRef,
    labour,
    sim::{
        SimCmdQueue,
        labour::{LABOUR_SECTOR_COUNT, LabourSector},
    },
    tile::sets::TileSector,
    unit::config::UnitConfigKey,
};

mod test_utils;
use test_utils::TestEnvironment;

// ----------------------------------------------
// Labour priorities (docs/plans/messages.md, M11 Phase 2): the city-wide allocation by the
// sectors' order, minimums first, within reach along the roads; Pause work.
// ----------------------------------------------

fn main() {
    test_utils::run_tests("Labour", &[
        test_utils::test_fn!(test_sectors_of_tiles),
        test_utils::test_fn!(test_priority_order),
        test_utils::test_fn!(test_higher_priority_fills_first),
        test_utils::test_fn!(test_raising_a_sector_moves_the_workers),
        test_utils::test_fn!(test_the_labour_page_queues_its_changes),
        test_utils::test_fn!(test_minimums_first),
        test_utils::test_fn!(test_building_short_of_its_minimum_gets_none),
        test_utils::test_fn!(test_houses_beyond_reach_send_no_one),
        test_utils::test_fn!(test_nearest_houses_first),
        test_utils::test_fn!(test_allocation_is_stable),
        test_utils::test_fn!(test_pause_and_resume),
        test_utils::test_fn!(test_house_gone),
        test_utils::test_fn!(test_allocation_runs_on_its_clock),
        test_utils::test_fn!(test_frozen_labour_stays_put),
        test_utils::test_fn!(test_report),
    ]);
}

// ----------------------------------------------
// Helpers
// ----------------------------------------------

const ROAD_Y: i32 = 10;

// A map with a road along y = 10 from x = 1 to `road_end`; the allocation running.
fn city(road_end: i32) -> TestEnvironment {
    cheats::get_mut().freeze_labour = false;
    let mut env = TestEnvironment::with_map_size(Size::new(64, 24));
    let road: Vec<Cell> = (1..=road_end).map(|x| Cell::new(x, ROAD_Y)).collect();
    test_utils::place_road(&mut env, &road);
    env
}

fn done(_env: TestEnvironment) {
    cheats::get_mut().freeze_labour = true;
}

// A level 0 house north of the road at `x`, with 2 residents (2 workers) that hold.
fn house(env: &mut TestEnvironment, x: i32) -> BuildingKindAndId {
    let house = test_utils::spawn_building(env, Cell::new(x, ROAD_Y - 1), "house0");
    for name in ["freeze_upgrade_update", "freeze_population_update", "freeze_stock_update", "freeze_deprivation"] {
        let mut value = true;
        let ok = test_utils::find_building_mut(env, house)
            .debug_options()
            .set_debug_option_by_name(name, GameObjectDebugVarRef::Bool(&mut value));
        assert!(ok, "houses have a '{name}' debug option");
    }
    let context = env.new_sim_context(0.0);
    let cmds = context.cmds_mut();
    let added = context.find_building_mut(house.kind, house.id).unwrap().add_population(cmds, &context, 2);
    cmds.execute(&context);
    assert_eq!(added, 2, "a level 0 house takes 2");
    house
}

// A 2 × 2 workplace south of the road at `x`.
fn workplace(env: &mut TestEnvironment, x: i32, tile: &str) -> BuildingKindAndId {
    let building = test_utils::spawn_building(env, Cell::new(x, ROAD_Y + 1), tile);
    assert!(test_utils::find_building(env, building).is_linked_to_road(), "the {tile} is by the road");
    quiet(env, building);
    building
}

// Keeps a workplace's walkers in (a test that steps the world must leave no task behind).
fn quiet(env: &mut TestEnvironment, handle: BuildingKindAndId) {
    for name in
        ["freeze_patrol", "freeze_stock_update", "freeze_production", "freeze_storage_delivery", "freeze_storage_fetching"]
    {
        let mut value = true;
        let _ = test_utils::find_building_mut(env, handle)
            .debug_options()
            .set_debug_option_by_name(name, GameObjectDebugVarRef::Bool(&mut value));
    }
}

fn employees(env: &TestEnvironment, handle: BuildingKindAndId) -> u32 {
    test_utils::find_building(env, handle).workers().unwrap().as_employer().unwrap().employee_count()
}

fn employed(env: &TestEnvironment, house: BuildingKindAndId) -> u32 {
    test_utils::find_building(env, house).workers().unwrap().as_household_worker_pool().unwrap().employed_count()
}

fn allocate(env: &mut TestEnvironment) {
    let context = env.new_sim_context(0.0);
    labour::allocate(&context);
}

fn set_priority(env: &mut TestEnvironment, sector: LabourSector, priority: usize) -> bool {
    let context = env.new_sim_context(0.0);
    labour::set_priority(&context, sector, priority)
}

fn set_paused(env: &mut TestEnvironment, handle: BuildingKindAndId, paused: bool) -> Option<bool> {
    let context = env.new_sim_context(0.0);
    labour::set_paused(&context, handle, paused)
}

// Every employer's households and house's employers, to compare passes.
fn snapshot(env: &TestEnvironment, handles: &[BuildingKindAndId]) -> Vec<String> {
    handles
        .iter()
        .map(|handle| {
            let workers = test_utils::find_building(env, *handle).workers().unwrap();
            if let Some(employer) = workers.as_employer() {
                format!("{}: {}", handle.id, employer.employee_count())
            } else {
                let pool = workers.as_household_worker_pool().unwrap();
                format!("{}: {} + {}", handle.id, pool.employed_count(), pool.unemployed_count())
            }
        })
        .collect()
}

// ----------------------------------------------
// Tests
// ----------------------------------------------

fn test_sectors_of_tiles() {
    assert_eq!(LabourSector::of_tile_sector(TileSector::FoodAndFarming), LabourSector::FoodAndFarming);
    assert_eq!(LabourSector::of_tile_sector(TileSector::IndustryAndResources), LabourSector::Industry);
    assert_eq!(LabourSector::of_tile_sector(TileSector::Services), LabourSector::Services);
    assert_eq!(LabourSector::of_tile_sector(TileSector::CultureAndReligion), LabourSector::CultureAndReligion);
    assert_eq!(LabourSector::of_tile_sector(TileSector::Infrastructure), LabourSector::Infrastructure);
    assert_eq!(LabourSector::of_tile_sector(TileSector::TradeAndEconomy), LabourSector::Trade);
    // Q6: anything else counts under Infrastructure.
    assert_eq!(LabourSector::of_tile_sector(TileSector::Beautification), LabourSector::Infrastructure);
    assert_eq!(LabourSector::of_tile_sector(TileSector::Housing), LabourSector::Infrastructure);

    // A granary is food's, a storage yard industry's, by their tiles.
    let mut env = city(20);
    let granary = test_utils::spawn_building(&mut env, Cell::new(2, ROAD_Y + 1), "granary");
    let yard = test_utils::spawn_building(&mut env, Cell::new(6, ROAD_Y + 1), "storage_yard");
    {
        let context = env.new_sim_context(0.0);
        let granary = context.find_building(granary.kind, granary.id).unwrap();
        let yard = context.find_building(yard.kind, yard.id).unwrap();
        assert_eq!(labour::sector_of(&context, granary), LabourSector::FoodAndFarming);
        assert_eq!(labour::sector_of(&context, yard), LabourSector::Industry);
    }
    done(env);
}

fn test_priority_order() {
    let env = city(4);
    let order = env.sim.labour().order().to_vec();
    assert_eq!(order, vec![
        LabourSector::FoodAndFarming,
        LabourSector::Industry,
        LabourSector::Services,
        LabourSector::CultureAndReligion,
        LabourSector::Infrastructure,
        LabourSector::Trade,
    ]);
    assert_eq!(order.len(), LABOUR_SECTOR_COUNT);

    let mut labour = env.sim.labour().clone();
    // Trade to 1: the others move down one.
    assert!(labour.set_priority(LabourSector::Trade, 1));
    assert_eq!(labour.order()[0], LabourSector::Trade);
    assert_eq!(labour.priority(LabourSector::FoodAndFarming), 2);
    assert_eq!(labour.priority(LabourSector::Infrastructure), 6);
    // Where it is already: nothing changes.
    assert!(!labour.set_priority(LabourSector::Trade, 1));
    // + and − swap with a neighbour.
    assert!(labour.set_priority(LabourSector::FoodAndFarming, 3));
    assert_eq!(labour.priority(LabourSector::Industry), 2);
    assert_eq!(labour.priority(LabourSector::FoodAndFarming), 3);
    // A priority past 6 is the last.
    assert!(labour.set_priority(LabourSector::Trade, 9));
    assert_eq!(labour.priority(LabourSector::Trade), 6);
    // An order that misses a sector is refused.
    assert!(!labour.set_order(&[LabourSector::Trade; LABOUR_SECTOR_COUNT]));
    done(env);
}

fn test_higher_priority_fills_first() {
    let mut env = city(30);
    // 4 workers; a farmstead (food, 2 to 6) and a pottery kiln (industry, 2 to 4).
    let houses = [house(&mut env, 8), house(&mut env, 9)];
    let farmstead = workplace(&mut env, 4, "farmstead");
    let kiln = workplace(&mut env, 12, "pottery_kiln");
    allocate(&mut env);

    // Food is first: it takes all 4.
    assert_eq!(employees(&env, farmstead), 4);
    assert_eq!(employees(&env, kiln), 0);
    assert_eq!(employed(&env, houses[0]) + employed(&env, houses[1]), 4);
    done(env);
}

fn test_raising_a_sector_moves_the_workers() {
    let mut env = city(30);
    house(&mut env, 8);
    house(&mut env, 9);
    let farmstead = workplace(&mut env, 4, "farmstead");
    let kiln = workplace(&mut env, 12, "pottery_kiln");
    allocate(&mut env);
    assert_eq!(employees(&env, farmstead), 4);

    // Industry over food: the kiln is filled first, up to its 4.
    assert!(set_priority(&mut env, LabourSector::Industry, 1));
    assert_eq!(env.sim.labour().priority(LabourSector::FoodAndFarming), 2);
    assert_eq!(employees(&env, kiln), 4);
    assert_eq!(employees(&env, farmstead), 0);
    done(env);
}

fn test_the_labour_page_queues_its_changes() {
    let mut env = city(30);
    house(&mut env, 8);
    let farmstead = workplace(&mut env, 4, "farmstead");
    let kiln = workplace(&mut env, 12, "pottery_kiln");
    allocate(&mut env);
    assert_eq!(employees(&env, farmstead), 2);

    // The page's + and − queue a move, run at the start of the next frame (Simulation::update).
    labour::request_priority(LabourSector::Industry, 1);
    assert!(labour::has_requests());
    assert_eq!(env.sim.labour().priority(LabourSector::Industry), 2, "nothing moves until it runs");
    {
        let context = env.new_sim_context(0.0);
        labour::run_requests(&context);
    }
    assert!(!labour::has_requests());
    assert_eq!(env.sim.labour().priority(LabourSector::Industry), 1);
    assert_eq!((employees(&env, kiln), employees(&env, farmstead)), (2, 0), "handed out again at once");
    done(env);
}

fn test_minimums_first() {
    let mut env = city(30);
    // 4 workers, two kilns of 2 to 4: each runs with 2, rather than one with 4.
    house(&mut env, 8);
    house(&mut env, 9);
    let kilns = [workplace(&mut env, 2, "pottery_kiln"), workplace(&mut env, 12, "pottery_kiln")];
    allocate(&mut env);
    assert_eq!([employees(&env, kilns[0]), employees(&env, kilns[1])], [2, 2]);

    // A fifth and sixth: the rest go to the first, in pool order.
    house(&mut env, 10);
    allocate(&mut env);
    assert_eq!([employees(&env, kilns[0]), employees(&env, kilns[1])], [4, 2]);
    done(env);
}

fn test_building_short_of_its_minimum_gets_none() {
    let mut env = city(30);
    // 3 workers, two kilns of 2 to 4: the first gets its 2, the second can't reach its 2 so it gets
    // none (one worker wouldn't run it), and the first takes the one left.
    house(&mut env, 8);
    let half = test_utils::spawn_building(&mut env, Cell::new(9, ROAD_Y - 1), "house0");
    {
        let context = env.new_sim_context(0.0);
        let cmds = context.cmds_mut();
        context.find_building_mut(half.kind, half.id).unwrap().add_population(cmds, &context, 1);
        cmds.execute(&context);
    }
    let kilns = [workplace(&mut env, 2, "pottery_kiln"), workplace(&mut env, 12, "pottery_kiln")];
    allocate(&mut env);
    assert_eq!([employees(&env, kilns[0]), employees(&env, kilns[1])], [3, 0]);

    // A farmstead first (food): it takes the 3; the kilns get none.
    let farmstead = workplace(&mut env, 16, "farmstead");
    allocate(&mut env);
    assert_eq!(employees(&env, farmstead), 3);
    assert_eq!([employees(&env, kilns[0]), employees(&env, kilns[1])], [0, 0]);
    done(env);
}

fn test_houses_beyond_reach_send_no_one() {
    let radius = GameConfigs::get().sim.workers_search_radius;
    let mut env = city(50);
    let kiln = workplace(&mut env, 2, "pottery_kiln");
    // Its road link is at x 2 or 3; a house more than the radius along the road from it.
    let far = house(&mut env, 4 + radius + 2);
    allocate(&mut env);
    assert_eq!(employees(&env, kiln), 0, "a house {} cells away is out of reach", radius + 2);
    assert_eq!(employed(&env, far), 0);

    let near = house(&mut env, 6);
    allocate(&mut env);
    assert_eq!(employees(&env, kiln), 2);
    assert_eq!(employed(&env, near), 2);
    assert_eq!(employed(&env, far), 0);
    done(env);
}

fn test_nearest_houses_first() {
    let mut env = city(40);
    let kiln = workplace(&mut env, 2, "pottery_kiln");
    let houses = [house(&mut env, 20), house(&mut env, 6), house(&mut env, 12)];
    // 6 workers for 4 jobs: the two nearest houses send them.
    allocate(&mut env);
    assert_eq!(employees(&env, kiln), 4);
    assert_eq!(employed(&env, houses[1]), 2, "the nearest");
    assert_eq!(employed(&env, houses[2]), 2, "the next");
    assert_eq!(employed(&env, houses[0]), 0, "the farthest");
    done(env);
}

fn test_allocation_is_stable() {
    let mut env = city(40);
    let mut handles = Vec::new();
    for x in [5, 6, 7, 8, 20] {
        handles.push(house(&mut env, x));
    }
    handles.push(workplace(&mut env, 2, "farmstead"));
    handles.push(workplace(&mut env, 10, "pottery_kiln"));
    handles.push(workplace(&mut env, 14, "theater"));
    handles.push(workplace(&mut env, 22, "tax_office"));
    allocate(&mut env);
    let first = snapshot(&env, &handles);
    for _ in 0..3 {
        allocate(&mut env);
        assert_eq!(snapshot(&env, &handles), first, "nothing changed, so no one moves");
    }
    // And through the world steps' passes.
    test_utils::tick_n(&mut env, 30, 1.0);
    assert_eq!(snapshot(&env, &handles), first);
    done(env);
}

fn test_pause_and_resume() {
    let mut env = city(30);
    house(&mut env, 8);
    house(&mut env, 9);
    let kiln = workplace(&mut env, 2, "pottery_kiln");
    let theater = workplace(&mut env, 12, "theater");
    allocate(&mut env);
    assert_eq!(employees(&env, kiln), 4, "industry before culture");
    assert_eq!(employees(&env, theater), 0);

    // Paused, its workers go to the theatre.
    assert_eq!(set_paused(&mut env, kiln, true), Some(true));
    assert!(test_utils::find_building(&env, kiln).is_paused());
    assert_eq!(employees(&env, kiln), 0);
    assert_eq!(employees(&env, theater), 4);
    // A pass keeps it without workers.
    allocate(&mut env);
    assert_eq!(employees(&env, kiln), 0);
    // Its jobs aren't open: the stats leave them out.
    env.world.refresh_stats(env.sim.stats_sources());
    assert_eq!(env.world.stats().workers.max_employed, 5, "the theatre's 5 jobs only");

    // Resumed, it's filled first again.
    assert_eq!(set_paused(&mut env, kiln, false), Some(false));
    assert_eq!(employees(&env, kiln), 4);
    assert_eq!(employees(&env, theater), 0);

    // A house can't be paused.
    let house = house(&mut env, 20);
    assert_eq!(set_paused(&mut env, house, true), None);
    done(env);
}

fn test_house_gone() {
    let mut env = city(30);
    let first = house(&mut env, 6);
    let second = house(&mut env, 20);
    let kiln = workplace(&mut env, 2, "pottery_kiln");
    allocate(&mut env);
    assert_eq!(employees(&env, kiln), 4);

    // The nearest house goes: its 2 leave at once, and no one is left to fill them.
    test_utils::despawn_building(&mut env, first);
    assert_eq!(employees(&env, kiln), 2);
    // Its residents left as settlers: gone, so no unit task is left behind.
    while let Some(settler) = test_utils::find_unit_by_config(&env, UnitConfigKey::Settler) {
        let context = env.new_sim_context(0.0);
        let cmds = context.cmds_mut();
        cmds.despawn_unit_with_id(settler);
        cmds.execute(&context);
    }
    allocate(&mut env);
    assert_eq!(employees(&env, kiln), 2);
    assert_eq!(employed(&env, second), 2);
    done(env);
}

fn test_allocation_runs_on_its_clock() {
    let mut env = city(30);
    house(&mut env, 8);
    let kiln = workplace(&mut env, 2, "pottery_kiln");
    assert_eq!(employees(&env, kiln), 0);
    let secs = GameConfigs::get().sim.labour.allocate_secs;
    test_utils::tick_n(&mut env, (secs as usize) + 2, 1.0);
    assert_eq!(employees(&env, kiln), 2, "staffed by the world step's pass");
    done(env);
}

fn test_frozen_labour_stays_put() {
    let mut env = city(30);
    house(&mut env, 8);
    let kiln = workplace(&mut env, 2, "pottery_kiln");
    cheats::get_mut().freeze_labour = true;
    allocate(&mut env);
    assert_eq!(employees(&env, kiln), 0);
    cheats::get_mut().freeze_labour = false;
    allocate(&mut env);
    assert_eq!(employees(&env, kiln), 2);
    done(env);
}

fn test_report() {
    let mut env = city(30);
    house(&mut env, 8);
    house(&mut env, 9);
    house(&mut env, 10);
    let farmstead = workplace(&mut env, 2, "farmstead");
    let kiln = workplace(&mut env, 12, "pottery_kiln");
    let theater = workplace(&mut env, 16, "theater");
    allocate(&mut env);
    assert_eq!(set_paused(&mut env, theater, true), Some(true));

    let context = env.new_sim_context(0.0);
    let report = labour::report(&context);
    assert_eq!(report.workforce, 6);
    assert_eq!(report.employed, 6);
    assert_eq!(report.looking_for_work, 0);
    let food = report.sector(LabourSector::FoodAndFarming).unwrap();
    assert_eq!((food.priority, food.needed, food.employed, food.short), (1, 6, 6, 0));
    let industry = report.sector(LabourSector::Industry).unwrap();
    assert_eq!((industry.priority, industry.needed, industry.employed, industry.short), (2, 4, 0, 4));
    // The paused theatre's jobs aren't open.
    let culture = report.sector(LabourSector::CultureAndReligion).unwrap();
    assert_eq!((culture.needed, culture.employed, culture.paused), (0, 0, 1));
    assert_eq!(culture.buildings.len(), 1);
    assert!(culture.buildings[0].paused && !culture.buildings[0].working);
    assert_eq!(report.jobs_not_filled, 4);
    assert_eq!(report.sectors.iter().map(|sector| sector.priority).collect::<Vec<_>>(), vec![1, 2, 3, 4, 5, 6]);
    assert!(report.sector(LabourSector::FoodAndFarming).unwrap().buildings[0].building == farmstead.kind);
    assert!(report.sector(LabourSector::Industry).unwrap().buildings[0].building == kiln.kind);
    done(env);
}
