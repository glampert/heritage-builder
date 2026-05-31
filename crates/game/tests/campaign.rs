use game::{
    campaign::{
        self,
        CampaignProgress,
        CampaignPrompt,
        config::{CampaignConfigs, CampaignDef, MissionDef, MissionGoal, MissionMap, MissionRequirements},
    },
    debug::preset_maps,
    sim::resources::ResourceKind,
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
        test_utils::test_fn!(test_tick_detects_mission_completion),
        test_utils::test_fn!(test_advance_to_next_mission),
        test_utils::test_fn!(test_continue_playing_no_requeue),
        test_utils::test_fn!(test_snapshot_restore_and_suppress),
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
            name: "Test Campaign".into(),
            missions: vec![
                MissionDef {
                    name: "Mission 1".into(),
                    description: String::new(),
                    map: MissionMap::Preset { preset_number: TEST_PRESET },
                    requirements: MissionRequirements { goals: vec![MissionGoal::Population { min: 10 }] },
                },
                MissionDef {
                    name: "Mission 2".into(),
                    description: String::new(),
                    map: MissionMap::Preset { preset_number: TEST_PRESET },
                    requirements: MissionRequirements { goals: vec![MissionGoal::Treasury { min_gold: 100 }] },
                },
            ],
        }],
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

    // At threshold: mission completes and a MissionComplete prompt is raised.
    env.world.stats_mut().population.total = 10;
    {
        let context = env.new_sim_context(0.0);
        campaign::tick(&context);
    }
    assert!(campaign::active_mission().unwrap().completed);
    assert_eq!(campaign::take_pending_prompt(), Some(CampaignPrompt::MissionComplete));
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

    campaign::restore_snapshot(snapshot);
    assert_eq!(campaign::active_mission().unwrap().mission_index, 0);

    // After start_campaign sets the suppress flag (campaign-driven load in
    // flight), the next restore is a no-op so the intended mission survives.
    campaign::reset();
    campaign::start_campaign(0);
    campaign::restore_snapshot(CampaignProgress::default()); // suppressed -> ignored
    assert!(campaign::active_mission().is_some());

    // The suppress flag is one-shot: a subsequent restore now applies.
    campaign::restore_snapshot(CampaignProgress::default());
    assert!(campaign::active_mission().is_none());
}
