// The HUD's message log (docs/plans/realm-ui.md, Phase 5: P5d; hud-<realm>.jpg): the three latest
// messages (crate::messages), centred at the bottom of the screen between the minimap and the build
// column on the skin's `hud_panel`, each with its kind's icon, the newest in the HUD's text colour
// and marked "now", the others in its sub colour. Hidden while the log is empty. Recorded as the
// canvas `MessageLog/messages` (value: the messages shown, newest first: `kind`, `text`). Its
// close tile (`MessageLog/Close`) hides it until a newer message comes (messages::dismiss), and it
// hides itself once its newest message is SHOW_SECS old.

use common::{Color, Rect, Vec2};
use engine::ui::{self, UiLabelLook, UiPainter, UiTextAnchor, text::UiTextCategory, widgets::*};
use serde_json::json;

use super::layout;
use crate::{
    menu::skin::{self, SkinPanel},
    messages::{self, Message},
    ui_context::GameUiContext,
};

const LOG_SLACK: f32 = 4.0; // Room the panel keeps below the canvas.
const SHOWN: usize = 3;
const ROW_HEIGHT: f32 = 30.0;
const MAX_WIDTH: f32 = 860.0;
const SHOW_SECS: f32 = 20.0; // How long the log shows after its newest message comes (frame time).
const CLOSE_SIZE: f32 = 26.0; // The close tile's size in points.
const CLOSE_GAP: f32 = 8.0;

pub struct MessageLog {
    menu: Option<UiMenuRcMut>,
    canvas: Option<UiWidgetGroupWidgetIndex>,
    row: Option<UiMenuWidgetIndex>,
    built_for: (Vec2, f32, u64), // The window, the minimap's right edge and the newest message it was built for.
    newest: u64,                 // The newest message's id, and how long it has shown.
    newest_age: f32,
}

impl MessageLog {
    pub fn new() -> Self {
        Self { menu: None, canvas: None, row: None, built_for: (Vec2::zero(), 0.0, 0), newest: 0, newest_age: 0.0 }
    }

    fn build(&mut self, context: &mut GameUiContext, shown: Vec<Message>) {
        let window = ui::screen_size(context.ui_sys);
        let panel = skin::panel(context, "hud_panel");
        let insets = match &panel {
            SkinPanel::NineSlice { content, .. } => [content.left, content.top, content.right, content.bottom],
            SkinPanel::Sprite { .. } => [8.0; 4],
        };
        // Centred, clear of the minimap at the left and the build column at the right.
        let minimap_right = context.tile_map.minimap().layout().window.max.x;
        let side = minimap_right.max(layout::column_width(context) + layout::MARGIN) + layout::GAP;
        let width = (window.x - 2.0 * side).min(MAX_WIDTH).max(240.0);
        let inner = Vec2::new(width - insets[0] - insets[2], SHOWN as f32 * ROW_HEIGHT);
        let lines = Vec2::new(inner.x - CLOSE_SIZE - CLOSE_GAP, inner.y);
        // A few points of slack: the menu lays its contents out in whole points, so fractional insets
        // (Đại Long's) would otherwise leave the canvas a point too tall for it and make it scroll.
        let size = Vec2::new(width, (inner.y + insets[1] + insets[3] + LOG_SLACK).ceil());
        let position = Vec2::new((window.x - width) * 0.5, window.y - size.y - layout::MARGIN);

        let text = skin::hud_color("text", "ink");
        let sub = skin::hud_color("sub", "sub");
        let icons: Vec<_> = shown.iter().map(|message| skin::hud_icon(context, message.kind.icon())).collect();
        let bleed = skin::hud_icon_bleed();
        let now = ui::text::get(UiTextCategory::Hud, "messages.now").to_string();
        let value = json!(shown.iter().map(|message| json!({ "kind": message.kind, "text": message.text })).collect::<Vec<_>>());
        let canvas = UiCanvas::new(lines, move |_, painter, rect| {
            for (k, message) in shown.iter().enumerate() {
                let y = rect.min.y + k as f32 * ROW_HEIGHT + ROW_HEIGHT * 0.5;
                let (icon, icon_size) = icons[k];
                let scale = 22.0 / (icon_size.x - 2.0 * bleed).max(1.0);
                let drawn = icon_size * scale;
                painter.image(icon, Rect::from_pos_and_size(Vec2::new(rect.min.x + 12.0, y) - drawn * 0.5, drawn), Color::white());
                let look = UiLabelLook { size: 17.0, heading_font: k == 0, color: if k == 0 { text } else { sub }, ..Default::default() };
                let right = if k == 0 { rect.max.x - 48.0 } else { rect.max.x };
                let line = fit_text(painter, &message.text, right - rect.min.x - 34.0, &look);
                painter.label(Vec2::new(rect.min.x + 34.0, y + 6.0), UiTextAnchor::Start, &line, &look);
                if k == 0 {
                    painter.label(Vec2::new(rect.max.x, y + 6.0), UiTextAnchor::End, &now, &UiLabelLook { size: 14.0, color: sub, ..Default::default() });
                }
            }
        })
        .with_label("messages", Some(value));

        let mut menu = panel.apply_to_menu(
            UiMenu::new("MessageLog").with_flags(UiMenuFlags::IsOpen).with_position(position).with_size(size).with_widget_spacing(Vec2::zero()),
        );
        // The messages, and the close tile at the right of the newest.
        let close_scale = CLOSE_SIZE / skin::hud_tile_size(skin::HudTileKind::Small).x;
        let close = skin::hud_tile_scaled(context, skin::HudTileKind::Small, close_scale, "Close", skin::HudGlyphGroup::Hud, "close")
            .with_tooltip(skin::tooltip(context, ui::UiString::key(UiTextCategory::Hud, "messages.close")))
            .on_pressed(|_, _| messages::dismiss());
        let mut row = UiWidgetGroup::new().with_stack_vertically(false).with_center_horizontally(false).with_widget_spacing(Vec2::new(CLOSE_GAP, 0.0));
        let canvas = row.add_widget(canvas);
        row.add_widget(close);
        let row = menu.add_widget(row);
        self.canvas = Some(canvas);
        self.row = Some(row);
        self.menu = Some(menu.into_rc());
    }

    pub fn draw(&mut self, context: &mut GameUiContext) {
        let newest = messages::newest_id();
        if newest == 0 {
            self.menu = None;
            return;
        }
        // Shown for a while after its newest message comes, unless closed.
        if newest != self.newest {
            self.newest = newest;
            self.newest_age = 0.0;
        } else {
            self.newest_age += context.delta_time_secs;
        }
        if messages::is_dismissed() || self.newest_age > SHOW_SECS {
            return;
        }
        let window = ui::screen_size(context.ui_sys);
        let minimap_right = context.tile_map.minimap().layout().window.max.x;
        let key = (window, minimap_right, newest);
        if self.menu.is_none() || key != self.built_for {
            self.build(context, messages::latest(SHOWN));
            self.built_for = key;
        }
        if let Some(menu) = &mut self.menu {
            menu.draw(context);
        }
    }

    // Built again with the current skin and strings (InGameMenus::rebuild_hud).
    pub fn reset(&mut self) {
        self.menu = None;
    }
}

// `text`, cut with an ellipsis where it would run past `width` points.
fn fit_text(painter: &UiPainter, text: &str, width: f32, look: &UiLabelLook) -> String {
    let lines = painter.wrap_label(text, look, width);
    match lines.as_slice() {
        [] => String::new(),
        [line] => line.clone(),
        [first, ..] => format!("{}…", first.trim_end()),
    }
}
