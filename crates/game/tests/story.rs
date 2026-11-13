use game::{
    campaign::config::{CampaignConfigs, MissionRef},
    profile::Profile,
    story::{BubbleKind, PANEL_SIZE, StoryConfigs, StoryDef, StoryTrigger, TriggerPoint, config::resolve_trigger},
};

mod test_utils;

// ----------------------------------------------
// Integration tests for the story comics' data (docs/plans/realm-ui.md, Phase 4: Data): the
// shipped assets/configs/story/*.json, their validation, what plays before and after a mission,
// and the sequences a profile has seen.
// ----------------------------------------------

fn main() {
    test_utils::run_tests("Story", &[
        test_utils::test_fn!(test_story_files_load_and_check_out),
        test_utils::test_fn!(test_campaign_one_sequences),
        test_utils::test_fn!(test_test_campaigns_sequences),
        test_utils::test_fn!(test_triggers_resolve),
        test_utils::test_fn!(test_validation_finds_errors),
        test_utils::test_fn!(test_seen_sequences_and_replay),
    ]);
}

fn configs() -> (&'static CampaignConfigs, &'static StoryConfigs) {
    if !CampaignConfigs::is_initialized() {
        CampaignConfigs::load();
    }
    if !StoryConfigs::is_initialized() {
        StoryConfigs::load();
    }
    (CampaignConfigs::get(), StoryConfigs::get())
}

fn index_of(id: &str) -> usize {
    configs().0.find_by_id(id).unwrap_or_else(|| panic!("campaigns.json should have the campaign '{id}'"))
}

// A campaign of four missions whose second is a choice point (site A starts it, site B the third,
// its variant), with no files behind it.
fn synthetic_campaigns() -> CampaignConfigs {
    let mission = |name: &str| serde_json::json!({ "name": name, "map": { "Preset": { "preset_number": 0 } } });
    let mut choice_point = mission("two");
    choice_point["choices"] = serde_json::json!([
        { "site": "A", "name": "city.a", "position": [0, 0], "mission": 1 },
        { "site": "B", "name": "city.b", "position": [0, 0], "mission": "two_b" }
    ]);
    let mut variant = mission("two b");
    variant["id"] = serde_json::json!("two_b");
    serde_json::from_value(serde_json::json!({ "campaigns": [{
        "id": "one",
        "name": "One",
        "missions": [mission("one"), choice_point, variant, mission("three")]
    }]}))
    .expect("the synthetic campaigns should parse")
}

fn story(json: serde_json::Value) -> StoryDef {
    serde_json::from_value(json).expect("the synthetic story should parse")
}

fn sequence(id: &str, trigger: serde_json::Value) -> serde_json::Value {
    serde_json::json!({ "id": id, "title": "story.title", "trigger": trigger, "panels": [{ "image": "story/x.jpg" }] })
}

fn stories_for(campaigns: &CampaignConfigs, json: serde_json::Value) -> StoryConfigs {
    let mut stories = StoryConfigs::default();
    stories.stories.insert(campaigns.campaigns[0].id.clone(), story(json));
    stories
}

// Every story file loads and checks out: its ids, triggers and placements, its keys in en.json,
// its pictures under assets/ui/.
fn test_story_files_load_and_check_out() {
    let (campaigns, stories) = configs();
    assert_eq!(stories.validate(campaigns), Vec::<String>::new());
    assert_eq!(stories.missing_text_keys(), Vec::<String>::new());
    assert_eq!(stories.missing_images(), Vec::<String>::new());
    for id in ["dragon_legacy", "test_choices", "test_choices_kambara", "test_choices_tampadesa"] {
        assert!(stories.stories.contains_key(id), "campaign '{id}' should have a story file");
    }
    // Campaigns II and III have no missions yet, and no story.
    assert!(!stories.stories.contains_key("temple_mountain"));
    assert!(!stories.stories.contains_key("ten_thousand_temples"));
    // Only I-B waits for its mission (Campaign I's fifth).
    let pending: Vec<(String, String)> =
        stories.pending(campaigns).into_iter().map(|(campaign, sequence, _)| (campaign, sequence)).collect();
    assert_eq!(pending, vec![("dragon_legacy".to_string(), "i_b".to_string())]);
}

// I-A plays before mission 1 (6 panels, the second a sepia flashback) and I-B after mission 5 (4
// panels), as story-script.md has them.
fn test_campaign_one_sequences() {
    let (campaigns, stories) = configs();
    let campaign = index_of("dragon_legacy");
    let story = stories.for_campaign(campaigns, campaign).unwrap();

    let i_a = story.sequence("i_a").unwrap();
    assert_eq!(i_a.trigger, StoryTrigger::BeforeMission(MissionRef::Index(0)));
    assert_eq!(i_a.panels.len(), 6);
    assert_eq!(i_a.panels.iter().map(|panel| panel.sepia).collect::<Vec<_>>(), [false, true, false, false, false, false]);
    assert!(i_a.panels.iter().all(|panel| !panel.visual.is_empty() && panel.image.starts_with("story/dragon_legacy/i_a_")));
    assert_eq!(i_a.panels[5].bubbles[0].kind, BubbleKind::Bold, "panel 6: bold ink");
    assert_eq!(i_a.panels[0].bubbles[0].kind, BubbleKind::Whisper, "panel 1: the courtier whispers");

    let i_b = story.sequence("i_b").unwrap();
    assert_eq!(i_b.trigger, StoryTrigger::AfterMission(MissionRef::Index(4)), "after mission 5");
    assert_eq!(i_b.panels.len(), 4);
    assert_eq!(
        i_b.panels[1].bubbles.iter().map(|bubble| bubble.kind).collect::<Vec<_>>(),
        [BubbleKind::Group, BubbleKind::Speech]
    );
    assert_eq!(resolve_trigger(campaigns.campaign(campaign).unwrap(), &i_b.trigger), TriggerPoint::Pending);

    assert_eq!(stories.before_mission(campaigns, campaign, 0).map(|sequence| sequence.sequence.id.as_str()), Some("i_a"));
    for mission in 0..campaigns.campaign(campaign).unwrap().missions.len() {
        assert!(stories.after_mission(campaigns, campaign, mission).is_none(), "nothing plays after mission {mission} yet");
    }
    assert!(stories.before_mission(campaigns, campaign, 1).is_none());
    assert!(stories.epilogue(campaigns, campaign).is_none(), "I-E isn't data yet: Watch the epilogue stays hidden");
    // In play order, the pending I-B left out.
    let order: Vec<&str> =
        stories.sequences_in_order(campaigns, campaign).iter().map(|sequence| sequence.sequence.id.as_str()).collect();
    assert_eq!(order, ["i_a"]);
}

// Each hidden test campaign has a short sequence before its first mission and one after it, in its
// realm's placeholder panel; Đại Long's test campaign has an epilogue with a stele lines panel.
fn test_test_campaigns_sequences() {
    let (campaigns, stories) = configs();
    for (id, realm) in
        [("test_choices", "dailong"), ("test_choices_kambara", "kambara"), ("test_choices_tampadesa", "tampadesa")]
    {
        let campaign = index_of(id);
        let before = stories
            .before_mission(campaigns, campaign, 0)
            .unwrap_or_else(|| panic!("'{id}' plays a sequence before mission 1"));
        let after = stories
            .after_mission(campaigns, campaign, 0)
            .unwrap_or_else(|| panic!("'{id}' plays a sequence after mission 1"));
        assert_eq!((before.sequence.id.as_str(), after.sequence.id.as_str()), ("test_before", "test_after"));
        for sequence in [before, after] {
            assert!((2..=3).contains(&sequence.sequence.panels.len()), "'{id}': short sequences");
            assert!(sequence.sequence.panels.iter().all(|panel| panel.image == format!("story/placeholder_{realm}.jpg")));
        }
        assert!(before.sequence.panels.iter().any(|panel| panel.sepia), "'{id}': a sepia panel");
        assert_eq!(stories.sequences_in_order(campaigns, campaign)[0].sequence.id, "test_before");
    }
    let epilogue = stories.epilogue(campaigns, index_of("test_choices")).expect("test_choices has an epilogue");
    assert!(epilogue.sequence.panels.iter().any(|panel| panel.stele_lines), "its epilogue lists the stele lines");
    assert!(stories.epilogue(campaigns, index_of("test_choices_kambara")).is_none());
    // Every bubble kind is used by some test sequence.
    let kinds: Vec<BubbleKind> = ["test_choices", "test_choices_kambara", "test_choices_tampadesa"]
        .iter()
        .flat_map(|id| &stories.stories[*id].sequences)
        .flat_map(|sequence| &sequence.panels)
        .flat_map(|panel| panel.bubbles.iter().map(|bubble| bubble.kind))
        .collect();
    for kind in [BubbleKind::Speech, BubbleKind::Whisper, BubbleKind::Group, BubbleKind::Bold] {
        assert!(kinds.contains(&kind), "a test sequence uses a {kind} bubble");
    }
}

// Triggers by index and by id; a sequence named for a choice point also plays for its variant, but
// one named for the variant wins there; an index past the missions waits; the epilogue comes last.
fn test_triggers_resolve() {
    let campaigns = synthetic_campaigns();
    let campaign = &campaigns.campaigns[0];
    assert_eq!(resolve_trigger(campaign, &StoryTrigger::BeforeMission(MissionRef::Index(1))), TriggerPoint::Before(1));
    assert_eq!(
        resolve_trigger(campaign, &StoryTrigger::AfterMission(MissionRef::Id("two_b".into()))),
        TriggerPoint::After(2)
    );
    assert_eq!(resolve_trigger(campaign, &StoryTrigger::AfterMission(MissionRef::Index(9))), TriggerPoint::Pending);
    assert_eq!(resolve_trigger(campaign, &StoryTrigger::AfterMission(MissionRef::Id("nine".into()))), TriggerPoint::Unknown);

    let stories = stories_for(
        &campaigns,
        serde_json::json!({ "sequences": [
            sequence("end", serde_json::json!("epilogue")),
            sequence("after_two_b", serde_json::json!({ "after_mission": "two_b" })),
            sequence("before_two", serde_json::json!({ "before_mission": 1 })),
            sequence("after_two", serde_json::json!({ "after_mission": 1 })),
            sequence("later", serde_json::json!({ "before_mission": 7 })),
            sequence("start", serde_json::json!({ "before_mission": 0 })),
        ]}),
    );
    assert_eq!(stories.validate(&campaigns), Vec::<String>::new());
    let id = |sequence: Option<game::story::SequenceRef>| sequence.map(|sequence| sequence.sequence.id.clone());
    assert_eq!(id(stories.before_mission(&campaigns, 0, 0)), Some("start".into()));
    assert_eq!(id(stories.before_mission(&campaigns, 0, 1)), Some("before_two".into()));
    assert_eq!(
        id(stories.before_mission(&campaigns, 0, 2)),
        Some("before_two".into()),
        "the variant takes its choice point's"
    );
    assert_eq!(id(stories.after_mission(&campaigns, 0, 1)), Some("after_two".into()));
    assert_eq!(id(stories.after_mission(&campaigns, 0, 2)), Some("after_two_b".into()), "its own wins");
    assert_eq!(id(stories.after_mission(&campaigns, 0, 3)), None);
    assert_eq!(id(stories.before_mission(&campaigns, 0, 7)), None, "no such mission");
    assert_eq!(id(stories.epilogue(&campaigns, 0)), Some("end".into()));
    let order: Vec<String> =
        stories.sequences_in_order(&campaigns, 0).iter().map(|sequence| sequence.sequence.id.clone()).collect();
    assert_eq!(order, ["start", "before_two", "after_two", "after_two_b", "end"], "play order, the waiting one left out");

    // A variant listed after a later step (missions are appended, since saves name them by index)
    // still plays, and sorts, with its choice point's step.
    let mut later_variant = synthetic_campaigns();
    later_variant.campaigns[0].missions.swap(2, 3);
    later_variant.campaigns[0].missions[1].choices[1].mission = MissionRef::Index(3);
    assert_eq!(later_variant.campaigns[0].step_of(3), 1);
    let stories = stories_for(
        &later_variant,
        serde_json::json!({ "sequences": [
            sequence("after_three", serde_json::json!({ "after_mission": 2 })),
            sequence("after_two_b", serde_json::json!({ "after_mission": "two_b" })),
            sequence("after_two", serde_json::json!({ "after_mission": 1 })),
        ]}),
    );
    assert_eq!(stories.validate(&later_variant), Vec::<String>::new());
    let order: Vec<String> =
        stories.sequences_in_order(&later_variant, 0).iter().map(|sequence| sequence.sequence.id.clone()).collect();
    assert_eq!(order, ["after_two", "after_two_b", "after_three"], "the variant's with its step, before the later step's");
}

fn test_validation_finds_errors() {
    let campaigns = synthetic_campaigns();
    let bubble = |anchor: [f32; 2], tail: [f32; 2]| {
        serde_json::json!({ "speaker": "speaker.x", "text": "story.x", "anchor": anchor, "tail": tail })
    };
    let errors = stories_for(
        &campaigns,
        serde_json::json!({ "sequences": [
            sequence("one", serde_json::json!({ "before_mission": 0 })),
            sequence("one", serde_json::json!({ "after_mission": 0 })),
            sequence("Bad-Id", serde_json::json!({ "after_mission": 3 })),
            sequence("twice", serde_json::json!({ "before_mission": 0 })),
            sequence("nowhere", serde_json::json!({ "before_mission": "nine" })),
            sequence("waits", serde_json::json!({ "after_mission": 8 })),
            sequence("waits_too", serde_json::json!({ "after_mission": 8 })),
            sequence("waits_elsewhere", serde_json::json!({ "after_mission": 9 })),
            { "id": "empty", "title": "", "trigger": "epilogue", "panels": [] },
            { "id": "placed", "title": "story.t", "trigger": { "after_mission": 1 }, "panels": [{
                "image": "",
                "narration_anchor": [10, 900],
                "bubbles": [bubble([10.0, 10.0], [PANEL_SIZE[0] + 1.0, 10.0]), bubble([1700.0, 10.0], [10.0, 10.0])]
            }]},
        ]}),
    )
    .validate(&campaigns);
    let expect = [
        "sequence id 'one' is given twice",
        "sequence id 'Bad-Id' is empty or not [a-z0-9_]",
        "Sequences 'one' and 'twice' both play Before(0)",
        "Sequence 'nowhere' plays",
        "Sequences 'waits' and 'waits_too' both play Pending",
        "Sequence 'empty' has no title",
        "Sequence 'empty' has no panels",
        "Sequence 'placed', panel 1 has no picture",
        "Sequence 'placed', panel 1: the caption is off the panel",
        "Sequence 'placed', panel 1, bubble 1: its anchor or tail is off the panel",
    ];
    for expected in expect {
        assert!(errors.iter().any(|error| error.contains(expected)), "expected an error with '{expected}' in {errors:?}");
    }
    assert_eq!(errors.len(), expect.len(), "{errors:?}");
    // A width that runs off the panel.
    let wide_bubble = serde_json::json!({ "text": "story.x", "anchor": [1500, 10], "width": 400, "tail": [10, 10] });
    let wide = stories_for(
        &campaigns,
        serde_json::json!({ "sequences": [{ "id": "wide", "title": "story.t", "trigger": "epilogue",
            "panels": [{ "image": "story/x.jpg", "bubbles": [wide_bubble] }] }] }),
    )
    .validate(&campaigns);
    assert!(wide.iter().any(|error| error.contains("width runs off")), "{wide:?}");
    // A picture that isn't there and keys en.json lacks.
    let missing =
        stories_for(&campaigns, serde_json::json!({ "sequences": [sequence("x", serde_json::json!("epilogue"))] }));
    assert!(missing.missing_images()[0].contains("ui/story/x.jpg"), "{:?}", missing.missing_images());
    assert_eq!(missing.missing_text_keys().len(), 1, "story.title isn't in en.json");
    // Unknown fields are refused.
    assert!(serde_json::from_value::<StoryDef>(serde_json::json!({ "sequences": [], "extra": 1 })).is_err());
}

// A sequence plays once per profile: seen (played or skipped) it's recorded in the campaign's
// record, each once, left out of the file while none is; Replay the story lists the seen ones in
// play order.
fn test_seen_sequences_and_replay() {
    let campaigns = synthetic_campaigns();
    let stories = stories_for(
        &campaigns,
        serde_json::json!({ "sequences": [
            sequence("end", serde_json::json!("epilogue")),
            sequence("start", serde_json::json!({ "before_mission": 0 })),
        ]}),
    );
    let mut profile = Profile::new();
    assert!(!profile.to_json().contains("sequences_seen"), "none seen: none in the file");
    assert!(stories.seen_sequences(&campaigns, &profile).is_empty());

    profile.sequence_seen(&campaigns, 0, "end");
    profile.sequence_seen(&campaigns, 0, "start");
    profile.sequence_seen(&campaigns, 0, "start");
    profile.sequence_seen(&campaigns, 5, "start"); // No such campaign: nothing.
    assert!(profile.has_seen_sequence("one", "start"));
    assert!(!profile.record("one").unwrap().reached, "seeing a sequence reaches nothing");
    assert_eq!(profile.record("one").unwrap().sequences_seen, ["end", "start"], "each once, as first seen");
    assert_eq!(Profile::from_json(&profile.to_json()).unwrap(), profile);
    let replay: Vec<String> =
        stories.seen_sequences(&campaigns, &profile).iter().map(|sequence| sequence.sequence.id.clone()).collect();
    assert_eq!(replay, ["start", "end"], "in play order");

    // profile.set keeps each once, in order.
    let mut edited = Profile::new();
    edited
        .set_campaign_records(
            &campaigns,
            vec![serde_json::json!({ "id": "one", "sequences_seen": ["start", "end", "start"] })],
        )
        .unwrap();
    assert_eq!(edited.record("one").unwrap().sequences_seen, ["start", "end"]);

    // profile.set refuses ids the campaign's story lacks (story::check_seen_sequences, on the
    // shipped stories).
    let (shipped, _) = configs();
    let with_seen = |campaign: &str, seen: &[&str]| {
        let mut profile = Profile::new();
        profile
            .set_campaign_records(shipped, vec![serde_json::json!({ "id": campaign, "sequences_seen": seen })])
            .unwrap();
        game::story::check_seen_sequences(&profile)
    };
    assert_eq!(with_seen("dragon_legacy", &["i_a"]), Ok(()));
    let unknown = with_seen("dragon_legacy", &["i_a", "nope"]).unwrap_err();
    assert!(unknown.contains("'nope'") && unknown.contains("i_a, i_b"), "{unknown}");
    let none = with_seen("temple_mountain", &["i_a"]).unwrap_err();
    assert!(none.contains("temple_mountain") && none.contains("none"), "{none}");
}
