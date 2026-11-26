// The HUD's message log (docs/plans/realm-ui.md, Phase 5: P5d; hud-<realm>.jpg): the three latest
// messages (crate::messages), centred at the bottom of the screen between the minimap and the build
// column on the skin's `hud_panel`, each with its kind's icon, the newest in the HUD's text colour
// and marked "now", the others in its sub colour. Hidden while the log is empty. Each line is a
// canvas of its own, recorded as `MessageLog/line_<N>` from the newest, `line_1` (kind `message`,
// value its message's `id`, `kind`, `priority`, `text`, `target` and `month`); a line whose message
// has a place or a popup takes clicks, lit while hovered, and takes the player there or opens the
// popup again (open; docs/plans/messages.md, Phases 0 and 1). Its close tile (`MessageLog/Close`) hides it until a newer
// message comes (messages::dismiss), and it hides itself once its newest message is SHOW_SECS old.

use common::{Color, Rect, Vec2};
use engine::ui::{self, UiLabelLook, UiPainter, UiTextAnchor, text::UiTextCategory, widgets::*};

use super::layout;
use crate::{
    menu::{
        dialog,
        skin::{self, SkinPanel},
    },
    messages::{self, Message, MessageTarget},
    ui_context::GameUiContext,
};

const LOG_SLACK: f32 = 4.0; // Room the panel keeps below the canvas.
const SHOWN: usize = 3;
const ROW_HEIGHT: f32 = 30.0;
const MAX_WIDTH: f32 = 860.0;
const SHOW_SECS: f32 = 20.0; // How long the log shows after its newest message comes (frame time).
const CLOSE_SIZE: f32 = 26.0; // The close tile's size in points.
const CLOSE_GAP: f32 = 8.0;
const HOVER_ALPHA: f32 = 0.2; // The hover fill's: the HUD's sub text colour, faint.

pub struct MessageLog {
    menu: Option<UiMenuRcMut>,
    built_for: (Vec2, f32, u64), // The window, the minimap's right edge and the newest message it was built for.
    newest: u64,                 // The newest message's id, and how long it has shown.
    newest_age: f32,
}

impl MessageLog {
    pub fn new() -> Self {
        Self { menu: None, built_for: (Vec2::zero(), 0.0, 0), newest: 0, newest_age: 0.0 }
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
        let width = (window.x - 2.0 * side).clamp(240.0, MAX_WIDTH);
        let inner = Vec2::new(width - insets[0] - insets[2], SHOWN as f32 * ROW_HEIGHT);
        let lines = Vec2::new(inner.x - CLOSE_SIZE - CLOSE_GAP, inner.y);
        // A few points of slack: the menu lays its contents out in whole points, so fractional insets
        // (Đại Long's) would otherwise leave the canvas a point too tall for it and make it scroll.
        let size = Vec2::new(width, (inner.y + insets[1] + insets[3] + LOG_SLACK).ceil());
        let position = Vec2::new((window.x - width) * 0.5, window.y - size.y - layout::MARGIN);

        // A line per message, newest first, the rows left empty under them.
        let mut lines_group = UiWidgetGroup::new().with_center_horizontally(false).with_widget_spacing(Vec2::zero());
        let count = shown.len();
        for (k, message) in shown.into_iter().enumerate() {
            lines_group.add_widget(line(context, message, k, lines.x));
        }
        for _ in count..SHOWN {
            lines_group.add_widget(UiCanvas::new(Vec2::new(lines.x, ROW_HEIGHT), |_, _, _| {}));
        }

        let mut menu = panel.apply_to_menu(
            UiMenu::new("MessageLog")
                .with_flags(UiMenuFlags::IsOpen)
                .with_position(position)
                .with_size(size)
                .with_widget_spacing(Vec2::zero()),
        );
        // The messages, and the close tile at the right of the newest.
        let close_scale = CLOSE_SIZE / skin::hud_tile_size(skin::HudTileKind::Small).x;
        let close = skin::hud_tile_scaled(
            context,
            skin::HudTileKind::Small,
            close_scale,
            "Close",
            skin::HudGlyphGroup::Hud,
            "close",
        )
        .with_tooltip(skin::tooltip(context, ui::UiString::key(UiTextCategory::Hud, "messages.close")))
        .on_pressed(|_, _| messages::dismiss());
        let mut row = UiWidgetGroup::new()
            .with_stack_vertically(false)
            .with_center_horizontally(false)
            .with_widget_spacing(Vec2::new(CLOSE_GAP, 0.0));
        row.add_widget(lines_group);
        row.add_widget(close);
        menu.add_widget(row);
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

// The line of the message `index` from the newest, `width` points wide: its kind's icon and its
// text, the newest's in the heading font and marked "now". It takes clicks when the message has a
// place or a popup.
fn line(context: &mut GameUiContext, message: Message, index: usize, width: f32) -> UiCanvas {
    let newest = index == 0;
    let text = skin::hud_color("text", "ink");
    let sub = skin::hud_color("sub", "sub");
    let (icon, icon_size) = skin::hud_icon(context, message.kind.icon());
    let bleed = skin::hud_icon_bleed();
    let now = ui::text::get(UiTextCategory::Hud, "messages.now").to_string();
    let value = message.to_json();
    let (id, clickable) = (message.id, message.target.is_some() || message.popup.is_some());
    let canvas = UiCanvas::new(Vec2::new(width, ROW_HEIGHT), move |_, painter, rect| {
        let y = rect.min.y + ROW_HEIGHT * 0.5;
        let scale = 22.0 / (icon_size.x - 2.0 * bleed).max(1.0);
        let drawn = icon_size * scale;
        painter.image(icon, Rect::from_pos_and_size(Vec2::new(rect.min.x + 12.0, y) - drawn * 0.5, drawn), Color::white());
        let look =
            UiLabelLook { size: 17.0, heading_font: newest, color: if newest { text } else { sub }, ..Default::default() };
        let right = if newest { rect.max.x - 48.0 } else { rect.max.x };
        let fitted = fit_text(painter, &message.text, right - rect.min.x - 34.0, &look);
        painter.label(Vec2::new(rect.min.x + 34.0, y + 6.0), UiTextAnchor::Start, &fitted, &look);
        if newest {
            painter.label(Vec2::new(rect.max.x, y + 6.0), UiTextAnchor::End, &now, &UiLabelLook {
                size: 14.0,
                color: sub,
                ..Default::default()
            });
        }
    })
    .with_label(format!("line_{}", index + 1), Some(value))
    .with_kind("message");
    if !clickable {
        return canvas;
    }
    let hover = Color { a: HOVER_ALPHA, ..skin::hud_color("sub", "sub") };
    canvas.with_hover_fill(hover, 4.0).on_clicked(move |_, context| {
        if let Some(message) = messages::find(id) {
            open(&message, context_as_mut::<GameUiContext>(context));
        }
    })
}

// A click on a message's line: an important event's popup opens again (its button goes to its
// place), any other message goes to its place (go_to_target).
pub fn open(message: &Message, context: &mut GameUiContext) -> bool {
    if message.popup.is_some() {
        return dialog::open_event_popup(message.clone(), context);
    }
    go_to_target(message, context)
}

// Takes the player to the message's place: the camera to its cell, or to its building (where it
// stood once it's gone); the City book on its page; the world map, its city picked. False when it
// has no place or the dialog didn't open (the world map outside a game).
pub fn go_to_target(message: &Message, context: &mut GameUiContext) -> bool {
    let Some(target) = &message.target else {
        return false;
    };
    match target {
        MessageTarget::Cell(cell) => go_to(*cell, context),
        MessageTarget::Building(building, cell) => {
            let standing = {
                let sim = context.new_sim_context();
                sim.find_building(building.kind, building.id).map(|building| building.base_cell())
            };
            go_to(standing.unwrap_or(*cell), context)
        }
        MessageTarget::CityPage(page) => dialog::open_city_page(page, context),
        MessageTarget::WorldMap(city) => dialog::open_world_map_at(city.as_deref(), context),
    }
}

// The camera onto `cell`, at its ground's height. False for a cell off the map.
fn go_to(cell: common::coords::Cell, context: &mut GameUiContext) -> bool {
    if !context.tile_map.is_cell_within_bounds(cell) {
        return false;
    }
    let height = context.tile_map.elevation().centre_height(cell);
    context.camera.teleport_raised(cell, height)
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
