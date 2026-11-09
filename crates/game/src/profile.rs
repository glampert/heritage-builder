// The player's profile (docs/plans/realm-ui.md, Realm and progress): their progress across
// sessions, which session saves don't hold. Which campaigns they've reached, the missions they've
// finished, the last save (what Continue loads) and the realms Custom Game offers.
//
// It's the user file "profile" (engine::save::storage): saves/user/profile.json on desktop,
// localStorage on the web. The whole profile is written each time it changes; it's not an
// overlay on defaults like the user configs. A missing file gives a fresh profile, and so do a
// broken one and one of another version, with a warning; the broken file stays as it is until
// the next write.
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

    // The realm of the latest campaign reached, in campaigns.json order. None if none is.
    pub fn latest_reached_realm(&self, configs: &CampaignConfigs) -> Option<Realm> {
        configs.campaigns.iter().rev().find(|campaign| self.is_reached(&campaign.id)).map(|campaign| campaign.realm)
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

        if !self.custom_realms.contains(&campaign.realm) {
            self.custom_realms.push(campaign.realm);
        }
    }

    // A mission's goals were met. Finishing a campaign's last mission finishes the campaign and
    // reaches the next one in campaigns.json.
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

        if mission_index + 1 >= campaign.missions.len() {
            record.finished = true;
            if let Some(next) = configs.campaign(campaign_index + 1) {
                self.mark_reached(&next.id);
            }
        }
    }

    pub fn game_saved(&mut self, save_name: &str) {
        self.last_save = Some(save_name.to_string());
    }
}

// ----------------------------------------------
// The profile of this run
// ----------------------------------------------

struct ProfileStore {
    profile: Profile,
    writes_file: bool, // False: kept in memory (--fresh-profile, scripts, replays, the smoke test, tests).
    initialized: bool,
}

impl ProfileStore {
    const fn new() -> Self {
        Self { profile: Profile::new(), writes_file: false, initialized: false }
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
        match error {
            Some(err) => log::warning!(
                log::channel!("profile"),
                "Starting a fresh profile: {file_path} {err}. The file stays as it is until the profile is next saved."
            ),
            None if text.is_some() => log::info!(log::channel!("profile"), "Loaded the profile from {file_path}."),
            None => {}
        }
        profile
    };

    store.writes_file = options.write_file;
    store.initialized = true;

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
        write();
    }
}

// Replaces the whole profile (profile.set), and writes it.
pub fn replace(profile: Profile) {
    ProfileStore::get_mut().profile = profile;
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
