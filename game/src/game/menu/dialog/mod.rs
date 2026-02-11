use std::any::Any;

use arrayvec::ArrayVec;
use enum_dispatch::enum_dispatch;
use strum::{EnumCount, IntoEnumIterator};
use strum_macros::{Display, EnumCount, EnumIter, EnumDiscriminants};

use super::{LARGE_HORIZONTAL_SEPARATOR_SPRITE};
use crate::{
    singleton_late_init,
    game::menu::ButtonDef,
    utils::{Vec2, mem},
    ui::{UiFontScale, UiStaticVar, widgets::*},
};

mod home;
use home::*;

mod main_game;
use main_game::*;

mod new_game;
use new_game::*;

mod about;
use about::*;

mod load_save;
use load_save::*;

mod settings;
use settings::*;

// ----------------------------------------------
// Macro: dialog_menu_factories
// ----------------------------------------------

macro_rules! dialog_menu_factories {
    ($($t:ty),* $(,)?) => {
        [$(<$t as DialogMenuFactory>::create),*]
    };
}

type DialogMenuFactoryFn = fn(&mut UiWidgetContext) -> DialogMenuImpl;

// ----------------------------------------------
// DialogMenuKind / DialogMenuImpl
// ----------------------------------------------

const DIALOG_MENU_COUNT: usize = DialogMenuKind::COUNT;

#[enum_dispatch]
#[derive(EnumDiscriminants)]
#[strum_discriminants(name(DialogMenuKind), derive(Display, EnumCount, EnumIter))]
pub enum DialogMenuImpl {
    Home,
    MainGame,
    NewGame,
    About,

    // Save Game menus:
    LoadGame,
    SaveGame,
    LoadOrSaveGame,

    // Settings menus:
    MainSettings,
    GameSettings,
    SoundSettings,
    GraphicsSettings,
}

const DIALOG_MENU_FACTORIES: [DialogMenuFactoryFn; DIALOG_MENU_COUNT] = dialog_menu_factories![
    Home,
    MainGame,
    NewGame,
    About,

    LoadGame,
    SaveGame,
    LoadOrSaveGame,

    MainSettings,
    GameSettings,
    SoundSettings,
    GraphicsSettings,
];

impl DialogMenuKind {
    fn build_menu(self, context: &mut UiWidgetContext) -> DialogMenuImpl {
        let dialog = DIALOG_MENU_FACTORIES[self as usize](context);
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

pub fn reset() {
    DialogMenusSingleton::get_mut().reset();
}

pub fn open(dialog_menu_kind: DialogMenuKind, close_all_others: bool, context: &mut UiWidgetContext) -> bool {
    if close_all_others {
        close_all(context);
    }

    DialogMenusSingleton::get_mut().open_dialog(dialog_menu_kind, context)
}

pub fn close(dialog_menu_kind: DialogMenuKind, context: &mut UiWidgetContext) -> bool {
    DialogMenusSingleton::get_mut().close_dialog(dialog_menu_kind, context)
}

pub fn close_all(context: &mut UiWidgetContext) -> bool {
    DialogMenusSingleton::get_mut().close_all(context)
}

pub fn current() -> Option<DialogMenuKind> {
    DialogMenusSingleton::get_mut().current_dialog().map(|dialog| dialog.kind())
}

pub fn close_current(context: &mut UiWidgetContext) -> bool {
    DialogMenusSingleton::get_mut().close_current(context)
}

pub fn draw_current(context: &mut UiWidgetContext) {
    DialogMenusSingleton::get_mut().draw_current(context);
}

pub fn set_global_menu_flags(flags: UiMenuFlags) {
    DialogMenusSingleton::get_mut().set_global_menu_flags(flags);
}

// ----------------------------------------------
// DialogMenu
// ----------------------------------------------

#[enum_dispatch(DialogMenuImpl)]
trait DialogMenu: Any {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any {
        mem::mut_ref_cast(self.as_any())
    }

    fn kind(&self) -> DialogMenuKind;
    fn menu(&self) -> &UiMenuRcMut;
    fn menu_mut(&mut self) -> &mut UiMenuRcMut {
        mem::mut_ref_cast(self.menu())
    }

    fn reset(&mut self) {
        let menu = self.menu_mut();
        menu.reset_message_box();
        menu.set_flags(UiMenuFlags::IsOpen, false);
    }

    fn is_open(&self) -> bool {
        self.menu().is_open()
    }

    fn open(&mut self, context: &mut UiWidgetContext) -> bool {
        self.menu_mut().open(context);
        true
    }

    fn close(&mut self, context: &mut UiWidgetContext) -> bool {
        let menu = self.menu_mut();
        if menu.is_message_box_open() {
            menu.close_message_box(context);
            false
        } else {
            menu.close(context);
            true
        }
    }

    fn draw(&mut self, context: &mut UiWidgetContext) {
        self.menu_mut().draw(context);
    }
}

// ----------------------------------------------
// DialogMenuFactory
// ----------------------------------------------

trait DialogMenuFactory: DialogMenu {
    const KIND: DialogMenuKind;
    const TITLE: &'static [&'static str];

    fn create(context: &mut UiWidgetContext) -> DialogMenuImpl;
}

// ----------------------------------------------
// Macro: implement_dialog_menu
// ----------------------------------------------

#[macro_export]
macro_rules! implement_dialog_menu {
    ($dialog_menu_struct:ident, $title:expr) => {
        impl DialogMenuFactory for $dialog_menu_struct {
            const KIND: DialogMenuKind = DialogMenuKind::$dialog_menu_struct;
            const TITLE: &'static [&'static str] = &$title;

            fn create(context: &mut UiWidgetContext) -> DialogMenuImpl {
                DialogMenuImpl::from($dialog_menu_struct::new(context))
            }
        }

        impl DialogMenu for $dialog_menu_struct {
            fn as_any(&self) -> &dyn Any {
                self
            }
            fn kind(&self) -> DialogMenuKind {
                Self::KIND
            }
            fn menu(&self) -> &UiMenuRcMut {
                &self.menu
            }
        }
    };
}

// ----------------------------------------------
// DialogMenusSingleton
// ----------------------------------------------

const DIALOG_MENU_STACK_MAX_DEPTH: usize = 8;

struct DialogMenusSingleton {
    dialog_menus: ArrayVec<DialogMenuImpl, DIALOG_MENU_COUNT>,
    menu_stack: ArrayVec<DialogMenuKind, DIALOG_MENU_STACK_MAX_DEPTH>,
}

impl DialogMenusSingleton {
    fn new(context: &mut UiWidgetContext) -> Self {
        let mut dialog_menus = ArrayVec::new();

        for dialog_menu_kind in DialogMenuKind::iter() {
            dialog_menus.push(dialog_menu_kind.build_menu(context));
        }

        Self { dialog_menus, menu_stack: ArrayVec::new() }
    }

    fn reset(&mut self) {
        self.menu_stack.clear();
        for dialog in &mut self.dialog_menus {
            dialog.reset();
        }
    }

    fn set_global_menu_flags(&mut self, flags: UiMenuFlags) {
        GLOBAL_DIALOG_MENU_FLAGS.set(flags);

        for dialog in &mut self.dialog_menus {
            dialog.menu_mut().reset_flags(flags);
        }
    }

    fn current_dialog(&mut self) -> Option<&mut DialogMenuImpl> {
        if let Some(&stack_top) = self.menu_stack.last() {
            return Some(self.find_dialog(stack_top));
        }
        None
    }

    fn current_dialog_as<Dialog: DialogMenu>(&mut self) -> Option<&mut Dialog> {
        if let Some(dialog) = self.current_dialog() {
            return dialog.as_any_mut().downcast_mut::<Dialog>();
        }
        None
    }

    fn find_dialog(&mut self, dialog_menu_kind: DialogMenuKind) -> &mut DialogMenuImpl {
        let dialog = &mut self.dialog_menus[dialog_menu_kind as usize];
        debug_assert!(dialog.kind() == dialog_menu_kind);
        dialog
    }

    fn find_dialog_as<Dialog: DialogMenuFactory>(&mut self) -> &mut Dialog {
        let dialog = self.find_dialog(Dialog::KIND);
        dialog.as_any_mut().downcast_mut::<Dialog>()
            .unwrap_or_else(|| panic!("Expected dialog menu kind to be {}!", Dialog::KIND))
    }

    fn open_dialog(&mut self, dialog_menu_kind: DialogMenuKind, context: &mut UiWidgetContext) -> bool {
        let dialog = self.find_dialog(dialog_menu_kind);
        if !dialog.is_open() && dialog.open(context) {
            debug_assert!(!self.menu_stack.contains(&dialog_menu_kind));
            self.menu_stack.push(dialog_menu_kind);
            return true;
        }
        false
    }

    fn close_dialog(&mut self, dialog_menu_kind: DialogMenuKind, context: &mut UiWidgetContext) -> bool {
        let dialog = self.find_dialog(dialog_menu_kind);
        if dialog.is_open() && dialog.close(context) {
            let index = self.menu_stack
                .iter()
                .position(|kind| *kind == dialog_menu_kind)
                .expect("Dialog menu not in menu stack!");
            let removed = self.menu_stack.remove(index);
            debug_assert!(removed == dialog_menu_kind, "Closed menu should have been in the menu stack!");
            return true;
        }
        false
    }

    fn close_all(&mut self, context: &mut UiWidgetContext) -> bool {
        let mut any_closed = false;
        // Close all open menus:
        for dialog_menu_kind in self.menu_stack.clone() {
            if self.close_dialog(dialog_menu_kind, context) {
                any_closed = true;
            }
        }
        any_closed
    }

    fn close_current(&mut self, context: &mut UiWidgetContext) -> bool {
        if let Some(&stack_top) = self.menu_stack.last() {
            let closed = self.close_dialog(stack_top, context);
            // If we didn't close the last dialog, keep the game in paused state.
            if closed && !self.menu_stack.is_empty() {
                context.sim.pause();
            }
            return closed;
        }
        false
    }

    fn draw_current(&mut self, context: &mut UiWidgetContext) {
        // Draw current open menu only:
        if let Some(dialog) = self.current_dialog() {
            debug_assert!(dialog.is_open());
            dialog.draw(context);
        }
    }
}

// Global instance:
singleton_late_init! { DIALOG_MENUS_SINGLETON, DialogMenusSingleton }

// ----------------------------------------------
// Internal Shared Constants & Helper Functions
// ----------------------------------------------

// For common dialog menus:
const DEFAULT_DIALOG_MENU_BUTTON_SPACING: f32 = 8.0; // Large stacked buttons
const DEFAULT_DIALOG_MENU_WIDGET_SPACING: f32 = 6.0; // Settings widgets / input fields
const DEFAULT_DIALOG_MENU_WIDGET_LABEL_SPACING: f32 = 5.0;
const DEFAULT_DIALOG_MENU_HEADING_MARGINS: (f32, f32) = (100.0, 10.0); // (top, bottom)
const DEFAULT_DIALOG_MENU_WIDGET_FONT_SCALE: UiFontScale = UiFontScale(1.0);
const DEFAULT_DIALOG_MENU_HEADING_FONT_SCALE: UiFontScale = UiFontScale(1.8);
const DEFAULT_DIALOG_MENU_BACKGROUND_SPRITE: &str = "misc/scroll_bg.png";

// For popup message boxes:
const DEFAULT_DIALOG_POPUP_FONT_SCALE: UiFontScale = UiFontScale(1.5);
const DEFAULT_DIALOG_POPUP_BACKGROUND_SPRITE: &str = "misc/square_page_bg.png";

static GLOBAL_DIALOG_MENU_FLAGS: UiStaticVar<UiMenuFlags> = UiStaticVar::new(UiMenuFlags::empty());

fn default_dialog_menu_size(context: &UiWidgetContext) -> Vec2 {
    Vec2::new(550.0, context.viewport_size.height as f32 - 150.0)
}

fn make_default_layout_dialog_menu(context: &mut UiWidgetContext,
                                   dialog_menu_kind: DialogMenuKind,
                                   heading_title: &[&str], // Each item in the slice is a heading line.
                                   widget_spacing: f32,
                                   widgets: Option<impl IntoIterator<Item = UiWidgetImpl>>)
                                   -> UiMenuRcMut
{
    let mut menu = UiMenu::new(
        context,
        UiMenuParams {
            label: Some(dialog_menu_kind.to_string()),
            flags: *GLOBAL_DIALOG_MENU_FLAGS,
            size: Some(default_dialog_menu_size(context)),
            background: Some(DEFAULT_DIALOG_MENU_BACKGROUND_SPRITE),
            ..Default::default()
        }
    );

    let heading = UiMenuHeading::new(
        context,
        UiMenuHeadingParams {
            font_scale: DEFAULT_DIALOG_MENU_HEADING_FONT_SCALE,
            lines: heading_title.iter().map(|line| line.to_string()).collect(),
            separator: Some(LARGE_HORIZONTAL_SEPARATOR_SPRITE),
            margin_top: DEFAULT_DIALOG_MENU_HEADING_MARGINS.0,
            margin_bottom: DEFAULT_DIALOG_MENU_HEADING_MARGINS.1,
            ..Default::default()
        }
    );

    menu.add_widget(heading);

    if let Some(widgets) = widgets {
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

        menu.add_widget(group);
    }

    menu
}

fn make_dialog_button_widgets<ButtonKind, const COUNT: usize>(context: &mut UiWidgetContext) -> ArrayVec::<UiWidgetImpl, COUNT>
    where ButtonKind: ButtonDef + EnumCount + IntoEnumIterator + 'static
{
    let mut buttons = ArrayVec::<UiWidgetImpl, COUNT>::new();

    for button_kind in ButtonKind::iter() {
        let on_pressed = UiTextButtonPressed::with_closure(
            move |_, context| { button_kind.on_pressed(context); }
        );

        buttons.push(UiWidgetImpl::from(
            button_kind.new_text_button(
                context,
                UiTextButtonSize::Large,
                on_pressed
            )
        ));
    }

    buttons
}
