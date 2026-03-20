#[cfg(feature = "desktop")]
use std::time;

#[cfg(feature = "web")]
use web_time as time;

use bitflags::bitflags;
use arrayvec::ArrayVec;
use strum::{EnumCount, EnumProperty, EnumIter, IntoEnumIterator};

use crate::{
    sound::{SoundSystem, SoundHandle, SoundKind, SoundKey, SfxSoundKey},
    utils::{mem::singleton_late_init, time::Seconds, paths::{PathRef, AssetPath}},
};

// ----------------------------------------------
// Global UI Sound Effects API
// ----------------------------------------------

pub const UI_SOUND_DEFAULT_COOLDOWN: Seconds = 0.2;

#[derive(Copy, Clone, PartialEq, Eq, EnumCount, EnumProperty, EnumIter)]
pub enum UiSoundKey {
    // UI button sound effects:
    #[strum(props(SfxPath = "buttons/default/hovered.wav"))]
    ButtonHovered,

    #[strum(props(SfxPath = "buttons/default/pressed.wav"))]
    ButtonPressed,

    // Tile Palette sound effects:
    #[strum(props(SfxPath = "misc/default/tile_placed.wav"))]
    TilePlaced,

    #[strum(props(SfxPath = "misc/default/tile_cleared.wav"))]
    TileCleared,

    #[strum(props(SfxPath = "misc/default/tile_placement_canceled.wav"))]
    TilePlacementCanceled,

    #[strum(props(SfxPath = "misc/default/tile_placement_failed.wav"))]
    TilePlacementFailed,
}

impl UiSoundKey {
    fn sfx_path(self) -> PathRef<'static> {
        PathRef::from_str(self.get_str("SfxPath").unwrap())
    }
}

bitflags! {
    #[derive(Copy, Clone, Default)]
    pub struct UiButtonSoundsEnabled: u8 {
        const Hovered = 1 << 0;
        const Pressed = 1 << 1;
    }
}

bitflags! {
    #[derive(Copy, Clone, Default)]
    pub struct UiPlaySoundFlags: u8 {
        const Exclusive         = 1 << 0; // Does not play if the same sound is already playing.
        const GloballyExclusive = 1 << 1; // Does not play if *any* UI sound is playing.
        const Looping           = 1 << 2;
    }
}

pub fn initialize(sound_sys: &mut SoundSystem) {
    if UiSoundManagerSingleton::is_initialized() {
        return; // Initialize only once.
    }

    UiSoundManagerSingleton::initialize(UiSoundManagerSingleton::new(sound_sys));
}

// Plays sound exclusively, i.e.:
//  - If no other UI sound is currently playing (GloballyExclusive).
//  - If the global cooldown time has elapsed since last call to play().
pub fn play(sound_sys: &mut SoundSystem, sound_key: UiSoundKey) {
    play_with_opts(sound_sys,
                   sound_key,
                   UI_SOUND_DEFAULT_COOLDOWN,
                   UiPlaySoundFlags::GloballyExclusive);
}

pub fn play_with_opts(sound_sys: &mut SoundSystem,
                      sound_key: UiSoundKey,
                      cooldown: Seconds,
                      flags: UiPlaySoundFlags)
{
    UiSoundManagerSingleton::get_mut().play_with_opts(sound_sys, sound_key, cooldown, flags);
}

// ----------------------------------------------
// UiSound
// ----------------------------------------------

pub struct UiSound {
    key: SfxSoundKey,
    handle: SoundHandle,

    last_play_time: Option<time::Instant>,
    cooldown: Seconds, // Only plays if at least this many seconds of cooldown have elapsed since last time played.
}

impl UiSound {
    pub fn load(sound_sys: &mut SoundSystem, sfx_path: PathRef, cooldown: Seconds) -> Self {
        debug_assert!(!sfx_path.is_empty());
        debug_assert!(cooldown >= 0.0);

        // All UI sound assets are under "sfx/ui/{sfx_path}"
        let path = AssetPath::from_str("ui").join(sfx_path);

        Self {
            key: sound_sys.load_sfx((&path).into()),
            handle: SoundHandle::invalid(SoundKind::Sfx), // Handle set when we first play the sound.
            last_play_time: None, // Never played.
            cooldown,
        }
    }

    pub fn is_loaded(&self) -> bool {
        self.key.is_valid()
    }

    pub fn is_playing(&self, sound_sys: &SoundSystem) -> bool {
        sound_sys.is_playing(self.handle)
    }

    pub fn play(&mut self, sound_sys: &mut SoundSystem) {
        self.play_with_opts(sound_sys, self.cooldown, UiPlaySoundFlags::Exclusive);
    }

    pub fn play_with_opts(&mut self, sound_sys: &mut SoundSystem, cooldown: Seconds, flags: UiPlaySoundFlags) {
        if !self.is_loaded() {
            return;
        }

        let exclusive = flags.intersects(UiPlaySoundFlags::Exclusive | UiPlaySoundFlags::GloballyExclusive);
        if exclusive && self.is_playing(sound_sys) {
            return;
        }

        let time_now = time::Instant::now();

        let cooldown_elapsed = if let Some(last_play_time) = self.last_play_time {
            let time_elapsed = time_now - last_play_time;
            time_elapsed.as_secs_f32() >= cooldown
        } else {
            true
        };

        if cooldown_elapsed {
            let looping = flags.intersects(UiPlaySoundFlags::Looping);
            self.handle = sound_sys.play_sfx(self.key, looping);
            self.last_play_time = Some(time_now);
        }
    }
}

// ----------------------------------------------
// UiSoundManagerSingleton
// ----------------------------------------------

const UI_SOUND_KEY_COUNT: usize = UiSoundKey::COUNT;

struct UiSoundManagerSingleton {
    sounds: ArrayVec<UiSound, UI_SOUND_KEY_COUNT>,
}

impl UiSoundManagerSingleton {
    fn new(sound_sys: &mut SoundSystem) -> Self {
        let mut sounds = ArrayVec::new();

        for key in UiSoundKey::iter() {
            let sound = UiSound::load(sound_sys, key.sfx_path(), UI_SOUND_DEFAULT_COOLDOWN);
            sounds.push(sound);
        }

        Self { sounds }
    }

    fn play_with_opts(&mut self,
                      sound_sys: &mut SoundSystem,
                      sound_key: UiSoundKey,
                      cooldown: Seconds,
                      flags: UiPlaySoundFlags)
    {
        if flags.intersects(UiPlaySoundFlags::GloballyExclusive)
            && self.is_any_playing(sound_sys)
        {
            return;
        }

        let sound = &mut self.sounds[sound_key as usize];
        sound.play_with_opts(sound_sys, cooldown, flags);
    }

    fn is_any_playing(&self, sound_sys: &SoundSystem) -> bool {
        for sound in &self.sounds {
            if sound.is_playing(sound_sys) {
                return true;
            }
        }
        false
    }
}

singleton_late_init! { UI_SOUND_MANAGER_SINGLETON, UiSoundManagerSingleton }
