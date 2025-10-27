use std::borrow::Cow;
use smallvec::SmallVec;

use crate::{
    debug::{self, popups::PopupMessages},
    engine::time::Seconds,
    game::sim::resources::ResourceKind,
    imgui_ui::UiSystem,
    tile::Tile,
    utils::{
        self,
        coords::{CellRange, WorldToScreenTransform},
        Color,
    },
};

// ----------------------------------------------
// GameObjectDebugVar
// ----------------------------------------------

pub struct GameObjectDebugVar<'a> {
    name: &'static str,
    value: GameObjectDebugVarRef<'a>,
}

pub enum GameObjectDebugVarRef<'a> {
    Bool(&'a mut bool),
    I32(&'a mut i32),
    F32(&'a mut f32),
}

pub trait IntoGameObjectDebugVar<'a> {
    fn into_debug_var(self) -> GameObjectDebugVarRef<'a>;
}

impl<'a> IntoGameObjectDebugVar<'a> for &'a mut bool {
    fn into_debug_var(self) -> GameObjectDebugVarRef<'a> {
        GameObjectDebugVarRef::Bool(self)
    }
}

impl<'a> IntoGameObjectDebugVar<'a> for &'a mut i32 {
    fn into_debug_var(self) -> GameObjectDebugVarRef<'a> {
        GameObjectDebugVarRef::I32(self)
    }
}

impl<'a> IntoGameObjectDebugVar<'a> for &'a mut f32 {
    fn into_debug_var(self) -> GameObjectDebugVarRef<'a> {
        GameObjectDebugVarRef::F32(self)
    }
}

impl<'a> GameObjectDebugVar<'a> {
    pub fn new(name: &'static str, value: impl IntoGameObjectDebugVar<'a>) -> Self {
        Self { name, value: value.into_debug_var() }
    }
}

// ----------------------------------------------
// GameObjectDebugPopups
// ----------------------------------------------

#[derive(Clone)]
pub struct GameObjectDebugPopups {
    messages: PopupMessages,
    pub show: bool,
}

impl Default for GameObjectDebugPopups {
    fn default() -> Self {
        Self { messages: PopupMessages::default(), show: debug::show_popup_messages() }
    }
}

impl GameObjectDebugPopups {
    fn clear(&mut self) {
        self.messages.clear();
    }
}

// ----------------------------------------------
// GameObjectDebugOptions
// ----------------------------------------------

pub trait GameObjectDebugOptions {
    fn get_popups(&mut self) -> &mut GameObjectDebugPopups;
    fn get_vars(&mut self) -> SmallVec<[GameObjectDebugVar; 16]>;

    #[inline]
    fn show_popups(&mut self) -> bool {
        self.get_popups().show && debug::show_popup_messages()
    }

    #[inline]
    fn set_show_popups(&mut self, show: bool) -> bool {
        let prev_value = self.get_popups().show;
        self.get_popups().show = show;
        prev_value
    }

    #[inline]
    fn clear_popups(&mut self) {
        self.get_popups().clear();
    }

    fn draw_debug_ui(&mut self, ui_sys: &UiSystem) {
        let mut vars = self.get_vars();
        if !vars.is_empty() {
            let ui = ui_sys.builder();
            if ui.collapsing_header("Debug Options##_game_obj_debug_opts",
                                    imgui::TreeNodeFlags::empty())
            {
                for var in &mut vars {
                    match &mut var.value {
                        GameObjectDebugVarRef::Bool(value) => {
                            ui.checkbox(utils::snake_case_to_title::<64>(var.name), value);
                        }
                        GameObjectDebugVarRef::I32(value) => {
                            ui.input_int(utils::snake_case_to_title::<64>(var.name), value)
                              .step(1)
                              .build();
                        }
                        GameObjectDebugVarRef::F32(value) => {
                            ui.input_float(utils::snake_case_to_title::<64>(var.name), value)
                              .display_format("%.2f")
                              .step(1.0)
                              .build();
                        }
                    }
                }
            }
        }
    }

    fn draw_popup_messages(&mut self,
                           tile: &Tile,
                           ui_sys: &UiSystem,
                           transform: WorldToScreenTransform,
                           visible_range: CellRange,
                           delta_time_secs: Seconds) {
        if self.show_popups() && visible_range.contains(tile.base_cell()) {
            const LIFETIME_MULTIPLIER: f32 = 3.0;
            const SCROLL_DIST: f32 = 5.0;
            const SCROLL_SPEED: f32 = 12.0;
            const START_BG_ALPHA: f32 = 0.6;

            let popups = self.get_popups();
            let screen_pos = tile.screen_rect(transform, true).center();

            popups.messages.update(LIFETIME_MULTIPLIER, delta_time_secs);
            popups.messages.draw(ui_sys, screen_pos, SCROLL_DIST, SCROLL_SPEED, START_BG_ALPHA);
        }
    }

    #[inline]
    fn popup_msg_string(&mut self, text: Cow<'static, str>) {
        if self.show_popups() {
            const LIFETIME: Seconds = 9.0;
            self.get_popups().messages.push_with_args(LIFETIME, Color::default(), text);
        }
    }

    #[inline]
    fn popup_msg_color_string(&mut self, color: Color, text: Cow<'static, str>) {
        if self.show_popups() {
            const LIFETIME: Seconds = 9.0;
            self.get_popups().messages.push_with_args(LIFETIME, color, text);
        }
    }

    #[inline]
    fn log_resources_gained(&mut self, kind: ResourceKind, count: u32) {
        if self.show_popups() && !kind.is_empty() && count != 0 {
            self.popup_msg_color_string(Color::green(), format!("+{count} {kind}").into());
        }
    }

    #[inline]
    fn log_resources_lost(&mut self, kind: ResourceKind, count: u32) {
        if self.show_popups() && !kind.is_empty() && count != 0 {
            self.popup_msg_color_string(Color::red(), format!("-{count} {kind}").into());
        }
    }
}

pub trait GameObjectDebugOptionsExt {
    fn popup_msg<T>(&mut self, text: T)
        where T: Into<Cow<'static, str>>;

    fn popup_msg_color<T>(&mut self, color: Color, text: T)
        where T: Into<Cow<'static, str>>;
}

// ----------------------------------------------
// Macro: game_object_debug_options
// ----------------------------------------------

#[macro_export]
macro_rules! game_object_debug_options {
    (
        $struct_name:ident,
        $($field_name:ident : $field_type:ty),* $(,)?
    ) => {
        use paste::paste;
        use std::borrow::Cow;
        use $crate::game::world::debug::{
            GameObjectDebugVar,
            GameObjectDebugPopups,
            GameObjectDebugOptions,
            GameObjectDebugOptionsExt
        };

        paste! {
            #[derive(Clone, Default)]
            struct $struct_name {
                popups: GameObjectDebugPopups,
                $(
                    [<opt_ $field_name>] : $field_type,
                )*
            }

            impl $struct_name {
                $(
                    #[must_use]
                    #[inline(always)]
                    fn $field_name(&self) -> $field_type {
                        self.[<opt_ $field_name>]
                    }
                )*
            }

            impl GameObjectDebugOptions for $struct_name {
                #[inline]
                fn get_popups(&mut self) -> &mut GameObjectDebugPopups {
                    &mut self.popups
                }

                #[inline]
                fn get_vars(&mut self) -> smallvec::SmallVec<[GameObjectDebugVar; 16]> {
                    let mut vars = smallvec::smallvec![
                        $(
                            GameObjectDebugVar::new(stringify!($field_name), &mut self.[<opt_ $field_name>]),
                        )*
                    ];
                    vars.push(GameObjectDebugVar::new("show_popup_messages", &mut self.popups.show));
                    vars
                }
            }

            impl GameObjectDebugOptionsExt for $struct_name {
                #[inline]
                fn popup_msg<T>(&mut self, text: T)
                    where T: Into<Cow<'static, str>>
                {
                    GameObjectDebugOptions::popup_msg_string(self, text.into());
                }

                #[inline]
                fn popup_msg_color<T>(&mut self, color: Color, text: T)
                    where T: Into<Cow<'static, str>>
                {
                    GameObjectDebugOptions::popup_msg_color_string(self, color, text.into());
                }
            }
        }
    };
}
