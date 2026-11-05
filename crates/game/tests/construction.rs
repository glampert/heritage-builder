use common::coords::Cell;
use game::{
    building::{BuildingKindAndId, construction::ConstructionMaterials},
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
        test_utils::test_fn!(test_city_stock_counts_every_yard),
        test_utils::test_fn!(test_withdrawal_takes_yards_in_pool_order),
        test_utils::test_fn!(test_refund_returns_materials_to_their_yards),
        test_utils::test_fn!(test_refund_goes_elsewhere_when_a_yard_is_gone),
        // Material costs:
        test_utils::test_fn!(test_temple_needs_bricks),
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
            assert!(TilePlacement::place(&context, Cell::new(4, 4), well_def, true, false).is_ok());
        }

        assert_eq!(gold(&mut env), gold_before - well_def.cost);
    });
}

// ----------------------------------------------
// City stock
// ----------------------------------------------

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
            match TilePlacement::place(&context, temple_cell, temple_def, true, true) {
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
            assert!(TilePlacement::place(&context, temple_cell, temple_def, true, true).is_ok());
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
