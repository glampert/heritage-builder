use game::{
    campaign::{
        self,
        ActiveMission,
        CampaignProgress,
        CampaignPrompt,
        config::{
            CampaignConfigs,
            CampaignDef,
            ChoiceSite,
            LabelSide,
            MissionCalendarDef,
            MissionChoiceDef,
            MissionDef,
            MissionDistinctionDef,
            MissionGoal,
            MissionMap,
            MissionRef,
            MissionRequirements,
        },
    },
    config::GameConfigs,
    debug::preset_maps,
    profile::{self, Profile},
    realm::Realm,
    sim::{calendar::EraDef, resources::ResourceKind},
    world::stats::WorldStats,
};

mod test_utils;
use test_utils::TestEnvironment;

// ----------------------------------------------
// Integration tests for the campaign system
// ----------------------------------------------

fn main() {
    test_utils::run_tests("Campaign", &[
        // Pure requirement evaluation:
        test_utils::test_fn!(test_mission_goal_is_met),
        test_utils::test_fn!(test_mission_goal_resource),
        test_utils::test_fn!(test_requirements_all_met),
        // Manager state machine:
        test_utils::test_fn!(test_start_campaign_sets_active_mission),
        test_utils::test_fn!(test_start_mission_resumes_a_campaign),
        test_utils::test_fn!(test_tick_detects_mission_completion),
        test_utils::test_fn!(test_advance_to_next_mission),
        test_utils::test_fn!(test_advance_through_a_choice),
        test_utils::test_fn!(test_continue_playing_no_requeue),
        test_utils::test_fn!(test_snapshot_restore_and_suppress),
        test_utils::test_fn!(test_cancelled_mission_start),
        test_utils::test_fn!(test_briefing_request),
        test_utils::test_fn!(test_mission_calendar),
        test_utils::test_fn!(test_mission_preview),
        // The stele:
        test_utils::test_fn!(test_stele_lines),
        test_utils::test_fn!(test_stele_validation),
    ]);
}

// Preset used to build a SimContext for the manager `tick` tests. The map
// contents are irrelevant here -- the tests drive completion by writing world
// stats directly.
const TEST_PRESET: usize = preset_maps::PRESET_1_FARM_1_GRANARY_1_HOUSE_2_WELLS_1_MARKET;

// A self-contained 2-mission campaign, injected into the CampaignConfigs
// singleton so the tests don't depend on the shipped campaigns.json.
fn build_test_configs() -> CampaignConfigs {
    CampaignConfigs {
        campaigns: vec![CampaignDef {
            id: "test_campaign".into(),
            name: "Test Campaign".into(),
            realm: Realm::DaiLong,
            missions: vec![
                MissionDef {
                    id: String::new(),
                    name: "Mission 1".into(),
                    description: String::new(),
                    map: MissionMap::Preset { preset_number: TEST_PRESET },
                    requirements: MissionRequirements { goals: vec![MissionGoal::Population { min: 10 }] },
                    calendar: Some(MissionCalendarDef { start_year: Some(5), start_month: Some(3), wet_months: None }),
                    finance: None,
                    risks: None,
                    city: None,
                    choices: Vec::new(),
                    choice_text: None,
                    stele_line: None,
                    lead_in: None,
                    briefing: None,
                    briefing_new: Vec::new(),
                    briefing_threats: None,
                    distinctions: Vec::new(),
                },
                MissionDef {
                    id: String::new(),
                    name: "Mission 2".into(),
                    description: String::new(),
                    map: MissionMap::Preset { preset_number: TEST_PRESET },
                    requirements: MissionRequirements { goals: vec![MissionGoal::Treasury { min_gold: 100 }] },
                    calendar: None,
                    finance: None,
                    risks: None,
                    city: None,
                    choices: Vec::new(),
                    choice_text: None,
                    stele_line: None,
                    lead_in: None,
                    briefing: None,
                    briefing_new: Vec::new(),
                    briefing_threats: None,
                    distinctions: Vec::new(),
                },
            ],
            eras: vec![EraDef { name: "Test Era".into(), start_year: 4 }],
            hidden: false,
            world_map: None,
            stele_title: None,
            stele_closing: None,
        }, build_choice_campaign()],
    }
}

// The second test campaign (CHOICE_CAMPAIGN): four missions, the second a choice point whose site
// A starts it and site B the third, its variant. Its steps are 0, 1 and 3.
const CHOICE_CAMPAIGN: usize = 1;

fn build_choice_campaign() -> CampaignDef {
    let mission = |name: &str| MissionDef {
        id: String::new(),
        name: name.into(),
        description: String::new(),
        map: MissionMap::Preset { preset_number: TEST_PRESET },
        requirements: MissionRequirements::default(),
        calendar: None,
        finance: None,
        risks: None,
        city: None,
        choices: Vec::new(),
        choice_text: None,
        stele_line: None,
        lead_in: None,
        briefing: None,
        briefing_new: Vec::new(),
        briefing_threats: None,
        distinctions: Vec::new(),
    };
    let site = |site, mission| MissionChoiceDef { site, name: "city.hoi_thuy".into(), phrase: None, position: [0.0, 0.0], label: LabelSide::Below, mission };
    let mut missions = vec![mission("First"), mission("Choice"), mission("Variant"), mission("Last")];
    missions[1].choices = vec![site(ChoiceSite::A, MissionRef::Index(1)), site(ChoiceSite::B, MissionRef::Index(2))];
    CampaignDef {
        id: "choice_campaign".into(),
        name: "Choice Campaign".into(),
        realm: Realm::DaiLong,
        missions,
        eras: Vec::new(),
        hidden: false,
        world_map: None,
        stele_title: None,
        stele_closing: None,
    }
}

// Idempotent per-test setup: install the test campaign config + manager once,
// and reset the manager so each test starts from a clean campaign state.
fn ensure_campaign_setup() {
    if !CampaignConfigs::is_initialized() {
        CampaignConfigs::initialize(build_test_configs());
    }
    campaign::initialize(); // No-op if already initialized.
    campaign::reset();
    campaign::take_finished_missions(); // A reset keeps them queued: drop an earlier test's.
}

// ----------------------------------------------
// Pure requirement evaluation
// ----------------------------------------------

fn test_mission_goal_is_met() {
    let mut stats = WorldStats::default();
    stats.population.total = 50;
    stats.population.employed = 30;
    stats.treasury.gold_units_total = 1000;

    assert!(MissionGoal::Population  { min: 50 }.is_met(&stats));
    assert!(!MissionGoal::Population { min: 51 }.is_met(&stats));

    assert!(MissionGoal::Employment  { min_employed: 30 }.is_met(&stats));
    assert!(!MissionGoal::Employment { min_employed: 31 }.is_met(&stats));

    assert!(MissionGoal::Treasury  { min_gold: 1000 }.is_met(&stats));
    assert!(!MissionGoal::Treasury { min_gold: 1001 }.is_met(&stats));
}

fn test_mission_goal_resource() {
    let mut stats = WorldStats::default();
    stats.resources.all.add(ResourceKind::Rice, 20);

    assert!(MissionGoal::Resource  { kind: ResourceKind::Rice, min: 20 }.is_met(&stats));
    assert!(!MissionGoal::Resource { kind: ResourceKind::Rice, min: 21 }.is_met(&stats));

    // A different resource the world has none of.
    assert!(!MissionGoal::Resource { kind: ResourceKind::Wood, min: 1 }.is_met(&stats));
}

fn test_requirements_all_met() {
    let mut stats = WorldStats::default();
    stats.population.total = 50;
    stats.treasury.gold_units_total = 500;

    let all = MissionRequirements {
        goals: vec![MissionGoal::Population { min: 50 }, MissionGoal::Treasury { min_gold: 500 }],
    };
    assert!(all.all_met(&stats));

    let partial = MissionRequirements {
        goals: vec![MissionGoal::Population { min: 50 }, MissionGoal::Treasury { min_gold: 501 }],
    };
    assert!(!partial.all_met(&stats));

    // No goals => already met.
    assert!(MissionRequirements::default().all_met(&stats));
}

// ----------------------------------------------
// Manager state machine
// ----------------------------------------------

fn test_start_campaign_sets_active_mission() {
    ensure_campaign_setup();

    let map = campaign::start_campaign(0).expect("campaign 0 should exist");
    assert!(matches!(map, MissionMap::Preset { .. }));

    let active = campaign::active_mission().expect("active mission should be set");
    assert_eq!(active.campaign_id, 0);
    assert_eq!(active.mission_index, 0);
    assert!(!active.completed);
    assert!(!campaign::has_pending_prompt());

    // An invalid campaign id starts nothing.
    campaign::reset();
    assert!(campaign::start_campaign(999).is_none());
    assert!(campaign::active_mission().is_none());
}

// The home menu's Campaign resumes a campaign at its next mission: the map of that mission, made
// the active one, and the next session is its start.
fn test_start_mission_resumes_a_campaign() {
    ensure_campaign_setup();

    let map = campaign::start_mission(0, 1).expect("campaign 0 has a mission 1");
    assert!(matches!(map, MissionMap::Preset { .. }));
    let active = campaign::active_mission().expect("active mission should be set");
    assert_eq!((active.campaign_id, active.mission_index, active.completed), (0, 1, false));
    assert!(campaign::take_mission_start());

    // A mission past the campaign's last starts nothing and leaves the active one.
    assert!(campaign::start_mission(0, 2).is_none());
    assert_eq!(campaign::active_mission().map(|active| active.mission_index), Some(1));
    campaign::reset();
}

fn test_tick_detects_mission_completion() {
    ensure_campaign_setup();
    campaign::start_campaign(0); // Mission 1 goal: Population >= 10.

    let mut env = TestEnvironment::with_preset_map(TEST_PRESET);

    // Below threshold: no completion, no prompt.
    env.world.stats_mut().population.total = 5;
    {
        let context = env.new_sim_context(0.0);
        campaign::tick(&context);
    }
    assert!(!campaign::has_pending_prompt());
    assert!(!campaign::active_mission().unwrap().completed);
    assert!(campaign::take_finished_missions().is_empty());

    // At threshold: mission completes and a MissionComplete prompt is raised.
    env.world.stats_mut().population.total = 10;
    {
        let context = env.new_sim_context(0.0);
        campaign::tick(&context);
    }
    assert!(campaign::active_mission().unwrap().completed);
    assert_eq!(campaign::take_pending_prompt(), Some(CampaignPrompt::MissionComplete));

    // The finished mission is queued once for the player's profile (GameLoop takes it).
    assert_eq!(campaign::take_finished_missions(), vec![(0, 0)]);
    {
        let context = env.new_sim_context(0.0);
        campaign::tick(&context);
    }
    assert!(campaign::take_finished_missions().is_empty());

    // A reset in the frame a mission finishes (an automation command) keeps it queued for the
    // profile.
    campaign::reset();
    campaign::start_campaign(0);
    {
        let context = env.new_sim_context(0.0);
        campaign::tick(&context);
    }
    campaign::reset();
    assert!(campaign::active_mission().is_none());
    assert_eq!(campaign::take_finished_missions(), vec![(0, 0)]);
}

fn test_advance_to_next_mission() {
    ensure_campaign_setup();
    campaign::start_campaign(0);

    // Mission 1 -> Mission 2.
    let map = campaign::advance_to_next_mission().expect("mission 2 should exist");
    assert!(matches!(map, MissionMap::Preset { .. }));

    let active = campaign::active_mission().unwrap();
    assert_eq!(active.mission_index, 1);
    assert!(!active.completed);
    assert!(!campaign::has_pending_prompt());

    // Past the last mission: no map, campaign-complete prompt raised.
    assert!(campaign::advance_to_next_mission().is_none());
    assert_eq!(campaign::take_pending_prompt(), Some(CampaignPrompt::CampaignComplete));
}

// Advancing to a choice point starts the site picked there (the profile's choice), else the choice
// point itself; advancing from either of its sites' missions skips the other, to the step after.
fn test_advance_through_a_choice() {
    ensure_campaign_setup();
    profile::reset();
    let advance = || campaign::advance_to_next_mission().map(|_| campaign::active_mission().unwrap().mission_index);
    // The choices check out (the test campaigns have no stele text: test_stele_validation).
    let errors: Vec<String> = CampaignConfigs::get()
        .validate()
        .into_iter()
        .filter(|error| {
            !["stele_title", "stele_closing", "stele_line", "lead_in", "briefing"].iter().any(|field| error.contains(field))
        })
        .collect();
    assert!(errors.is_empty(), "{errors:?}");

    campaign::start_mission(CHOICE_CAMPAIGN, 0);
    assert_eq!(advance(), Some(1), "no site picked: the choice point");

    profile::on_choice_made(CHOICE_CAMPAIGN, 1, ChoiceSite::B);
    campaign::start_mission(CHOICE_CAMPAIGN, 0);
    assert_eq!(advance(), Some(2), "site B picked: its variant");
    assert_eq!(advance(), Some(3), "the variant: on to the step after its choice point");

    campaign::start_mission(CHOICE_CAMPAIGN, 1);
    assert_eq!(advance(), Some(3), "the choice point: past its variant");
    assert_eq!(advance(), None, "past the last step");
    assert_eq!(campaign::take_pending_prompt(), Some(CampaignPrompt::CampaignComplete));

    profile::reset();
    campaign::reset();
}

fn test_continue_playing_no_requeue() {
    ensure_campaign_setup();
    campaign::start_campaign(0);

    let mut env = TestEnvironment::with_preset_map(TEST_PRESET);
    env.world.stats_mut().population.total = 10;
    {
        let context = env.new_sim_context(0.0);
        campaign::tick(&context);
    }
    assert!(campaign::has_pending_prompt());

    // Choosing to keep playing clears the prompt; the mission stays completed.
    campaign::continue_playing();
    assert!(!campaign::has_pending_prompt());
    assert!(campaign::active_mission().unwrap().completed);

    // Ticking again must not re-raise the prompt.
    {
        let context = env.new_sim_context(0.0);
        campaign::tick(&context);
    }
    assert!(!campaign::has_pending_prompt());
}

fn test_snapshot_restore_and_suppress() {
    ensure_campaign_setup();
    campaign::start_campaign(0);

    let snapshot = campaign::capture_snapshot();
    assert!(snapshot.active.is_some());

    // A normal (unsuppressed) restore overwrites the manager's progress.
    campaign::reset();
    assert!(campaign::active_mission().is_none());

    assert!(!campaign::restore_snapshot(snapshot), "an unsuppressed restore isn't a mission start");
    assert_eq!(campaign::active_mission().unwrap().mission_index, 0);

    // After start_campaign sets the suppress flag (campaign-driven load in
    // flight), the next restore is a no-op so the intended mission survives.
    campaign::reset();
    campaign::start_campaign(0);
    assert!(campaign::restore_snapshot(CampaignProgress::default()), "suppressed -> ignored, and starts the mission");
    assert!(campaign::active_mission().is_some());

    // The suppress flag is one-shot: a subsequent restore now applies.
    assert!(!campaign::restore_snapshot(CampaignProgress::default()));
    assert!(campaign::active_mission().is_none());
}

// A mission whose map fails to load doesn't start: the campaign goes back to what it was, and the
// next load (the player's own save) restores that save's progress instead of starting the mission.
fn test_cancelled_mission_start() {
    ensure_campaign_setup();

    // From the home menu: nothing active before the start.
    campaign::reset();
    campaign::start_mission(0, 1);
    campaign::cancel_mission_start(); // GameSession::load_save_game failed.
    assert!(campaign::active_mission().is_none(), "the failed start leaves no active mission");

    let saved = CampaignProgress { active: Some(ActiveMission { campaign_id: 0, mission_index: 0, completed: true }) };
    assert!(!campaign::restore_snapshot(saved), "the next load isn't the mission's start");
    let active = campaign::active_mission().expect("the save's progress is restored");
    assert_eq!((active.campaign_id, active.mission_index, active.completed), (0, 0, true));

    // In game: the next mission's map fails, and the mission being played stays active.
    assert!(campaign::advance_to_next_mission().is_some());
    campaign::cancel_mission_start();
    let active = campaign::active_mission().expect("the current mission stays active");
    assert_eq!((active.mission_index, active.completed), (0, true));

    // Without a start pending, it changes nothing.
    campaign::cancel_mission_start();
    assert_eq!(campaign::active_mission().map(|active| active.mission_index), Some(0));
    campaign::reset();
}

// The briefing is asked for once a mission's map has loaded (GameSession::start_mission), only
// while a mission is active, and taken once; any other session change drops the request: a save of
// a mission under way, a mission start that failed, the next start.
fn test_briefing_request() {
    ensure_campaign_setup();

    // No mission: no briefing.
    campaign::request_briefing();
    assert!(!campaign::is_briefing_pending(), "no briefing outside a mission");
    assert!(!campaign::take_pending_briefing());

    // A mission starts: its map loads (the restore it suppresses), then the briefing is asked for,
    // and taken once.
    assert!(campaign::start_mission(0, 0).is_some());
    assert!(!campaign::is_briefing_pending(), "asked for only once the map has loaded");
    assert!(campaign::restore_snapshot(CampaignProgress::default()), "the mission's own load");
    campaign::request_briefing();
    assert!(campaign::is_briefing_pending());
    assert!(campaign::take_pending_briefing(), "taken once");
    assert!(!campaign::take_pending_briefing(), "and only once");
    assert!(!campaign::is_briefing_pending());

    // A save of the mission under way loads without one.
    campaign::request_briefing();
    let saved = CampaignProgress { active: Some(ActiveMission { campaign_id: 0, mission_index: 0, completed: false }) };
    assert!(!campaign::restore_snapshot(saved));
    assert!(!campaign::is_briefing_pending(), "a save's load drops the request");

    // The next start drops a request still waiting, and so does a start whose map fails.
    campaign::request_briefing();
    assert!(campaign::start_mission(0, 1).is_some());
    assert!(!campaign::is_briefing_pending(), "a new start drops the request");
    campaign::request_briefing();
    campaign::cancel_mission_start();
    assert!(!campaign::is_briefing_pending(), "a failed start drops the request");

    // Advancing to the next mission drops it too.
    campaign::request_briefing();
    assert!(campaign::is_briefing_pending());
    assert!(campaign::advance_to_next_mission().is_some());
    assert!(!campaign::is_briefing_pending(), "advancing drops the request");

    // A reset (quit to the menu, another game) drops it.
    campaign::take_mission_start();
    campaign::request_briefing();
    campaign::reset();
    assert!(!campaign::is_briefing_pending(), "a reset drops the request");
}

// A preview (the developer editor's Screens menu) shows its mission in place of the one being
// played while it's set, and carves its stele from a copy of the profile played up to it.
fn test_mission_preview() {
    ensure_campaign_setup();
    assert!(campaign::start_mission(0, 1).is_some());
    let shown = |preview: Option<campaign::preview::MissionPreview>| {
        campaign::preview::set(preview);
        let mission = campaign::preview::shown_mission().map(|mission| (mission.campaign_id, mission.mission_index, mission.completed));
        campaign::preview::set(None);
        mission
    };
    assert_eq!(shown(None), Some((0, 1, false)), "the mission being played without one");
    let preview = campaign::preview::MissionPreview { campaign_index: 0, mission_index: 0, completed: true };
    assert_eq!(shown(Some(preview)), Some((0, 0, true)));
    assert!(!campaign::preview::is_active(), "cleared");
    assert_eq!(campaign::active_mission().map(|mission| mission.mission_index), Some(1), "the mission being played is left alone");

    // Played up to a mission: every step before it finished (at its first mission), then it.
    let configs = CampaignConfigs { campaigns: vec![build_choice_campaign()] };
    let finished_in = |profile: &Profile, id: &str| profile.record(id).map(|record| record.missions_finished.clone()).unwrap_or_default();
    let finished = |profile: &Profile| finished_in(profile, "choice_campaign");
    let empty = Profile::default();
    assert_eq!(finished(&campaign::preview::profile_through(&empty, &configs, 0, 0)), vec![0]);
    assert_eq!(finished(&campaign::preview::profile_through(&empty, &configs, 0, 2)), vec![0, 2], "a variant: its choice point isn't finished");
    assert_eq!(finished(&campaign::preview::profile_through(&empty, &configs, 0, 3)), vec![0, 1, 3]);
    assert!(finished(&empty).is_empty(), "the profile itself is left alone");
    // A step the profile finished through its variant stays so.
    let mut played = Profile::default();
    played.mission_finished(&configs, 0, 2);
    assert_eq!(finished(&campaign::preview::profile_through(&played, &configs, 0, 3)), vec![0, 2, 3]);
    // Steps the profile finished after the mission are dropped: the campaign as played up to it.
    let mut ahead = Profile::default();
    for mission in [0, 1, 3] {
        ahead.mission_finished(&configs, 0, mission);
    }
    assert!(ahead.record("choice_campaign").unwrap().finished);
    let through_first = campaign::preview::profile_through(&ahead, &configs, 0, 0);
    assert_eq!(finished(&through_first), vec![0], "the later steps are left out");
    assert!(!through_first.record("choice_campaign").unwrap().finished, "nor is the campaign finished");
    assert_eq!(finished(&campaign::preview::profile_through(&ahead, &configs, 0, 2)), vec![0, 2], "its other site too");
    assert!(campaign::preview::profile_through(&ahead, &configs, 0, 3).record("choice_campaign").unwrap().finished);
    // A preview's screens read its profile: the mission finished only if it shows so.
    assert!(std::ptr::eq(campaign::preview::shown_profile(), profile::get()), "the player's without a preview");
    let first_id = CampaignConfigs::get().campaigns[0].id.clone();
    let shown_finished = |completed: bool| {
        campaign::preview::set(Some(campaign::preview::MissionPreview { campaign_index: 0, mission_index: 0, completed }));
        let missions = finished_in(campaign::preview::shown_profile(), &first_id);
        campaign::preview::set(None);
        missions
    };
    assert_eq!(shown_finished(true), vec![0]);
    assert!(shown_finished(false).is_empty());
    assert!(std::ptr::eq(campaign::preview::shown_profile(), profile::get()), "cleared");
    campaign::reset();
}

// A mission starts with its own calendar over the configs' defaults, and its campaign's eras.
fn test_mission_calendar() {
    ensure_campaign_setup();
    assert!(campaign::active_mission_calendar().is_none(), "no calendar outside a campaign");

    let defaults = &GameConfigs::get().sim.calendar;

    campaign::start_campaign(0);
    assert!(campaign::take_mission_start(), "start_campaign makes the next session the mission's start");
    assert!(!campaign::take_mission_start(), "taken once");

    let first = campaign::active_mission_calendar().unwrap();
    assert_eq!((first.start_year, first.start_month), (5, 3));
    assert_eq!(first.wet_months, defaults.wet_months);
    assert_eq!(first.eras, vec![EraDef { name: "Test Era".into(), start_year: 4 }]);

    // Mission 2 has no calendar of its own: the defaults, with the campaign's eras.
    campaign::advance_to_next_mission();
    let second = campaign::active_mission_calendar().unwrap();
    assert_eq!((second.start_year, second.start_month), (defaults.start_year, defaults.start_month));
    assert_eq!(second.eras.len(), 1);
}

// ----------------------------------------------
// The stele
// ----------------------------------------------

// The stele carves a line per finished step, in step order: the mission's line, a variant's own
// or else its choice point's, then a line per distinction earned; the newest is the mission asked
// for, else the last; an empty row stands for each step to come.
fn test_stele_lines() {
    let mut campaign = build_choice_campaign();
    campaign.stele_title = Some("stele.dragon_legacy.title".into());
    campaign.missions[0].stele_line = Some("mission.dragon_legacy.1.stele".into());
    campaign.missions[1].stele_line = Some("mission.dragon_legacy.2.stele".into());
    campaign.missions[3].stele_line = Some("mission.dragon_legacy.3.stele".into());
    campaign.missions[0].distinctions =
        vec![MissionDistinctionDef { id: "full_granary".into(), stele_line: "mission.test_choices.1.stele".into() }];
    let configs = CampaignConfigs { campaigns: vec![campaign] };
    let line = |key: &str| engine::ui::text::get(engine::ui::text::UiTextCategory::Story, key).to_string();

    // Nothing finished: the title and an empty row per step.
    let mut profile = Profile::new();
    let stele = campaign::stele::stele(&configs, &profile, 0, None).unwrap();
    assert_eq!(stele.title, "The Deeds of the Governor of the South");
    assert!(stele.lines.is_empty());
    assert_eq!(stele.rows_to_come, 3);

    // The first mission: its line, freshly carved.
    profile.mission_finished(&configs, 0, 0);
    let stele = campaign::stele::stele(&configs, &profile, 0, None).unwrap();
    assert_eq!(stele.lines.len(), 1);
    assert_eq!(stele.lines[0].text, line("mission.dragon_legacy.1.stele"));
    assert!(stele.lines[0].text.starts_with("In the first year"), "{}", stele.lines[0].text);
    assert!(stele.lines[0].newest);
    assert_eq!(stele.rows_to_come, 2);

    // The choice point's variant (site B), with no line of its own: its choice point's.
    profile.choice_made(&configs, 0, 1, ChoiceSite::B);
    profile.mission_finished(&configs, 0, 2);
    let stele = campaign::stele::stele(&configs, &profile, 0, Some(0)).unwrap();
    let missions: Vec<usize> = stele.lines.iter().map(|line| line.mission_index).collect();
    assert_eq!(missions, [0, 2]);
    assert_eq!(stele.lines[1].text, line("mission.dragon_legacy.2.stele"));
    assert_eq!(stele.lines[1].mission_number, 2);
    assert_eq!(
        stele.lines.iter().map(|line| line.newest).collect::<Vec<_>>(),
        [true, false],
        "newest: the mission asked for"
    );
    let stele = campaign::stele::stele(&configs, &profile, 0, Some(3)).unwrap();
    assert!(stele.lines[1].newest, "a mission with no line yet marks the last");

    // A distinction earned adds its line after its mission's, newest with it.
    profile.record_mut("choice_campaign").distinctions.push("full_granary".into());
    let stele = campaign::stele::stele(&configs, &profile, 0, Some(0)).unwrap();
    assert_eq!(stele.lines.len(), 3);
    assert_eq!(stele.lines[1].distinction.as_deref(), Some("full_granary"));
    assert!(stele.lines[0].newest && stele.lines[1].newest && !stele.lines[2].newest);

    // The last step: the campaign's stele is whole, no row to come.
    profile.mission_finished(&configs, 0, 3);
    let stele = campaign::stele::stele(&configs, &profile, 0, None).unwrap();
    assert_eq!(stele.rows_to_come, 0);
    assert!(stele.lines.last().unwrap().newest);

    // Carved in full: every step at its first mission, no distinctions, the last newest.
    let full = campaign::stele::carved_in_full(&configs, 0).unwrap();
    assert_eq!(full.lines.iter().map(|line| line.mission_index).collect::<Vec<_>>(), [0, 1, 3]);
    assert_eq!(full.rows_to_come, 0);
    assert!(full.lines[2].newest);
    assert!(campaign::stele::stele(&configs, &profile, 5, None).is_none());
}

// A campaign with missions needs its stele title and closing line, each step its stele line,
// lead-in and briefing (a variant may leave its line out), and distinction ids are unique.
fn test_stele_validation() {
    let mut campaign = build_choice_campaign();
    let errors = CampaignConfigs { campaigns: vec![campaign.clone_for_test()] }.validate();
    assert!(errors.iter().any(|error| error.contains("no stele_title")), "{errors:?}");
    assert!(errors.iter().any(|error| error.contains("no stele_closing")), "{errors:?}");
    assert!(errors.iter().any(|error| error.contains("mission 0: no lead_in")), "{errors:?}");
    assert!(
        !errors.iter().any(|error| error.contains("mission 2: no stele_line")),
        "a variant may take its choice point's line: {errors:?}"
    );

    campaign.stele_title = Some("stele.dragon_legacy.title".into());
    campaign.stele_closing = Some("stele.dragon_legacy.closing".into());
    for mission in &mut campaign.missions {
        mission.stele_line = Some("mission.dragon_legacy.1.stele".into());
        mission.lead_in = Some("mission.dragon_legacy.1.lead_in".into());
        mission.briefing = Some("mission.dragon_legacy.1.briefing".into());
    }
    assert_eq!(CampaignConfigs { campaigns: vec![campaign.clone_for_test()] }.validate(), Vec::<String>::new());
    let twice = MissionDistinctionDef { id: "twice".into(), stele_line: "x".into() };
    campaign.missions[0].distinctions = vec![twice.clone()];
    campaign.missions[3].distinctions = vec![twice];
    let errors = CampaignConfigs { campaigns: vec![campaign] }.validate();
    assert!(errors.iter().any(|error| error.contains("two distinctions have the id 'twice'")), "{errors:?}");
}

trait CloneForTest {
    fn clone_for_test(&self) -> Self;
}

// CampaignDef isn't Clone: a copy through JSON.
impl CloneForTest for CampaignDef {
    fn clone_for_test(&self) -> Self {
        serde_json::from_value(serde_json::to_value(self).unwrap()).unwrap()
    }
}
