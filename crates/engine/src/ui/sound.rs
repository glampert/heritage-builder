// UI sounds, by role. A role (UiSoundKey) is named by what it's for, such as a button pressed or a
// dialog opened, not by its file: a mapping, which is data, gives each role a file under
// assets/sounds/sfx/ui/. The game gives the default mapping (set_default_mapping: Đại Long's skin's
// `sounds` block, assets/configs/ui/skins/dailong.json); set_mapping lays overrides over it (the
// current realm skin's), and a role neither names plays nothing.
//
// Each role has its own flags and cooldown (UiSoundKey::default_flags, default_cooldown), measured
// in frame time. Retained widgets play their roles through UiWidgetSounds, which lets a widget swap
// a role for another or silence it. Two roles mapped to the same file that start in the same frame
// play it once, e.g. a button's press and the dialog it opens.
//
// The roles played are recorded, with their frame, whether there's an audio device or not:
// recent() and clear_recent(), which automation reads (ui.sounds).

use std::{collections::{BTreeMap, VecDeque}, str::FromStr};
use arrayvec::ArrayVec;
use bitflags::bitflags;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumCount, EnumIter, EnumString, IntoEnumIterator, IntoStaticStr};

use common::{singleton_late_init, time::Seconds};

use super::widgets::UiWidgetContext;
use crate::{
    file_sys::paths::AssetPath,
    sound::{SfxSoundKey, SoundHandle, SoundKey, SoundKind, SoundSystem},
};

// ----------------------------------------------
// UiSoundKey
// ----------------------------------------------

pub const UI_SOUND_DEFAULT_COOLDOWN: Seconds = 0.2;

// What a sound is for. Mappings and automation name a role in snake_case ("dialog_opened").
// New roles can go anywhere in the list; give each a file in Đại Long's skin's `sounds` block.
#[derive(
    Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug,
    EnumCount, EnumIter, Display, EnumString, IntoStaticStr, Serialize, Deserialize,
)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum UiSoundKey {
    // Buttons: text and sprite buttons.
    ButtonHovered,
    ButtonPressed,

    // Menu items and list rows: item lists, a dropdown's items.
    ItemHovered,
    ItemSelected,

    // Tabs.
    TabSelected,

    // Toggles and checkboxes.
    Toggled,

    // Sliders, step selectors and a number field's steps. Rate-limited by the cooldown.
    SliderChanged,

    // A dropdown's list. Picking an item plays ItemSelected instead of DropdownClosed.
    DropdownOpened,
    DropdownClosed,

    // Dialogs and their popups (message boxes).
    DialogOpened,
    DialogClosed,
    PopupOpened,
    PopupClosed,

    // Messages: toasts and, later, the message log. An alert is a warning, such as a minimap alert
    // or a debt notice.
    Notification,
    Alert,

    // The build palette.
    TilePlaced,
    TileCleared,
    TilePlacementCanceled,
    TilePlacementFailed,

    // A building the player had built finished construction (its toast plays this, not Notification).
    ConstructionCompleted,
}

impl UiSoundKey {
    #[inline]
    pub fn name(self) -> &'static str {
        self.into()
    }

    // A role by its snake_case name.
    #[inline]
    pub fn parse(name: &str) -> Option<Self> {
        Self::from_str(name).ok()
    }

    // How play() plays the role. Button hovers and presses, item hovers and the palette's sounds
    // wait for any UI sound to finish, as buttons always did; the other roles wait only for their
    // own, so a dialog's open sound plays right after the press that opened it.
    pub const fn default_flags(self) -> UiPlaySoundFlags {
        match self {
            Self::ButtonHovered
            | Self::ButtonPressed
            | Self::ItemHovered
            | Self::TilePlaced
            | Self::TileCleared
            | Self::TilePlacementCanceled
            | Self::TilePlacementFailed => UiPlaySoundFlags::GloballyExclusive,
            _ => UiPlaySoundFlags::Exclusive,
        }
    }

    // The least time between two plays of the role: a slider's drag ticks at most this often,
    // and several toasts in a frame play one sound.
    pub const fn default_cooldown(self) -> Seconds {
        match self {
            Self::Notification | Self::ConstructionCompleted => 0.5,
            Self::Alert => 1.0,
            _ => UI_SOUND_DEFAULT_COOLDOWN,
        }
    }
}

bitflags! {
    #[derive(Copy, Clone, Default, PartialEq, Eq, Debug)]
    pub struct UiPlaySoundFlags: u8 {
        const Exclusive         = 1 << 0; // Does not play if the same sound is already playing.
        const GloballyExclusive = 1 << 1; // Does not play if *any* UI sound is playing.
        const Looping           = 1 << 2;
    }
}

// ----------------------------------------------
// UiSoundMapping
// ----------------------------------------------

// Each role's file, relative to assets/sounds/sfx/ui/ and with its extension, e.g.
// "buttons/default/pressed.wav". None plays nothing.
pub type UiSoundMapping = BTreeMap<UiSoundKey, Option<String>>;

// A mapping from role names, as a JSON file holds them. Unknown roles and empty file names are
// left out and returned as problems.
pub fn mapping_from_names(names: BTreeMap<String, Option<String>>) -> (UiSoundMapping, Vec<String>) {
    let mut mapping = UiSoundMapping::new();
    let mut problems = Vec::new();

    for (name, file) in names {
        let Some(role) = UiSoundKey::parse(&name) else {
            problems.push(format!("Unknown UI sound role '{name}'."));
            continue;
        };
        match file {
            Some(file) if file.trim().is_empty() => problems.push(format!("UI sound role '{name}' has an empty file name.")),
            file => {
                mapping.insert(role, file);
            }
        }
    }

    (mapping, problems)
}

// The file `role` plays with `overrides` laid over `defaults`: the override's (None silences it),
// else the default's, else nothing.
pub fn mapped_file<'a>(defaults: &'a UiSoundMapping, overrides: &'a UiSoundMapping, role: UiSoundKey) -> Option<&'a str> {
    overrides.get(&role).or_else(|| defaults.get(&role)).and_then(Option::as_deref)
}

// ----------------------------------------------
// Global UI Sound Effects API
// ----------------------------------------------

// Sets up the UI sounds, with no mapping yet: every role plays nothing until the game gives the
// default mapping (set_default_mapping), Đại Long's skin's. Once.
pub fn initialize() {
    if UiSoundManagerSingleton::is_initialized() {
        return; // Initialize only once.
    }

    UiSoundManagerSingleton::initialize(UiSoundManagerSingleton::new());
}

// The mapping roles fall back to when set_mapping's overrides don't name them: the game's base
// skin's (Đại Long's `sounds` block). Clears the overrides.
pub fn set_default_mapping(sound_sys: &mut SoundSystem, defaults: UiSoundMapping) {
    let manager = UiSoundManagerSingleton::get_mut();
    manager.defaults = defaults;
    manager.set_mapping(sound_sys, &UiSoundMapping::new());
}

// Lays `overrides` over the default mapping, e.g. a realm skin's sounds: a role it names takes
// its file (None silences it), and the others take the default's. Loads only the roles whose
// file changed; the sound registry keeps a file loaded once, so this is web-safe.
pub fn set_mapping(sound_sys: &mut SoundSystem, overrides: &UiSoundMapping) {
    UiSoundManagerSingleton::get_mut().set_mapping(sound_sys, overrides);
}

// The file `role` plays now, if any.
pub fn file(role: UiSoundKey) -> Option<String> {
    UiSoundManagerSingleton::get().sounds[role as usize].file.clone()
}

// Advances the UI sounds' clock; the engine calls it at the start of every frame.
pub fn begin_frame(frame: u64, delta_time_secs: Seconds) {
    if UiSoundManagerSingleton::is_initialized() {
        UiSoundManagerSingleton::get_mut().begin_frame(frame, delta_time_secs);
    }
}

// The frame begin_frame was last called for.
pub fn current_frame() -> u64 {
    if UiSoundManagerSingleton::is_initialized() { UiSoundManagerSingleton::get().frame } else { 0 }
}

// Plays the role with its own flags and cooldown (see UiSoundKey::default_flags).
pub fn play(sound_sys: &mut SoundSystem, sound_key: UiSoundKey) {
    play_with_opts(sound_sys, sound_key, sound_key.default_cooldown(), sound_key.default_flags());
}

pub fn play_with_opts(sound_sys: &mut SoundSystem, sound_key: UiSoundKey, cooldown: Seconds, flags: UiPlaySoundFlags) {
    UiSoundManagerSingleton::get_mut().play_with_opts(sound_sys, sound_key, cooldown, flags);
}

// A role that played: past its cooldown and exclusivity, with or without an audio device.
#[derive(Clone, Debug, Serialize)]
pub struct UiSoundPlayed {
    pub role: UiSoundKey,
    pub frame: u64,
    pub file: Option<String>, // None if the role is mapped to no file.
}

// How many recent() keeps.
pub const UI_SOUND_RECENT_CAPACITY: usize = 64;

// The roles that played lately, oldest first.
pub fn recent() -> Vec<UiSoundPlayed> {
    UiSoundManagerSingleton::get().recent.iter().cloned().collect()
}

pub fn clear_recent() {
    UiSoundManagerSingleton::get_mut().recent.clear();
}

// ----------------------------------------------
// UiSoundEvent / UiWidgetSounds
// ----------------------------------------------

// What happened to a widget, for its sound.
#[derive(Copy, Clone, PartialEq, Eq, Debug, EnumCount, EnumIter)]
pub enum UiSoundEvent {
    Hovered,  // The cursor moved onto it, or onto one of its rows.
    Pressed,  // A button pressed.
    Selected, // An item picked: a list row, a dropdown's item, a tab.
    Toggled,  // A checkbox or toggle switched.
    Changed,  // A value moved: a slider, a number field's step.
    Opened,   // A dropdown's list or a menu opened.
    Closed,
}

const UI_SOUND_EVENT_COUNT: usize = UiSoundEvent::COUNT;

#[derive(Copy, Clone, Default, PartialEq, Eq, Debug)]
enum UiSoundChoice {
    #[default]
    Default,
    Silent,
    Role(UiSoundKey),
}

// A widget's sounds: for each event, the role the widget plays by default, another role, or
// nothing. A widget plays an event with one call, giving its default role:
//   self.sounds.play(context, UiSoundEvent::Toggled, Some(UiSoundKey::Toggled));
#[derive(Copy, Clone, Default, PartialEq, Eq, Debug)]
pub struct UiWidgetSounds {
    choices: [UiSoundChoice; UI_SOUND_EVENT_COUNT],
}

impl UiWidgetSounds {
    // Each event plays the widget's default role.
    pub const fn new() -> Self {
        Self { choices: [UiSoundChoice::Default; UI_SOUND_EVENT_COUNT] }
    }

    // No event plays anything.
    pub const fn silent() -> Self {
        Self { choices: [UiSoundChoice::Silent; UI_SOUND_EVENT_COUNT] }
    }

    // `event` plays `role` instead of the widget's default; None silences it.
    pub fn with(mut self, event: UiSoundEvent, role: Option<UiSoundKey>) -> Self {
        self.set(event, role);
        self
    }

    pub fn set(&mut self, event: UiSoundEvent, role: Option<UiSoundKey>) {
        self.choices[event as usize] = match role {
            Some(role) => UiSoundChoice::Role(role),
            None => UiSoundChoice::Silent,
        };
    }

    // The role `event` plays: its override, else `default` (None: nothing by default).
    pub fn role(&self, event: UiSoundEvent, default: Option<UiSoundKey>) -> Option<UiSoundKey> {
        match self.choices[event as usize] {
            UiSoundChoice::Default => default,
            UiSoundChoice::Silent => None,
            UiSoundChoice::Role(role) => Some(role),
        }
    }

    pub fn play(&self, context: &mut dyn UiWidgetContext, event: UiSoundEvent, default: Option<UiSoundKey>) {
        if let Some(role) = self.role(event, default) {
            play(*context.sound_sys(), role);
        }
    }
}

// ----------------------------------------------
// UiSound
// ----------------------------------------------

// A role's sound: its file, loaded, and when it last played.
struct UiSound {
    file: Option<String>,
    key: SfxSoundKey,
    handle: SoundHandle,
    last_play_time: Option<f64>, // On the manager's clock.
}

impl UiSound {
    fn new() -> Self {
        Self { file: None, key: SfxSoundKey::invalid(), handle: SoundHandle::invalid(SoundKind::Sfx), last_play_time: None }
    }

    // Plays `file` from now on. Keeps the cooldown running.
    fn set_file(&mut self, sound_sys: &mut SoundSystem, file: Option<&str>) {
        if self.file.as_deref() == file {
            return;
        }

        self.file = file.map(str::to_string);
        self.handle = SoundHandle::invalid(SoundKind::Sfx);
        self.key = match file {
            // All UI sound assets are under "sfx/ui/{file}".
            Some(file) => sound_sys.load_sfx((&AssetPath::from_str("ui").join(file)).into()),
            None => SfxSoundKey::invalid(),
        };
    }

    #[inline]
    fn is_playing(&self, sound_sys: &SoundSystem) -> bool {
        sound_sys.is_playing(self.handle)
    }
}

// ----------------------------------------------
// UiSoundManagerSingleton
// ----------------------------------------------

const UI_SOUND_KEY_COUNT: usize = UiSoundKey::COUNT;

struct UiSoundManagerSingleton {
    defaults: UiSoundMapping,
    sounds: ArrayVec<UiSound, UI_SOUND_KEY_COUNT>, // By role.

    frame: u64,
    time_secs: f64, // Frame time since init: the sum of the frames' time steps.
    started_this_frame: ArrayVec<UiSoundKey, UI_SOUND_KEY_COUNT>,
    recent: VecDeque<UiSoundPlayed>,
}

impl UiSoundManagerSingleton {
    // No mapping: every role plays nothing.
    fn new() -> Self {
        Self {
            defaults: UiSoundMapping::new(),
            sounds: UiSoundKey::iter().map(|_| UiSound::new()).collect(),
            frame: 0,
            time_secs: 0.0,
            started_this_frame: ArrayVec::new(),
            recent: VecDeque::with_capacity(UI_SOUND_RECENT_CAPACITY),
        }
    }

    fn set_mapping(&mut self, sound_sys: &mut SoundSystem, overrides: &UiSoundMapping) {
        for role in UiSoundKey::iter() {
            let file = mapped_file(&self.defaults, overrides, role);
            self.sounds[role as usize].set_file(sound_sys, file);
        }
    }

    fn begin_frame(&mut self, frame: u64, delta_time_secs: Seconds) {
        self.frame = frame;
        self.time_secs += f64::from(delta_time_secs.max(0.0));
        self.started_this_frame.clear();
    }

    fn play_with_opts(&mut self, sound_sys: &mut SoundSystem, sound_key: UiSoundKey, cooldown: Seconds, flags: UiPlaySoundFlags) {
        if flags.intersects(UiPlaySoundFlags::GloballyExclusive) && self.is_any_playing(sound_sys) {
            return;
        }

        let time_now = self.time_secs;
        let sound = &self.sounds[sound_key as usize];

        let exclusive = flags.intersects(UiPlaySoundFlags::Exclusive | UiPlaySoundFlags::GloballyExclusive);
        if exclusive && sound.is_playing(sound_sys) {
            return;
        }

        let cooldown_elapsed = sound.last_play_time.is_none_or(|last_play_time| time_now - last_play_time >= f64::from(cooldown));
        if !cooldown_elapsed {
            return;
        }

        // Another role with the same file started this frame: share its sound rather than play
        // the file twice over itself.
        let same_file_handle = sound.file.as_ref().and_then(|file| {
            self.started_this_frame
                .iter()
                .map(|other| &self.sounds[*other as usize])
                .find(|other| other.file.as_ref() == Some(file))
                .map(|other| other.handle)
        });

        let sound = &mut self.sounds[sound_key as usize];
        sound.last_play_time = Some(time_now);
        if let Some(handle) = same_file_handle {
            sound.handle = handle;
        } else if sound.key.is_valid() {
            let looping = flags.intersects(UiPlaySoundFlags::Looping);
            sound.handle = sound_sys.play_sfx(sound.key, looping);
        }

        let file = sound.file.clone();
        if !self.started_this_frame.contains(&sound_key) {
            self.started_this_frame.push(sound_key);
        }

        if self.recent.len() == UI_SOUND_RECENT_CAPACITY {
            self.recent.pop_front();
        }
        self.recent.push_back(UiSoundPlayed { role: sound_key, frame: self.frame, file });
    }

    fn is_any_playing(&self, sound_sys: &SoundSystem) -> bool {
        self.sounds.iter().any(|sound| sound.is_playing(sound_sys))
    }
}

singleton_late_init! { UI_SOUND_MANAGER_SINGLETON, UiSoundManagerSingleton }

// ----------------------------------------------
// Unit Tests
// ----------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_names_round_trip() {
        for role in UiSoundKey::iter() {
            assert_eq!(UiSoundKey::parse(role.name()), Some(role));
            assert_eq!(serde_json::to_value(role).unwrap(), serde_json::json!(role.name()));
        }
        assert_eq!(UiSoundKey::DialogOpened.name(), "dialog_opened");
        assert_eq!(UiSoundKey::parse("DialogOpened"), None);
    }

    #[test]
    fn mapping_from_names_reports_problems() {
        let names = BTreeMap::from([
            ("button_pressed".to_string(), Some("buttons/default/pressed.wav".to_string())),
            ("alert".to_string(), None),
            ("no_such_role".to_string(), Some("x.wav".to_string())),
            ("toggled".to_string(), Some(" ".to_string())),
        ]);
        let (mapping, problems) = mapping_from_names(names);

        assert_eq!(mapping.len(), 2);
        assert_eq!(mapping[&UiSoundKey::ButtonPressed].as_deref(), Some("buttons/default/pressed.wav"));
        assert_eq!(mapping[&UiSoundKey::Alert], None);
        assert_eq!(problems.len(), 2, "{problems:?}");
    }

    #[test]
    fn overrides_fall_back_to_the_defaults() {
        let defaults = UiSoundMapping::from([
            (UiSoundKey::ButtonPressed, Some("buttons/default/pressed.wav".to_string())),
            (UiSoundKey::Alert, Some("misc/default/tile_placement_failed.wav".to_string())),
        ]);
        let overrides = UiSoundMapping::from([
            (UiSoundKey::ButtonPressed, Some("buttons/kambara/pressed.wav".to_string())),
            (UiSoundKey::Alert, None),
        ]);

        assert_eq!(mapped_file(&defaults, &overrides, UiSoundKey::ButtonPressed), Some("buttons/kambara/pressed.wav"));
        assert_eq!(mapped_file(&defaults, &overrides, UiSoundKey::Alert), None); // Silenced.
        assert_eq!(mapped_file(&defaults, &UiSoundMapping::new(), UiSoundKey::Alert), Some("misc/default/tile_placement_failed.wav"));
        assert_eq!(mapped_file(&defaults, &overrides, UiSoundKey::Toggled), None); // Nobody names it.
    }

    #[test]
    fn widget_sounds_overrides() {
        let sounds = UiWidgetSounds::new();
        assert_eq!(sounds.role(UiSoundEvent::Pressed, Some(UiSoundKey::ButtonPressed)), Some(UiSoundKey::ButtonPressed));
        assert_eq!(sounds.role(UiSoundEvent::Hovered, None), None);

        let sounds = sounds.with(UiSoundEvent::Pressed, Some(UiSoundKey::TilePlaced)).with(UiSoundEvent::Hovered, None);
        assert_eq!(sounds.role(UiSoundEvent::Pressed, Some(UiSoundKey::ButtonPressed)), Some(UiSoundKey::TilePlaced));
        assert_eq!(sounds.role(UiSoundEvent::Hovered, Some(UiSoundKey::ButtonHovered)), None);

        let silent = UiWidgetSounds::silent();
        for event in UiSoundEvent::iter() {
            assert_eq!(silent.role(event, Some(UiSoundKey::ButtonPressed)), None);
        }
    }
}
