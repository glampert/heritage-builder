use std::any::Any;
use egui::{pos2, vec2};

use common::Vec2;

use super::{UiWidget, UiWidgetContext, UiWidgetGroup};
use crate::ui::{UiSystem, immediate};

// ----------------------------------------------
// UiColumnWidth
// ----------------------------------------------

// A column's width: fixed, in points, or a share of what the fixed columns leave (fractions are
// weights: 1 and 2 split the rest a third and two thirds).
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum UiColumnWidth {
    Fixed(f32),
    Fraction(f32),
}

// The widths of `columns` across `total` points with `spacing` between them. Fixed columns keep
// their width (shrunk by one factor if they don't fit), and fractional ones share the rest by
// weight; a column is never negative.
pub fn split_column_widths(total: f32, columns: &[UiColumnWidth], spacing: f32) -> Vec<f32> {
    if columns.is_empty() {
        return Vec::new();
    }

    let room = (total - spacing * (columns.len() - 1) as f32).max(0.0);
    let fixed: f32 = columns.iter().map(|column| if let UiColumnWidth::Fixed(width) = column { width.max(0.0) } else { 0.0 }).sum();
    let weights: f32 = columns.iter().map(|column| if let UiColumnWidth::Fraction(weight) = column { weight.max(0.0) } else { 0.0 }).sum();

    let fixed_scale = if fixed > room && fixed > 0.0 { room / fixed } else { 1.0 };
    let rest = (room - fixed * fixed_scale).max(0.0);

    columns
        .iter()
        .map(|column| match *column {
            UiColumnWidth::Fixed(width) => width.max(0.0) * fixed_scale,
            UiColumnWidth::Fraction(weight) if weights > 0.0 => rest * weight.max(0.0) / weights,
            UiColumnWidth::Fraction(_) => 0.0,
        })
        .collect()
}

// ----------------------------------------------
// UiColumns
// ----------------------------------------------

#[derive(Copy, Clone)]
pub struct UiColumnsColumnIndex(usize);

// Columns side by side across the menu (or a given width), each a widget group laid out in its
// own column: two- and three-column dialogs (Custom Game: realm and map, preview, options). As
// tall as the tallest column. Not recorded itself: its widgets are.
pub struct UiColumns {
    columns: Vec<(UiColumnWidth, UiWidgetGroup)>,
    spacing: f32,
    width: Option<f32>, // The menu's width if None.
}

impl UiWidget for UiColumns {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut dyn UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        let ui_sys = context.ui_sys();
        let (origin, width, rects) = self.column_rects(ui_sys);
        let height = self.measure_columns(context, &rects);

        for ((_, group), rect) in self.columns.iter_mut().zip(rects) {
            ui_sys.draw_at(rect, |_| group.draw(context));
        }

        let rect = egui::Rect::from_min_size(origin, vec2(width, height));
        ui_sys.with_ui(|ui| ui.advance_cursor_after_rect(rect));
    }

    // Measured as drawn: each column in its own width, so wrapped text and contents that follow
    // their container's width (images, fill-width rows) report the height they'll take. Without
    // an open UI (nothing to lay out in) each column is measured at the whole width.
    fn measure(&self, context: &dyn UiWidgetContext) -> Vec2 {
        let ui_sys = context.ui_sys();
        if !ui_sys.has_ui() {
            let width = self.width.unwrap_or(0.0);
            let height = self.columns.iter().map(|(_, group)| group.measure(context).y).fold(0.0, f32::max);
            return Vec2::new(width, height);
        }

        let (_, width, rects) = self.column_rects(ui_sys);
        Vec2::new(width, self.measure_columns(context, &rects))
    }
}

impl Default for UiColumns {
    fn default() -> Self {
        Self::new()
    }
}

impl UiColumns {
    // No columns yet, 16 points apart, across the menu.
    pub fn new() -> Self {
        Self { columns: Vec::new(), spacing: 16.0, width: None }
    }

    // Between columns, in points.
    pub fn with_spacing(mut self, spacing: f32) -> Self {
        debug_assert!(spacing >= 0.0);
        self.spacing = spacing;
        self
    }

    pub fn with_width(mut self, width: f32) -> Self {
        debug_assert!(width > 0.0);
        self.width = Some(width);
        self
    }

    // A column of `width` holding `group`, after the others. Groups center their widgets unless
    // told otherwise (UiWidgetGroup::with_center_horizontally(false) for a column of rows).
    pub fn with_column(mut self, width: UiColumnWidth, group: UiWidgetGroup) -> Self {
        self.add_column(width, group);
        self
    }

    pub fn add_column(&mut self, width: UiColumnWidth, group: UiWidgetGroup) -> UiColumnsColumnIndex {
        self.columns.push((width, group));
        UiColumnsColumnIndex(self.columns.len() - 1)
    }

    pub fn column(&self, index: UiColumnsColumnIndex) -> &UiWidgetGroup {
        &self.columns[index.0].1
    }

    pub fn column_mut(&mut self, index: UiColumnsColumnIndex) -> &mut UiWidgetGroup {
        &mut self.columns[index.0].1
    }

    // Where the columns go: the top left corner, the whole width and each column's rect.
    fn column_rects(&self, ui_sys: &UiSystem) -> (egui::Pos2, f32, Vec<egui::Rect>) {
        let (origin, available) = ui_sys.with_ui(|ui| (ui.cursor().min, immediate::fill_size(ui)));
        let width = self.width.unwrap_or(available.x);
        let widths = split_column_widths(width, &self.column_widths(), self.spacing);

        let mut x = origin.x;
        let rects = widths
            .iter()
            .map(|column_width| {
                let rect = egui::Rect::from_min_size(pos2(x, origin.y), vec2(*column_width, available.y.max(1.0)));
                x += column_width + self.spacing;
                rect
            })
            .collect();
        (origin, width, rects)
    }

    // The tallest column, each measured in its own rect, so widgets that fill their container fit it.
    fn measure_columns(&self, context: &dyn UiWidgetContext, rects: &[egui::Rect]) -> f32 {
        let ui_sys = context.ui_sys();
        self.columns
            .iter()
            .zip(rects)
            .map(|((_, group), rect)| ui_sys.draw_at(*rect, |_| group.measure(context)).0.y)
            .fold(0.0, f32::max)
    }

    fn column_widths(&self) -> Vec<UiColumnWidth> {
        self.columns.iter().map(|(width, _)| *width).collect()
    }
}

// ----------------------------------------------
// Unit tests
// ----------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use UiColumnWidth::{Fixed, Fraction};

    #[test]
    fn fractions_share_what_fixed_columns_leave() {
        assert_eq!(split_column_widths(320.0, &[Fixed(100.0), Fraction(1.0), Fraction(1.0)], 10.0), [100.0, 100.0, 100.0]);
        assert_eq!(split_column_widths(310.0, &[Fraction(1.0), Fraction(2.0)], 10.0), [100.0, 200.0]);
        assert_eq!(split_column_widths(200.0, &[Fixed(50.0), Fixed(70.0)], 0.0), [50.0, 70.0]);
    }

    #[test]
    fn fixed_columns_shrink_to_fit_and_nothing_goes_negative() {
        assert_eq!(split_column_widths(110.0, &[Fixed(100.0), Fixed(100.0), Fraction(1.0)], 5.0), [50.0, 50.0, 0.0]);
        assert_eq!(split_column_widths(5.0, &[Fraction(1.0), Fraction(1.0)], 10.0), [0.0, 0.0]);
        assert_eq!(split_column_widths(100.0, &[Fraction(0.0), Fixed(-5.0)], 0.0), [0.0, 0.0]);
        assert!(split_column_widths(100.0, &[], 10.0).is_empty());
    }
}
