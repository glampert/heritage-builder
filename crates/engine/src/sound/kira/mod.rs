use slab::Slab;
use smallvec::SmallVec;
use std::{time::Duration, marker::PhantomData};

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

use super::{
    SoundKind, SoundHandle, SoundGlobalSettings, PlaySoundParams,
    SoundKey, SfxSoundKey, AmbienceSoundKey, MusicSoundKey, NarrationSoundKey,
};
use common::{
    mem::RcMut,
    time::Seconds,
    coords::IsoPointF32,
    hash::{self, StringHash, PreHashedKeyMap},
};
use crate::{
    log,
    file_sys::paths::{self, PathRef, AssetPath},
};

// ----------------------------------------------
// KiraSoundSystemBackend
// ----------------------------------------------

pub(super) struct KiraSoundSystemBackend {
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

impl super::SoundSystemBackend for KiraSoundSystemBackend {
    type Registry = KiraSoundAssetRegistry;

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

        // NOTE: Ambience Track is shared between AmbienceController and SpatialController.
        let ambience_track_rc = RcMut::new(ambience_track);

        let sfx = SfxController::new(sfx_track, SoundKind::Sfx);
        let ambience = AmbienceController::new(ambience_track_rc.clone(), SoundKind::Ambience);
        let spatial = SpatialController::new(ambience_track_rc, SoundKind::SpatialAmbience);
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
        self.sfx.set_volume(settings.master_volume(SoundKind::Sfx));
        self.ambience.set_volume(settings.master_volume(SoundKind::Ambience));
        self.spatial.set_volume(settings.master_volume(SoundKind::SpatialAmbience));
        self.music.set_volume(settings.master_volume(SoundKind::Music));
        self.narration.set_volume(settings.master_volume(SoundKind::Narration));
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

    fn play(&mut self, params: PlaySoundParams<KiraSoundAssetRegistry>) -> SoundHandle {
        let volume = params.settings.master_volume(params.kind);
        let fade_in = params.settings.fade_in_secs(params.kind);
        let fade_out = params.settings.fade_out_secs(params.kind);

        match params.kind {
            SoundKind::Sfx => {
                if let Some(sound) = params.registry.sfx.get(&params.key_hash) {
                    return self.sfx.play(sound, params.position, volume, fade_in, fade_out, params.looping);
                }
            }
            SoundKind::Ambience => {
                if let Some(sound) = params.registry.ambience.get(&params.key_hash) {
                    return self.ambience.play(sound, params.position, volume, fade_in, fade_out, params.looping);
                }
            }
            SoundKind::SpatialAmbience => {
                // NOTE: Spatial sounds share the ambience registry.
                if let Some(sound) = params.registry.ambience.get(&params.key_hash) {
                    return self.spatial.play(sound, params.position, volume, fade_in, fade_out, params.looping);
                }
            }
            SoundKind::Music => {
                if let Some(sound) = params.registry.music.get(&params.key_hash) {
                    return self.music.play(sound, params.position, volume, fade_in, fade_out, params.looping);
                }
            }
            SoundKind::Narration => {
                if let Some(sound) = params.registry.narration.get(&params.key_hash) {
                    return self.narration.play(sound, params.position, volume, fade_in, fade_out, params.looping);
                }
            }
        }

        SoundHandle::invalid(params.kind)
    }

    fn stop(&mut self, sound_handle: SoundHandle, settings: &SoundGlobalSettings) {
        if !sound_handle.is_valid() {
            return;
        }

        let fade_out = settings.fade_out_secs(sound_handle.kind);

        match sound_handle.kind {
            SoundKind::Sfx             => self.sfx.stop(sound_handle, fade_out),
            SoundKind::Ambience        => self.ambience.stop(sound_handle, fade_out),
            SoundKind::SpatialAmbience => self.spatial.stop(sound_handle, fade_out),
            SoundKind::Music           => self.music.stop(sound_handle, fade_out),
            SoundKind::Narration       => self.narration.stop(sound_handle, fade_out),
        }
    }

    fn stop_kind(&mut self, kind: SoundKind, fade_out: Seconds) {
        match kind {
            SoundKind::Sfx             => self.sfx.stop_all(fade_out),
            SoundKind::Ambience        => self.ambience.stop_all(fade_out),
            SoundKind::SpatialAmbience => self.spatial.stop_all(fade_out),
            SoundKind::Music           => self.music.stop_all(fade_out),
            SoundKind::Narration       => self.narration.stop_all(fade_out),
        }
    }

    fn stop_all(&mut self, settings: &SoundGlobalSettings) {
        self.sfx.stop_all(settings.fade_out_secs(SoundKind::Sfx));
        self.ambience.stop_all(settings.fade_out_secs(SoundKind::Ambience));
        self.spatial.stop_all(settings.fade_out_secs(SoundKind::SpatialAmbience));
        self.music.stop_all(settings.fade_out_secs(SoundKind::Music));
        self.narration.stop_all(settings.fade_out_secs(SoundKind::Narration));
    }

    fn is_playing(&self, sound_handle: SoundHandle) -> bool {
        match sound_handle.kind {
            SoundKind::Sfx             => self.sfx.is_playing(sound_handle),
            SoundKind::Ambience        => self.ambience.is_playing(sound_handle),
            SoundKind::SpatialAmbience => self.spatial.is_playing(sound_handle),
            SoundKind::Music           => self.music.is_playing(sound_handle),
            SoundKind::Narration       => self.narration.is_playing(sound_handle),
        }
    }
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

        let sound_data = self.data.volume(super::linear_to_decibels(volume));

        let mut handle = match track.play(
            sound_data.fade_in_tween(Tween {
                duration: Duration::from_secs_f32(fade_in_secs),
                ..Default::default()
        }))
        {
            Ok(handle) => handle,
            Err(err) => {
                log::warning!(log::channel!("sound"), "Failed to play StaticSound: {err}");
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
            Ok(sound_data) => sound_data.volume(super::linear_to_decibels(volume)),
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
                log::warning!(log::channel!("sound"), "Failed to play StreamedSound '{}': {err}", self.path);
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
// KiraSoundAssetRegistry
// ----------------------------------------------

// Stores loaded SFX, music, narration, etc.
pub(super) struct KiraSoundAssetRegistry {
    // SFX, Ambience, Spatial: StaticSoundData
    // Music & Narration: StreamingSoundData

    sfx: PreHashedKeyMap<StringHash, StaticSoundAsset>,
    ambience: PreHashedKeyMap<StringHash, StaticSoundAsset>, // Spatial sounds are the same as ambience.

    music: PreHashedKeyMap<StringHash, StreamedSoundAsset>,
    narration: PreHashedKeyMap<StringHash, StreamedSoundAsset>,

    paths: Box<SoundAssetPaths>,
}

struct SoundAssetPaths {
    sfx: AssetPath,
    ambience: AssetPath,
    music: AssetPath,
    narration: AssetPath,
}

impl super::SoundAssetRegistry for KiraSoundAssetRegistry {
    fn new() -> Self {
        let sound_base_path = paths::assets_path().join("sounds");
        let asset_paths = SoundAssetPaths {
            sfx:       sound_base_path.join("sfx"),
            ambience:  sound_base_path.join("ambience"),
            music:     sound_base_path.join("music"),
            narration: sound_base_path.join("narration"),
        };

        Self {
            sfx:       PreHashedKeyMap::default(),
            ambience:  PreHashedKeyMap::default(),
            music:     PreHashedKeyMap::default(),
            narration: PreHashedKeyMap::default(),
            paths:     Box::new(asset_paths),
        }
    }

    #[inline]
    fn load_sfx(&mut self, path: PathRef) -> SfxSoundKey {
        load_static_sound(&mut self.sfx, &self.paths.sfx, path)
    }

    #[inline]
    fn load_ambience(&mut self, path: PathRef) -> AmbienceSoundKey {
        load_static_sound(&mut self.ambience, &self.paths.ambience, path)
    }

    #[inline]
    fn load_music(&mut self, path: PathRef) -> MusicSoundKey {
        load_streamed_sound(&mut self.music, &self.paths.music, path)
    }

    #[inline]
    fn load_narration(&mut self, path: PathRef) -> NarrationSoundKey {
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
                                    base_path: &AssetPath,
                                    asset_path: PathRef) -> Key {
    debug_assert!(!asset_path.is_empty());

    let sound_path = base_path.join(asset_path);
    let sound_hash = hash::fnv1a_from_str(sound_path.as_str());

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

    hash_map.insert(sound_hash, StaticSoundAsset { path: sound_path.to_string(), data: sound_data });
    Key::new(sound_hash)
}

fn load_streamed_sound<Key: SoundKey>(hash_map: &mut PreHashedKeyMap<StringHash, StreamedSoundAsset>,
                                      base_path: &AssetPath,
                                      asset_path: PathRef) -> Key {
    debug_assert!(!asset_path.is_empty());

    let sound_path = base_path.join(asset_path);
    let sound_hash = hash::fnv1a_from_str(sound_path.as_str());

    if hash_map.get(&sound_hash).is_some() {
        return Key::new(sound_hash); // Already loaded.
    }

    // Only probe file path now. Data is lazily loaded on first reference.
    if !sound_path.exists() {
        log::error!(log::channel!("sound"), "Sound file path '{sound_path}' is invalid!");
        return Key::invalid();
    }

    hash_map.insert(sound_hash, StreamedSoundAsset { path: sound_path.to_string() });
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

impl AsTrackHandle for RcMut<TrackHandle> {
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
                let volume_db = super::linear_to_decibels(volume);
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
        let volume_db = super::linear_to_decibels(self.volume * dist_factor);
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
    RcMut<TrackHandle>,
    StaticSoundInstance,
    StaticSoundAsset,
    StaticSoundHandle
>;

// Fake 3D positional sound. Shares the ambiance track.
// Plays sounds that have a specific origin in the world. E.g.: fire, building collapsed.
type SpatialController = SoundController<
    false,
    true,
    RcMut<TrackHandle>,
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
