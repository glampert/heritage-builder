use arrayvec::ArrayVec;
use std::any::{Any, TypeId};
use num_enum::TryFromPrimitive;
use strum::{EnumCount, EnumProperty, IntoEnumIterator};
use strum_macros::{EnumCount, EnumProperty, EnumIter};

use super::{
    widgets,
    bar::MenuBar,
};
use crate::{
    imgui_ui::UiSystem,
    utils::{Size, Vec2, mem},
    game::{sim::Simulation, GameLoop},
};

// ----------------------------------------------
// Constants
// ----------------------------------------------

const MODAL_BUTTON_DEFAULT_SIZE: Size = Size::new(150, 30);
const MODAL_WINDOW_DEFAULT_SIZE: Size = Size::new(400, 400);

// ----------------------------------------------
// ModalMenu / BasicModalMenu
// ----------------------------------------------

pub type ModalMenuId = TypeId;

// A modal popup window / dialog menu that pauses the game while open.
pub trait ModalMenu: Any {
    fn as_any(&self) -> &dyn Any;
    fn id(&self) -> ModalMenuId {
        self.as_any().type_id()
    }

    fn is_open(&self) -> bool;
    fn open(&mut self, sim: &mut Simulation);
    fn close(&mut self, sim: &mut Simulation);
    fn draw(&mut self, sim: &mut Simulation, ui_sys: &UiSystem);
}

struct BasicModalMenu {
    title: String,
    size: Option<Size>,
    is_open: bool,
}

impl BasicModalMenu {
    fn new(title: String, size: Option<Size>) -> Self {
        Self {
            title,
            size,
            is_open: false,
        }
    }

    fn is_open(&self) -> bool {
        self.is_open
    }

    fn open(&mut self, sim: &mut Simulation) {
        self.is_open = true;
        sim.pause();
    }

    fn close(&mut self, sim: &mut Simulation) {
        self.is_open = false;
        sim.resume();
    }

    fn draw<F>(&mut self, sim: &mut Simulation, ui_sys: &UiSystem, f: F)
        where F: FnOnce(&mut Simulation)
    {
        if !self.is_open {
            return;
        }

        let ui = ui_sys.ui();
        let display_size = ui.io().display_size;

        // Center popup window to the middle of the display:
        widgets::set_next_window_pos(
            Vec2::new(display_size[0] * 0.5, display_size[1] * 0.5),
            Vec2::new(0.5, 0.5),
            imgui::Condition::Always
        );

        let window_size = self.size.unwrap_or_default().to_vec2();
        let size_cond = if self.size.is_some() { imgui::Condition::Always } else { imgui::Condition::Never };

        let mut window_flags = widgets::window_flags();
        window_flags.remove(imgui::WindowFlags::NO_TITLE_BAR);

        let mut is_open = self.is_open;

        ui.window(&self.title)
            .opened(&mut is_open)
            .size(window_size.to_array(), size_cond)
            .flags(window_flags)
            .build(|| {
                f(sim);
                widgets::draw_current_window_debug_rect(ui);
            });

        // Resume game if closed by user.
        if !is_open {
            self.is_open = false;
            sim.resume();
        }
    }
}

// ----------------------------------------------
// MainModalMenu
// ----------------------------------------------

const MAIN_MODAL_MENU_BUTTON_COUNT: usize = MainModalMenuButton::COUNT;

#[repr(usize)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, TryFromPrimitive, EnumCount, EnumProperty, EnumIter)]
enum MainModalMenuButton {
    #[strum(props(Label = "New Game"))]
    NewGame,

    #[strum(props(Label = "Load Game"))]
    LoadGame,

    #[strum(props(Label = "Save Game"))]
    SaveGame,

    #[strum(props(Label = "Settings"))]
    Settings,

    #[strum(props(Label = "Quit"))]
    Quit,
}

impl MainModalMenuButton {
    fn label(self) -> &'static str {
        self.get_str("Label").unwrap()
    }
}

pub struct MainModalMenu {
    menu: BasicModalMenu,
    parent: mem::RawPtr<dyn MenuBar>,
}

impl MainModalMenu {
    pub fn new(title: String, parent: &dyn MenuBar) -> Self {
        Self {
            menu: BasicModalMenu::new(title, Some(MODAL_WINDOW_DEFAULT_SIZE)),
            parent: mem::RawPtr::from_ref(parent),
        }
    }

    fn handle_button_click(sim: &mut Simulation, parent: &mut dyn MenuBar, button: MainModalMenuButton) {
        match button {
            MainModalMenuButton::NewGame  => Self::on_new_game_button(),
            MainModalMenuButton::LoadGame => Self::on_load_game_button(sim, parent),
            MainModalMenuButton::SaveGame => Self::on_save_game_button(sim, parent),
            MainModalMenuButton::Settings => Self::on_settings_button(sim, parent),
            MainModalMenuButton::Quit     => Self::on_quit_button(),
        }
    }

    fn on_new_game_button() {
        // TODO: Maybe a hidden modal menu of parent bar?
    }

    fn on_load_game_button(sim: &mut Simulation, parent: &mut dyn MenuBar) {
        parent.open_modal_menu(sim, ModalMenuId::of::<SaveGameModalMenu>());
    }

    fn on_save_game_button(sim: &mut Simulation, parent: &mut dyn MenuBar) {
        parent.open_modal_menu(sim, ModalMenuId::of::<SaveGameModalMenu>());
    }

    fn on_settings_button(sim: &mut Simulation, parent: &mut dyn MenuBar) {
        parent.open_modal_menu(sim, ModalMenuId::of::<SettingsModalMenu>());
    }

    fn on_quit_button() {
        GameLoop::get_mut().request_quit();
    }
}

impl ModalMenu for MainModalMenu {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn is_open(&self) -> bool {
        self.menu.is_open()
    }

    fn open(&mut self, sim: &mut Simulation) {
        self.menu.open(sim);
    }

    fn close(&mut self, sim: &mut Simulation) {
        self.menu.close(sim);
    }

    fn draw(&mut self, sim: &mut Simulation, ui_sys: &UiSystem) {
        self.menu.draw(sim, ui_sys, |sim| {
            let ui = ui_sys.ui();
            let _font = ui.push_font(ui_sys.fonts().game_hud_large);

            let mut labels = ArrayVec::<&str, MAIN_MODAL_MENU_BUTTON_COUNT>::new();
            for button in MainModalMenuButton::iter() {
                labels.push(button.label());
            }

            let pressed_button_index = widgets::draw_centered_button_group(
                ui,
                &ui.get_window_draw_list(),
                &labels,
                Some(MODAL_BUTTON_DEFAULT_SIZE)
            );

            if let Some(pressed_index) = pressed_button_index {
                let button = MainModalMenuButton::try_from_primitive(pressed_index).unwrap();
                Self::handle_button_click(sim, self.parent.as_mut(), button);
            }
        });
    }
}

// ----------------------------------------------
// SaveGameModalMenu
// ----------------------------------------------

pub struct SaveGameModalMenu {
    menu: BasicModalMenu,
}

impl SaveGameModalMenu {
    pub fn new(title: String, _parent: &dyn MenuBar) -> Self {
        Self {
            menu: BasicModalMenu::new(title, Some(MODAL_WINDOW_DEFAULT_SIZE)),
        }
    }
}

impl ModalMenu for SaveGameModalMenu {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn is_open(&self) -> bool {
        self.menu.is_open()
    }

    fn open(&mut self, sim: &mut Simulation) {
        self.menu.open(sim);
    }

    fn close(&mut self, sim: &mut Simulation) {
        self.menu.close(sim);
    }

    fn draw(&mut self, sim: &mut Simulation, ui_sys: &UiSystem) {
        self.menu.draw(sim, ui_sys, |_sim| {});
    }
}

// ----------------------------------------------
// SettingsModalMenu
// ----------------------------------------------

pub struct SettingsModalMenu {
    menu: BasicModalMenu,
}

impl SettingsModalMenu {
    pub fn new(title: String, _parent: &dyn MenuBar) -> Self {
        Self {
            menu: BasicModalMenu::new(title, Some(MODAL_WINDOW_DEFAULT_SIZE)),
        }
    }
}

impl ModalMenu for SettingsModalMenu {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn is_open(&self) -> bool {
        self.menu.is_open()
    }

    fn open(&mut self, sim: &mut Simulation) {
        self.menu.open(sim);
    }

    fn close(&mut self, sim: &mut Simulation) {
        self.menu.close(sim);
    }

    fn draw(&mut self, sim: &mut Simulation, ui_sys: &UiSystem) {
        self.menu.draw(sim, ui_sys, |_sim| {});
    }
}
