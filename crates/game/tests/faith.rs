use game::{
    campaign::config::MissionFaithDef,
    config::GameConfigs,
    faith::{
        FaithConfigs,
        Mood,
        RealmFaithDef,
        config::{self, FaithEffectDef, FaithEventDef, FaithRequirement},
    },
    realm::Realm,
    sim::CityFaith,
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
}
