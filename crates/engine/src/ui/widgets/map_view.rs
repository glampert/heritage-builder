use std::any::Any;
use egui::Sense;

use common::{Rect, Vec2};
use serde_json::{Value, json};

use super::{UiTooltipText, UiWidget, UiWidgetContext};
use crate::{
    render::texture::TextureHandle,
    ui::{UiPainter, UiString, UiSystem, immediate::{self, egui_vec2, to_rect}},
};

// ----------------------------------------------
// UiMapViewTransform
// ----------------------------------------------

// Where a map view shows its map this frame: the view's rect in window points, and the window
// point of the map's origin (its top left) and how many points one map unit takes. Markers drawn
// over the map go through it, so they follow pans and zooms while keeping their own size.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct UiMapViewTransform {
    pub rect: Rect,
    pub origin: Vec2,
    pub points_per_unit: f32,
}

impl UiMapViewTransform {
    // The window point of a point on the map, in map units.
    #[inline]
    pub fn to_screen(&self, position: Vec2) -> Vec2 {
        Vec2::new(self.origin.x + position.x * self.points_per_unit, self.origin.y + position.y * self.points_per_unit)
    }

    // The map point, in map units, under a window point.
    #[inline]
    pub fn to_map(&self, point: Vec2) -> Vec2 {
        let ppu = self.points_per_unit.max(f32::EPSILON);
        Vec2::new((point.x - self.origin.x) / ppu, (point.y - self.origin.y) / ppu)
    }
}

// ----------------------------------------------
// UiMapViewState
// ----------------------------------------------

// A map view's view: its zoom (1 shows the whole map, fit to the view; more is closer) and the map
// point, in map units, at the view's centre.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct UiMapViewState {
    pub zoom: f32,
    pub centre: Vec2,
}

// The pointer over a map view this frame: where it is (None unless it's over the view and nothing
// covers it there), and whether the primary button clicked there.
#[derive(Copy, Clone, Default, PartialEq, Debug)]
pub struct UiMapViewPointer {
    pub position: Option<Vec2>,
    pub clicked: bool,
}

// The map points per window point at zoom 1 for a map of `extent` units in `rect`: as large as
// covers the rect (a map of the rect's shape fits it whole).
pub fn map_fit_points_per_unit(rect: Rect, extent: Vec2) -> f32 {
    let x = rect.width() / extent.x.max(f32::EPSILON);
    let y = rect.height() / extent.y.max(f32::EPSILON);
    x.max(y).max(f32::EPSILON)
}

// `state` kept within its limits for a map of `extent` units in `rect`: its zoom from 1 to
// `max_zoom`, and its centre where the view stays on the map, covering the rect.
pub fn clamp_map_view(state: UiMapViewState, rect: Rect, extent: Vec2, max_zoom: f32) -> UiMapViewState {
    let zoom = if state.zoom.is_finite() { state.zoom.clamp(1.0, max_zoom.max(1.0)) } else { 1.0 };
    if zoom <= 1.0 {
        // The fit: the whole map, centred (a map a little off the view's shape loses as much of
        // each side).
        return UiMapViewState { zoom, centre: Vec2::new(extent.x * 0.5, extent.y * 0.5) };
    }
    let ppu = map_fit_points_per_unit(rect, extent) * zoom;
    let half = Vec2::new(rect.width() * 0.5 / ppu, rect.height() * 0.5 / ppu);
    let clamp_axis = |centre: f32, half: f32, extent: f32| {
        if half * 2.0 >= extent || !centre.is_finite() { extent * 0.5 } else { centre.clamp(half, extent - half) }
    };
    UiMapViewState { zoom, centre: Vec2::new(clamp_axis(state.centre.x, half.x, extent.x), clamp_axis(state.centre.y, half.y, extent.y)) }
}

// The transform of a view of `state` in `rect`.
pub fn map_view_transform(state: UiMapViewState, rect: Rect, extent: Vec2) -> UiMapViewTransform {
    let ppu = map_fit_points_per_unit(rect, extent) * state.zoom;
    let center = rect.center();
    UiMapViewTransform { rect, origin: Vec2::new(center.x - state.centre.x * ppu, center.y - state.centre.y * ppu), points_per_unit: ppu }
}

// `state` zoomed to `zoom` about the window point `anchor`: the map point under it stays there.
pub fn zoom_map_view_about(state: UiMapViewState, zoom: f32, anchor: Vec2, rect: Rect, extent: Vec2, max_zoom: f32) -> UiMapViewState {
    let before = map_view_transform(state, rect, extent).to_map(anchor);
    let zoomed = clamp_map_view(UiMapViewState { zoom, centre: state.centre }, rect, extent, max_zoom);
    let ppu = map_fit_points_per_unit(rect, extent) * zoomed.zoom;
    let center = rect.center();
    let centre = Vec2::new(before.x - (anchor.x - center.x) / ppu, before.y - (anchor.y - center.y) / ppu);
    clamp_map_view(UiMapViewState { zoom: zoomed.zoom, centre }, rect, extent, max_zoom)
}

// The view after a drag of `drag` points: the map moves with the cursor, kept on the map (at the
// fit, where the whole map shows, it doesn't move).
pub fn drag_map_view(state: UiMapViewState, drag: Vec2, rect: Rect, extent: Vec2, max_zoom: f32) -> UiMapViewState {
    let points_per_unit = map_fit_points_per_unit(rect, extent) * state.zoom;
    let centre = Vec2::new(state.centre.x - drag.x / points_per_unit, state.centre.y - drag.y / points_per_unit);
    clamp_map_view(UiMapViewState { centre, ..state }, rect, extent, max_zoom)
}

// Records a marker drawn over a map view (UiMapView's overlay): a city, a site, anything the
// player can point at, by its rect in window points, for ui.tree and ui.click. Its path is the
// menu's (and labeled groups') plus `label`. Not visible (so ui.click won't aim at it) unless the
// whole rect is in the view's.
pub fn record_map_marker(ui_sys: &UiSystem, label: &str, rect: Rect, view_rect: Rect, hovered: bool, value: Option<Value>) {
    record_map_item(ui_sys, "map_marker", label, rect, view_rect, hovered, value);
}

// As record_map_marker, for something else an overlay draws and the pointer acts on, of `kind`
// (the world map's choice box and its sites' names in it).
pub fn record_map_item(ui_sys: &UiSystem, kind: &'static str, label: &str, rect: Rect, view_rect: Rect, hovered: bool, value: Option<Value>) {
    if ui_sys.is_widget_registry_enabled() {
        let visible = view_rect.contains_rect(&rect);
        immediate::record_rect_visible(ui_sys, kind, label, immediate::egui_rect(rect), hovered, visible, value);
    }
}

// ----------------------------------------------
// UiMapView
// ----------------------------------------------

pub type UiMapViewOverlay = Box<dyn FnMut(&mut dyn UiWidgetContext, &UiPainter, &UiMapViewTransform, &UiMapViewPointer)>;
pub type UiMapViewChanged = Box<dyn FnMut(UiMapViewState)>;

// The art of a frame drawn around a map view, over its edges: a texture whose `map_rect` (pixels)
// is where the view's rect goes, the rest reaching past it.
#[derive(Copy, Clone, Debug)]
struct UiMapViewFrame {
    texture: TextureHandle,
    texture_size: Vec2,
    map_rect: Rect,
}

// A map that pans and zooms in a fixed rect: the world map. Its base is a texture of which
// `extent` map units take `pixels_per_unit` pixels each from the top left (the rest of the texture
// is past the map). At zoom 1 the map fits the view (covering it: a map of the view's shape shows
// whole); the mouse wheel zooms in about the cursor, up to a maximum, and back out to 1, never
// past it; while zoomed in, dragging it with the primary button pans it (the drag goes on while
// the cursor leaves the view), and the view always stays on the map. A press and release without
// a drag is a click. An overlay callback draws what goes over the map each frame (markers,
// labels, routes) through the view's transform, in a painter clipped to the view, and learns where
// the pointer is and whether it clicked; a frame texture may go around it, over its edges.
// Recorded as kind "map_view", its value {"zoom", "centre": [x, y] (map units), "fit", "max_zoom"};
// ui.set takes {"zoom", "centre"} (either) and keeps it in the limits; ui.drag pans it.
pub struct UiMapView {
    label: UiString,
    texture: Option<TextureHandle>,
    texture_size: Vec2,
    extent: Vec2,
    pixels_per_unit: f32,
    size: Vec2,
    frame: Option<UiMapViewFrame>,
    state: UiMapViewState,
    max_zoom: f32,
    wheel_step: f32, // The zoom's factor per wheel step.
    drag_pan: bool,
    overlay: Option<UiMapViewOverlay>,
    on_view_changed: Option<UiMapViewChanged>,
    last_rect: Option<Rect>,
}

impl UiWidget for UiMapView {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut dyn UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        let ui_sys = context.ui_sys();
        let (rect, response, wheel_lines, painter, window_rect) = ui_sys.with_ui(|ui| {
            let (rect, response) = ui.allocate_exact_size(egui_vec2(self.size), Sense::click_and_drag());
            let wheel_lines = if response.hovered() { wheel_lines(ui) } else { 0.0 };
            let window_rect = to_rect(ui.ctx().content_rect());
            (to_rect(rect), response, wheel_lines, ui.painter().with_clip_rect(ui.clip_rect().intersect(rect)), window_rect)
        });
        ui_sys.set_last_response(&response);

        let before = self.state;
        let pointer = response.hover_pos().map(|pos| Vec2::new(pos.x, pos.y)).filter(|_| response.hovered());
        let drag = if response.dragged_by(egui::PointerButton::Primary) {
            let delta = response.drag_delta();
            Vec2::new(delta.x, delta.y)
        } else {
            Vec2::zero()
        };
        self.update_view(rect, pointer, wheel_lines, drag);

        let transform = map_view_transform(self.state, rect, self.extent);
        let painter = UiPainter::from_egui(painter, ui_sys);

        if let Some(texture) = self.texture {
            let map_rect = Rect::from_pos_and_size(transform.origin, Vec2::new(self.extent.x * transform.points_per_unit, self.extent.y * transform.points_per_unit));
            let uv_size = Vec2::new(
                (self.extent.x * self.pixels_per_unit / self.texture_size.x).min(1.0),
                (self.extent.y * self.pixels_per_unit / self.texture_size.y).min(1.0),
            );
            painter.image_uv(texture, map_rect, Rect::from_pos_and_size(Vec2::zero(), uv_size), common::Color::white());
        }

        if let Some(overlay) = &mut self.overlay {
            let pointer = UiMapViewPointer { position: pointer, clicked: response.clicked() };
            overlay(context, &painter, &transform, &pointer);
        }

        if let Some(frame) = self.frame {
            let scale = rect.width() / frame.map_rect.width().max(1.0);
            let min = Vec2::new(rect.min.x - frame.map_rect.min.x * scale, rect.min.y - frame.map_rect.min.y * scale);
            let frame_rect = Rect::from_pos_and_size(min, Vec2::new(frame.texture_size.x * scale, frame.texture_size.y * scale));
            painter.unclipped(window_rect).image(frame.texture, frame_rect, common::Color::white());
        }

        let value = || Some(self.value());
        let pending = ui_sys.with_ui(|ui| immediate::record_lazy(ui_sys, ui, &response, "map_view", self.label.resolve(), None, true, value));
        if let Some(pending) = pending {
            self.apply_value(&pending, rect);
        }

        self.last_rect = Some(rect);
        if self.state != before
            && let Some(on_view_changed) = &mut self.on_view_changed
        {
            on_view_changed(self.state);
        }
    }

    fn measure(&self, _context: &dyn UiWidgetContext) -> Vec2 {
        self.size
    }

    fn label(&self) -> &str {
        self.label.resolve()
    }
}

impl UiMapView {
    // A view `size` points large of a map `extent` units large, whose base `texture` (`texture_size`
    // pixels) has `pixels_per_unit` pixels per map unit. At zoom 1, centred.
    pub fn new(texture: Option<TextureHandle>, texture_size: Vec2, extent: Vec2, pixels_per_unit: f32, size: Vec2) -> Self {
        debug_assert!(extent.x > 0.0 && extent.y > 0.0 && pixels_per_unit > 0.0);
        Self {
            label: UiString::default(),
            texture,
            texture_size: Vec2::new(texture_size.x.max(1.0), texture_size.y.max(1.0)),
            extent,
            pixels_per_unit,
            size,
            frame: None,
            state: UiMapViewState { zoom: 1.0, centre: Vec2::new(extent.x * 0.5, extent.y * 0.5) },
            max_zoom: 3.0,
            wheel_step: 1.2,
            drag_pan: true,
            overlay: None,
            on_view_changed: None,
            last_rect: None,
        }
    }

    // Names it in widget paths ("<menu>/map_view" without one).
    pub fn with_label(mut self, label: impl Into<UiString>) -> Self {
        self.label = label.into();
        self
    }

    // A frame around the view: `texture` (`texture_size` pixels) drawn so its `map_rect` (pixels)
    // covers the view's rect.
    pub fn with_frame(mut self, texture: TextureHandle, texture_size: Vec2, map_rect: Rect) -> Self {
        self.frame = Some(UiMapViewFrame { texture, texture_size, map_rect });
        self
    }

    // The view it starts with (kept in its limits once it knows its rect).
    pub fn with_state(mut self, state: UiMapViewState) -> Self {
        self.state = state;
        self
    }

    // How far it zooms in, and by what factor per wheel step.
    pub fn with_zoom_limits(mut self, max_zoom: f32, wheel_step: f32) -> Self {
        debug_assert!(max_zoom >= 1.0 && wheel_step > 1.0);
        self.max_zoom = max_zoom;
        self.wheel_step = wheel_step;
        self
    }

    // Pans while zoomed in when dragged with the primary button (the default); false: only the
    // wheel and ui.set move it.
    pub fn with_drag_pan(mut self, drag_pan: bool) -> Self {
        self.drag_pan = drag_pan;
        self
    }

    // Draws over the map each frame, after the base and before the frame.
    pub fn with_overlay<F>(mut self, overlay: F) -> Self
    where
        F: FnMut(&mut dyn UiWidgetContext, &UiPainter, &UiMapViewTransform, &UiMapViewPointer) + 'static,
    {
        self.overlay = Some(Box::new(overlay));
        self
    }

    // Told the view each time it changes (a zoom, a pan, ui.set).
    pub fn on_view_changed<F>(mut self, f: F) -> Self
    where
        F: FnMut(UiMapViewState) + 'static,
    {
        self.on_view_changed = Some(Box::new(f));
        self
    }

    pub fn state(&self) -> UiMapViewState {
        self.state
    }

    pub fn max_zoom(&self) -> f32 {
        self.max_zoom
    }

    // Sets the view, kept in its limits (once the view has been drawn and knows its rect).
    pub fn set_state(&mut self, state: UiMapViewState) {
        self.state = match self.last_rect {
            Some(rect) => clamp_map_view(state, rect, self.extent, self.max_zoom),
            None => state,
        };
    }

    // A wheel zoom about the cursor, then a drag's pan: the map follows the cursor, the point
    // under it staying under it.
    fn update_view(&mut self, rect: Rect, pointer: Option<Vec2>, wheel_lines: f32, drag: Vec2) {
        let mut state = clamp_map_view(self.state, rect, self.extent, self.max_zoom);
        if let Some(cursor) = pointer
            && wheel_lines != 0.0
        {
            let zoom = state.zoom * self.wheel_step.powf(wheel_lines);
            state = zoom_map_view_about(state, zoom, cursor, rect, self.extent, self.max_zoom);
        }
        if self.drag_pan && drag != Vec2::zero() {
            state = drag_map_view(state, drag, rect, self.extent, self.max_zoom);
        }
        self.state = state;
    }

    fn value(&self) -> Value {
        json!({
            "zoom": self.state.zoom,
            "centre": [self.state.centre.x, self.state.centre.y],
            "fit": self.state.zoom <= 1.0,
            "max_zoom": self.max_zoom,
        })
    }

    // ui.set's value: {"zoom": z, "centre": [x, y]}, either or both.
    fn apply_value(&mut self, value: &Value, rect: Rect) {
        let mut state = self.state;
        if let Some(zoom) = value.get("zoom").and_then(Value::as_f64) {
            state.zoom = zoom as f32;
        }
        if let Some([x, y]) = value.get("centre").and_then(Value::as_array).map(|centre| centre.iter().filter_map(Value::as_f64).collect::<Vec<_>>()).and_then(|centre| <[f64; 2]>::try_from(centre).ok()) {
            state.centre = Vec2::new(x as f32, y as f32);
        }
        self.state = clamp_map_view(state, rect, self.extent, self.max_zoom);
    }
}

// This frame's mouse wheel steps, up positive. The engine hands egui every wheel event in lines
// (UiBackend::on_scroll), but what a notch is worth differs: one line on GLFW, and on the web
// and Winit a pixel delta over 20, so a browser's notch of 100 pixels or more is 5 lines or more.
// Each event counts one step at most, so a notch zooms one step everywhere, while a touchpad's
// small deltas still zoom by a fraction of one.
fn wheel_lines(ui: &egui::Ui) -> f32 {
    ui.input(|input| {
        input
            .events
            .iter()
            .map(|event| match event {
                egui::Event::MouseWheel { delta, .. } => wheel_event_steps(delta.y),
                _ => 0.0,
            })
            .sum()
    })
}

// One wheel event's steps: its delta, at most one step either way.
fn wheel_event_steps(delta: f32) -> f32 {
    delta.clamp(-1.0, 1.0)
}

// ----------------------------------------------
// UiCanvas
// ----------------------------------------------

pub type UiCanvasPaint = Box<dyn FnMut(&mut dyn UiWidgetContext, &UiPainter, Rect)>;

// A rect painted by a callback each frame: a legend's glyphs, a coin, the HUD's stats. Recorded as
// kind "canvas" when labeled, with the value it's given, and its tooltip shown while it's hovered.
pub struct UiCanvas {
    label: UiString,
    size: Vec2,
    paint: Option<UiCanvasPaint>,
    value: Option<Value>,
    tooltip: Option<UiTooltipText>,
}

impl UiWidget for UiCanvas {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut dyn UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        let ui_sys = context.ui_sys();
        let (rect, response, painter) = ui_sys.with_ui(|ui| {
            let (rect, response) = ui.allocate_exact_size(egui_vec2(self.size), Sense::hover());
            (to_rect(rect), response, ui.painter().clone())
        });
        if let Some(paint) = &mut self.paint {
            let painter = UiPainter::from_egui(painter, ui_sys);
            paint(context, &painter, rect);
        }
        if !self.label.is_empty() {
            let value = &self.value;
            let tooltip = self.tooltip.as_ref().map(|tooltip| tooltip.text.resolve());
            ui_sys.with_ui(|ui| immediate::record_lazy(ui_sys, ui, &response, "canvas", self.label.resolve(), tooltip, true, || value.clone()));
        } else {
            ui_sys.set_last_response(&response);
        }
        if response.hovered()
            && let Some(tooltip) = &self.tooltip
        {
            tooltip.draw(context);
        }
    }

    fn measure(&self, _context: &dyn UiWidgetContext) -> Vec2 {
        self.size
    }

    fn label(&self) -> &str {
        self.label.resolve()
    }
}

impl UiCanvas {
    pub fn new<F>(size: Vec2, paint: F) -> Self
    where
        F: FnMut(&mut dyn UiWidgetContext, &UiPainter, Rect) + 'static,
    {
        Self { label: UiString::default(), size, paint: Some(Box::new(paint)), value: None, tooltip: None }
    }

    // Shown while the pointer is over it.
    pub fn with_tooltip(mut self, tooltip: UiTooltipText) -> Self {
        self.tooltip = Some(tooltip);
        self
    }

    // The value it records, changed after it's built (the HUD's stats, each frame).
    pub fn set_value(&mut self, value: Option<Value>) {
        self.value = value;
    }

    // Names it in widget paths and records it, with `value`.
    pub fn with_label(mut self, label: impl Into<UiString>, value: Option<Value>) -> Self {
        self.label = label.into();
        self.value = value;
        self
    }
}

// ----------------------------------------------
// Unit tests
// ----------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
        Rect::from_pos_and_size(Vec2::new(x, y), Vec2::new(width, height))
    }

    const EXTENT: Vec2 = Vec2::new(1000.0, 672.0);

    fn close(a: Vec2, b: Vec2) -> bool {
        (a.x - b.x).abs() < 1e-3 && (a.y - b.y).abs() < 1e-3
    }

    // At zoom 1 a map of the view's shape fits it whole, centred, whatever centre was asked.
    #[test]
    fn fit_shows_the_whole_map() {
        let view = rect(52.0, 132.0, 1290.0, 866.88);
        let state = clamp_map_view(UiMapViewState { zoom: 0.5, centre: Vec2::new(10.0, 10.0) }, view, EXTENT, 3.0);
        assert_eq!(state.zoom, 1.0);
        assert!(close(state.centre, Vec2::new(500.0, 336.0)));
        let transform = map_view_transform(state, view, EXTENT);
        assert!(close(transform.to_screen(Vec2::zero()), view.min));
        assert!(close(transform.to_screen(EXTENT), view.max));
        assert!((transform.points_per_unit - 1.29).abs() < 1e-4);
    }

    // Zoomed in, the view stays on the map: its centre at least half a view from each edge.
    #[test]
    fn zoomed_views_stay_on_the_map() {
        let view = rect(0.0, 0.0, 1000.0, 672.0);
        let state = clamp_map_view(UiMapViewState { zoom: 2.0, centre: Vec2::new(0.0, 1000.0) }, view, EXTENT, 3.0);
        assert!(close(state.centre, Vec2::new(250.0, 504.0)));
        let state = clamp_map_view(UiMapViewState { zoom: 9.0, centre: Vec2::new(500.0, 300.0) }, view, EXTENT, 3.0);
        assert_eq!(state.zoom, 3.0);
    }

    // A wheel event zooms one step at most: a browser's notch (a pixel delta of 5 lines or more)
    // as a desktop's line, a touchpad's fraction of a line by that fraction.
    #[test]
    fn wheel_events_count_one_step_at_most() {
        assert_eq!(wheel_event_steps(5.0), 1.0);
        assert_eq!(wheel_event_steps(-10.0), -1.0);
        assert_eq!(wheel_event_steps(1.0), 1.0);
        assert_eq!(wheel_event_steps(0.25), 0.25);
    }

    // Zooming about a point keeps the map point under it there.
    #[test]
    fn zoom_about_the_cursor() {
        let view = rect(0.0, 0.0, 1000.0, 672.0);
        let fit = UiMapViewState { zoom: 1.0, centre: Vec2::new(500.0, 336.0) };
        let cursor = Vec2::new(600.0, 300.0);
        let zoomed = zoom_map_view_about(fit, 2.0, cursor, view, EXTENT, 3.0);
        assert_eq!(zoomed.zoom, 2.0);
        let under = map_view_transform(zoomed, view, EXTENT).to_map(cursor);
        assert!(close(under, Vec2::new(600.0, 300.0)), "{under:?}");
        // Back out past the fit: the fit, centred.
        let out = zoom_map_view_about(zoomed, 0.25, cursor, view, EXTENT, 3.0);
        assert_eq!(out, fit);
    }

    // A drag moves the map with the cursor, in map units at the zoom; the view stays on the map,
    // and at the fit a drag moves nothing.
    #[test]
    fn drag_pans() {
        let view = rect(0.0, 0.0, 1000.0, 672.0);
        let extent = Vec2::new(1000.0, 672.0);
        let state = UiMapViewState { zoom: 2.0, centre: Vec2::new(500.0, 336.0) };
        // Dragging left by 100 points at zoom 2 (2 points a unit) shows 50 units further east.
        assert!(close(drag_map_view(state, Vec2::new(-100.0, 0.0), view, extent, 3.0).centre, Vec2::new(550.0, 336.0)));
        assert!(close(drag_map_view(state, Vec2::new(0.0, 40.0), view, extent, 3.0).centre, Vec2::new(500.0, 316.0)));
        // Never past the map's edge: at zoom 2 the centre stays a quarter of the map in.
        assert!(close(drag_map_view(state, Vec2::new(-5000.0, 0.0), view, extent, 3.0).centre, Vec2::new(750.0, 336.0)));
        let fit = UiMapViewState { zoom: 1.0, centre: Vec2::new(500.0, 336.0) };
        assert!(close(drag_map_view(fit, Vec2::new(-100.0, 60.0), view, extent, 3.0).centre, fit.centre));
    }
}
