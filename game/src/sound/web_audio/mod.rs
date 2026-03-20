use crate::{
    utils::{
        coords::IsoPointF32,
        hash::StringHash,
        paths::PathRef,
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
    listener_position: IsoPointF32,
}

impl SoundSystemBackend for WebAudioSoundSystemBackend {
    type Registry = WebAudioSoundAssetRegistry;

    fn new() -> Option<Box<Self>> {
        // TODO: Create AudioContext via web-sys.
        // Browser autoplay policy may require deferring until first user interaction.
        // Return None for now — SoundSystem handles this gracefully (no audio).
        None
    }

    fn update(&mut self, listener_position: IsoPointF32, _settings: &SoundGlobalSettings) {
        self.listener_position = listener_position;
    }

    fn set_volumes(&mut self, _settings: &SoundGlobalSettings) {}

    fn listener_position(&self) -> IsoPointF32 {
        self.listener_position
    }

    fn sounds_playing(&self) -> usize {
        0
    }

    fn play(&mut self, _params: PlaySoundParams<Self::Registry>) -> SoundHandle {
        SoundHandle::invalid(SoundKind::Sfx)
    }

    fn stop(&mut self, _sound_handle: SoundHandle, _settings: &SoundGlobalSettings) {}

    fn stop_kind(&mut self, _kind: SoundKind, _fade_out: Seconds) {}

    fn stop_all(&mut self, _settings: &SoundGlobalSettings) {}

    fn is_playing(&self, _sound_handle: SoundHandle) -> bool {
        false
    }
}

// ----------------------------------------------
// WebAudioSoundAssetRegistry
// ----------------------------------------------

pub struct WebAudioSoundAssetRegistry;

impl SoundAssetRegistry for WebAudioSoundAssetRegistry {
    fn new() -> Self {
        Self
    }

    fn load_sfx(&mut self, _path: PathRef) -> SfxSoundKey {
        SfxSoundKey::new(StringHash::default())
    }

    fn load_ambience(&mut self, _path: PathRef) -> AmbienceSoundKey {
        AmbienceSoundKey::new(StringHash::default())
    }

    fn load_music(&mut self, _path: PathRef) -> MusicSoundKey {
        MusicSoundKey::new(StringHash::default())
    }

    fn load_narration(&mut self, _path: PathRef) -> NarrationSoundKey {
        NarrationSoundKey::new(StringHash::default())
    }

    fn unload_all(&mut self) {}

    fn sounds_loaded(&self) -> usize {
        0
    }
}
