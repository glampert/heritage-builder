// Widgets that show data, painted from colours the caller gives (a skin's): label and value pairs
// (UiKeyValues), tables (UiTable) and meters (UiMeter). None takes input; each records itself with
// what it shows as its value, so automation can read it.

use std::any::Any;
use smallbox::smallbox;
use egui::{Align2, Sense, Stroke, StrokeKind, pos2, vec2};

use common::{Color, Vec2};

use super::{UiColumnWidth, UiTooltipText, UiWidget, UiWidgetCallback, UiWidgetContext, split_column_widths};
use crate::{
    render::texture::TextureHandle,
    ui::{
        UiFontScale, UiString, UiSystem,
        immediate::{self, color32, uv_full},
        internal,
        renderer::egui_texture,
    },
};

// ----------------------------------------------
// Shared
// ----------------------------------------------

#[derive(Copy, Clone, Default, PartialEq, Eq, Debug)]
pub enum UiColumnAlign {
    #[default]
    Left,
    Center,
    Right,
}

impl UiColumnAlign {
    fn anchor(self) -> Align2 {
        match self {
            Self::Left => Align2::LEFT_CENTER,
            Self::Center => Align2::CENTER_CENTER,
            Self::Right => Align2::RIGHT_CENTER,
        }
    }

    // Where text with this alignment is anchored in `rect`.
    fn anchor_pos(self, rect: egui::Rect) -> egui::Pos2 {
        match self {
            Self::Left => rect.left_center(),
            Self::Center => rect.center(),
            Self::Right => rect.right_center(),
        }
    }
}

// The rows' look, shared by UiKeyValues and UiTable: text colours (the style's if None), a rule
// under each row, and the room above and below the text.
#[derive(Copy, Clone, Debug)]
pub struct UiRowLook {
    pub label_color: Option<Color>, // Key-values' labels, a table's cells.
    pub value_color: Option<Color>, // Key-values' values.
    pub header_color: Option<Color>, // A table's header.
    pub rule_color: Option<Color>,  // No rule if None.
    pub padding: f32,
}

impl Default for UiRowLook {
    fn default() -> Self {
        Self { label_color: None, value_color: None, header_color: None, rule_color: Some(Color::new(0.5, 0.45, 0.4, 0.35)), padding: 4.0 }
    }
}

// One row's height for text at `font_scale`.
fn row_height(ui_sys: &UiSystem, font_scale: UiFontScale, look: &UiRowLook) -> f32 {
    internal::calc_text_line_height(ui_sys, font_scale) + look.padding * 2.0
}

fn draw_rule(ui: &egui::Ui, rect: egui::Rect, look: &UiRowLook) {
    if let Some(color) = look.rule_color {
        let y = rect.bottom() - 0.5;
        ui.painter().hline(rect.x_range(), y, Stroke::new(1.0, color32(color)));
    }
}

// The least room between a key-value row's label and its value, in points.
const KEY_VALUE_GAP: f32 = 8.0;

// The text in `rect`, cut at its width.
fn draw_cell_text(ui: &egui::Ui, rect: egui::Rect, text: &str, font: &egui::FontId, color: egui::Color32, align: UiColumnAlign) {
    draw_spaced_cell_text(ui, rect, text, font, color, align, 0.0);
}

// The same with `letter_spacing` points between its letters.
fn draw_spaced_cell_text(
    ui: &egui::Ui,
    rect: egui::Rect,
    text: &str,
    font: &egui::FontId,
    color: egui::Color32,
    align: UiColumnAlign,
    letter_spacing: f32,
) {
    let galley = if letter_spacing == 0.0 {
        ui.painter().layout(text.to_string(), font.clone(), color, f32::INFINITY)
    } else {
        let format = egui::TextFormat { font_id: font.clone(), color, extra_letter_spacing: letter_spacing, ..Default::default() };
        ui.painter().layout_job(egui::text::LayoutJob::single_section(text.to_string(), format))
    };
    let anchor = align.anchor();
    let pos = anchor.anchor_size(align.anchor_pos(rect), galley.size()).min;
    ui.painter().with_clip_rect(rect.intersect(ui.clip_rect())).galley(pos, galley, color);
}

// ----------------------------------------------
// UiKeyValues
// ----------------------------------------------

// Label and value pairs in two columns, labels on the left and values right-aligned, with a rule
// under each row: a map's preview, the city pages. Or side by side (with_columns), each pair a
// column with its label over its value, larger: the debrief's stats. As wide as the menu (or a
// given width). Recorded as kind "key_values", its value the pairs: [[label, value], ...].
pub struct UiKeyValues {
    label: UiString,
    pairs: Vec<(UiString, UiString)>,
    font_scale: UiFontScale,
    width: Option<f32>,
    look: UiRowLook,
    columns: Option<UiKeyValueColumns>,
}

// The side-by-side layout (UiKeyValues::with_columns): each value at `value_font_scale`, in the
// heading font or not, `gap` points under its label. `max_lines` caps the label's and the value's
// rows at that many points (their text stays centred in them), for a font whose own line is much
// taller than its letters; None keeps the font's.
#[derive(Copy, Clone, Debug)]
pub struct UiKeyValueColumns {
    pub value_font_scale: UiFontScale,
    pub heading_font: bool,
    pub gap: f32,
    pub max_lines: Option<[f32; 2]>,
}

impl UiKeyValueColumns {
    fn value_font(&self, ui_sys: &UiSystem) -> egui::FontId {
        let mut font = internal::font(ui_sys, self.value_font_scale);
        if self.heading_font {
            font.family = ui_sys.style_font(&egui::TextStyle::Heading).family;
        }
        font
    }

    // The label's and the value's rows: their fonts' lines, capped by `max_lines`.
    fn lines(&self, ui_sys: &UiSystem, font_scale: UiFontScale) -> [f32; 2] {
        let value_font = self.value_font(ui_sys);
        let label_line = internal::calc_text_line_height(ui_sys, font_scale);
        let value_line = ui_sys.ctx().fonts_mut(|fonts| fonts.row_height(&value_font));
        match self.max_lines {
            Some([label_max, value_max]) => [label_line.min(label_max.max(1.0)), value_line.min(value_max.max(1.0))],
            None => [label_line, value_line],
        }
    }

    // The height of the pairs: a label's row, the gap and a value's row, and the look's padding.
    fn height(&self, ui_sys: &UiSystem, font_scale: UiFontScale, look: &UiRowLook) -> f32 {
        let [label_row, value_row] = self.lines(ui_sys, font_scale);
        label_row + look.padding * 2.0 + self.gap + value_row
    }
}

impl UiWidget for UiKeyValues {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut dyn UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        let ui_sys = context.ui_sys();
        let row = row_height(ui_sys, self.font_scale, &self.look);
        let font = internal::font(ui_sys, self.font_scale);

        if let Some(columns) = self.columns {
            self.draw_columns(ui_sys, &columns);
            return;
        }

        ui_sys.with_ui(|ui| {
            let width = self.width.unwrap_or_else(|| immediate::fill_size(ui).x);
            let (rect, response) = ui.allocate_exact_size(vec2(width, row * self.pairs.len() as f32), Sense::hover());

            let label_color = self.look.label_color.map_or_else(|| ui.visuals().weak_text_color(), color32);
            let value_color = self.look.value_color.map_or_else(|| ui.visuals().text_color(), color32);
            for (index, (label, value)) in self.pairs.iter().enumerate() {
                let row_rect = egui::Rect::from_min_size(pos2(rect.left(), rect.top() + row * index as f32), vec2(width, row));
                let text_rect = row_rect.shrink2(vec2(0.0, self.look.padding));
                // The value whole, the label cut short of it.
                let value_width = ui.fonts_mut(|fonts| fonts.layout_no_wrap(value.resolve().to_string(), font.clone(), value_color).size().x);
                let label_right = (text_rect.right() - value_width - KEY_VALUE_GAP).max(text_rect.left());
                draw_cell_text(ui, text_rect.with_max_x(label_right), label.resolve(), &font, label_color, UiColumnAlign::Left);
                draw_cell_text(ui, text_rect, value.resolve(), &font, value_color, UiColumnAlign::Right);
                draw_rule(ui, row_rect, &self.look);
            }

            immediate::record_lazy(ui_sys, ui, &response, "key_values", self.label.resolve(), None, true, || {
                let pairs = self.pairs.iter().map(|(label, value)| serde_json::json!([label.resolve(), value.resolve()])).collect();
                Some(serde_json::Value::Array(pairs))
            });
        });
    }

    fn measure(&self, context: &dyn UiWidgetContext) -> Vec2 {
        let ui_sys = context.ui_sys();
        let width = self.width.unwrap_or_else(|| internal::available_size(ui_sys).x);
        if let Some(columns) = self.columns {
            return Vec2::new(width, columns.height(ui_sys, self.font_scale, &self.look));
        }
        Vec2::new(width, row_height(ui_sys, self.font_scale, &self.look) * self.pairs.len() as f32)
    }

    fn label(&self) -> &str {
        self.label.resolve()
    }

    fn font_scale(&self) -> UiFontScale {
        self.font_scale
    }
}

impl Default for UiKeyValues {
    fn default() -> Self {
        Self::new()
    }
}

impl UiKeyValues {
    pub fn new() -> Self {
        Self {
            label: UiString::default(),
            pairs: Vec::new(),
            font_scale: UiFontScale::default(),
            width: None,
            look: UiRowLook::default(),
            columns: None,
        }
    }

    // The pairs side by side, in columns of equal width, each label over its value (UiKeyValueColumns).
    pub fn with_columns(mut self, columns: UiKeyValueColumns) -> Self {
        debug_assert!(columns.value_font_scale.is_valid());
        self.columns = Some(columns);
        self
    }

    fn draw_columns(&self, ui_sys: &UiSystem, columns: &UiKeyValueColumns) {
        let font = internal::font(ui_sys, self.font_scale);
        let value_font = columns.value_font(ui_sys);
        let label_line = internal::calc_text_line_height(ui_sys, self.font_scale);
        let value_line = ui_sys.ctx().fonts_mut(|fonts| fonts.row_height(&value_font));
        let [label_row, value_row] = columns.lines(ui_sys, self.font_scale);
        let height = columns.height(ui_sys, self.font_scale, &self.look);

        ui_sys.with_ui(|ui| {
            let width = self.width.unwrap_or_else(|| immediate::fill_size(ui).x);
            let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
            let label_color = self.look.label_color.map_or_else(|| ui.visuals().weak_text_color(), color32);
            let value_color = self.look.value_color.map_or_else(|| ui.visuals().text_color(), color32);
            let column_width = width / self.pairs.len().max(1) as f32;
            for (index, (label, value)) in self.pairs.iter().enumerate() {
                let left = rect.left() + column_width * index as f32;
                let top = rect.top() + self.look.padding;
                let cell_width = column_width - KEY_VALUE_GAP;
                // Each text centred in its row, in a rect of its font's line so a capped row
                // doesn't clip its letters.
                let label_middle = pos2(left + cell_width * 0.5, top + label_row * 0.5);
                let label_rect = egui::Rect::from_center_size(label_middle, vec2(cell_width, label_line));
                draw_cell_text(ui, label_rect, label.resolve(), &font, label_color, UiColumnAlign::Left);
                let value_middle = pos2(left + cell_width * 0.5, top + label_row + columns.gap + value_row * 0.5);
                let value_rect = egui::Rect::from_center_size(value_middle, vec2(cell_width, value_line));
                draw_cell_text(ui, value_rect, value.resolve(), &value_font, value_color, UiColumnAlign::Left);
            }

            immediate::record_lazy(ui_sys, ui, &response, "key_values", self.label.resolve(), None, true, || {
                let pairs = self.pairs.iter().map(|(label, value)| serde_json::json!([label.resolve(), value.resolve()])).collect();
                Some(serde_json::Value::Array(pairs))
            });
        });
    }

    // Names it in widget paths ("<menu>/key_values" without one).
    pub fn with_label(mut self, label: impl Into<UiString>) -> Self {
        self.label = label.into();
        self
    }

    pub fn with_pair(mut self, label: impl Into<UiString>, value: impl Into<UiString>) -> Self {
        self.pairs.push((label.into(), value.into()));
        self
    }

    pub fn with_font_scale(mut self, font_scale: UiFontScale) -> Self {
        debug_assert!(font_scale.is_valid());
        self.font_scale = font_scale;
        self
    }

    pub fn with_width(mut self, width: f32) -> Self {
        debug_assert!(width > 0.0);
        self.width = Some(width);
        self
    }

    pub fn with_look(mut self, look: UiRowLook) -> Self {
        self.look = look;
        self
    }

    pub fn pairs(&self) -> &[(UiString, UiString)] {
        &self.pairs
    }

    // The value of the pair at `index`, e.g. a stat that changes.
    pub fn set_value(&mut self, index: usize, value: impl Into<UiString>) {
        self.pairs[index].1 = value.into();
    }

    pub fn set_pairs(&mut self, pairs: impl IntoIterator<Item = (UiString, UiString)>) {
        self.pairs = pairs.into_iter().collect();
    }
}

// ----------------------------------------------
// UiTable
// ----------------------------------------------

// A table's column: its header, width and the alignment of its header and cells.
#[derive(Clone, Debug)]
pub struct UiTableColumn {
    pub title: UiString,
    pub width: UiColumnWidth,
    pub align: UiColumnAlign,
    pub title_color: Option<Color>, // Its header's colour: the look's header colour if None.
    pub color: Option<Color>,       // Its cells' colour: the look's label colour if None.
}

impl UiTableColumn {
    pub fn new(title: impl Into<UiString>, width: UiColumnWidth, align: UiColumnAlign) -> Self {
        Self { title: title.into(), width, align, title_color: None, color: None }
    }

    pub fn with_title_color(mut self, color: Color) -> Self {
        self.title_color = Some(color);
        self
    }

    pub fn with_color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
}

// A row's status, drawn in the table's status column: a goal met, still open, or failed.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum UiTableStatus {
    Done,
    Pending,
    Failed,
}

impl UiTableStatus {
    pub fn name(self) -> &'static str {
        match self {
            Self::Done => "done",
            Self::Pending => "pending",
            Self::Failed => "failed",
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct UiTableRow {
    pub cells: Vec<String>,
    pub status: Option<UiTableStatus>,
}

impl UiTableRow {
    pub fn new(cells: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self { cells: cells.into_iter().map(Into::into).collect(), status: None }
    }

    pub fn with_status(mut self, status: UiTableStatus) -> Self {
        self.status = Some(status);
        self
    }
}

// How the status column draws each status: a sprite each, or painted marks (a tick, a ring, a
// cross) in these colours.
#[derive(Copy, Clone, Debug)]
pub enum UiTableStatusLook {
    Painted { done: Color, pending: Color, failed: Color },
    Sprites { done: TextureHandle, pending: TextureHandle, failed: TextureHandle },
    // A disc in `done` with a tick in `mark` for a goal met; a ring in `open` for one still open,
    // with a cross in it for one failed (the debrief's goals).
    Badges { done: Color, mark: Color, open: Color },
}

impl Default for UiTableStatusLook {
    fn default() -> Self {
        Self::Painted { done: Color::new(0.2, 0.55, 0.25, 1.0), pending: Color::new(0.5, 0.45, 0.4, 1.0), failed: Color::new(0.7, 0.15, 0.1, 1.0) }
    }
}

// Rows under a header, each column with its width and alignment, and an optional status column
// on the left: the goals (target and achieved), the finance accounts. As wide as the menu (or a
// given width). Recorded as kind "table", its value {"columns": [...], "rows": [[...], ...]},
// with "status" (each row's, or null) when it has a status column.
pub struct UiTable {
    label: UiString,
    columns: Vec<UiTableColumn>,
    rows: Vec<UiTableRow>,
    status_column: Option<UiTableStatusLook>,
    bullet: Option<(UiBullet, Color, f32)>,
    font_scale: UiFontScale,
    width: Option<f32>,
    look: UiRowLook,
    column_spacing: f32,
    header: UiTableHeader,
}

// How a table's header row is drawn: its text at its own font scale (the table's if None), in the
// heading font or not, with a rule under it or not, and its first column's title reaching over the
// status column (a section title, as the debrief's "GOALS"), its letters `letter_spacing` points
// apart.
#[derive(Copy, Clone, Debug)]
pub struct UiTableHeader {
    pub font_scale: Option<UiFontScale>,
    pub heading_font: bool,
    pub rule: bool,
    pub over_status: bool,
    pub letter_spacing: f32,
}

impl Default for UiTableHeader {
    fn default() -> Self {
        Self { font_scale: None, heading_font: false, rule: true, over_status: false, letter_spacing: 0.0 }
    }
}

// The width of a table's bullet column, over its bullets' size.
const BULLET_COLUMN_FACTOR: f32 = 2.4;

// A mark before each of a table's rows (UiTable::with_bullets): a list of goods.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum UiBullet {
    Square,
    Diamond,
    Circle,
}

fn draw_bullet(ui: &egui::Ui, center: egui::Pos2, size: f32, bullet: UiBullet, color: Color) {
    let half = size * 0.5;
    let fill = color32(color);
    match bullet {
        UiBullet::Square => {
            ui.painter().rect_filled(egui::Rect::from_center_size(center, vec2(size, size)), 0.0, fill);
        }
        UiBullet::Diamond => {
            let points = vec![center + vec2(0.0, -half), center + vec2(half, 0.0), center + vec2(0.0, half), center + vec2(-half, 0.0)];
            ui.painter().add(egui::Shape::convex_polygon(points, fill, Stroke::NONE));
        }
        UiBullet::Circle => {
            ui.painter().circle_filled(center, half, fill);
        }
    }
}

impl UiWidget for UiTable {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut dyn UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        let ui_sys = context.ui_sys();
        let row = row_height(ui_sys, self.font_scale, &self.look);
        let font = internal::font(ui_sys, self.font_scale);
        let status_width = if self.status_column.is_some() {
            row
        } else if let Some((_, _, size)) = self.bullet {
            (size * BULLET_COLUMN_FACTOR).ceil()
        } else {
            0.0
        };

        let header_row = self.header_height(ui_sys);
        let mut header_font = internal::font(ui_sys, self.header.font_scale.unwrap_or(self.font_scale));
        if self.header.heading_font {
            header_font.family = ui_sys.style_font(&egui::TextStyle::Heading).family;
        }

        ui_sys.with_ui(|ui| {
            let width = self.width.unwrap_or_else(|| immediate::fill_size(ui).x);
            let height = header_row + row * self.rows.len() as f32;
            let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::hover());

            let column_widths: Vec<UiColumnWidth> = self.columns.iter().map(|column| column.width).collect();
            let cells_left = rect.left() + status_width;
            let widths = split_column_widths(width - status_width, &column_widths, self.column_spacing);
            let column_rect = |index: usize, row_rect: egui::Rect| {
                let left = cells_left + widths[..index].iter().sum::<f32>() + self.column_spacing * index as f32;
                egui::Rect::from_min_size(pos2(left, row_rect.top()), vec2(widths[index], row_rect.height())).shrink2(vec2(0.0, self.look.padding))
            };

            let header_color = self.look.header_color.map_or_else(|| ui.visuals().strong_text_color(), color32);
            let text_color = self.look.label_color.map_or_else(|| ui.visuals().text_color(), color32);

            let header_rect = egui::Rect::from_min_size(rect.min, vec2(width, header_row));
            for (index, column) in self.columns.iter().enumerate() {
                let mut cell = column_rect(index, header_rect);
                if index == 0 && self.header.over_status {
                    cell = cell.with_min_x(rect.left());
                }
                let color = column.title_color.map_or(header_color, color32);
                draw_spaced_cell_text(ui, cell, column.title.resolve(), &header_font, color, column.align, self.header.letter_spacing);
            }
            if self.header.rule {
                draw_rule(ui, header_rect, &UiRowLook { rule_color: self.look.rule_color.or(Some(Color::gray())), ..self.look });
            }

            for (row_index, table_row) in self.rows.iter().enumerate() {
                let row_rect = egui::Rect::from_min_size(pos2(rect.left(), rect.top() + header_row + row * row_index as f32), vec2(width, row));
                if let (Some(look), Some(status)) = (&self.status_column, table_row.status) {
                    let icon_rect = egui::Rect::from_center_size(pos2(rect.left() + status_width * 0.5, row_rect.center().y), vec2(row, row) * 0.55);
                    draw_status(ui, icon_rect, status, look);
                }
                if let Some((bullet, color, size)) = self.bullet {
                    let center = pos2(rect.left() + status_width * 0.4, row_rect.center().y);
                    draw_bullet(ui, center, size, bullet, color);
                }
                for (index, column) in self.columns.iter().enumerate() {
                    let text = table_row.cells.get(index).map_or("", String::as_str);
                    let color = column.color.map_or(text_color, color32);
                    draw_cell_text(ui, column_rect(index, row_rect), text, &font, color, column.align);
                }
                draw_rule(ui, row_rect, &self.look);
            }

            immediate::record_lazy(ui_sys, ui, &response, "table", self.label.resolve(), None, true, || Some(self.value()));
        });
    }

    fn measure(&self, context: &dyn UiWidgetContext) -> Vec2 {
        let ui_sys = context.ui_sys();
        let width = self.width.unwrap_or_else(|| internal::available_size(ui_sys).x);
        Vec2::new(width, self.header_height(ui_sys) + row_height(ui_sys, self.font_scale, &self.look) * self.rows.len() as f32)
    }

    fn label(&self) -> &str {
        self.label.resolve()
    }

    fn font_scale(&self) -> UiFontScale {
        self.font_scale
    }
}

fn draw_status(ui: &egui::Ui, rect: egui::Rect, status: UiTableStatus, look: &UiTableStatusLook) {
    match *look {
        UiTableStatusLook::Sprites { done, pending, failed } => {
            let texture = match status {
                UiTableStatus::Done => done,
                UiTableStatus::Pending => pending,
                UiTableStatus::Failed => failed,
            };
            ui.painter().image(egui_texture(texture), rect, uv_full(), egui::Color32::WHITE);
        }
        UiTableStatusLook::Badges { done, mark, open } => {
            let radius = rect.width() * 0.5;
            let width = (rect.width() * 0.09).max(1.0);
            match status {
                UiTableStatus::Done => {
                    ui.painter().circle_filled(rect.center(), radius, color32(done));
                    let c = rect.center();
                    let points = vec![c + vec2(-radius * 0.5, 0.0), c + vec2(-radius * 0.17, radius * 0.42), c + vec2(radius * 0.5, -radius * 0.42)];
                    ui.painter().add(egui::Shape::line(points, Stroke::new(width * 1.4, color32(mark))));
                }
                UiTableStatus::Pending | UiTableStatus::Failed => {
                    ui.painter().circle_stroke(rect.center(), radius - width * 0.5, Stroke::new(width, color32(open)));
                    if status == UiTableStatus::Failed {
                        let r = rect.shrink(radius * 0.58);
                        let stroke = Stroke::new(width, color32(open));
                        ui.painter().line_segment([r.left_top(), r.right_bottom()], stroke);
                        ui.painter().line_segment([r.right_top(), r.left_bottom()], stroke);
                    }
                }
            }
        }
        UiTableStatusLook::Painted { done, pending, failed } => {
            let width = (rect.width() * 0.18).max(1.5);
            match status {
                UiTableStatus::Done => {
                    let points = vec![
                        rect.left_center() + vec2(rect.width() * 0.05, 0.0),
                        pos2(rect.left() + rect.width() * 0.4, rect.bottom() - rect.height() * 0.1),
                        rect.right_top() + vec2(-rect.width() * 0.05, rect.height() * 0.1),
                    ];
                    ui.painter().add(egui::Shape::line(points, Stroke::new(width, color32(done))));
                }
                UiTableStatus::Pending => {
                    ui.painter().circle_stroke(rect.center(), rect.width() * 0.4, Stroke::new(width * 0.7, color32(pending)));
                }
                UiTableStatus::Failed => {
                    let stroke = Stroke::new(width, color32(failed));
                    let r = rect.shrink(rect.width() * 0.12);
                    ui.painter().line_segment([r.left_top(), r.right_bottom()], stroke);
                    ui.painter().line_segment([r.right_top(), r.left_bottom()], stroke);
                }
            }
        }
    }
}

impl Default for UiTable {
    fn default() -> Self {
        Self::new()
    }
}

impl UiTable {
    pub fn new() -> Self {
        Self {
            label: UiString::default(),
            columns: Vec::new(),
            rows: Vec::new(),
            status_column: None,
            bullet: None,
            font_scale: UiFontScale::default(),
            width: None,
            look: UiRowLook::default(),
            column_spacing: 8.0,
            header: UiTableHeader::default(),
        }
    }

    // How its header row is drawn (UiTableHeader).
    pub fn with_header(mut self, header: UiTableHeader) -> Self {
        debug_assert!(header.font_scale.is_none_or(|scale| scale.is_valid()));
        self.header = header;
        self
    }

    // The header row's height: its text's line and the look's padding.
    fn header_height(&self, ui_sys: &UiSystem) -> f32 {
        let font_scale = self.header.font_scale.unwrap_or(self.font_scale);
        if self.header.heading_font {
            let mut font = internal::font(ui_sys, font_scale);
            font.family = ui_sys.style_font(&egui::TextStyle::Heading).family;
            ui_sys.ctx().fonts_mut(|fonts| fonts.row_height(&font)) + self.look.padding * 2.0
        } else {
            row_height(ui_sys, font_scale, &self.look)
        }
    }

    // Names it in widget paths ("<menu>/table" without one).
    pub fn with_label(mut self, label: impl Into<UiString>) -> Self {
        self.label = label.into();
        self
    }

    pub fn with_column(mut self, column: UiTableColumn) -> Self {
        self.columns.push(column);
        self
    }

    pub fn with_row(mut self, row: UiTableRow) -> Self {
        self.rows.push(row);
        self
    }

    // A status column on the left, as wide as a row is tall, drawn with `look`.
    pub fn with_status_column(mut self, look: UiTableStatusLook) -> Self {
        self.status_column = Some(look);
        self
    }

    pub fn with_font_scale(mut self, font_scale: UiFontScale) -> Self {
        debug_assert!(font_scale.is_valid());
        self.font_scale = font_scale;
        self
    }

    // A `bullet` in `color`, `size` points large, before each row (not the header), in a narrow
    // column at the left.
    pub fn with_bullets(mut self, bullet: UiBullet, color: Color, size: f32) -> Self {
        self.bullet = Some((bullet, color, size.max(1.0)));
        self
    }

    pub fn with_width(mut self, width: f32) -> Self {
        debug_assert!(width > 0.0);
        self.width = Some(width);
        self
    }

    pub fn with_look(mut self, look: UiRowLook) -> Self {
        self.look = look;
        self
    }

    pub fn with_column_spacing(mut self, spacing: f32) -> Self {
        debug_assert!(spacing >= 0.0);
        self.column_spacing = spacing;
        self
    }

    pub fn rows(&self) -> &[UiTableRow] {
        &self.rows
    }

    pub fn set_rows(&mut self, rows: impl IntoIterator<Item = UiTableRow>) {
        self.rows = rows.into_iter().collect();
    }

    // What ui.tree shows.
    fn value(&self) -> serde_json::Value {
        let mut value = serde_json::json!({
            "columns": self.columns.iter().map(|column| column.title.resolve()).collect::<Vec<_>>(),
            "rows": self.rows.iter().map(|row| row.cells.clone()).collect::<Vec<_>>(),
        });
        if self.status_column.is_some() {
            value["status"] = self.rows.iter().map(|row| row.status.map(UiTableStatus::name)).collect();
        }
        value
    }
}

// ----------------------------------------------
// UiMeter
// ----------------------------------------------

// The part of a meter `value` fills out of `max`, from 0 to 1.
pub fn meter_fraction(value: f32, max: f32) -> f32 {
    if max > 0.0 && value.is_finite() { (value / max).clamp(0.0, 1.0) } else { 0.0 }
}

// How many of `steps` steps `value` out of `max` fills: the nearest whole step.
pub fn meter_filled_steps(value: f32, max: f32, steps: u32) -> u32 {
    ((meter_fraction(value, max) * steps as f32).round() as u32).min(steps)
}

// The points of a ring's filled arc: from the top, clockwise, `fraction` of the way round, at
// most `segments` segments for a full turn (at least one for any arc). None for an empty arc.
pub fn ring_arc_points(center: Vec2, radius: f32, fraction: f32, segments: u32) -> Vec<Vec2> {
    let fraction = fraction.clamp(0.0, 1.0);
    if fraction <= 0.0 {
        return Vec::new();
    }
    let count = ((segments as f32 * fraction).ceil() as u32).max(1);
    (0..=count)
        .map(|index| {
            // Screen y grows down: the top is -90°, clockwise is increasing angle.
            let angle = -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * fraction * index as f32 / count as f32;
            Vec2::new(center.x + radius * angle.cos(), center.y + radius * angle.sin())
        })
        .collect()
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum UiMeterKind {
    Bar,        // Filled from the left: a goal's progress.
    Ring,       // Filled clockwise from the top, with its value in the middle: the rating badges.
    Steps(u32), // So many segments, filled from the left: relations 0–5.
}

// A meter's colours: the filled part, the empty part, an outline, the goal's tick and the ring's
// text (the style's text colour if None).
#[derive(Copy, Clone, Debug)]
pub struct UiMeterLook {
    pub fill: Color,
    pub track: Color,
    pub outline: Option<Color>,
    pub goal: Color,
    pub text: Option<Color>,
}

impl Default for UiMeterLook {
    fn default() -> Self {
        Self {
            fill: Color::new(0.66, 0.15, 0.11, 1.0),
            track: Color::new(0.95, 0.89, 0.74, 1.0),
            outline: Some(Color::new(0.42, 0.31, 0.2, 1.0)),
            goal: Color::new(0.17, 0.13, 0.09, 1.0),
            text: None,
        }
    }
}

pub type UiMeterReadValue = UiWidgetCallback<dyn Fn(&UiMeter, &mut dyn UiWidgetContext) -> f32>;

// A painted meter: a bar, a ring or steps, showing a value out of a maximum, with an optional goal
// tick on a bar. The value is set (set_value) or read each frame (on_read_value). Recorded as kind
// "meter", its value {"value": v, "max": m}.
pub struct UiMeter {
    label: UiString,
    kind: UiMeterKind,
    value: f32,
    max: f32,
    goal: Option<f32>,
    size: Vec2,
    look: UiMeterLook,
    text: Option<UiString>, // In a ring's middle; its value, rounded, if None.
    font_scale: UiFontScale,
    tooltip: Option<UiTooltipText>,
    on_read_value: UiMeterReadValue,
}

impl UiWidget for UiMeter {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut dyn UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        if let Some(on_read_value) = &self.on_read_value {
            self.value = on_read_value(self, context);
        }

        let ui_sys = context.ui_sys();
        let font = internal::font(ui_sys, self.font_scale);
        let hovered = ui_sys.with_ui(|ui| {
            let (rect, response) = ui.allocate_exact_size(egui::vec2(self.size.x, self.size.y), Sense::hover());
            if ui.is_rect_visible(rect) {
                self.paint(ui, rect, &font);
            }
            let tooltip = self.tooltip.as_ref().map(|tooltip| tooltip.text.resolve());
            let value = || Some(serde_json::json!({ "value": self.value, "max": self.max }));
            immediate::record_lazy(ui_sys, ui, &response, "meter", self.label.resolve(), tooltip, true, value);
            response.hovered()
        });

        if hovered && let Some(tooltip) = &self.tooltip {
            tooltip.draw(context);
        }
    }

    fn measure(&self, _context: &dyn UiWidgetContext) -> Vec2 {
        self.size
    }

    fn label(&self) -> &str {
        self.label.resolve()
    }

    fn font_scale(&self) -> UiFontScale {
        self.font_scale
    }
}

impl UiMeter {
    // A meter of `kind` taking `size` points, at 0 out of 1.
    pub fn new(kind: UiMeterKind, size: Vec2) -> Self {
        debug_assert!(size.x > 0.0 && size.y > 0.0);
        Self {
            label: UiString::default(),
            kind,
            value: 0.0,
            max: 1.0,
            goal: None,
            size,
            look: UiMeterLook::default(),
            text: None,
            font_scale: UiFontScale::default(),
            tooltip: None,
            on_read_value: None,
        }
    }

    // Names it in widget paths ("<menu>/meter" without one).
    pub fn with_label(mut self, label: impl Into<UiString>) -> Self {
        self.label = label.into();
        self
    }

    pub fn with_value(mut self, value: f32, max: f32) -> Self {
        self.set_value(value, max);
        self
    }

    // A tick at the goal, on a bar.
    pub fn with_goal(mut self, goal: f32) -> Self {
        self.goal = Some(goal);
        self
    }

    pub fn with_look(mut self, look: UiMeterLook) -> Self {
        self.look = look;
        self
    }

    // The text in a ring's middle, in place of its value.
    pub fn with_text(mut self, text: impl Into<UiString>) -> Self {
        self.text = Some(text.into());
        self
    }

    pub fn with_font_scale(mut self, font_scale: UiFontScale) -> Self {
        debug_assert!(font_scale.is_valid());
        self.font_scale = font_scale;
        self
    }

    pub fn with_tooltip(mut self, tooltip: UiTooltipText) -> Self {
        self.tooltip = Some(tooltip);
        self
    }

    // Gives the value each frame, e.g. from the game's state.
    pub fn on_read_value<F>(mut self, f: F) -> Self
    where
        F: Fn(&UiMeter, &mut dyn UiWidgetContext) -> f32 + 'static,
    {
        self.on_read_value = Some(smallbox!(f));
        self
    }

    pub fn set_value(&mut self, value: f32, max: f32) {
        debug_assert!(max > 0.0);
        self.value = value;
        self.max = max;
    }

    pub fn value(&self) -> f32 {
        self.value
    }

    pub fn max(&self) -> f32 {
        self.max
    }

    fn paint(&self, ui: &egui::Ui, rect: egui::Rect, font: &egui::FontId) {
        let painter = ui.painter();
        let look = &self.look;
        let fraction = meter_fraction(self.value, self.max);
        let outline = look.outline.map_or(Stroke::NONE, |color| Stroke::new(1.0, color32(color)));

        match self.kind {
            UiMeterKind::Bar => {
                let radius = rect.height() * 0.5;
                painter.rect_filled(rect, radius, color32(look.track));
                if fraction > 0.0 {
                    let fill = egui::Rect::from_min_size(rect.min, vec2(rect.width() * fraction, rect.height()));
                    painter.rect_filled(fill, radius, color32(look.fill));
                }
                painter.rect_stroke(rect, radius, outline, StrokeKind::Inside);
                if let Some(goal) = self.goal {
                    let x = rect.left() + rect.width() * meter_fraction(goal, self.max);
                    painter.vline(x, rect.y_range().expand(2.0), Stroke::new(2.0, color32(look.goal)));
                }
            }
            UiMeterKind::Ring => {
                let thickness = (rect.width().min(rect.height()) * 0.14).max(2.0);
                let radius = rect.width().min(rect.height()) * 0.5 - thickness * 0.5 - 1.0;
                let center = rect.center();
                painter.circle_stroke(center, radius, Stroke::new(thickness, color32(look.track)));
                let points: Vec<egui::Pos2> =
                    ring_arc_points(Vec2::new(center.x, center.y), radius, fraction, 64).into_iter().map(|point| pos2(point.x, point.y)).collect();
                if points.len() > 1 {
                    painter.add(egui::Shape::line(points, Stroke::new(thickness, color32(look.fill))));
                }
                if look.outline.is_some() {
                    painter.circle_stroke(center, radius + thickness * 0.5, outline);
                    painter.circle_stroke(center, radius - thickness * 0.5, outline);
                }
                let text = match &self.text {
                    Some(text) => text.resolve().to_string(),
                    None => format!("{}", self.value.round() as i64),
                };
                let color = look.text.map_or_else(|| ui.visuals().text_color(), color32);
                painter.text(center, Align2::CENTER_CENTER, text, font.clone(), color);
            }
            UiMeterKind::Steps(steps) => {
                let steps = steps.max(1);
                let filled = meter_filled_steps(self.value, self.max, steps);
                let gap = (rect.width() * 0.02).clamp(2.0, 6.0);
                let width = ((rect.width() - gap * (steps - 1) as f32) / steps as f32).max(1.0);
                for step in 0..steps {
                    let step_rect = egui::Rect::from_min_size(pos2(rect.left() + (width + gap) * step as f32, rect.top()), vec2(width, rect.height()));
                    let color = if step < filled { look.fill } else { look.track };
                    painter.rect_filled(step_rect, 2.0, color32(color));
                    painter.rect_stroke(step_rect, 2.0, outline, StrokeKind::Inside);
                }
            }
        }
    }
}

// ----------------------------------------------
// Unit tests
// ----------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meter_maths() {
        assert_eq!(meter_fraction(5.0, 10.0), 0.5);
        assert_eq!(meter_fraction(-1.0, 10.0), 0.0);
        assert_eq!(meter_fraction(11.0, 10.0), 1.0);
        assert_eq!(meter_fraction(1.0, 0.0), 0.0);
        assert_eq!(meter_fraction(f32::NAN, 1.0), 0.0);

        assert_eq!(meter_filled_steps(3.0, 5.0, 5), 3);
        assert_eq!(meter_filled_steps(62.0, 100.0, 5), 3); // 3.1 steps.
        assert_eq!(meter_filled_steps(70.0, 100.0, 5), 4); // 3.5 rounds up.
        assert_eq!(meter_filled_steps(200.0, 100.0, 5), 5);
        assert_eq!(meter_filled_steps(0.0, 100.0, 5), 0);
    }

    #[test]
    fn ring_arcs_start_at_the_top_and_turn_clockwise() {
        assert!(ring_arc_points(Vec2::zero(), 10.0, 0.0, 64).is_empty());

        let quarter = ring_arc_points(Vec2::zero(), 10.0, 0.25, 64);
        assert_eq!(quarter.len(), 17);
        let (first, last) = (quarter[0], *quarter.last().unwrap());
        assert!(first.x.abs() < 1e-4 && (first.y + 10.0).abs() < 1e-4, "{first:?}"); // Top.
        assert!((last.x - 10.0).abs() < 1e-4 && last.y.abs() < 1e-4, "{last:?}"); // Right.

        let full = ring_arc_points(Vec2::new(5.0, 5.0), 2.0, 1.0, 8);
        assert_eq!(full.len(), 9);
        assert!((full[0].x - full[8].x).abs() < 1e-4 && (full[0].y - full[8].y).abs() < 1e-4);
        assert_eq!(ring_arc_points(Vec2::zero(), 1.0, 0.001, 8).len(), 2); // At least one segment.
    }
}
