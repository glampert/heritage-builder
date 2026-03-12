use std::any::Any;
use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumProperty, EnumIter, Display};
use strum::{EnumCount, EnumProperty, IntoEnumIterator};

use super::GameSystem;
use crate::{
    log,
    utils::Color,
    engine::Engine,
    save::PostLoadContext,
    game::{config::GameConfigs, sim::SimContext},
    sound::{SoundSystem, SoundHandle, SoundKind, SoundKey, AmbienceSoundKey},
};

// ----------------------------------------------
// AmbientSoundKey
// ----------------------------------------------

#[derive(Copy, Clone, PartialEq, Eq, Display, EnumCount, EnumProperty, EnumIter)]
enum AmbientSoundKey {
    #[strum(props(SoundPath = "birds_chirping.mp3"))]
    BirdsChirping,
}

impl AmbientSoundKey {
    fn sound_path(self) -> &'static str {
        self.get_str("SoundPath").unwrap()
    }
}

// ----------------------------------------------
// AmbientSound
// ----------------------------------------------

const AMBIENT_SOUND_COUNT: usize = AmbientSoundKey::COUNT;

struct AmbientSound {
    key: AmbienceSoundKey,
    handle: SoundHandle,
}

impl AmbientSound {
    fn load(sound_sys: &mut SoundSystem, sound_path: &str) -> Self {
        debug_assert!(!sound_path.is_empty());
        Self {
            // All ambience sound assets are under "ambience/{sound_path}"
            key: sound_sys.load_ambience(sound_path),
            handle: SoundHandle::invalid(SoundKind::Ambience), // Handle set when we first play the sound.
        }
    }

    fn is_loaded(&self) -> bool {
        self.key.is_valid()
    }

    fn is_playing(&self, sound_sys: &SoundSystem) -> bool {
        sound_sys.is_playing(self.handle)
    }

    fn play(&mut self, sound_sys: &mut SoundSystem, looping: bool) {
        if !self.is_loaded() {
            return;
        }

        self.handle = sound_sys.play_ambience(self.key, looping);
    }
}

// ----------------------------------------------
// AmbientSoundsSystem
// ----------------------------------------------

#[derive(Default, Serialize, Deserialize)]
pub struct AmbientSoundsSystem {
    #[serde(skip)]
    sounds: Vec<AmbientSound>,

    #[serde(skip)]
    current_sound_playing: Option<AmbientSoundKey>,
}

impl GameSystem for AmbientSoundsSystem {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn update(&mut self, engine: &mut dyn Engine, _query: &SimContext) {
        if !self.is_enabled() {
            return;
        }

        let sound_sys = engine.sound_system();

        if !self.sounds_are_loaded() {
            self.load_sounds(sound_sys);
        }

        let sound_is_playing   = self.update_current_sound(sound_sys);
        let game_state_changed = self.update_game_state();

        // If nothing is currently playing or if the game state has changed, start a new sound.
        if !sound_is_playing || game_state_changed {
            self.start_new_sound(sound_sys);
        }
    }

    fn reset(&mut self, engine: &mut dyn Engine) {
        self.stop_sounds(engine.sound_system());
    }

    fn post_load(&mut self, context: &PostLoadContext) {
        // Just reset and stop whatever is playing.
        // Next update will take care of starting a new sound.
        self.reset(context.engine_mut());
    }

    fn draw_debug_ui(&mut self, engine: &mut dyn Engine, _query: &SimContext) {
        let ui = engine.ui_system().ui();

        if !self.is_enabled() {
            ui.text_colored(Color::red().to_array(), "AmbientSoundsSystem DISABLED.");
            return;
        }

        if let Some(key) = self.current_sound_playing {
            ui.text(format!("Current Ambient Sound Playing: {} ('{}')", key, key.sound_path()));
        } else {
            ui.text("Current Ambient Sound Playing: None");
        }

        if ui.button("Reset Ambient Sounds") {
            self.reset(engine);
        }
    }
}

impl AmbientSoundsSystem {
    #[inline]
    fn is_enabled(&self) -> bool {
        !GameConfigs::get().debug.disable_ambient_sounds
    }

    #[inline]
    fn sound(&self, key: AmbientSoundKey) -> &AmbientSound {
        debug_assert!(self.sounds_are_loaded());
        &self.sounds[key as usize]
    }

    #[inline]
    fn sound_mut(&mut self, key: AmbientSoundKey) -> &mut AmbientSound {
        debug_assert!(self.sounds_are_loaded());
        &mut self.sounds[key as usize]
    }

    #[inline]
    fn sounds_are_loaded(&self) -> bool {
        !self.sounds.is_empty()
    }

    fn load_sounds(&mut self, sound_sys: &mut SoundSystem) {
        log::info!(log::channel!("ambient_sounds"), "Loading ambient sounds...");

        debug_assert!(!self.sounds_are_loaded(), "Ambient sounds already loaded!");
        self.sounds.reserve(AMBIENT_SOUND_COUNT);

        for key in AmbientSoundKey::iter() {
            let sound = AmbientSound::load(sound_sys, key.sound_path());
            self.sounds.push(sound);
        }
    }

    fn play_sound(&mut self, sound_sys: &mut SoundSystem, key: AmbientSoundKey) {
        log::verbose!(log::channel!("ambient_sounds"), "Starting ambient sound {} ('{}')", key, key.sound_path());

        const LOOPING: bool = false;
        self.sound_mut(key).play(sound_sys, LOOPING);

        self.current_sound_playing = Some(key);
    }

    fn stop_sounds(&mut self, sound_sys: &mut SoundSystem) {
        sound_sys.stop_ambience();
        self.current_sound_playing = None;
    }

    fn start_new_sound(&mut self, sound_sys: &mut SoundSystem) {
        self.stop_sounds(sound_sys);

        // TODO: Choose sound to play based on game state.
        self.play_sound(sound_sys, AmbientSoundKey::BirdsChirping);
    }

    fn update_current_sound(&mut self, sound_sys: &SoundSystem) -> bool {
        if let Some(key) = self.current_sound_playing {
            if !self.sound(key).is_playing(sound_sys) {
                self.current_sound_playing = None;
            }
        }

        self.current_sound_playing.is_some() // == is playing
    }

    fn update_game_state(&mut self) -> bool {
        // TODO: Query game state so we can choose an ambient sound based on it.
        false
    }
}
