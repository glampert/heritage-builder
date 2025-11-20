use slab::Slab;
use smallvec::SmallVec;
use proc_macros::DrawDebugUi;

use std::{
    time::Duration,
    path::{Path, PathBuf, MAIN_SEPARATOR},
};

use kira::{
    backend::DefaultBackend,
    AudioManager, AudioManagerSettings,
    Tween, track::{TrackHandle, TrackBuilder},
    sound::{
        PlaybackState, FromFileError,
        static_sound::{StaticSoundData, StaticSoundHandle},
        streaming::{StreamingSoundData, StreamingSoundHandle},
    },
};

use crate::{
    log,
    engine::time::Seconds,
    imgui_ui::{self, UiSystem},
    utils::{
        hash::{self, StringHash, PreHashedKeyMap},
        Vec2, platform::paths, mem
    },
};

// ----------------------------------------------
// SoundKeys / SoundKind
// ----------------------------------------------

macro_rules! declare_sound_keys {
    ($($struct_name:ident),* $(,)?) => {
        $(
            #[derive(Copy, Clone)]
            pub struct $struct_name {
                hash: StringHash,
            }

            impl $struct_name {
                #[inline]
                fn new(hash: StringHash) -> Self {
                    Self { hash }
                }

                #[inline]
                fn invalid() -> Self {
                    Self { hash: hash::NULL_HASH }
                }

                #[inline]
                fn is_valid(&self) -> bool {
                    self.hash != hash::NULL_HASH
                }
            }
        )*
    };
}

declare_sound_keys! {
    SfxSoundKey,
    AmbienceSoundKey,
    MusicSoundKey,
    NarrationSoundKey,
}

#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Eq)]
enum SoundKind {
    Sfx,
    Ambience,
    SpatialAmbience,
    Music,
    Narration,
}

// ----------------------------------------------
// SoundHandle
// ----------------------------------------------

#[derive(Copy, Clone)]
pub struct SoundHandle {
    kind: SoundKind,
    index: u32,
    generation: u32,
}

impl SoundHandle {
    #[inline]
    fn new(kind: SoundKind, index: usize, generation: u32) -> Self {
        debug_assert!(index < u32::MAX as usize);
        debug_assert!(generation != 0);
        Self {
            kind,
            index: index.try_into().expect("SoundHandle index cannot fit in a u32!"),
            generation,
        }
    }

    #[inline]
    fn invalid(kind: SoundKind) -> Self {
        Self { kind, index: u32::MAX, generation: 0 }
    }

    #[inline]
    fn is_valid(&self) -> bool {
        self.index < u32::MAX && self.generation != 0
    }
}

// ----------------------------------------------
// SoundGlobalSettings
// ----------------------------------------------

#[derive(Copy, Clone, PartialEq, DrawDebugUi)]
pub struct SoundGlobalSettings {
    // Master volumes:
    #[debug_ui(edit, widget = "slider", min = "0", max = "1")]
    pub spatial_master_volume: f32,

    #[debug_ui(edit, widget = "slider", min = "0", max = "1")]
    pub ambience_master_volume: f32,

    #[debug_ui(edit, widget = "slider", min = "0", max = "1")]
    pub music_master_volume: f32,

    #[debug_ui(edit, widget = "slider", min = "0", max = "1")]
    pub narration_master_volume: f32,

    #[debug_ui(edit, widget = "slider", min = "0", max = "1", separator)]
    pub sfx_master_volume: f32,

    // Cutoff distance from the camera where we mute spatial sounds.
    #[debug_ui(edit, widget = "slider", min = "0", max = "1000")]
    pub spatial_cutoff_distance: f32,
    #[debug_ui(edit, widget = "slider", min = "0", max = "10", separator)]
    pub spatial_transition_secs: Seconds,

    // Fade times:
    #[debug_ui(edit, widget = "slider", min = "0", max = "10")]
    pub spatial_fade_in_secs: Seconds,
    #[debug_ui(edit, widget = "slider", min = "0", max = "10")]
    pub spatial_fade_out_secs: Seconds,

    #[debug_ui(edit, widget = "slider", min = "0", max = "10")]
    pub ambience_fade_in_secs: Seconds,
    #[debug_ui(edit, widget = "slider", min = "0", max = "10")]
    pub ambience_fade_out_secs: Seconds,

    #[debug_ui(edit, widget = "slider", min = "0", max = "10")]
    pub music_fade_in_secs: Seconds,
    #[debug_ui(edit, widget = "slider", min = "0", max = "10")]
    pub music_fade_out_secs: Seconds,

    #[debug_ui(edit, widget = "slider", min = "0", max = "10")]
    pub narration_fade_in_secs: Seconds,
    #[debug_ui(edit, widget = "slider", min = "0", max = "10")]
    pub narration_fade_out_secs: Seconds,

    #[debug_ui(edit, widget = "slider", min = "0", max = "10")]
    pub sfx_fade_in_secs: Seconds,
    #[debug_ui(edit, widget = "slider", min = "0", max = "10", separator)]
    pub sfx_fade_out_secs: Seconds,
}

impl Default for SoundGlobalSettings {
    fn default() -> Self {
        Self {
            // Master volumes:
            spatial_master_volume: 1.0,
            ambience_master_volume: 1.0,
            music_master_volume: 1.0,
            narration_master_volume: 1.0,
            sfx_master_volume: 1.0,

            // Spatial ambience:
            spatial_cutoff_distance: 500.0,
            spatial_transition_secs: 0.5,

            // Fade times:
            spatial_fade_in_secs: 1.0,
            spatial_fade_out_secs: 2.0,
            ambience_fade_in_secs: 1.0,
            ambience_fade_out_secs: 2.0,
            music_fade_in_secs: 1.0,
            music_fade_out_secs: 3.0,
            narration_fade_in_secs: 1.0,
            narration_fade_out_secs: 3.0,
            sfx_fade_in_secs: 0.0,
            sfx_fade_out_secs: 0.0,
        }
    }
}

// ----------------------------------------------
// SoundSystem
// ----------------------------------------------

pub struct SoundSystem {
    backend: Option<Box<SoundSystemBackend>>,
    registry: SoundAssetRegistry,
    settings: SoundGlobalSettings,
}

impl SoundSystem {
    pub fn new() -> Self {
        Self {
            backend: SoundSystemBackend::new(),
            registry: SoundAssetRegistry::new(),
            settings: SoundGlobalSettings::default(),
        }
    }

    #[inline]
    pub fn is_initialized(&self) -> bool {
        self.backend.is_some()
    }

    pub fn settings(&self) -> &SoundGlobalSettings {
        &self.settings
    }

    pub fn settings_mut(&mut self) -> &mut SoundGlobalSettings {
        &mut self.settings
    }

    pub fn update(&mut self, listener_position: Vec2) {
        if let Some(backend) = &mut self.backend {
            backend.update(listener_position, &self.settings);
        }
        // Else if backend failed to initialize we'll operate as a no-op/null SoundSystem.
    }

    // ----------------------
    // Sound Loading:
    // ----------------------

    pub fn load_sfx(&mut self, path: &str) -> SfxSoundKey {
        if !self.is_initialized() {
            return SfxSoundKey::invalid();
        }
        self.registry.load_sfx(path)
    }

    pub fn load_ambience(&mut self, path: &str) -> AmbienceSoundKey {
        if !self.is_initialized() {
            return AmbienceSoundKey::invalid();
        }
        self.registry.load_ambience(path)
    }

    pub fn load_music(&mut self, path: &str) -> MusicSoundKey {
        if !self.is_initialized() {
            return MusicSoundKey::invalid();
        }
        self.registry.load_music(path)
    }

    pub fn load_narration(&mut self, path: &str) -> NarrationSoundKey {
        if !self.is_initialized() {
            return NarrationSoundKey::invalid();
        }
        self.registry.load_narration(path)
    }

    // ----------------------
    // Sound Play/Stop:
    // ----------------------

    pub fn play_sfx(&mut self, sound_key: SfxSoundKey, looping: bool) -> SoundHandle {
        if let Some(backend) = &mut self.backend {
            if let Some(sound) = self.registry.sfx.get(&sound_key.hash) {
                return backend.sfx.play(sound,
                                        self.settings.sfx_master_volume,
                                        self.settings.sfx_fade_in_secs,
                                        looping);
            }
        }
        SoundHandle::invalid(SoundKind::Sfx)
    }

    pub fn play_ambience(&mut self, sound_key: AmbienceSoundKey, looping: bool) -> SoundHandle {
        if let Some(backend) = &mut self.backend {
            if let Some(sound) = self.registry.ambience.get(&sound_key.hash) {
                return backend.ambience.play(sound,
                                             self.settings.ambience_master_volume,
                                             self.settings.ambience_fade_in_secs,
                                             looping);
            }
        }
        SoundHandle::invalid(SoundKind::Ambience)
    }

    pub fn play_spatial_ambience(&mut self, sound_key: AmbienceSoundKey, world_position: Vec2, looping: bool) -> SoundHandle {
        if let Some(backend) = &mut self.backend {
            if let Some(sound) = self.registry.ambience.get(&sound_key.hash) { // Same as ambience.
                return backend.spatial.play(sound,
                                            world_position,
                                            self.settings.spatial_master_volume,
                                            self.settings.spatial_fade_in_secs,
                                            looping);
            }
        }
        SoundHandle::invalid(SoundKind::SpatialAmbience)
    }

    pub fn play_music(&mut self, sound_key: MusicSoundKey, looping: bool) -> SoundHandle {
        if let Some(backend) = &mut self.backend {
            if let Some(sound) = self.registry.music.get(&sound_key.hash) {
                return backend.music.play(sound,
                                          self.settings.music_master_volume,
                                          self.settings.music_fade_in_secs,
                                          self.settings.music_fade_out_secs,
                                          looping);
            }
        }
        SoundHandle::invalid(SoundKind::Music)
    }

    pub fn play_narration(&mut self, sound_key: NarrationSoundKey, looping: bool) -> SoundHandle {
        if let Some(backend) = &mut self.backend {
            if let Some(sound) = self.registry.narration.get(&sound_key.hash) {
                return backend.narration.play(sound,
                                              self.settings.narration_master_volume,
                                              self.settings.narration_fade_in_secs,
                                              self.settings.narration_fade_out_secs,
                                              looping);
            }
        }
        SoundHandle::invalid(SoundKind::Narration)
    }

    // Stop all sounds on these tracks.
    pub fn stop_sfx(&mut self) {
        if let Some(backend) = &mut self.backend {
            backend.sfx.stop_all(self.settings.sfx_fade_out_secs);
        }
    }

    pub fn stop_ambience(&mut self) {
        if let Some(backend) = &mut self.backend {
            backend.ambience.stop_all(self.settings.ambience_fade_out_secs);
        }
    }

    pub fn stop_spatial_ambience(&mut self) {
        if let Some(backend) = &mut self.backend {
            backend.spatial.stop_all(self.settings.spatial_fade_out_secs);
        }
    }

    pub fn stop_music(&mut self) {
        if let Some(backend) = &mut self.backend {
            backend.music.stop_all(self.settings.music_fade_out_secs);
        }
    }

    pub fn stop_narration(&mut self) {
        if let Some(backend) = &mut self.backend {
            backend.narration.stop_all(self.settings.narration_fade_out_secs);
        }
    }

    // Stop specific sound by handle.
    pub fn stop(&mut self, sound_handle: SoundHandle) {
        if let Some(backend) = &mut self.backend {
            match sound_handle.kind {
                SoundKind::Sfx => {
                    backend.sfx.stop(sound_handle, self.settings.sfx_fade_out_secs);
                }
                SoundKind::Ambience => {
                    backend.ambience.stop(sound_handle, self.settings.ambience_fade_out_secs);
                }
                SoundKind::SpatialAmbience => {
                    backend.spatial.stop(sound_handle, self.settings.spatial_fade_out_secs);
                }
                SoundKind::Music => {
                    backend.music.stop(sound_handle, self.settings.music_fade_out_secs);
                }
                SoundKind::Narration => {
                    backend.narration.stop(sound_handle, self.settings.narration_fade_out_secs);
                }
            }
        }
    }

    // Stops all currently playing sounds.
    pub fn stop_all(&mut self) {
        if let Some(backend) = &mut self.backend {
            backend.sfx.stop_all(self.settings.sfx_fade_out_secs);
            backend.ambience.stop_all(self.settings.ambience_fade_out_secs);
            backend.spatial.stop_all(self.settings.spatial_fade_out_secs);
            backend.music.stop_all(self.settings.music_fade_out_secs);
            backend.narration.stop_all(self.settings.narration_fade_out_secs);
        }
    }

    // ----------------------
    // Debug UI:
    // ----------------------

    pub fn draw_debug_ui(&mut self, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();

        if !self.is_initialized() {
            ui.text("No Sound System available!");
            return;
        }

        let backend = self.backend.as_mut().unwrap();

        ui.text(format!("Listener Pos   : {}", backend.listener_position()));
        ui.text(format!("Sounds Playing : {}", backend.sounds_playing()));
        ui.text(format!("Sounds Loaded  : {}", self.registry.sounds_loaded()));

        ui.separator();

        let mut new_settings = self.settings;
        new_settings.draw_debug_ui(ui_sys);

        // Only update volumes if anything was changed.
        if new_settings != self.settings {
            self.settings = new_settings;
            backend.set_volumes(&new_settings);
        }

        ui.separator();

        if ui.collapsing_header("Debug", imgui::TreeNodeFlags::empty()) {
            #[allow(static_mut_refs)]
            let looping = unsafe {
                static mut LOOPING: bool = false;
                ui.checkbox("Looping", &mut LOOPING);
                LOOPING
            };

            ui.text("SFX:");

            if ui.button("Play SFX (bleep)") {
                let sound_key = self.load_sfx("test/bleep.ogg");
                self.play_sfx(sound_key, looping);
            }

            if ui.button("Play SFX (drums)") {
                let sound_key = self.load_sfx("test/drums.ogg");
                self.play_sfx(sound_key, looping);
            }

            if ui.button("Stop All SFX") {
                self.stop_sfx();
            }

            ui.text("Music:");

            if ui.button("Play Music") {
                let sound_key = self.load_music("dynasty_legacy.mp3");
                self.play_music(sound_key, looping);
            }

            if ui.button("Stop All Music") {
                self.stop_music();
            }

            ui.text("Ambience:");

            if ui.button("Play Ambience") {
                let sound_key = self.load_ambience("birds_chirping_ambiance.mp3");
                self.play_ambience(sound_key, looping);
            }

            if ui.button("Stop All Ambience") {
                self.stop_ambience();
            }

            #[allow(static_mut_refs)]
            let spatial_origin = unsafe {
                static mut SPATIAL_ORIGIN: Vec2 = Vec2::zero();
                imgui_ui::input_f32_xy(ui, "Spatial:", &mut SPATIAL_ORIGIN, false, None, None);
                SPATIAL_ORIGIN
            };

            if ui.button("Play Spatial") {
                let sound_key = self.load_ambience("birds_chirping_ambiance.mp3");
                self.play_spatial_ambience(sound_key, spatial_origin, looping);
            }

            if ui.button("Stop All Spatial") {
                self.stop_spatial_ambience();
            }
        }
    }
}

// ----------------------------------------------
// SoundSystemBackend
// ----------------------------------------------

struct SoundSystemBackend {
    // Kira API:
    manager: AudioManager,

    // Controllers:
    sfx: SfxController,
    ambience: AmbienceController,
    spatial: SpatialController, // NOTE: Spatial sounds share the ambience track.
    music: MusicController,
    narration: NarrationController,

    // Stats/debug:
    listener_position: Vec2,
}

impl SoundSystemBackend {
    fn new() -> Option<Box<Self>> {
        let mut manager = match AudioManager::<DefaultBackend>::new(AudioManagerSettings::default()) {
            Ok(manager) => manager,
            Err(err) => {
                log::error!(log::channel!("sound"), "Failed to initialize Kira AudioManager: {err}");
                return None;
            }
        };

        let sfx_track = match manager.add_sub_track(TrackBuilder::default()) {
            Ok(track) => track,
            Err(err) => {
                log::error!(log::channel!("sound"), "Failed to create Kira Sub-Track for SFX: {err}");
                return None;
            }
        };

        let ambience_track = match manager.add_sub_track(TrackBuilder::default()) {
            Ok(track) => track,
            Err(err) => {
                log::error!(log::channel!("sound"), "Failed to create Kira Sub-Track for Ambience: {err}");
                return None;
            }
        };

        let music_track = match manager.add_sub_track(TrackBuilder::default()) {
            Ok(track) => track,
            Err(err) => {
                log::error!(log::channel!("sound"), "Failed to create Kira Sub-Track for Music: {err}");
                return None;
            }
        };

        let narration_track = match manager.add_sub_track(TrackBuilder::default()) {
            Ok(track) => track,
            Err(err) => {
                log::error!(log::channel!("sound"), "Failed to create Kira Sub-Track for Narration: {err}");
                return None;
            }
        };

        let sfx = SfxController::new(sfx_track, SoundKind::Sfx);
        let ambience = AmbienceController::new(ambience_track, SoundKind::Ambience);
        let spatial = SpatialController::new(&ambience.track); // NOTE: Track shared with ambience.
        let music = MusicController::new(music_track, SoundKind::Music);
        let narration = NarrationController::new(narration_track, SoundKind::Narration);

        Some(Box::new(Self {
            manager,
            sfx,
            ambience,
            spatial,
            music,
            narration,
            listener_position: Vec2::zero(),
        }))
    }

    fn update(&mut self, listener_position: Vec2, settings: &SoundGlobalSettings) {
        self.listener_position = listener_position;
        self.sfx.update();
        self.ambience.update();
        self.spatial.update(listener_position, settings);
        self.music.update();
        self.narration.update();
    }

    fn set_volumes(&mut self, settings: &SoundGlobalSettings) {
        self.sfx.set_volume(settings.sfx_master_volume);
        self.ambience.set_volume(settings.ambience_master_volume);
        self.spatial.set_volume(settings.spatial_master_volume);
        self.music.set_volume(settings.music_master_volume);
        self.narration.set_volume(settings.narration_master_volume);
    }

    fn listener_position(&self) -> Vec2 {
        self.listener_position
    }

    fn sounds_playing(&self) -> usize {
        self.sfx.playing_count()
        + self.ambience.playing_count()
        + self.spatial.playing_count()
        + self.music.playing_count()
        + self.narration.playing_count()
    }
}

#[inline]
fn linear_to_decibels(mut volume: f32) -> f32 {
    volume = volume.clamp(0.0, 1.0);

    // If near zero -> treat as silent.
    if volume <= 0.0001 {
        return -60.0;
    }

    20.0 * volume.log10()
}

// ----------------------------------------------
// SoundAssetRegistry
// ----------------------------------------------

// Stores loaded SFX, music, narration, etc.
#[derive(Default)]
struct SoundAssetRegistry {
    // SFX, Ambience, Spatial: StaticSoundData
    // Music & Narration: StreamingSoundData

    sfx: PreHashedKeyMap<StringHash, StaticSound>,
    ambience: PreHashedKeyMap<StringHash, StaticSound>, // Spatial sounds are the same as ambience.

    music: PreHashedKeyMap<StringHash, StreamedSound>,
    narration: PreHashedKeyMap<StringHash, StreamedSound>,

    paths: SoundAssetPaths,
}

struct StaticSound {
    path: String,
    data: StaticSoundData,
}

struct StreamedSound {
    path: String,
    // Data lazily loaded on first reference.
}

#[derive(Default)]
struct SoundAssetPaths {
    sfx: PathBuf,
    ambience: PathBuf,
    music: PathBuf,
    narration: PathBuf,
}

macro_rules! load_static_sound {
    ($sound_key_kind:ty, $hash_map:expr, $base_path:expr, $sound_path:expr) => {{
        debug_assert!(!$sound_path.is_empty());

        let sound_path = format!("{}{}{}", $base_path.to_str().unwrap(), MAIN_SEPARATOR, $sound_path);
        let sound_hash = hash::fnv1a_from_str(&sound_path);

        if $hash_map.get(&sound_hash).is_some() {
            return <$sound_key_kind>::new(sound_hash); // Already loaded.
        }

        let sound_data = match StaticSoundData::from_file(&sound_path) {
            Ok(sound_data) => sound_data,
            Err(err) => {
                log::error!(log::channel!("sound"), "Failed to load sound '{sound_path}': {err}");
                return <$sound_key_kind>::invalid();
            }
        };

        $hash_map.insert(sound_hash, StaticSound { path: sound_path, data: sound_data });
        <$sound_key_kind>::new(sound_hash)
    }};
}

macro_rules! load_streamed_sound {
    ($sound_key_kind:ty, $hash_map:expr, $base_path:expr, $sound_path:expr) => {{
        debug_assert!(!$sound_path.is_empty());

        let sound_path = format!("{}{}{}", $base_path.to_str().unwrap(), MAIN_SEPARATOR, $sound_path);
        let sound_hash = hash::fnv1a_from_str(&sound_path);

        if $hash_map.get(&sound_hash).is_some() {
            return <$sound_key_kind>::new(sound_hash); // Already loaded.
        }

        // Only probe file path now. Data is lazily loaded on first reference.
        if std::fs::exists(&sound_path).is_err() {
            log::error!(log::channel!("sound"), "Sound file path '{sound_path}' is invalid!");
            return <$sound_key_kind>::invalid();
        }

        $hash_map.insert(sound_hash, StreamedSound { path: sound_path });
        <$sound_key_kind>::new(sound_hash)
    }};
}

impl SoundAssetRegistry {
    fn new() -> Self {
        let mut registry = Self { ..Default::default() };
        registry.paths.sfx       = paths::asset_path(Path::new("sounds").join("sfx"));
        registry.paths.ambience  = paths::asset_path(Path::new("sounds").join("ambience"));
        registry.paths.music     = paths::asset_path(Path::new("sounds").join("music"));
        registry.paths.narration = paths::asset_path(Path::new("sounds").join("narration"));
        registry
    }

    fn load_sfx(&mut self, path: &str) -> SfxSoundKey {
        load_static_sound!(SfxSoundKey, self.sfx, self.paths.sfx, path)
    }

    fn load_ambience(&mut self, path: &str) -> AmbienceSoundKey {
        load_static_sound!(AmbienceSoundKey, self.ambience, self.paths.ambience, path)
    }

    fn load_music(&mut self, path: &str) -> MusicSoundKey {
        load_streamed_sound!(MusicSoundKey, self.music, self.paths.music, path)
    }

    fn load_narration(&mut self, path: &str) -> NarrationSoundKey {
        load_streamed_sound!(NarrationSoundKey, self.narration, self.paths.narration, path)
    }

    fn sounds_loaded(&self) -> usize {
        self.sfx.len() + self.ambience.len() + self.music.len() + self.narration.len()
    }
}

// ----------------------------------------------
// StaticSoundController
// ----------------------------------------------

struct StaticSoundController {
    track: Box<TrackHandle>,
    sounds: Slab<(u32, StaticSoundHandle)>,
    kind: SoundKind,
    generation: u32,
}

impl StaticSoundController {
    fn new(track: TrackHandle, kind: SoundKind) -> Self {
        Self {
            track: Box::new(track),
            sounds: Slab::new(),
            kind,
            generation: 0,
        }
    }

    fn play(&mut self,
            sound: &StaticSound,
            volume: f32,
            fade_in_secs: Seconds,
            looping: bool)
            -> SoundHandle
    {
        let sound_data = sound.data.volume(linear_to_decibels(volume));

        let mut handle = match self.track.play(
            sound_data.fade_in_tween(Tween {
                duration: Duration::from_secs_f32(fade_in_secs),
                ..Default::default()
        }))
        {
            Ok(handle) => handle,
            Err(err) => {
                log::warn!(log::channel!("sound"), "Failed to play sound effect '{}': {err}", sound.path);
                return SoundHandle::invalid(self.kind);
            }
        };

        if looping {
            // Play in a loop until told to stop.
            handle.set_loop_region(..);
        }

        self.generation += 1;
        let index = self.sounds.insert((self.generation, handle));

        SoundHandle::new(self.kind, index, self.generation)
    }

    fn stop(&mut self, sound_handle: SoundHandle, fade_out_secs: Seconds) {
        if !sound_handle.is_valid() || sound_handle.kind != self.kind {
            return;
        }

        if let Some((generation, handle)) = self.sounds.get_mut(sound_handle.index as usize) {
            if *generation == sound_handle.generation {
                handle.stop(Tween {
                    duration: Duration::from_secs_f32(fade_out_secs),
                    ..Default::default()
                });
                self.sounds.remove(sound_handle.index as usize);
            }
        }
    }

    fn stop_all(&mut self, fade_out_secs: Seconds) {
        for (_, (_, handle)) in &mut self.sounds {
            handle.stop(Tween {
                duration: Duration::from_secs_f32(fade_out_secs),
                ..Default::default()
            });
        }
        self.sounds.clear();
    }

    fn update(&mut self) {
        // Tidy up stopped sounds.
        let mut indices_to_remove: SmallVec<[usize; 32]> = SmallVec::new();

        for (index, (_, handle)) in &self.sounds {
            if handle.state() == PlaybackState::Stopped {
                indices_to_remove.push(index);
            }
        }

        for index in indices_to_remove {
            self.sounds.remove(index);
        }
    }

    fn set_volume(&mut self, volume: f32) {
        let volume_db = linear_to_decibels(volume);
        for (_, (_, handle)) in &mut self.sounds {
            if handle.state() != PlaybackState::Stopped {
                handle.set_volume(volume_db, Tween::default());
            }
        }
    }

    fn playing_count(&self) -> usize {
        self.sounds.len()
    }
}

// ----------------------------------------------
// StreamedSoundController
// ----------------------------------------------

struct StreamedSoundController {
    track: TrackHandle,

    // Only one streamed sound plays at a time.
    current: Option<StreamingSoundHandle<FromFileError>>,

    kind: SoundKind,
    generation: u32,
}

impl StreamedSoundController {
    fn new(track: TrackHandle, kind: SoundKind) -> Self {
        Self {
            track,
            current: None,
            kind,
            generation: 0,
        }
    }

    fn play(&mut self,
            sound: &StreamedSound,
            volume: f32,
            fade_in_secs: Seconds,
            fade_out_secs: Seconds,
            looping: bool)
            -> SoundHandle
    {
        debug_assert!(!sound.path.is_empty());

        let sound_data = match StreamingSoundData::from_file(&sound.path) {
            Ok(sound_data) => sound_data.volume(linear_to_decibels(volume)),
            Err(err) => {
                log::error!(log::channel!("sound"), "Failed to load streamed sound '{}': {err}", sound.path);
                return SoundHandle::invalid(self.kind);
            }
        };

        // Stop current if any is already playing.
        self.stop_all(fade_out_secs);

        let mut handle = match self.track.play(
            sound_data.fade_in_tween(Tween {
                duration: Duration::from_secs_f32(fade_in_secs),
                ..Default::default()
        }))
        {
            Ok(handle) => handle,
            Err(err) => {
                log::warn!(log::channel!("sound"), "Failed to play streamed sound '{}': {err}", sound.path);
                return SoundHandle::invalid(self.kind);
            }
        };

        if looping {
            // Play in a loop until told to stop.
            handle.set_loop_region(..);
        }

        self.generation += 1;
        self.current = Some(handle);

        SoundHandle::new(self.kind, 0, self.generation) // index not used here.
    }

    fn stop(&mut self, sound_handle: SoundHandle, fade_out_secs: Seconds) {
        if !sound_handle.is_valid()
            || sound_handle.kind != self.kind
            || sound_handle.generation != self.generation
        {
            return;
        }
        self.stop_all(fade_out_secs);
    }

    fn stop_all(&mut self, fade_out_secs: Seconds) {
        if let Some(handle) = &mut self.current {
            handle.stop(Tween {
                duration: Duration::from_secs_f32(fade_out_secs),
                ..Default::default()
            });
            self.current = None;
        }
    }

    fn update(&mut self) {
        if let Some(handle) = &self.current {
            if handle.state() == PlaybackState::Stopped {
                self.current = None;
            }
        }
    }

    fn set_volume(&mut self, volume: f32) {
        if let Some(handle) = &mut self.current {
            if handle.state() != PlaybackState::Stopped {
                let volume_db = linear_to_decibels(volume);
                handle.set_volume(volume_db, Tween::default());
            }
        }
    }

    fn playing_count(&self) -> usize {
        if self.current.is_some() { 1 } else { 0 }
    }
}

// ----------------------------------------------
// SpatialController
// ----------------------------------------------

// Fake 3D positional sound. Shares the ambiance track.
// Plays sounds that have a specific origin in the world. E.g.: fire, building collapsed.
struct SpatialController {
    track: mem::RawPtr<TrackHandle>, // NOTE: Owned by AmbienceController.
    sounds: Slab<(u32, SpatialSound)>,
    generation: u32,
}

struct SpatialSound {
    handle: StaticSoundHandle,
    position: Vec2,
    base_volume: f32, // linear volume [0-1] range.
}

impl SpatialController {
    fn new(track: &TrackHandle) -> Self {
        Self {
            track: mem::RawPtr::from_ref(track),
            sounds: Slab::new(),
            generation: 0,
        }
    }

    fn play(&mut self,
            sound: &StaticSound,
            position: Vec2,
            volume: f32,
            fade_in_secs: Seconds,
            looping: bool)
            -> SoundHandle
    {
        let sound_data = sound.data.volume(linear_to_decibels(volume));

        let mut handle = match self.track.play(
            sound_data.fade_in_tween(Tween {
                duration: Duration::from_secs_f32(fade_in_secs),
                ..Default::default()
        }))
        {
            Ok(handle) => handle,
            Err(err) => {
                log::warn!(log::channel!("sound"), "Failed to play spatial ambiance loop: {err}");
                return SoundHandle::invalid(SoundKind::SpatialAmbience);
            }
        };

        if looping {
            // Play in a loop until told to stop.
            handle.set_loop_region(..);
        }

        self.generation += 1;
        let index = self.sounds.insert((self.generation, SpatialSound { handle, position, base_volume: volume }));

        SoundHandle::new(SoundKind::SpatialAmbience, index, self.generation)
    }

    fn stop(&mut self, sound_handle: SoundHandle, fade_out_secs: Seconds) {
        if !sound_handle.is_valid() || sound_handle.kind != SoundKind::SpatialAmbience {
            return;
        }

        if let Some((generation, sound)) = self.sounds.get_mut(sound_handle.index as usize) {
            if *generation == sound_handle.generation {
                sound.handle.stop(Tween {
                    duration: Duration::from_secs_f32(fade_out_secs),
                    ..Default::default()
                });
                self.sounds.remove(sound_handle.index as usize);
            }
        }
    }

    fn stop_all(&mut self, fade_out_secs: Seconds) {
        for (_, (_, sound)) in &mut self.sounds {
            sound.handle.stop(Tween {
                duration: Duration::from_secs_f32(fade_out_secs),
                ..Default::default()
            });
        }
        self.sounds.clear();
    }

    fn update(&mut self, listener_position: Vec2, settings: &SoundGlobalSettings) {        
        let mut indices_to_remove: SmallVec<[usize; 32]> = SmallVec::new();

        for (index, (_, sound)) in &mut self.sounds {
            // Tidy up stopped sounds.
            if sound.handle.state() == PlaybackState::Stopped {
                indices_to_remove.push(index);
                continue;
            }

            let dx = sound.position.x - listener_position.x;
            let dy = sound.position.y - listener_position.y;

            let distance = ((dx * dx) + (dy * dy)).sqrt();
            let dist_factor = 1.0 - (distance / settings.spatial_cutoff_distance).clamp(0.0, 1.0);

            // Convert linear volume [0-1] to decibels:
            let volume_db = linear_to_decibels(sound.base_volume * dist_factor);
            sound.handle.set_volume(volume_db, Tween {
                duration: Duration::from_secs_f32(settings.spatial_transition_secs),
                ..Default::default()
            });

            let panning = (dx / settings.spatial_cutoff_distance).clamp(-1.0, 1.0);
            sound.handle.set_panning(panning, Tween {
                duration: Duration::from_secs_f32(settings.spatial_transition_secs),
                ..Default::default()
            });
        }

        for index in indices_to_remove {
            self.sounds.remove(index);
        }
    }

    fn set_volume(&mut self, volume: f32) {
        for (_, (_, sound)) in &mut self.sounds {
            sound.base_volume = volume;
        }
    }

    fn playing_count(&self) -> usize {
        self.sounds.len()
    }
}

// ----------------------------------------------
// Specialized sound controllers
// ----------------------------------------------

// Plays general sound effects like UI clicks, alerts, popups.
type SfxController = StaticSoundController;

// Plays non-positional ambience sounds. E.g.: global city noises, nature sounds.
type AmbienceController = StaticSoundController;

// Plays streamed background soundtrack music.
type MusicController = StreamedSoundController;

// Plays streamed narration / voice-over tracks.
type NarrationController = StreamedSoundController;
