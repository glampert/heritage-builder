use common::{Size, coords::Cell};
use game::{
    building::{BuildingKindAndId, construction::ConstructionMode},
    cheats,
    debug::game_object_debug::GameObjectDebugVarRef,
    faith::events,
    farms::{self, FieldPlot, FieldStage, GrowingCrop},
    menu::{PlaceOrClearResult, TilePlacement},
    sim::{SimCmdQueue, calendar::CalendarDate, resources::ResourceKind},
    tile::{
        TileKind,
        TileMapLayerKind,
        sets::{OBJECTS_FIELDS_CATEGORY, TileDef, TileSets},
    },
    undo_redo,
    unit::config::UnitConfigKey,
};

mod test_utils;
use test_utils::TestEnvironment;

// ----------------------------------------------
// Integration tests for farmsteads and their fields
// (docs/plans/seasons-floods.md, Phase 1).
// ----------------------------------------------

fn main() {
    test_utils::run_tests("Farms", &[
        // Laying fields:
        test_utils::test_fn!(test_a_field_needs_a_farmstead_in_reach),
        test_utils::test_fn!(test_a_field_joins_the_nearest_farmstead),
        test_utils::test_fn!(test_a_farmstead_claims_the_fields_left_without_one),
        test_utils::test_fn!(test_a_cleared_field_is_dropped),
        test_utils::test_fn!(test_an_area_is_one_undo_step),
        test_utils::test_fn!(test_fields_are_kept_nearest_first),
        // Crops:
        test_utils::test_fn!(test_workers_tend_the_nearest_fields),
        test_utils::test_fn!(test_a_field_is_sown_in_its_season_and_reaped),
        test_utils::test_fn!(test_a_field_is_sown_once_a_season),
        test_utils::test_fn!(test_months_untended_cut_the_harvest),
        test_utils::test_fn!(test_a_site_tends_nothing),
        test_utils::test_fn!(test_the_stage_follows_the_crop),
        test_utils::test_fn!(test_seasons_start_where_they_turn),
        test_utils::test_fn!(test_a_harvest_scales_the_harvest),
        // Delivery and state:
        test_utils::test_fn!(test_a_harvest_is_carried_in_loads),
        test_utils::test_fn!(test_the_reports),
        test_utils::test_fn!(test_fields_round_trip_a_save),
    ]);
}

// ----------------------------------------------
// Helpers
// ----------------------------------------------

fn field_def() -> &'static TileDef {
    TileSets::get()
        .find_tile_def_by_name(TileMapLayerKind::Objects, OBJECTS_FIELDS_CATEGORY.string, "rice_field")
        .expect("the rice field's tile def")
}

fn grass_map() -> TestEnvironment {
    TestEnvironment::with_terrain(Size::new(48, 48), test_utils::find_terrain_def("grass"))
}

fn place(env: &mut TestEnvironment, cell: Cell, tile_def: &'static TileDef) -> PlaceOrClearResult {
    let context = env.new_sim_context(0.0);
    TilePlacement::place(&context, cell, tile_def, false, false, ConstructionMode::Finished)
}

fn place_field(env: &mut TestEnvironment, cell: Cell) -> bool {
    place(env, cell, field_def()).is_ok()
}

fn farmstead(env: &mut TestEnvironment, cell: Cell) -> BuildingKindAndId {
    let result = place(env, cell, test_utils::find_building_def("farmstead"));
    assert!(result.is_ok(), "a farmstead at {cell}");
    let building = env.world.find_building_for_cell(cell, &env.tile_map).expect("the farmstead");
    building.kind_and_id()
}

fn fields_of(env: &TestEnvironment, farmstead: BuildingKindAndId) -> Vec<Cell> {
    test_utils::find_building(env, farmstead).as_producer().fields().iter().map(|plot| plot.cell).collect()
}

fn plots(env: &TestEnvironment, farmstead: BuildingKindAndId) -> Vec<FieldPlot> {
    test_utils::find_building(env, farmstead).as_producer().fields().to_vec()
}

// A house of `population` that keeps them (as the faith tests' houses do).
fn house_with(env: &mut TestEnvironment, cell: Cell, population: u32) -> BuildingKindAndId {
    let house = test_utils::spawn_building(env, cell, "house2");
    for name in ["freeze_upgrade_update", "freeze_population_update", "freeze_stock_update", "freeze_deprivation"] {
        let mut value = true;
        let ok = test_utils::find_building_mut(env, house)
            .debug_options()
            .set_debug_option_by_name(name, GameObjectDebugVarRef::Bool(&mut value));
        assert!(ok, "houses have a '{name}' debug option");
    }
    let context = env.new_sim_context(0.0);
    let cmds = context.cmds_mut();
    let added = context.find_building_mut(house.kind, house.id).unwrap().add_population(cmds, &context, population);
    cmds.execute(&context);
    assert_eq!(added, population, "the house should take {population}");
    house
}

// The harness staffs every building (the ignore_worker_requirements cheat): tests that count
// workers turn it off while they run.
fn with_workers_counted<F: FnOnce()>(test: F) {
    cheats::get_mut().ignore_worker_requirements = false;
    test();
    cheats::get_mut().ignore_worker_requirements = true;
}

fn staff(env: &mut TestEnvironment, farmstead: BuildingKindAndId, workers: u32) {
    let house = house_with(env, Cell::new(40, 40), workers.max(1));
    let added = test_utils::find_building_mut(env, farmstead).add_workers(workers, house);
    assert_eq!(added, workers, "{workers} workers for the farmstead");
}

// The start of `month` in year 1.
fn month(month: u32) -> CalendarDate {
    CalendarDate { year: 1, month, day: 1 }
}

fn new_month(env: &mut TestEnvironment, number: u32) {
    let context = env.new_sim_context(0.0);
    farms::on_new_month(&context, month(number));
}

// Every month from `from` to `to` (inclusive), as their starts come; months past 12 wrap into year 2.
fn months(env: &mut TestEnvironment, from: u32, to: u32) {
    for ordinal in from..=to {
        let date = CalendarDate { year: 1 + (ordinal - 1) / 12, month: (ordinal - 1) % 12 + 1, day: 1 };
        let context = env.new_sim_context(0.0);
        farms::on_new_month(&context, date);
    }
}

fn held(env: &TestEnvironment, farmstead: BuildingKindAndId) -> u32 {
    test_utils::find_building(env, farmstead).available_resources(ResourceKind::Rice)
}

fn stage_at(env: &TestEnvironment, cell: Cell) -> usize {
    env.tile_map.find_tile(cell, TileKind::Field).expect("a field").variation_index()
}

// A farmstead at (10, 10), 2x2, with `workers`, and a row of fields along its east side.
fn farm(env: &mut TestEnvironment, workers: u32, fields: i32) -> BuildingKindAndId {
    let farmstead = farmstead(env, Cell::new(10, 10));
    if workers != 0 {
        staff(env, farmstead, workers);
    }
    for i in 0..fields {
        assert!(place_field(env, Cell::new(12 + i % 4, 10 + i / 4)), "field {i}");
    }
    farmstead
}

// ----------------------------------------------
// Laying fields
// ----------------------------------------------

fn test_a_field_needs_a_farmstead_in_reach() {
    let mut env = grass_map();
    assert!(!place_field(&mut env, Cell::new(14, 10)), "no farmstead");

    let farmstead = farmstead(&mut env, Cell::new(10, 10)); // Cells 10..=11.
    let reach = farms::crop_of(test_utils::find_building(&env, farmstead)).unwrap().field_reach;
    assert_eq!(reach, 4);
    assert!(place_field(&mut env, Cell::new(11 + reach, 10)), "at its reach");
    assert!(!place_field(&mut env, Cell::new(12 + reach, 10)), "beyond it");
    assert!(place_field(&mut env, Cell::new(10, 10 - reach)), "above it, at its reach");
    assert_eq!(fields_of(&env, farmstead).len(), 2);

    // Only on empty land.
    assert!(!place_field(&mut env, Cell::new(11, 11)), "not on the farmstead");
    assert!(!place_field(&mut env, Cell::new(11 + reach, 10)), "not on another field");
    undo_redo::clear();
}

fn test_a_field_joins_the_nearest_farmstead() {
    let mut env = grass_map();
    let west = farmstead(&mut env, Cell::new(10, 10));
    let east = farmstead(&mut env, Cell::new(17, 10)); // Five cells apart.
    assert!(place_field(&mut env, Cell::new(13, 10))); // 2 from the west, 4 from the east.
    assert!(place_field(&mut env, Cell::new(15, 10))); // 4 from the west, 2 from the east.
    assert!(place_field(&mut env, Cell::new(14, 11))); // 3 and 3: the first in the world's order.
    assert_eq!(fields_of(&env, west), vec![Cell::new(13, 10), Cell::new(14, 11)]);
    assert_eq!(fields_of(&env, east), vec![Cell::new(15, 10)]);
    undo_redo::clear();
}

fn test_a_farmstead_claims_the_fields_left_without_one() {
    let mut env = grass_map();
    let first = farm(&mut env, 0, 4);
    assert_eq!(fields_of(&env, first).len(), 4);

    test_utils::despawn_building(&mut env, first);
    let context = env.new_sim_context(0.0);
    farms::claim_fields(&context);
    assert!(farms::owner_of_field(&env.world, Cell::new(12, 10)).is_none(), "fallow without a farmstead");

    let second = farmstead(&mut env, Cell::new(10, 13));
    assert_eq!(fields_of(&env, second).len(), 4, "the new farmstead claims them");
    undo_redo::clear();
}

fn test_a_cleared_field_is_dropped() {
    let mut env = grass_map();
    let farmstead = farm(&mut env, 0, 3);
    let context = env.new_sim_context(0.0);
    let tile = context.tile_map().find_tile(Cell::new(13, 10), TileKind::Field).unwrap();
    assert!(TilePlacement::clear(&context, tile, false, false).is_ok());
    assert_eq!(fields_of(&env, farmstead), vec![Cell::new(12, 10), Cell::new(14, 10)]);
    undo_redo::clear();
}

fn test_an_area_is_one_undo_step() {
    let mut env = grass_map();
    let farmstead = farmstead(&mut env, Cell::new(10, 10));
    let cells: Vec<Cell> = (12..=17).flat_map(|x| (10..=11).map(move |y| Cell::new(x, y))).collect();
    let context = env.new_sim_context(0.0);
    let placed = TilePlacement::place_area(&context, &cells, field_def(), true);
    assert_eq!(placed.placed.len(), 8, "x 12 to 15: within reach");
    assert_eq!(placed.refused.len(), 4, "x 16 and 17: beyond it");
    assert_eq!(fields_of(&env, farmstead).len(), 8);

    let context = env.new_sim_context(0.0);
    undo_redo::undo(&context);
    farms::claim_fields(&context);
    assert!(farms::field_cells(&env.tile_map).is_empty(), "one undo takes the whole area");
    assert!(fields_of(&env, farmstead).is_empty());
    undo_redo::clear();
}

fn test_fields_are_kept_nearest_first() {
    let mut env = grass_map();
    let farmstead = farmstead(&mut env, Cell::new(10, 10));
    for cell in [Cell::new(15, 10), Cell::new(12, 11), Cell::new(13, 10), Cell::new(12, 10)] {
        assert!(place_field(&mut env, cell));
    }
    assert_eq!(fields_of(&env, farmstead), vec![Cell::new(12, 10), Cell::new(12, 11), Cell::new(13, 10), Cell::new(15, 10)]);
    undo_redo::clear();
}

// ----------------------------------------------
// Crops
// ----------------------------------------------

fn test_workers_tend_the_nearest_fields() {
    with_workers_counted(workers_tend_the_nearest_fields);
}

fn workers_tend_the_nearest_fields() {
    let mut env = grass_map();
    let farmstead = farm(&mut env, 0, 12);
    assert_eq!(test_utils::find_building(&env, farmstead).as_producer().tended_field_count(), 0, "no workers");
    staff(&mut env, farmstead, 2);
    assert_eq!(test_utils::find_building(&env, farmstead).as_producer().tended_field_count(), 8, "4 a worker");

    new_month(&mut env, 5);
    let sown = plots(&env, farmstead).iter().filter(|plot| plot.crop.is_some()).count();
    assert_eq!(sown, 8, "the 8 nearest are sown, the other 4 lie fallow");
    assert!(plots(&env, farmstead)[..8].iter().all(|plot| plot.crop.is_some()));
    undo_redo::clear();
}

fn test_a_field_is_sown_in_its_season_and_reaped() {
    let mut env = grass_map();
    let farmstead = farm(&mut env, 2, 8);

    new_month(&mut env, 4);
    assert!(plots(&env, farmstead).iter().all(|plot| plot.crop.is_none()), "month 4 is dry: nothing sown");

    new_month(&mut env, 5);
    assert!(plots(&env, farmstead).iter().all(|plot| plot.crop.is_some()), "sown as the wet season begins");
    assert_eq!(stage_at(&env, Cell::new(12, 10)), FieldStage::Sown.variation());

    months(&mut env, 6, 8);
    assert_eq!(stage_at(&env, Cell::new(12, 10)), FieldStage::Ripe.variation());
    assert_eq!(held(&env, farmstead), 0);

    new_month(&mut env, 9);
    // 8 fields x 4 rice x fertility 50 (x1) x tended every month.
    assert_eq!(held(&env, farmstead), 32);
    assert_eq!(stage_at(&env, Cell::new(12, 10)), FieldStage::Fallow.variation());
    let notices = farms::take_notices();
    assert_eq!(notices.len(), 1);
    assert_eq!((notices[0].farmsteads, notices[0].fields, notices[0].amount), (1, 8, 32));
    undo_redo::clear();
}

fn test_a_field_is_sown_once_a_season() {
    let mut env = grass_map();
    let farmstead = farm(&mut env, 2, 4);
    months(&mut env, 5, 9);
    assert_eq!(held(&env, farmstead), 16, "reaped in month 9");
    assert!(plots(&env, farmstead).iter().all(|plot| plot.crop.is_none()), "month 9 is still wet: sown already");
    months(&mut env, 10, 16);
    assert!(plots(&env, farmstead).iter().all(|plot| plot.crop.is_none()), "the dry season: no water, no crop");
    months(&mut env, 17, 17);
    assert!(plots(&env, farmstead).iter().all(|plot| plot.crop.is_some()), "the next year's wet season");
    farms::take_notices();
    undo_redo::clear();
}

fn test_months_untended_cut_the_harvest() {
    with_workers_counted(months_untended_cut_the_harvest);
}

fn months_untended_cut_the_harvest() {
    let mut env = grass_map();
    let farmstead = farm(&mut env, 0, 8);
    let house = house_with(&mut env, Cell::new(40, 40), 2);
    assert_eq!(test_utils::find_building_mut(&mut env, farmstead).add_workers(2, house), 2);
    new_month(&mut env, 5);
    // Its workers leave for two of the four months.
    assert_eq!(test_utils::find_building_mut(&mut env, farmstead).remove_workers(2, house), 2);
    months(&mut env, 6, 7);
    assert_eq!(test_utils::find_building_mut(&mut env, farmstead).add_workers(2, house), 2);
    months(&mut env, 8, 9);
    assert_eq!(held(&env, farmstead), 16, "half the months tended: half the harvest");
    farms::take_notices();
    undo_redo::clear();
}

fn test_a_site_tends_nothing() {
    cheats::get_mut().instant_construction = false;
    let mut env = grass_map();
    let context = env.new_sim_context(0.0);
    let def = test_utils::find_building_def("farmstead");
    assert!(TilePlacement::place(&context, Cell::new(10, 10), def, false, false, ConstructionMode::Site).is_ok());
    let farmstead = env.world.find_building_for_cell(Cell::new(10, 10), &env.tile_map).unwrap().kind_and_id();
    assert!(test_utils::find_building(&env, farmstead).is_under_construction());
    assert!(place_field(&mut env, Cell::new(12, 10)), "fields can be laid around a site");
    new_month(&mut env, 5);
    assert_eq!(plots(&env, farmstead).len(), 1);
    assert!(plots(&env, farmstead).iter().all(|plot| plot.crop.is_none()), "nothing sown by a site");
    cheats::get_mut().instant_construction = true;
    undo_redo::clear();
}

fn test_the_stage_follows_the_crop() {
    let plot = |sown| FieldPlot {
        cell: Cell::new(0, 0),
        crop: Some(GrowingCrop { sown, months: 0, tended_months: 0 }),
        sown_season: None,
    };
    assert_eq!(FieldPlot::new(Cell::new(0, 0)).stage(20, 4), FieldStage::Fallow);
    assert_eq!(plot(20).stage(20, 4), FieldStage::Sown);
    assert_eq!(plot(20).stage(21, 4), FieldStage::Growing);
    assert_eq!(plot(20).stage(22, 4), FieldStage::Growing);
    assert_eq!(plot(20).stage(23, 4), FieldStage::Ripe);
    assert_eq!(plot(20).reaped_in(4), Some(24));
    let stages = [FieldStage::Fallow, FieldStage::Sown, FieldStage::Growing, FieldStage::Ripe];
    let names: Vec<&str> = field_def().variations.iter().map(|variation| variation.name.as_str()).collect();
    for stage in stages {
        assert_eq!(names[stage.variation()], stage.name(), "the tile's variations are the stages, in order");
    }
}

fn test_seasons_start_where_they_turn() {
    let env = grass_map();
    let settings = env.sim.calendar().settings();
    // Wet months 5 to 10. Ordinal months of year 1: month m is 11 + m.
    assert_eq!(farms::season_start(settings, 12 + 4), 12 + 4, "month 5 starts the wet season");
    assert_eq!(farms::season_start(settings, 12 + 8), 12 + 4, "month 9: still the one from month 5");
    assert_eq!(farms::season_start(settings, 12 + 10), 12 + 10, "month 11 starts the dry season");
    assert_eq!(farms::season_start(settings, 24 + 2), 12 + 10, "month 3 of year 2: since month 11");
}

fn test_a_harvest_scales_the_harvest() {
    let mut env = grass_map();
    let farmstead = farm(&mut env, 2, 8);
    new_month(&mut env, 5);
    let context = env.new_sim_context(0.0);
    events::set_off_now(&context, "village_guardian", "rich_harvest").unwrap();
    months(&mut env, 6, 9);
    assert_eq!(held(&env, farmstead), 48, "32 x 1.5 under a rich harvest");
    farms::take_notices();
    undo_redo::clear();
}

// ----------------------------------------------
// Delivery and state
// ----------------------------------------------

fn test_a_harvest_is_carried_in_loads() {
    let mut env = grass_map();
    test_utils::place_road(&mut env, &(5..=25).map(|x| Cell::new(x, 9)).collect::<Vec<_>>());
    let farmstead = farm(&mut env, 2, 8);
    test_utils::spawn_building(&mut env, Cell::new(20, 6), "granary");
    months(&mut env, 5, 9);
    assert_eq!(held(&env, farmstead), 32);

    let ticks = test_utils::tick_until(&mut env, 200, 0.5, |env| held(env, farmstead) < 32);
    assert!(ticks < 200, "a runner goes out");
    assert_eq!(held(&env, farmstead), 16, "a load goes, not the whole harvest; the rest waits");
    assert!(test_utils::find_unit_by_config(&env, UnitConfigKey::Runner).is_some());

    // Let the deliveries finish so no task is left running.
    let ticks = test_utils::tick_until(&mut env, 2000, 0.5, |env| {
        held(env, farmstead) == 0 && test_utils::find_unit_by_config(env, UnitConfigKey::Runner).is_none()
    });
    assert!(ticks < 2000, "both loads delivered");
    farms::take_notices();
    undo_redo::clear();
}

fn test_the_reports() {
    with_workers_counted(the_reports);
}

fn the_reports() {
    let mut env = grass_map();
    let farmstead = farm(&mut env, 2, 10);
    new_month(&mut env, 5);
    let context = env.new_sim_context(0.0);
    let building = context.find_building(farmstead.kind, farmstead.id).unwrap();
    let report = farms::farmstead_report(&context, building).unwrap();
    assert_eq!((report.fields, report.tended, report.growing), (10, 8, 8));
    assert_eq!(report.next_reaped, Some(12 + 8), "month 9 of year 1");
    assert_eq!(report.expected, 32.0, "8 tended fields x 4 rice");

    let field = farms::field_report(&context, Cell::new(12, 10)).unwrap();
    assert_eq!(field.farmstead_cell, Some(Cell::new(10, 10)));
    assert_eq!(field.stage, FieldStage::Sown);
    assert!(field.tended && !field.irrigated && !field.flooded);
    let untended = farms::field_report(&context, Cell::new(15, 11)).unwrap();
    assert!(!untended.tended, "the two farthest lie fallow");
    assert_eq!(untended.stage, FieldStage::Fallow);
    assert!(farms::field_report(&context, Cell::new(30, 30)).is_none(), "no field there");
    undo_redo::clear();
}

fn test_fields_round_trip_a_save() {
    let mut env = grass_map();
    let farmstead = farm(&mut env, 2, 4);
    new_month(&mut env, 5);
    let fields = plots(&env, farmstead);
    let json = serde_json::to_string(&fields).unwrap();
    let back: Vec<FieldPlot> = serde_json::from_str(&json).unwrap();
    assert_eq!(serde_json::to_string(&back).unwrap(), json);
    assert!(back.iter().all(|plot| plot.crop.is_some_and(|crop| crop.sown == 12 + 4)));
    undo_redo::clear();
}
