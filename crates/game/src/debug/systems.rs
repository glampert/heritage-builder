use arrayvec::ArrayString;
use smallvec::SmallVec;
use strum::{IntoEnumIterator, VariantNames};
use common::{Color, format_small};
use engine::{
    Engine,
    ui::{self, DrawDebugUi, UiStaticVar, UiSystem},
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
        // A tab per system, then "Create Systems".
        let mut tabs: SmallVec<[ArrayString<128>; 8]> =
            self.systems.iter().map(|entry| format_small!("{}", entry.system)).collect();
        tabs.push(format_small!("Create Systems"));

        let Some(tab) = ui::tab_bar(ui_sys, "Game Systems Tab Bar", &tabs) else {
            return;
        };

        if let Some(entry) = self.systems.get_mut(tab.index) {
            entry.system.draw_debug_ui(engine, cmds, context);
        } else {
            ui::text(ui_sys, "Create and register system if not already created.");

            static SYSTEM_INDEX: UiStaticVar<usize> = UiStaticVar::new(0);
            ui::combo(ui_sys, "Systems", SYSTEM_INDEX.as_mut(), GameSystemImpl::VARIANTS);

            if ui::button(ui_sys, "Create")
                && let Some(system) = GameSystemImpl::iter().nth(*SYSTEM_INDEX)
                && !self.has(system.as_any().type_id())
            {
                self.register(system);
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

        let ui_sys = engine.ui_system();

        let color_text = |text: &str, cond: bool| {
            ui::horizontal(ui_sys, |ui_sys| {
                ui::text(ui_sys, text);
                if cond {
                    ui::text_colored(ui_sys, Color::green(), "yes");
                } else {
                    ui::text_colored(ui_sys, Color::red(), "no");
                }
            });
        };

        color_text("Has vacant lots:", Self::has_vacant_lots(context));

        let spawn_point = Self::find_spawn_point(cmds, context);
        ui::text(ui_sys, format_small!("Spawn Point: {}", spawn_point.cell));

        if ui::input_scalar(ui_sys, "Population Per Settler Unit", &mut self.population_per_settler_unit, 1) {
            self.population_per_settler_unit = self.population_per_settler_unit.max(1);
        }

        if ui::button(ui_sys, "Force Spawn Now") {
            self.spawn_settler(cmds, context);
        }

        if ui::button(ui_sys, "Highlight Spawn Point") {
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

        let ui_sys = engine.ui_system();

        if ui::button(ui_sys, "Spawn Bird (left-to-right path") {
            spawn_bird(cmds, context, BirdFlightPath::LeftToRight);
        }

        if ui::button(ui_sys, "Spawn Bird (right-to-left path)") {
            spawn_bird(cmds, context, BirdFlightPath::RightToLeft);
        }

        if ui::button(ui_sys, "Spawn Big Flock") {
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
        let ui_sys = engine.ui_system();

        if !self.is_enabled() {
            ui::text_colored(ui_sys, Color::red(), "AmbientSoundsSystem DISABLED.");
            return;
        }

        if let Some(key) = self.current_sound_playing() {
            ui::text(ui_sys, format_small!("Current Ambient Sound Playing: {} ('{}')", key, key.sound_path()));
        } else {
            ui::text(ui_sys, "Current Ambient Sound Playing: None");
        }

        if ui::button(engine.ui_system(), "Reset Ambient Sounds") {
            self.reset(engine);
        }
    }
}

// ----------------------------------------------
// AmbientMusicSystem Debug UI
// ----------------------------------------------

impl AmbientMusicSystem {
    pub(crate) fn draw_debug_ui_dispatch(&mut self, engine: &mut Engine, _cmds: &mut SimCmds, _context: &SimContext) {
        let ui_sys = engine.ui_system();

        if !self.is_enabled() {
            ui::text_colored(ui_sys, Color::red(), "AmbientMusicSystem DISABLED.");
            return;
        }

        if let Some(key) = self.current_track_playing() {
            ui::text(ui_sys, format_small!("Current Track Playing: {} ('{}')", key, key.track_path()));
        } else {
            ui::text(ui_sys, "Current Track Playing: None");
        }

        ui::text(ui_sys, format_small!("Current Game State: {}", self.current_game_state()));
        ui::separator(ui_sys);

        if ui::button(engine.ui_system(), "Reset Track") {
            self.reset(engine);
        }
    }
}
