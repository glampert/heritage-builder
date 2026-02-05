use arrayvec::ArrayVec;
use enum_dispatch::enum_dispatch;
use strum::{EnumCount, IntoEnumIterator};
use strum_macros::{EnumCount, EnumIter, EnumDiscriminants};

use crate::{
    singleton_late_init,
    ui::widgets::*,
};

mod main_menu;
use main_menu::MainMenu;

mod new_game;
use new_game::NewGame;

mod save_game;
use save_game::SaveGame;

mod settings;
use settings::Settings;

// ----------------------------------------------
// DialogMenuKind / DialogMenuImpl
// ----------------------------------------------

const DIALOG_MENU_COUNT: usize = DialogMenuKind::COUNT;

#[enum_dispatch]
#[derive(EnumDiscriminants)]
#[strum_discriminants(repr(u32), name(DialogMenuKind), derive(EnumCount, EnumIter))]
pub enum DialogMenuImpl {
    MainMenu,
    NewGame,
    SaveGame,
    Settings,
}

impl DialogMenuKind {
    fn build_menu(self, context: &mut UiWidgetContext) -> DialogMenuImpl {
        let dialog = match self {
            Self::MainMenu => DialogMenuImpl::from(MainMenu::new(context)),
            Self::NewGame  => DialogMenuImpl::from(NewGame::new(context)),
            Self::SaveGame => DialogMenuImpl::from(SaveGame::new(context)),
            Self::Settings => DialogMenuImpl::from(Settings::new(context)),
        };
        debug_assert!(dialog.kind() == self, "Wrong DialogMenuKind! Check DialogMenu::kind() impl!");
        dialog
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

pub fn open(dialog_menu_kind: DialogMenuKind, context: &mut UiWidgetContext) -> bool {
    DialogMenusSingleton::get_mut().open(dialog_menu_kind, context)
}

pub fn close(dialog_menu_kind: DialogMenuKind, context: &mut UiWidgetContext) -> bool {
    DialogMenusSingleton::get_mut().close(dialog_menu_kind, context)
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

#[enum_dispatch(DialogMenuImpl)]
trait DialogMenu {
    fn kind(&self) -> DialogMenuKind;
    fn is_open(&self) -> bool;
    fn open(&mut self, context: &mut UiWidgetContext);
    fn close(&mut self, context: &mut UiWidgetContext);
    fn draw(&mut self, context: &mut UiWidgetContext);
}

// ----------------------------------------------
// DialogMenusSingleton
// ----------------------------------------------

struct DialogMenusSingleton {
    dialog_menus: ArrayVec<DialogMenuImpl, DIALOG_MENU_COUNT>,
}

impl DialogMenusSingleton {
    fn new(context: &mut UiWidgetContext) -> Self {
        let mut dialog_menus = ArrayVec::new();

        for dialog_menu_kind in DialogMenuKind::iter() {
            dialog_menus.push(dialog_menu_kind.build_menu(context));
        }

        Self { dialog_menus }
    }

    fn find_mut(&mut self, dialog_menu_kind: DialogMenuKind) -> Option<&mut DialogMenuImpl> {
        self.dialog_menus
            .iter_mut()
            .find(|dialog| dialog.kind() == dialog_menu_kind)
    }

    fn open(&mut self, dialog_menu_kind: DialogMenuKind, context: &mut UiWidgetContext) -> bool {
        if let Some(dialog) = self.find_mut(dialog_menu_kind) {
            if !dialog.is_open() {
                dialog.open(context);
                return true;
            }
        }
        false
    }

    fn close(&mut self, dialog_menu_kind: DialogMenuKind, context: &mut UiWidgetContext) -> bool {
        if let Some(dialog) = self.find_mut(dialog_menu_kind) {
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
