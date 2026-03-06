use arrayvec::ArrayString;
use bitflags::bitflags;
use std::{path::MAIN_SEPARATOR, time};
use strum::{EnumCount, EnumProperty, IntoEnumIterator};
use strum_macros::{EnumCount, EnumProperty, EnumIter};

use crate::{
    engine::time::Seconds,
    utils::fixed_string::format_fixed_string,
    sound::{SoundSystem, SoundHandle, SoundKind, SoundKey, SfxSoundKey},
};

// ----------------------------------------------
// UiSound
// ----------------------------------------------

pub struct UiSound {
    key: SfxSoundKey,
    handle: SoundHandle,

    last_play_time: Option<time::Instant>,
    cooldown_secs: Seconds, // Only plays if at least this many seconds of cooldown have elapsed since last time played.
}

impl UiSound {
    pub fn unloaded() -> Self {
        Self {
            key: SfxSoundKey::invalid(),
            handle: SoundHandle::invalid(SoundKind::Sfx),
            last_play_time: None,
            cooldown_secs: 0.0,
        }
    }

    pub fn load(sfx_path: &str, cooldown: Seconds, sound_sys: &mut SoundSystem) -> Self {
        debug_assert!(!sfx_path.is_empty());
        debug_assert!(cooldown >= 0.0);

        let path = format_fixed_string!(128, "ui{MAIN_SEPARATOR}{sfx_path}");

        Self {
            key: sound_sys.load_sfx(&path),
            handle: SoundHandle::invalid(SoundKind::Sfx), // Handle set when we first play the sound.
            last_play_time: None, // Never played.
            cooldown_secs: cooldown,
        }
    }

    pub fn play(&mut self, sound_sys: &mut SoundSystem) {
        if self.key.is_valid() && !self.is_playing(sound_sys) {
            let time_now = time::Instant::now();

            let cooldown_elapsed = if let Some(last_play_time) = self.last_play_time {
                let time_elapsed = time_now - last_play_time;
                time_elapsed.as_secs_f32() >= self.cooldown_secs
            } else {
                true
            };

            if cooldown_elapsed {
                const LOOPING: bool = false;
                self.handle = sound_sys.play_sfx(self.key, LOOPING);
                self.last_play_time = Some(time_now);
            }
        }
    }

    #[inline]
    pub fn is_playing(&self, sound_sys: &SoundSystem) -> bool {
        sound_sys.is_playing(self.handle)
    }
}

// ----------------------------------------------
// UiButtonSound
// ----------------------------------------------

const UI_BUTTON_SOUND_COUNT: usize = UiButtonSound::COUNT;

#[derive(Copy, Clone, PartialEq, Eq, EnumCount, EnumProperty, EnumIter)]
pub enum UiButtonSound {
    #[strum(props(Sfx = "hovered", Format = "wav"))]
    Hovered,

    #[strum(props(Sfx = "pressed", Format = "wav"))]
    Pressed,
}

impl UiButtonSound {
    pub fn path(self, sfx_path: &str) -> ArrayString<128> {
        let sfx = self.get_str("Sfx").unwrap();
        let fmt = self.get_str("Format").unwrap();

        // E.g.: "ui/buttons/default/pressed.wav"
        format_fixed_string!(128, "ui{MAIN_SEPARATOR}buttons{MAIN_SEPARATOR}{sfx_path}{MAIN_SEPARATOR}{sfx}.{fmt}")
    }

    // Fire and forget.
    pub fn play(self, sfx_path: &str, sound_sys: &mut SoundSystem) -> SoundHandle {
        debug_assert!(!sfx_path.is_empty());
        let sound_key = sound_sys.load_sfx(&self.path(sfx_path));

        const LOOPING: bool = false;
        sound_sys.play_sfx(sound_key, LOOPING)
    }
}

// ----------------------------------------------
// UiButtonSoundsEnabled
// ----------------------------------------------

bitflags! {
    #[derive(Copy, Clone)]
    pub struct UiButtonSoundsEnabled: u8 {
        const Hovered = 1 << UiButtonSound::Hovered as usize;
        const Pressed = 1 << UiButtonSound::Pressed as usize;
    }
}

// ----------------------------------------------
// UiButtonSounds
// ----------------------------------------------

pub struct UiButtonSounds {
    keys: [SfxSoundKey; UI_BUTTON_SOUND_COUNT],
    handles: [SoundHandle; UI_BUTTON_SOUND_COUNT],
    enabled: UiButtonSoundsEnabled,

    last_play_times: [Option<time::Instant>; UI_BUTTON_SOUND_COUNT],
    cooldown_secs: Seconds, // Only plays if at least this many seconds of cooldown have elapsed since last time played.
}

impl UiButtonSounds {
    pub fn unloaded() -> Self {
        Self {
            keys: [SfxSoundKey::invalid(); UI_BUTTON_SOUND_COUNT],
            handles: [SoundHandle::invalid(SoundKind::Sfx); UI_BUTTON_SOUND_COUNT],
            enabled: UiButtonSoundsEnabled::empty(),
            last_play_times: [None; UI_BUTTON_SOUND_COUNT],
            cooldown_secs: 0.0,
        }
    }

    pub fn load(sfx_path: &str, cooldown: Seconds, enabled: UiButtonSoundsEnabled, sound_sys: &mut SoundSystem) -> Self {
        debug_assert!(!sfx_path.is_empty());
        debug_assert!(cooldown >= 0.0);

        let mut sounds = Self::unloaded();

        sounds.cooldown_secs = cooldown;
        sounds.enabled = enabled;

        for sound in UiButtonSound::iter() {
            let index = sound as usize;
            if (enabled.bits() & (1 << index)) == 0 {
                continue; // Button sound not enabled.
            }

            let path = sound.path(sfx_path);
            sounds.keys[index] = sound_sys.load_sfx(&path);
        }

        sounds
    }

    pub fn play(&mut self, sound_sys: &mut SoundSystem, sound: UiButtonSound) {
        let index = sound as usize;
        if (self.enabled.bits() & (1 << index)) == 0 {
            return; // Button sound not enabled.
        }

        let sound_key = self.keys[index];
        if sound_key.is_valid() && !self.is_any_playing(sound_sys) {
            let time_now = time::Instant::now();

            let cooldown_elapsed = if let Some(last_play_time) = self.last_play_times[index] {
                let time_elapsed = time_now - last_play_time;
                time_elapsed.as_secs_f32() >= self.cooldown_secs
            } else {
                true
            };

            if cooldown_elapsed {
                const LOOPING: bool = false;
                let sound_handle = sound_sys.play_sfx(sound_key, LOOPING);
                self.handles[index] = sound_handle;
                self.last_play_times[index] = Some(time_now);
            }
        }
    }

    pub fn is_playing(&self, sound_sys: &SoundSystem, sound: UiButtonSound) -> bool {
        let sound_handle = self.handles[sound as usize];
        sound_sys.is_playing(sound_handle)
    }

    pub fn is_any_playing(&self, sound_sys: &SoundSystem) -> bool {
        for sound_handle in self.handles {
            if sound_sys.is_playing(sound_handle) {
                return true;
            }
        }
        false
    }
}
