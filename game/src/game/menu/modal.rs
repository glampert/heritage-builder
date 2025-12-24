#![allow(clippy::type_complexity)]

use bitflags::bitflags;
use arrayvec::ArrayVec;
use smallvec::{SmallVec, smallvec};
use num_enum::TryFromPrimitive;
use std::{any::{Any, TypeId}, path::Path};
use strum::{EnumCount, EnumProperty, IntoEnumIterator, VariantArray};
use strum_macros::{EnumCount, EnumProperty, EnumIter};

use super::{
    widgets,
    bar::MenuBar,
};
use crate::{
    utils::{Size, Rect, Vec2, mem},
    render::{TextureCache, TextureFilter},
    imgui_ui::{UiSystem, UiTextureHandle, UiStaticVar},
    tile::{sets::PresetTiles, camera::CameraGlobalSettings},
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

#[derive(Default)]
pub struct ModalMenuParams {
    pub title: Option<String>,
    pub size: Option<Size>,
    pub position: Option<Vec2>,
    pub background_sprite: Option<&'static str>,
    pub start_open: bool,
}

pub struct BasicModalMenu {
    title: String,
    size: Option<Size>,
    position: Option<Vec2>,
    background_sprite: Option<UiTextureHandle>,
    is_open: bool,
    with_title_bar: bool,
    dialog: Option<Box<ModalPopupDialog>>,
}

impl BasicModalMenu {
    pub fn new(tex_cache: &mut dyn TextureCache, ui_sys: &UiSystem, params: ModalMenuParams) -> Self {
        let background_sprite = params.background_sprite.map(|sprite_path| {
            let file_path = super::ui_assets_path().join(sprite_path);
            let tex_handle = tex_cache.load_texture_with_settings(file_path.to_str().unwrap(), Some(super::ui_texture_settings()));
            ui_sys.to_ui_texture(tex_cache, tex_handle)
        });

        let with_title_bar = params.title.is_some();
        let title = params.title.unwrap_or("##ModalMenu".to_string());

        Self {
            title,
            size: params.size,
            position: params.position,
            background_sprite,
            is_open: params.start_open,
            with_title_bar,
            dialog: None,
        }
    }

    pub fn is_open(&self) -> bool {
        self.is_open
    }

    pub fn open(&mut self, sim: &mut Simulation) {
        self.is_open = true;
        self.dialog = None;
        sim.pause();
    }

    pub fn close(&mut self, sim: &mut Simulation) {
        self.is_open = false;
        self.dialog = None;
        sim.resume();
    }

    pub fn size(&self) -> Vec2 {
        self.size.unwrap_or_default().to_vec2()
    }

    pub fn draw<F>(&mut self, sim: &mut Simulation, ui_sys: &UiSystem, draw_menu_fn: F)
        where F: FnOnce(&mut Simulation)
    {
        if !self.is_open {
            return;
        }

        let ui = ui_sys.ui();
        let display_size = ui.io().display_size;
        let _font = ui.push_font(ui_sys.fonts().game_hud_large);

        let window_size = self.size();
        let mut is_open = self.is_open;

        if let Some(window_position) = self.position {
            widgets::set_next_window_pos(
                window_position,
                Vec2::zero(),
                imgui::Condition::Always
            );
        } else {
            // Center dialog window to the middle of the display if no explicit position is specified.
            widgets::set_next_window_pos(
                Vec2::new(display_size[0] * 0.5, display_size[1] * 0.5),
                Vec2::new(0.5, 0.5),
                imgui::Condition::Always
            );
        }

        if let Some(dialog) = &self.dialog {
            ui.window(&self.title)
                .flags(widgets::window_flags())
                .build(|| {
                    ui.child_window("DialogTextContainer")
                        .size(dialog.size)
                        .no_inputs()
                        .scroll_bar(false)
                        .border(true)
                        .build(|| {
                            let dialog_draw_fn = &dialog.draw_fn;
                            dialog_draw_fn(ui);
                        });

                    let mut pressed_button_index: Option<usize> = None;

                    for (index, button) in dialog.buttons.iter().enumerate() {
                        if ui.button(button.label) && pressed_button_index.is_none() {
                            pressed_button_index = Some(index);
                        }

                        // Horizontal layout (side-by-side buttons).
                        ui.same_line();
                        // Extra spacing between buttons.
                        widgets::spacing(ui, &ui.get_window_draw_list(), Vec2::new(5.0, 0.0));
                        ui.same_line();
                    }

                    if let Some(pressed_index) = pressed_button_index {
                        let button_click_fn = &dialog.buttons[pressed_index].on_click_fn;
                        button_click_fn(dialog.parent.mut_ref_cast(), sim);
                    }

                    widgets::draw_current_window_debug_rect(ui);
                });
        } else {
            let size_cond = if self.size.is_some() {
                imgui::Condition::Always
            } else {
                imgui::Condition::Never
            };

            let mut window_flags = widgets::window_flags();

            if self.with_title_bar {
                window_flags.remove(imgui::WindowFlags::NO_TITLE_BAR);
            }

            if self.background_sprite.is_some() {
                window_flags.insert(imgui::WindowFlags::NO_BACKGROUND);
            }

            ui.window(&self.title)
                .opened(&mut is_open)
                .size(window_size.to_array(), size_cond)
                .flags(window_flags)
                .build(|| {
                    if let Some(background_texture) = self.background_sprite {
                        let window_rect = Rect::new(
                            Vec2::from_array(ui.window_pos()),
                            Vec2::from_array(ui.window_size())
                        );
                        ui.get_window_draw_list()
                            .add_image(background_texture, window_rect.min.to_array(), window_rect.max.to_array())
                            .build();
                    }
                    draw_menu_fn(sim);
                    widgets::draw_current_window_debug_rect(ui);
                });
        }

        // Resume game if closed by user.
        if !is_open {
            self.close(sim);
        }
    }

    pub fn show_popup_dialog<DrawFn>(&self,
                                     parent: &dyn ModalMenu,
                                     size: [f32; 2],
                                     draw_fn: DrawFn,
                                     buttons: ModalPopupDialogButtonList)
        where DrawFn: Fn(&imgui::Ui) + 'static
    {
        // NOTE: Need to take self as immutable here so we can also receive the parent ModalMenu ref.
        // SAFETY: Parent owns the BasicMenu and therefore the ModalPopupDialog. Holding a weak parent
        // reference is safe. Mut ref cast is a necessary workaround for this.
        let mut_self = mem::mut_ref_cast(self);
        mut_self.dialog = Some(Box::new(ModalPopupDialog::new(parent, size, draw_fn, buttons)));
    }
}

// ----------------------------------------------
// ModalPopupDialog
// ----------------------------------------------

pub type ModalPopupDialogButtonList = SmallVec<[ModalPopupDialogButton; 4]>;

// Child popup dialog of a ModalMenu.
struct ModalPopupDialog {
    parent: mem::RawPtr<dyn ModalMenu>,
    size: [f32; 2],
    draw_fn: Box<dyn Fn(&imgui::Ui) + 'static>,
    buttons: ModalPopupDialogButtonList,
}

impl ModalPopupDialog {
    fn new<DrawFn>(parent: &dyn ModalMenu,
                   size: [f32; 2],
                   draw_fn: DrawFn,
                   buttons: ModalPopupDialogButtonList) -> Self
        where DrawFn: Fn(&imgui::Ui) + 'static
    {
        Self {
            parent: mem::RawPtr::from_ref(parent),
            size,
            draw_fn: Box::new(draw_fn),
            buttons,
        }
    }
}

// ----------------------------------------------
// ModalPopupDialogButton
// ----------------------------------------------

pub struct ModalPopupDialogButton {
    label: &'static str,
    on_click_fn: Box<dyn Fn(&mut dyn ModalMenu, &mut Simulation) + 'static>
}

impl ModalPopupDialogButton {
    pub fn new<OnClickFn>(label: &'static str, on_click_fn: OnClickFn) -> Self
        where OnClickFn: Fn(&mut dyn ModalMenu, &mut Simulation) + 'static
    {
        Self {
            label,
            on_click_fn: Box::new(on_click_fn),
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

    #[strum(props(Label = "Resume"))]
    Resume,
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
    pub fn new(tex_cache: &mut dyn TextureCache, ui_sys: &UiSystem, title: String, parent: &dyn MenuBar) -> Self {
        Self {
            menu: BasicModalMenu::new(
                tex_cache,
                ui_sys,
                ModalMenuParams { title: Some(title), size: Some(MODAL_WINDOW_DEFAULT_SIZE), ..Default::default() }
            ),
            parent: mem::RawPtr::from_ref(parent),
        }
    }

    fn handle_button_click(&mut self, ui_sys: &UiSystem, sim: &mut Simulation, button: MainModalMenuButton) {
        match button {
            MainModalMenuButton::NewGame  => self.on_new_game_button(ui_sys, sim),
            MainModalMenuButton::LoadGame => self.on_load_game_button(ui_sys, sim),
            MainModalMenuButton::SaveGame => self.on_save_game_button(ui_sys, sim),
            MainModalMenuButton::Settings => self.on_settings_button(ui_sys, sim),
            MainModalMenuButton::Quit     => self.on_quit_button(ui_sys, sim),
            MainModalMenuButton::Resume   => self.on_resume_button(ui_sys, sim),
        }
    }

    fn on_new_game_button(&mut self, _ui_sys: &UiSystem, sim: &mut Simulation) {
        self.parent.open_modal_menu(sim, ModalMenuId::of::<NewGameModalMenu>()).unwrap();
        // TODO: Actually this should be a "Restart Level" button instead.
        // "New Game" can be selected from main menu, when we have it.
    }

    fn on_load_game_button(&mut self, _ui_sys: &UiSystem, sim: &mut Simulation) {
        let modal_menu =
            self.parent.open_modal_menu(sim, ModalMenuId::of::<SaveGameModalMenu>()).unwrap();
        debug_assert!(modal_menu.is_open());

        let save_menu = modal_menu.as_any_mut().downcast_mut::<SaveGameModalMenu>().unwrap();
        save_menu.set_actions(SaveGameActions::Load);
    }

    fn on_save_game_button(&mut self, _ui_sys: &UiSystem, sim: &mut Simulation) {
        let modal_menu =
            self.parent.open_modal_menu(sim, ModalMenuId::of::<SaveGameModalMenu>()).unwrap();
        debug_assert!(modal_menu.is_open());

        let save_menu = modal_menu.as_any_mut().downcast_mut::<SaveGameModalMenu>().unwrap();
        save_menu.set_actions(SaveGameActions::Save);
    }

    fn on_settings_button(&mut self, _ui_sys: &UiSystem, sim: &mut Simulation) {
        self.parent.open_modal_menu(sim, ModalMenuId::of::<SettingsModalMenu>()).unwrap();
    }

    fn on_quit_button(&mut self, ui_sys: &UiSystem, _sim: &mut Simulation) {
        self.menu.show_popup_dialog(
            self,
            [self.menu.size().x, ui_sys.ui().text_line_height_with_spacing() * 3.0], // Space for roughly 3 lines of text.
            |ui| {
                ui.text("Quit Game?");
                ui.text("Any unsaved progress will be lost...");
            },
            smallvec![
                ModalPopupDialogButton::new("Quit to Main Menu", |_, _| {}), // TODO: Redirect to main menu.
                ModalPopupDialogButton::new("Exit Game", |_, _| GameLoop::get_mut().request_quit()),
                ModalPopupDialogButton::new("Cancel", |parent, sim| parent.close(sim)),
            ]
        );
    }

    fn on_resume_button(&mut self, _ui_sys: &UiSystem, sim: &mut Simulation) {
        self.parent.close_all_modal_menus(sim);
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
        let mut pressed_button: Option<MainModalMenuButton> = None;

        self.menu.draw(sim, ui_sys, |_| {
            let ui = ui_sys.ui();

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
                pressed_button = MainModalMenuButton::try_from_primitive(pressed_index).ok();
            }
        });

        if let Some(button) = pressed_button {
            self.handle_button_click(ui_sys, sim, button);
        }
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

pub struct SaveGameModalMenu {
    menu: BasicModalMenu,
    actions: SaveGameActions,
    save_file_name: String,
}

impl SaveGameModalMenu {
    pub fn new(tex_cache: &mut dyn TextureCache, ui_sys: &UiSystem) -> Self {
        let mut menu = Self {
            menu: BasicModalMenu::new(
                tex_cache,
                ui_sys,
                ModalMenuParams { title: Some("".to_string()), size: Some(MODAL_WINDOW_DEFAULT_SIZE), ..Default::default() }
            ),
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

        let mut load_game = false;
        let mut overwrite_save_game = false;
        let mut create_new_save_game = false;
        let mut should_close = false;

        self.menu.draw(sim, ui_sys, |_sim| {
            let save_files_list = GameLoop::get().save_files_list();
            let ui = ui_sys.ui();

            let container_size = [
                ui.content_region_avail()[0],
                MODAL_WINDOW_DEFAULT_SIZE.height as f32 - 100.0
            ];

            ui.set_next_item_width(container_size[0]);
            ui.input_text("##SaveFileName", &mut self.save_file_name).build();

            ui.child_window("SaveFileList")
                .size(container_size)
                .border(true)
                .build(|| {
                    let mut selected_file_index: Option<usize> = None;
                    for (index, path) in save_files_list.iter().enumerate() {
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

            if self.actions.intersects(SaveGameActions::Load) {
                if ui.button("Load Game") && !self.save_file_name.is_empty() {
                    load_game = true;
                }

                ui.same_line();
                // Extra spacing between buttons.
                widgets::spacing(ui, &ui.get_window_draw_list(), Vec2::new(5.0, 0.0));
                ui.same_line();
            }

            if self.actions.intersects(SaveGameActions::Save) {
                if ui.button("Save Game") && !self.save_file_name.is_empty() {
                    if save_files_list.iter().any(
                        |file| file.file_stem().unwrap().eq_ignore_ascii_case(&self.save_file_name))
                    {
                        overwrite_save_game = true;
                    } else {
                        create_new_save_game = true;
                    }
                }

                ui.same_line();
                // Extra spacing between buttons.
                widgets::spacing(ui, &ui.get_window_draw_list(), Vec2::new(5.0, 0.0));
                ui.same_line();
            }

            if ui.button("Cancel") {
                should_close = true;
            }
        });

        if should_close {
            self.close(sim);
        }

        if load_game {
            debug_assert!(!create_new_save_game && !overwrite_save_game);
            GameLoop::get_mut().load_save_game(&self.save_file_name);   
        } else if create_new_save_game {
            debug_assert!(!load_game && !overwrite_save_game);
            GameLoop::get_mut().save_game(&self.save_file_name);
        } else if overwrite_save_game {
            debug_assert!(!load_game && !create_new_save_game);
            // User wants to overwrite existing save file. Ask for confirmation first.
            let save_file_name = self.save_file_name.clone();
            self.menu.show_popup_dialog(
                self,
                [self.menu.size().x, ui_sys.ui().text_line_height_with_spacing() * 2.0], // Space for roughly 2 lines of text.
                |ui| {
                    ui.text("Overwrite existing save game?");
                },
                smallvec![
                    ModalPopupDialogButton::new("Yes", move |parent, sim| {
                        GameLoop::get_mut().save_game(&save_file_name);
                        parent.close(sim);
                    }),
                    ModalPopupDialogButton::new("No", |parent, sim| {
                        parent.close(sim);
                    }),
                ]
            );
        }
    }
}

// ----------------------------------------------
// SettingsModalMenu
// ----------------------------------------------

const SETTINGS_MENU_BUTTON_COUNT: usize = SettingsMenuButton::COUNT;

#[repr(usize)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, TryFromPrimitive, EnumCount, EnumProperty, EnumIter)]
enum SettingsMenuButton {
    #[strum(props(Label = "Game"))]
    Game,

    #[strum(props(Label = "Sound"))]
    Sound,

    #[strum(props(Label = "Graphics"))]
    Graphics,

    #[strum(props(Label = "Resume"))]
    Resume,
}

impl SettingsMenuButton {
    fn label(self) -> &'static str {
        self.get_str("Label").unwrap()
    }
}

pub struct SettingsModalMenu {
    menu: BasicModalMenu,
    current_selection: Option<SettingsMenuButton>,
}

impl SettingsModalMenu {
    pub fn new(tex_cache: &mut dyn TextureCache, ui_sys: &UiSystem, title: String) -> Self {
        Self {
            menu: BasicModalMenu::new(
                tex_cache,
                ui_sys,
                ModalMenuParams { title: Some(title), size: Some(MODAL_WINDOW_DEFAULT_SIZE), ..Default::default() }
            ),
            current_selection: None,
        }
    }

    fn draw_game_settings_menu(ui_sys: &UiSystem) {
        let ui = ui_sys.ui();

        let game_loop = GameLoop::get_mut();
        let camera_settings = CameraGlobalSettings::get_mut();

        let mut autosave = game_loop.is_autosave_enabled();
        if ui.checkbox("Autosave", &mut autosave) {
            game_loop.enable_autosave(autosave);
        }

        let mut camera_keyboard_zoom = !camera_settings.disable_key_shortcut_zoom;
        if ui.checkbox("Keyboard Shortcut Camera Zoom", &mut camera_keyboard_zoom) {
            camera_settings.disable_key_shortcut_zoom = !camera_keyboard_zoom;
        }

        let mut camera_mouse_scroll_zoom = !camera_settings.disable_mouse_scroll_zoom;
        if ui.checkbox("Mouse Scroll Camera Zoom", &mut camera_mouse_scroll_zoom) {
            camera_settings.disable_mouse_scroll_zoom = !camera_mouse_scroll_zoom;
        }

        let mut camera_smooth_mouse_scroll_zoom = !camera_settings.disable_smooth_mouse_scroll_zoom;
        if ui.checkbox("Smooth Mouse Scroll Camera Zoom", &mut camera_smooth_mouse_scroll_zoom) {
            camera_settings.disable_smooth_mouse_scroll_zoom = !camera_smooth_mouse_scroll_zoom;
        }
    }

    fn draw_sound_settings_menu(ui_sys: &UiSystem) {
        let ui = ui_sys.ui();

        let game_loop = GameLoop::get_mut();
        let sound_sys = game_loop.engine_mut().sound_system();

        let mut current_sound_settings = sound_sys.current_sound_settings();
        let mut sound_settings_changed = false;

        let mut volume_sliders = [
            ("SFX Volume: ", &mut current_sound_settings.sfx_master_volume),
            ("Music Volume: ", &mut current_sound_settings.music_master_volume),
            ("Ambience Volume: ", &mut current_sound_settings.ambience_master_volume),
            ("Narration Volume: ", &mut current_sound_settings.narration_master_volume),
            ("Spatial Volume: ", &mut current_sound_settings.spatial_master_volume),
        ];

        let mut longest_label: f32 = 0.0;
        for (label, _) in &volume_sliders {
            let width = ui.calc_text_size(label)[0];
            if width > longest_label {
                longest_label = width;
            }
        }
        longest_label += 5.0; // Extra padding between the label & slider.

        let mut draw_volume_slider = |label: &str, master_volume: &mut f32| {
            ui.text(label);
            ui.same_line();
            ui.set_next_item_width(-1.0);
            ui.set_cursor_pos([longest_label, ui.cursor_pos()[1]]);
            let mut volume = (*master_volume * 100.0) as u32;
            if ui.slider_config(format!("##{label}"), 0, 100)
                .flags(imgui::SliderFlags::ALWAYS_CLAMP | imgui::SliderFlags::NO_INPUT)
                .build(&mut volume)    
            {
                *master_volume = volume.clamp(0, 100) as f32 / 100.0;
                sound_settings_changed = true;
            }
        };

        for (label, master_volume) in &mut volume_sliders {
            draw_volume_slider(label, master_volume);
        }

        if sound_settings_changed {
            sound_sys.change_sound_settings(current_sound_settings);
        }
    }

    fn draw_graphics_settings_menu(ui_sys: &UiSystem) {
        let ui = ui_sys.ui();

        let game_loop = GameLoop::get_mut();
        let tex_cache = game_loop.engine_mut().texture_cache();

        let mut current_texture_settings = tex_cache.current_texture_settings();
        let mut texture_settings_changed = false;

        if ui.checkbox("Use Texture Mipmaps", &mut current_texture_settings.gen_mipmaps) {
            texture_settings_changed = true;
        }

        ui.text("Texture Filtering: ");
        ui.same_line();
        ui.set_next_item_width(-1.0);
        let mut current_texture_filter_index = current_texture_settings.filter as usize;
        if ui.combo("##TextureFiltering",
                    &mut current_texture_filter_index,
                    TextureFilter::VARIANTS,
                    |v| { v.to_string().into() })
        {
            current_texture_settings.filter = TextureFilter::try_from_primitive(current_texture_filter_index as u32).unwrap();
            texture_settings_changed = true;
        }

        if texture_settings_changed {
            tex_cache.change_texture_settings(current_texture_settings);
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
        self.current_selection = None;
    }

    fn close(&mut self, sim: &mut Simulation) {
        self.menu.close(sim);
        self.current_selection = None;
    }

    fn draw(&mut self, sim: &mut Simulation, ui_sys: &UiSystem) {
        let mut ok_pressed = false;
        let mut cancel_pressed = false;
        let mut should_close = false;

        self.menu.draw(sim, ui_sys, |_sim| {
            let ui = ui_sys.ui();

            // A settings button is selected, draw its sub-menu:
            if let Some(selection) = self.current_selection {
                type DrawMenuFn = fn(&UiSystem);

                let (label, draw_fn): (&str, DrawMenuFn) = match selection {
                    SettingsMenuButton::Game => ("Game Settings", Self::draw_game_settings_menu),
                    SettingsMenuButton::Sound => ("Sound Settings", Self::draw_sound_settings_menu),
                    SettingsMenuButton::Graphics => ("Graphics Settings", Self::draw_graphics_settings_menu),
                    SettingsMenuButton::Resume => {
                        should_close = true;
                        return;
                    }
                };

                // Frame the settings inside a child container window.
                let container_size = [
                    ui.content_region_avail()[0],
                    MODAL_WINDOW_DEFAULT_SIZE.height as f32 - 100.0
                ];

                ui.text(label);
                ui.child_window("SettingsList")
                    .size(container_size)
                    .border(true)
                    .build(|| {
                        draw_fn(ui_sys);
                    });

                ok_pressed |= ui.button("Ok");

                ui.same_line();
                // Extra spacing between buttons.
                widgets::spacing(ui, &ui.get_window_draw_list(), Vec2::new(5.0, 0.0));
                ui.same_line();

                cancel_pressed |= ui.button("Cancel");
            } else {
                // Draw main settings menu:
                let mut labels = ArrayVec::<&str, SETTINGS_MENU_BUTTON_COUNT>::new();
                for button in SettingsMenuButton::iter() {
                    labels.push(button.label());
                }

                let pressed_button_index = widgets::draw_centered_button_group(
                    ui,
                    &ui.get_window_draw_list(),
                    &labels,
                    Some(MODAL_BUTTON_DEFAULT_SIZE)
                );

                if let Some(pressed_index) = pressed_button_index {
                    self.current_selection = SettingsMenuButton::try_from_primitive(pressed_index).ok();
                }
            }
        });

        if ok_pressed || cancel_pressed {
            self.current_selection = None; // Go back to main settings.
        }

        if should_close {
            self.close(sim);
        }
    }
}

// ----------------------------------------------
// NewGameModalMenu
// ----------------------------------------------

pub struct NewGameModalMenu {
    menu: BasicModalMenu,
    new_map_size: Size,
}

impl NewGameModalMenu {
    pub fn new(tex_cache: &mut dyn TextureCache, ui_sys: &UiSystem, title: String) -> Self {
        Self {
            menu: BasicModalMenu::new(
                tex_cache,
                ui_sys,
                ModalMenuParams { title: Some(title), size: Some(MODAL_WINDOW_DEFAULT_SIZE), ..Default::default() }
            ),
            new_map_size: Size::new(64, 64),
        }
    }

    fn calc_centered_group_start(ui: &imgui::Ui, group_width: f32) -> Vec2 {
        let avail = ui.content_region_avail();
        let avail_width = avail[0];
        let avail_height = avail[1];

        let group_height =
            ui.text_line_height_with_spacing()     // "Map Size"
            + ui.frame_height_with_spacing() * 2.0 // width + height inputs
            + 8.0                                  // widgets::spacing
            + ui.text_line_height_with_spacing()   // "Terrain Kind"
            + ui.frame_height_with_spacing()       // combo
            + 8.0                                  // widgets::spacing
            + ui.frame_height_with_spacing();      // button

        let start_x = ((avail_width  - group_width)  * 0.5).max(0.0);
        let start_y = ((avail_height - group_height) * 0.5).max(0.0);

        Vec2::new(start_x, start_y)
    }
}

impl ModalMenu for NewGameModalMenu {
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
        let mut should_close = false;

        self.menu.draw(sim, ui_sys, |_sim| {
            let ui = ui_sys.ui();

            const GROUP_WIDTH: f32 = 210.0;
            let group_start = Self::calc_centered_group_start(ui, GROUP_WIDTH);

            ui.set_cursor_pos([group_start.x, group_start.y]);
            ui.text("Map Size:");

            // NOTE: Use "Height" here to keep both sizes even (it's the longest label).
            let map_size_label_width = ui.calc_text_size("Height ")[0];

            // Left-hand-side labels.
            ui.set_cursor_pos([group_start.x, ui.cursor_pos()[1]]);
            ui.text("Width ");
            ui.same_line();
            ui.set_cursor_pos([group_start.x + map_size_label_width, ui.cursor_pos()[1]]);
            ui.set_next_item_width(GROUP_WIDTH - map_size_label_width);
            let w_edited = ui.input_int("##Width", &mut self.new_map_size.width).step(32).build();

            ui.set_cursor_pos([group_start.x, ui.cursor_pos()[1]]);
            ui.text("Height ");
            ui.same_line();
            ui.set_cursor_pos([group_start.x + map_size_label_width, ui.cursor_pos()[1]]);
            ui.set_next_item_width(GROUP_WIDTH - map_size_label_width);
            let h_edited = ui.input_int("##Height", &mut self.new_map_size.height).step(32).build();

            if w_edited || h_edited {
                self.new_map_size.width = self.new_map_size.width.clamp(32, 256);
                self.new_map_size.height = self.new_map_size.height.clamp(32, 256);
            }

            widgets::spacing(ui, &ui.get_window_draw_list(), Vec2::new(0.0, 8.0));

            ui.set_cursor_pos([group_start.x, ui.cursor_pos()[1]]);
            ui.text("Terrain Kind:");

            const TILE_KIND_NAMES: [&str; 3] = ["Grass", "Dirt", "Water"];
            const TILE_KIND_HASHES: [PresetTiles; 3] = [PresetTiles::Grass, PresetTiles::Dirt, PresetTiles::Water];
            static CURRENT_TILE_KIND: UiStaticVar<usize> = UiStaticVar::new(0);

            ui.set_cursor_pos([group_start.x, ui.cursor_pos()[1]]);
            ui.set_next_item_width(GROUP_WIDTH);
            ui.combo_simple_string("##TileKind", CURRENT_TILE_KIND.as_mut(), &TILE_KIND_NAMES);

            widgets::spacing(ui, &ui.get_window_draw_list(), Vec2::new(0.0, 8.0));

            ui.set_cursor_pos([group_start.x, ui.cursor_pos()[1]]);
            if ui.button("Start New Game") {
                let selected_tile_kind = TILE_KIND_HASHES[*CURRENT_TILE_KIND];
                let opt_tile_def = selected_tile_kind.find_tile_def();
                GameLoop::get_mut().reset_session(opt_tile_def, Some(self.new_map_size));
                should_close = true;
            }

            ui.same_line();
            // Extra spacing between buttons.
            widgets::spacing(ui, &ui.get_window_draw_list(), Vec2::new(5.0, 0.0));
            ui.same_line();

            if ui.button("Cancel") {
                should_close = true;
            }
        });

        // Close modal window if user clicked the new game or cancel button.
        if should_close {
            self.close(sim);
        }
    }
}
