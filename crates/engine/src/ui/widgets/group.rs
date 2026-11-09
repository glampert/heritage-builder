use std::any::Any;
use egui::vec2;

use common::Vec2;

use super::{UiWidget, UiWidgetContext, UiWidgetImpl};
use crate::ui::{UiString, immediate::egui_vec2, internal};

// ----------------------------------------------
// UiWidgetGroup
// ----------------------------------------------

#[derive(Copy, Clone)]
pub struct UiWidgetGroupWidgetIndex(usize);

// Groups UiWidgets to draw them centered/aligned.
// Supports vertical and horizontal alignment and custom item spacing.
pub struct UiWidgetGroup {
    widgets: Vec<UiWidgetImpl>,
    widget_spacing: Vec2,
    center_vertically: bool,
    center_horizontally: bool,
    stack_vertically: bool,
    margin_left: f32,
    margin_right: f32,
}

impl UiWidget for UiWidgetGroup {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut dyn UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        let ui_sys = context.ui_sys();
        let spacing = egui_vec2(self.widget_spacing);
        let previous_spacing = ui_sys.with_ui(|ui| std::mem::replace(&mut ui.spacing_mut().item_spacing, spacing));

        internal::draw_centered_widget_group(
            context,
            &mut self.widgets,
            self.center_vertically,
            self.center_horizontally,
            self.stack_vertically,
            (self.margin_left, self.margin_right),
        );

        ui_sys.with_ui(|ui| ui.spacing_mut().item_spacing = previous_spacing);
    }

    fn measure(&self, context: &dyn UiWidgetContext) -> Vec2 {
        let mut size = Vec2::zero();

        for widget in &self.widgets {
            let widget_size = widget.measure(context);

            if self.stack_vertically {
                size.x = size.x.max(widget_size.x); // Max width.
                size.y += widget_size.y; // Total height.
            } else {
                size.x += widget_size.x; // Total width.
                size.y = size.y.max(widget_size.y); // Max height.
            }
        }

        if !self.widgets.is_empty() {
            // Add inter-widget spacing
            let spacing = self.widget_spacing;

            if self.stack_vertically {
                size.y += spacing.y * (self.widgets.len() - 1) as f32; // v-spacing
            } else {
                size.x += spacing.x * (self.widgets.len() - 1) as f32; // h-spacing
            }
        }

        // Subtract margins.
        size.x -= self.margin_left;
        size.x -= self.margin_right;

        size
    }
}

impl Default for UiWidgetGroup {
    fn default() -> Self {
        Self::new()
    }
}

impl UiWidgetGroup {
    // An empty stack of widgets, centered across the menu, with no spacing between them.
    pub fn new() -> Self {
        Self {
            widgets: Vec::new(),
            widget_spacing: Vec2::zero(),
            center_vertically: false,
            center_horizontally: true,
            stack_vertically: true,
            margin_left: 0.0,
            margin_right: 0.0,
        }
    }

    pub fn with_widget_spacing(mut self, spacing: Vec2) -> Self {
        debug_assert!(spacing.x >= 0.0 && spacing.y >= 0.0);
        self.widget_spacing = spacing;
        self
    }

    // Centered in the room left in the menu, instead of starting at its top.
    pub fn with_center_vertically(mut self, center: bool) -> Self {
        self.center_vertically = center;
        self
    }

    // Centered across the menu (the default), or starting at its left.
    pub fn with_center_horizontally(mut self, center: bool) -> Self {
        self.center_horizontally = center;
        self
    }

    // Widgets stacked top to bottom (the default), or laid out side by side.
    pub fn with_stack_vertically(mut self, stack: bool) -> Self {
        self.stack_vertically = stack;
        self
    }

    // Room left free at the group's sides; with a right margin, inputs fill the width up to it.
    pub fn with_margins(mut self, left: f32, right: f32) -> Self {
        debug_assert!(left >= 0.0 && right >= 0.0);
        self.margin_left = left;
        self.margin_right = right;
        self
    }

    pub fn with_widget(mut self, widget: impl Into<UiWidgetImpl>) -> Self {
        self.add_widget(widget);
        self
    }

    pub fn add_widget(&mut self, widget: impl Into<UiWidgetImpl>) -> UiWidgetGroupWidgetIndex {
        let index = self.widgets.len();
        self.widgets.push(widget.into());
        UiWidgetGroupWidgetIndex(index)
    }

    #[inline]
    pub fn widgets(&self) -> &[UiWidgetImpl] {
        &self.widgets
    }

    #[inline]
    pub fn widgets_mut(&mut self) -> &mut [UiWidgetImpl] {
        &mut self.widgets
    }

    #[inline]
    pub fn widget_as<Widget: UiWidget>(&self, index: UiWidgetGroupWidgetIndex) -> Option<&Widget> {
        self.widgets[index.0].as_any().downcast_ref::<Widget>()
    }

    #[inline]
    pub fn widget_as_mut<Widget: UiWidget>(&mut self, index: UiWidgetGroupWidgetIndex) -> Option<&mut Widget> {
        self.widgets[index.0].as_any_mut().downcast_mut::<Widget>()
    }

    pub fn find_widget_of_type<Widget: UiWidget>(&self) -> Option<(UiWidgetGroupWidgetIndex, &Widget)> {
        for (index, widget) in self.widgets.iter().enumerate() {
            if let Some(w) = widget.as_any().downcast_ref::<Widget>() {
                return Some((UiWidgetGroupWidgetIndex(index), w));
            }
        }

        None
    }

    pub fn find_widget_of_type_mut<Widget: UiWidget>(&mut self) -> Option<(UiWidgetGroupWidgetIndex, &mut Widget)> {
        for (index, widget) in self.widgets.iter_mut().enumerate() {
            if let Some(w) = widget.as_any_mut().downcast_mut::<Widget>() {
                return Some((UiWidgetGroupWidgetIndex(index), w));
            }
        }

        None
    }

    pub fn find_widget_with_label<Widget: UiWidget>(&self, label: &str) -> Option<(UiWidgetGroupWidgetIndex, &Widget)> {
        debug_assert!(!label.is_empty());

        for (index, widget) in self.widgets.iter().enumerate() {
            if let Some(w) = widget.as_any().downcast_ref::<Widget>() {
                if w.label() == label {
                    return Some((UiWidgetGroupWidgetIndex(index), w));
                }
            }
        }

        None
    }

    pub fn find_widget_with_label_mut<Widget: UiWidget>(
        &mut self,
        label: &str,
    ) -> Option<(UiWidgetGroupWidgetIndex, &mut Widget)> {
        debug_assert!(!label.is_empty());

        for (index, widget) in self.widgets.iter_mut().enumerate() {
            if let Some(w) = widget.as_any_mut().downcast_mut::<Widget>() {
                if w.label() == label {
                    return Some((UiWidgetGroupWidgetIndex(index), w));
                }
            }
        }

        None
    }
}

// ----------------------------------------------
// UiLabeledWidgetGroup
// ----------------------------------------------

#[derive(Copy, Clone)]
pub struct UiLabeledWidgetGroupWidgetIndex(usize);

// Groups labels + UiWidgets to draw them centered/aligned.
// Supports vertical and horizontal alignment and custom item spacing.
pub struct UiLabeledWidgetGroup {
    labels_and_widgets: Vec<(UiString, UiWidgetImpl)>,
    label_spacing: f32,
    widget_spacing: f32,
    center_vertically: bool,
    center_horizontally: bool,
    margin_left: f32,
    margin_right: f32,
    label_column_width: Option<f32>, // The labels' column, shared with other groups; the longest label's width if None.
}

impl UiWidget for UiLabeledWidgetGroup {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut dyn UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        let ui_sys = context.ui_sys();
        let spacing = vec2(self.label_spacing, self.widget_spacing);
        let previous_spacing = ui_sys.with_ui(|ui| std::mem::replace(&mut ui.spacing_mut().item_spacing, spacing));

        internal::draw_centered_labeled_widget_group(
            context,
            &mut self.labels_and_widgets,
            self.center_vertically,
            self.center_horizontally,
            (self.margin_left, self.margin_right),
            self.label_column_width,
        );

        ui_sys.with_ui(|ui| ui.spacing_mut().item_spacing = previous_spacing);
    }

    fn measure(&self, context: &dyn UiWidgetContext) -> Vec2 {
        let spacing = Vec2::new(self.label_spacing, self.widget_spacing);
        let mut size = Vec2::zero();

        for (label, widget) in &self.labels_and_widgets {
            let widget_size = widget.measure(context);
            let label_size = context.calc_text_size(widget.font_scale(), label.resolve());
            let label_width = self.label_column_width.map_or(label_size.x, |width| width.max(label_size.x));

            size.x = size.x.max(label_width + spacing.x + widget_size.x); // Max width (label + widget).
            size.y += label_size.y.max(widget_size.y); // Total height (largest of the two).
        }

        if !self.labels_and_widgets.is_empty() {
            // Add inter-widget spacing
            size.y += spacing.y * (self.labels_and_widgets.len() - 1) as f32;
        }

        // Subtract margins.
        size.x -= self.margin_left;
        size.x -= self.margin_right;

        size
    }
}

impl Default for UiLabeledWidgetGroup {
    fn default() -> Self {
        Self::new()
    }
}

impl UiLabeledWidgetGroup {
    // Empty rows of a label and its widget, centered across the menu, with no spacing between them.
    pub fn new() -> Self {
        Self {
            labels_and_widgets: Vec::new(),
            label_spacing: 0.0,
            widget_spacing: 0.0,
            center_vertically: false,
            center_horizontally: true,
            margin_left: 0.0,
            margin_right: 0.0,
            label_column_width: None,
        }
    }

    // Between a label and its widget.
    pub fn with_label_spacing(mut self, spacing: f32) -> Self {
        debug_assert!(spacing >= 0.0);
        self.label_spacing = spacing;
        self
    }

    // Between the rows.
    pub fn with_widget_spacing(mut self, spacing: f32) -> Self {
        debug_assert!(spacing >= 0.0);
        self.widget_spacing = spacing;
        self
    }

    // Centered in the room left in the menu, instead of starting at its top.
    pub fn with_center_vertically(mut self, center: bool) -> Self {
        self.center_vertically = center;
        self
    }

    // Centered across the menu (the default), or starting at its left.
    pub fn with_center_horizontally(mut self, center: bool) -> Self {
        self.center_horizontally = center;
        self
    }

    // Room left free at the group's sides; with a right margin, inputs fill the width up to it.
    pub fn with_margins(mut self, left: f32, right: f32) -> Self {
        debug_assert!(left >= 0.0 && right >= 0.0);
        self.margin_left = left;
        self.margin_right = right;
        self
    }

    // The labels' column, to share with other groups; the longest label's width if not given.
    pub fn with_label_column_width(mut self, width: f32) -> Self {
        self.label_column_width = Some(width);
        self
    }

    pub fn with_widget(mut self, label: impl Into<UiString>, widget: impl Into<UiWidgetImpl>) -> Self {
        self.add_widget(label, widget);
        self
    }

    pub fn add_widget(
        &mut self,
        label: impl Into<UiString>,
        widget: impl Into<UiWidgetImpl>,
    ) -> UiLabeledWidgetGroupWidgetIndex {
        let label = label.into();
        let widget = widget.into();
        debug_assert!(!label.is_empty(), "UiLabeledWidgetGroup requires a non-empty label!");
        debug_assert!(widget.label().is_empty(), "Widgets added to UiLabeledWidgetGroup should not have a label!");

        let index = self.labels_and_widgets.len();
        self.labels_and_widgets.push((label, widget));
        UiLabeledWidgetGroupWidgetIndex(index)
    }

    #[inline]
    pub fn labels_and_widgets(&self) -> &[(UiString, UiWidgetImpl)] {
        &self.labels_and_widgets
    }

    #[inline]
    pub fn labels_and_widgets_mut(&mut self) -> &mut [(UiString, UiWidgetImpl)] {
        &mut self.labels_and_widgets
    }

    #[inline]
    pub fn widget_as<Widget: UiWidget>(&self, index: UiLabeledWidgetGroupWidgetIndex) -> Option<&Widget> {
        self.labels_and_widgets[index.0].1.as_any().downcast_ref::<Widget>()
    }

    #[inline]
    pub fn widget_as_mut<Widget: UiWidget>(&mut self, index: UiLabeledWidgetGroupWidgetIndex) -> Option<&mut Widget> {
        self.labels_and_widgets[index.0].1.as_any_mut().downcast_mut::<Widget>()
    }

    pub fn find_widget_of_type<Widget: UiWidget>(&self) -> Option<(UiLabeledWidgetGroupWidgetIndex, &Widget)> {
        for (index, (_, widget)) in self.labels_and_widgets.iter().enumerate() {
            if let Some(w) = widget.as_any().downcast_ref::<Widget>() {
                return Some((UiLabeledWidgetGroupWidgetIndex(index), w));
            }
        }

        None
    }

    pub fn find_widget_of_type_mut<Widget: UiWidget>(&mut self) -> Option<(UiLabeledWidgetGroupWidgetIndex, &mut Widget)> {
        for (index, (_, widget)) in self.labels_and_widgets.iter_mut().enumerate() {
            if let Some(w) = widget.as_any_mut().downcast_mut::<Widget>() {
                return Some((UiLabeledWidgetGroupWidgetIndex(index), w));
            }
        }

        None
    }

    pub fn find_widget_with_label<Widget: UiWidget>(
        &self,
        label: &str,
    ) -> Option<(UiLabeledWidgetGroupWidgetIndex, &Widget)> {
        debug_assert!(!label.is_empty());

        for (index, (widget_label, widget)) in self.labels_and_widgets.iter().enumerate() {
            if let Some(w) = widget.as_any().downcast_ref::<Widget>() {
                if widget_label.resolve() == label {
                    return Some((UiLabeledWidgetGroupWidgetIndex(index), w));
                }
            }
        }

        None
    }

    pub fn find_widget_with_label_mut<Widget: UiWidget>(
        &mut self,
        label: &str,
    ) -> Option<(UiLabeledWidgetGroupWidgetIndex, &mut Widget)> {
        debug_assert!(!label.is_empty());

        for (index, (widget_label, widget)) in self.labels_and_widgets.iter_mut().enumerate() {
            if let Some(w) = widget.as_any_mut().downcast_mut::<Widget>() {
                if widget_label.resolve() == label {
                    return Some((UiLabeledWidgetGroupWidgetIndex(index), w));
                }
            }
        }

        None
    }
}
