use common::coords::Cell;
use game::{
    building::{BuildingKind, BuildingKindAndId, construction::ConstructionMode},
    campaign::config::MissionFaithDef,
    cheats,
    config::{FaithRules, GameConfigs},
    debug::game_object_debug::GameObjectDebugVarRef,
    faith::{
        self,
        FaithConfigs,
        Mood,
        RealmFaithDef,
        config::{self, FaithEffectDef, FaithEventDef, FaithRequirement},
        events,
        festivals::{self, FestivalState, NotHeld, PlanOutcome},
    },
    menu::{PlaceOrClearResult, TilePlacement},
    realm::Realm,
    sim::{
        CityFaith,
        SimCmdQueue,
        calendar::CalendarDate,
        faith::FaithNoticeKind,
        finance::{self, LedgerCategory},
        resources::ResourceKind,
    },
    tile::{
        TileFlags,
        TileKind,
        minimap::{
            MinimapLens,
            lens::{self, LensContext, LensData},
            style::MinimapStyle,
        },
        sets::TileDef,
    },
    undo_redo,
    unit::config::UnitConfigKey,
};
use strum::IntoEnumIterator;

mod test_utils;
use test_utils::TestEnvironment;

// ----------------------------------------------
// Integration tests for faith and festivals
// (docs/plans/faith.md).
// ----------------------------------------------

fn main() {
    test_utils::run_tests("Faith", &[
        // Data:
        test_utils::test_fn!(test_every_realm_loads_without_errors),
        test_utils::test_fn!(test_validation_finds_errors),
        test_utils::test_fn!(test_events_wait_for_their_system),
        // State:
        test_utils::test_fn!(test_a_new_game_starts_with_dai_longs_patrons),
        test_utils::test_fn!(test_starting_a_city_in_a_realm),
        test_utils::test_fn!(test_mission_switches),
        test_utils::test_fn!(test_favour_is_clamped),
        test_utils::test_fn!(test_mood_bands),
        test_utils::test_fn!(test_patrons_follow_the_realms_file_after_a_load),
        test_utils::test_fn!(test_the_simulation_has_the_faith_state),
        // Dedicated shrines and temples:
        test_utils::test_fn!(test_every_dai_long_patron_has_its_shrine_and_temple),
        test_utils::test_fn!(test_a_shrines_patron_is_its_variation),
        test_utils::test_fn!(test_a_shrine_without_a_pick_gets_an_offered_patron),
        test_utils::test_fn!(test_undo_and_redo_keep_the_patron),
        // Favour:
        test_utils::test_fn!(test_coverage_counts_the_residents_in_reach),
        test_utils::test_fn!(test_favour_drifts_toward_coverage),
        test_utils::test_fn!(test_favour_holds_while_off_or_with_no_one),
        test_utils::test_fn!(test_the_faith_lens),
        // Blessings and misfortunes:
        test_utils::test_fn!(test_events_follow_favour_and_the_cooldown),
        test_utils::test_fn!(test_rolls_repeat_with_the_same_seed),
        test_utils::test_fn!(test_events_that_wait_are_never_picked),
        test_utils::test_fn!(test_a_harvest_scales_production_and_runs_out),
        test_utils::test_fn!(test_upkeep_effects_scale_the_upkeep),
        test_utils::test_fn!(test_misfortunes_through_the_city_risks),
        test_utils::test_fn!(test_set_off_now_refuses_what_cant_happen),
        // Festivals:
        test_utils::test_fn!(test_festival_terms_follow_the_scale),
        test_utils::test_fn!(test_the_festival_year),
        test_utils::test_fn!(test_a_festival_is_held_at_the_start_of_its_month),
        test_utils::test_fn!(test_a_festival_in_its_month_is_held_at_once),
        test_utils::test_fn!(test_an_every_patron_festival_pleases_them_all),
        test_utils::test_fn!(test_a_short_festival_isnt_held),
        test_utils::test_fn!(test_an_unstaffed_square_holds_nothing),
        test_utils::test_fn!(test_one_plan_at_a_time_and_cancel),
        test_utils::test_fn!(test_festivals_off_refuse_plans),
        // Saves:
        test_utils::test_fn!(test_faith_round_trips_through_a_save),
    ]);
}

// ----------------------------------------------
// Helpers
// ----------------------------------------------

fn rules() -> &'static game::config::FaithRules {
    &GameConfigs::get().sim.faith
}

fn realm_def(realm: Realm) -> &'static RealmFaithDef {
    FaithConfigs::get().for_realm(realm).unwrap_or_else(|| panic!("{realm} should have a faith file"))
}

// ----------------------------------------------
// Data
// ----------------------------------------------

// Every realm's file loads, checks and names only text keys en.json has; each has four patrons
// and four festivals, from the Heritage mockups.
fn test_every_realm_loads_without_errors() {
    let configs = FaithConfigs::get();
    assert_eq!(configs.validate(None), Vec::<String>::new());
    assert_eq!(configs.missing_text_keys(), Vec::<String>::new());
    for realm in Realm::iter() {
        let def = realm_def(realm);
        assert_eq!(def.patrons.len(), 4, "{realm}'s patrons");
        assert_eq!(def.festivals.len(), 4, "{realm}'s festivals");
        assert_eq!(def.scales.len(), 3, "{realm}'s scales");
        for festival in &def.festivals {
            assert_eq!(festival.goods().len(), festival.goods.len(), "{realm} {}: every good parses", festival.id);
        }
    }
    let dai_long = realm_def(Realm::DaiLong);
    let ids: Vec<&str> = dai_long.patrons.iter().map(|patron| patron.id.as_str()).collect();
    assert_eq!(ids, ["mountain_lord", "water_lord", "village_guardian", "ancestors"]);
    assert_eq!(config::patron_name(Realm::DaiLong, "water_lord"), "The Water Lord");
    // Effects read their building kinds by name.
    let rich_harvest = dai_long.event("rich_harvest").unwrap();
    assert!(matches!(rich_harvest.effect, FaithEffectDef::Harvest { scale, months: 3, .. } if scale == 1.5));
}

fn test_validation_finds_errors() {
    let mut def = realm_def(Realm::DaiLong).clone();
    def.patrons[1].id = def.patrons[0].id.clone();
    def.patrons[2].colour = "red".to_string();
    def.patrons[3].blessings.push("no_such_event".to_string());
    def.festivals[0].month = 13;
    def.festivals[1].patron = "no_such_patron".to_string();
    def.festivals[2].goods.insert("Moonbeams".to_string(), 5);
    def.festivals[3].month = def.festivals[2].month;
    let errors = def.validate(None);
    let expect = [
        "patron 'mountain_lord' is given twice.",
        "patron 'village_guardian': its colour 'red' isn't #RRGGBB.",
        "patron 'ancestors': no event 'no_such_event'.",
        "festival 'new_year': month 13 isn't 1 to 12.",
        "festival 'dragon_boat_races': no patron 'no_such_patron'.",
        "festival 'floodwater_offering': no patron 'water_lord'.", // Renamed away above.
        "festival 'floodwater_offering': 'Moonbeams' isn't a good.",
        "festival 'mid_autumn_lanterns': another festival is in month 8.",
    ];
    for error in expect {
        assert!(errors.iter().any(|e| e == error), "expected \"{error}\" in {errors:?}");
    }
    assert_eq!(errors.len(), expect.len(), "{errors:?}");
}

// Floods, trade, education and monasteries don't exist yet, so the events that need them wait.
fn test_events_wait_for_their_system() {
    let dai_long = realm_def(Realm::DaiLong);
    assert!(!dai_long.event("gentle_flood").unwrap().is_available(), "floods are M7's");
    assert!(!dai_long.event("scholars").unwrap().is_available(), "education comes later");
    assert!(dai_long.event("rich_harvest").unwrap().is_available());
    assert!(dai_long.event("rockfall").unwrap().is_available());

    // An effect that needs a system waits even when its event doesn't say so.
    let event = FaithEventDef { id: "x".to_string(), effect: FaithEffectDef::Envoys, requires: None };
    assert!(!event.is_available());
    assert_eq!(FaithEffectDef::Envoys.needs(), Some(FaithRequirement::Trade));
}

// ----------------------------------------------
// State
// ----------------------------------------------

fn test_a_new_game_starts_with_dai_longs_patrons() {
    let faith = CityFaith::new(rules());
    assert_eq!(faith.realm(), Realm::DaiLong);
    assert!(faith.is_favour_enabled());
    assert!(faith.is_festivals_enabled());
    assert_eq!(faith.patrons().len(), 4);
    for id in ["mountain_lord", "water_lord", "village_guardian", "ancestors"] {
        assert_eq!(faith.favour(id), Some(rules().starting_favour), "{id}");
    }
    assert!(faith.plan().is_none());
    assert!(faith.log().is_empty());
    assert!(faith.effects().is_empty());
}

fn test_starting_a_city_in_a_realm() {
    let mut faith = CityFaith::new(rules());
    faith.set_favour("water_lord", 90.0);
    faith.start(Realm::Kambara, rules());
    assert_eq!(faith.realm(), Realm::Kambara);
    assert_eq!(faith.favour("water_lord"), None, "Đại Long's patrons are gone");
    assert_eq!(faith.favour("serpent_king"), Some(rules().starting_favour));
    assert_eq!(faith.patrons().len(), 4);
}

fn test_mission_switches() {
    let mut faith = CityFaith::new(rules());
    faith.start_mission(&MissionFaithDef { favour: Some(false), festivals: None });
    assert!(!faith.is_favour_enabled());
    assert!(faith.is_festivals_enabled(), "left out: on");
    faith.start_mission(&MissionFaithDef::default());
    assert!(faith.is_favour_enabled());
}

fn test_favour_is_clamped() {
    let mut faith = CityFaith::new(rules());
    assert!(faith.set_favour("ancestors", 140.0));
    assert_eq!(faith.favour("ancestors"), Some(100.0));
    assert!(faith.set_favour("ancestors", -5.0));
    assert_eq!(faith.favour("ancestors"), Some(0.0));
    assert!(!faith.set_favour("serpent_king", 50.0), "not Đại Long's");
}

// The bands fit every favour the Heritage mockups show with its mood.
fn test_mood_bands() {
    let mockups = [
        (74.0, Mood::Pleased),
        (71.0, Mood::Pleased),
        (66.0, Mood::Pleased),
        (61.0, Mood::Content),
        (58.0, Mood::Content),
        (55.0, Mood::Content),
        (52.0, Mood::Content),
        (47.0, Mood::Watchful),
        (44.0, Mood::Watchful),
        (38.0, Mood::Restless),
        (33.0, Mood::Restless),
        (29.0, Mood::Neglected),
    ];
    for (favour, mood) in mockups {
        assert_eq!(Mood::from_favour(favour), mood, "favour {favour}");
    }
    assert_eq!(Mood::from_favour(65.0), Mood::Pleased);
    assert_eq!(Mood::from_favour(0.0), Mood::Neglected);
    assert_eq!(Mood::Watchful.display_name(), "Watchful");
}

// A save from before a patron was added gets it at the starting favour; one whose patron the
// file no longer lists drops it. A migrated v9 save has none, and gets them all.
fn test_patrons_follow_the_realms_file_after_a_load() {
    let faith = CityFaith::new(rules());
    let mut saved = serde_json::to_value(&faith).unwrap();
    saved["patrons"]["water_lord"]["favour"] = serde_json::json!(80.0);
    saved["patrons"].as_object_mut().unwrap().remove("ancestors");
    saved["patrons"]["old_spirit"] = serde_json::json!({ "favour": 10.0, "months_since_event": 2 });

    let mut loaded: CityFaith = serde_json::from_value(saved).unwrap();
    loaded.sync_patrons(rules());
    assert_eq!(loaded.favour("water_lord"), Some(80.0), "kept");
    assert_eq!(loaded.favour("ancestors"), Some(rules().starting_favour), "added");
    assert_eq!(loaded.favour("old_spirit"), None, "dropped");

    let migrated = serde_json::json!({
        "realm": "tampadesa", "favour_enabled": true, "festivals_enabled": true,
        "patrons": {}, "plan": null, "log": [], "effects": [],
    });
    let mut loaded: CityFaith = serde_json::from_value(migrated).unwrap();
    loaded.sync_patrons(rules());
    assert_eq!(loaded.realm(), Realm::Tampadesa);
    assert_eq!(loaded.patrons().len(), 4);
}

fn test_the_simulation_has_the_faith_state() {
    let mut env = TestEnvironment::new();
    assert_eq!(env.sim.faith().realm(), Realm::DaiLong);
    env.sim.faith_mut().set_favour("village_guardian", 12.0);
    {
        let context = env.new_sim_context(0.0);
        assert_eq!(context.faith().favour("village_guardian"), Some(12.0));
        context.faith_mut().set_favour("village_guardian", 34.0);
    }
    assert_eq!(env.sim.faith().favour("village_guardian"), Some(34.0));
}

// ----------------------------------------------
// Saves
// ----------------------------------------------

fn test_faith_round_trips_through_a_save() {
    let mut faith = CityFaith::new(rules());
    faith.start(Realm::Kambara, rules());
    faith.set_favour("sky_deva", 63.5);
    faith.set_festivals_enabled(false);

    let saved = serde_json::to_value(&faith).unwrap();
    let mut keys: Vec<&str> = saved.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, ["effects", "favour_enabled", "festivals_enabled", "log", "patrons", "plan", "realm"]);
    assert_eq!(saved["realm"], "kambara", "saved by name");
    assert_eq!(saved["patrons"]["sky_deva"]["favour"], 63.5, "favour saved by patron id");

    let mut loaded: CityFaith = serde_json::from_value(saved).unwrap();
    loaded.sync_patrons(rules());
    assert_eq!(loaded.realm(), Realm::Kambara);
    assert_eq!(loaded.favour("sky_deva"), Some(63.5));
    assert!(loaded.is_favour_enabled());
    assert!(!loaded.is_festivals_enabled());

    // An effect under way is saved with its months left, and does what it did after the load.
    faith.add_effect("sky_deva", "lean_year", 6);
    let saved = serde_json::to_value(&faith).unwrap();
    assert_eq!(saved["effects"][0]["event"], "lean_year");
    assert_eq!(saved["effects"][0]["months_left"], 6);
    let loaded: CityFaith = serde_json::from_value(saved).unwrap();
    assert_eq!(loaded.effects().len(), 1);
    assert_eq!(loaded.upkeep_scale(), 1.5);
}

// ----------------------------------------------
// Dedicated shrines and temples
// ----------------------------------------------

fn place_dedicated(env: &mut TestEnvironment, cell: Cell, tile: &str, variation: Option<&str>) {
    let tile_def: &'static TileDef = test_utils::find_building_def(tile);
    let variation = variation.map(|name| tile_def.find_variation(name).unwrap_or_else(|| panic!("{tile} has no {name}")));
    let context = env.new_sim_context(0.0);
    let result = TilePlacement::place_variation(&context, cell, tile_def, variation, false, true, ConstructionMode::Site);
    if let PlaceOrClearResult::Failed { message, .. } = result {
        panic!("placing {tile} at {cell} failed: {message}");
    }
}

fn patron_at(env: &mut TestEnvironment, cell: Cell) -> Option<String> {
    let context = env.new_sim_context(0.0);
    let building = context.world().find_building_for_cell(cell, context.tile_map())?;
    faith::dedicated_patron(&context, building).map(|patron| patron.id.clone())
}

// Each Đại Long patron has its shrine and its temple, picked in the palette (pick_variation),
// offered in Đại Long and in no other realm, labelled and described by the patron's strings.
fn test_every_dai_long_patron_has_its_shrine_and_temple() {
    for tile in ["shrine", "temple"] {
        let tile_def = test_utils::find_building_def(tile);
        assert!(tile_def.pick_variation, "{tile}");
        assert!(tile_def.offered_variations(Realm::Kambara).is_empty(), "{tile}: no Kambara art yet");
        let offered: Vec<&str> = tile_def
            .offered_variations(Realm::DaiLong)
            .iter()
            .map(|index| tile_def.variations[*index].name.as_str())
            .collect();
        let patrons: Vec<&str> = realm_def(Realm::DaiLong).patrons.iter().map(|patron| patron.id.as_str()).collect();
        assert_eq!(offered, patrons, "{tile}: a variation per patron, in the file's order");
        let water = &tile_def.variations[tile_def.find_variation("water_lord").unwrap()];
        assert_eq!(water.label_text(), "The Water Lord");
        assert_eq!(water.tooltip_text().as_deref(), Some("The river and its floods"));
    }
    // The old variations' indices name the first patrons: old saves keep their buildings' shapes.
    let shrine = test_utils::find_building_def("shrine");
    assert_eq!(shrine.variation_name(0), "mountain_lord");
    assert_eq!(shrine.variation_name(1), "water_lord");
    assert_eq!(test_utils::find_building_def("temple").variation_name(2), "village_guardian");
}

fn test_a_shrines_patron_is_its_variation() {
    let mut env = TestEnvironment::new();
    place_dedicated(&mut env, Cell::new(5, 5), "shrine", Some("water_lord"));
    place_dedicated(&mut env, Cell::new(10, 5), "temple", Some("ancestors"));
    assert_eq!(patron_at(&mut env, Cell::new(5, 5)).as_deref(), Some("water_lord"));
    assert_eq!(patron_at(&mut env, Cell::new(11, 6)).as_deref(), Some("ancestors"), "any cell of the temple");

    // Other buildings tend no one, and neither does a shrine in a realm whose patrons it doesn't name.
    test_utils::spawn_building(&mut env, Cell::new(20, 5), "small_well");
    assert_eq!(patron_at(&mut env, Cell::new(20, 5)), None);
    env.sim.faith_mut().start(Realm::Kambara, rules());
    assert_eq!(patron_at(&mut env, Cell::new(5, 5)), None);
    undo_redo::clear();
}

fn test_a_shrine_without_a_pick_gets_an_offered_patron() {
    let mut env = TestEnvironment::new();
    let ids: Vec<String> = realm_def(Realm::DaiLong).patrons.iter().map(|patron| patron.id.clone()).collect();
    for x in 0..6 {
        let cell = Cell::new(2 + x * 3, 8);
        place_dedicated(&mut env, cell, "shrine", None);
        let patron = patron_at(&mut env, cell).expect("a random shrine is a patron's");
        assert!(ids.contains(&patron), "{patron}");
    }
    let mut shrines = 0;
    env.world.for_each_building(BuildingKind::Shrine, |_| {
        shrines += 1;
        true
    });
    assert_eq!(shrines, 6);
    undo_redo::clear();
}

fn test_undo_and_redo_keep_the_patron() {
    let mut env = TestEnvironment::new();
    let cell = Cell::new(6, 6);
    place_dedicated(&mut env, cell, "temple", Some("ancestors"));
    {
        let context = env.new_sim_context(0.0);
        undo_redo::undo(&context);
    }
    assert_eq!(patron_at(&mut env, cell), None, "undone");
    {
        let context = env.new_sim_context(0.0);
        undo_redo::redo(&context);
    }
    assert_eq!(patron_at(&mut env, cell).as_deref(), Some("ancestors"), "redone as the same patron's");
    undo_redo::clear();
}

// ----------------------------------------------
// Favour
// ----------------------------------------------

// A house of `tile` at `cell` with `population` residents, frozen (no growth, upgrades or needs).
fn house_with(env: &mut TestEnvironment, cell: Cell, tile: &str, population: u32) -> BuildingKindAndId {
    let house = test_utils::spawn_building(env, cell, tile);
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

// A Water Lord shrine beside one house (6 residents) and far from another (12): the Water Lord
// covers 6 of 18, over half the city (coverage_needed): 6 / 9. An Ancestors temple, whose reach (40)
// takes both, counts three times over and covers them all.
fn city_with_shrine_and_temple(env: &mut TestEnvironment) {
    house_with(env, Cell::new(5, 5), "house2", 6);
    house_with(env, Cell::new(30, 5), "house2", 12);
    place_dedicated(env, Cell::new(9, 5), "shrine", Some("water_lord"));
    place_dedicated(env, Cell::new(14, 10), "temple", Some("ancestors"));
}

fn test_coverage_counts_the_residents_in_reach() {
    let mut env = TestEnvironment::new();
    city_with_shrine_and_temple(&mut env);
    let coverage = faith::coverage(&env.world, &env.tile_map, Realm::DaiLong, rules());
    let needed = 18.0 * rules().coverage_needed;

    let water = &coverage["water_lord"];
    assert_eq!((water.shrines, water.temples, water.residents), (1, 0, 6));
    assert!((water.coverage - (6.0 / needed).min(1.0)).abs() < 1e-5, "{}", water.coverage);

    let ancestors = &coverage["ancestors"];
    assert_eq!((ancestors.shrines, ancestors.temples, ancestors.residents), (0, 1, 18));
    assert_eq!(ancestors.coverage, 1.0, "a temple counts three times: capped");

    assert_eq!(coverage["mountain_lord"].coverage, 0.0);
    assert_eq!(faith::population(&env.world), 18);
    undo_redo::clear();
}

fn test_favour_drifts_toward_coverage() {
    let mut env = TestEnvironment::new();
    city_with_shrine_and_temple(&mut env);
    let start = rules().starting_favour;
    let drift = rules().favour_drift;
    let water_target = 100.0 * (6.0 / (18.0 * rules().coverage_needed)).min(1.0);

    env.sim.faith_mut().on_new_month(&env.world, &env.tile_map, rules());
    let faith = env.sim.faith();
    assert!((faith.favour("water_lord").unwrap() - (start + drift * (water_target - start))).abs() < 1e-3);
    assert!((faith.favour("ancestors").unwrap() - (start + drift * (100.0 - start))).abs() < 1e-3);
    assert!((faith.favour("mountain_lord").unwrap() - start * (1.0 - drift)).abs() < 1e-3, "untended: falls");
    assert_eq!(faith.patron("mountain_lord").unwrap().months_since_event, 1);

    // Month after month, favour closes in on the target.
    for _ in 0..40 {
        env.sim.faith_mut().on_new_month(&env.world, &env.tile_map, rules());
    }
    let faith = env.sim.faith();
    assert!((faith.favour("water_lord").unwrap() - water_target).abs() < 0.1);
    assert!(faith.favour("ancestors").unwrap() > 99.9);
    assert!(faith.favour("mountain_lord").unwrap() < 0.1);
    assert_eq!(faith.mood("mountain_lord"), Some(Mood::Neglected));
    undo_redo::clear();
}

fn test_favour_holds_while_off_or_with_no_one() {
    let mut env = TestEnvironment::new();
    place_dedicated(&mut env, Cell::new(9, 5), "shrine", Some("water_lord"));
    env.sim.faith_mut().on_new_month(&env.world, &env.tile_map, rules());
    assert_eq!(env.sim.faith().favour("mountain_lord"), Some(rules().starting_favour), "no one lives here");

    house_with(&mut env, Cell::new(5, 5), "house2", 6);
    env.sim.faith_mut().set_favour_enabled(false);
    env.sim.faith_mut().on_new_month(&env.world, &env.tile_map, rules());
    assert_eq!(env.sim.faith().favour("mountain_lord"), Some(rules().starting_favour), "favour off");
    assert_eq!(env.sim.faith().patron("mountain_lord").unwrap().months_since_event, 2, "the months still count");
    undo_redo::clear();
}

// The Faith lens colours shrines and temples by patron and houses by how many patrons reach them.
fn test_the_faith_lens() {
    assert!(MinimapLens::available().contains(&MinimapLens::Faith), "in the lens bar");
    let mut env = TestEnvironment::new();
    city_with_shrine_and_temple(&mut env);

    let style = MinimapStyle::default();
    let mut data = LensData::default();
    let context = env.new_sim_context(0.0);
    let map_size = context.tile_map().size_in_cells();
    let lens_context =
        LensContext { sim: &context, features: context.tile_map().minimap().features(), map_size, style: &style };
    lens::provider(MinimapLens::Faith).expect("a provider").compute(&lens_context, &mut data);

    let def = realm_def(Realm::DaiLong);
    let colour = |id: &str| def.patron(id).unwrap().colour();
    assert_eq!(data.building_colors[&(5, 9)], colour("water_lord"), "the shrine");
    assert_eq!(data.building_colors[&(10, 14)], colour("ancestors"), "the temple");
    let (none, all) = (style.lenses.faith_none.color(), style.lenses.faith_all.color());
    let near = data.building_colors[&(5, 5)];
    let half = |a: f32, b: f32| a + (b - a) * 0.5;
    assert!((near.r - half(none.r, all.r)).abs() < 1e-4, "two of four patrons reach the near house");
    let far = data.building_colors[&(5, 30)];
    assert!((far.r - (none.r + (all.r - none.r) * 0.25)).abs() < 1e-4, "one of four, the temple");
    assert!(data.values.iter().any(|value| *value > 0), "the reach's heat");

    let legend: Vec<&str> = data.legend.iter().map(|(_, label)| label.as_str()).collect();
    assert_eq!(legend, [
        "The Mountain Lord",
        "The Water Lord",
        "The Village Guardian",
        "The Ancestors",
        "No patron",
        "Every patron"
    ]);
    undo_redo::clear();
}

// ----------------------------------------------
// Blessings and misfortunes
// ----------------------------------------------

// Rules under which every roll past a threshold comes, with no cooldown.
fn sure_rules() -> FaithRules {
    FaithRules { blessing_chance: 1.0, misfortune_chance: 1.0, event_cooldown_months: 0, ..FaithRules::default() }
}

fn roll(env: &mut TestEnvironment, rules: &FaithRules) -> Vec<(String, String)> {
    let context = env.new_sim_context(0.0);
    events::roll_events(&context, rules)
}

fn set_off(env: &mut TestEnvironment, patron: &str, event: &str) -> Result<(), String> {
    let context = env.new_sim_context(0.0);
    events::set_off_now(&context, patron, event)
}

fn came(patron: &str, event: &str) -> Vec<(String, String)> {
    vec![(patron.to_string(), event.to_string())]
}

fn count_buildings(env: &TestEnvironment, test: impl Fn(&game::building::Building) -> bool) -> usize {
    let mut count = 0;
    env.world.for_each_building_of_any_kind(|building| {
        count += usize::from(test(building));
        true
    });
    count
}

// A blessing at or above 70, a misfortune at or below 30, nothing between; an event's patron waits
// out its cooldown; each event needs something to act on.
fn test_events_follow_favour_and_the_cooldown() {
    let mut env = TestEnvironment::new();
    assert!(roll(&mut env, &sure_rules()).is_empty(), "no one lives here");
    house_with(&mut env, Cell::new(5, 5), "house2", 6);
    assert!(roll(&mut env, &sure_rules()).is_empty(), "every patron at 50: between the thresholds");

    // The Ancestors' scholars wait for education, so their gifts come.
    env.sim.faith_mut().set_favour("ancestors", 100.0);
    assert_eq!(roll(&mut env, &sure_rules()), came("ancestors", "family_gifts"));
    assert_eq!(env.sim.finance().this_year().total(LedgerCategory::Offerings), 150, "a windfall is an offering");
    assert_eq!(env.sim.finance().this_year().income(), 150, "offerings are income");
    let notice = env.sim.faith_mut().take_notices().pop().expect("a notice for the message log");
    assert_eq!(notice.kind, FaithNoticeKind::Blessing);
    assert!(notice.text.starts_with(&config::patron_name(Realm::DaiLong, "ancestors")), "{}", notice.text);
    assert!(notice.text.contains("gifts"), "{}", notice.text);
    assert_eq!(env.sim.faith().patron("ancestors").unwrap().months_since_event, 0, "its cooldown starts");

    // With the default cooldown, nothing more for six months.
    let waiting = FaithRules { blessing_chance: 1.0, misfortune_chance: 1.0, ..FaithRules::default() };
    assert!(roll(&mut env, &waiting).is_empty(), "cooling down");
    for _ in 0..waiting.event_cooldown_months {
        env.sim
            .faith_mut()
            .on_new_month(&env.world, &env.tile_map, &FaithRules { favour_drift: 0.0, ..FaithRules::default() });
    }
    assert_eq!(roll(&mut env, &waiting), came("ancestors", "family_gifts"), "after the cooldown");

    // No chance, no event; favour off, no event.
    let never =
        FaithRules { blessing_chance: 0.0, misfortune_chance: 0.0, event_cooldown_months: 0, ..FaithRules::default() };
    assert!(roll(&mut env, &never).is_empty());
    env.sim.faith_mut().set_favour_enabled(false);
    assert!(roll(&mut env, &sure_rules()).is_empty(), "favour off for the mission");
    env.sim.faith_mut().set_favour_enabled(true);

    // A neglected Village Guardian sends blight, but only once there are fields for it.
    env.sim.faith_mut().set_favour("ancestors", 50.0);
    env.sim.faith_mut().set_favour("village_guardian", 0.0);
    assert!(roll(&mut env, &sure_rules()).is_empty(), "no farm for the blight");
    test_utils::spawn_building(&mut env, Cell::new(20, 20), "livestock_farm");
    assert_eq!(roll(&mut env, &sure_rules()), came("village_guardian", "blight"));
    let notice = env.sim.faith_mut().take_notices().pop().unwrap();
    assert_eq!(notice.kind, FaithNoticeKind::Misfortune);
    undo_redo::clear();
}

// The rolls draw from the simulation's RNG in a fixed order: the same seed gives the same events,
// and a chance between 0 and 1 gives some months with one and some without.
fn test_rolls_repeat_with_the_same_seed() {
    let run = || {
        let mut env = TestEnvironment::new();
        house_with(&mut env, Cell::new(5, 5), "house2", 6);
        let rules = FaithRules { blessing_chance: 0.5, event_cooldown_months: 0, ..FaithRules::default() };
        let mut months = Vec::new();
        for _ in 0..40 {
            env.sim.faith_mut().set_favour("ancestors", 100.0);
            months.push(!roll(&mut env, &rules).is_empty());
        }
        undo_redo::clear();
        months
    };
    let first = run();
    assert_eq!(first, run(), "the same seed, the same events");
    assert!(first.contains(&true) && first.contains(&false), "{first:?}");
}

// An event whose system the game lacks is never picked: the Mountain Lord's gentle flood waits for
// M7, so a pleased Mountain Lord always sends steady hills.
fn test_events_that_wait_are_never_picked() {
    let mut env = TestEnvironment::new();
    house_with(&mut env, Cell::new(5, 5), "house2", 6);
    env.sim.faith_mut().set_favour("mountain_lord", 100.0);
    for _ in 0..10 {
        assert_eq!(roll(&mut env, &sure_rules()), came("mountain_lord", "steady_hills"));
    }
    assert_eq!(env.sim.faith().effects().len(), 1, "the same effect again runs on, not twice");
    let error = set_off(&mut env, "mountain_lord", "gentle_flood").unwrap_err();
    assert!(error.contains("floods"), "{error}");
    undo_redo::clear();
}

// A rich harvest makes farms produce 1.5 times as fast for three months, and a blight beside it
// halves that; then they run out.
fn test_a_harvest_scales_production_and_runs_out() {
    let mut env = TestEnvironment::new();
    house_with(&mut env, Cell::new(5, 5), "house2", 6);
    test_utils::spawn_building(&mut env, Cell::new(20, 20), "livestock_farm");
    assert_eq!(env.sim.faith().production_scale(BuildingKind::Farm), 1.0);

    set_off(&mut env, "village_guardian", "rich_harvest").unwrap();
    let faith = env.sim.faith();
    assert_eq!(faith.production_scale(BuildingKind::Farm), 1.5);
    assert_eq!(faith.production_scale(BuildingKind::Mine), 1.0, "farms only");
    assert_eq!(faith.effects()[0].months_left, 3);

    set_off(&mut env, "village_guardian", "blight").unwrap();
    assert_eq!(env.sim.faith().production_scale(BuildingKind::Farm), 0.75, "both under way");

    for month in 1..=3 {
        env.sim.faith_mut().on_new_month(&env.world, &env.tile_map, rules());
        let left = env.sim.faith().effects().iter().map(|effect| effect.months_left).max().unwrap_or(0);
        assert_eq!(left, 3 - month);
    }
    assert!(env.sim.faith().effects().is_empty(), "run out");
    assert_eq!(env.sim.faith().production_scale(BuildingKind::Farm), 1.0);
    undo_redo::clear();
}

// Kambara's lean year makes every building cost half as much again to keep, its willing hands half as
// much; the month-end charges the upkeep times their product.
fn test_upkeep_effects_scale_the_upkeep() {
    let mut env = TestEnvironment::new();
    env.sim.faith_mut().start(Realm::Kambara, rules());
    house_with(&mut env, Cell::new(5, 5), "house2", 6);
    assert_eq!(env.sim.faith().upkeep_scale(), 1.0);

    set_off(&mut env, "sky_deva", "lean_year").unwrap();
    assert_eq!(env.sim.faith().upkeep_scale(), 1.5);
    set_off(&mut env, "compassionate_one", "willing_hands").unwrap();
    assert_eq!(env.sim.faith().upkeep_scale(), 0.75);

    assert_eq!(finance::scaled_upkeep(120, 1.5), 180);
    assert_eq!(finance::scaled_upkeep(120, 0.75), 90);
    assert_eq!(finance::scaled_upkeep(120, 1.0), 120);
    assert_eq!(finance::scaled_upkeep(120, -1.0), 0);
    undo_redo::clear();
}

// Fires, collapses and sickness land through the city risks: a building set burning, one destroyed,
// a house fallen sick.
fn test_misfortunes_through_the_city_risks() {
    cheats::get_mut().no_city_risks = false;
    let mut env = TestEnvironment::new();
    house_with(&mut env, Cell::new(5, 5), "house2", 6);
    house_with(&mut env, Cell::new(30, 5), "house2", 12);

    set_off(&mut env, "ancestors", "fever").unwrap();
    assert_eq!(count_buildings(&env, |building| building.has_outbreak()), 1, "a fever in one house");

    let standing = count_buildings(&env, |_| true);
    set_off(&mut env, "mountain_lord", "rockfall").unwrap();
    assert_eq!(count_buildings(&env, |_| true), standing - 1, "rockfall: a building collapses");
    // Its residents left as settlers: gone, so no unit task is left behind.
    while let Some(settler) = test_utils::find_unit_by_config(&env, UnitConfigKey::Settler) {
        let context = env.new_sim_context(0.0);
        let cmds = context.cmds_mut();
        cmds.despawn_unit_with_id(settler);
        cmds.execute(&context);
    }

    env.sim.faith_mut().start(Realm::Tampadesa, rules());
    set_off(&mut env, "house_guardian", "fires").unwrap();
    assert_eq!(count_buildings(&env, |building| building.is_burning()), 1, "the one house left burns");
    cheats::get_mut().no_city_risks = true;
    undo_redo::clear();
}

// set_off_now refuses a patron or an event the realm doesn't have, an event that isn't the patron's,
// and one with nothing to act on: fires while risks are off.
fn test_set_off_now_refuses_what_cant_happen() {
    let mut env = TestEnvironment::new();
    house_with(&mut env, Cell::new(5, 5), "house2", 6);
    assert!(set_off(&mut env, "nobody", "blight").unwrap_err().contains("No patron"));
    assert!(set_off(&mut env, "ancestors", "blight").unwrap_err().contains("isn't one of"));
    assert!(set_off(&mut env, "ancestors", "fever").is_err(), "risks are off (the harness's cheat)");
    assert!(env.sim.faith().notices().is_empty(), "nothing came");
    assert_eq!(env.sim.faith().patron("ancestors").unwrap().months_since_event, 0);
    undo_redo::clear();
}

// ----------------------------------------------
// Festivals
// ----------------------------------------------

// A city to hold festivals in: a house with people, a festival square, a granary with `rice`, a
// storage yard with `wine`, and `gold` in the treasury. Returns the square.
fn festival_city(env: &mut TestEnvironment, rice: u32, wine: u32, gold: i64) -> BuildingKindAndId {
    let house = house_with(env, Cell::new(5, 5), "house2", 6);
    let square = test_utils::spawn_building(env, Cell::new(20, 5), "festival_square");
    assert_eq!(test_utils::find_building_mut(env, square).add_workers(1, house), 1, "a worker for the square");
    let granary = test_utils::spawn_building(env, Cell::new(5, 20), "granary");
    if rice != 0 {
        assert_eq!(test_utils::find_building_mut(env, granary).receive_resources(ResourceKind::Rice, rice), rice);
    }
    let yard = test_utils::spawn_building(env, Cell::new(20, 20), "storage_yard");
    if wine != 0 {
        assert_eq!(test_utils::find_building_mut(env, yard).receive_resources(ResourceKind::Wine, wine), wine);
    }
    env.sim.finance_mut().set_balance_unrecorded(gold);
    env.world.refresh_stats(env.sim.finance(), env.sim.faith());
    square
}

// A date in the calendar's current year.
fn in_month(env: &TestEnvironment, month: u32) -> CalendarDate {
    let today = festivals::date_of(env.sim.calendar());
    CalendarDate { year: today.year, month, day: 1 }
}

fn set_month(env: &mut TestEnvironment, month: u32) {
    let configs = &GameConfigs::get().sim;
    let date = in_month(env, month);
    env.sim.calendar_mut().set_date(date, configs.days_per_month, configs.seconds_per_month).unwrap();
}

fn plan(env: &mut TestEnvironment, festival: &str, scale: &str) -> Result<PlanOutcome, String> {
    let context = env.new_sim_context(0.0);
    festivals::plan(&context, festival, scale)
}

fn new_month(env: &mut TestEnvironment, month: u32) {
    let date = in_month(env, month);
    let context = env.new_sim_context(0.0);
    festivals::on_new_month(&context, date);
}

fn stored(env: &TestEnvironment, kind: ResourceKind) -> u32 {
    festivals::held(&env.world, kind)
}

fn is_festive(env: &TestEnvironment, square: BuildingKindAndId) -> bool {
    let cell = test_utils::find_building(env, square).base_cell();
    env.tile_map.find_tile(cell, TileKind::Building).is_some_and(|tile| tile.has_flags(TileFlags::Festive))
}

// A festival's costs and effects are its middle scale's times the scale's: Đại Long's Floodwater
// offering is 240 gold, 60 rice and 12 wine for +18 at Grand; half the costs for 0.55 of the
// effects at Modest, twice for one and a half at Imperial.
fn test_festival_terms_follow_the_scale() {
    let def = realm_def(Realm::DaiLong);
    let festival = def.festival("floodwater_offering").unwrap();
    let terms = |scale: &str| festivals::terms(festival, def.scale(scale).unwrap());
    let goods = |scale: &str| -> Vec<(ResourceKind, u32)> {
        terms(scale).goods.iter().map(|item| (item.kind, item.count)).collect()
    };

    let grand = terms("grand");
    assert_eq!(grand.gold, 240);
    assert_eq!(goods("grand"), [(ResourceKind::Rice, 60), (ResourceKind::Wine, 12)]);
    assert_eq!(grand.favour, 18.0);
    assert_eq!((terms("modest").gold, goods("modest")), (120, vec![(ResourceKind::Rice, 30), (ResourceKind::Wine, 6)]));
    assert!((terms("modest").favour - 9.9).abs() < 1e-4);
    assert_eq!((terms("imperial").gold, terms("imperial").favour), (480, 27.0));
    assert_eq!(festivals::default_scale(def).unwrap().id, "grand", "the middle scale");
}

// Where each festival stands this year, and the one the planner shows first.
fn test_the_festival_year() {
    let mut env = TestEnvironment::new();
    let def = realm_def(Realm::DaiLong);
    let state = |env: &TestEnvironment, id: &str| {
        festivals::state(env.sim.faith(), def.festival(id).unwrap(), festivals::date_of(env.sim.calendar()))
    };
    set_month(&mut env, 5);
    assert_eq!(state(&env, "new_year"), FestivalState::Missed, "month 1 passed");
    assert_eq!(state(&env, "dragon_boat_races"), FestivalState::Now);
    assert_eq!(state(&env, "floodwater_offering"), FestivalState::ToCome);
    let date = festivals::date_of(env.sim.calendar());
    assert_eq!(festivals::next_festival(def, env.sim.faith(), date).unwrap().id, "dragon_boat_races", "this month's");

    set_month(&mut env, 6);
    let date = festivals::date_of(env.sim.calendar());
    assert_eq!(festivals::next_festival(def, env.sim.faith(), date).unwrap().id, "floodwater_offering", "the next");
    set_month(&mut env, 10);
    let date = festivals::date_of(env.sim.calendar());
    assert_eq!(festivals::next_festival(def, env.sim.faith(), date).unwrap().id, "new_year", "next year's first");
    undo_redo::clear();
}

// Planned before its month, a festival is held at the start of it: its gold and goods are taken
// (rice from the granary, wine from the yard), its patron's favour rises, it's logged, the square is
// dressed for the month and a message says so; the plan is done with.
fn test_a_festival_is_held_at_the_start_of_its_month() {
    let mut env = TestEnvironment::new();
    set_month(&mut env, 6);
    let square = festival_city(&mut env, 64, 20, 1000);
    env.sim.faith_mut().set_favour("water_lord", 40.0);

    assert_eq!(plan(&mut env, "floodwater_offering", "grand"), Ok(PlanOutcome::Planned));
    assert_eq!(env.sim.faith().plan().unwrap().festival, "floodwater_offering");
    new_month(&mut env, 7);
    assert!(env.sim.faith().log().is_empty(), "not its month yet");

    new_month(&mut env, 8);
    let faith = env.sim.faith();
    assert_eq!(faith.festivals_held(), 1);
    let record = &faith.log()[0];
    assert_eq!((record.festival.as_str(), record.scale.as_str()), ("floodwater_offering", "grand"));
    assert_eq!(record.ordinal_month % 12 + 1, 8);
    assert_eq!(faith.favour("water_lord"), Some(58.0), "40 + 18");
    assert_eq!(faith.favour("ancestors"), Some(rules().starting_favour), "only its patron");
    assert!(faith.plan().is_none(), "done with");
    assert_eq!(env.sim.finance().balance(), 760);
    assert_eq!(env.sim.finance().this_year().total(LedgerCategory::Festivals), 240);
    assert_eq!((stored(&env, ResourceKind::Rice), stored(&env, ResourceKind::Wine)), (4, 8));
    assert_eq!(env.world.stats().resources.granaries.count(ResourceKind::Rice), 4, "the stats follow");
    assert!(is_festive(&env, square), "dressed for the month");
    let notice = env.sim.faith_mut().take_notices().pop().unwrap();
    assert_eq!(notice.kind, FaithNoticeKind::FestivalHeld);
    assert!(notice.text.contains("Floodwater offering"), "{}", notice.text);

    // The goal counts it once the stats are counted again.
    env.world.refresh_stats(env.sim.finance(), env.sim.faith());
    assert!(game::campaign::config::MissionGoal::FestivalsHeld { min: 1 }.is_met(env.world.stats()));

    new_month(&mut env, 9);
    assert!(!is_festive(&env, square), "the month is over");
    undo_redo::clear();
}

// In its own month a festival planned is held at once; once held, planning it again is for next
// year.
fn test_a_festival_in_its_month_is_held_at_once() {
    let mut env = TestEnvironment::new();
    set_month(&mut env, 5);
    festival_city(&mut env, 64, 20, 1000);
    assert_eq!(plan(&mut env, "dragon_boat_races", "modest"), Ok(PlanOutcome::Held));
    assert_eq!(env.sim.faith().festivals_held(), 1);
    assert!(env.sim.faith().plan().is_none());
    assert_eq!(env.sim.finance().balance(), 900, "modest: half of 200");

    assert_eq!(plan(&mut env, "dragon_boat_races", "grand"), Ok(PlanOutcome::Planned), "next year's");
    let def = realm_def(Realm::DaiLong);
    let date = festivals::date_of(env.sim.calendar());
    assert_eq!(festivals::state(env.sim.faith(), def.festival("dragon_boat_races").unwrap(), date), FestivalState::Planned);
    undo_redo::clear();
}

// New Year is for every patron: each one's favour rises (Modest: 0.55 of its 10).
fn test_an_every_patron_festival_pleases_them_all() {
    let mut env = TestEnvironment::new();
    set_month(&mut env, 1);
    festival_city(&mut env, 64, 20, 1000);
    let before: Vec<f32> =
        realm_def(Realm::DaiLong).patrons.iter().map(|patron| env.sim.faith().favour(&patron.id).unwrap()).collect();
    assert_eq!(plan(&mut env, "new_year", "modest"), Ok(PlanOutcome::Held));
    for (patron, before) in realm_def(Realm::DaiLong).patrons.iter().zip(before) {
        let favour = env.sim.faith().favour(&patron.id).unwrap();
        assert!((favour - (before + 5.5)).abs() < 1e-4, "{}: {favour}", patron.id);
    }
    let notice = env.sim.faith_mut().take_notices().pop().unwrap();
    assert!(notice.text.contains("Every patron"), "{}", notice.text);
    undo_redo::clear();
}

// Without a staffed festival square, or short of gold or goods, a festival isn't held: nothing is
// taken, a message says what was short, and the plan is dropped.
fn test_a_short_festival_isnt_held() {
    let mut env = TestEnvironment::new();
    set_month(&mut env, 6);
    let square = festival_city(&mut env, 50, 20, 100);
    assert_eq!(plan(&mut env, "floodwater_offering", "grand"), Ok(PlanOutcome::Planned));
    new_month(&mut env, 8);
    assert!(env.sim.faith().log().is_empty());
    assert!(env.sim.faith().plan().is_none(), "dropped");
    assert_eq!(env.sim.finance().balance(), 100, "nothing taken");
    assert_eq!(stored(&env, ResourceKind::Rice), 50);
    let notice = env.sim.faith_mut().take_notices().pop().unwrap();
    assert_eq!(notice.kind, FaithNoticeKind::FestivalNotHeld);
    assert!(notice.text.contains("140 gold") && notice.text.contains("10 Rice"), "{}", notice.text);

    // In its month a hold that falls short says why, and plans nothing.
    set_month(&mut env, 8);
    match plan(&mut env, "floodwater_offering", "modest") {
        Ok(PlanOutcome::NotHeld(NotHeld::Short(short))) => {
            assert_eq!((short.venue, short.gold, short.goods.len()), (false, 20, 0), "120 gold against 100");
        }
        other => panic!("{other:?}"),
    }
    assert!(env.sim.faith().plan().is_none());
    assert_eq!(env.sim.faith_mut().take_notices().pop().unwrap().kind, FaithNoticeKind::FestivalNotHeld);

    // No square, no festival, whatever the treasury holds.
    test_utils::despawn_building(&mut env, square);
    env.sim.finance_mut().set_balance_unrecorded(1000);
    match plan(&mut env, "floodwater_offering", "modest") {
        Ok(PlanOutcome::NotHeld(NotHeld::Short(short))) => assert!(short.venue && short.gold == 0),
        other => panic!("{other:?}"),
    }
    undo_redo::clear();
}

// An unstaffed festival square can't hold a festival (without the harness's cheat).
fn test_an_unstaffed_square_holds_nothing() {
    let mut env = TestEnvironment::new();
    test_utils::spawn_building(&mut env, Cell::new(20, 5), "festival_square");
    cheats::get_mut().ignore_worker_requirements = false;
    let venues = festivals::venues(&env.world).len();
    cheats::get_mut().ignore_worker_requirements = true;
    assert_eq!(venues, 0, "no workers");
    assert_eq!(festivals::venues(&env.world).len(), 1, "the cheat staffs it");
    undo_redo::clear();
}

// One plan at a time: a second replaces the first; cancel drops it.
fn test_one_plan_at_a_time_and_cancel() {
    let mut env = TestEnvironment::new();
    set_month(&mut env, 2);
    assert_eq!(plan(&mut env, "floodwater_offering", "grand"), Ok(PlanOutcome::Planned));
    assert_eq!(plan(&mut env, "mid_autumn_lanterns", "imperial"), Ok(PlanOutcome::Planned));
    let planned = env.sim.faith().plan().unwrap().clone();
    assert_eq!((planned.festival.as_str(), planned.scale.as_str()), ("mid_autumn_lanterns", "imperial"));
    assert!(festivals::cancel(env.sim.faith_mut()));
    assert!(env.sim.faith().plan().is_none());
    assert!(!festivals::cancel(env.sim.faith_mut()), "nothing left to cancel");
    assert!(plan(&mut env, "harvest_moon", "grand").unwrap_err().contains("No festival"));
    assert!(plan(&mut env, "new_year", "royal").unwrap_err().contains("No scale"), "Kambara's, not Đại Long's");
    undo_redo::clear();
}

// A mission that turns festivals off refuses plans, and a plan made before isn't held, without a
// message (the mission's switch, not a shortage).
fn test_festivals_off_refuse_plans() {
    let mut env = TestEnvironment::new();
    set_month(&mut env, 6);
    festival_city(&mut env, 64, 20, 1000);
    assert_eq!(plan(&mut env, "floodwater_offering", "grand"), Ok(PlanOutcome::Planned));
    env.sim.faith_mut().set_festivals_enabled(false);
    assert!(plan(&mut env, "floodwater_offering", "grand").unwrap_err().contains("off"));
    new_month(&mut env, 8);
    assert!(env.sim.faith().log().is_empty());
    assert!(env.sim.faith().plan().is_none());
    assert!(env.sim.faith().notices().is_empty());
    undo_redo::clear();
}
