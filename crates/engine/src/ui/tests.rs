use common::{Color, Rect, format_fixed_string};

use super::*;
use crate::{
    file_sys::paths::{AssetPath, PathRef},
    log,
    ui::{UiStaticVar, widgets::*},
};

const SAMPLE_BACKGROUND_SPRITE: PathRef = PathRef::from_str("misc/square_page_bg.png");
const SAMPLE_SEPARATOR_SPRITE: PathRef = PathRef::from_str("misc/brush_stroke_divider.png");

// 9-slice samples (Sample Menu 3), from sprites that have plain borders: the Kambara minimap's
// stone frame (720 × 420, mossy borders), and the Tampadesa minimap tooltip's gilt frame
// (360 × 120, an 8 pixel border).
const SAMPLE_PANEL_SPRITE: PathRef = PathRef::from_str("minimap/frame_kambara.png");
const SAMPLE_PANEL_SLICE_PX: [f32; 4] = [32.0, 24.0, 32.0, 24.0];
const SAMPLE_FRAME_SPRITE: PathRef = PathRef::from_str("minimap/frame_tooltip_tampadesa.png");
const SAMPLE_FRAME_SLICE_PX: [f32; 4] = [10.0, 10.0, 10.0, 10.0];
const SAMPLE_TILE_SPRITE: PathRef = PathRef::from_str("minimap/icons/temple.png"); // 48 × 48, white.

// ----------------------------------------------
// Sample Menu 1:
// ----------------------------------------------

struct SampleMenu1State {
    slider: u32,
    checkbox: bool,
    int_input: i32,
    text_input: String,
    dropdown: String,
    item_list: String,
}

static SAMPLE_MENU_1_STATE: UiStaticVar<SampleMenu1State> = UiStaticVar::new(SampleMenu1State {
    slider: 0,
    checkbox: false,
    int_input: 0,
    text_input: String::new(),
    dropdown: String::new(),
    item_list: String::new(),
});

static SAMPLE_MENU_1_INSTANCE: UiStaticVar<Option<UiMenuRcMut>> = UiStaticVar::new(None);

fn sample_text_button(context: &mut dyn UiWidgetContext, label: &str, size: UiTextButtonSize) -> UiTextButton {
    UiTextButton::new(label)
        .with_size(size)
        .with_hover_sprite(context, SAMPLE_SEPARATOR_SPRITE)
        .on_pressed(|button, _context| {
            log::info!("Pressed Button: {}", button.label());
        })
}

fn create_sample_menu_1_once(context: &mut dyn UiWidgetContext) {
    if SAMPLE_MENU_1_INSTANCE.is_some() {
        return; // Already created.
    }

    let mut menu = UiMenu::new("Sample Menu 1")
        .with_flags(UiMenuFlags::IsOpen | UiMenuFlags::AlignCenter | UiMenuFlags::AlignLeft)
        .with_size(Vec2::new(512.0, 700.0))
        .with_background(context, SAMPLE_BACKGROUND_SPRITE)
        .into_rc();

    SAMPLE_MENU_1_INSTANCE.set(Some(menu.clone()));

    let heading_font_scale = UiFontScale(1.8);
    let widgets_font_scale = UiFontScale(1.0);

    let menu_heading = UiMenuHeading::new([UiText::new("Sample Menu 1", heading_font_scale).with_color(Color::red())])
        .with_separator(context, SAMPLE_SEPARATOR_SPRITE)
        .with_margin_top(10.0)
        .with_margin_bottom(10.0);

    let slider = UiSlider::new(0u32..=100)
        .with_font_scale(widgets_font_scale)
        .on_read_value(|_slider, _context| SAMPLE_MENU_1_STATE.slider)
        .on_update_value(|_slider, _context, new_value: u32| {
            log::info!("Updated Slider value: {new_value}");
            SAMPLE_MENU_1_STATE.as_mut().slider = new_value;
        });

    let checkbox = UiCheckbox::new()
        .with_font_scale(widgets_font_scale)
        .on_read_value(|_checkbox, _context| SAMPLE_MENU_1_STATE.checkbox)
        .on_update_value(|_checkbox, _context, new_value| {
            log::info!("Updated Checkbox value: {new_value}");
            SAMPLE_MENU_1_STATE.as_mut().checkbox = new_value;
        });

    let text_input = UiTextInput::new()
        .with_font_scale(widgets_font_scale)
        .on_read_value(|_input, _context| SAMPLE_MENU_1_STATE.text_input.as_str())
        .on_update_value(|_input, _context, new_value| {
            log::info!("Updated TextInput value: {new_value}");
            SAMPLE_MENU_1_STATE.as_mut().text_input = new_value.into();
        });

    let int_input = UiIntInput::new()
        .with_font_scale(widgets_font_scale)
        .with_range(0..=256)
        .with_step(32)
        .on_read_value(|_input, _context| SAMPLE_MENU_1_STATE.int_input)
        .on_update_value(|_input, _context, new_value| {
            log::info!("Updated IntInput value: {new_value}");
            SAMPLE_MENU_1_STATE.as_mut().int_input = new_value;
        });

    let dropdown = UiDropdown::new(["Zero", "One", "Two"])
        .with_font_scale(widgets_font_scale)
        .on_selection_changed(|dropdown, _context| {
            log::info!("Updated Dropdown: '{}' [{}]", dropdown.current_selection(), dropdown.current_selection_index());
            SAMPLE_MENU_1_STATE.as_mut().dropdown = dropdown.current_selection().into();
        });

    let item_list = UiItemList::new()
        .with_label("Item List")
        .with_font_scale(widgets_font_scale)
        .with_size(Vec2::new(0.0, 100.0)) // Use whole parent window width minus margin, fixed height.
        .with_margins(30.0, 30.0)
        .with_flags(UiItemListFlags::Border | UiItemListFlags::TextInputField)
        .with_items(["Zero", "One", "Two"])
        .with_current_item(0)
        .on_selection_changed(|item_list, _context| {
            let selection_string =
                item_list.current_selection().unwrap_or_else(|| item_list.current_text_input_field().unwrap_or_default());

            log::info!("Updated ItemList: '{}' [{:?}]", selection_string, item_list.current_selection_index());

            SAMPLE_MENU_1_STATE.as_mut().item_list = selection_string.into();
        });

    let labeled_group = UiLabeledWidgetGroup::new()
        .with_label_spacing(5.0)
        .with_widget_spacing(5.0)
        .with_widget("Slider", slider)
        .with_widget("Checkbox", checkbox)
        .with_widget("Int Input", int_input)
        .with_widget("Text Input", text_input)
        .with_widget("Dropdown", dropdown);

    let stacked_button_group = UiWidgetGroup::new()
        .with_widget_spacing(Vec2::new(5.0, 5.0))
        .with_widget(sample_text_button(context, "Small Size Button", UiTextButtonSize::Small))
        .with_widget(sample_text_button(context, "Normal Size Button", UiTextButtonSize::Normal))
        .with_widget(sample_text_button(context, "Large Size Button", UiTextButtonSize::Large))
        .with_widget(sample_text_button(context, "Disabled Button", UiTextButtonSize::Normal).with_enabled(false));

    let menu_weak_ref_open_popup_btn = menu.downgrade();
    let open_message_box_button = sample_text_button(context, "Open Message Box", UiTextButtonSize::Normal).on_pressed(
        move |button, context| {
            log::info!("Pressed Button: {}", button.label());

            if let Some(mut menu_rc) = menu_weak_ref_open_popup_btn.upgrade() {
                menu_rc.open_message_box(context, |context| {
                    let menu_weak_ref_ok_btn = menu_weak_ref_open_popup_btn.clone();
                    let menu_weak_ref_cancel_btn = menu_weak_ref_open_popup_btn.clone();

                    let heading = UiMenuHeading::new([
                        UiText::new("Quit to main menu?", widgets_font_scale),
                        UiText::new("Unsaved progress will be lost", widgets_font_scale),
                    ])
                    .with_separator(context, SAMPLE_SEPARATOR_SPRITE)
                    .with_margin_top(5.0);

                    let ok_button =
                        sample_text_button(context, "Ok", UiTextButtonSize::Small).on_pressed(move |button, context| {
                            log::info!("Pressed Button: {}", button.label());
                            if let Some(mut menu_rc) = menu_weak_ref_ok_btn.upgrade() {
                                menu_rc.close_message_box(context);
                            }
                        });

                    let cancel_button =
                        sample_text_button(context, "Cancel", UiTextButtonSize::Small).on_pressed(move |button, context| {
                            log::info!("Pressed Button: {}", button.label());
                            if let Some(mut menu_rc) = menu_weak_ref_cancel_btn.upgrade() {
                                menu_rc.close_message_box(context);
                            }
                        });

                    UiMessageBox::new("Message Box Popup")
                        .with_background(context, SAMPLE_BACKGROUND_SPRITE)
                        .with_content(heading)
                        .with_buttons([ok_button, cancel_button])
                });
            }
        },
    );

    let menu_weak_ref_close_popup_btn = menu.downgrade();
    let close_message_box_button = sample_text_button(context, "Close Message Box", UiTextButtonSize::Normal).on_pressed(
        move |button, context| {
            log::info!("Pressed Button: {}", button.label());

            if let Some(mut menu_rc) = menu_weak_ref_close_popup_btn.upgrade() {
                menu_rc.close_message_box(context);
            }
        },
    );

    let side_by_side_button_group = UiWidgetGroup::new()
        .with_widget_spacing(Vec2::new(5.0, 5.0))
        .with_stack_vertically(false)
        .with_widget(open_message_box_button)
        .with_widget(close_message_box_button);

    let separator = UiSeparator::new().with_sprite(context, SAMPLE_SEPARATOR_SPRITE).with_thickness(15.0);

    menu.add_widget(menu_heading);
    menu.add_widget(labeled_group);
    menu.add_widget(item_list);
    menu.add_widget(separator.clone());
    menu.add_widget(stacked_button_group);
    menu.add_widget(separator.clone());
    menu.add_widget(side_by_side_button_group);

    log::info!("Sample Menu 1 created.");
}

fn draw_sample_menu_1(context: &mut dyn UiWidgetContext) {
    if let Some(menu) = SAMPLE_MENU_1_INSTANCE.as_mut() {
        menu.draw(context);
    }
}

// ----------------------------------------------
// Sample Menu 2:
// ----------------------------------------------

static SAMPLE_MENU_2_INSTANCE: UiStaticVar<Option<UiMenuRcMut>> = UiStaticVar::new(None);

fn create_sample_menu_2_once(context: &mut dyn UiWidgetContext) {
    if SAMPLE_MENU_2_INSTANCE.is_some() {
        return; // Already created.
    }

    let mut menu = UiMenu::new("Sample Menu 2")
        .with_flags(UiMenuFlags::IsOpen | UiMenuFlags::AlignCenter | UiMenuFlags::AlignRight)
        .with_size(Vec2::new(512.0, 700.0))
        .with_background(context, SAMPLE_BACKGROUND_SPRITE)
        .into_rc();

    SAMPLE_MENU_2_INSTANCE.set(Some(menu.clone()));

    let button_tooltip = UiTooltipText::new("This is a Sprite Button")
        .with_font_scale(UiFontScale(0.8))
        .with_background(context, SAMPLE_BACKGROUND_SPRITE);

    let button_size = Vec2::new(50.0, 50.0);

    let sample_sprite_button = |context: &mut dyn UiWidgetContext, sprite: &str, state: UiSpriteButtonState| {
        UiSpriteButton::new(context, sprite, button_size)
            .with_tooltip(button_tooltip.clone())
            .with_show_tooltip_when_pressed(true)
            .with_state(state)
    };

    let stacked_button_group = UiWidgetGroup::new()
        .with_widget_spacing(Vec2::new(5.0, 5.0))
        .with_widget(sample_sprite_button(context, "palette/housing", UiSpriteButtonState::Idle))
        .with_widget(sample_sprite_button(context, "palette/roads", UiSpriteButtonState::Idle))
        .with_widget(sample_sprite_button(context, "palette/food_and_farming", UiSpriteButtonState::Disabled));

    let side_by_side_button_group = UiWidgetGroup::new()
        .with_widget_spacing(Vec2::new(5.0, 5.0))
        .with_stack_vertically(false)
        .with_widget(sample_sprite_button(context, "palette/housing", UiSpriteButtonState::Idle))
        .with_widget(sample_sprite_button(context, "palette/roads", UiSpriteButtonState::Idle))
        .with_widget(sample_sprite_button(context, "palette/food_and_farming", UiSpriteButtonState::Disabled));

    let slideshow_frame_count = 3;
    let mut slideshow_frames = Vec::with_capacity(slideshow_frame_count);
    for i in 0..slideshow_frame_count {
        let path = format_fixed_string!(64, "misc/home_menu_anim/frame{i}.jpg");
        slideshow_frames.push(AssetPath::from_str(&path));
    }

    let slideshow = UiSlideshow::new(context, slideshow_frames.iter())
        .with_loop_mode(UiSlideshowLoopMode::WholeAnim)
        .with_frame_duration_secs(0.5)
        .with_size(Vec2::new(0.0, 250.0))
        .with_margins(30.0, 30.0);

    let separator = UiSeparator::new().with_sprite(context, SAMPLE_SEPARATOR_SPRITE).with_thickness(15.0);

    menu.add_widget(stacked_button_group);
    menu.add_widget(side_by_side_button_group);
    menu.add_widget(separator);
    menu.add_widget(slideshow);

    log::info!("Sample Menu 2 created.");
}

fn draw_sample_menu_2(context: &mut dyn UiWidgetContext) {
    if let Some(menu) = SAMPLE_MENU_2_INSTANCE.as_mut() {
        menu.draw(context);
    }
}

// ----------------------------------------------
// Sample Menu 3: 9-slice panels
// ----------------------------------------------

// A 9-slice panel for a background, framed text buttons with a panel tooltip, a popup on a panel,
// and panels drawn with UiPainter::nine_slice: stretched, tiled, and on a rect smaller than their
// borders. Narrow, to fit between the other two sample menus on a 1280 point wide window.

struct SampleMenu3 {
    menu: UiMenuRcMut,
    stretched: UiNineSlice,
    tiled: UiNineSlice,
}

static SAMPLE_MENU_3_INSTANCE: UiStaticVar<Option<SampleMenu3>> = UiStaticVar::new(None);

const SAMPLE_PAINTER_PANEL_SIZE: Vec2 = Vec2::new(95.0, 60.0);
const SAMPLE_PAINTER_SMALL_PANEL_SIZE: Vec2 = Vec2::new(14.0, 14.0); // Smaller than its borders.

fn create_sample_menu_3_once(context: &mut dyn UiWidgetContext) {
    if SAMPLE_MENU_3_INSTANCE.is_some() {
        return; // Already created.
    }

    let panel = context.load_nine_slice(SAMPLE_PANEL_SPRITE, SAMPLE_PANEL_SLICE_PX).with_scale(0.5);
    let frame_panel = context.load_nine_slice(SAMPLE_FRAME_SPRITE, SAMPLE_FRAME_SLICE_PX);

    let mut menu = UiMenu::new("Sample Menu 3")
        .with_flags(UiMenuFlags::IsOpen | UiMenuFlags::AlignCenter)
        .with_size(Vec2::new(240.0, 380.0))
        .with_panel(panel, UiMenuInsets::same(20.0))
        .into_rc();

    let heading = UiMenuHeading::new([UiText::new("9-Slice Panels", UiFontScale(1.2)).with_color(Color::red())])
        .with_margin_bottom(4.0);

    // The same frame for every state, told apart by tints, at half size.
    let frame = UiTextButtonFrame::uniform(frame_panel.with_scale(0.5)).with_padding(Vec2::new(12.0, 6.0));
    let tooltip = UiTooltipText::new("A tooltip on a 9-slice panel").with_font_scale(UiFontScale(0.8)).with_panel(frame_panel);

    let framed_button = UiTextButton::new("Framed Button")
        .with_frame(frame)
        .with_tooltip(tooltip)
        .on_pressed(|button, _context| log::info!("Pressed Button: {}", button.label()));
    let disabled_button = UiTextButton::new("Disabled Framed Button").with_size(UiTextButtonSize::Small).with_frame(frame).with_enabled(false);

    let menu_weak_ref = menu.downgrade();
    let popup_button = UiTextButton::new("Open Panel Popup").with_size(UiTextButtonSize::Small).with_frame(frame).on_pressed(
        move |_button, context| {
            let Some(mut menu_rc) = menu_weak_ref.upgrade() else {
                return;
            };

            let menu_weak_ref_ok_btn = menu_weak_ref.clone();
            menu_rc.open_message_box(context, |context| {
                let ok_button = UiTextButton::new("Ok").with_size(UiTextButtonSize::Small).with_frame(frame).on_pressed(
                    move |_button, context| {
                        if let Some(mut menu_rc) = menu_weak_ref_ok_btn.upgrade() {
                            menu_rc.close_message_box(context);
                        }
                    },
                );

                let panel = context.load_nine_slice(SAMPLE_PANEL_SPRITE, SAMPLE_PANEL_SLICE_PX).with_scale(0.5);
                UiMessageBox::new("Panel Popup")
                    .with_panel(panel, UiMenuInsets::points(24.0, 18.0, 24.0, 18.0))
                    .with_content(UiMenuHeading::new([UiText::new("A popup on a 9-slice panel", UiFontScale(1.0))]))
                    .with_buttons([ok_button])
            });
        },
    );

    let buttons = UiWidgetGroup::new()
        .with_widget_spacing(Vec2::new(6.0, 6.0))
        .with_widget(framed_button)
        .with_widget(disabled_button)
        .with_widget(popup_button);

    menu.add_widget(heading);
    menu.add_widget(buttons);

    // For UiPainter::nine_slice: the gilt frame at a quarter size, stretched; and an icon with no
    // borders, all middle, tiled at half size, so it repeats and the last row and column are cut.
    let stretched = frame_panel.with_scale(0.25);
    let tiled = context.load_nine_slice(SAMPLE_TILE_SPRITE, [0.0; 4]).with_scale(0.5).with_center(UiNineSliceCenter::Tile);

    SAMPLE_MENU_3_INSTANCE.set(Some(SampleMenu3 { menu, stretched, tiled }));
    log::info!("Sample Menu 3 created.");
}

fn draw_sample_menu_3(context: &mut dyn UiWidgetContext) {
    let Some(sample) = SAMPLE_MENU_3_INSTANCE.as_mut() else {
        return;
    };

    let (stretched, tiled) = (sample.stretched, sample.tiled);
    let flags = sample.menu.flags();
    sample.menu.draw_custom(context, flags, UiMenu::close, UiMenu::message_box, |menu, context| {
        menu.draw_menu_contents(context);
        draw_painter_panels(context.ui_sys(), &stretched, &tiled);
    });
}

// Panels drawn with the painter, captioned: stretched, tiled, and one smaller than its borders.
fn draw_painter_panels(ui_sys: &UiSystem, stretched: &UiNineSlice, tiled: &UiNineSlice) {
    const GAP: f32 = 10.0;
    const CAPTION_HEIGHT: f32 = 22.0;

    let origin = available_rect(ui_sys).min;
    let size = SAMPLE_PAINTER_PANEL_SIZE;
    dummy(ui_sys, Vec2::new(size.x * 2.0 + GAP, size.y + CAPTION_HEIGHT));

    let painter = painter(ui_sys, UiLayer::Window);
    let ink = Color::new(0.1, 0.1, 0.1, 1.0);
    let rect_at = |x: f32, size: Vec2| Rect::from_pos_and_size(origin + Vec2::new(x, CAPTION_HEIGHT), size);

    painter.text(origin, ink, "Stretch");
    painter.nine_slice(rect_at(0.0, size), stretched, Color::white());

    painter.text(origin + Vec2::new(size.x + GAP, 0.0), ink, "Tile");
    painter.nine_slice(rect_at(size.x + GAP, size), stretched, Color::white());
    painter.nine_slice(rect_at(size.x + GAP, size).shrunk(Vec2::new(3.0, 3.0)), tiled, Color::new(0.45, 0.3, 0.1, 0.8));

    // Smaller than its borders (the frame at full scale has 10 + 10 points of border on each
    // axis): they shrink to fit, so the corners meet.
    let small = SAMPLE_PAINTER_SMALL_PANEL_SIZE;
    let shrinking = stretched.with_scale(1.0);
    painter.nine_slice(rect_at(size.x - small.x, small).translated(Vec2::new(0.0, size.y + 4.0)), &shrinking, Color::white());
    dummy(ui_sys, Vec2::new(small.x, small.y + 4.0));
}

// ----------------------------------------------
// Sample Menus 4 and 5: the realm UI's widgets
// ----------------------------------------------

// Every widget the realm UI adds, in Đại Long's art where it exists (the skins' sprites under
// assets/ui/skins/dailong/, at 2×) and painted otherwise. Shown on their own (Debug/Show Widget
// Samples, draw_widget_sample_menus), as they take the room the other sample menus take.
// Sample Menu 4: controls (tabs, toggles, step selectors, menu items, framed and icon buttons).
// Sample Menu 5: data (template list rows, images, key-value pairs, a table, meters, columns,
// rich text).

const SKIN_ART_SCALE: f32 = 0.5; // 2× art.
const SKIN_INK: Color = Color::new(0.169, 0.129, 0.094, 1.0); // #2b2118
const SKIN_SUB: Color = Color::new(0.42, 0.31, 0.2, 1.0); // #6b4f33
const SKIN_ACCENT: Color = Color::new(0.659, 0.149, 0.11, 1.0); // #a8261c
const SKIN_ON_ACCENT: Color = Color::new(1.0, 0.957, 0.863, 1.0); // #fff4dc

struct WidgetSamplesState {
    edge_scrolling: bool,
    smooth_zoom: bool,
    music: bool,
    difficulty: usize,
    speed: usize,
    pressed: String,
}

static WIDGET_SAMPLES_STATE: UiStaticVar<WidgetSamplesState> = UiStaticVar::new(WidgetSamplesState {
    edge_scrolling: true,
    smooth_zoom: false,
    music: true,
    difficulty: 2,
    speed: 1,
    pressed: String::new(),
});

static SAMPLE_MENU_4_INSTANCE: UiStaticVar<Option<UiMenuRcMut>> = UiStaticVar::new(None);
static SAMPLE_MENU_5_INSTANCE: UiStaticVar<Option<UiMenuRcMut>> = UiStaticVar::new(None);

fn skin_slice(context: &mut dyn UiWidgetContext, sprite: &str, slice_px: [f32; 4]) -> UiNineSlice {
    context.load_nine_slice(PathRef::from_str(sprite), slice_px).with_scale(SKIN_ART_SCALE)
}

fn skin_frame(context: &mut dyn UiWidgetContext, stem: &str, slice_px: [f32; 4], padding: Vec2, text: Color) -> UiTextButtonFrame {
    UiTextButtonFrame::load(context, stem, slice_px).with_scale(SKIN_ART_SCALE).with_padding(padding).with_text_color(text)
}

fn skin_sample_menu(context: &mut dyn UiWidgetContext, label: &str, flags: UiMenuFlags, size: Vec2) -> UiMenuRcMut {
    let panel = skin_slice(context, "skins/dailong/panel_card.png", [28.0; 4]).with_center(UiNineSliceCenter::Round);
    UiMenu::new(label).with_flags(UiMenuFlags::IsOpen | flags).with_size(size).with_panel(panel, UiMenuInsets::same(26.0)).into_rc()
}

fn sample_heading(text: &str) -> UiMenuHeading {
    UiMenuHeading::new([UiText::new(text.to_string(), UiFontScale(1.3)).with_color(SKIN_ACCENT)]).with_margin_bottom(6.0)
}

fn create_sample_menu_4_once(context: &mut dyn UiWidgetContext) {
    if SAMPLE_MENU_4_INSTANCE.is_some() {
        return;
    }

    let mut menu = skin_sample_menu(context, "Sample Menu 4", UiMenuFlags::AlignCenter | UiMenuFlags::AlignLeft, Vec2::new(560.0, 720.0));

    // Tabs: framed, at least one width, each with its contents under it.
    let tab_look = UiTabLook {
        tab: Some(skin_frame(context, "skins/dailong/tab", [16.0; 4], Vec2::new(14.0, 5.0), SKIN_INK)),
        selected: Some(skin_frame(context, "skins/dailong/tab_selected", [16.0; 4], Vec2::new(14.0, 5.0), SKIN_ON_ACCENT)),
        min_tab_width: Some(110.0),
        ..UiTabLook::default()
    };

    let toggle_sprites = UiToggleLook::Sprites {
        off: context.load_texture(PathRef::from_str("skins/dailong/toggle_off.png")),
        on: context.load_texture(PathRef::from_str("skins/dailong/toggle_on.png")),
        knob: context.load_texture(PathRef::from_str("skins/dailong/toggle_knob.png")),
        size: Vec2::new(62.0, 30.0),
        knob_size: Vec2::new(24.0, 24.0),
    };
    let edge_scrolling = UiToggle::new()
        .with_label("Edge scrolling")
        .with_look(toggle_sprites)
        .with_fill_width(true)
        .with_text_colors(Some(SKIN_INK), Some(SKIN_SUB))
        .on_read_value(|_toggle, _context| WIDGET_SAMPLES_STATE.edge_scrolling)
        .on_update_value(|_toggle, _context, value| WIDGET_SAMPLES_STATE.as_mut().edge_scrolling = value);
    let smooth_zoom = UiToggle::new()
        .with_label("Smooth zoom")
        .with_fill_width(true)
        .on_read_value(|_toggle, _context| WIDGET_SAMPLES_STATE.smooth_zoom)
        .on_update_value(|_toggle, _context, value| WIDGET_SAMPLES_STATE.as_mut().smooth_zoom = value);
    let difficulty = UiStepSelector::new("Difficulty", ["Very Easy", "Easy", "Normal", "Hard", "Very Hard"].map(UiString::from))
        .with_current(WIDGET_SAMPLES_STATE.difficulty)
        .with_text_colors(Some(SKIN_INK), Some(SKIN_ACCENT))
        .on_changed(|selector, _context, _previous| WIDGET_SAMPLES_STATE.as_mut().difficulty = selector.current());
    let speed = UiStepSelector::new("Speed", ["Slow", "Normal", "Fast"].map(UiString::from))
        .with_style(UiStepSelectorStyle::Arrows)
        .with_wrap(true)
        .with_current(WIDGET_SAMPLES_STATE.speed)
        .on_changed(|selector, _context, _previous| WIDGET_SAMPLES_STATE.as_mut().speed = selector.current());
    let relations = UiStepSelector::numbered("Relations", 5).with_current(3);

    let game_tab = UiWidgetGroup::new()
        .with_center_horizontally(false)
        .with_widget_spacing(Vec2::new(8.0, 10.0))
        .with_widget(edge_scrolling)
        .with_widget(smooth_zoom)
        .with_widget(difficulty)
        .with_widget(speed)
        .with_widget(relations);
    let music = UiToggle::new()
        .with_label("Music")
        .with_state_texts("", "")
        .on_read_value(|_toggle, _context| WIDGET_SAMPLES_STATE.music)
        .on_update_value(|_toggle, _context, value| WIDGET_SAMPLES_STATE.as_mut().music = value);
    let sound_tab = UiWidgetGroup::new().with_center_horizontally(false).with_widget(music);
    let graphics_tab = UiRichText::new("Nothing to set here: a tab may hold *any* widget.").with_label("Graphics note");

    let tabs = UiTabBar::new()
        .with_look(tab_look)
        .with_tab_contents("Game", game_tab)
        .with_tab_contents("Sound", sound_tab)
        .with_tab_contents("Graphics", graphics_tab)
        .on_tab_changed(|bar, _context, previous| log::info!("Tab: {previous} -> {}", bar.selected_index()));

    // Menu items: a hover panel, a marker, a subline; one disabled.
    let hover = skin_slice(context, "skins/dailong/panel_row_selected.png", [20.0; 4]);
    let marker = context.load_texture(PathRef::from_str("skins/dailong/marker.png"));
    let menu_item = |label: &str, subline: Option<&str>| {
        let item = UiMenuItem::new(label.to_string())
            .with_hover_panel(hover, Vec2::new(0.0, 1.0))
            .with_marker(marker, Vec2::new(22.0, 22.0))
            .with_text_colors(Some(SKIN_INK), Some(SKIN_ACCENT), Some(SKIN_SUB))
            .on_pressed(|item, _context| WIDGET_SAMPLES_STATE.as_mut().pressed = item.label().to_string());
        match subline {
            Some(subline) => item.with_subline(subline.to_string()),
            None => item,
        }
    };
    let items = UiWidgetGroup::new()
        .with_widget(menu_item("Continue", Some("The Southern Shore · mission 6 · autosave")).with_enabled(false))
        .with_widget(menu_item("Campaign", Some("Campaign I · mission 6 of 10")))
        .with_widget(menu_item("Settings", None));

    // Framed buttons of a fixed width, and icon buttons on the skin's icon frame.
    let primary = skin_frame(context, "skins/dailong/button_primary", [20.0; 4], Vec2::new(16.0, 5.0), SKIN_ON_ACCENT);
    let secondary = skin_frame(context, "skins/dailong/button_secondary", [20.0; 4], Vec2::new(16.0, 5.0), SKIN_ON_ACCENT);
    let buttons = UiWidgetGroup::new()
        .with_stack_vertically(false)
        .with_widget_spacing(Vec2::new(10.0, 0.0))
        .with_widget(UiTextButton::new("Cancel").with_size(UiTextButtonSize::Small).with_frame(secondary).with_width(UiTextButtonWidth::Fixed(120.0)))
        .with_widget(UiTextButton::new("Start").with_size(UiTextButtonSize::Small).with_frame(primary).with_width(UiTextButtonWidth::Min(140.0)));

    let icon_frame = UiTextButtonFrame::load(context, "skins/dailong/icon_frame", [20.0; 4]).with_scale(SKIN_ART_SCALE);
    let icon_selected = skin_slice(context, "skins/dailong/icon_frame_selected.png", [20.0; 4]);
    let icon_button = |context: &mut dyn UiWidgetContext, name: &str, icon: &str| {
        UiIconButton::new(name.to_string(), Vec2::new(36.0, 36.0))
            .with_frame(icon_frame)
            .with_selected_panel(icon_selected)
            .with_icon(context, PathRef::from_str(icon), 7.0)
            .with_icon_tint(SKIN_INK)
    };
    let icons = UiWidgetGroup::new()
        .with_stack_vertically(false)
        .with_widget_spacing(Vec2::new(8.0, 0.0))
        .with_widget(icon_button(context, "temple", "minimap/icons/temple.png").with_toggle(true))
        .with_widget(icon_button(context, "market", "minimap/icons/market.png").with_selected(true))
        .with_widget(icon_button(context, "fort", "minimap/icons/fort.png").with_enabled(false))
        .with_widget(UiIconButton::new("painted", Vec2::new(36.0, 36.0)).with_icon(context, PathRef::from_str("minimap/icons/port.png"), 7.0).with_toggle(true));

    menu.add_widget(sample_heading("Controls"));
    menu.add_widget(tabs);
    menu.add_widget(UiSeparator::new().with_thickness(2.0));
    menu.add_widget(items);
    menu.add_widget(buttons);
    menu.add_widget(icons);

    SAMPLE_MENU_4_INSTANCE.set(Some(menu));
    log::info!("Sample Menu 4 created.");
}

fn create_sample_menu_5_once(context: &mut dyn UiWidgetContext) {
    if SAMPLE_MENU_5_INSTANCE.is_some() {
        return;
    }

    let mut menu = skin_sample_menu(context, "Sample Menu 5", UiMenuFlags::AlignCenter | UiMenuFlags::AlignRight, Vec2::new(600.0, 720.0));

    // Template rows: emblems, titles and subtitles, right-hand texts; a disabled (locked) row.
    let row_look = UiListRowLook {
        panel: Some(skin_slice(context, "skins/dailong/panel_row.png", [20.0, 0.0, 20.0, 4.0])),
        selected_panel: Some(skin_slice(context, "skins/dailong/panel_row_selected.png", [20.0; 4])),
        title_color: Some(SKIN_INK),
        selected_title_color: Some(SKIN_ACCENT),
        subtitle_color: Some(SKIN_SUB),
        icon_size: 30.0,
        ..UiListRowLook::default()
    };
    let emblem = |context: &mut dyn UiWidgetContext, name: &str| context.load_texture(PathRef::from_str(&format!("skins/dailong/emblem_{name}.png")));
    let realms = UiItemList::new()
        .with_label("Realm")
        .with_size(Vec2::new(0.0, 160.0))
        .with_flags(UiItemListFlags::Scrollable)
        .with_row_look(row_look)
        .with_rows(vec![
            UiListRow::new("Đại Long").with_subtitle("The Dragon Legacy").with_icon(emblem(context, "dailong_current")),
            UiListRow::new("Kambara").with_subtitle("Start Campaign II to unlock").with_icon(emblem(context, "kambara_locked")).with_enabled(false),
            UiListRow::new("Tampadesa").with_subtitle("Start Campaign III to unlock").with_icon(emblem(context, "tampadesa_locked")).with_enabled(false),
        ])
        .with_current_item(0);
    let maps = UiItemList::new()
        .with_label("Map")
        .with_size(Vec2::new(0.0, 110.0))
        .with_flags(UiItemListFlags::Scrollable)
        .with_row_look(UiListRowLook { icon_size: 0.0, ..row_look })
        .with_rows(vec![
            UiListRow::new("Delta Flood Plain").with_right("128 × 128", None),
            UiListRow::new("Karst Valley").with_right("96 × 96", None),
            UiListRow::new("Highland Pass").with_right("96 × 96", None),
        ])
        .with_current_item(0);

    let well = skin_slice(context, "skins/dailong/panel_well.png", [16.0; 4]);
    let preview = UiImage::new(context, PathRef::from_str("misc/home_menu_static_bg.png"), Vec2::new(0.0, 110.0))
        .with_label("Preview")
        .with_fit(UiImageFit::Cover)
        .with_frame(well, 5.0);
    let emblems = UiWidgetGroup::new()
        .with_stack_vertically(false)
        .with_widget_spacing(Vec2::new(6.0, 0.0))
        .with_widget(UiImage::new(context, PathRef::from_str("skins/dailong/emblem_dailong_idle.png"), Vec2::new(40.0, 40.0)).with_label("Contain"))
        .with_widget(
            UiImage::new(context, PathRef::from_str("misc/home_menu_static_bg.png"), Vec2::new(80.0, 40.0))
                .with_label("Contained")
                .with_background(SKIN_INK),
        )
        .with_widget(
            UiImage::new(context, PathRef::from_str("misc/home_menu_static_bg.png"), Vec2::new(40.0, 40.0)).with_label("Stretched").with_fit(UiImageFit::Stretch),
        );
    let stats = UiKeyValues::new()
        .with_label("Map Stats")
        .with_look(UiRowLook { label_color: Some(SKIN_SUB), value_color: Some(SKIN_INK), ..UiRowLook::default() })
        .with_pair("Size", "128 × 128")
        .with_pair("Rivers", "1 large, 3 streams")
        .with_pair("Hazards", "Floods, fire");

    let left = UiWidgetGroup::new().with_center_horizontally(false).with_widget_spacing(Vec2::new(0.0, 6.0)).with_widget(realms).with_widget(maps);
    let right = UiWidgetGroup::new()
        .with_center_horizontally(false)
        .with_widget_spacing(Vec2::new(0.0, 6.0))
        .with_widget(preview)
        .with_widget(emblems)
        .with_widget(stats);
    let columns = UiColumns::new().with_spacing(18.0).with_column(UiColumnWidth::Fraction(1.0), left).with_column(UiColumnWidth::Fraction(1.0), right);

    let goals = UiTable::new()
        .with_label("Goals")
        .with_status_column(UiTableStatusLook::default())
        .with_look(UiRowLook { header_color: Some(SKIN_ACCENT), label_color: Some(SKIN_INK), ..UiRowLook::default() })
        .with_column(UiTableColumn::new("Goal", UiColumnWidth::Fraction(2.0), UiColumnAlign::Left))
        .with_column(UiTableColumn::new("Target", UiColumnWidth::Fixed(80.0), UiColumnAlign::Right))
        .with_column(UiTableColumn::new("Achieved", UiColumnWidth::Fixed(90.0), UiColumnAlign::Right))
        .with_row(UiTableRow::new(["Population", "1,200", "1,240"]).with_status(UiTableStatus::Done))
        .with_row(UiTableRow::new(["Culture rating", "40", "31"]).with_status(UiTableStatus::Pending))
        .with_row(UiTableRow::new(["Debt-free months", "12", "0"]).with_status(UiTableStatus::Failed));

    let meters = UiWidgetGroup::new()
        .with_stack_vertically(false)
        .with_widget_spacing(Vec2::new(14.0, 0.0))
        .with_widget(UiMeter::new(UiMeterKind::Bar, Vec2::new(180.0, 14.0)).with_label("Progress").with_value(0.62, 1.0).with_goal(0.8))
        .with_widget(UiMeter::new(UiMeterKind::Ring, Vec2::new(54.0, 54.0)).with_label("Culture").with_value(62.0, 100.0))
        .with_widget(UiMeter::new(UiMeterKind::Steps(5), Vec2::new(120.0, 12.0)).with_label("Relations").with_value(3.0, 5.0));

    let stele = UiRichText::new("The river rose *early* this year; [accent]the dykes held[/], and the rice was saved.\nA second line, [#2e6b3a]in a colour of its own[/].")
        .with_label("Stele")
        .with_text_color(SKIN_INK)
        .with_emphasis_color(SKIN_SUB)
        .with_color("accent", SKIN_ACCENT);

    menu.add_widget(sample_heading("Data"));
    menu.add_widget(columns);
    menu.add_widget(goals);
    menu.add_widget(meters);
    menu.add_widget(stele);

    SAMPLE_MENU_5_INSTANCE.set(Some(menu));
    log::info!("Sample Menu 5 created.");
}

// Draws Sample Menus 4 and 5, the realm UI's widgets, in `theme`, e.g. the retail UI's.
pub fn draw_widget_sample_menus(context: &mut dyn UiWidgetContext, theme: UiThemeId) {
    let prev_theme = context.ui_sys().current_ui_theme();
    context.ui_sys().set_ui_theme(theme);

    create_sample_menu_4_once(context);
    create_sample_menu_5_once(context);

    for menu in [SAMPLE_MENU_4_INSTANCE.as_mut(), SAMPLE_MENU_5_INSTANCE.as_mut()].into_iter().flatten() {
        menu.draw(context);
    }

    context.ui_sys().set_ui_theme(prev_theme);
}

// ----------------------------------------------
// draw_sample_menus():
// ----------------------------------------------

// Draws the sample menus in `theme`, e.g. the retail UI's.
pub fn draw_sample_menus(context: &mut dyn UiWidgetContext, theme: UiThemeId) {
    let prev_theme = context.ui_sys().current_ui_theme();
    context.ui_sys().set_ui_theme(theme);

    create_sample_menu_1_once(context);
    create_sample_menu_2_once(context);
    create_sample_menu_3_once(context);

    draw_sample_menu_1(context);
    draw_sample_menu_2(context);
    draw_sample_menu_3(context);

    context.ui_sys().set_ui_theme(prev_theme);
}
