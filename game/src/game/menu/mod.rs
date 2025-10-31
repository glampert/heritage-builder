use crate::{
    engine::time::Seconds,
    save::{Save, Load},
    imgui_ui::{UiSystem, UiInputEvent},
    utils::{Vec2, coords::{CellRange, WorldToScreenTransform}},
    app::input::{InputAction, InputKey, InputModifiers, MouseButton},
    tile::{TileMap, selection::TileSelection, rendering::TileMapRenderFlags, camera::Camera},
    game::{world::World, sim::Simulation, system::GameSystems},
};

pub mod hud;

// ----------------------------------------------
// Helper structs
// ----------------------------------------------

pub enum GameMenusInputCmd {
    Key {
        key: InputKey,
        action: InputAction,
        modifiers: InputModifiers,
    },
    Mouse {
        button: MouseButton,
        action: InputAction,
        modifiers: InputModifiers,
    },
    Scroll {
        amount: Vec2,
    },
}

pub struct GameMenusInputArgs<'game> {
    pub cmd: GameMenusInputCmd,

    // Tile Map:
    pub tile_map: &'game mut TileMap,
    pub tile_selection: &'game mut TileSelection,

    // Sim/World:
    pub sim: &'game mut Simulation,
    pub world: &'game mut World,

    // Camera/Input:
    pub transform: WorldToScreenTransform,
    pub cursor_screen_pos: Vec2,
}

pub struct GameMenusFrameArgs<'game> {
    // UI System:
    pub ui_sys: &'game UiSystem,

    // Tile Map:
    pub tile_map: &'game mut TileMap,
    pub tile_selection: &'game mut TileSelection,

    // Sim/World/Game:
    pub sim: &'game mut Simulation,
    pub world: &'game mut World,
    pub systems: &'game mut GameSystems,

    // Camera/Input:
    pub camera: &'game mut Camera,
    pub visible_range: CellRange,
    pub cursor_screen_pos: Vec2,
    pub delta_time_secs: Seconds,
}

// ----------------------------------------------
// GameMenusSystem
// ----------------------------------------------

pub trait GameMenusSystem: Save + Load {
    fn handle_input(&mut self, args: &mut GameMenusInputArgs) -> UiInputEvent;
    fn begin_frame(&mut self, args: &mut GameMenusFrameArgs) -> TileMapRenderFlags;
    fn end_frame(&mut self, args: &mut GameMenusFrameArgs);
}
