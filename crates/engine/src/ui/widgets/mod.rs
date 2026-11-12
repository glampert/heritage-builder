#![allow(clippy::enum_variant_names)]
#![allow(clippy::type_complexity)]

// Retained widgets for the game's menus and HUD, drawn with egui: a UiMenu holds widgets, which it
// draws in an egui Area (or a Modal) each frame. Widgets keep their own state and report changes
// through callbacks. Callbacks run after the widget has drawn, outside egui's borrow of the Ui,
// since they may open dialogs or draw other widgets.
//
// A widget is built with `new` and `with_*` setters for what differs from its defaults, and `on_*`
// setters for its callbacks, which take plain closures. The context is passed only where a sprite
// is loaded: `UiSpriteButton::new`, `UiSlideshow::new` and the `with_*` setters that take one.
// 9-slice panels (UiNineSlice) are loaded first, with UiWidgetContext::load_nine_slice, and handed
// to the setters that take them: UiMenu::with_panel, UiMessageBox::with_panel,
// UiTooltipText::with_panel and UiTextButton::with_frame.
//
// Widgets record themselves in the widget registry (registry.rs) as they draw, for UI automation.
//
// Interactive widgets play UI sound roles for their events (ui::sound): buttons ButtonHovered and
// ButtonPressed, checkboxes and toggles Toggled, sliders and step selectors SliderChanged,
// dropdowns DropdownOpened, ItemHovered, ItemSelected and DropdownClosed, item lists and menu
// items ItemHovered and ItemSelected, tab bars TabSelected, and menus that ask for them their open
// and close sounds (dialogs, popups). Each holds a UiWidgetSounds, which
// `with_sound` overrides per event; a new widget plays an event with one call to its
// `sounds.play(context, event, Some(default role))`.
//
// This module holds what the widgets share: the context, the UiWidget trait, UiWidgetImpl and the
// registry helpers. Each widget family has a file of its own, re-exported here, so every widget is
// `ui::widgets::<Name>`:
// - menu.rs: UiMenu, its flags, position and content insets.
// - menu_item.rs: UiMenuItem, an entry of a dialog's list of choices (label, subline, marker).
// - card.rs: UiCard, a panel around widgets that's picked by clicking it (the campaign picker's).
// - message_box.rs: UiMessageBox, a menu's modal popup.
// - text.rs: UiText, UiMenuHeading and UiSizedTextLabel.
// - group.rs: UiWidgetGroup and UiLabeledWidgetGroup.
// - columns.rs: UiColumns, widget groups side by side (UiColumnWidth, split_column_widths).
// - tab_bar.rs: UiTabBar, tabs with each tab's contents under them.
// - button.rs: UiTextButton (plain or framed, UiTextButtonFrame), UiIconButton and UiSpriteButton.
// - tooltip.rs: UiTooltipText.
// - separator.rs: UiSeparator.
// - image.rs: UiSpriteIcon, UiSlideshow and UiImage (a texture fit to a rect, fit_image).
// - input.rs: UiSlider, UiCheckbox, UiIntInput, UiTextInput and UiDropdown.
// - choice.rs: UiToggle (a pill switch) and UiStepSelector (segments or arrows).
// - list.rs: UiItemList, with plain items or template rows (UiListRow, UiListRowLook).
// - data.rs: UiKeyValues, UiTable and UiMeter, painted displays of data.
// - rich_text.rs: UiRichText, wrapped text with emphasis and coloured runs (parse_rich_text).
// - frame.rs: UiFrame, a widget group on a panel in a rect of its own size (a bar, a side panel).
// - map_view.rs: UiMapView, a map that pans and zooms with markers drawn over it by a callback
//   (record_map_marker), and UiCanvas, a rect painted by a callback.

use std::{any::Any, cell::RefMut};
use smallbox::SmallBox;
use enum_dispatch::enum_dispatch;
use egui::pos2;

use common::{RectTexCoords, Size, Vec2, hash::FNV1aHash, mem, time::Seconds};

use super::{UiFontScale, UiNineSlice, UiSystem, immediate::{self, rich_text}, internal};
use crate::{file_sys::paths::PathRef, render::{RenderSystem, texture::TextureHandle}, sound::SoundSystem};

mod button;
mod card;
mod choice;
mod columns;
mod data;
mod frame;
mod group;
mod image;
mod input;
mod list;
mod map_view;
mod menu;
mod menu_item;
mod message_box;
mod rich_text;
mod separator;
mod tab_bar;
mod text;
mod tooltip;

pub use button::*;
pub use card::*;
pub use choice::*;
pub use columns::*;
pub use data::*;
pub use frame::*;
pub use group::*;
pub use image::*;
pub use input::*;
pub use list::*;
pub use map_view::*;
pub use menu::*;
pub use menu_item::*;
pub use message_box::*;
pub use rich_text::*;
pub use separator::*;
pub use tab_bar::*;
pub use text::*;
pub use tooltip::*;

// ----------------------------------------------
// Widget ids
// ----------------------------------------------

// An egui id for a widget that keeps state in egui (an Area, a combo box, a scroll area): its type,
// label and address. Widgets don't move once they're in a menu. A UiString label passes its
// id_source (a key, not its text), so a language switch keeps the state.
#[inline]
fn widget_id<W>(widget: &W, label: &str) -> egui::Id {
    egui::Id::unique((std::any::type_name::<W>(), label, widget as *const W as usize))
}

// ----------------------------------------------
// UiWidgetContext
// ----------------------------------------------

// Base UI widget context; implemented by the game layer with extended game context.
pub trait UiWidgetContext<'game> {
    fn runtime_type_id(&self) -> FNV1aHash;

    fn ui_sys(&self) -> &'game UiSystem;
    fn render_sys(&mut self) -> RefMut<'_, &'game mut RenderSystem>;
    fn sound_sys(&mut self) -> RefMut<'_, &'game mut SoundSystem>;
    fn viewport_size(&self) -> Size;
    fn delta_time_secs(&self) -> Seconds;
    fn cursor_screen_pos(&self) -> Vec2;

    fn begin_widget_window(&mut self);
    fn end_widget_window(&mut self);
    fn is_inside_widget_window(&self) -> bool;

    fn begin_side_by_side_layout(&mut self);
    fn end_side_by_side_layout(&mut self);
    fn is_side_by_side_layout(&self) -> bool;

    fn pause_simulation(&mut self);
    fn resume_simulation(&mut self);

    #[inline]
    fn set_window_font_scale(&self, font_scale: UiFontScale) {
        debug_assert!(self.is_inside_widget_window());
        self.ui_sys().set_window_font_scale(font_scale);
    }

    #[inline]
    fn calc_text_size(&self, font_scale: UiFontScale, text: &str) -> Vec2 {
        internal::calc_text_size(self.ui_sys(), font_scale, text).0
    }

    #[inline]
    fn calc_text_and_font_size(&self, font_scale: UiFontScale, text: &str) -> (Vec2, f32) {
        internal::calc_text_size(self.ui_sys(), font_scale, text)
    }

    #[inline]
    fn load_texture(&mut self, path: PathRef) -> TextureHandle {
        let file_path = super::assets_path().join(path);
        self.render_sys()
            .texture_cache_mut()
            .load_texture_with_settings((&file_path).into(), Some(super::texture_settings()))
    }

    // A 9-slice panel of a sprite under assets/ui/, cut by `slice_px`, its borders in pixels (left,
    // top, right, bottom). At one point per pixel and stretched: see UiNineSlice::with_scale and
    // with_center. A sprite that fails to load draws as the magenta dummy, its borders kept.
    fn load_nine_slice(&mut self, path: PathRef, slice_px: [f32; 4]) -> UiNineSlice {
        let texture = self.load_texture(path);
        let texture_size = match self.render_sys().texture_cache().texture_size(texture) {
            Some(size) => Vec2::new(size.width as f32, size.height as f32),
            None => Vec2::new(slice_px[0] + slice_px[2] + 1.0, slice_px[1] + slice_px[3] + 1.0),
        };
        UiNineSlice::new(texture, texture_size, slice_px)
    }
}

// ----------------------------------------------
// Checked UiWidgetContext up-casting
// ----------------------------------------------

pub trait UiWidgetContextStaticTypeId {
    const STATIC_TYPE_ID: FNV1aHash;
}

#[inline]
pub fn context_as<'game, T>(context: &dyn UiWidgetContext<'game>) -> &'game T
where
    T: UiWidgetContext<'game> + UiWidgetContextStaticTypeId,
{
    // SAFETY: Assert guarantees T is the concrete type we are casting to.
    debug_assert_eq!(context.runtime_type_id(), T::STATIC_TYPE_ID, "Casting UiWidgetContext to incorrect type!");
    unsafe { &*(context as *const dyn UiWidgetContext as *const T) }
}

#[inline]
pub fn context_as_mut<'game, T>(context: &mut dyn UiWidgetContext<'game>) -> &'game mut T
where
    T: UiWidgetContext<'game> + UiWidgetContextStaticTypeId,
{
    debug_assert_eq!(context.runtime_type_id(), T::STATIC_TYPE_ID, "Casting UiWidgetContext to incorrect type!");
    unsafe { &mut *(context as *mut dyn UiWidgetContext as *mut T) }
}

// ----------------------------------------------
// UiWidgetCallback
// ----------------------------------------------

// A widget's callbacks are plain closures: each takes the widget, the context and, for an event
// with a value, that value. A closure is stored inline when it fits in two words (a weak menu
// reference, a few Copy values) and on the heap otherwise. One that takes its widget mutably
// (UiMenu's open/close, UiSpriteButton's state change) is called through `mem::mut_ref_cast`,
// since it lives in that widget: it must not replace itself while it runs.
pub type UiWidgetClosure<F> = SmallBox<F, smallbox::space::S2>;

// A callback field: None until the widget's `on_*` setter is called.
pub type UiWidgetCallback<F> = Option<UiWidgetClosure<F>>;

// ----------------------------------------------
// UiWidget Trait
// ----------------------------------------------

#[enum_dispatch(UiWidgetImpl)]
pub trait UiWidget: Any {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any {
        mem::mut_ref_cast(self.as_any())
    }

    fn draw(&mut self, context: &mut dyn UiWidgetContext);
    fn measure(&self, context: &dyn UiWidgetContext) -> Vec2;

    fn label(&self) -> &str {
        ""
    }

    fn font_scale(&self) -> UiFontScale {
        UiFontScale::default()
    }
}

// ----------------------------------------------
// Widget registry helpers (UI automation)
// ----------------------------------------------

// Records the widget just drawn and returns a value ui.set queued for it, which the widget then
// applies through its own update callback.
#[inline]
fn record_settable(
    ui_sys: &UiSystem,
    ui: &egui::Ui,
    response: &egui::Response,
    kind: &'static str,
    label: &str,
    value: serde_json::Value,
) -> Option<serde_json::Value> {
    immediate::record(ui_sys, ui, response, kind, label, None, true, Some(value))
}

// An item of a list or dropdown, given by text or index.
pub(super) fn list_index(value: &serde_json::Value, items: &[impl AsRef<str>]) -> Option<usize> {
    match value {
        serde_json::Value::String(text) => items.iter().position(|item| item.as_ref() == text),
        serde_json::Value::Number(index) => index.as_u64().map(|index| index as usize).filter(|index| *index < items.len()),
        _ => None,
    }
}

// A label to the left of a framed widget, centered on its height.
fn left_label(ui_sys: &UiSystem, ui: &mut egui::Ui, label: &str) {
    if !label.is_empty() {
        ui.add(egui::Label::new(rich_text(ui_sys, ui, label)).selectable(false));
    }
}

// The UVs of `tex_coords` (V up), from the rect's top left corner to its bottom right.
fn tex_coords_uv(tex_coords: &RectTexCoords) -> egui::Rect {
    let top_left = tex_coords.top_left();
    let bottom_right = tex_coords.bottom_right();
    egui::Rect::from_min_max(pos2(top_left.x, 1.0 - bottom_right.y), pos2(bottom_right.x, 1.0 - top_left.y))
}

// ----------------------------------------------
// UiWidgetImpl
// ----------------------------------------------

#[enum_dispatch]
pub enum UiWidgetImpl {
    UiMenu,
    UiMenuHeading,
    UiSizedTextLabel,
    UiWidgetGroup,
    UiLabeledWidgetGroup,
    UiTextButton,
    UiSpriteButton,
    UiSeparator,
    UiSpriteIcon,
    UiSlider,
    UiCheckbox,
    UiIntInput,
    UiTextInput,
    UiDropdown,
    UiItemList,
    UiMessageBox,
    UiSlideshow,
    UiTabBar,
    UiToggle,
    UiStepSelector,
    UiMenuItem,
    UiIconButton,
    UiImage,
    UiKeyValues,
    UiTable,
    UiMeter,
    UiColumns,
    UiRichText,
    UiCard,
    UiFrame,
    UiMapView,
    UiCanvas,
}
