use std::any::Any;

use arrayvec::ArrayVec;
use common::{Color, Vec2, mem};
use engine::{
    file_sys::paths::PathRef,
    ui::{
        self,
        UiFontScale,
        UiStaticVar,
        sound::{self, UiButtonSoundsEnabled, UiSoundKey},
        widgets::*,
    },
};
use enum_dispatch::enum_dispatch;
use strum::{Display, EnumCount, EnumDiscriminants, EnumIter, IntoEnumIterator};

use super::LARGE_HORIZONTAL_SEPARATOR_SPRITE;
use crate::{menu::ButtonDef, ui_context::GameUiContext};

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

mod city_management;
use city_management::*;

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

type DialogMenuFactoryFn = fn(&mut GameUiContext) -> DialogMenuImpl;

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

    // City management menus:
    CityManagement,
    PopulationManagement,
    ResourcesManagement,
    FinancesManagement,

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

    CityManagement,
    PopulationManagement,
    ResourcesManagement,
    FinancesManagement,

    MainSettings,
    GameSettings,
    SoundSettings,
    GraphicsSettings,
];

impl DialogMenuKind {
    fn build_menu(self, context: &mut GameUiContext) -> DialogMenuImpl {
        let dialog = DIALOG_MENU_FACTORIES[self as usize](context);
        debug_assert!(dialog.kind() == self, "Wrong DialogMenuKind! Check DialogMenu::kind() impl!");
        dialog
    }
}

// ----------------------------------------------
// Public API
// ----------------------------------------------

pub fn initialize(context: &mut GameUiContext) {
    if DialogMenusSingleton::is_initialized() {
        return; // Initialize only once.
    }

    DialogMenusSingleton::initialize(DialogMenusSingleton::new(context));
}

pub fn reset() {
    DialogMenusSingleton::get_mut().reset();
}

pub fn is_open(dialog_menu_kind: DialogMenuKind) -> bool {
    // Only the current stack top is considered "open" here.
    current().is_some_and(|dialog| dialog == dialog_menu_kind)
}

pub fn open(dialog_menu_kind: DialogMenuKind, close_all_others: bool, context: &mut GameUiContext) -> bool {
    if close_all_others {
        close_all(context);
    }

    DialogMenusSingleton::get_mut().open_dialog(dialog_menu_kind, context)
}

pub fn close(dialog_menu_kind: DialogMenuKind, context: &mut GameUiContext) -> bool {
    DialogMenusSingleton::get_mut().close_dialog(dialog_menu_kind, context)
}

pub fn close_all(context: &mut GameUiContext) -> bool {
    DialogMenusSingleton::get_mut().close_all(context)
}

pub fn current() -> Option<DialogMenuKind> {
    DialogMenusSingleton::get_mut().current_dialog().map(|dialog| dialog.kind())
}

pub fn current_as<Dialog: DialogMenu>() -> Option<&'static mut Dialog> {
    DialogMenusSingleton::get_mut().current_dialog_as::<Dialog>()
}

pub fn find<Dialog: DialogMenuFactory>() -> &'static mut Dialog {
    DialogMenusSingleton::get_mut().find_dialog_as::<Dialog>()
}

pub fn close_current(context: &mut GameUiContext) -> bool {
    DialogMenusSingleton::get_mut().close_current(context)
}

pub fn draw_current(context: &mut GameUiContext) {
    DialogMenusSingleton::get_mut().draw_current(context);
}

pub fn set_global_menu_flags(flags: UiMenuFlags) {
    DialogMenusSingleton::get_mut().set_global_menu_flags(flags);
}

pub fn set_bg_dim_alpha(context: &mut GameUiContext, alpha: f32) {
    debug_assert!((0.0..=1.0).contains(&alpha));
    context.ui_sys.set_style_color(imgui::StyleColor::ModalWindowDimBg, Color::new(0.0, 0.0, 0.0, alpha));
}

// ----------------------------------------------
// DialogMenu
// ----------------------------------------------

#[enum_dispatch(DialogMenuImpl)]
pub trait DialogMenu: Any {
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

    fn open(&mut self, context: &mut GameUiContext) -> bool {
        self.menu_mut().open(context);
        true
    }

    fn close(&mut self, context: &mut GameUiContext) -> bool {
        let menu = self.menu_mut();
        if menu.is_message_box_open() {
            menu.close_message_box(context);
            false
        } else {
            menu.close(context);
            true
        }
    }

    fn draw_root_menu(&mut self, context: &mut GameUiContext) {
        self.menu_mut().draw(context);
    }

    fn draw_child_menu(&mut self, context: &mut GameUiContext, root_menu: &mut DialogMenuImpl) {
        let this_menu = self.menu();

        let mut menu_rc_on_close = this_menu.clone();
        let mut menu_rc_get_msg_box = this_menu.clone();
        let mut menu_rc_on_draw = this_menu.clone();

        root_menu.menu_mut().draw_custom(
            context,
            this_menu.flags(),
            move |_, context| {
                // NOTE: Send close event to child menu, not to the root.
                if menu_rc_on_close.is_message_box_open() {
                    menu_rc_on_close.close_message_box(context);
                } else {
                    menu_rc_on_close.close(context);
                }
            },
            move |_| menu_rc_get_msg_box.message_box(),
            move |_, context| {
                // Draw child menu inside root layout.
                menu_rc_on_draw.draw_menu_contents(context);
            },
        );
    }
}

// ----------------------------------------------
// DialogMenuFactory
// ----------------------------------------------

pub trait DialogMenuFactory: DialogMenu {
    const KIND: DialogMenuKind;
    const TITLE: &'static [&'static str];

    fn create(context: &mut GameUiContext) -> DialogMenuImpl;
}

// ----------------------------------------------
// Macro: implement_dialog_menu
// ----------------------------------------------

macro_rules! implement_dialog_menu {
    ($dialog_menu_struct:ident, $title:expr) => {
        impl DialogMenuFactory for $dialog_menu_struct {
            const KIND: DialogMenuKind = DialogMenuKind::$dialog_menu_struct;
            const TITLE: &'static [&'static str] = &$title;

            fn create(context: &mut GameUiContext) -> DialogMenuImpl {
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

pub(crate) use implement_dialog_menu;

// ----------------------------------------------
// DialogMenusSingleton
// ----------------------------------------------

const DIALOG_MENU_STACK_MAX_DEPTH: usize = 8;

struct DialogMenusSingleton {
    dialog_menus: ArrayVec<DialogMenuImpl, DIALOG_MENU_COUNT>,
    menu_stack: ArrayVec<DialogMenuKind, DIALOG_MENU_STACK_MAX_DEPTH>,
}

impl DialogMenusSingleton {
    fn new(context: &mut GameUiContext) -> Self {
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
        GLOBAL_DIALOG_MENU_FLAGS.set(flags | DEFAULT_DIALOG_MENU_FLAGS);

        for dialog in &mut self.dialog_menus {
            let menu = dialog.menu_mut();

            let mut new_flags = flags;

            // Preserve this flag.
            if menu.has_flags(UiMenuFlags::HideWhenMessageBoxOpen) {
                new_flags |= UiMenuFlags::HideWhenMessageBoxOpen;
            }

            menu.reset_flags(new_flags | DEFAULT_DIALOG_MENU_FLAGS);
        }
    }

    fn current_dialog_with_root_menu(&mut self) -> Option<(&mut DialogMenuImpl, Option<&mut DialogMenuImpl>)> {
        fn get2<T>(slice: &mut [T], i: usize, j: usize) -> (&mut T, &mut T) {
            debug_assert!(i != j);
            if i < j {
                let (left, right) = slice.split_at_mut(j);
                (&mut left[i], &mut right[0])
            } else {
                let (left, right) = slice.split_at_mut(i);
                (&mut right[0], &mut left[j])
            }
        }

        let stack_length = self.menu_stack.len();

        // Return stack top and root menu if we have one.
        // NOTE: Root menu is not necessarily the direct ancestor of the stack top.
        if stack_length > 1 {
            let stack_top = self.menu_stack[stack_length - 1];
            let root_menu = self.menu_stack[0];

            let (dialog, root) = get2(&mut self.dialog_menus, stack_top as usize, root_menu as usize);

            debug_assert!(dialog.kind() == stack_top);
            debug_assert!(root.kind() == root_menu);

            return Some((dialog, Some(root)));
        }

        // Root menu or empty stack.
        self.current_dialog().map(|dialog| (dialog, None))
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
        dialog
            .as_any_mut()
            .downcast_mut::<Dialog>()
            .unwrap_or_else(|| panic!("Expected dialog menu kind to be {}!", Dialog::KIND))
    }

    fn is_current_dialog(&self, dialog_menu_kind: DialogMenuKind) -> bool {
        if let Some(&stack_top) = self.menu_stack.last() {
            return stack_top == dialog_menu_kind;
        }
        false
    }

    fn push_dialog(&mut self, dialog_menu_kind: DialogMenuKind) {
        debug_assert!(!self.menu_stack.contains(&dialog_menu_kind), "Dialog menu {dialog_menu_kind} is already open!");
        self.menu_stack.push(dialog_menu_kind);
    }

    fn pop_dialog(&mut self, dialog_menu_kind: DialogMenuKind) {
        let index =
            self.menu_stack.iter().position(|kind| *kind == dialog_menu_kind).expect("Dialog menu not in menu stack!");
        let removed = self.menu_stack.remove(index);
        debug_assert!(removed == dialog_menu_kind, "Closed menu should have been in the menu stack!");
    }

    fn open_dialog(&mut self, dialog_menu_kind: DialogMenuKind, context: &mut GameUiContext) -> bool {
        let dialog = self.find_dialog(dialog_menu_kind);
        if !dialog.is_open() && dialog.open(context) {
            self.push_dialog(dialog_menu_kind);
            return true;
        }
        false
    }

    fn close_dialog(&mut self, dialog_menu_kind: DialogMenuKind, context: &mut GameUiContext) -> bool {
        let dialog = self.find_dialog(dialog_menu_kind);
        if dialog.is_open() && dialog.close(context) {
            self.pop_dialog(dialog_menu_kind);
            return true;
        }
        false
    }

    fn close_all(&mut self, context: &mut GameUiContext) -> bool {
        let mut any_closed = false;
        // Close all open menus:
        for dialog_menu_kind in self.menu_stack.clone() {
            if self.close_dialog(dialog_menu_kind, context) {
                any_closed = true;
            }
        }
        any_closed
    }

    fn close_current(&mut self, context: &mut GameUiContext) -> bool {
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

    fn draw_current(&mut self, context: &mut GameUiContext) {
        // Draw current open menu only:
        if let Some((dialog, opt_root_menu)) = self.current_dialog_with_root_menu() {
            // NOTE: If the current menu is a child modal dialog we want to render it
            // inside its root menu, using the root layout. This is necessary to avoid
            // flickering when switching between menus. If we instead stopped rendering
            // the previous menu and opened a new one, there might a visible flicker when
            // the switch between menus happen. With this approach, we always keep the
            // root menu open and change its contents instead. The main limitation with
            // this approach is that all dialog menus will have to share the same size
            // and background image.
            if let Some(root_menu) = opt_root_menu {
                dialog.draw_child_menu(context, root_menu);
            } else {
                dialog.draw_root_menu(context);
            }

            // In case the menu was closed via [ESCAPE] key (handled internally by modal menus).
            let dialog_menu_kind = dialog.kind();
            if !dialog.is_open() && self.is_current_dialog(dialog_menu_kind) {
                self.pop_dialog(dialog_menu_kind);

                if dialog_menu_kind != DialogMenuKind::Home {
                    // Simulate a UI button press sound when hitting [ESCAPE] while inside a modal dialog.
                    sound::play(*context.sound_sys(), UiSoundKey::ButtonPressed);
                }

                // If we didn't close the last dialog, keep the game in paused state.
                if !self.menu_stack.is_empty() {
                    context.sim.pause();
                }
            }
        }

        // [Debug]:
        const DEBUG_DRAW_MENU_STACK: bool = false;
        if DEBUG_DRAW_MENU_STACK && !self.menu_stack.is_empty() {
            let ui = context.ui_sys.ui();
            let position = Vec2::new(context.viewport_size.width as f32 - 350.0, 100.0);
            ui::overlay(ui, "Dialog Menu Stack Debug", position, 1.0, || {
                for (index, kind) in self.menu_stack.iter().enumerate() {
                    ui.text(format!("[{index}]: {kind}"));
                }
            });
        }
    }
}

// Global instance:
common::singleton_late_init! { DIALOG_MENUS_SINGLETON, DialogMenusSingleton }

// ----------------------------------------------
// Internal Shared Constants & Helper Functions
// ----------------------------------------------

// For common dialog menus:
const DEFAULT_DIALOG_MENU_BUTTON_SPACING: Vec2 = Vec2::new(8.0, 8.0); // Large stacked buttons
const DEFAULT_DIALOG_MENU_WIDGET_SPACING: Vec2 = Vec2::new(6.0, 6.0); // Settings widgets / input fields
const DEFAULT_DIALOG_MENU_WIDGET_LABEL_SPACING: f32 = 5.0;
const DEFAULT_DIALOG_MENU_HEADING_MARGINS: (f32, f32) = (100.0, 10.0); // (top, bottom)
const DEFAULT_DIALOG_MENU_WIDGET_FONT_SCALE: UiFontScale = UiFontScale(1.0);
const DEFAULT_DIALOG_MENU_HEADING_FONT_SCALE: UiFontScale = UiFontScale(1.8);
const DEFAULT_DIALOG_MENU_BACKGROUND_SPRITE: PathRef = PathRef::from_str("misc/scroll_bg.png");

// For popup message boxes:
const DEFAULT_DIALOG_POPUP_FONT_SCALE: UiFontScale = UiFontScale(1.5);
const DEFAULT_DIALOG_POPUP_BACKGROUND_SPRITE: PathRef = PathRef::from_str("misc/square_page_bg.png");

const DEFAULT_DIALOG_MENU_FLAGS: UiMenuFlags = UiMenuFlags::from_bits_retain(
    UiMenuFlags::Modal.bits() | UiMenuFlags::CloseModalOnEscape.bits() | UiMenuFlags::PauseSimIfOpen.bits(),
);

static GLOBAL_DIALOG_MENU_FLAGS: UiStaticVar<UiMenuFlags> = UiStaticVar::new(DEFAULT_DIALOG_MENU_FLAGS);

fn default_dialog_menu_size(context: &GameUiContext) -> Vec2 {
    Vec2::new(550.0, context.viewport_size.height as f32 - 150.0)
}

fn make_default_layout_dialog_menu(
    context: &mut GameUiContext,
    dialog_menu_kind: DialogMenuKind,
    heading_title: &[&str], // Each item in the slice is a heading line.
    widget_spacing: Vec2,
    widgets: Option<impl IntoIterator<Item = UiWidgetImpl>>,
) -> UiMenuRcMut {
    let menu_size = default_dialog_menu_size(context);
    let mut menu = UiMenu::new(context, UiMenuParams {
        label: Some(dialog_menu_kind.to_string()),
        flags: *GLOBAL_DIALOG_MENU_FLAGS,
        size: Some(menu_size),
        background: Some(DEFAULT_DIALOG_MENU_BACKGROUND_SPRITE),
        ..Default::default()
    });

    let heading = UiMenuHeading::new(context, UiMenuHeadingParams {
        lines: heading_title
            .iter()
            .map(|line| UiText::new(line.to_string(), DEFAULT_DIALOG_MENU_HEADING_FONT_SCALE))
            .collect(),
        separator: Some(LARGE_HORIZONTAL_SEPARATOR_SPRITE),
        margin_top: DEFAULT_DIALOG_MENU_HEADING_MARGINS.0,
        margin_bottom: DEFAULT_DIALOG_MENU_HEADING_MARGINS.1,
        ..Default::default()
    });

    menu.add_widget(heading);

    if let Some(widgets) = widgets {
        let mut group = UiWidgetGroup::new(context, UiWidgetGroupParams {
            widget_spacing,
            center_vertically: false,
            center_horizontally: true,
            ..Default::default()
        });

        for widget in widgets {
            group.add_widget(widget);
        }

        menu.add_widget(group);
    }

    menu
}

fn make_dialog_button_widgets<ButtonKind, const COUNT: usize>(context: &mut GameUiContext) -> ArrayVec<UiWidgetImpl, COUNT>
where
    ButtonKind: ButtonDef + EnumCount + IntoEnumIterator + 'static,
{
    let mut buttons = ArrayVec::<UiWidgetImpl, COUNT>::new();

    for button_kind in ButtonKind::iter() {
        let on_pressed = UiTextButtonPressed::with_closure(move |_, context| {
            button_kind.on_pressed(ui::widgets::context_as_mut::<GameUiContext>(context));
        });

        buttons.push(UiWidgetImpl::from(button_kind.new_text_button(
            context,
            UiButtonSoundsEnabled::all(),
            UiTextButtonSize::Large,
            on_pressed,
        )));
    }

    buttons
}
