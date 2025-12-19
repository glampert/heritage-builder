use bitflags::bitflags;
use arrayvec::ArrayVec;
use num_enum::TryFromPrimitive;
use std::{any::{Any, TypeId}, path::Path};
use strum::{EnumCount, EnumProperty, IntoEnumIterator};
use strum_macros::{EnumCount, EnumProperty, EnumIter};

use super::{
    widgets,
    bar::MenuBar,
};
use crate::{
    imgui_ui::UiSystem,
    utils::{Size, Vec2, mem},
    game::{sim::Simulation, GameLoop, DEFAULT_SAVE_FILE_NAME, AUTOSAVE_FILE_NAME},
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

    fn as_any_mut(&mut self) -> &mut dyn Any {
        mem::mut_ref_cast(self.as_any())
    }

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
        // Actually this should be a "Restart Level" button instead.
        // New Game can be selected from main menu, when we have it.
    }

    fn on_load_game_button(sim: &mut Simulation, parent: &mut dyn MenuBar) {
        let modal_menu =
            parent.open_modal_menu(sim, ModalMenuId::of::<SaveGameModalMenu>()).unwrap();
        debug_assert!(modal_menu.is_open());

        let save_menu = modal_menu.as_any_mut().downcast_mut::<SaveGameModalMenu>().unwrap();
        save_menu.set_actions(SaveGameActions::Load);
    }

    fn on_save_game_button(sim: &mut Simulation, parent: &mut dyn MenuBar) {
        let modal_menu =
            parent.open_modal_menu(sim, ModalMenuId::of::<SaveGameModalMenu>()).unwrap();
        debug_assert!(modal_menu.is_open());

        let save_menu = modal_menu.as_any_mut().downcast_mut::<SaveGameModalMenu>().unwrap();
        save_menu.set_actions(SaveGameActions::Save);
    }

    fn on_settings_button(sim: &mut Simulation, parent: &mut dyn MenuBar) {
        parent.open_modal_menu(sim, ModalMenuId::of::<SettingsModalMenu>());
    }

    fn on_quit_button() {
        GameLoop::get_mut().request_quit();
        // TODO: Should open a child modal popup with the options: "Quit to Main Menu" and "Exit Game".
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

bitflags! {
    #[derive(Copy, Clone, PartialEq, Eq)]
    pub struct SaveGameActions: u32 {
        const Save = 1 << 0;
        const Load = 1 << 1;
    }
}

const SAVE_GAME_LIST_FRAME_SIZE: Size = Size::new(
    MODAL_WINDOW_DEFAULT_SIZE.width - 100,
    MODAL_WINDOW_DEFAULT_SIZE.height / 2
);

pub struct SaveGameModalMenu {
    menu: BasicModalMenu,
    actions: SaveGameActions,
    save_file_name: String,
}

impl SaveGameModalMenu {
    pub fn new(_title: String, _parent: &dyn MenuBar) -> Self {
        let mut menu = Self {
            menu: BasicModalMenu::new(String::new(), Some(MODAL_WINDOW_DEFAULT_SIZE)),
            actions: SaveGameActions::empty(),
            save_file_name: String::new(),
        };
        // NOTE: Title set here instead.
        menu.set_actions(SaveGameActions::Save | SaveGameActions::Load);
        menu
    }

    pub fn set_actions(&mut self, actions: SaveGameActions) {
        if self.actions != actions {
            self.actions = actions;
            self.menu.title.clear();
            if self.actions.intersects(SaveGameActions::Load) &&
               self.actions.intersects(SaveGameActions::Save) {
                self.menu.title += "Load or Save a Game";
            } else if self.actions.intersects(SaveGameActions::Load) {
                self.menu.title += "Load Saved Game";
            } else if self.actions.intersects(SaveGameActions::Save) {
                self.menu.title += "Save Game";
            }
        }
    }

    fn set_default_save_file_name(&mut self) {
        if !self.save_file_name.is_empty() {
            return;
        }

        let default_file_name =
            if self.actions.intersects(SaveGameActions::Load) {
                AUTOSAVE_FILE_NAME
            } else {
                DEFAULT_SAVE_FILE_NAME
            };

        self.save_file_name =
            Path::new(default_file_name)
                .with_extension("")
                .to_str().unwrap().into();
    }

    fn calc_centered_group_start(ui: &imgui::Ui, group_width: f32, list_height: f32) -> Vec2 {
        let avail = ui.content_region_avail();
        let avail_width = avail[0];
        let avail_height = avail[1];

        let group_height =
            ui.frame_height_with_spacing() + // input_text
            list_height +                    // child window
            ui.frame_height_with_spacing();  // buttons row

        let start_x = ((avail_width  - group_width)  * 0.5).max(0.0);
        let start_y = ((avail_height - group_height) * 0.5).max(0.0);

        Vec2::new(start_x, start_y)
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
        // Default value when opened. Can be overwritten.
        self.set_actions(SaveGameActions::Save | SaveGameActions::Load);
        self.menu.open(sim);
    }

    fn close(&mut self, sim: &mut Simulation) {
        self.menu.close(sim);
    }

    fn draw(&mut self, sim: &mut Simulation, ui_sys: &UiSystem) {
        if !self.is_open() {
            return;
        }

        self.set_default_save_file_name();

        self.menu.draw(sim, ui_sys, |_sim| {
            let game_loop = GameLoop::get_mut();
            let file_list = game_loop.save_files_list();

            let ui = ui_sys.ui();
            let _font = ui.push_font(ui_sys.fonts().game_hud_large);

            // Align the following group of widgets to the window center.
            let group_size  = SAVE_GAME_LIST_FRAME_SIZE.to_vec2();
            let group_start = Self::calc_centered_group_start(ui, group_size.x, group_size.y);

            ui.set_cursor_pos([group_start.x, group_start.y]);
            ui.set_next_item_width(group_size.x);
            ui.input_text("##SaveFileName", &mut self.save_file_name).build();

            ui.set_cursor_pos([group_start.x, ui.cursor_pos()[1]]);
            ui.child_window("SaveFileList")
                .size(group_size.to_array())
                .border(true)
                .build(|| {
                    let mut selected_file_index: Option<usize> = None;
                    for (index, path) in file_list.iter().enumerate() {
                        let is_selected = selected_file_index == Some(index);
                        let file_name_no_ext = path.with_extension("");
                        let save_file_name = file_name_no_ext.to_str().unwrap();
                        if ui.selectable_config(save_file_name)
                            .selected(is_selected)
                            .build()
                        {
                            selected_file_index = Some(index);
                            self.save_file_name = save_file_name.into();
                        }
                    }
                });

            ui.set_cursor_pos([group_start.x, ui.cursor_pos()[1]]);
            if self.actions.intersects(SaveGameActions::Load) {
                if ui.button("Load Game") && !self.save_file_name.is_empty() {
                    game_loop.load_save_game(&self.save_file_name);
                }

                ui.same_line();
                // Extra spacing between buttons.
                widgets::spacing(ui, &ui.get_window_draw_list(), Vec2::new(5.0, 0.0));
                ui.same_line();
            }

            if self.actions.intersects(SaveGameActions::Save) {
                if ui.button("Save Game") && !self.save_file_name.is_empty() {
                    if file_list.iter().any(
                        |file| file.file_stem().unwrap().eq_ignore_ascii_case(&self.save_file_name))
                    {
                        // TODO: Show confirmation popup asking if user wants to overwrite existing save file.
                    }

                    game_loop.save_game(&self.save_file_name);
                }
            }
        });
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
