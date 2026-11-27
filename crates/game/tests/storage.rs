use common::{Size, coords::Cell};
use game::{
    building::BuildingKindAndId,
    config::GameConfigs,
    inspect::StorageOrder,
    sim::resources::ResourceKind,
    unit::config::UnitConfigKey,
};

mod test_utils;
use test_utils::TestEnvironment;

// ----------------------------------------------
// A store's orders' errands (docs/plans/messages.md, M11 Phase 2): Get fetches a good from the
// nearest other store in reach that holds it and doesn't get it too; Empty carries it out to the
// nearest that takes it; one errand at a time, by its runner. The harness staffs every store
// (ignore_worker_requirements).
// ----------------------------------------------

fn main() {
    test_utils::run_tests("Storage", &[
        test_utils::test_fn!(test_get_fetches_from_another_store),
        test_utils::test_fn!(test_get_skips_stores_that_get_it_too),
        test_utils::test_fn!(test_empty_carries_out),
        test_utils::test_fn!(test_empty_with_nowhere_to_go),
        test_utils::test_fn!(test_stores_beyond_reach),
        test_utils::test_fn!(test_errand_reported),
    ]);
}

// ----------------------------------------------
// Helpers
// ----------------------------------------------

const ROAD_Y: i32 = 10;
const TICK: f32 = 0.1;

fn city(road_end: i32) -> TestEnvironment {
    let mut env = TestEnvironment::with_map_size(Size::new(64, 24));
    let road: Vec<Cell> = (1..=road_end).map(|x| Cell::new(x, ROAD_Y)).collect();
    test_utils::place_road(&mut env, &road);
    env
}

// A 3 × 3 storage yard south of the road at `x`.
fn yard(env: &mut TestEnvironment, x: i32) -> BuildingKindAndId {
    let yard = test_utils::spawn_building(env, Cell::new(x, ROAD_Y + 1), "storage_yard");
    assert!(test_utils::find_building(env, yard).is_linked_to_road(), "the yard is by the road");
    yard
}

fn stock(env: &mut TestEnvironment, store: BuildingKindAndId, kind: ResourceKind, count: u32) {
    let received = test_utils::find_building_mut(env, store).receive_resources(kind, count);
    assert_eq!(received, count, "the store takes {count} {kind}");
}

fn held(env: &TestEnvironment, store: BuildingKindAndId, kind: ResourceKind) -> u32 {
    test_utils::find_building(env, store).available_resources(kind)
}

fn order(env: &mut TestEnvironment, store: BuildingKindAndId, kind: ResourceKind, order: StorageOrder) {
    test_utils::find_building_mut(env, store).as_storage_mut().set_orders(kind, order);
}

fn runner_out(env: &TestEnvironment, store: BuildingKindAndId) -> bool {
    test_utils::find_building(env, store).as_storage().is_runner_out()
}

// Ticks until no runner is about, so no task is left behind.
fn drain(env: &mut TestEnvironment) {
    test_utils::tick_until(env, 2000, TICK, |env| test_utils::find_unit_by_config(env, UnitConfigKey::Runner).is_none());
}

// ----------------------------------------------
// Tests
// ----------------------------------------------

fn test_get_fetches_from_another_store() {
    let mut env = city(30);
    let getter = yard(&mut env, 2);
    let holder = yard(&mut env, 12);
    stock(&mut env, holder, ResourceKind::Clay, 12);
    order(&mut env, getter, ResourceKind::Clay, StorageOrder::Get);

    // A load (8) a trip, then the rest.
    test_utils::tick_until(&mut env, 1500, TICK, |env| held(env, getter, ResourceKind::Clay) >= 8);
    assert_eq!(held(&env, getter, ResourceKind::Clay) + held(&env, holder, ResourceKind::Clay), 12, "nothing lost");
    test_utils::tick_until(&mut env, 2500, TICK, |env| held(env, getter, ResourceKind::Clay) == 12);
    assert_eq!(held(&env, holder, ResourceKind::Clay), 0);

    // Accept again: it sends no one.
    order(&mut env, getter, ResourceKind::Clay, StorageOrder::Accept);
    drain(&mut env);
    test_utils::tick_n(&mut env, 150, TICK);
    assert!(!runner_out(&env, getter));
}

fn test_get_skips_stores_that_get_it_too() {
    let mut env = city(30);
    let first = yard(&mut env, 2);
    let second = yard(&mut env, 12);
    stock(&mut env, second, ResourceKind::Clay, 8);
    order(&mut env, first, ResourceKind::Clay, StorageOrder::Get);
    order(&mut env, second, ResourceKind::Clay, StorageOrder::Get);

    // Neither takes from the other, nor from itself.
    test_utils::tick_n(&mut env, 300, TICK);
    assert!(!runner_out(&env, first) && !runner_out(&env, second));
    assert_eq!(held(&env, first, ResourceKind::Clay), 0);
    assert_eq!(held(&env, second, ResourceKind::Clay), 8);

    // A store that only accepts it: they fetch from it.
    let third = yard(&mut env, 22);
    stock(&mut env, third, ResourceKind::Clay, 8);
    test_utils::tick_until(&mut env, 2500, TICK, |env| held(env, third, ResourceKind::Clay) == 0);
    drain(&mut env);
    assert_eq!(held(&env, second, ResourceKind::Clay) + held(&env, first, ResourceKind::Clay), 16);
    assert!(held(&env, second, ResourceKind::Clay) >= 8, "the second gave none of its own");
}

fn test_empty_carries_out() {
    let mut env = city(30);
    let emptied = yard(&mut env, 2);
    let taker = yard(&mut env, 12);
    stock(&mut env, emptied, ResourceKind::Bricks, 10);
    order(&mut env, emptied, ResourceKind::Bricks, StorageOrder::Empty);
    // It takes in no more.
    assert_eq!(test_utils::find_building(&env, emptied).receivable_resources(ResourceKind::Bricks), 0);

    test_utils::tick_until(&mut env, 2500, TICK, |env| held(env, taker, ResourceKind::Bricks) == 10);
    assert_eq!(held(&env, emptied, ResourceKind::Bricks), 0);
    drain(&mut env);
    assert!(test_utils::find_building(&env, emptied).as_storage().cannot_empty().is_empty());
}

fn test_empty_with_nowhere_to_go() {
    let mut env = city(30);
    let emptied = yard(&mut env, 2);
    let other = yard(&mut env, 12);
    stock(&mut env, emptied, ResourceKind::Bricks, 6);
    order(&mut env, emptied, ResourceKind::Bricks, StorageOrder::Empty);
    order(&mut env, other, ResourceKind::Bricks, StorageOrder::Refuse);

    let secs = GameConfigs::get().sim.storage_errand_secs;
    test_utils::tick_n(&mut env, ((secs / TICK) as usize) * 2, TICK);
    assert!(!runner_out(&env, emptied));
    assert_eq!(held(&env, emptied, ResourceKind::Bricks), 6, "it keeps them");
    assert_eq!(test_utils::find_building(&env, emptied).as_storage().cannot_empty(), ResourceKind::Bricks);

    // Once the other takes them, they go.
    order(&mut env, other, ResourceKind::Bricks, StorageOrder::Accept);
    test_utils::tick_until(&mut env, 2500, TICK, |env| held(env, other, ResourceKind::Bricks) == 6);
    drain(&mut env);
}

fn test_stores_beyond_reach() {
    let reach = GameConfigs::get().sim.storage_reach;
    let mut env = city(62);
    let getter = yard(&mut env, 2);
    // Its road link to the other's along the road: more than the reach.
    let far = yard(&mut env, 2 + reach + 6);
    stock(&mut env, far, ResourceKind::Wood, 8);
    order(&mut env, getter, ResourceKind::Wood, StorageOrder::Get);

    let secs = GameConfigs::get().sim.storage_errand_secs;
    test_utils::tick_n(&mut env, ((secs / TICK) as usize) * 2, TICK);
    assert!(!runner_out(&env, getter), "no store within {reach} cells holds wood");
    assert_eq!(held(&env, far, ResourceKind::Wood), 8);
}

fn test_errand_reported() {
    let mut env = city(30);
    let getter = yard(&mut env, 2);
    let holder = yard(&mut env, 16);
    stock(&mut env, holder, ResourceKind::Pottery, 8);
    order(&mut env, getter, ResourceKind::Pottery, StorageOrder::Get);
    test_utils::tick_until(&mut env, 500, TICK, |env| runner_out(env, getter));
    // Spawned on the next step.
    test_utils::tick_n(&mut env, 2, TICK);
    {
        let context = env.new_sim_context(0.0);
        let store = context.find_building(getter.kind, getter.id).unwrap().as_storage();
        assert_eq!(store.errand(&context), Some((StorageOrder::Get, ResourceKind::Pottery, 8)));
    }
    test_utils::tick_until(&mut env, 2500, TICK, |env| held(env, getter, ResourceKind::Pottery) == 8);
    drain(&mut env);
    let context = env.new_sim_context(0.0);
    let store = context.find_building(getter.kind, getter.id).unwrap().as_storage();
    assert_eq!(store.errand(&context), None);
}
