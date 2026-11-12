// The player's profile (docs/plans/realm-ui.md, Realm and progress): their progress across
// sessions, which session saves don't hold. Which campaigns they've reached, the missions they've
// finished, the last save (what Continue loads) and the realms Custom Game offers.
//
// It's the user file "profile" (engine::save::storage): saves/user/profile.json on desktop,
// localStorage on the web. The whole profile is written each time it changes; it's not an
// overlay on defaults like the user configs. A missing file gives a fresh profile, and so do a
// broken one and one of another version, with a warning; a run that writes the profile first
// copies such a file to saves/user/profile_unloaded.json, as the next write replaces it.
//
// Loaded once at startup (GameLoop::start). --fresh-profile starts from an empty profile and
// never reads or writes the file. Script mode, replays and the save smoke test read it but never
// write it, and the test harness never loads it: they keep their changes in memory, so
// automation can't leave a profile behind. A replay uses the profile its recording holds, unless
// --fresh-profile is given.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use engine::{file_sys::paths::PathRef, log, save::storage};

use crate::{
    campaign::config::CampaignConfigs,
    realm::Realm,
};

// Bump when the layout changes. A profile of another version is replaced by a fresh one (with a
// warning) until a migration exists.
pub const PROFILE_VERSION: u32 = 1;

// The user file's name: saves/user/profile.json. No dots: the storage adds the extension.
const PROFILE_FILE: PathRef<'static> = PathRef::from_str("profile");

// Where a profile file that fails to load is copied before the fresh profile replaces it:
// saves/user/profile_unloaded.json (another version's, a broken one, one with unknown fields).
const PROFILE_UNLOADED_FILE: PathRef<'static> = PathRef::from_str("profile_unloaded");

// ----------------------------------------------
// Profile
// ----------------------------------------------

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub version: u32,
    // One record per campaign the player has played or reached, by campaign id.
    pub campaigns: Vec<CampaignRecord>,
    // The name of the last game saved (a file in saves/, without its extension): what Continue loads.
    pub last_save: Option<String>,
    // The realms Custom Game offers: each is unlocked when its campaign is started (gdd.md 4).
    pub custom_realms: Vec<Realm>,
}

// A campaign's progress. Named so it isn't confused with campaign::CampaignProgress, the active
// mission a session save holds.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CampaignRecord {
    pub id: String,                    // CampaignDef::id.
    pub reached: bool,                 // Playable: the first campaign, or the one before it finished.
    pub current_mission: Option<usize>, // The mission last started, by index in the campaign.
    pub missions_finished: Vec<usize>, // Indices of the missions finished, ascending.
    pub distinctions: Vec<String>,     // TODO(realm-ui): the distinctions earned, once M9 awards them.
    pub finished: bool,                // Its last mission is finished.
}

impl Default for Profile {
    fn default() -> Self {
        Self::new()
    }
}

impl Profile {
    // A fresh profile: nothing reached, nothing saved.
    pub const fn new() -> Self {
        Self { version: PROFILE_VERSION, campaigns: Vec::new(), last_save: None, custom_realms: Vec::new() }
    }

    // ----------------------
    // JSON:
    // ----------------------

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("A profile should serialize to JSON")
    }

    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).expect("A profile should serialize to JSON")
    }

    // A profile from the file's text. Fails on broken JSON, a profile of another version or one
    // that doesn't fit.
    pub fn from_json(text: &str) -> Result<Self, String> {
        let value: Value = serde_json::from_str(text).map_err(|err| format!("isn't valid JSON: {err}"))?;
        Self::from_value(value)
    }

    pub fn from_value(value: Value) -> Result<Self, String> {
        match value.get("version").and_then(Value::as_u64) {
            Some(version) if version == u64::from(PROFILE_VERSION) => {}
            Some(version) => return Err(format!("is version {version}, and this game reads version {PROFILE_VERSION}")),
            None => return Err("has no version".to_string()),
        }
        serde_json::from_value(value).map_err(|err| format!("doesn't fit the profile: {err}"))
    }

    // The profile a file's text (None: no file) gives: the file's, or a fresh one, with the reason
    // for a fresh one if the file was there.
    pub fn from_file_text(text: Option<&str>) -> (Self, Option<String>) {
        match text {
            None => (Self::new(), None),
            Some(text) => match Self::from_json(text) {
                Ok(profile) => (profile, None),
                Err(err) => (Self::new(), Some(err)),
            },
        }
    }

    // ----------------------
    // Queries:
    // ----------------------

    pub fn record(&self, campaign_id: &str) -> Option<&CampaignRecord> {
        self.campaigns.iter().find(|record| record.id == campaign_id)
    }

    // The campaign's record, added if it has none.
    pub fn record_mut(&mut self, campaign_id: &str) -> &mut CampaignRecord {
        let index = match self.campaigns.iter().position(|record| record.id == campaign_id) {
            Some(index) => index,
            None => {
                self.campaigns.push(CampaignRecord { id: campaign_id.to_string(), ..Default::default() });
                self.campaigns.len() - 1
            }
        };
        &mut self.campaigns[index]
    }

    pub fn is_reached(&self, campaign_id: &str) -> bool {
        self.record(campaign_id).is_some_and(|record| record.reached)
    }

    // The realm of the latest campaign reached, in campaigns.json order, hidden ones aside. None if
    // none is.
    pub fn latest_reached_realm(&self, configs: &CampaignConfigs) -> Option<Realm> {
        configs.visible().map(|(_, campaign)| campaign).filter(|campaign| self.is_reached(&campaign.id)).last().map(|campaign| campaign.realm)
    }

    // Whether the campaign at `campaign_index` in `configs` can be played: the first one always
    // (a new player starts it), the others once reached (the one before them finished, or
    // profile.set for a hidden one).
    pub fn is_playable(&self, configs: &CampaignConfigs, campaign_index: usize) -> bool {
        configs.campaign(campaign_index).is_some_and(|campaign| (campaign_index == 0 && !campaign.hidden) || self.is_reached(&campaign.id))
    }

    // The campaigns the menus offer, by index in `configs`, in order: the playable ones, hidden
    // campaigns aside.
    pub fn playable_campaigns(&self, configs: &CampaignConfigs) -> Vec<usize> {
        configs.visible().map(|(index, _)| index).filter(|&index| self.is_playable(configs, index)).collect()
    }

    // The mission the campaign at `campaign_index` resumes at: the first step it hasn't finished
    // (a choice point stands for its sites' missions: CampaignDef, Missions and choices), or its
    // first again once every one is finished. None for a campaign without missions (yet).
    pub fn next_mission(&self, configs: &CampaignConfigs, campaign_index: usize) -> Option<usize> {
        let campaign = configs.campaign(campaign_index)?;
        let finished = self.record(&campaign.id).map_or(&[][..], |record| record.missions_finished.as_slice());
        let steps = campaign.steps();
        steps.iter().copied().find(|&step| !campaign.is_step_finished(step, finished)).or_else(|| steps.first().copied())
    }

    // ----------------------
    // Progress:
    // ----------------------

    pub fn mark_reached(&mut self, campaign_id: &str) {
        self.record_mut(campaign_id).reached = true;
    }

    // A mission of the campaign at `campaign_index` in `configs` started: the campaign is reached
    // and its realm unlocked for Custom Game.
    pub fn mission_started(&mut self, configs: &CampaignConfigs, campaign_index: usize, mission_index: usize) {
        let Some(campaign) = configs.campaign(campaign_index) else {
            return;
        };

        let record = self.record_mut(&campaign.id);
        record.reached = true;
        record.current_mission = Some(mission_index);

        if !campaign.hidden && !self.custom_realms.contains(&campaign.realm) {
            self.custom_realms.push(campaign.realm);
        }
    }

    // A mission's goals were met. Finishing a mission of a campaign's last step finishes the
    // campaign and reaches the next visible one in campaigns.json (a hidden campaign reaches none).
    pub fn mission_finished(&mut self, configs: &CampaignConfigs, campaign_index: usize, mission_index: usize) {
        let Some(campaign) = configs.campaign(campaign_index) else {
            return;
        };

        let record = self.record_mut(&campaign.id);
        record.reached = true;
        if let Err(position) = record.missions_finished.binary_search(&mission_index) {
            record.missions_finished.insert(position, mission_index);
        }
        // TODO(realm-ui): record the mission's distinctions here once M9 awards them.

        if campaign.is_last_step(mission_index) {
            record.finished = true;
            if let Some(next) = configs.next_visible_after(campaign_index).and_then(|index| configs.campaign(index)) {
                self.mark_reached(&next.id);
            }
        }
    }

    pub fn game_saved(&mut self, save_name: &str) {
        self.last_save = Some(save_name.to_string());
    }

    // ----------------------
    // Edits (profile.set), checked against the configs:
    // ----------------------

    // Replaces every campaign record with `records`, each a record's JSON over the defaults (its
    // fields left out take theirs), naming a campaign of `configs` once, its missions in range.
    pub fn set_campaign_records(&mut self, configs: &CampaignConfigs, records: Vec<Value>) -> Result<(), String> {
        let records: Vec<CampaignRecord> = records.into_iter().map(|record| campaign_record_from_json(configs, record)).collect::<Result<_, _>>()?;
        for (index, record) in records.iter().enumerate() {
            if records[..index].iter().any(|other| other.id == record.id) {
                return Err(format!("Campaign '{}' has two records.", record.id));
            }
        }
        self.campaigns = records;
        Ok(())
    }

    // The campaigns reached are exactly those with these ids; the others are no longer reached.
    pub fn set_reached(&mut self, configs: &CampaignConfigs, reached: &[String]) -> Result<(), String> {
        for id in reached {
            campaign_index(configs, id)?;
        }
        for record in &mut self.campaigns {
            record.reached = reached.contains(&record.id);
        }
        for id in reached {
            self.mark_reached(id);
        }
        Ok(())
    }

    // The realms Custom Game offers, by name, each once.
    pub fn set_custom_realms(&mut self, names: &[String]) -> Result<(), String> {
        let mut realms = Vec::new();
        for name in names {
            let realm = Realm::parse(name)?;
            if !realms.contains(&realm) {
                realms.push(realm);
            }
        }
        self.custom_realms = realms;
        Ok(())
    }
}

fn campaign_index(configs: &CampaignConfigs, id: &str) -> Result<usize, String> {
    configs.find_by_id(id).ok_or_else(|| {
        let ids: Vec<&str> = configs.campaigns.iter().map(|campaign| campaign.id.as_str()).collect();
        format!("No campaign has the id '{id}'. Campaigns: {}.", ids.join(", "))
    })
}

// A record from its JSON, over the defaults, checked against the campaign it names; its finished
// missions sorted, each once.
fn campaign_record_from_json(configs: &CampaignConfigs, record: Value) -> Result<CampaignRecord, String> {
    let mut merged = serde_json::to_value(CampaignRecord::default()).expect("A record should serialize to JSON");
    match (record, &mut merged) {
        (Value::Object(fields), Value::Object(defaults)) => defaults.extend(fields),
        (other, _) => return Err(format!("A campaign record is an object, not {other}.")),
    }

    let mut record: CampaignRecord = serde_json::from_value(merged).map_err(|err| format!("Invalid campaign record: {err}"))?;

    let missions = configs.campaigns[campaign_index(configs, &record.id)?].missions.len();
    let out_of_range = record.current_mission.iter().chain(&record.missions_finished).find(|&&index| index >= missions);
    if let Some(index) = out_of_range {
        return Err(format!("Campaign '{}' has {missions} missions: there's no mission {index}.", record.id));
    }

    record.missions_finished.sort_unstable();
    record.missions_finished.dedup();
    Ok(record)
}

// ----------------------------------------------
// The profile of this run
// ----------------------------------------------

struct ProfileStore {
    profile: Profile,
    writes_file: bool, // False: kept in memory (--fresh-profile, scripts, replays, the smoke test, tests).
    initialized: bool,
    revision: u32, // Goes up with every change (see revision()).
}

impl ProfileStore {
    const fn new() -> Self {
        Self { profile: Profile::new(), writes_file: false, initialized: false, revision: 0 }
    }
}

common::singleton! { PROFILE_STORE, ProfileStore }

pub struct ProfileOptions {
    pub read_file: bool,          // False: start from a fresh profile (--fresh-profile), even in a replay.
    pub write_file: bool,         // False: keep every change in memory.
    pub recorded: Option<Profile>, // A replay's: used instead of the file, unless read_file is false.
}

// Loads the profile for this run. Called once by GameLoop::start, after the launch options and
// before the first session. Until then (and in tests) the profile is a fresh one in memory.
pub fn initialize(options: ProfileOptions) {
    let store = ProfileStore::get_mut();
    if store.initialized {
        log::error!(log::channel!("profile"), "The profile is already loaded.");
        return;
    }

    let file_path = storage::user_file_path(PROFILE_FILE);
    let mut write_file = options.write_file;

    // --fresh-profile wins over a replay's profile, as options given with --replay win over the
    // recorded ones. A recording made with it holds a fresh profile, so its replay is the same.
    store.profile = if !options.read_file {
        log::info!(log::channel!("profile"), "--fresh-profile: starting from an empty profile, leaving out {file_path}.");
        if options.recorded.is_some_and(|recorded| recorded != Profile::new()) {
            log::warning!(
                log::channel!("profile"),
                "--fresh-profile: replaying with an empty profile, not the recording's, so the replay may differ."
            );
        }
        Profile::new()
    } else if let Some(recorded) = options.recorded {
        log::info!(log::channel!("profile"), "Using the recording's profile.");
        recorded
    } else {
        let text = match storage::read_user_file(PROFILE_FILE) {
            Ok(text) => text,
            Err(err) => {
                log::warning!(log::channel!("profile"), "Starting a fresh profile: can't read {file_path}: {err}");
                None
            }
        };
        let (profile, error) = Profile::from_file_text(text.as_deref());
        match (error, &text) {
            // The fresh profile would overwrite the file at its first change (the first autosave
            // sets last_save), so the file is copied aside first; if it can't be, this run leaves
            // the file alone.
            (Some(err), Some(text)) if options.write_file => {
                let copy = storage::user_file_path(PROFILE_UNLOADED_FILE);
                match storage::write_user_file(PROFILE_UNLOADED_FILE, text) {
                    Ok(()) => log::warning!(
                        log::channel!("profile"),
                        "Starting a fresh profile: {file_path} {err}. It's kept as {copy}; the fresh profile replaces it at its first change."
                    ),
                    Err(copy_err) => {
                        write_file = false;
                        log::warning!(
                            log::channel!("profile"),
                            "Starting a fresh profile: {file_path} {err}. It can't be kept as {copy} ({copy_err}), so it's left alone for this run."
                        );
                    }
                }
            }
            (Some(err), _) => log::warning!(log::channel!("profile"), "Starting a fresh profile: {file_path} {err}. The file is left as it is."),
            (None, Some(_)) => log::info!(log::channel!("profile"), "Loaded the profile from {file_path}."),
            (None, None) => {}
        }
        profile
    };

    store.writes_file = write_file;
    store.initialized = true;
    store.revision += 1;

    if !store.writes_file {
        log::info!(log::channel!("profile"), "The profile is kept in memory for this run: {file_path} won't be written.");
    }
}

pub fn get() -> &'static Profile {
    &ProfileStore::get().profile
}

// Whether changes are written to the file (false under --fresh-profile, in scripts, replays, the
// smoke test and tests).
pub fn writes_file() -> bool {
    ProfileStore::get().writes_file
}

// Goes up whenever the profile changes (a change through update, profile.set, profile.reset), for
// the menus that show it: the home menu and the campaign picker rebuild when it does.
pub fn revision() -> u32 {
    ProfileStore::get().revision
}

// Where the profile is kept, e.g. saves/user/profile.json.
pub fn file_path() -> String {
    storage::user_file_path(PROFILE_FILE).to_string()
}

// Changes the profile, and writes it if that changed anything.
pub fn update(change: impl FnOnce(&mut Profile)) {
    let store = ProfileStore::get_mut();
    let before = store.profile.clone();
    change(&mut store.profile);
    if store.profile != before {
        store.revision += 1;
        write();
    }
}

// Replaces the whole profile (profile.set), and writes it.
pub fn replace(profile: Profile) {
    let store = ProfileStore::get_mut();
    store.profile = profile;
    store.revision += 1;
    write();
}

// A fresh profile (profile.reset), written.
pub fn reset() {
    replace(Profile::new());
}

// Writes the whole profile, unless this run keeps it in memory. Failures are logged.
fn write() {
    let store = ProfileStore::get();
    if !store.writes_file {
        return;
    }
    if let Err(err) = storage::write_user_file(PROFILE_FILE, &store.profile.to_json()) {
        log::error!(log::channel!("profile"), "Can't save the profile to {}: {err}", file_path());
    }
}

// ----------------------------------------------
// Hooks
// ----------------------------------------------

// A campaign mission started (GameSession::start_mission).
pub fn on_mission_started(campaign_index: usize, mission_index: usize) {
    if !CampaignConfigs::is_initialized() {
        return;
    }
    update(|profile| profile.mission_started(CampaignConfigs::get(), campaign_index, mission_index));
}

// A campaign mission's goals were met (campaign::take_finished_missions, drained by GameLoop).
pub fn on_mission_finished(campaign_index: usize, mission_index: usize) {
    if !CampaignConfigs::is_initialized() {
        return;
    }
    update(|profile| profile.mission_finished(CampaignConfigs::get(), campaign_index, mission_index));
}

// A game was saved (GameSession::save_game), the autosave included.
pub fn on_game_saved(save_name: &str) {
    update(|profile| profile.game_saved(save_name));
}
