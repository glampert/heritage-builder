// The HUD's message log (docs/plans/realm-ui.md, Phase 5: P5d; hud-<realm>.jpg): the three latest
// messages (crate::messages), centred at the bottom of the screen between the minimap and the build
// column on the skin's `hud_panel`, each with its kind's icon, the newest in the HUD's text colour
// and marked "now", the others in its sub colour. Hidden while the log is empty. Recorded as the
// canvas `MessageLog/messages` (value: the messages shown, newest first: `kind`, `text`).

use common::{Color, Rect, Vec2};
use engine::ui::{self, UiLabelLook, UiPainter, UiTextAnchor, text::UiTextCategory, widgets::*};
use serde_json::json;

use super::layout;
use crate::{
    menu::skin::{self, SkinPanel},
    messages::{self, Message},
    ui_context::GameUiContext,
};

const SHOWN: usize = 3;
const ROW_HEIGHT: f32 = 30.0;
const MAX_WIDTH: f32 = 860.0;

pub struct MessageLog {
    menu: Option<UiMenuRcMut>,
    canvas: Option<UiMenuWidgetIndex>,
    built_for: (Vec2, f32, u64), // The window, the minimap's right edge and the newest message it was built for.
}

impl MessageLog {
    pub fn new() -> Self {
        Self { menu: None, canvas: None, built_for: (Vec2::zero(), 0.0, 0) }
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
        let size = Vec2::new(width, inner.y + insets[1] + insets[3]);
        let position = Vec2::new((window.x - width) * 0.5, window.y - size.y - layout::MARGIN);

        let text = skin::hud_color("text", "ink");
        let sub = skin::hud_color("sub", "sub");
        let icons: Vec<_> = shown.iter().map(|message| skin::hud_icon(context, message.kind.icon())).collect();
        let bleed = skin::hud_icon_bleed();
        let now = ui::text::get(UiTextCategory::Hud, "messages.now").to_string();
        let value = json!(shown.iter().map(|message| json!({ "kind": message.kind, "text": message.text })).collect::<Vec<_>>());
        let canvas = UiCanvas::new(inner, move |_, painter, rect| {
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
        self.canvas = Some(menu.add_widget(canvas));
        self.menu = Some(menu.into_rc());
    }

    pub fn draw(&mut self, context: &mut GameUiContext) {
        let newest = messages::newest_id();
        if newest == 0 {
            self.menu = None;
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
