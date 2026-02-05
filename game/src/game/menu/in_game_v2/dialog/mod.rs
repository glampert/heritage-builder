use std::rc::Rc;

use arrayvec::ArrayVec;
use strum::{EnumCount, IntoEnumIterator};
use strum_macros::{EnumCount, EnumIter};

use crate::{
    singleton_late_init,
    utils::mem::RcMut,
    ui::widgets::*,
};

mod main_menu;
mod new_game;
mod save_game;
mod settings;

// ----------------------------------------------
// DialogKind
// ----------------------------------------------

const DIALOG_MENU_COUNT: usize = DialogKind::COUNT;

#[derive(Copy, Clone, PartialEq, Eq, EnumCount, EnumIter)]
pub enum DialogKind {
    MainMenu,
    NewGame,
    SaveGame,
    Settings,
}

impl DialogKind {
    fn build_menu(self, context: &mut UiWidgetContext) -> DialogMenuRcMut {
        let rc: Rc<dyn DialogMenu> = {
            match self {
                Self::MainMenu => main_menu::MainMenuDialog::new(context),
                Self::NewGame  => new_game::NewGameDialog::new(context),
                Self::SaveGame => save_game::SaveGameDialog::new(context),
                Self::Settings => settings::SettingsDialog::new(context),
            }
        };
        RcMut::from(rc)
    }
}

// ----------------------------------------------
// Public API
// ----------------------------------------------

pub fn initialize(context: &mut UiWidgetContext) {
    if DialogMenusSingleton::is_initialized() {
        return; // Initialize only once.
    }

    DialogMenusSingleton::initialize(DialogMenusSingleton::new(context));
}

pub fn open(dialog_kind: DialogKind, context: &mut UiWidgetContext) -> bool {
    DialogMenusSingleton::get_mut().open(dialog_kind, context)
}

pub fn close(dialog_kind: DialogKind, context: &mut UiWidgetContext) -> bool {
    DialogMenusSingleton::get_mut().close(dialog_kind, context)
}

pub fn close_all(context: &mut UiWidgetContext) -> bool {
    DialogMenusSingleton::get_mut().close_all(context)
}

pub fn draw_all(context: &mut UiWidgetContext) {
    DialogMenusSingleton::get_mut().draw_all(context);
}

// ----------------------------------------------
// DialogMenu
// ----------------------------------------------

trait DialogMenu {
    fn kind(&self) -> DialogKind;
    fn is_open(&self) -> bool;
    fn open(&mut self, context: &mut UiWidgetContext);
    fn close(&mut self, context: &mut UiWidgetContext);
    fn draw(&mut self, context: &mut UiWidgetContext);
}

type DialogMenuRcMut = RcMut<dyn DialogMenu>;

// ----------------------------------------------
// DialogMenusSingleton
// ----------------------------------------------

struct DialogMenusSingleton {
    dialog_menus: ArrayVec<DialogMenuRcMut, DIALOG_MENU_COUNT>,
}

impl DialogMenusSingleton {
    fn new(context: &mut UiWidgetContext) -> Self {
        let mut dialog_menus = ArrayVec::new();

        for dialog_kind in DialogKind::iter() {
            dialog_menus.push(dialog_kind.build_menu(context));
        }

        Self { dialog_menus }
    }

    fn find_mut(&mut self, dialog_kind: DialogKind) -> Option<&mut DialogMenuRcMut> {
        for dialog in &mut self.dialog_menus {
            if dialog.kind() == dialog_kind {
                return Some(dialog);
            }
        }
        None
    }

    fn open(&mut self, dialog_kind: DialogKind, context: &mut UiWidgetContext) -> bool {
        if let Some(dialog) = self.find_mut(dialog_kind) {
            if !dialog.is_open() {
                dialog.open(context);
                return true;
            }
        }
        false
    }

    fn close(&mut self, dialog_kind: DialogKind, context: &mut UiWidgetContext) -> bool {
        if let Some(dialog) = self.find_mut(dialog_kind) {
            if dialog.is_open() {
                dialog.close(context);
                return true;
            }
        }
        false
    }

    fn close_all(&mut self, context: &mut UiWidgetContext) -> bool {
        let mut any_closed = false;
        // Close all open menus:
        for dialog in &mut self.dialog_menus {
            if dialog.is_open() {
                dialog.close(context);
                any_closed = true;
            }
        }
        any_closed
    }

    fn draw_all(&mut self, context: &mut UiWidgetContext) {
        // Draw all open menus:
        for dialog in &mut self.dialog_menus {
            if dialog.is_open() {
                dialog.draw(context);
            }
        }
    }
}

// Global instance:
singleton_late_init! { DIALOG_MENUS_SINGLETON, DialogMenusSingleton }
