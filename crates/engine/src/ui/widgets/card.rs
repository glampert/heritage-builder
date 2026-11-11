use std::{any::Any, cell::Cell, rc::Rc};
use smallbox::smallbox;
use egui::{Color32, Sense, vec2};

use common::{Color, Vec2};

use super::{UiTooltipText, UiWidget, UiWidgetCallback, UiWidgetContext, UiWidgetGroup, widget_id};
use crate::ui::{
    UiNineSlice, UiString,
    immediate::{self, nine_slice_shape},
    internal,
    sound::{UiSoundEvent, UiSoundKey, UiWidgetSounds},
};

// ----------------------------------------------
// UiCard
// ----------------------------------------------

// The room a card's contents are measured in: their own height, whatever the menu has left.
const MEASURE_HEIGHT: f32 = 100_000.0;

// The height a row of cards shares, so cards side by side line up: each is drawn as tall as the
// tallest of the row was on the last pass (and its own height), so a row whose contents change
// size follows a frame later.
#[derive(Clone, Default)]
pub struct UiCardRow(Rc<Cell<UiCardRowHeights>>);

#[derive(Copy, Clone, Default)]
struct UiCardRowHeights {
    pass: u64,
    tallest: f32,      // On this pass, so far.
    tallest_last: f32, // On the last pass.
}

impl UiCardRow {
    pub fn new() -> Self {
        Self::default()
    }

    // The height to draw a card of `height` on egui pass `pass`, noting it for the row.
    fn height(&self, pass: u64, height: f32) -> f32 {
        let mut heights = self.0.get();
        if heights.pass != pass {
            heights = UiCardRowHeights { pass, tallest: 0.0, tallest_last: heights.tallest };
        }
        heights.tallest = heights.tallest.max(height);
        self.0.set(heights);
        height.max(heights.tallest_last)
    }

    // The tallest card of the row on the last pass that drew it.
    fn tallest(&self) -> f32 {
        let heights = self.0.get();
        heights.tallest.max(heights.tallest_last)
    }
}

pub type UiCardPressed = UiWidgetCallback<dyn Fn(&UiCard, &mut dyn UiWidgetContext)>;
pub type UiCardReadSelected = UiWidgetCallback<dyn Fn(&UiCard, &dyn UiWidgetContext) -> bool>;

// A panel around a group of widgets that the player picks by clicking anywhere on it, as the
// campaign picker's cards. Drawn on its panel, or on its selected panel while selected (asked of
// on_read_selected each frame, so a set of cards can show which one is picked), lit while hovered
// and faded while disabled, in the caller's tints (with_tints). As wide as its container, and as tall as its contents plus its padding,
// or its minimum height, or the tallest of its row (with_row). Recorded as kind "card", its value
// {"selected": ...}; its widgets' paths take its label (`<menu>/<card>/<widget>`). Plays
// ItemHovered and ItemSelected.
pub struct UiCard {
    label: UiString,
    contents: UiWidgetGroup,
    panel: Option<UiNineSlice>,
    selected_panel: Option<UiNineSlice>,
    padding: Vec2, // Between the panel's edges and the contents.
    hover_fill: Color32,    // Over a hovered card that isn't selected.
    pressed_tint: Color32,  // The panel's, while pressed.
    disabled_tint: Color32, // The panel's, while disabled.
    min_height: f32,
    row: Option<UiCardRow>,
    enabled: bool,
    tooltip: Option<UiTooltipText>,
    sounds: UiWidgetSounds, // Hovered: ItemHovered, Selected: ItemSelected.
    hovered: bool,
    on_read_selected: UiCardReadSelected,
    on_pressed: UiCardPressed,
}

impl UiWidget for UiCard {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut dyn UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        let ui_sys = context.ui_sys();
        let selected = self.is_selected(context);
        let enabled = self.enabled;
        let id = widget_id(self, self.label.id_source());

        // Its rect: as wide as its container, as tall as its contents laid out inside it.
        let (origin, width) = ui_sys.with_ui(|ui| (ui.cursor().min, immediate::fill_size(ui).x));
        let inner_width = (width - self.padding.x * 2.0).max(1.0);
        let measure_rect = egui::Rect::from_min_size(origin, vec2(inner_width, MEASURE_HEIGHT));
        let contents_height = ui_sys.draw_at(measure_rect, |_| self.contents.measure(context)).0.y;
        let mut height = (contents_height + self.padding.y * 2.0).max(self.min_height);
        if let Some(row) = &self.row
            && width > 0.0
        {
            // Not in a sizing pass, which lays the contents out with no width.
            height = row.height(ui_sys.ctx().cumulative_pass_nr(), height);
        }

        // The panel goes under the contents, painted once the click area knows it's hovered.
        let (rect, slot) = ui_sys.with_ui(|ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
            (rect, ui.painter().add(egui::Shape::Noop))
        });

        let inner = egui::Rect::from_min_size(rect.min + vec2(self.padding.x, self.padding.y), vec2(inner_width, height - self.padding.y * 2.0));
        {
            let _path = immediate::path_scope(ui_sys, self.label.resolve());
            ui_sys.draw_at(inner, |_| self.contents.draw(context));
        }

        // The click area over the whole card, after its contents, so it's the card that's hovered
        // and pressed wherever the cursor is on it.
        let (pressed, hovered) = ui_sys.with_ui(|ui| {
            let sense = if enabled { Sense::click() } else { Sense::hover() };
            let response = ui.interact(rect, id, sense);
            let lit = enabled && response.hovered();

            let panel = if selected { self.selected_panel.as_ref().or(self.panel.as_ref()) } else { self.panel.as_ref() };
            if let Some(panel) = panel
                && ui.is_rect_visible(rect)
            {
                let tint = match (enabled, lit, response.is_pointer_button_down_on()) {
                    (false, _, _) => self.disabled_tint,
                    (true, true, true) => self.pressed_tint,
                    _ => Color32::WHITE,
                };
                ui.painter().set(slot, nine_slice_shape(rect, panel, tint));
            }
            if lit && !selected {
                // A hovered card the selected panel doesn't show yet: a faint fill over its panel.
                ui.painter().rect_filled(rect.shrink(2.0), 6.0, self.hover_fill);
            }

            let tooltip = self.tooltip.as_ref().map(|tooltip| tooltip.text.resolve());
            let value = serde_json::json!({ "selected": selected });
            immediate::record(ui_sys, ui, &response, "card", self.label.resolve(), tooltip, enabled, Some(value));
            (response.clicked() && enabled, response.hovered())
        });

        if hovered {
            if !self.hovered && enabled && !pressed {
                self.sounds.play(context, UiSoundEvent::Hovered, Some(UiSoundKey::ItemHovered));
            }
            if let Some(tooltip) = &self.tooltip {
                tooltip.draw(context);
            }
        }
        self.hovered = hovered;

        if pressed {
            self.sounds.play(context, UiSoundEvent::Selected, Some(UiSoundKey::ItemSelected));
            if let Some(on_pressed) = &self.on_pressed {
                on_pressed(self, context);
            }
        }
    }

    // As wide as its container, its contents measured inside its padding, as they're drawn. Without
    // an open UI (nothing to lay out in) they're measured at the container's width.
    fn measure(&self, context: &dyn UiWidgetContext) -> Vec2 {
        let ui_sys = context.ui_sys();
        let width = internal::available_size(ui_sys).x;
        let contents = if ui_sys.has_ui() {
            let origin = ui_sys.with_ui(|ui| ui.cursor().min);
            let inner_width = (width - self.padding.x * 2.0).max(1.0);
            ui_sys.draw_at(egui::Rect::from_min_size(origin, vec2(inner_width, MEASURE_HEIGHT)), |_| self.contents.measure(context)).0
        } else {
            self.contents.measure(context)
        };
        let row = self.row.as_ref().map_or(0.0, UiCardRow::tallest);
        Vec2::new(width, (contents.y + self.padding.y * 2.0).max(self.min_height).max(row))
    }

    fn label(&self) -> &str {
        self.label.resolve()
    }
}

impl UiCard {
    // A card labeled `label` (its name in widget paths) holding `contents`, 16 points in from its
    // edges, with no panel yet.
    pub fn new(label: impl Into<UiString>, contents: UiWidgetGroup) -> Self {
        let label = label.into();
        debug_assert!(!label.is_empty());
        Self {
            label,
            contents,
            panel: None,
            selected_panel: None,
            padding: Vec2::new(16.0, 16.0),
            hover_fill: Color32::from_white_alpha(28),
            pressed_tint: Color32::from_gray(225),
            disabled_tint: Color32::from_white_alpha(150),
            min_height: 0.0,
            row: None,
            enabled: true,
            tooltip: None,
            sounds: UiWidgetSounds::new(),
            hovered: false,
            on_read_selected: None,
            on_pressed: None,
        }
    }

    // Its panel, and the one it's drawn on while selected (its panel if None).
    pub fn with_panels(mut self, panel: UiNineSlice, selected: Option<UiNineSlice>) -> Self {
        self.panel = Some(panel);
        self.selected_panel = selected;
        self
    }

    // Between its panel's edges and its contents, in points: across and down.
    pub fn with_padding(mut self, padding: Vec2) -> Self {
        debug_assert!(padding.x >= 0.0 && padding.y >= 0.0);
        self.padding = padding;
        self
    }

    // The fill over a hovered card that isn't selected, and the tints of its panel while it's
    // pressed and while it's disabled (a faint white, a light grey and a half-transparent white by
    // default).
    pub fn with_tints(mut self, hover_fill: Color, pressed_tint: Color, disabled_tint: Color) -> Self {
        self.hover_fill = immediate::color32(hover_fill);
        self.pressed_tint = immediate::color32(pressed_tint);
        self.disabled_tint = immediate::color32(disabled_tint);
        self
    }

    // The least it's drawn tall, so cards side by side line up.
    pub fn with_min_height(mut self, height: f32) -> Self {
        debug_assert!(height >= 0.0);
        self.min_height = height;
        self
    }

    // Drawn as tall as the tallest card of `row`: give each card side by side a clone of one row.
    pub fn with_row(mut self, row: UiCardRow) -> Self {
        self.row = Some(row);
        self
    }

    // A disabled card fades and doesn't press, but still shows its tooltip.
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn with_tooltip(mut self, tooltip: UiTooltipText) -> Self {
        self.tooltip = Some(tooltip);
        self
    }

    // `event` (Hovered or Selected) plays `role` instead, or nothing for None.
    pub fn with_sound(mut self, event: UiSoundEvent, role: Option<UiSoundKey>) -> Self {
        self.sounds.set(event, role);
        self
    }

    // Whether it's drawn selected, asked each frame.
    pub fn on_read_selected<F>(mut self, f: F) -> Self
    where
        F: Fn(&UiCard, &dyn UiWidgetContext) -> bool + 'static,
    {
        self.on_read_selected = Some(smallbox!(f));
        self
    }

    pub fn on_pressed<F>(mut self, f: F) -> Self
    where
        F: Fn(&UiCard, &mut dyn UiWidgetContext) + 'static,
    {
        self.on_pressed = Some(smallbox!(f));
        self
    }

    pub fn is_selected(&self, context: &dyn UiWidgetContext) -> bool {
        self.on_read_selected.as_ref().is_some_and(|read| read(self, context))
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn contents(&self) -> &UiWidgetGroup {
        &self.contents
    }

    pub fn contents_mut(&mut self) -> &mut UiWidgetGroup {
        &mut self.contents
    }
}
