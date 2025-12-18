use super::{
    widgets::{
        ModalMenu, BasicModalMenu
    },
};
use crate::{
    utils::Size,
    imgui_ui::UiSystem,
    game::sim::Simulation,
};

// ----------------------------------------------
// MainModalMenu
// ----------------------------------------------

pub struct MainModalMenu {
    menu: BasicModalMenu,
}

impl MainModalMenu {
    pub fn new() -> Self {
        Self {
            menu: BasicModalMenu::new("Main Menu".into(), Some(Size::new(500, 500))),
        }
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
        self.menu.draw(sim, ui_sys, || {});
    }
}

// ----------------------------------------------
// SaveGameModalMenu
// ----------------------------------------------

pub struct SaveGameModalMenu {
    menu: BasicModalMenu,
}

impl SaveGameModalMenu {
    pub fn new() -> Self {
        Self {
            menu: BasicModalMenu::new("Save Game".into(), Some(Size::new(500, 500))),
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
        self.menu.draw(sim, ui_sys, || {});
    }
}

// ----------------------------------------------
// SettingsModalMenu
// ----------------------------------------------

pub struct SettingsModalMenu {
    menu: BasicModalMenu,
}

impl SettingsModalMenu {
    pub fn new() -> Self {
        Self {
            menu: BasicModalMenu::new("Settings".into(), Some(Size::new(500, 500))),
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
        self.menu.draw(sim, ui_sys, || {});
    }
}
