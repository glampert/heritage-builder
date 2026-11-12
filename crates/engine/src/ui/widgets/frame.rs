use std::any::Any;
use egui::{Sense, vec2};

use common::{Color, Vec2};

use super::{UiWidget, UiWidgetContext, UiWidgetGroup};
use crate::ui::{UiNineSlice, UiString, immediate::{self, color32, nine_slice_shape}};

// ----------------------------------------------
// UiFrame
// ----------------------------------------------

// A widget group on a panel, in a rect of its own size: a screen's bar or side panel. The panel (a
// 9-slice, or a plain fill) goes over the whole rect, and the group is laid out inside its padding
// (left, top, right, bottom). A width or height of 0 takes the container's. A labeled frame adds
// its label to its widgets' paths ("WorldMap/panel/name") and records its inner rect as their
// content area (kind menu_content, as a menu's), which ui.layout_issues holds them to.
pub struct UiFrame {
    label: UiString,
    contents: UiWidgetGroup,
    panel: Option<UiNineSlice>,
    fill: Option<Color>,
    rounding: f32,
    outlines: Vec<UiFrameOutline>,
    size: Vec2,
    padding: [f32; 4],
}

// A line round a frame's rect, `inset` points in, solid or in dashes (`dash`: length and gap).
#[derive(Copy, Clone, Debug)]
pub struct UiFrameOutline {
    pub color: Color,
    pub width: f32,
    pub inset: f32,
    pub dash: Option<[f32; 2]>,
}

impl UiWidget for UiFrame {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut dyn UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        let ui_sys = context.ui_sys();
        let rect = ui_sys.with_ui(|ui| {
            let available = immediate::fill_size(ui);
            let size = vec2(
                if self.size.x > 0.0 { self.size.x } else { available.x.max(1.0) },
                if self.size.y > 0.0 { self.size.y } else { available.y.max(1.0) },
            );
            let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
            if ui.is_rect_visible(rect) {
                if let Some(fill) = self.fill {
                    ui.painter().rect_filled(rect, self.rounding, color32(fill));
                }
                if let Some(panel) = &self.panel {
                    ui.painter().add(nine_slice_shape(rect, panel, egui::Color32::WHITE));
                }
                for outline in &self.outlines {
                    let line = rect.shrink(outline.inset + outline.width * 0.5);
                    let stroke = egui::Stroke::new(outline.width, color32(outline.color));
                    match outline.dash {
                        None => {
                            ui.painter().rect_stroke(line, 0.0, stroke, egui::StrokeKind::Middle);
                        }
                        Some([dash, gap]) => {
                            let points = [line.left_top(), line.right_top(), line.right_bottom(), line.left_bottom(), line.left_top()];
                            ui.painter().extend(egui::Shape::dashed_line(&points, stroke, dash, gap));
                        }
                    }
                }
            }
            rect
        });

        let [left, top, right, bottom] = self.padding;
        let inner = egui::Rect::from_min_max(rect.min + vec2(left, top), rect.max - vec2(right, bottom));
        // A labeled frame's inner rect is its widgets' content area, for the layout check, as a
        // menu's is (kind menu_content), listed before them.
        if !self.label.is_empty() && ui_sys.is_widget_registry_enabled() {
            immediate::record_rect(ui_sys, "menu_content", self.label.resolve(), inner, false, Some(serde_json::json!({ "scrolls": false })));
        }
        if inner.is_positive() {
            let _path = (!self.label.is_empty()).then(|| immediate::path_scope(ui_sys, self.label.resolve()));
            ui_sys.draw_at(inner, |_| self.contents.draw(context));
        }
    }

    fn measure(&self, _context: &dyn UiWidgetContext) -> Vec2 {
        self.size
    }

    fn label(&self) -> &str {
        self.label.resolve()
    }
}

impl UiFrame {
    // `contents` in a frame `size` points large (0: the container's), with no panel yet.
    pub fn new(contents: UiWidgetGroup, size: Vec2) -> Self {
        Self { label: UiString::default(), contents, panel: None, fill: None, rounding: 0.0, outlines: Vec::new(), size, padding: [0.0; 4] }
    }

    // Names it in its widgets' paths.
    pub fn with_label(mut self, label: impl Into<UiString>) -> Self {
        self.label = label.into();
        self
    }

    pub fn with_panel(mut self, panel: UiNineSlice) -> Self {
        self.panel = Some(panel);
        self
    }

    // A plain fill under the panel (or alone), with rounded corners.
    pub fn with_fill(mut self, fill: Color, rounding: f32) -> Self {
        self.fill = Some(fill);
        self.rounding = rounding;
        self
    }

    // A line round it, over its panel, under its contents.
    pub fn with_outline(mut self, outline: UiFrameOutline) -> Self {
        self.outlines.push(outline);
        self
    }

    // Room between the frame's edges and its contents, in points: left, top, right, bottom.
    pub fn with_padding(mut self, padding: [f32; 4]) -> Self {
        debug_assert!(padding.iter().all(|side| *side >= 0.0));
        self.padding = padding;
        self
    }

    pub fn contents(&self) -> &UiWidgetGroup {
        &self.contents
    }

    pub fn contents_mut(&mut self) -> &mut UiWidgetGroup {
        &mut self.contents
    }
}
