use arrayvec::{ArrayVec, ArrayString};
use strum::EnumCount;
use strum_macros::EnumCount;

use super::TileInspectorMenuWeakMut;
use crate::{
    format_fixed_string,
    utils::Vec2,
    ui::{UiFontScale, widgets::*},
    tile::{TileKind, sets::TileIconSprite},
    game::menu::TEXT_BUTTON_HOVERED_SPRITE,
};

// ----------------------------------------------
// Constants
// ----------------------------------------------

const INSPECTOR_MENU_BACKGROUND_SPRITE: &str = "misc/square_page_bg.png";

const INSPECTOR_MENU_FLAGS: UiMenuFlags =
    UiMenuFlags::from_bits_retain(
        UiMenuFlags::PauseSimIfOpen.bits()
        | UiMenuFlags::AlignCenter.bits()
        | UiMenuFlags::Modal.bits()
        | UiMenuFlags::CloseModalOnEscape.bits()
    );

const INSPECTOR_HEADING_FONT_SCALE: UiFontScale = UiFontScale(1.5);
const INSPECTOR_SUBHEADING_FONT_SCALE: UiFontScale = UiFontScale(1.0);
const INSPECTOR_BODY_FONT_SCALE: UiFontScale = UiFontScale(1.0);

const INSPECTOR_BODY_TEXT_MAX_LINES: usize = 10;
const INSPECTOR_BODY_TEXT_MAX_LEN: usize = 1024;
const INSPECTOR_FMT_STR_MAX_LEN: usize = 128;

#[repr(usize)]
#[derive(Copy, Clone, EnumCount)]
enum InspectorHeadingIdx {
    Title,
    Subheading0,
    Subheading1,
    Subheading2,
    Subheading3,
}

const INSPECTOR_SUBHEADING_COUNT: usize = InspectorHeadingIdx::COUNT - 1; // Skip Title.

// ----------------------------------------------
// InspectorMenuHeadings
// ----------------------------------------------

pub struct InspectorMenuHeadings<'a> {
    key_vals: ArrayVec<(&'a str, ArrayString<INSPECTOR_FMT_STR_MAX_LEN>), INSPECTOR_SUBHEADING_COUNT>,
}

impl<'a> InspectorMenuHeadings<'a> {
    pub const FMT_STR_MAX_LEN: usize = INSPECTOR_FMT_STR_MAX_LEN;

    pub fn new() -> Self {
        Self { key_vals: ArrayVec::new() }
    }

    pub fn add(&mut self, key: &'a str, val: ArrayString<INSPECTOR_FMT_STR_MAX_LEN>) {
        self.key_vals.push((key, val));
    }
}

macro_rules! add_heading {
    (&mut $headings:ident, $key:expr, $($arg:tt)*) => {
        $headings.add($key, crate::format_fixed_string!({ InspectorMenuHeadings::FMT_STR_MAX_LEN }, $($arg)*))
    };
}

pub(super) use add_heading;

// ----------------------------------------------
// InspectorMenuBody
// ----------------------------------------------

pub struct InspectorMenuBody {
    lines: ArrayString<INSPECTOR_BODY_TEXT_MAX_LEN>,
}

impl InspectorMenuBody {
    pub const FMT_STR_MAX_LEN: usize = INSPECTOR_FMT_STR_MAX_LEN;

    pub fn new() -> Self {
        Self { lines: ArrayString::new() }
    }

    pub fn add_line(&mut self, text: &str) {
        self.lines.push_str(text);
        self.lines.push('\n');
    }

    pub fn add_str(&mut self, text: &str) {
        self.lines.push_str(text);
    }

    pub fn append(&mut self, other: &Self) {
        self.lines.push_str(&other.lines);
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

macro_rules! add_body_line {
    (&mut $body:ident, $($arg:tt)*) => {
        $body.add_line(&crate::format_fixed_string!({ InspectorMenuBody::FMT_STR_MAX_LEN }, $($arg)*))
    };
}

pub(super) use add_body_line;

macro_rules! add_body_str {
    (&mut $body:ident, $($arg:tt)*) => {
        $body.add_str(&crate::format_fixed_string!({ InspectorMenuBody::FMT_STR_MAX_LEN }, $($arg)*))
    };
}

pub(super) use add_body_str;

// ----------------------------------------------
// InspectorMenuRenderer
// ----------------------------------------------

// Handles menu layout and rendering.
pub struct InspectorMenuRenderer {
    menu: UiMenuRcMut,

    // Indices within `icon_and_heading_group`.
    icon_index: UiWidgetGroupWidgetIndex,
    heading_index: UiWidgetGroupWidgetIndex,

    // Indices withing `self.menu`.
    icon_and_heading_group_index: UiMenuWidgetIndex,
    body_text_index: UiMenuWidgetIndex,
    button_group_index: UiMenuWidgetIndex,
}

impl InspectorMenuRenderer {
    #[inline]
    pub fn menu(&mut self) -> &mut UiMenu {
        &mut self.menu
    }

    pub fn set_title(&mut self, text: &str) {
        let heading = self.find_heading();
        heading.set_line_string(InspectorHeadingIdx::Title as usize, text);
    }

    pub fn set_headings(&mut self, headings: &InspectorMenuHeadings) {
        self.set_heading_pairs(&headings.key_vals);
    }

    pub fn set_heading_pairs<K, V>(&mut self, key_vals: &[(K, V)])
        where K: std::fmt::Display,
              V: std::fmt::Display
    {
        self.clear_headings();
        let heading = self.find_heading();

        for (i, (key, val)) in key_vals.iter().enumerate() {
            let index = i + 1; // Skip over InspectorHeadingIdx::Title.
            debug_assert!(index < InspectorHeadingIdx::COUNT, "Invalid subheading index: {index}");
            let text = format_fixed_string!(INSPECTOR_FMT_STR_MAX_LEN, "{}: {}", key, val);
            heading.set_line_string(index, &text);
        }
    }

    pub fn clear_headings(&mut self) {
        let heading = self.find_heading();
        let lines = heading.lines_mut();

        // Clear all subheading lines except for InspectorHeadingIdx::Title.
        for line in lines.iter_mut().take(InspectorHeadingIdx::COUNT).skip(1) {
            line.string.clear();
        }
    }

    pub fn set_body(&mut self, body: &InspectorMenuBody) {
        self.set_body_text(&body.lines);
    }

    pub fn set_body_text(&mut self, text: &str) {
        let body_text = self.find_body_text();
        body_text.clear_all_lines();

        for (i, line) in text.split('\n').enumerate() {
            body_text.set_line_string(i, line);
        }
    }

    pub fn set_icon(&mut self, context: &mut UiWidgetContext, icon_sprite: TileIconSprite, tile_kind: TileKind) {
        let icon = self.find_icon();

        let sprite = context.ui_sys.to_ui_texture(context.tex_cache, icon_sprite.tex_info.texture);
        icon.set_sprite(sprite);

        let tex_coords = icon_sprite.tex_info.coords;
        icon.set_tex_coords(tex_coords);

        // Scale proportionally to desired min/max icon size:
        let (min_size, max_size) = {
            if tile_kind.intersects(TileKind::Building) {
                (128.0, 192.0)
            } else {
                (64.0, 140.0)
            }
        };

        let size = icon_sprite.size.to_vec2();

        let min_scale_x = min_size / size.x;
        let max_scale_x = max_size / size.x;

        let min_scale_y = min_size / size.y;
        let max_scale_y = max_size / size.y;

        let mut min_scale = min_scale_x.max(min_scale_y);
        let mut max_scale = max_scale_x.min(max_scale_y);

        if min_scale > max_scale {
            std::mem::swap(&mut min_scale, &mut max_scale);
        }

        let scale = 1.0_f32.clamp(min_scale, max_scale);
        let scaled_size = size * scale;

        icon.set_size(scaled_size);
    }

    pub fn new(context: &mut UiWidgetContext,
               tile_inspector_menu_weak_ref: &TileInspectorMenuWeakMut,
               menu_name: &str) -> Self
    {
        let icon = UiSpriteIcon::new(
            context,
            UiSpriteIconParams {
                size: Vec2::one(), // placeholder
                outline: true,
                clip_to_parent_menu: true,
                ..Default::default()
            }
        );

        const HEADING:    UiText = UiText { string: String::new(), font_scale: INSPECTOR_HEADING_FONT_SCALE,    color: None };
        const SUBHEADING: UiText = UiText { string: String::new(), font_scale: INSPECTOR_SUBHEADING_FONT_SCALE, color: None };

        let heading = UiMenuHeading::new(
            context,
            UiMenuHeadingParams {
                lines: vec![
                    HEADING,    // heading/title placeholder
                    SUBHEADING, // subheading 0 placeholder
                    SUBHEADING, // subheading 1 placeholder
                    SUBHEADING, // subheading 2 placeholder
                    SUBHEADING, // subheading 3 placeholder
                ],
                center_vertically: false,
                center_horizontally: false,
                ..Default::default()
            }
        );

        let mut icon_and_heading_group = UiWidgetGroup::new(
            context,
            UiWidgetGroupParams {
                widget_spacing: Vec2::new(20.0, 8.0),
                center_vertically: false,
                center_horizontally: true,
                stack_vertically: false,
                ..Default::default()
            }
        );

        let icon_index = icon_and_heading_group.add_widget(icon);
        let heading_index = icon_and_heading_group.add_widget(heading);

        let close_button_inspector_menu_weak_ref = tile_inspector_menu_weak_ref.clone();
        let close_button = UiTextButton::new(
            context,
            UiTextButtonParams {
                label: "Close".into(),
                size: UiTextButtonSize::Normal,
                hover: Some(TEXT_BUTTON_HOVERED_SPRITE),
                enabled: true,
                on_pressed: UiTextButtonPressed::with_closure(move |_, context| {
                    let mut inspector_menu = close_button_inspector_menu_weak_ref.upgrade().unwrap();
                    inspector_menu.close_inspector(context);
                }),
                ..Default::default()
            }
        );

        let mut button_group = UiWidgetGroup::new(
            context,
            UiWidgetGroupParams {
                center_vertically: false,
                center_horizontally: true,
                stack_vertically: false,
                ..Default::default()
            }
        );

        button_group.add_widget(close_button);

        let body_text = UiMenuHeading::new(
            context,
            UiMenuHeadingParams {
                // placeholder text
                lines: vec![
                    UiText { string: String::new(), font_scale: INSPECTOR_BODY_FONT_SCALE, color: None };
                    INSPECTOR_BODY_TEXT_MAX_LINES
                ],
                center_vertically: false,
                center_horizontally: true,
                ..Default::default()
            }
        );

        let separator = UiSeparator::new(
            context,
            UiSeparatorParams {
                thickness: Some(10.0),
                ..Default::default()
            }
        );

        let mut menu = UiMenu::new(
            context,
            UiMenuParams {
                label: Some(menu_name.into()),
                flags: INSPECTOR_MENU_FLAGS,
                size: Some(Self::calc_menu_size(context)),
                background: Some(INSPECTOR_MENU_BACKGROUND_SPRITE),
                widget_spacing: Some(Vec2::new(0.0, 10.0)),
                ..Default::default()
            }
        );

        menu.add_widget(separator.clone());
        let icon_and_heading_group_index = menu.add_widget(icon_and_heading_group);

        menu.add_widget(separator.clone());
        let body_text_index = menu.add_widget(body_text);

        menu.add_widget(separator.clone());
        let button_group_index = menu.add_widget(button_group);

        Self {
            menu,
            icon_index,
            heading_index,
            icon_and_heading_group_index,
            body_text_index,
            button_group_index,
        }
    }

    // ----------------------
    // Internal helpers:
    // ----------------------

    fn find_icon_and_heading_group(&mut self) -> &mut UiWidgetGroup {
        self.menu.widget_as_mut::<UiWidgetGroup>(self.icon_and_heading_group_index).unwrap()
    }

    fn find_icon(&mut self) -> &mut UiSpriteIcon {
        let icon_index = self.icon_index;
        let icon_and_heading_group = self.find_icon_and_heading_group();
        icon_and_heading_group.widget_as_mut::<UiSpriteIcon>(icon_index).unwrap()
    }

    fn find_heading(&mut self) -> &mut UiMenuHeading {
        let heading_index = self.heading_index;
        let icon_and_heading_group = self.find_icon_and_heading_group();
        icon_and_heading_group.widget_as_mut::<UiMenuHeading>(heading_index).unwrap()
    }

    fn find_body_text(&mut self) -> &mut UiMenuHeading {
        let body_text_index = self.body_text_index;
        self.menu.widget_as_mut::<UiMenuHeading>(body_text_index).unwrap()
    }

    fn calc_menu_size(context: &UiWidgetContext) -> Vec2 {
        Vec2::new(
            context.viewport_size.width  as f32 * 0.5 - 120.0,
            context.viewport_size.height as f32 * 0.5
        )
    }
}
