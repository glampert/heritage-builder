use game::{
    campaign::{
        self,
        config::{CampaignConfigs, CampaignDef, MissionDef, MissionMap, MissionRequirements, assign_campaign_ids},
    },
    debug::preset_maps,
    profile::{self, CampaignRecord, PROFILE_VERSION, Profile},
    realm::{self, Realm, UiRealm, UiRealmInputs, UiRealmSource, choose_ui_realm},
    tile::minimap::style,
};
use strum::IntoEnumIterator;

mod test_utils;

// ----------------------------------------------
// Integration tests for realms and the player's profile
// (docs/plans/realm-ui.md, Realm and progress).
//
// They never load or write saves/user/profile.json: the profile is read from strings, and the
// profile of the run stays in memory, as the test harness never loads it.
// ----------------------------------------------

fn main() {
    test_utils::run_tests("Realm and profile", &[
        // Realm:
        test_utils::test_fn!(test_realm_names),
        test_utils::test_fn!(test_campaign_ids),
        // Profile:
        test_utils::test_fn!(test_profile_round_trips),
        test_utils::test_fn!(test_broken_profile_gives_fresh_one),
        test_utils::test_fn!(test_other_version_gives_fresh_one),
        test_utils::test_fn!(test_mission_progress),
        test_utils::test_fn!(test_profile_edits),
        // UI realm:
        test_utils::test_fn!(test_ui_realm_choice),
        test_utils::test_fn!(test_reaching_campaign_two_switches_home_realm),
        test_utils::test_fn!(test_in_game_realm_follows_active_campaign),
        test_utils::test_fn!(test_override_wins),
        test_utils::test_fn!(test_minimap_style_follows_ui_realm),
        // Last: the profile of the run was changed in memory only.
        test_utils::test_fn!(test_profile_kept_in_memory),
    ]);
}

const TEST_PRESET: usize = preset_maps::PRESET_1_FARM_1_GRANARY_1_HOUSE_2_WELLS_1_MARKET;

fn mission(name: &str) -> MissionDef {
    MissionDef {
        name: name.into(),
        description: String::new(),
        map: MissionMap::Preset { preset_number: TEST_PRESET },
        requirements: MissionRequirements::default(),
        calendar: None,
        finance: None,
        risks: None,
    }
}

fn campaign_def(id: &str, realm: Realm, mission_count: usize) -> CampaignDef {
    CampaignDef {
        id: id.into(),
        name: format!("Campaign {id}"),
        realm,
        missions: (0..mission_count).map(|index| mission(&format!("Mission {index}"))).collect(),
        eras: Vec::new(),
    }
}

// Three campaigns, one per realm, as the game will have them.
fn build_test_configs() -> CampaignConfigs {
    CampaignConfigs {
        campaigns: vec![
            campaign_def("one", Realm::DaiLong, 2),
            campaign_def("two", Realm::Kambara, 2),
            campaign_def("three", Realm::Tampadesa, 1),
        ],
    }
}

// The configs singleton the global functions read (realm::ui_realm_for, profile hooks), and a
// clean campaign manager, profile and override for each test.
fn ensure_setup() -> &'static CampaignConfigs {
    if !CampaignConfigs::is_initialized() {
        CampaignConfigs::initialize(build_test_configs());
    }
    campaign::initialize();
    campaign::reset();
    campaign::take_finished_missions(); // A reset keeps them queued: drop an earlier test's.
    profile::reset();
    realm::set_override(None);
    CampaignConfigs::get()
}

// ----------------------------------------------
// Realm
// ----------------------------------------------

// Configs, the profile and automation spell realms in lowercase, through serde and strum alike.
fn test_realm_names() {
    assert_eq!(Realm::names(), vec!["dailong", "kambara", "tampadesa"]);
    for realm in Realm::iter() {
        let json = serde_json::to_string(&realm).unwrap();
        assert_eq!(json, format!("\"{realm}\""));
        assert_eq!(serde_json::from_str::<Realm>(&json).unwrap(), realm);
        assert_eq!(Realm::parse(&realm.to_string()).unwrap(), realm);
    }
    assert_eq!(Realm::DaiLong.display_name(), "Đại Long");
    assert_eq!(Realm::Kambara.minimap_style_file(), "ui/minimap/kambara.json");

    let err = Realm::parse("DaiLong").unwrap_err();
    assert!(err.contains("dailong, kambara, tampadesa"), "{err}");
    assert!(serde_json::from_str::<Realm>("\"DaiLong\"").is_err());

    // A campaign names its realm, and one that doesn't is Đại Long's.
    let def: CampaignDef = serde_json::from_str(r#"{ "id": "x", "name": "X", "realm": "tampadesa", "missions": [] }"#).unwrap();
    assert_eq!(def.realm, Realm::Tampadesa);
    let def: CampaignDef = serde_json::from_str(r#"{ "name": "X", "missions": [] }"#).unwrap();
    assert_eq!(def.realm, Realm::DaiLong);
    assert!(def.id.is_empty(), "the configs' post_load gives it an id (assign_campaign_ids)");
}

// The configs give a campaign without an id one from its place in the list, and report ids that
// campaigns share, since the profile keys progress by id.
fn test_campaign_ids() {
    let mut campaigns = vec![campaign_def("", Realm::DaiLong, 1), campaign_def("two", Realm::Kambara, 1), campaign_def("", Realm::Tampadesa, 1)];
    assert!(assign_campaign_ids(&mut campaigns).is_empty());
    let ids: Vec<&str> = campaigns.iter().map(|campaign| campaign.id.as_str()).collect();
    assert_eq!(ids, ["campaign_0", "two", "campaign_2"]);

    let mut campaigns = vec![
        campaign_def("same", Realm::DaiLong, 1),
        campaign_def("same", Realm::Kambara, 1),
        campaign_def("same", Realm::Tampadesa, 1),
        campaign_def("campaign_4", Realm::DaiLong, 1),
        campaign_def("", Realm::DaiLong, 1), // Takes "campaign_4" too.
    ];
    assert_eq!(assign_campaign_ids(&mut campaigns), ["same", "campaign_4"]);
}

// ----------------------------------------------
// Profile
// ----------------------------------------------

fn sample_profile() -> Profile {
    Profile {
        version: PROFILE_VERSION,
        campaigns: vec![
            CampaignRecord {
                id: "one".into(),
                reached: true,
                current_mission: Some(1),
                missions_finished: vec![0, 1],
                distinctions: vec!["builder".into()],
                finished: true,
            },
            CampaignRecord { id: "two".into(), reached: true, current_mission: None, ..Default::default() },
        ],
        last_save: Some("autosave".into()),
        custom_realms: vec![Realm::DaiLong, Realm::Kambara],
    }
}

fn test_profile_round_trips() {
    let profile = sample_profile();
    let json = profile.to_json();
    assert!(json.contains("\"kambara\""), "realms are saved by their lowercase names: {json}");
    assert_eq!(Profile::from_json(&json).unwrap(), profile);

    // Through a Value too, as profile.state and recordings hold it.
    assert_eq!(Profile::from_value(profile.to_value()).unwrap(), profile);

    // A fresh profile round-trips, and no file means a fresh profile without a warning.
    assert_eq!(Profile::from_json(&Profile::new().to_json()).unwrap(), Profile::new());
    assert_eq!(Profile::from_file_text(None), (Profile::new(), None));
    assert_eq!(Profile::from_file_text(Some(&json)), (profile, None));
}

fn test_broken_profile_gives_fresh_one() {
    for broken in ["", "{ not json", "[]", r#"{ "version": 1 }"#, r#"{ "version": 1, "campaigns": [], "custom_realms": ["atlantis"] }"#] {
        let (profile, warning) = Profile::from_file_text(Some(broken));
        assert_eq!(profile, Profile::new(), "{broken}");
        assert!(warning.is_some(), "a broken file is reported: {broken}");
    }

    // A field the profile doesn't have is an error, not ignored.
    let mut value = sample_profile().to_value();
    value["unknown"] = serde_json::json!(true);
    let (profile, warning) = Profile::from_file_text(Some(&value.to_string()));
    assert_eq!(profile, Profile::new());
    assert!(warning.unwrap().contains("unknown"));
}

fn test_other_version_gives_fresh_one() {
    let mut value = sample_profile().to_value();
    value["version"] = serde_json::json!(PROFILE_VERSION + 1);
    let (profile, warning) = Profile::from_file_text(Some(&value.to_string()));
    assert_eq!(profile, Profile::new());
    let warning = warning.expect("another version is reported");
    assert!(warning.contains(&format!("version {}", PROFILE_VERSION + 1)), "{warning}");

    value.as_object_mut().unwrap().remove("version");
    let (profile, warning) = Profile::from_file_text(Some(&value.to_string()));
    assert_eq!(profile, Profile::new());
    assert!(warning.is_some());
}

// Starting a mission reaches its campaign and unlocks its realm; finishing the last mission
// finishes the campaign and reaches the next one.
fn test_mission_progress() {
    let configs = build_test_configs();
    let mut profile = Profile::new();

    profile.mission_started(&configs, 0, 0);
    let record = profile.record("one").unwrap();
    assert!(record.reached && !record.finished);
    assert_eq!(record.current_mission, Some(0));
    assert_eq!(profile.custom_realms, vec![Realm::DaiLong]);

    profile.mission_finished(&configs, 0, 0);
    profile.mission_finished(&configs, 0, 0); // Once only.
    assert_eq!(profile.record("one").unwrap().missions_finished, vec![0]);
    assert!(!profile.is_reached("two"));

    profile.mission_started(&configs, 0, 1);
    profile.mission_finished(&configs, 0, 1);
    let record = profile.record("one").unwrap();
    assert!(record.finished);
    assert_eq!(record.missions_finished, vec![0, 1]);
    assert_eq!(record.current_mission, Some(1));
    assert!(profile.is_reached("two"), "finishing campaign one reaches campaign two");
    assert!(!profile.custom_realms.contains(&Realm::Kambara), "a realm is unlocked when its campaign is started");

    profile.mission_started(&configs, 1, 0);
    assert_eq!(profile.custom_realms, vec![Realm::DaiLong, Realm::Kambara]);

    // The last campaign has no next one; an unknown campaign changes nothing.
    profile.mission_finished(&configs, 2, 0);
    assert!(profile.record("three").unwrap().finished);
    let before = profile.clone();
    profile.mission_started(&configs, 7, 0);
    profile.mission_finished(&configs, 7, 0);
    assert_eq!(profile, before);

    profile.game_saved("my_city");
    assert_eq!(profile.last_save.as_deref(), Some("my_city"));
}

// profile.set's edits: records merge over the defaults and are checked against the configs.
fn test_profile_edits() {
    let configs = ensure_setup();
    let mut profile = Profile::new();

    // A record names a campaign once, its missions in range; the fields left out take their
    // defaults, and its finished missions are sorted, each once.
    profile
        .set_campaign_records(configs, vec![serde_json::json!({ "id": "one", "missions_finished": [1, 0, 1] }), serde_json::json!({ "id": "two", "reached": true, "current_mission": 1 })])
        .unwrap();
    assert_eq!(profile.campaigns.len(), 2);
    assert_eq!(profile.campaigns[0], CampaignRecord { id: "one".into(), missions_finished: vec![0, 1], ..CampaignRecord::default() });
    assert!(!profile.is_reached("one") && profile.is_reached("two"));
    assert_eq!(profile.campaigns[1].current_mission, Some(1));

    let before = profile.clone();
    let err = profile.set_campaign_records(configs, vec![serde_json::json!({ "id": "three", "current_mission": 1 })]).unwrap_err();
    assert!(err.contains("has 1 missions: there's no mission 1"), "{err}");
    let err = profile.set_campaign_records(configs, vec![serde_json::json!({ "id": "one" }), serde_json::json!({ "id": "one" })]).unwrap_err();
    assert!(err.contains("two records"), "{err}");
    assert!(profile.set_campaign_records(configs, vec![serde_json::json!({ "id": "four" })]).unwrap_err().contains("No campaign has the id 'four'"));
    assert!(profile.set_campaign_records(configs, vec![serde_json::json!(["one"])]).unwrap_err().contains("is an object"));
    assert!(profile.set_campaign_records(configs, vec![serde_json::json!({ "id": "one", "stars": 3 })]).unwrap_err().contains("Invalid campaign record"));
    assert_eq!(profile, before, "a failed edit changes nothing");

    // Reached: exactly the campaigns named, a new record for one without.
    profile.set_reached(configs, &["one".into(), "three".into()]).unwrap();
    assert!(profile.is_reached("one") && !profile.is_reached("two") && profile.is_reached("three"));
    assert_eq!(profile.campaigns[1].current_mission, Some(1), "the rest of a record stays");
    assert!(profile.set_reached(configs, &["four".into()]).unwrap_err().contains("Campaigns: one, two, three."));
    assert_eq!(profile.latest_reached_realm(configs), Some(Realm::Tampadesa));

    // Custom Game's realms, each once.
    profile.set_custom_realms(&["kambara".into(), "dailong".into(), "kambara".into()]).unwrap();
    assert_eq!(profile.custom_realms, [Realm::Kambara, Realm::DaiLong]);
    assert!(profile.set_custom_realms(&["atlantis".into()]).is_err());
    assert_eq!(profile.custom_realms, [Realm::Kambara, Realm::DaiLong]);
}

// ----------------------------------------------
// UI realm
// ----------------------------------------------

fn test_ui_realm_choice() {
    let chosen = |realm, source| UiRealm { realm, source };

    // Nothing to go by: Đại Long, in the menus and in game.
    for in_home_menu in [true, false] {
        let inputs = UiRealmInputs { in_home_menu, ..Default::default() };
        assert_eq!(choose_ui_realm(&inputs), chosen(Realm::DaiLong, UiRealmSource::Default));
    }

    // In game: the campaign's realm, else the Custom Game's; the profile's doesn't count.
    let in_game = UiRealmInputs { profile_realm: Some(Realm::Tampadesa), ..Default::default() };
    assert_eq!(choose_ui_realm(&in_game), chosen(Realm::DaiLong, UiRealmSource::Default));
    let custom_game = UiRealmInputs { custom_game_realm: Some(Realm::Kambara), ..in_game };
    assert_eq!(choose_ui_realm(&custom_game), chosen(Realm::Kambara, UiRealmSource::CustomGame));
    let campaign = UiRealmInputs { campaign_realm: Some(Realm::Tampadesa), ..custom_game };
    assert_eq!(choose_ui_realm(&campaign), chosen(Realm::Tampadesa, UiRealmSource::Campaign));

    // In the menus: the profile's; the campaign's doesn't count.
    let menus = UiRealmInputs { in_home_menu: true, profile_realm: Some(Realm::Kambara), campaign_realm: Some(Realm::Tampadesa), ..Default::default() };
    assert_eq!(choose_ui_realm(&menus), chosen(Realm::Kambara, UiRealmSource::Profile));

    // The override wins over both.
    for inputs in [menus, campaign] {
        let overridden = UiRealmInputs { override_realm: Some(Realm::DaiLong), ..inputs };
        assert_eq!(choose_ui_realm(&overridden), chosen(Realm::DaiLong, UiRealmSource::Override));
    }
}

fn test_reaching_campaign_two_switches_home_realm() {
    let configs = ensure_setup();

    // A fresh profile has reached nothing: the menus are Đại Long's by default.
    assert_eq!(profile::get().latest_reached_realm(configs), None);
    assert_eq!(realm::ui_realm_for(true), UiRealm { realm: Realm::DaiLong, source: UiRealmSource::Default });

    // Playing campaign one reaches it.
    profile::on_mission_started(0, 0);
    assert_eq!(realm::ui_realm_for(true), UiRealm { realm: Realm::DaiLong, source: UiRealmSource::Profile });

    // Finishing its last mission reaches campaign two, and the menus take Kambara's look.
    profile::on_mission_finished(0, 0);
    assert_eq!(realm::ui_realm_for(true).realm, Realm::DaiLong);
    profile::on_mission_finished(0, 1);
    assert!(profile::get().is_reached("two"));
    assert_eq!(realm::ui_realm_for(true), UiRealm { realm: Realm::Kambara, source: UiRealmSource::Profile });

    // The latest reached in campaigns.json order counts, whatever order the records are in.
    let mut reordered = profile::get().clone();
    reordered.campaigns.reverse();
    reordered.mark_reached("three");
    profile::replace(reordered);
    assert_eq!(realm::ui_realm_for(true).realm, Realm::Tampadesa);

    // In game, outside a campaign, the profile doesn't count.
    assert_eq!(realm::ui_realm_for(false), UiRealm { realm: Realm::DaiLong, source: UiRealmSource::Default });
}

fn test_in_game_realm_follows_active_campaign() {
    ensure_setup();

    assert!(campaign::start_campaign(1).is_some());
    assert_eq!(realm::active_campaign_realm(), Some(Realm::Kambara));
    assert_eq!(realm::ui_realm_for(false), UiRealm { realm: Realm::Kambara, source: UiRealmSource::Campaign });

    // The next mission is the same campaign's.
    assert!(campaign::advance_to_next_mission().is_some());
    assert_eq!(realm::ui_realm_for(false).realm, Realm::Kambara);

    campaign::reset();
    assert!(campaign::start_campaign(2).is_some());
    assert_eq!(realm::ui_realm_for(false).realm, Realm::Tampadesa);

    // Quitting the campaign: Đại Long again.
    campaign::reset();
    assert_eq!(realm::ui_realm_for(false), UiRealm { realm: Realm::DaiLong, source: UiRealmSource::Default });
}

fn test_override_wins() {
    ensure_setup();
    profile::on_mission_finished(0, 1); // Campaign two reached: Kambara in the menus.
    campaign::start_campaign(2); // Tampadesa in game.

    realm::set_override(Some(Realm::DaiLong));
    for in_home_menu in [true, false] {
        assert_eq!(realm::ui_realm_for(in_home_menu), UiRealm { realm: Realm::DaiLong, source: UiRealmSource::Override });
    }

    // Cleared: each follows the game again.
    realm::set_override(None);
    assert_eq!(realm::override_realm(), None);
    assert_eq!(realm::ui_realm_for(true), UiRealm { realm: Realm::Kambara, source: UiRealmSource::Profile });
    assert_eq!(realm::ui_realm_for(false), UiRealm { realm: Realm::Tampadesa, source: UiRealmSource::Campaign });
    campaign::reset();
}

// The minimap's style is the UI realm's: it loads another realm's when that changes.
fn test_minimap_style_follows_ui_realm() {
    ensure_setup();

    // No game loop here: ui_realm() takes the in-game rule.
    assert_eq!(realm::ui_realm(), Realm::DaiLong);
    style::current();
    assert_eq!(style::current_realm(), Realm::DaiLong);

    let version = style::version();
    realm::set_override(Some(Realm::Kambara));
    let kambara_buttons = style::current().buttons.clone();
    assert_eq!(style::current_realm(), Realm::Kambara);
    assert!(style::version() > version, "a realm change makes the minimap rebuild");

    // The active campaign's realm, once the override is cleared.
    realm::set_override(None);
    campaign::start_campaign(2);
    style::current();
    assert_eq!(style::current_realm(), Realm::Tampadesa);
    assert_ne!(style::current().buttons, kambara_buttons);

    // No change, no reload.
    let version = style::version();
    style::current();
    assert_eq!(style::version(), version);

    campaign::reset();
    style::current();
    assert_eq!(style::current_realm(), Realm::DaiLong);
}

// The tests changed the profile of the run through its hooks, but the harness never loaded it:
// nothing was written to saves/user/profile.json.
fn test_profile_kept_in_memory() {
    assert!(!profile::writes_file());
}
