use common::{Vec2, coords::IsoPointF32};

use crate::{
    file_sys::paths::PathRef,
    sound::SoundSystem,
    ui::{self, DrawDebugUi, UiStaticVar, UiSystem},
};

impl DrawDebugUi for SoundSystem {
    fn draw_debug_ui(&mut self, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();

        if !self.is_initialized() {
            ui.text("No Sound System available!");
            return;
        }

        ui.text(common::format_small!("Listener Pos   : {}", self.listener_position()));
        ui.text(common::format_small!("Sounds Playing : {}", self.sounds_playing()));
        ui.text(common::format_small!("Sounds Loaded  : {}", self.sounds_loaded()));

        ui.separator();

        let mut new_settings = self.current_sound_settings();
        new_settings.draw_debug_ui(ui_sys);

        // Only update volumes if anything was changed.
        if new_settings != self.current_sound_settings() {
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
