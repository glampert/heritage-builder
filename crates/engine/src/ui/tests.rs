use common::{Color, format_fixed_string};

use super::*;
use crate::{
    file_sys::paths::{AssetPath, PathRef},
    log,
    ui::{UiStaticVar, widgets::*},
};

const SAMPLE_BACKGROUND_SPRITE: PathRef = PathRef::from_str("misc/square_page_bg.png");
const SAMPLE_SEPARATOR_SPRITE: PathRef = PathRef::from_str("misc/brush_stroke_divider.png");

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
// draw_sample_menus():
// ----------------------------------------------

pub fn draw_sample_menus(context: &mut dyn UiWidgetContext) {
    let prev_theme = context.ui_sys().current_ui_theme();
    context.ui_sys().set_ui_theme(UiTheme::InGame);

    create_sample_menu_1_once(context);
    create_sample_menu_2_once(context);

    draw_sample_menu_1(context);
    draw_sample_menu_2(context);

    context.ui_sys().set_ui_theme(prev_theme);
}
