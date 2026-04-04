use serde::{Serialize, Deserialize};
use proc_macros::DrawDebugUi;

use common::{
    Vec2, coords::IsoPointF32,
    hash::{self, StringHash},
    time::Seconds,
};
use crate::{
    ui::{self, UiSystem, UiStaticVar},
    file_sys::paths::PathRef,
};

// ----------------------------------------------
// Internal backend implementations
// ----------------------------------------------

// Kira
#[cfg(feature = "desktop")]
mod kira;
#[cfg(feature = "desktop")]
type SoundSystemBackendImpl = kira::KiraSoundSystemBackend;
#[cfg(feature = "desktop")]
type SoundAssetRegistryImpl = kira::KiraSoundAssetRegistry;

// Web Audio
#[cfg(feature = "web")]
mod web_audio;
#[cfg(feature = "web")]
type SoundSystemBackendImpl = web_audio::WebAudioSoundSystemBackend;
#[cfg(feature = "web")]
type SoundAssetRegistryImpl = web_audio::WebAudioSoundAssetRegistry;

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
pub enum SoundKind {
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
    pub fn invalid(kind: SoundKind) -> Self {
        Self { kind, index: u32::MAX, generation: 0 }
    }

    #[inline]
    pub fn is_valid(&self) -> bool {
        self.index < u32::MAX && self.generation != 0
    }
}

// ----------------------------------------------
// SoundGlobalSettings
// ----------------------------------------------

#[derive(Copy, Clone, PartialEq, DrawDebugUi, Serialize, Deserialize)]
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

impl SoundGlobalSettings {
    #[inline]
    fn master_volume(&self, kind: SoundKind) -> f32 {
        match kind {
            SoundKind::Sfx             => self.sfx_master_volume,
            SoundKind::Ambience        => self.ambience_master_volume,
            SoundKind::SpatialAmbience => self.spatial_master_volume,
            SoundKind::Music           => self.music_master_volume,
            SoundKind::Narration       => self.narration_master_volume,
        }
    }

    #[inline]
    fn fade_in_secs(&self, kind: SoundKind) -> Seconds {
        match kind {
            SoundKind::Sfx             => self.sfx_fade_in_secs,
            SoundKind::Ambience        => self.ambience_fade_in_secs,
            SoundKind::SpatialAmbience => self.spatial_fade_in_secs,
            SoundKind::Music           => self.music_fade_in_secs,
            SoundKind::Narration       => self.narration_fade_in_secs,
        }
    }

    #[inline]
    fn fade_out_secs(&self, kind: SoundKind) -> Seconds {
        match kind {
            SoundKind::Sfx             => self.sfx_fade_out_secs,
            SoundKind::Ambience        => self.ambience_fade_out_secs,
            SoundKind::SpatialAmbience => self.spatial_fade_out_secs,
            SoundKind::Music           => self.music_fade_out_secs,
            SoundKind::Narration       => self.narration_fade_out_secs,
        }
    }
}

// ----------------------------------------------
// PlaySoundParams
// ----------------------------------------------

struct PlaySoundParams<'a, R>
    where R: SoundAssetRegistry
{
    registry: &'a R,
    settings: &'a SoundGlobalSettings,
    kind: SoundKind,
    key_hash: StringHash,
    position: IsoPointF32,
    looping: bool,
}

// ----------------------------------------------
// SoundSystemBackend / SoundAssetRegistry
// ----------------------------------------------

trait SoundSystemBackend: Sized {
    type Registry: SoundAssetRegistry;

    fn new() -> Option<Box<Self>>;
    fn update(&mut self, listener_position: IsoPointF32, settings: &SoundGlobalSettings);
    fn set_volumes(&mut self, settings: &SoundGlobalSettings);
    fn listener_position(&self) -> IsoPointF32;
    fn sounds_playing(&self) -> usize;

    fn play(&mut self, params: PlaySoundParams<Self::Registry>) -> SoundHandle;
    fn stop(&mut self, sound_handle: SoundHandle, settings: &SoundGlobalSettings);
    fn stop_kind(&mut self, kind: SoundKind, fade_out: Seconds);
    fn stop_all(&mut self, settings: &SoundGlobalSettings);
    fn is_playing(&self, sound_handle: SoundHandle) -> bool;
}

trait SoundAssetRegistry: Sized {
    fn new() -> Self;
    fn load_sfx(&mut self, path: PathRef) -> SfxSoundKey;
    fn load_ambience(&mut self, path: PathRef) -> AmbienceSoundKey;
    fn load_music(&mut self, path: PathRef) -> MusicSoundKey;
    fn load_narration(&mut self, path: PathRef) -> NarrationSoundKey;
    fn unload_all(&mut self);
    fn sounds_loaded(&self) -> usize;
}

// ----------------------------------------------
// SoundSystem
// ----------------------------------------------

pub struct SoundSystem {
    backend:  Option<Box<SoundSystemBackendImpl>>,
    registry: SoundAssetRegistryImpl,
    settings: SoundGlobalSettings,
}

impl SoundSystem {
    pub fn new(settings: SoundGlobalSettings) -> Self {
        Self {
            backend:  SoundSystemBackendImpl::new(),
            registry: SoundAssetRegistryImpl::new(),
            settings,
        }
    }

    #[inline]
    pub fn is_initialized(&self) -> bool {
        self.backend.is_some()
    }

    #[inline]
    pub fn current_sound_settings(&self) -> SoundGlobalSettings {
        self.settings
    }

    pub fn change_sound_settings(&mut self, settings: SoundGlobalSettings) {
        self.settings = settings;
        if let Some(backend) = &mut self.backend {
            backend.set_volumes(&self.settings);
        }
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

    pub fn load_sfx(&mut self, path: PathRef) -> SfxSoundKey {
        if !self.is_initialized() {
            return SfxSoundKey::invalid();
        }
        self.registry.load_sfx(path)
    }

    pub fn load_ambience(&mut self, path: PathRef) -> AmbienceSoundKey {
        if !self.is_initialized() {
            return AmbienceSoundKey::invalid();
        }
        self.registry.load_ambience(path)
    }

    pub fn load_music(&mut self, path: PathRef) -> MusicSoundKey {
        if !self.is_initialized() {
            return MusicSoundKey::invalid();
        }
        self.registry.load_music(path)
    }

    pub fn load_narration(&mut self, path: PathRef) -> NarrationSoundKey {
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
        self.play_backend(SoundKind::Sfx, sound_key.hash, IsoPointF32::default(), looping)
    }

    pub fn play_ambience(&mut self, sound_key: AmbienceSoundKey, looping: bool) -> SoundHandle {
        self.play_backend(SoundKind::Ambience, sound_key.hash, IsoPointF32::default(), looping)
    }

    pub fn play_spatial_ambience(&mut self, sound_key: AmbienceSoundKey, world_position: IsoPointF32, looping: bool) -> SoundHandle {
        self.play_backend(SoundKind::SpatialAmbience, sound_key.hash, world_position, looping)
    }

    pub fn play_music(&mut self, sound_key: MusicSoundKey, looping: bool) -> SoundHandle {
        self.play_backend(SoundKind::Music, sound_key.hash, IsoPointF32::default(), looping)
    }

    pub fn play_narration(&mut self, sound_key: NarrationSoundKey, looping: bool) -> SoundHandle {
        self.play_backend(SoundKind::Narration, sound_key.hash, IsoPointF32::default(), looping)
    }

    fn play_backend(&mut self, kind: SoundKind, key_hash: StringHash, position: IsoPointF32, looping: bool) -> SoundHandle {
        if let Some(backend) = &mut self.backend {
            return backend.play(PlaySoundParams {
                registry: &self.registry,
                settings: &self.settings,
                kind,
                key_hash,
                position,
                looping,
            });
        }
        SoundHandle::invalid(kind)
    }

    // Stop all sounds on these tracks.
    pub fn stop_sfx(&mut self) {
        if let Some(backend) = &mut self.backend {
            backend.stop_kind(SoundKind::Sfx, self.settings.fade_out_secs(SoundKind::Sfx));
        }
    }

    pub fn stop_ambience(&mut self) {
        if let Some(backend) = &mut self.backend {
            backend.stop_kind(SoundKind::Ambience, self.settings.fade_out_secs(SoundKind::Ambience));
        }
    }

    pub fn stop_spatial_ambience(&mut self) {
        if let Some(backend) = &mut self.backend {
            backend.stop_kind(SoundKind::SpatialAmbience, self.settings.fade_out_secs(SoundKind::SpatialAmbience));
        }
    }

    pub fn stop_music(&mut self) {
        if let Some(backend) = &mut self.backend {
            backend.stop_kind(SoundKind::Music, self.settings.fade_out_secs(SoundKind::Music));
        }
    }

    pub fn stop_narration(&mut self) {
        if let Some(backend) = &mut self.backend {
            backend.stop_kind(SoundKind::Narration, self.settings.fade_out_secs(SoundKind::Narration));
        }
    }

    // Stop specific sound by handle.
    pub fn stop(&mut self, sound_handle: SoundHandle) {
        if let Some(backend) = &mut self.backend {
            backend.stop(sound_handle, &self.settings);
        }
    }

    // Stops all currently playing sounds.
    pub fn stop_all(&mut self) {
        if let Some(backend) = &mut self.backend {
            backend.stop_all(&self.settings);
        }
    }

    // True if the sound is advancing and producing audio output (Fading-in, Playing, Fading-out).
    // False if the sound is not advancing (Paused, Stopped).
    pub fn is_playing(&self, sound_handle: SoundHandle) -> bool {
        if !sound_handle.is_valid() {
            return false;
        }

        if let Some(backend) = &self.backend {
            return backend.is_playing(sound_handle);
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
            self.change_sound_settings(new_settings);
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
                let sound_key = self.load_sfx(PathRef::from_str("test/bleep.ogg"));
                self.play_sfx(sound_key, *LOOPING);
            }

            if ui.button("Play SFX (drums)") {
                let sound_key = self.load_sfx(PathRef::from_str("test/drums.ogg"));
                self.play_sfx(sound_key, *LOOPING);
            }

            if ui.button("Stop All SFX") {
                self.stop_sfx();
            }

            ui.text("Music:");

            if ui.button("Play Music") {
                let sound_key = self.load_music(PathRef::from_str("dynastys_legacy_1.mp3"));
                self.play_music(sound_key, *LOOPING);
            }

            if ui.button("Stop All Music") {
                self.stop_music();
            }

            ui.text("Ambience:");

            if ui.button("Play Ambience") {
                let sound_key = self.load_ambience(PathRef::from_str("birds_chirping_ambiance.mp3"));
                self.play_ambience(sound_key, *LOOPING);
            }

            if ui.button("Stop All Ambience") {
                self.stop_ambience();
            }

            static SPATIAL_ORIGIN: UiStaticVar<Vec2> = UiStaticVar::new(Vec2::zero());
            ui::input_f32_xy(ui, "Spatial:", SPATIAL_ORIGIN.as_mut(), false, None, None);

            if ui.button("Play Spatial") {
                let sound_key = self.load_ambience(PathRef::from_str("birds_chirping_ambiance.mp3"));
                self.play_spatial_ambience(sound_key, IsoPointF32(*SPATIAL_ORIGIN), *LOOPING);
            }

            if ui.button("Stop All Spatial") {
                self.stop_spatial_ambience();
            }
        }
    }
}

// ----------------------------------------------
// Utilities
// ----------------------------------------------

#[inline]
fn linear_to_decibels(mut volume: f32) -> f32 {
    volume = volume.clamp(0.0, 1.0);

    // If near zero -> treat as silent.
    if volume <= 0.0001 {
        return -60.0;
    }

    20.0 * volume.log10()
}
