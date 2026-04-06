use std::any::Any;

use common::Color;
use engine::{
    Engine,
    file_sys::paths::PathRef,
    log,
    sound::{MusicSoundKey, SoundHandle, SoundKey, SoundKind, SoundSystem},
};
use serde::{Deserialize, Serialize};
use strum::{Display, EnumCount, EnumIter, EnumProperty, IntoEnumIterator};

use super::GameSystem;
use crate::{GameLoop, config::GameConfigs, save_context::PostLoadContext, sim::SimContext};

// ----------------------------------------------
// MusicTrackKey
// ----------------------------------------------

#[derive(Copy, Clone, PartialEq, Eq, Display, EnumCount, EnumProperty, EnumIter)]
enum MusicTrackKey {
    // Home menu track.
    #[strum(props(TrackPath = "dynastys_legacy_1.mp3"))]
    HomeMenu,

    // In-game default track.
    #[strum(props(TrackPath = "dynastys_legacy_2.mp3"))]
    InGame,
}

impl MusicTrackKey {
    fn track_path(self) -> PathRef<'static> {
        PathRef::from_str(self.get_str("TrackPath").unwrap())
    }
}

// ----------------------------------------------
// MusicTrack
// ----------------------------------------------

const MUSIC_TRACK_COUNT: usize = MusicTrackKey::COUNT;

struct MusicTrack {
    key: MusicSoundKey,
    handle: SoundHandle,
}

impl MusicTrack {
    fn load(sound_sys: &mut SoundSystem, track_path: PathRef) -> Self {
        debug_assert!(!track_path.is_empty());
        Self {
            // All music track assets are under "music/{track_path}"
            key: sound_sys.load_music(track_path),
            handle: SoundHandle::invalid(SoundKind::Music), // Handle set when we first play the track.
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

        self.handle = sound_sys.play_music(self.key, looping);
    }
}

// ----------------------------------------------
// GameState
// ----------------------------------------------

#[derive(Copy, Clone, Default, PartialEq, Eq, Display)]
enum GameState {
    #[default]
    Unknown,
    HomeMenu,
    InGame,
}

impl GameState {
    fn query() -> Self {
        let game_loop = GameLoop::get();
        if game_loop.is_in_home_menu() {
            Self::HomeMenu
        } else if game_loop.is_in_game() {
            Self::InGame
        } else {
            Self::Unknown
        }
    }
}

// ----------------------------------------------
// AmbientMusicSystem
// ----------------------------------------------

#[derive(Default, Serialize, Deserialize)]
pub struct AmbientMusicSystem {
    #[serde(skip)]
    tracks: Vec<MusicTrack>,

    #[serde(skip)]
    current_track_playing: Option<MusicTrackKey>,

    #[serde(skip)]
    current_game_state: GameState,
}

impl GameSystem for AmbientMusicSystem {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn update(&mut self, engine: &mut Engine, _query: &SimContext) {
        if !self.is_enabled() {
            return;
        }

        let sound_sys = engine.sound_system_mut();

        if !self.tracks_are_loaded() {
            self.load_tracks(sound_sys);
        }

        let track_is_playing   = self.update_current_track(sound_sys);
        let game_state_changed = self.update_game_state();

        // If nothing is currently playing or if the game state has changed, start a new track.
        if !track_is_playing || game_state_changed {
            self.start_new_track(sound_sys);
        }
    }

    fn paused_update(&mut self, engine: &mut Engine, context: &SimContext) {
        // We want to update as normal when paused since the home menu will pause the game simulation.
        self.update(engine, context);
    }

    fn reset(&mut self, engine: &mut Engine) {
        self.stop_music(engine.sound_system_mut());
        self.current_game_state = GameState::default();
    }

    fn post_load(&mut self, context: &mut PostLoadContext) {
        // Just reset and stop whatever is playing.
        // Next update will take care of starting a new track.
        self.reset(context.engine_mut());
    }

    fn draw_debug_ui(&mut self, engine: &mut Engine, _query: &SimContext) {
        let ui = engine.ui_system().ui();

        if !self.is_enabled() {
            ui.text_colored(Color::red().to_array(), "AmbientMusicSystem DISABLED.");
            return;
        }

        if let Some(key) = self.current_track_playing {
            ui.text(format!("Current Track Playing: {} ('{}')", key, key.track_path()));
        } else {
            ui.text("Current Track Playing: None");
        }

        ui.text(format!("Current Game State: {}", self.current_game_state));
        ui.separator();

        if ui.button("Reset Track") {
            self.reset(engine);
        }
    }
}

impl AmbientMusicSystem {
    #[inline]
    fn is_enabled(&self) -> bool {
        !GameConfigs::get().debug.disable_ambient_music
    }

    #[inline]
    fn track(&self, key: MusicTrackKey) -> &MusicTrack {
        debug_assert!(self.tracks_are_loaded());
        &self.tracks[key as usize]
    }

    #[inline]
    fn track_mut(&mut self, key: MusicTrackKey) -> &mut MusicTrack {
        debug_assert!(self.tracks_are_loaded());
        &mut self.tracks[key as usize]
    }

    #[inline]
    fn tracks_are_loaded(&self) -> bool {
        !self.tracks.is_empty()
    }

    fn load_tracks(&mut self, sound_sys: &mut SoundSystem) {
        log::info!(log::channel!("ambient_music"), "Loading ambient music tracks...");

        debug_assert!(!self.tracks_are_loaded(), "Ambient music tracks already loaded!");
        self.tracks.reserve(MUSIC_TRACK_COUNT);

        for key in MusicTrackKey::iter() {
            let track = MusicTrack::load(sound_sys, key.track_path());
            self.tracks.push(track);
        }
    }

    fn play_track(&mut self, sound_sys: &mut SoundSystem, key: MusicTrackKey) {
        log::verbose!(log::channel!("ambient_music"), "Starting music track {} ('{}')", key, key.track_path());

        const LOOPING: bool = false;
        self.track_mut(key).play(sound_sys, LOOPING);

        self.current_track_playing = Some(key);
    }

    fn stop_music(&mut self, sound_sys: &mut SoundSystem) {
        sound_sys.stop_music();
        self.current_track_playing = None;
    }

    fn start_new_track(&mut self, sound_sys: &mut SoundSystem) {
        self.stop_music(sound_sys);

        match self.current_game_state {
            GameState::HomeMenu => self.play_track(sound_sys, MusicTrackKey::HomeMenu),
            GameState::InGame   => self.play_track(sound_sys, MusicTrackKey::InGame),
            GameState::Unknown  => {}, // Silence.
        }
    }

    fn update_current_track(&mut self, sound_sys: &SoundSystem) -> bool {
        if let Some(key) = self.current_track_playing {
            if !self.track(key).is_playing(sound_sys) {
                self.current_track_playing = None;
            }
        }

        self.current_track_playing.is_some() // == is playing
    }

    fn update_game_state(&mut self) -> bool {
        let new_state = GameState::query();
        let mut state_changed = false;

        if self.current_game_state != new_state {
            self.current_game_state = new_state;
            state_changed = true;
        }

        state_changed
    }
}
