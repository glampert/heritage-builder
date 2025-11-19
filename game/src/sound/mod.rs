use slab::Slab;

use std::{
    time::Duration,
    collections::VecDeque,
    path::{Path, PathBuf, MAIN_SEPARATOR},
};

use kira::{
    backend::DefaultBackend,
    AudioManager, AudioManagerSettings,
    StartTime, Tween, Easing, track::{TrackHandle, TrackBuilder},
    sound::{
        FromFileError,
        static_sound::{StaticSoundData, StaticSoundHandle},
        streaming::{StreamingSoundData, StreamingSoundHandle},
    },
};

use crate::{
    log,
    engine::time::Seconds,
    utils::{hash::{self, StringHash, PreHashedKeyMap}, platform::paths, mem, Vec2},
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
// SoundEvent
// ----------------------------------------------

#[derive(Copy, Clone)]
pub enum SoundEvent {
    PlaySfx(SfxSoundKey, Seconds, bool), // With optional fade in time and looping.
    PlayAmbience(AmbienceSoundKey),
    PlaySpatialAmbience(AmbienceSoundKey, Vec2), // sound key + world position.
    PlayMusic(MusicSoundKey),
    PlayNarration(NarrationSoundKey),

    // Stop all sounds on these tracks, with optional fade out time for SFXs.
    StopSfx(Seconds),
    StopAmbience,
    StopSpatialAmbience,
    StopMusic,
    StopNarration,

    // Stop specific sound, with optional fade out time for SFXs.
    Stop(SoundHandle, Seconds),

    // Stops all currently playing sounds, with optional fade out time for SFXs.
    StopAll(Seconds),
}

// ----------------------------------------------
// SoundGlobalSettings
// ----------------------------------------------

#[derive(Copy, Clone)]
pub struct SoundGlobalSettings {
    // Cutoff distance from the camera where we mute spatial sounds.
    pub spatial_cutoff_distance: f32,
    pub spatial_transition_secs: Seconds,

    // Fade times:
    pub ambience_fade_in_secs: Seconds,
    pub ambience_fade_out_secs: Seconds,
    pub spatial_fade_in_secs: Seconds,
    pub spatial_fade_out_secs: Seconds,
    pub music_fade_in_secs: Seconds,
    pub music_fade_out_secs: Seconds,
    pub narration_fade_in_secs: Seconds,
    pub narration_fade_out_secs: Seconds,
}

impl Default for SoundGlobalSettings {
    fn default() -> Self {
        Self {
            spatial_cutoff_distance: 20.0,
            spatial_transition_secs: 5.0,
            ambience_fade_in_secs: 5.0,
            ambience_fade_out_secs: 5.0,
            spatial_fade_in_secs: 5.0,
            spatial_fade_out_secs: 5.0,
            music_fade_in_secs: 5.0,
            music_fade_out_secs: 5.0,
            narration_fade_in_secs: 5.0,
            narration_fade_out_secs: 5.0,
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
    event_queue: VecDeque<SoundEvent>,
}

impl SoundSystem {
    pub fn new() -> Self {
        Self {
            backend: SoundSystemBackend::new(),
            registry: SoundAssetRegistry::new(),
            settings: SoundGlobalSettings::default(),
            event_queue: VecDeque::with_capacity(64),
        }
    }

    pub fn settings(&self) -> &SoundGlobalSettings {
        &self.settings
    }

    pub fn update_settings(&mut self, new_settings: SoundGlobalSettings) {
        self.settings = new_settings;
    }

    pub fn update(&mut self, camera_world_position: Vec2) {
        if !self.is_initialized() {
            // If backend failed to initialize we'll operate as a no-op/null SoundSystem.
            return;
        }

        let backend = self.backend.as_mut().unwrap();
        backend.spatial.update(camera_world_position, &self.settings);

        while let Some(event) = self.event_queue.pop_front() {
            backend.process_event(event, &self.registry, &self.settings);
        }
    }

    pub fn push_event(&mut self, event: SoundEvent) {
        if !self.is_initialized() {
            return;
        }
        self.event_queue.push_back(event);
    }

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

    #[inline]
    fn is_initialized(&self) -> bool {
        self.backend.is_some()
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

        let sfx = SfxController::new(sfx_track);
        let ambience = AmbienceController::new(ambience_track);
        let spatial = SpatialController::new(&ambience.track); // Track shared with ambience.
        let music = MusicController::new(music_track, SoundKind::Music);
        let narration = NarrationController::new(narration_track, SoundKind::Narration);

        Some(Box::new(Self {
            manager,
            sfx,
            ambience,
            spatial,
            music,
            narration,
        }))
    }

    fn process_event(&mut self, event: SoundEvent, registry: &SoundAssetRegistry, settings: &SoundGlobalSettings) {
        match event {
            SoundEvent::PlaySfx(sound_key, fade_in_time_secs, looping) => {
                if let Some(sound) = registry.sfx.get(&sound_key.hash) {
                    self.sfx.play(sound, fade_in_time_secs, looping);
                }
            }
            SoundEvent::PlayAmbience(sound_key) => {
                if let Some(sound) = registry.ambience.get(&sound_key.hash) {
                    self.ambience.play(sound, settings.ambience_fade_in_secs);
                }
            }
            SoundEvent::PlaySpatialAmbience(sound_key, world_position) => {
               if let Some(sound) = registry.ambience.get(&sound_key.hash) { // Same as ambience.
                    self.spatial.play(sound, world_position, settings.spatial_fade_in_secs);
                }
            }
            SoundEvent::PlayMusic(sound_key) => {
                if let Some(sound) = registry.music.get(&sound_key.hash) {
                    self.music.play(sound, settings.music_fade_in_secs, settings.music_fade_out_secs);
                }
            }
            SoundEvent::PlayNarration(sound_key) => {
                if let Some(sound) = registry.narration.get(&sound_key.hash) {
                    self.narration.play(sound, settings.narration_fade_in_secs, settings.narration_fade_out_secs);
                }
            }
            SoundEvent::StopSfx(fade_out_time_secs) => self.sfx.stop_all(fade_out_time_secs),
            SoundEvent::StopAmbience => self.ambience.stop_all(settings.ambience_fade_out_secs),
            SoundEvent::StopSpatialAmbience => self.spatial.stop_all(settings.spatial_fade_out_secs),
            SoundEvent::StopMusic => self.music.stop_all(settings.music_fade_out_secs),
            SoundEvent::StopNarration => self.narration.stop_all(settings.narration_fade_out_secs),
            SoundEvent::Stop(sound_handle, fade_out_time_secs) => {
                match sound_handle.kind {
                    SoundKind::Sfx => self.sfx.stop(sound_handle, fade_out_time_secs),
                    SoundKind::Ambience => self.ambience.stop(sound_handle, settings.ambience_fade_out_secs),
                    SoundKind::SpatialAmbience => self.spatial.stop(sound_handle, settings.spatial_fade_out_secs),
                    SoundKind::Music => self.music.stop(sound_handle, settings.music_fade_out_secs),
                    SoundKind::Narration => self.narration.stop(sound_handle, settings.narration_fade_out_secs),
                }
            }
            SoundEvent::StopAll(fade_out_time_secs) => {
                self.sfx.stop_all(fade_out_time_secs);
                self.ambience.stop_all(settings.ambience_fade_out_secs);
                self.spatial.stop_all(settings.spatial_fade_out_secs);
                self.music.stop_all(settings.music_fade_out_secs);
                self.narration.stop_all(settings.narration_fade_out_secs);
            }
        }
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

        // Probe file. Data lazily loaded on first reference.
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
}

// ----------------------------------------------
// SfxController
// ----------------------------------------------

// Plays general sound effects like UI clicks, alerts, popups.
struct SfxController {
    track: TrackHandle,
    sounds: Slab<(u32, StaticSoundHandle)>,
    generation: u32,
}

impl SfxController {
    fn new(track: TrackHandle) -> Self {
        Self {
            track,
            sounds: Slab::new(),
            generation: 0,
        }
    }

    fn play(&mut self, sound: &StaticSound, fade_in_time_secs: Seconds, looping: bool) -> SoundHandle {
        let sound_data = sound.data.clone();

        let mut handle = match self.track.play(
            sound_data.fade_in_tween(Tween {
                start_time: StartTime::Immediate,
                duration: Duration::from_secs_f32(fade_in_time_secs),
                easing: Easing::Linear,
        }))
        {
            Ok(handle) => handle,
            Err(err) => {
                log::warn!(log::channel!("sound"), "Failed to play sound effect: {err}");
                return SoundHandle::invalid(SoundKind::Sfx);
            }
        };

        if looping {
            // Play in a loop until told to stop.
            handle.set_loop_region(..);
        }

        self.generation += 1;
        let index = self.sounds.insert((self.generation, handle));

        SoundHandle::new(SoundKind::Sfx, index, self.generation)
    }

    fn stop(&mut self, sound_handle: SoundHandle, fade_out_time_secs: Seconds) {
        if !sound_handle.is_valid() || sound_handle.kind != SoundKind::Sfx {
            return;
        }

        if let Some((generation, handle)) = self.sounds.get_mut(sound_handle.index as usize) {
            if *generation == sound_handle.generation {
                handle.stop(Tween {
                    start_time: StartTime::Immediate,
                    duration: Duration::from_secs_f32(fade_out_time_secs),
                    easing: Easing::Linear,
                });
                self.sounds.remove(sound_handle.index as usize);
            }
        }
    }

    fn stop_all(&mut self, fade_out_time_secs: Seconds) {
        for (_, (_, handle)) in &mut self.sounds {
            handle.stop(Tween {
                start_time: StartTime::Immediate,
                duration: Duration::from_secs_f32(fade_out_time_secs),
                easing: Easing::Linear,
            });
        }
        self.sounds.clear();
    }
}

// ----------------------------------------------
// AmbienceController
// ----------------------------------------------

// Plays non-positional ambience sounds. E.g.: global city noises, nature sounds.
struct AmbienceController {
    track: Box<TrackHandle>,
    sounds: Slab<(u32, StaticSoundHandle)>,
    generation: u32,
}

impl AmbienceController {
    fn new(track: TrackHandle) -> Self {
        Self {
            track: Box::new(track),
            sounds: Slab::new(),
            generation: 0,
        }
    }

    fn play(&mut self, sound: &StaticSound, fade_in_secs: Seconds) -> SoundHandle {
        let sound_data = sound.data.clone();

        let mut handle = match self.track.play(
            sound_data.fade_in_tween(Tween {
                start_time: StartTime::Immediate,
                duration: Duration::from_secs_f32(fade_in_secs),
                easing: Easing::Linear,
        }))
        {
            Ok(handle) => handle,
            Err(err) => {
                log::warn!(log::channel!("sound"), "Failed to play ambiance loop: {err}");
                return SoundHandle::invalid(SoundKind::Ambience);
            }
        };

        // Play in a loop until told to stop.
        handle.set_loop_region(..);

        self.generation += 1;
        let index = self.sounds.insert((self.generation, handle));

        SoundHandle::new(SoundKind::Ambience, index, self.generation)
    }

    fn stop(&mut self, sound_handle: SoundHandle, fade_out_secs: Seconds) {
        if !sound_handle.is_valid() || sound_handle.kind != SoundKind::Ambience {
            return;
        }

        if let Some((generation, handle)) = self.sounds.get_mut(sound_handle.index as usize) {
            if *generation == sound_handle.generation {
                handle.stop(Tween {
                    start_time: StartTime::Immediate,
                    duration: Duration::from_secs_f32(fade_out_secs),
                    easing: Easing::Linear,
                });
                self.sounds.remove(sound_handle.index as usize);
            }
        }
    }

    fn stop_all(&mut self, fade_out_secs: Seconds) {
        for (_, (_, handle)) in &mut self.sounds {
            handle.stop(Tween {
                start_time: StartTime::Immediate,
                duration: Duration::from_secs_f32(fade_out_secs),
                easing: Easing::Linear,
            });
        }
        self.sounds.clear();
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
    world_position: Vec2,
    base_volume: f32,
}

impl SpatialController {
    fn new(track: &TrackHandle) -> Self {
        Self {
            track: mem::RawPtr::from_ref(track),
            sounds: Slab::new(),
            generation: 0,
        }
    }

    fn play(&mut self, sound: &StaticSound, world_position: Vec2, fade_in_secs: Seconds) -> SoundHandle {
        let sound_data = sound.data.clone();

        let mut handle = match self.track.play(
            sound_data.fade_in_tween(Tween {
                start_time: StartTime::Immediate,
                duration: Duration::from_secs_f32(fade_in_secs),
                easing: Easing::Linear,
        }))
        {
            Ok(handle) => handle,
            Err(err) => {
                log::warn!(log::channel!("sound"), "Failed to play spatial ambiance loop: {err}");
                return SoundHandle::invalid(SoundKind::SpatialAmbience);
            }
        };

        // Play in a loop until told to stop.
        handle.set_loop_region(..);

        self.generation += 1;
        let index = self.sounds.insert((self.generation, SpatialSound { handle, world_position, base_volume: 1.0 }));

        SoundHandle::new(SoundKind::SpatialAmbience, index, self.generation)
    }

    fn stop(&mut self, sound_handle: SoundHandle, fade_out_secs: Seconds) {
        if !sound_handle.is_valid() || sound_handle.kind != SoundKind::SpatialAmbience {
            return;
        }

        if let Some((generation, sound)) = self.sounds.get_mut(sound_handle.index as usize) {
            if *generation == sound_handle.generation {
                sound.handle.stop(Tween {
                    start_time: StartTime::Immediate,
                    duration: Duration::from_secs_f32(fade_out_secs),
                    easing: Easing::Linear,
                });
                self.sounds.remove(sound_handle.index as usize);
            }
        }
    }

    fn stop_all(&mut self, fade_out_secs: Seconds) {
        for (_, (_, sound)) in &mut self.sounds {
            sound.handle.stop(Tween {
                start_time: StartTime::Immediate,
                duration: Duration::from_secs_f32(fade_out_secs),
                easing: Easing::Linear,
            });
        }
        self.sounds.clear();
    }

    fn update(&mut self, camera_world_position: Vec2, settings: &SoundGlobalSettings) {
        for (_, (_, sound)) in &mut self.sounds {
            let dx = sound.world_position.x - camera_world_position.x;
            let dy = sound.world_position.y - camera_world_position.y;

            let distance = ((dx * dx) + (dy * dy)).sqrt();
            let dist_factor = 1.0 - (distance / settings.spatial_cutoff_distance).clamp(0.0, 1.0);

            let volume = sound.base_volume * dist_factor;
            sound.handle.set_volume(volume, Tween {
                start_time: StartTime::Immediate,
                duration: Duration::from_secs_f32(settings.spatial_transition_secs),
                easing: Easing::Linear,
            });

            let panning = (dx / settings.spatial_cutoff_distance).clamp(-1.0, 1.0);
            sound.handle.set_panning(panning, Tween {
                start_time: StartTime::Immediate,
                duration: Duration::from_secs_f32(settings.spatial_transition_secs),
                easing: Easing::Linear,
            });
        }
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

    fn play(&mut self, sound: &StreamedSound, fade_in_secs: Seconds, fade_out_secs: Seconds) -> SoundHandle {
        // Stop current if any is already playing.
        self.stop_all(fade_out_secs);

        debug_assert!(!sound.path.is_empty());
        let sound_data = match StreamingSoundData::from_file(&sound.path) {
            Ok(data) => data,
            Err(err) => {
                log::error!(log::channel!("sound"), "Failed to load streamed sound '{}': {err}", sound.path);
                return SoundHandle::invalid(self.kind);
            }
        };

        let handle = match self.track.play(
            sound_data.fade_in_tween(Tween {
                start_time: StartTime::Immediate,
                duration: Duration::from_secs_f32(fade_in_secs),
                easing: Easing::Linear,
        }))
        {
            Ok(handle) => handle,
            Err(err) => {
                log::warn!(log::channel!("sound"), "Failed to play streamed sound: {err}");
                return SoundHandle::invalid(self.kind);
            }
        };

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
                start_time: StartTime::Immediate,
                duration: Duration::from_secs_f32(fade_out_secs),
                easing: Easing::Linear,
            });
        }
        self.current = None;
    }
}

// ----------------------------------------------
// MusicController / NarrationController
// ----------------------------------------------

// Plays streamed background soundtrack music.
type MusicController = StreamedSoundController;

// Plays streamed narration / voice-over tracks.
type NarrationController = StreamedSoundController;
