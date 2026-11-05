use common::coords::Cell;
use common::time::Seconds;
use game::{
    building::{BuildingKind, BuildingKindAndId, config::BuildingConfigs, construction::{ConstructionMaterials, ConstructionMode}},
    config::GameConfigs,
    cheats,
    menu::{PlaceOrClearResult, TilePlacement},
    sim::{SimCmdQueue, resources::{ResourceKind, StockItem}},
    tile::placement::TilePlacementErrReason,
    undo_redo,
    world::city_stock::{CityStock, Withdrawal, WithdrawnItem},
};

mod test_utils;
use test_utils::TestEnvironment;

// ----------------------------------------------
// Integration tests for building costs and
// construction (docs/plans/construction.md).
// ----------------------------------------------

fn main() {
    test_utils::run_tests("Construction", &[
        test_utils::test_fn!(test_deferred_spawns_are_free),
        test_utils::test_fn!(test_player_placement_charges_gold),
        // City stock:
        test_utils::test_fn!(test_yard_accessors_see_every_slot),
        test_utils::test_fn!(test_city_stock_counts_every_yard),
        test_utils::test_fn!(test_withdrawal_takes_yards_in_pool_order),
        test_utils::test_fn!(test_refund_returns_materials_to_their_yards),
        test_utils::test_fn!(test_refund_goes_elsewhere_when_a_yard_is_gone),
        // Material costs:
        test_utils::test_fn!(test_temple_needs_bricks),
        // Construction sites:
        test_utils::test_fn!(test_site_is_inert_until_built),
        test_utils::test_fn!(test_clearing_a_site_refunds_it),
        test_utils::test_fn!(test_undo_and_redo_of_a_site),
        test_utils::test_fn!(test_finished_building_refunds_nothing_when_cleared),
        test_utils::test_fn!(test_settlers_move_in_when_their_house_is_built),
        test_utils::test_fn!(test_editor_mode_builds_finished),
    ]);
}

// ----------------------------------------------
// Helpers
// ----------------------------------------------

// Runs `test` with the tile cost cheat off (the harness turns it on), then turns it back on.
fn with_tile_costs<F: FnOnce()>(test: F) {
    cheats::get_mut().ignore_tile_cost = false;
    test();
    cheats::get_mut().ignore_tile_cost = true;
}

// Runs `test` with tile costs and construction sites on, as the player plays.
fn as_player<F: FnOnce()>(test: F) {
    cheats::get_mut().instant_construction = false;
    with_tile_costs(test);
    cheats::get_mut().instant_construction = true;
}

fn place(env: &mut TestEnvironment, cell: Cell, name: &str, mode: ConstructionMode) -> bool {
    let tile_def = test_utils::find_building_def(name);
    env.world.refresh_stats(env.sim.treasury()); // Placement checks the treasury through the stats.
    let context = env.new_sim_context(0.0);
    match TilePlacement::place(&context, cell, tile_def, true, true, mode) {
        PlaceOrClearResult::Failed { message, .. } => panic!("placing {name} at {cell} failed: {message}"),
        _ => true,
    }
}

fn clear(env: &mut TestEnvironment, cell: Cell) -> bool {
    let context = env.new_sim_context(0.0);
    let tile = context.find_tile(cell, game::tile::TileKind::Building).expect("a building to clear");
    TilePlacement::clear(&context, tile, false, true).is_ok()
}

fn building_at(env: &TestEnvironment, cell: Cell) -> Option<&game::building::Building> {
    env.world.find_building_for_cell(cell, &env.tile_map)
}

// Ticks the world (as world steps do) for `secs` of simulated time.
fn run_for(env: &mut TestEnvironment, secs: Seconds) {
    const STEP: Seconds = 0.5;
    let steps = (secs / STEP).ceil() as usize;
    test_utils::tick_n(env, steps, STEP);
}

// Gold the HUD and the affordability checks see: the world stats, recounted now.
fn gold(env: &mut TestEnvironment) -> u32 {
    env.world.refresh_stats(env.sim.treasury());
    env.world.stats().treasury.gold_units_total
}

fn materials(items: &[(ResourceKind, u32)]) -> ConstructionMaterials {
    let items: Vec<StockItem> = items.iter().map(|&(kind, count)| StockItem { kind, count }).collect();
    ConstructionMaterials::new(&items).unwrap()
}

fn spawn_yard(env: &mut TestEnvironment, cell: Cell, bricks: u32) -> BuildingKindAndId {
    let yard = test_utils::spawn_building(env, cell, "storage_yard");
    if bricks != 0 {
        let received = test_utils::find_building_mut(env, yard).receive_resources(ResourceKind::Bricks, bricks);
        assert_eq!(received, bricks);
    }
    yard
}

fn bricks_in(env: &TestEnvironment, yard: BuildingKindAndId) -> u32 {
    test_utils::find_building(env, yard).available_resources(ResourceKind::Bricks)
}

// ----------------------------------------------
// Charging
// ----------------------------------------------

// A building spawned through SimCmds, as a settler's house is, costs nothing: only the
// player's placement charges.
fn test_deferred_spawns_are_free() {
    with_tile_costs(|| {
        let mut env = TestEnvironment::new();
        let gold_before = gold(&mut env);

        let well_def = test_utils::find_building_def("small_well");
        assert!(well_def.cost > 0, "the test needs a building that costs gold");

        {
            let context = env.new_sim_context(0.0);
            let cmds = context.cmds_mut();
            cmds.spawn_building_with_tile_def_cb(Cell::new(4, 4), well_def, |_, result| {
                assert!(result.is_ok(), "the deferred spawn should succeed");
            });
            cmds.execute(&context);
        }

        assert!(env.world.find_building_for_cell(Cell::new(4, 4), &env.tile_map).is_some());
        assert_eq!(gold(&mut env), gold_before, "a deferred spawn shouldn't charge");
    });
}

// The player's placement path charges the tile's gold cost.
fn test_player_placement_charges_gold() {
    with_tile_costs(|| {
        let mut env = TestEnvironment::new();
        let gold_before = gold(&mut env);

        let well_def = test_utils::find_building_def("small_well");
        {
            let context = env.new_sim_context(0.0);
            assert!(TilePlacement::place(&context, Cell::new(4, 4), well_def, true, false, ConstructionMode::Finished).is_ok());
        }

        assert_eq!(gold(&mut env), gold_before - well_def.cost);
    });
}

// ----------------------------------------------
// City stock
// ----------------------------------------------

// A storage yard's own accessors, which walkers use, see all of its slots, as the city stock
// does. Taking goods empties the emptiest slots first, so a good keeps to as few as it can.
fn test_yard_accessors_see_every_slot() {
    let mut env = TestEnvironment::new();
    let yard = test_utils::spawn_building(&mut env, Cell::new(2, 2), "storage_yard");
    let config = BuildingConfigs::get().find_storage_config(BuildingKind::StorageYard);
    let (slot_capacity, num_slots) = (config.slot_capacity, config.num_slots);
    assert_eq!(slot_capacity, 4, "the counts below assume slots of 4 units");

    let building = test_utils::find_building_mut(&mut env, yard);

    // Wood fills a slot and starts a second, bricks take a third, then more wood tops up the
    // second and starts a fourth: wood 4 + 4 + 1.
    assert_eq!(building.receive_resources(ResourceKind::Wood, 6), 6);
    assert_eq!(building.receive_resources(ResourceKind::Bricks, 3), 3);
    assert_eq!(building.receive_resources(ResourceKind::Wood, 3), 3);
    assert_eq!(building.available_resources(ResourceKind::Wood), 9);
    assert_eq!(building.stock().as_slice(), &[
        StockItem { kind: ResourceKind::Wood, count: 9 },
        StockItem { kind: ResourceKind::Bricks, count: 3 },
    ]);

    // Taking 7 wood empties the 1-unit slot and a full one and leaves 2 in the other, so only
    // that slot and the bricks' are in use.
    assert_eq!(building.remove_resources(ResourceKind::Wood, 7), 7);
    assert_eq!(building.available_resources(ResourceKind::Wood), 2);
    assert_eq!(building.receivable_resources(ResourceKind::Bricks), (slot_capacity - 3) + (num_slots - 2) * slot_capacity);

    // A delivery takes as much as fits; taking more than there is takes all there is.
    let room = building.receivable_resources(ResourceKind::Wood);
    assert_eq!(building.receive_resources(ResourceKind::Wood, room + 5), room);
    assert_eq!(building.receivable_resources(ResourceKind::Wood), 0);
    assert_eq!(building.remove_resources(ResourceKind::Wood, 100), 2 + room);
    assert_eq!(building.receivable_resources(ResourceKind::Wood), (num_slots - 1) * slot_capacity);
}

// The city stock is every storage yard's, and affordability reports the shortfall.
fn test_city_stock_counts_every_yard() {
    with_tile_costs(|| {
        let mut env = TestEnvironment::new();
        spawn_yard(&mut env, Cell::new(2, 2), 3);
        spawn_yard(&mut env, Cell::new(8, 2), 4);
        env.world.refresh_stats(env.sim.treasury());

        assert_eq!(CityStock::available(&env.world, ResourceKind::Bricks), 7);
        assert_eq!(CityStock::available(&env.world, ResourceKind::Wood), 0);

        assert!(CityStock::can_afford(&env.world, 10, &materials(&[(ResourceKind::Bricks, 7)])).is_ok());

        let missing = CityStock::can_afford(&env.world, 1000, &materials(&[(ResourceKind::Bricks, 8), (ResourceKind::Wood, 2)]))
            .unwrap_err();
        assert_eq!(missing.gold, 1000 - gold(&mut env));
        assert_eq!(missing.materials.as_slice(), &[
            StockItem { kind: ResourceKind::Bricks, count: 1 },
            StockItem { kind: ResourceKind::Wood, count: 2 },
        ]);
    });
}

// A withdrawal empties the first yard before taking from the next, and records where it took from.
fn test_withdrawal_takes_yards_in_pool_order() {
    let mut env = TestEnvironment::new();
    let first = spawn_yard(&mut env, Cell::new(2, 2), 3);
    let second = spawn_yard(&mut env, Cell::new(8, 2), 4);

    let withdrawal = CityStock::withdraw(&mut env.world, &materials(&[(ResourceKind::Bricks, 5)]));

    assert_eq!(withdrawal.taken.as_slice(), &[
        WithdrawnItem { storage_id: first.id, kind: ResourceKind::Bricks, count: 3 },
        WithdrawnItem { storage_id: second.id, kind: ResourceKind::Bricks, count: 2 },
    ]);
    assert_eq!(withdrawal.count(ResourceKind::Bricks), 5);
    assert_eq!((bricks_in(&env, first), bricks_in(&env, second)), (0, 2));
}

fn test_refund_returns_materials_to_their_yards() {
    let mut env = TestEnvironment::new();
    let first = spawn_yard(&mut env, Cell::new(2, 2), 3);
    let second = spawn_yard(&mut env, Cell::new(8, 2), 4);

    let withdrawal = CityStock::withdraw(&mut env.world, &materials(&[(ResourceKind::Bricks, 5)]));
    let lost = CityStock::refund(&mut env.world, &withdrawal);

    assert!(lost.is_empty());
    assert_eq!((bricks_in(&env, first), bricks_in(&env, second)), (3, 4));
}

// When the yard a withdrawal came from is gone, its share goes to another yard; with no room
// anywhere, it's reported lost.
fn test_refund_goes_elsewhere_when_a_yard_is_gone() {
    let mut env = TestEnvironment::new();
    let first = spawn_yard(&mut env, Cell::new(2, 2), 3);
    let second = spawn_yard(&mut env, Cell::new(8, 2), 0);

    let withdrawal = CityStock::withdraw(&mut env.world, &materials(&[(ResourceKind::Bricks, 3)]));
    test_utils::despawn_building(&mut env, first);

    let lost = CityStock::refund(&mut env.world, &withdrawal);
    assert!(lost.is_empty());
    assert_eq!(CityStock::available(&env.world, ResourceKind::Bricks), 3);
    assert_eq!(bricks_in(&env, second), 3);

    // No yards at all: everything is lost.
    test_utils::despawn_building(&mut env, second);
    let lost = CityStock::refund(&mut env.world, &withdrawal);
    assert_eq!(lost.as_slice(), &[StockItem { kind: ResourceKind::Bricks, count: 3 }]);

    // A refund of nothing does nothing.
    assert!(CityStock::refund(&mut env.world, &Withdrawal::default()).is_empty());
}

// ----------------------------------------------
// Material costs
// ----------------------------------------------

// A temple costs gold, wood and bricks: it can't be placed until the storage yards hold the
// materials, placing it takes them, and undoing the placement gives them and the gold back.
fn test_temple_needs_bricks() {
    with_tile_costs(|| {
        let mut env = TestEnvironment::new();
        let temple_def = test_utils::find_building_def("temple");
        let bricks_needed = temple_def.materials.iter().find(|item| item.kind == ResourceKind::Bricks).unwrap().count;
        let wood_needed = temple_def.materials.iter().find(|item| item.kind == ResourceKind::Wood).unwrap().count;

        let yard = spawn_yard(&mut env, Cell::new(2, 2), 0);
        let gold_before = gold(&mut env);
        let temple_cell = Cell::new(14, 14);

        // No materials: refused, with the reason.
        {
            let context = env.new_sim_context(0.0);
            match TilePlacement::place(&context, temple_cell, temple_def, true, true, ConstructionMode::Finished) {
                PlaceOrClearResult::Failed { message, .. } => assert!(message.contains("Bricks"), "{message}"),
                _ => panic!("the temple shouldn't be placed without bricks"),
            }
        }
        {
            let context = env.new_sim_context(0.0);
            let mut spawner = game::world::object::Spawner::new(&context);
            spawner.set_subtract_tile_cost(true);
            let err = spawner.try_spawn_building_with_tile_def(temple_cell, temple_def).err().unwrap();
            assert!(matches!(err.reason, TilePlacementErrReason::CannotAffordMaterials));
        }

        // Deliver the materials, plus a spare brick.
        let lost = CityStock::deposit(&mut env.world, &materials(&[(ResourceKind::Bricks, bricks_needed + 1), (ResourceKind::Wood, wood_needed)]));
        assert!(lost.is_empty());

        {
            let context = env.new_sim_context(0.0);
            assert!(TilePlacement::place(&context, temple_cell, temple_def, true, true, ConstructionMode::Finished).is_ok());
        }
        assert_eq!(CityStock::available(&env.world, ResourceKind::Bricks), 1);
        assert_eq!(CityStock::available(&env.world, ResourceKind::Wood), 0);
        assert_eq!(gold(&mut env), gold_before - temple_def.cost);

        // Undo: the materials and gold come back.
        {
            let context = env.new_sim_context(0.0);
            undo_redo::undo(&context);
        }
        assert!(env.world.find_building_for_cell(temple_cell, &env.tile_map).is_none());
        assert_eq!(CityStock::available(&env.world, ResourceKind::Bricks), bricks_needed + 1);
        assert_eq!(CityStock::available(&env.world, ResourceKind::Wood), wood_needed);
        assert_eq!(gold(&mut env), gold_before);
        assert!(test_utils::find_building(&env, yard).is(game::building::BuildingKind::StorageYard));

        undo_redo::clear();
    });
}

// ----------------------------------------------
// Construction sites
// ----------------------------------------------

// A placed storage yard is a site for its tier's duration: it holds nothing and counts for
// nothing in the city stock until it completes, then works.
fn test_site_is_inert_until_built() {
    as_player(|| {
        let mut env = TestEnvironment::new();
        let cell = Cell::new(4, 4);
        let build_secs = GameConfigs::get().sim.construction.basic_secs;

        assert!(place(&mut env, cell, "storage_yard", ConstructionMode::Site));
        let yard = building_at(&env, cell).unwrap();
        assert!(yard.is_under_construction());
        assert!(!yard.is_operational());
        let handle = yard.kind_and_id();

        // Deliveries and deposits skip it.
        assert_eq!(test_utils::find_building_mut(&mut env, handle).receive_resources(ResourceKind::Wood, 2), 0);
        assert_eq!(CityStock::deposit_resource(&mut env.world, ResourceKind::Wood, 2), 2);

        run_for(&mut env, build_secs - 1.0);
        let site = building_at(&env, cell).unwrap().construction_site().unwrap();
        assert!(site.progress() > 0.9 && site.progress() < 1.0, "progress {}", site.progress());

        run_for(&mut env, 1.5);
        assert!(!building_at(&env, cell).unwrap().is_under_construction(), "the yard should be built");
        assert_eq!(CityStock::deposit_resource(&mut env.world, ResourceKind::Wood, 2), 0);
        assert_eq!(CityStock::available(&env.world, ResourceKind::Wood), 2);
    });
}

// Clearing a site refunds its gold and materials, into the yards they came from.
fn test_clearing_a_site_refunds_it() {
    as_player(|| {
        let mut env = TestEnvironment::new();
        let yard = spawn_yard(&mut env, Cell::new(2, 2), 0);
        assert!(CityStock::deposit(&mut env.world, &materials(&[(ResourceKind::Bricks, 8), (ResourceKind::Wood, 6)])).is_empty());
        let gold_before = gold(&mut env);

        let temple_cell = Cell::new(14, 14);
        assert!(place(&mut env, temple_cell, "temple", ConstructionMode::Site));
        assert!(building_at(&env, temple_cell).unwrap().is_under_construction());
        assert_eq!(CityStock::available(&env.world, ResourceKind::Bricks), 0);

        run_for(&mut env, 5.0);
        assert!(clear(&mut env, temple_cell));

        assert!(building_at(&env, temple_cell).is_none());
        assert_eq!(gold(&mut env), gold_before);
        assert_eq!(CityStock::available(&env.world, ResourceKind::Bricks), 8);
        assert_eq!(CityStock::available(&env.world, ResourceKind::Wood), 6);
        // The yard's own count, which walkers see, is the same: 6 wood in two slots.
        assert_eq!(test_utils::find_building(&env, yard).available_resources(ResourceKind::Wood), 6);
        undo_redo::clear();
    });
}

// Undoing a site's placement refunds it; redo places a new site and charges again.
fn test_undo_and_redo_of_a_site() {
    as_player(|| {
        let mut env = TestEnvironment::new();
        let gold_before = gold(&mut env);
        let cell = Cell::new(6, 6);
        let well_cost = test_utils::find_building_def("small_well").cost;

        assert!(place(&mut env, cell, "small_well", ConstructionMode::Site));
        run_for(&mut env, 3.0);
        assert_eq!(gold(&mut env), gold_before - well_cost);

        {
            let context = env.new_sim_context(0.0);
            undo_redo::undo(&context);
        }
        assert!(building_at(&env, cell).is_none());
        assert_eq!(gold(&mut env), gold_before);

        {
            let context = env.new_sim_context(0.0);
            undo_redo::redo(&context);
        }
        let well = building_at(&env, cell).expect("redo should place the well again");
        assert!(well.is_under_construction(), "redo places a new site");
        assert_eq!(well.construction_site().unwrap().progress(), 0.0);
        assert_eq!(gold(&mut env), gold_before - well_cost);
        undo_redo::clear();
    });
}

// Demolishing a finished building refunds nothing.
fn test_finished_building_refunds_nothing_when_cleared() {
    as_player(|| {
        let mut env = TestEnvironment::new();
        let gold_before = gold(&mut env);
        let cell = Cell::new(6, 6);
        let well_cost = test_utils::find_building_def("small_well").cost;

        assert!(place(&mut env, cell, "small_well", ConstructionMode::Site));
        run_for(&mut env, GameConfigs::get().sim.construction.basic_secs + 1.0);
        assert!(!building_at(&env, cell).unwrap().is_under_construction());

        assert!(clear(&mut env, cell));
        assert_eq!(gold(&mut env), gold_before - well_cost);
        undo_redo::clear();
    });
}

// A settler's house is a short site; the settlers move in when it's built.
fn test_settlers_move_in_when_their_house_is_built() {
    as_player(|| {
        let mut env = TestEnvironment::new();
        let cell = Cell::new(8, 8);
        let house_def = test_utils::find_building_def("house0");
        let build_secs = GameConfigs::get().sim.construction.house_build_secs;

        // What the settler task does on arrival (system/settlers.rs), as a deferred spawn.
        {
            let context = env.new_sim_context(0.0);
            let cmds = context.cmds_mut();
            cmds.spawn_building_with_tile_def_cb(cell, house_def, |context, result| {
                let Ok(building) = result else { panic!("the house should spawn") };
                let site = game::building::construction::ConstructionSite::new(4.0, 0, Withdrawal::default()).with_settlers(2);
                building.start_construction(site, context);
            });
            cmds.execute(&context);
        }

        let house = building_at(&env, cell).unwrap();
        assert!(house.is(BuildingKind::House) && house.is_under_construction());
        assert_eq!(house.population_count(), 0);

        run_for(&mut env, build_secs + 1.0);
        let house = building_at(&env, cell).unwrap();
        assert!(!house.is_under_construction());
        assert_eq!(house.population_count(), 2);
    });
}

// The developer editor's mode places finished buildings, even with sites on.
fn test_editor_mode_builds_finished() {
    as_player(|| {
        let mut env = TestEnvironment::new();
        let cell = Cell::new(6, 6);
        assert!(place(&mut env, cell, "small_well", ConstructionMode::Finished));
        assert!(!building_at(&env, cell).unwrap().is_under_construction());
        undo_redo::clear();
    });
}
