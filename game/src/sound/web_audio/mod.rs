// Web Audio API backend for the browser.
//
// Maps to the same SoundSystemBackend / SoundAssetRegistry traits as the Kira
// desktop backend, using the browser's AudioContext for playback.
//
// Architecture:
//   AudioContext
//     └─ destination
//          ├─ sfxGain        (SFX master volume)
//          ├─ ambienceGain   (Ambience + Spatial master volume)
//          ├─ musicGain      (Music master volume)
//          └─ narrationGain  (Narration master volume)
//
// Each playing sound gets its own chain:
//   AudioBufferSourceNode → GainNode (per-sound volume/fade)
//                         → [StereoPannerNode] (spatial only)
//                         → track GainNode
//
// Autoplay policy: AudioContext is created in `new()` but may be in
// "suspended" state. We attempt `ctx.resume()` on first play.

use std::{cell::RefCell, rc::Rc};
use slab::Slab;

use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

use crate::{
    log,
    file_sys::{
        self,
        paths::{self, PathRef, AssetPath},
    },
    utils::{
        coords::IsoPointF32,
        hash::{self, StringHash, PreHashedKeyMap},
        time::Seconds,
    },
};

use super::{
    PlaySoundParams,
    SoundAssetRegistry, SoundSystemBackend,
    SoundGlobalSettings, SoundHandle, SoundKind, SoundKey,
    SfxSoundKey, AmbienceSoundKey, MusicSoundKey, NarrationSoundKey,
};

// ----------------------------------------------
// WebAudioSoundSystemBackend
// ----------------------------------------------

pub struct WebAudioSoundSystemBackend {
    ctx: web_sys::AudioContext,

    // Per-kind master gain nodes, connected to ctx.destination.
    sfx_gain: web_sys::GainNode,
    ambience_gain: web_sys::GainNode,
    music_gain: web_sys::GainNode,
    narration_gain: web_sys::GainNode,

    // Per-kind sound instance pools.
    sfx: SoundPool,
    ambience: SoundPool,
    spatial: SoundPool,
    music: SoundPool,
    narration: SoundPool,

    listener_position: IsoPointF32,
    resumed: bool,
}

impl SoundSystemBackend for WebAudioSoundSystemBackend {
    type Registry = WebAudioSoundAssetRegistry;

    fn new() -> Option<Box<Self>> {
        let ctx = match web_sys::AudioContext::new() {
            Ok(ctx) => ctx,
            Err(err) => {
                log::error!(log::channel!("sound"), "Failed to create AudioContext: {err:?}");
                return None;
            }
        };

        let destination = ctx.destination();

        let sfx_gain = create_gain_node(&ctx, &destination)?;
        let ambience_gain = create_gain_node(&ctx, &destination)?;
        let music_gain = create_gain_node(&ctx, &destination)?;
        let narration_gain = create_gain_node(&ctx, &destination)?;

        log::info!(log::channel!("sound"),
            "WebAudio initialized. State: {:?}, SampleRate: {}",
            ctx.state(), ctx.sample_rate());

        Some(Box::new(Self {
            ctx,
            sfx_gain,
            ambience_gain,
            music_gain,
            narration_gain,
            sfx: SoundPool::new(SoundKind::Sfx),
            ambience: SoundPool::new(SoundKind::Ambience),
            spatial: SoundPool::new(SoundKind::SpatialAmbience),
            music: SoundPool::new(SoundKind::Music),
            narration: SoundPool::new(SoundKind::Narration),
            listener_position: IsoPointF32::default(),
            resumed: false,
        }))
    }

    fn update(&mut self, listener_position: IsoPointF32, settings: &SoundGlobalSettings) {
        self.listener_position = listener_position;

        let now = self.ctx.current_time();
        self.sfx.remove_stopped(now);
        self.ambience.remove_stopped(now);
        self.spatial.remove_stopped(now);
        self.music.remove_stopped(now);
        self.narration.remove_stopped(now);

        // Update spatial sound volumes and panning based on listener position.
        for (_, sound) in &mut self.spatial.sounds {
            sound.spatial_update(&self.ctx, listener_position, settings);
        }
    }

    fn set_volumes(&mut self, settings: &SoundGlobalSettings) {
        let now = self.ctx.current_time();
        set_gain(&self.sfx_gain, settings.sfx_master_volume, now);
        set_gain(&self.ambience_gain, settings.ambience_master_volume, now);
        // Spatial shares ambience gain node.
        set_gain(&self.music_gain, settings.music_master_volume, now);
        set_gain(&self.narration_gain, settings.narration_master_volume, now);
    }

    fn listener_position(&self) -> IsoPointF32 {
        self.listener_position
    }

    fn sounds_playing(&self) -> usize {
        self.sfx.sounds.len()
        + self.ambience.sounds.len()
        + self.spatial.sounds.len()
        + self.music.sounds.len()
        + self.narration.sounds.len()
    }

    fn play(&mut self, params: PlaySoundParams<Self::Registry>) -> SoundHandle {
        // Try to resume AudioContext on first play (browser autoplay policy).
        if !self.resumed {
            self.resumed = true;
            let _ = self.ctx.resume();
        }

        let (pool, track_gain) = match params.kind {
            SoundKind::Sfx             => (&mut self.sfx,        &self.sfx_gain),
            SoundKind::Ambience        => (&mut self.ambience,   &self.ambience_gain),
            SoundKind::SpatialAmbience => (&mut self.spatial,    &self.ambience_gain),
            SoundKind::Music           => (&mut self.music,      &self.music_gain),
            SoundKind::Narration       => (&mut self.narration,  &self.narration_gain),
        };

        // Look up the AudioBuffer for this sound.
        let buffer = match params.kind {
            SoundKind::SpatialAmbience => params.registry.ambience.get(&params.key_hash),
            SoundKind::Sfx             => params.registry.sfx.get(&params.key_hash),
            SoundKind::Ambience        => params.registry.ambience.get(&params.key_hash),
            SoundKind::Music           => params.registry.music.get(&params.key_hash),
            SoundKind::Narration       => params.registry.narration.get(&params.key_hash),
        };

        let buffer = match buffer {
            Some(asset) => match asset.buffer.borrow().clone() {
                Some(buf) => buf,
                None => {
                    log::warning!(log::channel!("sound"),
                        "Sound '{}' not yet decoded, skipping play", asset.path);
                    return SoundHandle::invalid(params.kind);
                }
            },
            None => return SoundHandle::invalid(params.kind),
        };

        let volume  = params.settings.master_volume(params.kind);
        let fade_in = params.settings.fade_in_secs(params.kind);
        let spatial = params.kind == SoundKind::SpatialAmbience;

        // For single-sound kinds (music, narration), stop current before playing.
        if matches!(params.kind, SoundKind::Music | SoundKind::Narration) {
            let fade_out = params.settings.fade_out_secs(params.kind);
            pool.stop_all(&self.ctx, fade_out);
        }

        match WebAudioSoundInstance::new(
            &self.ctx, &buffer, track_gain, volume, fade_in,
            params.looping, spatial, params.position,
        ) {
            Some(instance) => pool.insert(instance),
            None => SoundHandle::invalid(params.kind),
        }
    }

    fn stop(&mut self, sound_handle: SoundHandle, settings: &SoundGlobalSettings) {
        if !sound_handle.is_valid() {
            return;
        }
        let fade_out = settings.fade_out_secs(sound_handle.kind);
        let pool = match sound_handle.kind {
            SoundKind::Sfx             => &mut self.sfx,
            SoundKind::Ambience        => &mut self.ambience,
            SoundKind::SpatialAmbience => &mut self.spatial,
            SoundKind::Music           => &mut self.music,
            SoundKind::Narration       => &mut self.narration,
        };
        pool.stop_one(&self.ctx, sound_handle, fade_out);
    }

    fn stop_kind(&mut self, kind: SoundKind, fade_out: Seconds) {
        let pool = match kind {
            SoundKind::Sfx             => &mut self.sfx,
            SoundKind::Ambience        => &mut self.ambience,
            SoundKind::SpatialAmbience => &mut self.spatial,
            SoundKind::Music           => &mut self.music,
            SoundKind::Narration       => &mut self.narration,
        };
        pool.stop_all(&self.ctx, fade_out);
    }

    fn stop_all(&mut self, settings: &SoundGlobalSettings) {
        self.sfx.stop_all(&self.ctx, settings.fade_out_secs(SoundKind::Sfx));
        self.ambience.stop_all(&self.ctx, settings.fade_out_secs(SoundKind::Ambience));
        self.spatial.stop_all(&self.ctx, settings.fade_out_secs(SoundKind::SpatialAmbience));
        self.music.stop_all(&self.ctx, settings.fade_out_secs(SoundKind::Music));
        self.narration.stop_all(&self.ctx, settings.fade_out_secs(SoundKind::Narration));
    }

    fn is_playing(&self, sound_handle: SoundHandle) -> bool {
        let pool = match sound_handle.kind {
            SoundKind::Sfx             => &self.sfx,
            SoundKind::Ambience        => &self.ambience,
            SoundKind::SpatialAmbience => &self.spatial,
            SoundKind::Music           => &self.music,
            SoundKind::Narration       => &self.narration,
        };
        pool.is_playing(sound_handle)
    }
}

// ----------------------------------------------
// WebAudioSoundAssetRegistry
// ----------------------------------------------

struct WebAudioSoundAsset {
    path: String,
    // AudioBuffer decoded from the raw file bytes.
    // Wrapped in Rc<RefCell> because decodeAudioData is async —
    // the buffer is None until decoding completes.
    buffer: Rc<RefCell<Option<web_sys::AudioBuffer>>>,
}

pub struct WebAudioSoundAssetRegistry {
    sfx:       PreHashedKeyMap<StringHash, WebAudioSoundAsset>,
    ambience:  PreHashedKeyMap<StringHash, WebAudioSoundAsset>,
    music:     PreHashedKeyMap<StringHash, WebAudioSoundAsset>,
    narration: PreHashedKeyMap<StringHash, WebAudioSoundAsset>,
    paths:     Box<SoundAssetPaths>,
}

struct SoundAssetPaths {
    sfx: AssetPath,
    ambience: AssetPath,
    music: AssetPath,
    narration: AssetPath,
}

impl SoundAssetRegistry for WebAudioSoundAssetRegistry {
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

    fn load_sfx(&mut self, path: PathRef) -> SfxSoundKey {
        load_sound(&mut self.sfx, &self.paths.sfx, path)
    }

    fn load_ambience(&mut self, path: PathRef) -> AmbienceSoundKey {
        load_sound(&mut self.ambience, &self.paths.ambience, path)
    }

    fn load_music(&mut self, path: PathRef) -> MusicSoundKey {
        load_sound(&mut self.music, &self.paths.music, path)
    }

    fn load_narration(&mut self, path: PathRef) -> NarrationSoundKey {
        load_sound(&mut self.narration, &self.paths.narration, path)
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

fn load_sound<Key: SoundKey>(hash_map: &mut PreHashedKeyMap<StringHash, WebAudioSoundAsset>,
                             base_path: &AssetPath,
                             asset_path: PathRef) -> Key
{
    debug_assert!(!asset_path.is_empty());

    let sound_path = base_path.join(asset_path);
    let sound_hash = hash::fnv1a_from_str(sound_path.as_str());

    if hash_map.get(&sound_hash).is_some() {
        return Key::new(sound_hash); // Already loaded.
    }

    // Read raw audio bytes from the WASM asset cache.
    let bytes = match file_sys::load_bytes(&sound_path) {
        Ok(bytes) => bytes,
        Err(err) => {
            log::error!(log::channel!("sound"), "Failed to load sound '{sound_path}': {err}");
            return Key::invalid();
        }
    };

    // Decode asynchronously via AudioContext.decodeAudioData.
    let buffer_cell: Rc<RefCell<Option<web_sys::AudioBuffer>>> = Rc::new(RefCell::new(None));
    let buffer_cell_clone = buffer_cell.clone();
    let path = sound_path.to_string();

    wasm_bindgen_futures::spawn_local(async move {
        match decode_audio_data(&bytes).await {
            Ok(audio_buffer) => {
                *buffer_cell_clone.borrow_mut() = Some(audio_buffer);
            }
            Err(err) => {
                log::error!(log::channel!("sound"),
                    "Failed to decode audio '{sound_path}': {err}");
            }
        }
    });

    hash_map.insert(sound_hash, WebAudioSoundAsset { path, buffer: buffer_cell });
    Key::new(sound_hash)
}

async fn decode_audio_data(bytes: &[u8]) -> Result<web_sys::AudioBuffer, String> {
    // We need a temporary AudioContext for decoding. Re-use the global window
    // one would be ideal, but we don't have access to it here. Creating a
    // short-lived one is fine — browsers handle this efficiently.
    let ctx = web_sys::AudioContext::new()
        .map_err(|e| format!("AudioContext::new failed: {e:?}"))?;

    // Copy bytes into a JS ArrayBuffer.
    let js_array = js_sys::Uint8Array::new_with_length(bytes.len() as u32);
    js_array.copy_from(bytes);

    let array_buffer = js_array.buffer();

    let promise = ctx.decode_audio_data(&array_buffer)
        .map_err(|e| format!("decodeAudioData failed: {e:?}"))?;

    let result = JsFuture::from(promise).await
        .map_err(|e| format!("decodeAudioData rejected: {e:?}"))?;

    result.dyn_into::<web_sys::AudioBuffer>()
        .map_err(|_| "decodeAudioData did not return an AudioBuffer".to_string())
}

// ----------------------------------------------
// WebAudioSoundInstance
// ----------------------------------------------

struct WebAudioSoundInstance {
    source: web_sys::AudioBufferSourceNode,
    gain: web_sys::GainNode,
    panner: Option<web_sys::StereoPannerNode>,
    generation: u32,
    position: IsoPointF32,
    volume: f32,
    // Time (ctx.currentTime) at which this sound will stop naturally.
    // f64::MAX for looping sounds.
    end_time: f64,
    // Set to true after stop() is called, so we don't double-stop.
    stopping: bool,
}

impl WebAudioSoundInstance {
    fn new(ctx: &web_sys::AudioContext,
           buffer: &web_sys::AudioBuffer,
           track_gain: &web_sys::GainNode,
           volume: f32,
           fade_in_secs: Seconds,
           looping: bool,
           spatial: bool,
           position: IsoPointF32) -> Option<Self>
    {
        // Create source node.
        let source = ctx.create_buffer_source().ok()?;
        source.set_buffer(Some(buffer));
        source.set_loop(looping);

        // Create per-sound gain node for volume control and fading.
        let gain = ctx.create_gain().ok()?;
        let now = ctx.current_time();

        if fade_in_secs > 0.0 {
            // Start silent, ramp to target volume.
            gain.gain().set_value_at_time(0.0, now).ok()?;
            gain.gain().linear_ramp_to_value_at_time(volume, now + fade_in_secs as f64).ok()?;
        } else {
            gain.gain().set_value(volume);
        }

        // Wire the audio graph: source → gain → [panner] → track_gain.
        source.connect_with_audio_node(&gain).ok()?;

        let panner = if spatial {
            let panner = ctx.create_stereo_panner().ok()?;
            gain.connect_with_audio_node(&panner).ok()?;
            panner.connect_with_audio_node(track_gain).ok()?;
            Some(panner)
        } else {
            gain.connect_with_audio_node(track_gain).ok()?;
            None
        };

        // Start playback.
        source.start().ok()?;

        let end_time = if looping {
            f64::MAX
        } else {
            now + buffer.duration()
        };

        Some(Self {
            source,
            gain,
            panner,
            generation: 0, // Set by SoundPool::insert.
            position,
            volume,
            end_time,
            stopping: false,
        })
    }

    fn is_playing(&self, now: f64) -> bool {
        !self.stopping && now < self.end_time
    }

    fn is_stopped(&self, now: f64) -> bool {
        self.stopping || now >= self.end_time
    }

    fn stop(&mut self, ctx: &web_sys::AudioContext, fade_out_secs: Seconds) {
        if self.stopping {
            return;
        }
        self.stopping = true;

        let now = ctx.current_time();
        let scheduled: &web_sys::AudioScheduledSourceNode = self.source.as_ref();

        if fade_out_secs > 0.0 {
            // Fade out then stop.
            let stop_time = now + fade_out_secs as f64;
            let _ = self.gain.gain().cancel_scheduled_values(now);
            let _ = self.gain.gain().set_value_at_time(self.gain.gain().value(), now);
            let _ = self.gain.gain().linear_ramp_to_value_at_time(0.0, stop_time);
            let _ = scheduled.stop_with_when(stop_time);
            self.end_time = stop_time;
        } else {
            let _ = scheduled.stop();
            self.end_time = now;
        }
    }

    fn set_volume(&mut self, volume: f32) {
        self.volume = volume;
        self.gain.gain().set_value(volume);
    }

    fn spatial_update(&mut self,
                      ctx: &web_sys::AudioContext,
                      listener_position: IsoPointF32,
                      settings: &SoundGlobalSettings)
    {
        let dx = self.position.0.x - listener_position.0.x;
        let dy = self.position.0.y - listener_position.0.y;

        let distance = ((dx * dx) + (dy * dy)).sqrt();
        let dist_factor = 1.0 - (distance / settings.spatial_cutoff_distance).clamp(0.0, 1.0);

        // Adjust per-sound gain based on distance.
        let target_volume = self.volume * dist_factor;
        let now = ctx.current_time();
        let ramp_end = now + settings.spatial_transition_secs as f64;
        let _ = self.gain.gain().cancel_scheduled_values(now);
        let _ = self.gain.gain().set_value_at_time(self.gain.gain().value(), now);
        let _ = self.gain.gain().linear_ramp_to_value_at_time(target_volume, ramp_end);

        // Panning: -1.0 = hard left, 0.0 = center, 1.0 = hard right.
        if let Some(panner) = &self.panner {
            let pan = (dx / settings.spatial_cutoff_distance).clamp(-1.0, 1.0);
            let _ = panner.pan().cancel_scheduled_values(now);
            let _ = panner.pan().set_value_at_time(panner.pan().value(), now);
            let _ = panner.pan().linear_ramp_to_value_at_time(pan, ramp_end);
        }
    }
}

// ----------------------------------------------
// SoundPool
// ----------------------------------------------

struct SoundPool {
    kind: SoundKind,
    sounds: Slab<WebAudioSoundInstance>,
    generation: u32,
}

impl SoundPool {
    fn new(kind: SoundKind) -> Self {
        Self {
            kind,
            sounds: Slab::new(),
            generation: 0,
        }
    }

    fn insert(&mut self, mut instance: WebAudioSoundInstance) -> SoundHandle {
        self.generation += 1;
        instance.generation = self.generation;
        let index = self.sounds.insert(instance);
        SoundHandle::new(self.kind, index, self.generation)
    }

    fn is_playing(&self, handle: SoundHandle) -> bool {
        if let Some(sound) = self.sounds.get(handle.index as usize) {
            if sound.generation == handle.generation {
                // We can't easily get ctx.current_time() here without storing it,
                // so we check the stopping flag as a proxy.
                return !sound.stopping;
            }
        }
        false
    }

    fn stop_one(&mut self, ctx: &web_sys::AudioContext, handle: SoundHandle, fade_out: Seconds) {
        if let Some(sound) = self.sounds.get_mut(handle.index as usize) {
            if sound.generation == handle.generation {
                sound.stop(ctx, fade_out);
            }
        }
    }

    fn stop_all(&mut self, ctx: &web_sys::AudioContext, fade_out: Seconds) {
        for (_, sound) in &mut self.sounds {
            sound.stop(ctx, fade_out);
        }
    }

    fn remove_stopped(&mut self, now: f64) {
        let to_remove: smallvec::SmallVec<[usize; 32]> = self.sounds.iter()
            .filter(|(_, s)| s.is_stopped(now))
            .map(|(i, _)| i)
            .collect();

        for index in to_remove {
            self.sounds.remove(index);
        }
    }
}

// ----------------------------------------------
// Helpers
// ----------------------------------------------

fn create_gain_node(ctx: &web_sys::AudioContext,
                    destination: &web_sys::AudioDestinationNode)
                    -> Option<web_sys::GainNode>
{
    let gain = match ctx.create_gain() {
        Ok(g) => g,
        Err(err) => {
            log::error!(log::channel!("sound"), "Failed to create GainNode: {err:?}");
            return None;
        }
    };

    if let Err(err) = gain.connect_with_audio_node(destination) {
        log::error!(log::channel!("sound"), "Failed to connect GainNode to destination: {err:?}");
        return None;
    }

    Some(gain)
}

fn set_gain(gain: &web_sys::GainNode, volume: f32, now: f64) {
    // Smooth transition to avoid clicks.
    let _ = gain.gain().cancel_scheduled_values(now);
    let _ = gain.gain().set_value_at_time(gain.gain().value(), now);
    let _ = gain.gain().linear_ramp_to_value_at_time(volume, now + 0.05);
}
