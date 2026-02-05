use arrayvec::ArrayVec;
use enum_dispatch::enum_dispatch;
use strum::{EnumCount, IntoEnumIterator};
use strum_macros::{Display, EnumCount, EnumIter, EnumDiscriminants};

use crate::{
    singleton_late_init,
    utils::Vec2,
    ui::{UiFontScale, widgets::*},
    game::menu::LARGE_HORIZONTAL_SEPARATOR_SPRITE,
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
#[strum_discriminants(repr(u32), name(DialogMenuKind), derive(Display, EnumCount, EnumIter))]
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

pub fn open(dialog_menu_kind: DialogMenuKind, close_all_others: bool, context: &mut UiWidgetContext) -> bool {
    if close_all_others {
        close_all(context);
    }

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
    fn menu(&self) -> &UiMenuRcMut;
    fn menu_mut(&mut self) -> &mut UiMenuRcMut;

    fn is_open(&self) -> bool {
        self.menu().is_open()
    }

    fn open(&mut self, context: &mut UiWidgetContext) {
        self.menu_mut().open(context);
    }

    fn close(&mut self, context: &mut UiWidgetContext) {
        self.menu_mut().close(context);
    }

    fn draw(&mut self, context: &mut UiWidgetContext) {
        self.menu_mut().draw(context);
    }
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

// ----------------------------------------------
// Internal Shared Constants & Helper Functions
// ----------------------------------------------

const DEFAULT_DIALOG_MENU_HEADING_MARGINS: (f32, f32) = (100.0, 10.0); // (top, bottom)
const DEFAULT_DIALOG_MENU_HEADING_FONT_SCALE: UiFontScale = UiFontScale(1.8);
const DEFAULT_DIALOG_MENU_BACKGROUND_SPRITE: &str = "misc/scroll_bg.png";

fn default_dialog_menu_size(context: &UiWidgetContext) -> Vec2 {
    Vec2::new(550.0, context.viewport_size.height as f32 - 150.0)
}

fn make_default_dialog_menu_layout(context: &mut UiWidgetContext,
                                   dialog_menu_kind: DialogMenuKind,
                                   heading_title: &str,
                                   widget_spacing: f32,
                                   widgets: impl IntoIterator<Item = UiWidgetImpl>)
                                   -> UiMenuRcMut
{
    let mut group = UiWidgetGroup::new(
        context,
        UiWidgetGroupParams {
            widget_spacing,
            center_vertically: false,
            center_horizontally: true,
            ..Default::default()
        }
    );

    for widget in widgets {
        group.add_widget(widget);
    }

    let heading = UiMenuHeading::new(
        context,
        UiMenuHeadingParams {
            font_scale: DEFAULT_DIALOG_MENU_HEADING_FONT_SCALE,
            lines: vec![heading_title.into()],
            separator: Some(LARGE_HORIZONTAL_SEPARATOR_SPRITE),
            margin_top: DEFAULT_DIALOG_MENU_HEADING_MARGINS.0,
            margin_bottom: DEFAULT_DIALOG_MENU_HEADING_MARGINS.1,
            ..Default::default()
        }
    );

    let mut menu = UiMenu::new(
        context,
        UiMenuParams {
            label: Some(dialog_menu_kind.to_string()),
            flags: UiMenuFlags::PauseSimIfOpen | UiMenuFlags::AlignCenter,
            size: Some(default_dialog_menu_size(context)),
            background: Some(DEFAULT_DIALOG_MENU_BACKGROUND_SPRITE),
            ..Default::default()
        }
    );

    menu.add_widget(heading);
    menu.add_widget(group);

    menu
}
