use arrayvec::{ArrayString, ArrayVec};
use common::{Vec2, format_fixed_string};
use engine::ui::{self, UiFontScale, UiString, text::UiTextCategory, widgets::*};
use strum::EnumCount;

use super::{TileInspector, TileInspectorMenuWeakMut};
use crate::{
    menu::skin,
    tile::{TileKind, sets::TileIconSprite},
    ui_context::GameUiContext,
};

// ----------------------------------------------
// Constants
// ----------------------------------------------

const INSPECTOR_TITLE_FONT_SCALE:      UiFontScale = UiFontScale(1.5);
const INSPECTOR_SUBHEADING_FONT_SCALE: UiFontScale = UiFontScale(1.0);
const INSPECTOR_BODY_TEXT_FONT_SCALE:  UiFontScale = UiFontScale(1.0);

const INSPECTOR_BODY_TEXT_MAX_LINES: usize = 14;
const INSPECTOR_CONTENT_INSET:       f32   = 20.0; // Without a 9-slice panel.
const INSPECTOR_PANEL:               &str  = "hud_large";
const INSPECTOR_WIDTH:               f32   = 520.0;
const INSPECTOR_TOP:                 f32   = 130.0; // Under the top bar, room for a large panel's crest.
const INSPECTOR_BODY_TEXT_MAX_LEN:   usize = 1024;
const INSPECTOR_FMT_STR_MAX_LEN:     usize = 128;

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
        $headings.add($key, common::format_fixed_string!({ InspectorMenuHeadings::FMT_STR_MAX_LEN }, $($arg)*))
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
        $body.add_line(&common::format_fixed_string!({ InspectorMenuBody::FMT_STR_MAX_LEN }, $($arg)*))
    };
}

macro_rules! add_body_str {
    (&mut $body:ident, $($arg:tt)*) => {
        $body.add_str(&common::format_fixed_string!({ InspectorMenuBody::FMT_STR_MAX_LEN }, $($arg)*))
    };
}

pub(super) use add_body_line;
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
    where
        K: std::fmt::Display,
        V: std::fmt::Display,
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

        // Lines past the panel's room are left out, not a crash.
        for (i, line) in text.split('\n').take(INSPECTOR_BODY_TEXT_MAX_LINES).enumerate() {
            body_text.set_line_string(i, line);
        }
    }

    pub fn set_icon(&mut self, icon_sprite: TileIconSprite, tile_kind: TileKind) {
        let icon = self.find_icon();

        icon.set_sprite(icon_sprite.tex_info.texture);

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

    pub fn new(
        context: &mut GameUiContext,
        tile_inspector_menu_weak_ref: &TileInspectorMenuWeakMut,
        menu_name: &str,
    ) -> Self {
        let icon = UiSpriteIcon::new(Vec2::one()).with_outline(true); // placeholder size; set_icon sizes it.

        // In the HUD's colours, on its panel (light text on Kambara's stone and Tampadesa's mudbrick).
        let text_color = skin::hud_color("text", "ink");
        let sub_color = skin::hud_color("sub", "sub");
        let title = UiText::empty(INSPECTOR_TITLE_FONT_SCALE).with_color(text_color);
        let subheading = UiText::empty(INSPECTOR_SUBHEADING_FONT_SCALE).with_color(sub_color);

        // Placeholders: the title, then INSPECTOR_SUBHEADING_COUNT subheadings.
        let heading = UiMenuHeading::new([title, subheading.clone(), subheading.clone(), subheading.clone(), subheading])
            .with_center_horizontally(false);

        let mut icon_and_heading_group =
            UiWidgetGroup::new().with_widget_spacing(Vec2::new(20.0, 8.0)).with_stack_vertically(false);

        let icon_index = icon_and_heading_group.add_widget(icon);
        let heading_index = icon_and_heading_group.add_widget(heading);

        let close_button_inspector_menu_weak_ref = tile_inspector_menu_weak_ref.clone();
        // The HUD's close tile, named Close in widget paths (`BuildingInspector/Close`).
        let close_button = skin::hud_tile(context, skin::HudTileKind::Small, "Close", skin::HudGlyphGroup::Hud, "close")
            .with_tooltip(skin::tooltip(context, UiString::key(UiTextCategory::Hud, "button.close")))
            .on_pressed(move |_, context| {
                let mut inspector_menu = close_button_inspector_menu_weak_ref.upgrade().unwrap();
                inspector_menu.close(context_as_mut::<GameUiContext>(context));
            });

        let button_group = UiWidgetGroup::new().with_stack_vertically(false).with_widget(close_button);

        let menu_size = Self::calc_menu_size(context);

        // Placeholder text, wrapped at the menu's content width.
        let insets = match skin::panel(context, INSPECTOR_PANEL) {
            skin::SkinPanel::NineSlice { content, .. } => content.left + content.right,
            skin::SkinPanel::Sprite { .. } => INSPECTOR_CONTENT_INSET * 2.0,
        };
        let body_line = UiText::empty(INSPECTOR_BODY_TEXT_FONT_SCALE).with_color(text_color).with_wrap_width(menu_size.x - insets);
        let body_text = UiMenuHeading::new(vec![body_line; INSPECTOR_BODY_TEXT_MAX_LINES]);

        let separator = UiSeparator::new(); // Includes widget_spacing/item_spacing.

        // Beside the build column, under the top bar (inspector-<realm>.jpg), placed each frame so it
        // follows the window (the menus are built before the window's first frame when a game starts
        // from the command line, and aren't built again on a resize).
        let right_of_inspector = super::super::layout::column_width(context) + super::super::layout::MARGIN + super::super::layout::GAP;
        let width = menu_size.x;
        let position = UiMenuPosition::callback(move |_, context| {
            let window = ui::screen_size(context.ui_sys());
            Vec2::new((window.x - right_of_inspector - width).max(0.0), INSPECTOR_TOP)
        });
        let menu = UiMenu::new(menu_name)
            .with_flags(UiMenuFlags::PauseSimIfOpen | UiMenuFlags::Modal | UiMenuFlags::CloseModalOnEscape | UiMenuFlags::AdjustSizeToContents)
            .with_position(position)
            .with_size(menu_size)
            .with_widget_spacing(Vec2::new(0.0, 10.0));
        // The HUD's large panel, its ornaments included.
        let mut menu = skin::panel(context, INSPECTOR_PANEL).apply_to_menu(menu);

        menu.add_widget(separator.clone());
        let icon_and_heading_group_index = menu.add_widget(icon_and_heading_group);

        menu.add_widget(separator.clone());
        let body_text_index = menu.add_widget(body_text);

        menu.add_widget(separator.clone());
        let button_group_index = menu.add_widget(button_group);

        menu.add_widget(separator);

        let menu = menu.into_rc();
        Self { menu, icon_index, heading_index, icon_and_heading_group_index, body_text_index, button_group_index }
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

    // The mockup's width where the window has room; as tall as half the window.
    fn calc_menu_size(context: &GameUiContext) -> Vec2 {
        let window = Vec2::new(context.viewport_size.width as f32, context.viewport_size.height as f32);
        Vec2::new(INSPECTOR_WIDTH.min(window.x * 0.5), (window.y * 0.5).max(320.0).min(window.y - INSPECTOR_TOP - 140.0))
    }
}
