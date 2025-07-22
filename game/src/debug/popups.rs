use std::borrow::Cow;
use smallvec::SmallVec;
use rand::{self, Rng};

use crate::{
    utils::{self, Color, Vec2, Seconds},
    imgui_ui::UiSystem
};

// ----------------------------------------------
// PopupMessages
// ----------------------------------------------

type PopupMessageList = SmallVec<[PopupMessage; 6]>;

#[derive(Default)]
pub struct PopupMessages {
    list: Option<Box<PopupMessageList>>, // Initialized on demand.
}

impl PopupMessages {
    #[inline]
    pub fn push_with_args(&mut self, lifetime_secs: Seconds, color: Color, text: impl Into<Cow<'static, str>>) {
        self.push_message(PopupMessage::with_random_offset(Some(lifetime_secs), color, text));
    }

    #[inline]
    pub fn push_message(&mut self, message: PopupMessage) {
        let list = self.get_or_init_list();
        list.push(message);
    }

    pub fn for_each<F>(&self, visitor_fn: F)
        where F: Fn(&PopupMessage)
    {
        // Don't lazily initialize the list here.
        if let Some(list) = self.try_get_list() {
            for message in list {
                visitor_fn(message);
            }
        }
    }

    pub fn update(&mut self, lifetime_multiplier: f32, delta_time_secs: Seconds) {
        // Don't lazily initialize the list here.
        if let Some(list) = self.try_get_list_mut() {
            let time_elapsed = lifetime_multiplier * delta_time_secs;
            let mut expired_indices = SmallVec::<[usize; 32]>::new();

            for (index, message) in list.iter_mut().enumerate() {
                // Update and expire messages that have a lifetime.
                if let Some(lifetime) = &mut message.lifetime {
                    let time_left = &mut lifetime.1;
                    *time_left -= time_elapsed;
                    if *time_left <= 0.0 {
                        expired_indices.push(index);
                    }
                }
            }

            // Remove in reverse order so any vector shuffles will not invalidate the remaining indices.
            for expired_index in expired_indices.iter().rev() {
                list.swap_remove(*expired_index);
            }
        }
    }

    pub fn draw(&self,
                ui_sys: &UiSystem,
                screen_pos: Vec2,
                scroll_dist: f32,
                scroll_speed: f32,
                start_bg_alpha: f32) {
        // Don't lazily initialize the list here.
        if let Some(list) = self.try_get_list() {
            for message in list {
                draw_popup_message(ui_sys, message, screen_pos, scroll_dist, scroll_speed, start_bg_alpha);
            }
        }
    }

    // ----------------------
    // Internal:
    // ----------------------

    #[inline]
    fn get_or_init_list(&mut self) -> &mut PopupMessageList {
        if self.list.is_none() {
            self.list = Some(Box::new(PopupMessageList::new()));
        }
        self.list.as_deref_mut().unwrap()
    }

    #[inline]
    fn try_get_list(&self) -> Option<&PopupMessageList> {
        match &self.list {
            Some(list) => Some(list.as_ref()),
            None => None,
        }
    }

    #[inline]
    fn try_get_list_mut(&mut self) -> Option<&mut PopupMessageList> {
        match &mut self.list {
            Some(list) => Some(list.as_mut()),
            None => None,
        }
    }
}

// ----------------------------------------------
// PopupMessage
// ----------------------------------------------

pub struct PopupMessage {
    // One lifetime reaches zero the message expires.
    pub lifetime: Option<(Seconds, Seconds)>, // (lifetime, time_left)
    pub color: Color,
    pub offset: Vec2,

    // Hold either a ref to a static literal string or an owned string
    // (like from the return of format!()), without performing unnecessary copies.
    pub text: Cow<'static, str>,
}

impl PopupMessage {
    pub fn new(lifetime_secs: Option<Seconds>, color: Color, text: impl Into<Cow<'static, str>>) -> Self {
        let lifetime = lifetime_secs.map(|seconds| (seconds, seconds));
        Self {
            lifetime,
            color,
            offset: Vec2::zero(),
            text: text.into(),
        }
    }

    pub fn with_random_offset(lifetime_secs: Option<Seconds>, color: Color, text: impl Into<Cow<'static, str>>) -> Self {
        let mut message = PopupMessage::new(lifetime_secs, color, text);

        // Add a random offset to the message so when we render multiple popups over
        // a building in the same frame they won't all overlap and cover each other.
        let mut rng = rand::rng();
        let offset_x = rng.random_range(1..=40);
        let offset_y = rng.random_range(1..=40);
        message.offset = Vec2::new(offset_x as f32, offset_y as f32);

        message
    }
}

// ----------------------------------------------
// Internal helpers
// ----------------------------------------------

fn draw_text_with_bg(ui: &imgui::Ui,
                     text: &str,
                     pos: Vec2,
                     text_color: Color,
                     bg_color: Color,
                     padding: f32) {

    let draw_list = ui.get_background_draw_list();

    // Measure text size
    let text_size = ui.calc_text_size(text);

    // Compute padded rectangle
    let p_min = [
        pos.x - padding,
        pos.y - padding,
    ];
    let p_max = [
        pos.x + text_size[0] + padding,
        pos.y + text_size[1] + padding,
    ];

    // Convert colors
    let bg_col_u32   = imgui::ImColor32::from_rgba_f32s(bg_color.r, bg_color.g, bg_color.b, bg_color.a);
    let text_col_u32 = imgui::ImColor32::from_rgba_f32s(text_color.r, text_color.g, text_color.b, text_color.a);

    // Draw background rectangle
    draw_list
        .add_rect(p_min, p_max, bg_col_u32)
        .filled(true)
        .build();

    // Draw text on top
    draw_list.add_text([ pos.x, pos.y ], text_col_u32, text);
}

fn calc_message_lifetime_percentage(lifetime_secs: Seconds,
                                    time_left_secs: Seconds) -> f32 {
    if lifetime_secs <= 0.0 {
        return 0.0;
    }
    (time_left_secs / lifetime_secs).clamp(0.0, 1.0)
}

fn draw_popup_message(ui_sys: &UiSystem,
                      message: &PopupMessage,
                      screen_pos: Vec2,
                      scroll_dist: f32,
                      scroll_speed: f32,
                      start_bg_alpha: f32) {

    let ui = ui_sys.builder();

    let lifetime_percentage = match message.lifetime {
        Some(lifetime) => calc_message_lifetime_percentage(lifetime.0, lifetime.1),
        None => 0.0, // default
    };

    let (text_scroll, bg_alpha) = {
        if lifetime_percentage != 0.0 {
            (
                scroll_dist * scroll_speed * (1.0 - lifetime_percentage),
                utils::lerp(0.0, start_bg_alpha, lifetime_percentage)
            )
        } else {
            (
                0.0, // no text scroll
                start_bg_alpha // default bg_alpha
            )
        }
    };

    let pos = Vec2::new(
        screen_pos.x - message.offset.x,
        screen_pos.y - message.offset.y - text_scroll
    );

    let text_color = message.color;
    let bg_color = Color::new(0.1, 0.1, 0.1, bg_alpha);

    draw_text_with_bg(ui, &message.text, pos, text_color, bg_color, 4.0);
}
