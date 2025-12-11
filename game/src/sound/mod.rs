use slab::Slab;
use smallvec::SmallVec;
use proc_macros::DrawDebugUi;

use std::{
    time::Duration,
    marker::PhantomData,
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
    imgui_ui::{self, UiSystem, UiStaticVar},
    utils::{
        hash::{self, StringHash, PreHashedKeyMap},
        Vec2, platform::paths, mem::RawPtr,
        coords::IsoPointF32
    },
};

// ----------------------------------------------
// SoundKey
// ----------------------------------------------

pub trait SoundKey {
    fn new(hash: StringHash) -> Self;
    fn invalid() -> Self;
    fn is_valid(&self) -> bool;
}

macro_rules! declare_sound_keys {
    ($($struct_name:ident),* $(,)?) => {
        $(
            #[derive(Copy, Clone)]
            pub struct $struct_name {
                hash: StringHash,
            }

            impl SoundKey for $struct_name {
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

// ----------------------------------------------
// SoundKind
// ----------------------------------------------

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
        debug_assert!(index < u32::MAX as usize); // Reserved for invalid.
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
    #[debug_ui(edit, widget = "slider", min = "0", max = "10")]
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

    #[inline]
    pub fn settings(&self) -> &SoundGlobalSettings {
        &self.settings
    }

    pub fn update(&mut self, listener_position: IsoPointF32) {
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

    // Clears the sound registry. Note that any sound still playing is
    // not freed immediately, only after it finishes playing or is stopped.
    pub fn unload_all(&mut self) {
        self.registry.unload_all();
    }

    // ----------------------
    // Sound Play/Stop:
    // ----------------------

    pub fn play_sfx(&mut self, sound_key: SfxSoundKey, looping: bool) -> SoundHandle {
        if let Some(backend) = &mut self.backend {
            if let Some(sound) = self.registry.sfx.get(&sound_key.hash) {
                return backend.sfx.play(sound,
                                        IsoPointF32::default(),
                                        self.settings.sfx_master_volume,
                                        self.settings.sfx_fade_in_secs,
                                        self.settings.sfx_fade_out_secs,
                                        looping);
            }
        }
        SoundHandle::invalid(SoundKind::Sfx)
    }

    pub fn play_ambience(&mut self, sound_key: AmbienceSoundKey, looping: bool) -> SoundHandle {
        if let Some(backend) = &mut self.backend {
            if let Some(sound) = self.registry.ambience.get(&sound_key.hash) {
                return backend.ambience.play(sound,
                                             IsoPointF32::default(),
                                             self.settings.ambience_master_volume,
                                             self.settings.ambience_fade_in_secs,
                                             self.settings.ambience_fade_out_secs,
                                             looping);
            }
        }
        SoundHandle::invalid(SoundKind::Ambience)
    }

    pub fn play_spatial_ambience(&mut self, sound_key: AmbienceSoundKey, world_position: IsoPointF32, looping: bool) -> SoundHandle {
        if let Some(backend) = &mut self.backend {
            // Same as ambience registry.
            if let Some(sound) = self.registry.ambience.get(&sound_key.hash) {
                return backend.spatial.play(sound,
                                            world_position,
                                            self.settings.spatial_master_volume,
                                            self.settings.spatial_fade_in_secs,
                                            self.settings.spatial_fade_out_secs,
                                            looping);
            }
        }
        SoundHandle::invalid(SoundKind::SpatialAmbience)
    }

    pub fn play_music(&mut self, sound_key: MusicSoundKey, looping: bool) -> SoundHandle {
        if let Some(backend) = &mut self.backend {
            if let Some(sound) = self.registry.music.get(&sound_key.hash) {
                return backend.music.play(sound,
                                          IsoPointF32::default(),
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
                                              IsoPointF32::default(),
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

    // True if the sound is advancing and producing audio output (Fading-in, Playing, Fading-out).
    // False if the sound is not advancing (Paused, Stopped).
    pub fn is_playing(&self, sound_handle: SoundHandle) -> bool {
        if let Some(backend) = &self.backend {
            match sound_handle.kind {
                SoundKind::Sfx => {
                    return backend.sfx.is_playing(sound_handle);
                }
                SoundKind::Ambience => {
                    return backend.ambience.is_playing(sound_handle);
                }
                SoundKind::SpatialAmbience => {
                    return backend.spatial.is_playing(sound_handle);
                }
                SoundKind::Music => {
                    return backend.music.is_playing(sound_handle);
                }
                SoundKind::Narration => {
                    return backend.narration.is_playing(sound_handle);
                }
            }
        }
        false
    }

    // ----------------------
    // Debug UI:
    // ----------------------

    pub fn draw_debug_ui(&mut self, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();

        if !self.is_initialized() {
            ui.text("No Sound System available!");
            return;
        }

        let backend = self.backend.as_mut().unwrap();

        ui.text(format!("Listener Pos   : {}", backend.listener_position().0));
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
            if ui.button("Unload All Sounds") {
                self.stop_all();
                self.unload_all();
            }

            static LOOPING: UiStaticVar<bool> = UiStaticVar::new(false);
            ui.checkbox("Looping", LOOPING.as_mut());

            ui.text("SFX:");

            if ui.button("Play SFX (bleep)") {
                let sound_key = self.load_sfx("test/bleep.ogg");
                self.play_sfx(sound_key, *LOOPING);
            }

            if ui.button("Play SFX (drums)") {
                let sound_key = self.load_sfx("test/drums.ogg");
                self.play_sfx(sound_key, *LOOPING);
            }

            if ui.button("Stop All SFX") {
                self.stop_sfx();
            }

            ui.text("Music:");

            if ui.button("Play Music") {
                let sound_key = self.load_music("dynastys_legacy_1.mp3");
                self.play_music(sound_key, *LOOPING);
            }

            if ui.button("Stop All Music") {
                self.stop_music();
            }

            ui.text("Ambience:");

            if ui.button("Play Ambience") {
                let sound_key = self.load_ambience("birds_chirping_ambiance.mp3");
                self.play_ambience(sound_key, *LOOPING);
            }

            if ui.button("Stop All Ambience") {
                self.stop_ambience();
            }

            static SPATIAL_ORIGIN: UiStaticVar<Vec2> = UiStaticVar::new(Vec2::zero());
            imgui_ui::input_f32_xy(ui, "Spatial:", SPATIAL_ORIGIN.as_mut(), false, None, None);

            if ui.button("Play Spatial") {
                let sound_key = self.load_ambience("birds_chirping_ambiance.mp3");
                self.play_spatial_ambience(sound_key, IsoPointF32(*SPATIAL_ORIGIN), *LOOPING);
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

    // Debug:
    listener_position: IsoPointF32,
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
        let ambience = AmbienceController::new(Box::new(ambience_track), SoundKind::Ambience);
        let spatial = SpatialController::new(RawPtr::from_ref(&ambience.track), SoundKind::SpatialAmbience); // NOTE: Track shared with ambience.
        let music = MusicController::new(music_track, SoundKind::Music);
        let narration = NarrationController::new(narration_track, SoundKind::Narration);

        Some(Box::new(Self {
            manager,
            sfx,
            ambience,
            spatial,
            music,
            narration,
            listener_position: IsoPointF32::default(),
        }))
    }

    fn update(&mut self, listener_position: IsoPointF32, settings: &SoundGlobalSettings) {
        self.listener_position = listener_position;
        self.sfx.update(listener_position, settings);
        self.ambience.update(listener_position, settings);
        self.spatial.update(listener_position, settings);
        self.music.update(listener_position, settings);
        self.narration.update(listener_position, settings);
    }

    fn set_volumes(&mut self, settings: &SoundGlobalSettings) {
        self.sfx.set_volume(settings.sfx_master_volume);
        self.ambience.set_volume(settings.ambience_master_volume);
        self.spatial.set_volume(settings.spatial_master_volume);
        self.music.set_volume(settings.music_master_volume);
        self.narration.set_volume(settings.narration_master_volume);
    }

    fn listener_position(&self) -> IsoPointF32 {
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
// SoundAsset/StaticSoundAsset/StreamedSoundAsset
// ----------------------------------------------

trait SoundAsset {
    // Low-level Kira API handle, AKA StaticSoundHandle or StreamingSoundHandle.
    type BackendSoundHandle;

    // Returns a Kira sound handle for the playing sound.
    fn play(&self,
            track: &mut TrackHandle,
            volume: f32,
            fade_in_secs: Seconds,
            looping: bool) -> Option<Self::BackendSoundHandle>;
}

struct StaticSoundAsset {
    path: String,
    data: StaticSoundData, // Sound data loaded in memory.
}

struct StreamedSoundAsset {
    path: String,
    // Data lazily loaded on first play.
}

impl SoundAsset for StaticSoundAsset {
    type BackendSoundHandle = StaticSoundHandle;

    fn play(&self,
            track: &mut TrackHandle,
            volume: f32,
            fade_in_secs: Seconds,
            looping: bool) -> Option<Self::BackendSoundHandle>
    {
        debug_assert!(!self.path.is_empty());

        let sound_data = self.data.volume(linear_to_decibels(volume));

        let mut handle = match track.play(
            sound_data.fade_in_tween(Tween {
                duration: Duration::from_secs_f32(fade_in_secs),
                ..Default::default()
        }))
        {
            Ok(handle) => handle,
            Err(err) => {
                log::warn!(log::channel!("sound"), "Failed to play StaticSound: {err}");
                return None;
            }
        };

        if looping {
            // Play in a loop until told to stop.
            handle.set_loop_region(..);
        }

        Some(handle)
    }
}

impl SoundAsset for StreamedSoundAsset {
    type BackendSoundHandle = StreamingSoundHandle<FromFileError>;

    fn play(&self,
            track: &mut TrackHandle,
            volume: f32,
            fade_in_secs: Seconds,
            looping: bool) -> Option<Self::BackendSoundHandle>
    {
        debug_assert!(!self.path.is_empty());

        let sound_data = match StreamingSoundData::from_file(&self.path) {
            Ok(sound_data) => sound_data.volume(linear_to_decibels(volume)),
            Err(err) => {
                log::error!(log::channel!("sound"), "Failed to load StreamedSound '{}': {err}", self.path);
                return None;
            }
        };

        let mut handle = match track.play(
            sound_data.fade_in_tween(Tween {
                duration: Duration::from_secs_f32(fade_in_secs),
                ..Default::default()
        }))
        {
            Ok(handle) => handle,
            Err(err) => {
                log::warn!(log::channel!("sound"), "Failed to play StreamedSound '{}': {err}", self.path);
                return None;
            }
        };

        if looping {
            // Play in a loop until told to stop.
            handle.set_loop_region(..);
        }

        Some(handle)
    }
}

// ----------------------------------------------
// SoundAssetRegistry
// ----------------------------------------------

// Stores loaded SFX, music, narration, etc.
#[derive(Default)]
struct SoundAssetRegistry {
    // SFX, Ambience, Spatial: StaticSoundData
    // Music & Narration: StreamingSoundData

    sfx: PreHashedKeyMap<StringHash, StaticSoundAsset>,
    ambience: PreHashedKeyMap<StringHash, StaticSoundAsset>, // Spatial sounds are the same as ambience.

    music: PreHashedKeyMap<StringHash, StreamedSoundAsset>,
    narration: PreHashedKeyMap<StringHash, StreamedSoundAsset>,

    paths: SoundAssetPaths,
}

#[derive(Default)]
struct SoundAssetPaths {
    sfx: PathBuf,
    ambience: PathBuf,
    music: PathBuf,
    narration: PathBuf,
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

    #[inline]
    fn load_sfx(&mut self, path: &str) -> SfxSoundKey {
        load_static_sound(&mut self.sfx, &self.paths.sfx, path)
    }

    #[inline]
    fn load_ambience(&mut self, path: &str) -> AmbienceSoundKey {
        load_static_sound(&mut self.ambience, &self.paths.ambience, path)
    }

    #[inline]
    fn load_music(&mut self, path: &str) -> MusicSoundKey {
        load_streamed_sound(&mut self.music, &self.paths.music, path)
    }

    #[inline]
    fn load_narration(&mut self, path: &str) -> NarrationSoundKey {
        load_streamed_sound(&mut self.narration, &self.paths.narration, path)
    }

    fn unload_all(&mut self) {
        self.sfx.clear();
        self.ambience.clear();
        self.music.clear();
        self.narration.clear();
    }

    fn sounds_loaded(&self) -> usize {
        self.sfx.len()
        + self.ambience.len()
        + self.music.len()
        + self.narration.len()
    }
}

fn load_static_sound<Key: SoundKey>(hash_map: &mut PreHashedKeyMap<StringHash, StaticSoundAsset>,
                                    base_path: &Path,
                                    asset_path: &str) -> Key {
    debug_assert!(!asset_path.is_empty());

    let sound_path = format!("{}{}{}", base_path.to_str().unwrap(), MAIN_SEPARATOR, asset_path);
    let sound_hash = hash::fnv1a_from_str(&sound_path);

    if hash_map.get(&sound_hash).is_some() {
        return Key::new(sound_hash); // Already loaded.
    }

    let sound_data = match StaticSoundData::from_file(&sound_path) {
        Ok(sound_data) => sound_data,
        Err(err) => {
            log::error!(log::channel!("sound"), "Failed to load sound '{sound_path}': {err}");
            return Key::invalid();
        }
    };

    hash_map.insert(sound_hash, StaticSoundAsset { path: sound_path, data: sound_data });
    Key::new(sound_hash)
}

fn load_streamed_sound<Key: SoundKey>(hash_map: &mut PreHashedKeyMap<StringHash, StreamedSoundAsset>,
                                      base_path: &Path,
                                      asset_path: &str) -> Key {
    debug_assert!(!asset_path.is_empty());

    let sound_path = format!("{}{}{}", base_path.to_str().unwrap(), MAIN_SEPARATOR, asset_path);
    let sound_hash = hash::fnv1a_from_str(&sound_path);

    if hash_map.get(&sound_hash).is_some() {
        return Key::new(sound_hash); // Already loaded.
    }

    // Only probe file path now. Data is lazily loaded on first reference.
    if std::fs::exists(&sound_path).is_err() {
        log::error!(log::channel!("sound"), "Sound file path '{sound_path}' is invalid!");
        return Key::invalid();
    }

    hash_map.insert(sound_hash, StreamedSoundAsset { path: sound_path });
    Key::new(sound_hash)
}

// ----------------------------------------------
// SoundInstance/SoundController
// ----------------------------------------------

trait SoundInstance {
    // Low-level Kira API handle, AKA StaticSoundHandle or StreamingSoundHandle.
    type BackendSoundHandle;

    fn new(handle: Self::BackendSoundHandle, generation: u32, position: IsoPointF32, volume: f32) -> Self;
    fn generation(&self) -> u32;

    fn spatial_update(&mut self, _listener_position: IsoPointF32, _settings: &SoundGlobalSettings) {}
    fn stop(&mut self, fade_out_secs: Seconds);
    fn set_volume(&mut self, volume: f32);

    fn is_playing(&self) -> bool;
    fn is_stopped(&self) -> bool;
}

struct SoundController<const SINGLE_SOUND: bool, const SPATIAL: bool, Track, Inst, Asset, Handle> {
    track: Track,
    kind: SoundKind,
    sounds: Slab<Inst>,
    generation: u32,
    _asset_type: PhantomData<Asset>,
    _handle_type: PhantomData<Handle>,
}

impl<const SINGLE_SOUND: bool, const SPATIAL: bool, Track, Inst, Asset, Handle>
    SoundController<SINGLE_SOUND, SPATIAL, Track, Inst, Asset, Handle>
{
    fn new(track: Track, kind: SoundKind) -> Self {
        Self {
            track,
            kind,
            sounds: Slab::new(),
            generation: 0,
            _asset_type: PhantomData,
            _handle_type: PhantomData,
        }
    }
}

impl<const SINGLE_SOUND: bool, const SPATIAL: bool, Track, Inst, Asset, Handle>
    SoundController<SINGLE_SOUND, SPATIAL, Track, Inst, Asset, Handle>
        where
            Track: AsTrackHandle,
            Inst:  SoundInstance<BackendSoundHandle = Handle>,
            Asset: SoundAsset<BackendSoundHandle = Handle>,
{
    fn play(&mut self,
            sound_asset: &Asset,
            position: IsoPointF32,
            volume: f32,
            fade_in_secs: Seconds,
            fade_out_secs: Seconds,
            looping: bool) -> SoundHandle
    {
        if SINGLE_SOUND {
            // Stop current if any is already playing.
            self.stop_all(fade_out_secs);
        }

        if let Some(handle) =
            sound_asset.play(self.track.as_handle_mut(), volume, fade_in_secs, looping)
        {
            self.generation += 1;
            let index = self.sounds.insert(Inst::new(handle, self.generation, position, volume));
            return SoundHandle::new(self.kind, index, self.generation);
        }

        SoundHandle::invalid(self.kind)
    }

    fn is_playing(&self, sound_handle: SoundHandle) -> bool {
        if sound_handle.is_valid() && sound_handle.kind == self.kind {
            if let Some(sound) = self.try_get_sound(sound_handle) {
                return sound.is_playing();
            }
        }
        false
    }

    fn stop(&mut self, sound_handle: SoundHandle, fade_out_secs: Seconds) {
        if !sound_handle.is_valid() || sound_handle.kind != self.kind {
            return;
        }

        if let Some(sound) = self.try_get_sound_mut(sound_handle) {
            sound.stop(fade_out_secs);
        }
    }

    fn stop_all(&mut self, fade_out_secs: Seconds) {
        for (_, sound) in &mut self.sounds {
            sound.stop(fade_out_secs);
        }
    }

    fn set_volume(&mut self, volume: f32) {
        for (_, sound) in &mut self.sounds {
            sound.set_volume(volume);
        }
    }

    fn update(&mut self, listener_position: IsoPointF32, settings: &SoundGlobalSettings) {
        self.remove_stopped_sounds();

        if SPATIAL {
            for (_, sound) in &mut self.sounds {
                sound.spatial_update(listener_position, settings);
            }
        }
    }

    fn remove_stopped_sounds(&mut self) {
        let mut indices_to_remove = SmallVec::<[usize; 32]>::new();

        for (index, sound) in &self.sounds {
            if sound.is_stopped() {
                indices_to_remove.push(index);
            }
        }

        for index in indices_to_remove {
            self.sounds.remove(index);
        }
    }

    #[inline]
    fn try_get_sound(&self, sound_handle: SoundHandle) -> Option<&Inst> {
        if let Some(sound) = self.sounds.get(sound_handle.index as usize) {
            if sound_handle.generation == sound.generation() {
                return Some(sound);
            }
        }
        None
    }

    #[inline]
    fn try_get_sound_mut(&mut self, sound_handle: SoundHandle) -> Option<&mut Inst> {
        if let Some(sound) = self.sounds.get_mut(sound_handle.index as usize) {
            if sound_handle.generation == sound.generation() {
                return Some(sound);
            }
        }
        None
    }

    #[inline]
    fn playing_count(&self) -> usize {
        self.sounds.len()
    }
}

// ----------------------------------------------
// AsTrackHandle
// ----------------------------------------------

trait AsTrackHandle {
    fn as_handle_mut(&mut self) -> &mut TrackHandle;
}

impl AsTrackHandle for TrackHandle {
    fn as_handle_mut(&mut self) -> &mut TrackHandle {
        self
    }
}

impl AsTrackHandle for Box<TrackHandle> {
    fn as_handle_mut(&mut self) -> &mut TrackHandle {
        self.as_mut()
    }
}

impl AsTrackHandle for RawPtr<TrackHandle> {
    fn as_handle_mut(&mut self) -> &mut TrackHandle {
        self.as_mut()
    }
}

// ----------------------------------------------
// StaticSoundInstance/StreamedSoundInstance
// ----------------------------------------------

macro_rules! declare_sound_instance {
    ($struct_name:ident, $handle_type:path) => {
        struct $struct_name {
            handle: $handle_type,
            generation: u32,
        }

        impl SoundInstance for $struct_name {
            type BackendSoundHandle = $handle_type;

            #[inline]
            fn new(handle: Self::BackendSoundHandle, generation: u32, _: IsoPointF32, _: f32) -> Self {
                Self { handle, generation }
            }

            #[inline]
            fn generation(&self) -> u32 {
                self.generation
            }

            #[inline]
            fn stop(&mut self, fade_out_secs: Seconds) {
                self.handle.stop(Tween {
                    duration: Duration::from_secs_f32(fade_out_secs),
                    ..Default::default()
                });
            }

            #[inline]
            fn set_volume(&mut self, volume: f32) {
                let volume_db = linear_to_decibels(volume);
                self.handle.set_volume(volume_db, Tween::default());
            }

            #[inline]
            fn is_playing(&self) -> bool {
                self.handle.state().is_advancing()
            }

            #[inline]
            fn is_stopped(&self) -> bool {
                self.handle.state() == PlaybackState::Stopped
            }
        }
    };
}

declare_sound_instance! { StaticSoundInstance, StaticSoundHandle }
declare_sound_instance! { StreamedSoundInstance, StreamingSoundHandle<FromFileError> }

// ----------------------------------------------
// SpatialSoundInstance: Spatial emulation in 2D
// ----------------------------------------------

struct SpatialSoundInstance {
    inner: StaticSoundInstance,
    position: IsoPointF32,
    volume: f32, // linear volume [0-1] range.
}

impl SoundInstance for SpatialSoundInstance {
    type BackendSoundHandle = StaticSoundHandle;

    #[inline]
    fn new(handle: Self::BackendSoundHandle, generation: u32, position: IsoPointF32, volume: f32) -> Self {
        Self { inner: StaticSoundInstance { handle, generation }, position, volume }
    }

    #[inline]
    fn generation(&self) -> u32 {
        self.inner.generation
    }

    #[inline]
    fn stop(&mut self, fade_out_secs: Seconds) {
        self.inner.stop(fade_out_secs);
    }

    #[inline]
    fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
    }

    #[inline]
    fn is_playing(&self) -> bool {
        self.inner.is_playing()
    }

    #[inline]
    fn is_stopped(&self) -> bool {
        self.inner.is_stopped()
    }

    fn spatial_update(&mut self, listener_position: IsoPointF32, settings: &SoundGlobalSettings) {
        let dx = self.position.0.x - listener_position.0.x;
        let dy = self.position.0.y - listener_position.0.y;

        let distance = ((dx * dx) + (dy * dy)).sqrt();
        let dist_factor = 1.0 - (distance / settings.spatial_cutoff_distance).clamp(0.0, 1.0);

        // Convert linear volume [0-1] to decibels:
        let volume_db = linear_to_decibels(self.volume * dist_factor);
        self.inner.handle.set_volume(volume_db, Tween {
            duration: Duration::from_secs_f32(settings.spatial_transition_secs),
            ..Default::default()
        });

        // -1.0 is hard left, 0.0 is center, and 1.0 is hard right.
        let panning = (dx / settings.spatial_cutoff_distance).clamp(-1.0, 1.0);
        self.inner.handle.set_panning(panning, Tween {
            duration: Duration::from_secs_f32(settings.spatial_transition_secs),
            ..Default::default()
        });
    }
}

// ----------------------------------------------
// Specialized sound controllers
// ----------------------------------------------

// Plays general sound effects like UI clicks, alerts, popups.
type SfxController = SoundController<
    false,
    false,
    TrackHandle,
    StaticSoundInstance,
    StaticSoundAsset,
    StaticSoundHandle
>;

// Plays non-positional ambience sounds. E.g.: global city noises, nature sounds.
type AmbienceController = SoundController<
    false,
    false,
    Box<TrackHandle>,
    StaticSoundInstance,
    StaticSoundAsset,
    StaticSoundHandle
>;

// Fake 3D positional sound. Shares the ambiance track.
// Plays sounds that have a specific origin in the world. E.g.: fire, building collapsed.
type SpatialController = SoundController<
    false,
    true,
    RawPtr<TrackHandle>,
    SpatialSoundInstance,
    StaticSoundAsset,
    StaticSoundHandle
>;

// Plays streamed background soundtrack music.
type MusicController = SoundController<
    true,
    false,
    TrackHandle,
    StreamedSoundInstance,
    StreamedSoundAsset,
    StreamingSoundHandle<FromFileError>
>;

// Plays streamed narration / voice-over tracks.
type NarrationController = SoundController<
    true,
    false,
    TrackHandle,
    StreamedSoundInstance,
    StreamedSoundAsset,
    StreamingSoundHandle<FromFileError>
>;
