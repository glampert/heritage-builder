use std::any::Any;

use arrayvec::ArrayVec;
use common::{Vec2, mem};
use engine::{
    file_sys::paths::PathRef,
    ui::{
        self,
        UiFontScale,
        UiStaticVar,
        UiString,
        sound::{UiSoundEvent, UiSoundKey},
        text::UiTextCategory,
        widgets::*,
    },
};
use enum_dispatch::enum_dispatch;
use strum::{Display, EnumCount, EnumDiscriminants, EnumIter, IntoEnumIterator};

use super::{
    make_text_button,
    skin::{self, ButtonRole, HeadingLevel, SkinAlign, SkinDialogSize},
};
use crate::{menu::ButtonDef, profile, ui_context::GameUiContext, campaign::config::MissionMap};

mod home;
use home::*;

mod campaign_picker;
use campaign_picker::*;

mod main_game;
use main_game::*;

mod custom_game;
use custom_game::*;

mod about;
use about::*;

mod load_save;
use load_save::*;

mod city_management;
use city_management::*;

mod settings;
use settings::*;

mod mission_complete;
use mission_complete::*;

mod campaign_complete;
use campaign_complete::*;

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
    CampaignPicker,
    MainGame,
    CustomGame,
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

    // Campaign progression menus:
    MissionComplete,
    CampaignComplete,
}

const DIALOG_MENU_FACTORIES: [DialogMenuFactoryFn; DIALOG_MENU_COUNT] = dialog_menu_factories![
    Home,
    CampaignPicker,
    MainGame,
    CustomGame,
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

    MissionComplete,
    CampaignComplete,
];

impl DialogMenuKind {
    fn build_menu(self, context: &mut GameUiContext) -> DialogMenuImpl {
        let mut dialog = DIALOG_MENU_FACTORIES[self as usize](context);
        debug_assert!(dialog.kind() == self, "Wrong DialogMenuKind! Check DialogMenu::kind() impl!");

        // Every dialog plays its open and close sounds (its menu plays them, so Escape, a Close
        // button and a stacked dialog's close all do), but Home: it's the home screen itself,
        // reopened whenever nothing is over it. Set here, so a rebuilt dialog keeps them.
        if self != DialogMenuKind::Home {
            let menu = dialog.menu_mut();
            menu.set_sound(UiSoundEvent::Opened, Some(UiSoundKey::DialogOpened));
            menu.set_sound(UiSoundEvent::Closed, Some(UiSoundKey::DialogClosed));
        }

        dialog
    }

    // The dialogs that show the player's progress and the last save: built again when the profile
    // changes and whenever the home menu opens (rebuild_profile_dialogs).
    fn shows_profile(self) -> bool {
        matches!(self, DialogMenuKind::Home | DialogMenuKind::CampaignPicker)
    }

    // The dialogs whose contents scale with the window (skin::dialog_content_scale), besides the
    // profile's: built again when the window's size changes how large the dialogs are drawn.
    fn scales_with_window(self) -> bool {
        matches!(self, DialogMenuKind::About | DialogMenuKind::MainGame)
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

// Rebuilds every dialog if the skin or the language changed since they were built (a realm
// switch, entering a session, ui.reload_skin, text.language), keeping the open ones open; the
// ones that show the player's progress if the profile changed, or the window's size changed how
// large the dialogs are drawn (their contents scale with it); and Load / Save and Settings, which
// lay their contents out for the window, when it changed (and the saves, for Load / Save); and
// Custom Game, when its realm or its Blank map's options come or go. The retail menus call it at
// the start of every frame.
pub fn sync(context: &mut GameUiContext) {
    if !DialogMenusSingleton::is_initialized() {
        return;
    }
    let dialogs = DialogMenusSingleton::get_mut();
    if dialogs.built != built_versions() {
        rebuild_all(context);
    } else if dialogs.menu_stack.contains(&DialogMenuKind::Home) && dialogs.profile_built != profile_versions(context) {
        // Only while the home menu shows: in game they'd read the last save for nothing, and the
        // home menu builds them again when it opens.
        rebuild_profile_dialogs(context);
    } else if dialogs.menu_stack.contains(&DialogMenuKind::LoadOrSaveGame)
        && !dialogs.find_dialog(DialogMenuKind::LoadOrSaveGame).menu().is_message_box_open()
        && load_save::needs_rebuild(context)
    {
        // The saves changed (a save, its thumbnail, a deletion), or the window's size changed the
        // dialog's. Not under its popup, which a rebuild would close: once it's gone.
        dialogs.rebuild(context, |kind| kind == DialogMenuKind::LoadOrSaveGame);
    } else if dialogs.menu_stack.contains(&DialogMenuKind::CustomGame)
        && !dialogs.find_dialog(DialogMenuKind::CustomGame).menu().is_message_box_open()
        && custom_game::needs_rebuild(context)
    {
        // The realm changed (its rows and dates), the Blank map was picked or left (its options),
        // or the window's size changed the dialog's. What the player picked is kept (custom_game::STATE).
        dialogs.rebuild(context, |kind| kind == DialogMenuKind::CustomGame);
    } else if dialogs.menu_stack.contains(&DialogMenuKind::MainSettings)
        && !dialogs.find_dialog(DialogMenuKind::MainSettings).menu().is_message_box_open()
        && settings::needs_rebuild(context)
    {
        // The window's size changed the dialog's, which its rows are laid out for. Its pending
        // values and tab are kept (settings::STATE).
        dialogs.rebuild(context, |kind| kind == DialogMenuKind::MainSettings);
    } else if dialogs.window_built != window_version(context)
        && !DialogMenuKind::iter().any(|kind| kind.scales_with_window() && dialogs.find_dialog(kind).menu().is_message_box_open())
    {
        // About and the Game menu, whose contents scale with the window: not under a popup, which
        // a rebuild would close (the Game menu's Quit): once it's gone.
        dialogs.rebuild(context, DialogMenuKind::scales_with_window);
    }
}

// How large the window draws the dialogs, in hundredths: the dialogs whose contents scale with it
// are built again when it changes.
fn window_version(context: &GameUiContext) -> u32 {
    (skin::dialog_scale(ui::screen_size(context.ui_sys)) * 100.0).round() as u32
}

// The dialog a kind opens: LoadGame and SaveGame open LoadOrSaveGame (load_save::redirect);
// GameSettings, SoundSettings and GraphicsSettings open MainSettings (settings::redirect).
fn opened_kind(dialog_menu_kind: DialogMenuKind) -> DialogMenuKind {
    if load_save::redirect(dialog_menu_kind).is_some() {
        DialogMenuKind::LoadOrSaveGame
    } else if settings::redirect(dialog_menu_kind).is_some() {
        DialogMenuKind::MainSettings
    } else {
        dialog_menu_kind
    }
}

// Builds every dialog again, from the current skin and strings. The stack stays: each open
// dialog is opened again, bottom up, without its open sound (its open callback runs, refreshing
// its contents). A popup that was open closes.
// TODO(realm-ui): a button under the cursor plays its hover sound again, since the new widget
// wasn't hovered; carry the hovered state across (or skip hover sounds for a frame).
pub fn rebuild_all(context: &mut GameUiContext) {
    if DialogMenusSingleton::is_initialized() {
        DialogMenusSingleton::get_mut().rebuild(context, |_| true);
    }
}

// Builds the dialogs that show the player's progress and the last save (the home menu, the
// campaign picker) again, as rebuild_all does.
pub fn rebuild_profile_dialogs(context: &mut GameUiContext) {
    if DialogMenusSingleton::is_initialized() {
        DialogMenusSingleton::get_mut().rebuild(context, DialogMenuKind::shows_profile);
    }
}

// Makes the home menu read the last save again when it's next built: the home menu calls it each
// time it opens, since the save may have changed under the same name (the autosave's). Otherwise
// it's read again only when the profile changes.
pub fn forget_last_save() {
    home::forget_last_save();
}

// What the dialogs are built from: the skin's and the strings' versions.
fn built_versions() -> (u32, u32) {
    (skin::version(), ui::text::version())
}

// What the dialogs that show the profile are built from too: its revision, the saves' (Continue's
// save may be deleted or written over: save_header::revision), and how large the window draws the
// dialogs (in hundredths).
fn profile_versions(context: &GameUiContext) -> (u32, u32, u32) {
    let scale = skin::dialog_scale(ui::screen_size(context.ui_sys));
    (profile::revision(), crate::save_header::revision(), (scale * 100.0).round() as u32)
}

// Dialogs exist once the home or in-game menus have set them up (not in the dev editor alone).
pub fn is_initialized() -> bool {
    DialogMenusSingleton::is_initialized()
}

pub fn is_open(dialog_menu_kind: DialogMenuKind) -> bool {
    // Only the current stack top is considered "open" here.
    current().is_some_and(|dialog| dialog == opened_kind(dialog_menu_kind))
}

// Whether opening `kind` while `other` is open switches the open dialog instead of opening another:
// both open the Settings dialog, which shows `kind`'s tab, its pending edits kept (open_dialog).
// Not Load / Save, whose load or save mode is picked when it's built.
pub fn switches_in_place(kind: DialogMenuKind, other: DialogMenuKind) -> bool {
    settings::redirect(kind).is_some() && settings::redirect(other).is_some()
}

// Whether the dialog is anywhere in the stack, on top or under others.
pub fn is_in_stack(dialog_menu_kind: DialogMenuKind) -> bool {
    DialogMenusSingleton::get().menu_stack.contains(&opened_kind(dialog_menu_kind))
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

// The menus' Escape (HomeMenus::handle_input, InGameMenus::handle_custom_input): closes the top
// dialog's popup if it has one open, else the dialog on top, but never the home menu's own dialog.
// True if it closed either, so the key goes no further. Only the top one: the dialogs don't close
// themselves on Escape, except on one a text field in the top dialog took from the menus
// (draw_current).
pub fn close_current_on_escape(context: &mut GameUiContext) -> bool {
    DialogMenusSingleton::get_mut().close_current_on_escape(context)
}

pub fn draw_current(context: &mut GameUiContext) {
    DialogMenusSingleton::get_mut().draw_current(context);
}

// Whether the dialogs are built and drawn for the home menu or for a game: the retail menus say so
// before they build any (HomeMenus::new, InGameMenus::new). In game, the home menu is built without
// reading the last save, which takes a parse of the whole save; the home menu builds it again when
// it opens. In the home menu, a dialog over Home dims the backdrop (draw_current).
static HOME_MENU: UiStaticVar<bool> = UiStaticVar::new(false);

pub fn set_home_menu(home_menu: bool) {
    *HOME_MENU.as_mut() = home_menu;
}

fn in_home_menu() -> bool {
    *HOME_MENU.as_ref()
}

// Dims the screen behind the dialogs with the skin's colour for the home menu or the game.
pub fn set_backdrop_dim(context: &mut GameUiContext, home_menu: bool) {
    context.ui_sys.set_modal_backdrop_color(skin::backdrop_dim(home_menu));
}

// Load the starting map for a campaign mission. Routes through the GameLoop
// session command queue (deferred). This is the UI-side glue that the
// GameLoop-free campaign manager deliberately does not perform itself.
fn load_mission_map(map: &MissionMap) {
    let game_loop = crate::GameLoop::get_mut();
    match map {
        MissionMap::SaveGame { save_file }     => game_loop.load_save_game(PathRef::from_str(save_file)),
        MissionMap::Preset   { preset_number } => game_loop.load_preset_map(*preset_number),
    }
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

    // The dialog on top of the stack: a modal window, taking the input and dimming what's under it.
    fn draw_on_top(&mut self, context: &mut GameUiContext) {
        self.menu_mut().draw(context);
    }

    // A dialog under the top one that stays visible (DialogFrame::show_below): a plain window, inert
    // under the top one's modal backdrop.
    fn draw_below(&mut self, context: &mut GameUiContext) {
        let menu = self.menu_mut();
        let flags = menu.flags() & !UiMenuFlags::Modal;
        menu.draw_with_flags(context, flags);
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
    frames: ArrayVec<DialogFrame, DIALOG_MENU_COUNT>, // Each dialog's window, from the skin it was built with.
    menu_stack: ArrayVec<DialogMenuKind, DIALOG_MENU_STACK_MAX_DEPTH>,
    built: (u32, u32),              // built_versions() when the dialogs were built.
    profile_built: (u32, u32, u32), // profile_versions() when the dialogs that show the profile were.
    window_built: u32,              // window_version() when the dialogs that scale with the window were.
}

impl DialogMenusSingleton {
    fn new(context: &mut GameUiContext) -> Self {
        let mut dialog_menus = ArrayVec::new();
        let mut frames = ArrayVec::new();

        for dialog_menu_kind in DialogMenuKind::iter() {
            dialog_menus.push(dialog_menu_kind.build_menu(context));
            frames.push(DialogFrame::from_skin(dialog_menu_kind));
        }

        Self {
            dialog_menus,
            frames,
            menu_stack: ArrayVec::new(),
            built: built_versions(),
            profile_built: profile_versions(context),
            window_built: window_version(context),
        }
    }

    // Builds the dialogs `which` picks again. The stack stays: each of them that's open is opened
    // again, without its open sound.
    fn rebuild(&mut self, context: &mut GameUiContext, which: impl Fn(DialogMenuKind) -> bool) {
        let stack = self.menu_stack.clone();
        if DialogMenuKind::iter().all(&which) {
            self.built = built_versions();
        }
        if DialogMenuKind::iter().filter(|kind| kind.shows_profile()).all(&which) {
            self.profile_built = profile_versions(context);
        }
        if DialogMenuKind::iter().filter(|kind| kind.scales_with_window()).all(&which) {
            self.window_built = window_version(context);
        }

        // The old dialogs go without closing (their callbacks would resume the simulation and
        // play sounds); the new ones open below. What their widgets held is lost, so a dialog
        // whose edits must survive keeps them outside its widgets, read by the new ones: Settings'
        // pending values and tab (settings::STATE), Load / Save's typed name (load_save::STATE).
        // Those are read afresh only when the dialog is opened (open_dialog), not reopened here.
        for dialog_menu_kind in DialogMenuKind::iter().filter(|kind| which(*kind)) {
            self.dialog_menus[dialog_menu_kind as usize] = dialog_menu_kind.build_menu(context);
            self.frames[dialog_menu_kind as usize] = DialogFrame::from_skin(dialog_menu_kind);
        }

        for dialog_menu_kind in stack.into_iter().filter(|kind| which(*kind)) {
            let dialog = self.find_dialog(dialog_menu_kind);
            let open_sound = (dialog_menu_kind != DialogMenuKind::Home).then_some(UiSoundKey::DialogOpened);
            dialog.menu_mut().set_sound(UiSoundEvent::Opened, None);
            let opened = dialog.open(context);
            dialog.menu_mut().set_sound(UiSoundEvent::Opened, open_sound);
            if !opened {
                self.pop_dialog(dialog_menu_kind);
            }
        }
    }

    // Drops the stack without closing the dialogs (a session switch): their close callbacks don't
    // run, so state they'd clear on closing is cleared here.
    fn reset(&mut self) {
        self.menu_stack.clear();
        for dialog in &mut self.dialog_menus {
            dialog.reset();
        }
        campaign_picker::forget_pick();
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
        // LoadGame and SaveGame open LoadOrSaveGame in their mode, built afresh from the saves.
        let dialog_menu_kind = match load_save::redirect(dialog_menu_kind) {
            Some(mode) => {
                let kind = DialogMenuKind::LoadOrSaveGame;
                if self.find_dialog(kind).is_open() {
                    return false;
                }
                load_save::prepare(mode);
                self.dialog_menus[kind as usize] = kind.build_menu(context);
                self.frames[kind as usize] = DialogFrame::from_skin(kind);
                kind
            }
            None => dialog_menu_kind,
        };

        // MainSettings and the kinds that open it on their tab: built afresh from the configs, or
        // switched to that tab if it's open, its pending edits kept.
        let dialog_menu_kind = match settings::redirect(dialog_menu_kind) {
            Some(tab) => {
                let kind = DialogMenuKind::MainSettings;
                if self.find_dialog(kind).is_open() {
                    self.find_dialog_as::<MainSettings>().select_tab(tab);
                    return false;
                }
                settings::prepare(tab);
                self.dialog_menus[kind as usize] = kind.build_menu(context);
                self.frames[kind as usize] = DialogFrame::from_skin(kind);
                kind
            }
            None => dialog_menu_kind,
        };

        // Custom Game: built afresh from the maps and the profile.
        if dialog_menu_kind == DialogMenuKind::CustomGame && !self.find_dialog(dialog_menu_kind).is_open() {
            custom_game::prepare(in_home_menu());
            self.dialog_menus[dialog_menu_kind as usize] = dialog_menu_kind.build_menu(context);
            self.frames[dialog_menu_kind as usize] = DialogFrame::from_skin(dialog_menu_kind);
        }

        let dialog = self.find_dialog(dialog_menu_kind);
        if !dialog.is_open() && dialog.open(context) {
            self.push_dialog(dialog_menu_kind);
            return true;
        }
        false
    }

    fn close_dialog(&mut self, dialog_menu_kind: DialogMenuKind, context: &mut GameUiContext) -> bool {
        let dialog_menu_kind = opened_kind(dialog_menu_kind);
        let dialog = self.find_dialog(dialog_menu_kind);
        if dialog.is_open() && dialog.close(context) {
            self.pop_dialog(dialog_menu_kind);
            self.keep_paused(context);
            return true;
        }
        false
    }

    // Pausing isn't counted: a closing dialog resumes the simulation (PauseSimIfOpen), so while
    // dialogs remain open it's paused again.
    fn keep_paused(&self, context: &mut GameUiContext) {
        if !self.menu_stack.is_empty() {
            context.sim.pause();
        }
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
        match self.menu_stack.last() {
            Some(&stack_top) => self.close_dialog(stack_top, context),
            None => false,
        }
    }

    fn close_current_on_escape(&mut self, context: &mut GameUiContext) -> bool {
        let Some(&stack_top) = self.menu_stack.last() else {
            return false;
        };

        let menu = self.find_dialog(stack_top).menu_mut();
        if menu.is_message_box_open() {
            menu.close_message_box(context);
            return true;
        }

        stack_top != DialogMenuKind::Home && self.close_dialog(stack_top, context)
    }

    // Draws the dialogs that show: the one on top of the stack, as a modal window, and under it
    // those its frame keeps visible (show_below), each in its own window and from the bottom up,
    // as plain windows under the top one's backdrop. The others aren't drawn, so their widgets
    // aren't recorded either. Each window takes its size for the window as it is now.
    fn draw_current(&mut self, context: &mut GameUiContext) {
        // Escape the menus didn't see because a focused text field in the top dialog took it (the
        // Save Game name, a number being typed) closes the dialog here, on the first press, as the
        // menus' handler would have. Not one a field in another window took (the log viewer's
        // console), nor one an open dropdown list took to close itself.
        if let Some(&top) = self.menu_stack.last()
            && self.find_dialog(top).menu().took_escape(context.ui_sys)
        {
            self.close_current_on_escape(context);
        }

        let stack = self.menu_stack.clone();
        if let Some(&top) = stack.last() {
            // In the home menu, the backdrop shows clear around Home's own panel and dims behind a
            // dialog opened over it, as the mockups draw them.
            if in_home_menu() {
                let dim = if top == DialogMenuKind::Home { skin::backdrop_dim(true) } else { skin::backdrop_dim_over_home() };
                context.ui_sys.set_modal_backdrop_color(dim);
            }

            let window = ui::screen_size(context.ui_sys);
            let first_visible = first_visible_dialog(&stack, |kind| self.frames[kind as usize].shows_below());

            for &kind in &stack[first_visible..] {
                let frame = self.frames[kind as usize];
                let dialog = self.find_dialog(kind);
                dialog.menu_mut().set_size(frame.size(window));
                if let Some(offset) = frame.offset(window) {
                    dialog.menu_mut().set_position(offset);
                }
                if kind == top {
                    dialog.draw_on_top(context);
                } else {
                    dialog.draw_below(context);
                }
            }

            // In case the top dialog closed itself while drawing (its menu has played DialogClosed).
            if !self.find_dialog(top).is_open() && self.is_current_dialog(top) {
                self.pop_dialog(top);
                self.keep_paused(context);
            }
        }

        // [Debug]:
        const DEBUG_DRAW_MENU_STACK: bool = false;
        if DEBUG_DRAW_MENU_STACK && !self.menu_stack.is_empty() {
            let ui_sys = context.ui_sys;
            let position = Vec2::new(context.viewport_size.width as f32 - 350.0, 100.0);
            ui::overlay(ui_sys, ui::UiOverlay::new("Dialog Menu Stack Debug", position).bg_alpha(1.0), |ui_sys| {
                for (index, kind) in self.menu_stack.iter().enumerate() {
                    ui::text(ui_sys, common::format_small!("[{index}]: {kind}"));
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
const DEFAULT_DIALOG_MENU_TEXT_SPACING:   Vec2 = Vec2::new(4.0, 4.0); // Lines of text: the city management lists
const DEFAULT_DIALOG_MENU_WIDGET_LABEL_SPACING: f32 = 12.0;
const DEFAULT_DIALOG_MENU_HEADING_MARGIN_BOTTOM: f32 = 10.0;
const DEFAULT_DIALOG_MENU_WIDGET_FONT_SCALE: UiFontScale = UiFontScale(1.0);

// No CloseModalOnEscape: Escape is the menus' (HomeMenus::handle_input, InGameMenus::handle_custom_input),
// which close the top dialog only (close_current_on_escape). With both, the dialog under it, on top
// by the time the dialogs are drawn, saw the same key press and closed too. An Escape a focused text
// field in the top dialog took from the menus closes it in draw_current instead.
const DEFAULT_DIALOG_MENU_FLAGS: UiMenuFlags = UiMenuFlags::from_bits_retain(
    UiMenuFlags::Modal.bits() | UiMenuFlags::PauseSimIfOpen.bits(),
);

const ALIGN_FLAGS: UiMenuFlags = UiMenuFlags::from_bits_retain(
    UiMenuFlags::AlignCenter.bits() | UiMenuFlags::AlignCenterTop.bits() | UiMenuFlags::AlignLeft.bits() | UiMenuFlags::AlignRight.bits(),
);

// A dialog's window, from the skin's `dialogs` block (skin::SkinDialogDef): its size (in points at
// 1920 × 1080, or the whole window), its alignment, its offset from where that puts it, and
// whether the dialog under it stays visible. The panel is the menu's own, set when it's built.
#[derive(Copy, Clone, PartialEq, Debug)]
struct DialogFrame {
    size: SkinDialogSize,
    align: Option<SkinAlign>,
    offset: Option<[f32; 2]>,
    show_below: bool,
}

impl DialogFrame {
    // The code's default when neither the dialog's entry nor `default` says: the dialog under it
    // is hidden, as the mockups hide Home under Settings.
    const DEFAULT_SHOW_BELOW: bool = false;

    fn from_skin(dialog_menu_kind: DialogMenuKind) -> Self {
        let def = skin::dialog_def(&dialog_menu_kind.to_string());
        Self {
            size: def.size.unwrap_or(SkinDialogSize::DEFAULT),
            align: def.align,
            offset: def.offset,
            show_below: def.show_below.unwrap_or(Self::DEFAULT_SHOW_BELOW),
        }
    }

    // Its size on a window of `window` points.
    fn size(&self, window: Vec2) -> Vec2 {
        skin::dialog_size(self.size, window)
    }

    // Its offset on a window of `window` points (a full-screen dialog has none).
    fn offset(&self, window: Vec2) -> Option<Vec2> {
        self.offset.filter(|_| !self.size.is_fullscreen()).map(|offset| skin::dialog_offset(offset, window))
    }

    // Whether the dialog under it is drawn: a full-screen dialog covers it.
    fn shows_below(&self) -> bool {
        self.show_below && !self.size.is_fullscreen()
    }

    // Its menu's flags: placed where the skin says, else centred (a full-screen dialog centred,
    // covering the window). Every dialog pauses the simulation while open (DEFAULT_DIALOG_MENU_FLAGS).
    // A full-screen dialog also carries the engine's UiMenuFlags::Fullscreen, so the menu says what
    // it is, but its size comes from skin::dialog_size like every dialog's (draw_current sets it
    // each frame, and a set size wins over the flag in UiMenu::calc_window_size): both are the
    // window's size, ui::screen_size, so there's one rule for every dialog kind.
    fn menu_flags(&self) -> UiMenuFlags {
        let fullscreen = self.size.is_fullscreen();
        let mut flags = DEFAULT_DIALOG_MENU_FLAGS;
        if fullscreen {
            flags |= UiMenuFlags::Fullscreen;
        }
        let align = if fullscreen { None } else { self.align.map(SkinAlign::menu_flags) };
        flags | align.unwrap_or(UiMenuFlags::AlignCenter)
    }
}

// The index in `stack` (bottom first) of the lowest dialog drawn: the top one always is, and each
// drawn dialog whose frame shows the one below it (`shows_below`) brings that one in too.
fn first_visible_dialog<Kind: Copy>(stack: &[Kind], shows_below: impl Fn(Kind) -> bool) -> usize {
    let mut first = stack.len().saturating_sub(1);
    while first > 0 && shows_below(stack[first]) {
        first -= 1;
    }
    first
}

// A size given on the reference window and on the smallest (`sizes`: the first at 1920 × 1080, the
// second at 1280 × 720) at a dialog scale of `scale` (skin::dialog_scale): in proportion between
// them. For sizes that shrink faster or slower than the dialogs do (text, buttons).
fn by_window(sizes: (f32, f32), scale: f32) -> f32 {
    let smallest = skin::dialog_scale(skin::DIALOG_MIN_WINDOW);
    let (reference, minimum) = sizes;
    let t = ((scale - smallest) / (1.0 - smallest)).clamp(0.0, 1.0);
    minimum + (reference - minimum) * t
}

// The dialogs' shared layout: the skin's dialog (its panel and size) with the title heading, then
// `widgets`, if any, stacked in a group with `widget_spacing` between them.
fn make_default_layout_dialog_menu(
    context: &mut GameUiContext,
    dialog_menu_kind: DialogMenuKind,
    heading_title: &[&str], // Each item in the slice is a heading line.
    widget_spacing: Vec2,
    widgets: impl IntoIterator<Item = UiWidgetImpl>,
) -> UiMenuRcMut {
    let heading = skin::heading(context, heading_title.iter().map(|line| UiString::from(*line)), HeadingLevel::Dialog);

    let flags = DialogFrame::from_skin(dialog_menu_kind).menu_flags();
    let mut menu = skin::dialog(context, &dialog_menu_kind.to_string(), flags)
        .with_widget_spacing(widget_spacing) // Between its widgets and the lines of its text.
        .with_widget(heading);

    let group = UiWidgetGroup::new().with_widget_spacing(widget_spacing);
    let group = widgets.into_iter().fold(group, |group, widget| group.with_widget(widget));
    if !group.widgets().is_empty() {
        menu = menu.with_widget(group);
    }

    menu.into_rc()
}

// A menu item button per variant of `ButtonKind`, pressing which runs its on_pressed.
fn make_dialog_button_widgets<ButtonKind, const COUNT: usize>(context: &mut GameUiContext) -> ArrayVec<UiWidgetImpl, COUNT>
where
    ButtonKind: ButtonDef + EnumCount + IntoEnumIterator + 'static,
{
    let mut buttons = ArrayVec::<UiWidgetImpl, COUNT>::new();

    for button_kind in ButtonKind::iter() {
        let button = button_kind.new_text_button(context, ButtonRole::MenuItem, move |_, context| {
            button_kind.on_pressed(context_as_mut::<GameUiContext>(context));
        });

        buttons.push(UiWidgetImpl::from(button));
    }

    buttons
}

// A button of `role` that closes the current dialog.
fn make_dialog_close_button(context: &mut dyn UiWidgetContext, role: ButtonRole, label: impl Into<UiString>) -> UiTextButton {
    make_text_button(context, role, label, |_, context| {
        close_current(context_as_mut::<GameUiContext>(context));
    })
}

// ----------------------------------------------
// Tests
// ----------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // A size given on the reference and the smallest window: the first on the reference window or
    // a larger one, the second on the smallest or a smaller one, in proportion between them.
    #[test]
    fn sizes_by_window() {
        let sizes = (56.0, 36.0);
        let close = |a: f32, b: f32| (a - b).abs() < 0.01;
        let smallest = skin::dialog_scale(skin::DIALOG_MIN_WINDOW);
        assert!(close(by_window(sizes, 1.0), 56.0));
        assert!(close(by_window(sizes, smallest), 36.0));
        assert!(close(by_window(sizes, (1.0 + smallest) / 2.0), 46.0));
        assert!(close(by_window(sizes, 0.1), 36.0));
        assert!(close(by_window(sizes, 2.0), 56.0));
        assert!(close(by_window(sizes, skin::dialog_scale(Vec2::new(1440.0, 800.0))), 36.0 + 20.0 * (800.0 / 1080.0 - smallest) / (1.0 - smallest)));
    }
    use crate::menu::skin::SkinDialogSizeMode;

    // The top dialog always shows; under it, each one shows while every dialog above it shows the
    // one below it.
    #[test]
    fn visible_dialogs_follow_show_below() {
        let shows_below = |kind: char| kind.is_uppercase();
        assert_eq!(first_visible_dialog::<char>(&[], shows_below), 0);
        assert_eq!(first_visible_dialog(&['h'], shows_below), 0);
        assert_eq!(first_visible_dialog(&['h', 's'], shows_below), 1); // Settings hides Home.
        assert_eq!(first_visible_dialog(&['h', 'S'], shows_below), 0); // This one keeps it visible.
        assert_eq!(first_visible_dialog(&['h', 's', 'G'], shows_below), 1); // Down to the one that hides.
        assert_eq!(first_visible_dialog(&['H', 'S', 'G'], shows_below), 0); // The bottom one's flag is moot.
        assert_eq!(first_visible_dialog(&['h', 'S', 'g'], shows_below), 2); // Only the top one's flag counts first.
    }

    fn frame(size: SkinDialogSize, align: Option<SkinAlign>, show_below: bool) -> DialogFrame {
        DialogFrame { size, align, offset: None, show_below }
    }

    // An offset scales with the window as the size does; a full-screen dialog takes none.
    #[test]
    fn dialog_offsets_scale() {
        let points = SkinDialogSize::Points([600.0, 960.0]);
        let left = DialogFrame { offset: Some([90.0, 0.0]), ..frame(points, Some(SkinAlign::Left), false) };
        assert_eq!(left.offset(Vec2::new(1920.0, 1080.0)), Some(Vec2::new(90.0, 0.0)));
        assert_eq!(left.offset(Vec2::new(1280.0, 720.0)), Some(Vec2::new(60.0, 0.0)));
        assert_eq!(frame(points, None, false).offset(Vec2::new(1920.0, 1080.0)), None);
        let fullscreen = DialogFrame { offset: Some([90.0, 0.0]), ..frame(SkinDialogSize::Mode(SkinDialogSizeMode::Fullscreen), None, false) };
        assert_eq!(fullscreen.offset(Vec2::new(1920.0, 1080.0)), None);
    }

    // A full-screen dialog covers the window, centred, hides what's under it, and pauses the
    // simulation and takes the input like any other.
    #[test]
    fn fullscreen_dialogs() {
        let fullscreen = frame(SkinDialogSize::Mode(SkinDialogSizeMode::Fullscreen), Some(SkinAlign::Left), true);
        for window in [Vec2::new(1440.0, 800.0), Vec2::new(1920.0, 1080.0), Vec2::new(3840.0, 2160.0), Vec2::new(800.0, 600.0)] {
            assert_eq!(fullscreen.size(window), window);
        }
        assert!(!fullscreen.shows_below());

        let flags = fullscreen.menu_flags();
        assert!(flags.contains(UiMenuFlags::PauseSimIfOpen | UiMenuFlags::Modal | UiMenuFlags::Fullscreen));
        assert!(!flags.intersects(UiMenuFlags::CloseModalOnEscape)); // Escape is the menus'.
        assert_eq!((flags & ALIGN_FLAGS).bits(), UiMenuFlags::AlignCenter.bits());
    }

    #[test]
    fn dialog_frames_place_and_pause() {
        let points = SkinDialogSize::Points([1220.0, 920.0]);
        let left = frame(points, Some(SkinAlign::Left), false).menu_flags();
        assert_eq!((left & ALIGN_FLAGS).bits(), (UiMenuFlags::AlignCenter | UiMenuFlags::AlignLeft).bits());
        assert!(left.contains(UiMenuFlags::PauseSimIfOpen | UiMenuFlags::Modal));
        assert!(!left.intersects(UiMenuFlags::Fullscreen));

        let unaligned = frame(points, None, true).menu_flags();
        assert_eq!((unaligned & ALIGN_FLAGS).bits(), UiMenuFlags::AlignCenter.bits()); // Centred unless the skin says.

        assert!(frame(points, None, true).shows_below());
        assert!(!frame(points, None, false).shows_below());
        assert_eq!(frame(points, None, false).size(Vec2::new(1920.0, 1080.0)), Vec2::new(1220.0, 920.0));
    }
}
