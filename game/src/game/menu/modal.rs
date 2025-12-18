use arrayvec::ArrayVec;
use num_enum::TryFromPrimitive;
use strum::{EnumCount, EnumProperty, IntoEnumIterator};
use strum_macros::{EnumCount, EnumProperty, EnumIter};

use super::{
    widgets::{
        self,
        ModalMenu, BasicModalMenu
    },
    bar::{LeftBar, LeftBarModalMenu}
};
use crate::{
    imgui_ui::UiSystem,
    utils::{Size, mem::RawPtr},
    game::{sim::Simulation, GameLoop},
};

// ----------------------------------------------
// Constants
// ----------------------------------------------

const MODAL_BUTTON_DEFAULT_SIZE: Size = Size::new(150, 30);
const MODAL_WINDOW_DEFAULT_SIZE: Size = Size::new(400, 400);

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
    parent: RawPtr<LeftBar>,
}

impl MainModalMenu {
    pub fn new(title: String, parent: &LeftBar) -> Self {
        Self {
            menu: BasicModalMenu::new(title, Some(MODAL_WINDOW_DEFAULT_SIZE)),
            parent: RawPtr::from_ref(parent),
        }
    }

    fn handle_button_click(sim: &mut Simulation, parent: &mut LeftBar, button: MainModalMenuButton) {
        match button {
            MainModalMenuButton::NewGame  => Self::on_new_game_button(),
            MainModalMenuButton::LoadGame => Self::on_load_game_button(sim, parent),
            MainModalMenuButton::SaveGame => Self::on_save_game_button(sim, parent),
            MainModalMenuButton::Settings => Self::on_settings_button(sim, parent),
            MainModalMenuButton::Quit     => Self::on_quit_button(),
        }
    }

    fn on_new_game_button() {
        // TODO: Maybe a hidden modal menu of parent?
    }

    fn on_load_game_button(sim: &mut Simulation, parent: &mut LeftBar) {
        parent.open_modal_menu(sim, LeftBarModalMenu::SaveGame);
    }

    fn on_save_game_button(sim: &mut Simulation, parent: &mut LeftBar) {
        parent.open_modal_menu(sim, LeftBarModalMenu::SaveGame);
    }

    fn on_settings_button(sim: &mut Simulation, parent: &mut LeftBar) {
        parent.open_modal_menu(sim, LeftBarModalMenu::Settings);
    }

    fn on_quit_button() {
        GameLoop::get_mut().request_quit();
    }
}

impl ModalMenu for MainModalMenu {
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
    pub fn new(title: String) -> Self {
        Self {
            menu: BasicModalMenu::new(title, Some(MODAL_WINDOW_DEFAULT_SIZE)),
        }
    }
}

impl ModalMenu for SaveGameModalMenu {
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
    pub fn new(title: String) -> Self {
        Self {
            menu: BasicModalMenu::new(title, Some(MODAL_WINDOW_DEFAULT_SIZE)),
        }
    }
}

impl ModalMenu for SettingsModalMenu {
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
