use strum::{IntoEnumIterator, VariantNames};
use common::{Color, format_small};
use engine::{
    Engine,
    ui::{DrawDebugUi, UiStaticVar, UiSystem},
};

use crate::{
    sim::{SimCmds, SimContext},
    tile::{TileFlags, TileKind},
    system::{
        GameSystem,
        GameSystemImpl,
        GameSystems,
        ambient_effects::{AmbientEffectsSystem, BirdFlightPath, spawn_bird, spawn_bird_with_random_flight_path},
        ambient_music::AmbientMusicSystem,
        ambient_sounds::AmbientSoundsSystem,
        settlers::SettlersSpawnSystem,
    },
};

// ----------------------------------------------
// GameSystems Debug UI
// ----------------------------------------------

impl GameSystems {
    pub(crate) fn draw_debug_ui(&mut self, engine: &mut Engine, cmds: &mut SimCmds, context: &SimContext, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();
        if let Some(_tab_bar) = ui.tab_bar("Game Systems Tab Bar") {
            for entry in &mut self.systems {
                if let Some(_tab) = ui.tab_item(entry.system.to_string()) {
                    entry.system.draw_debug_ui(engine, cmds, context);
                }
            }

            if let Some(_tab) = ui.tab_item("Create Systems") {
                ui.text("Create and register system if not already created.");

                static SYSTEM_INDEX: UiStaticVar<usize> = UiStaticVar::new(0);
                ui.combo_simple_string("Systems", SYSTEM_INDEX.as_mut(), GameSystemImpl::VARIANTS);

                if ui.button("Create") {
                    if let Some(system) = GameSystemImpl::iter().nth(*SYSTEM_INDEX) {
                        if !self.has(system.as_any().type_id()) {
                            self.register(system);
                        }
                    }
                }
            }
        }
    }
}

// ----------------------------------------------
// SettlersSpawnSystem Debug UI
// ----------------------------------------------

impl SettlersSpawnSystem {
    pub(crate) fn draw_debug_ui_dispatch(&mut self, engine: &mut Engine, cmds: &mut SimCmds, context: &SimContext) {
        self.spawn_timer.draw_debug_ui_with_header("Settler Spawn", engine.ui_system());

        let ui = engine.ui_system().ui();

        let color_text = |text: &str, cond: bool| {
            ui.text(text);
            ui.same_line();
            if cond {
                ui.text_colored(Color::green().to_array(), "yes");
            } else {
                ui.text_colored(Color::red().to_array(), "no");
            }
        };

        color_text("Has vacant lots:", Self::has_vacant_lots(context));

        let spawn_point = Self::find_spawn_point(cmds, context);
        ui.text(format_small!("Spawn Point: {}", spawn_point.cell));

        if ui.input_scalar("Population Per Settler Unit", &mut self.population_per_settler_unit).step(1).build() {
            self.population_per_settler_unit = self.population_per_settler_unit.max(1);
        }

        if ui.button("Force Spawn Now") {
            self.spawn_settler(cmds, context);
        }

        if ui.button("Highlight Spawn Point") {
            context.tile_map_mut().set_tile_flags(
                spawn_point.cell,
                TileKind::Terrain,
                TileFlags::Highlighted | TileFlags::DrawDebugBounds,
                true,
            );
        }
    }
}

// ----------------------------------------------
// AmbientEffectsSystem Debug UI
// ----------------------------------------------

impl AmbientEffectsSystem {
    pub(crate) fn draw_debug_ui_dispatch(&mut self, engine: &mut Engine, cmds: &mut SimCmds, context: &SimContext) {
        self.bird_spawn_timer.draw_debug_ui_with_header("Bird Spawn", engine.ui_system());

        let ui = engine.ui_system().ui();

        if ui.button("Spawn Bird (left-to-right path") {
            spawn_bird(cmds, context, BirdFlightPath::LeftToRight);
        }

        if ui.button("Spawn Bird (right-to-left path)") {
            spawn_bird(cmds, context, BirdFlightPath::RightToLeft);
        }

        if ui.button("Spawn Big Flock") {
            for _ in 0..50 {
                spawn_bird_with_random_flight_path(cmds, context);
            }
        }
    }
}

// ----------------------------------------------
// AmbientSoundsSystem Debug UI
// ----------------------------------------------

impl AmbientSoundsSystem {
    pub(crate) fn draw_debug_ui_dispatch(&mut self, engine: &mut Engine, _cmds: &mut SimCmds, _context: &SimContext) {
        let ui = engine.ui_system().ui();

        if !self.is_enabled() {
            ui.text_colored(Color::red().to_array(), "AmbientSoundsSystem DISABLED.");
            return;
        }

        if let Some(key) = self.current_sound_playing() {
            ui.text(format_small!("Current Ambient Sound Playing: {} ('{}')", key, key.sound_path()));
        } else {
            ui.text("Current Ambient Sound Playing: None");
        }

        if ui.button("Reset Ambient Sounds") {
            self.reset(engine);
        }
    }
}

// ----------------------------------------------
// AmbientMusicSystem Debug UI
// ----------------------------------------------

impl AmbientMusicSystem {
    pub(crate) fn draw_debug_ui_dispatch(&mut self, engine: &mut Engine, _cmds: &mut SimCmds, _context: &SimContext) {
        let ui = engine.ui_system().ui();

        if !self.is_enabled() {
            ui.text_colored(Color::red().to_array(), "AmbientMusicSystem DISABLED.");
            return;
        }

        if let Some(key) = self.current_track_playing() {
            ui.text(format_small!("Current Track Playing: {} ('{}')", key, key.track_path()));
        } else {
            ui.text("Current Track Playing: None");
        }

        ui.text(format_small!("Current Game State: {}", self.current_game_state()));
        ui.separator();

        if ui.button("Reset Track") {
            self.reset(engine);
        }
    }
}
