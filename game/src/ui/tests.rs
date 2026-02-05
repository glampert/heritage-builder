use super::*;
use crate::{log, utils::mem::RawPtr, ui::{widgets::*, UiStaticVar}};

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

fn create_sample_menu_1_once(context: &mut UiWidgetContext) {
    if SAMPLE_MENU_1_INSTANCE.is_some() {
        return; // Already created.
    }

    let mut menu = UiMenu::new(
        context,
        UiMenuParams {
            label: Some("Sample Menu 1".into()),
            flags: UiMenuFlags::IsOpen | UiMenuFlags::AlignCenter | UiMenuFlags::AlignLeft,
            size: Some(Vec2::new(512.0, 700.0)),
            background: Some("misc/square_page_bg.png"),
            ..Default::default()
        }
    );

    SAMPLE_MENU_1_INSTANCE.set(Some(menu.clone()));

    let heading_font_scale = UiFontScale(1.8);
    let widgets_font_scale = UiFontScale(1.0);

    let menu_heading = UiMenuHeading::new(
        context,
        UiMenuHeadingParams {
            font_scale: heading_font_scale,
            lines: vec!["Sample Menu 1".into()],
            separator: Some("misc/brush_stroke_divider.png"),
            margin_top: 10.0,
            margin_bottom: 10.0,
            ..Default::default()
        }
    );

    let slider = UiSlider::new(
        context,
        UiSliderParams {
            font_scale: widgets_font_scale,
            min: 0,
            max: 100,
            on_read_value: UiSliderReadValue::with_fn(
                |_slider, _context| -> u32 {
                    SAMPLE_MENU_1_STATE.slider
                }
            ),
            on_update_value: UiSliderUpdateValue::with_fn(
                |_slider, _context, new_value: u32| {
                    log::info!("Updated Slider value: {new_value}");
                    SAMPLE_MENU_1_STATE.as_mut().slider = new_value;
                }
            ),
            ..Default::default()
        }
    );

    let checkbox = UiCheckbox::new(
        context,
        UiCheckboxParams {
            font_scale: widgets_font_scale,
            on_read_value: UiCheckboxReadValue::with_fn(
                |_checkbox, _context| -> bool {
                    SAMPLE_MENU_1_STATE.checkbox
                }
            ),
            on_update_value: UiCheckboxUpdateValue::with_fn(
                |_checkbox, _context, new_value: bool| {
                    log::info!("Updated Checkbox value: {new_value}");
                    SAMPLE_MENU_1_STATE.as_mut().checkbox = new_value;
                }
            ),
            ..Default::default()
        }
    );

    let text_input = UiTextInput::new(
        context,
        UiTextInputParams {
            font_scale: widgets_font_scale,
            on_read_value: UiTextInputReadValue::with_fn(
                |_input, _context| -> RawPtr<str> {
                    RawPtr::from_ref(&SAMPLE_MENU_1_STATE.text_input)
                }
            ),
            on_update_value: UiTextInputUpdateValue::with_fn(
                |_input, _context, new_value: RawPtr<str>| {
                    log::info!("Updated TextInput value: {}", new_value.as_ref());
                    SAMPLE_MENU_1_STATE.as_mut().text_input = new_value.as_ref().into();
                }
            ),
            ..Default::default()
        }
    );

    let int_input = UiIntInput::new(
        context,
        UiIntInputParams {
            font_scale: widgets_font_scale,
            min: Some(0),
            max: Some(256),
            step: Some(32),
            on_read_value: UiIntInputReadValue::with_fn(
                |_input, _context| -> i32 {
                    SAMPLE_MENU_1_STATE.int_input
                }
            ),
            on_update_value: UiIntInputUpdateValue::with_fn(
                |_input, _context, new_value: i32| {
                    log::info!("Updated IntInput value: {}", new_value);
                    SAMPLE_MENU_1_STATE.as_mut().int_input = new_value;
                }
            ),
            ..Default::default()
        }
    );

    let dropdown = UiDropdown::new(
        context,
        UiDropdownParams {
            font_scale: widgets_font_scale,
            current_item: 0,
            items: vec!["Zero".into(), "One".into(), "Two".into()],
            on_selection_changed: UiDropdownSelectionChanged::with_fn(
                |dropdown, _context| {
                    log::info!("Updated Dropdown: '{}' [{}]",
                        dropdown.current_selection(),
                        dropdown.current_selection_index());

                    SAMPLE_MENU_1_STATE.as_mut().dropdown = dropdown.current_selection().into();
                }
            ),
            ..Default::default()
        }
    );

    let item_list = UiItemList::new(
        context,
        UiItemListParams {
            font_scale: widgets_font_scale,
            label: Some("Item List".into()),
            size: Some(Vec2::new(0.0, 100.0)), // Use whole parent window width minus margin, fixed height.
            margin_left: 30.0,
            margin_right: 30.0,
            flags: UiItemListFlags::Border | UiItemListFlags::TextInputField,
            current_item: Some(0),
            items: vec!["Zero".into(), "One".into(), "Two".into()],
            on_selection_changed: UiItemListSelectionChanged::with_fn(
                |item_list, _context| {
                    let selection_string = item_list.current_selection().unwrap_or_else(|| {
                        item_list.current_text_input_field().unwrap_or_default()
                    });

                    log::info!("Updated ItemList: '{}' [{:?}]",
                        selection_string,
                        item_list.current_selection_index());

                    SAMPLE_MENU_1_STATE.as_mut().item_list = selection_string.into();
                }
            ),
        }
    );

    let mut labeled_group = UiLabeledWidgetGroup::new(
        context,
        UiLabeledWidgetGroupParams {
            label_spacing: 5.0,
            widget_spacing: 5.0,
            center_vertically: false,
            center_horizontally: true,
        }
    );

    labeled_group.add_widget("Slider".into(), slider);
    labeled_group.add_widget("Checkbox".into(), checkbox);
    labeled_group.add_widget("Int Input".into(), int_input);
    labeled_group.add_widget("Text Input".into(), text_input);
    labeled_group.add_widget("Dropdown".into(), dropdown);

    let mut stacked_button_group = UiWidgetGroup::new(
        context,
        UiWidgetGroupParams {
            widget_spacing: 5.0,
            center_vertically: false,
            center_horizontally: true,
            stack_vertically: true,
        }
    );

    stacked_button_group.add_widget(UiTextButton::new(
        context,
        UiTextButtonParams {
            label: "Small Size Button".into(),
            size: UiTextButtonSize::Small,
            hover: Some("misc/brush_stroke_divider.png"),
            enabled: true,
            on_pressed: UiTextButtonPressed::with_fn(
                |button, _context| {
                    log::info!("Pressed Button: {}", button.label());
                }
            ),
            ..Default::default()
        }
    ));

    stacked_button_group.add_widget(UiTextButton::new(
        context,
        UiTextButtonParams {
            label: "Normal Size Button".into(),
            size: UiTextButtonSize::Normal,
            hover: Some("misc/brush_stroke_divider.png"),
            enabled: true,
            on_pressed: UiTextButtonPressed::with_fn(
                |button, _context| {
                    log::info!("Pressed Button: {}", button.label());
                }
            ),
            ..Default::default()
        }
    ));

    stacked_button_group.add_widget(UiTextButton::new(
        context,
        UiTextButtonParams {
            label: "Large Size Button".into(),
            size: UiTextButtonSize::Large,
            hover: Some("misc/brush_stroke_divider.png"),
            enabled: true,
            on_pressed: UiTextButtonPressed::with_fn(
                |button, _context| {
                    log::info!("Pressed Button: {}", button.label());
                }
            ),
            ..Default::default()
        }
    ));

    stacked_button_group.add_widget(UiTextButton::new(
        context,
        UiTextButtonParams {
            label: "Disabled Button".into(),
            size: UiTextButtonSize::Normal,
            hover: Some("misc/brush_stroke_divider.png"),
            enabled: false,
            on_pressed: UiTextButtonPressed::with_fn(
                |button, _context| {
                    log::info!("Pressed Button: {}", button.label());
                }
            ),
            ..Default::default()
        }
    ));

    let mut side_by_side_button_group = UiWidgetGroup::new(
        context,
        UiWidgetGroupParams {
            widget_spacing: 5.0,
            center_vertically: false,
            center_horizontally: true,
            stack_vertically: false,
        }
    );

    let menu_weak_ref_open_popup_btn = menu.downgrade();
    side_by_side_button_group.add_widget(UiTextButton::new(
        context,
        UiTextButtonParams {
            label: "Open Message Box".into(),
            size: UiTextButtonSize::Normal,
            hover: Some("misc/brush_stroke_divider.png"),
            enabled: true,
            on_pressed: UiTextButtonPressed::with_closure(
                move |button, context| {
                    log::info!("Pressed Button: {}", button.label());

                    if let Some(mut menu_rc) = menu_weak_ref_open_popup_btn.upgrade() {
                        menu_rc.open_message_box(context, |context: &mut UiWidgetContext| {
                            let menu_weak_ref_ok_btn = menu_weak_ref_open_popup_btn.clone();
                            let menu_weak_ref_cancel_btn = menu_weak_ref_open_popup_btn.clone();

                            UiMessageBoxParams {
                                label: Some("Message Box Popup".into()),
                                background: Some("misc/square_page_bg.png"),
                                contents: vec![
                                    UiWidgetImpl::from(UiMenuHeading::new(
                                        context,
                                        UiMenuHeadingParams {
                                            font_scale: widgets_font_scale,
                                            lines: vec!["Quit to main menu?".into(), "Unsaved progress will be lost".into()],
                                            separator: Some("misc/brush_stroke_divider.png"),
                                            margin_top: 5.0,
                                            ..Default::default()
                                        }
                                    ))
                                ],
                                buttons: vec![
                                    UiWidgetImpl::from(UiTextButton::new(
                                        context,
                                        UiTextButtonParams {
                                            label: "Ok".into(),
                                            size: UiTextButtonSize::Small,
                                            hover: Some("misc/brush_stroke_divider.png"),
                                            enabled: true,
                                            on_pressed: UiTextButtonPressed::with_closure(
                                                move |button, context| {
                                                    log::info!("Pressed Button: {}", button.label());
                                                    if let Some(mut menu_rc) = menu_weak_ref_ok_btn.upgrade() {
                                                        menu_rc.close_message_box(context);
                                                    }
                                                }
                                            ),
                                            ..Default::default()
                                        }
                                    )),
                                    UiWidgetImpl::from(UiTextButton::new(
                                        context,
                                        UiTextButtonParams {
                                            label: "Cancel".into(),
                                            size: UiTextButtonSize::Small,
                                            hover: Some("misc/brush_stroke_divider.png"),
                                            enabled: true,
                                            on_pressed: UiTextButtonPressed::with_closure(
                                                move |button, context| {
                                                    log::info!("Pressed Button: {}", button.label());
                                                    if let Some(mut menu_rc) = menu_weak_ref_cancel_btn.upgrade() {
                                                        menu_rc.close_message_box(context);
                                                    }
                                                }
                                            ),
                                            ..Default::default()
                                        }
                                    )),
                                ],
                                ..Default::default()
                            }
                        });
                    }
                }
            ),
            ..Default::default()
        }
    ));

    let menu_weak_ref_close_popup_btn = menu.downgrade();
    side_by_side_button_group.add_widget(UiTextButton::new(
        context,
        UiTextButtonParams {
            label: "Close Message Box".into(),
            size: UiTextButtonSize::Normal,
            hover: Some("misc/brush_stroke_divider.png"),
            enabled: true,
            on_pressed: UiTextButtonPressed::with_closure(
                move |button, context| {
                    log::info!("Pressed Button: {}", button.label());

                    if let Some(mut menu_rc) = menu_weak_ref_close_popup_btn.upgrade() {
                        menu_rc.close_message_box(context);
                    }
                }
            ),
            ..Default::default()
        }
    ));

    let separator = UiSeparator::new(
        context,
        UiSeparatorParams {
            separator: Some("misc/brush_stroke_divider.png"),
            thickness: Some(15.0),
            ..Default::default()
        }
    );

    menu.add_widget(menu_heading);
    menu.add_widget(labeled_group);
    menu.add_widget(item_list);
    menu.add_widget(separator.clone());
    menu.add_widget(stacked_button_group);
    menu.add_widget(separator.clone());
    menu.add_widget(side_by_side_button_group);

    log::info!("Sample Menu 1 created.");
}

fn draw_sample_menu_1(context: &mut UiWidgetContext) {
    if let Some(menu) = SAMPLE_MENU_1_INSTANCE.as_mut() {
        menu.draw(context);
    }
}

// ----------------------------------------------
// Sample Menu 2:
// ----------------------------------------------

static SAMPLE_MENU_2_INSTANCE: UiStaticVar<Option<UiMenuRcMut>> = UiStaticVar::new(None);

fn create_sample_menu_2_once(context: &mut UiWidgetContext) {
    if SAMPLE_MENU_2_INSTANCE.is_some() {
        return; // Already created.
    }

    let mut menu = UiMenu::new(
        context,
        UiMenuParams {
            label: Some("Sample Menu 2".into()),
            flags: UiMenuFlags::IsOpen | UiMenuFlags::AlignCenter | UiMenuFlags::AlignRight,
            size: Some(Vec2::new(512.0, 700.0)),
            background: Some("misc/square_page_bg.png"),
            ..Default::default()
        }
    );

    SAMPLE_MENU_2_INSTANCE.set(Some(menu.clone()));

    let button_tooltip = UiTooltipText::new(
        context,
        UiTooltipTextParams {
            text: "This is a Sprite Button".into(),
            font_scale: UiFontScale(0.8),
            background: Some("misc/square_page_bg.png"),
        }
    );

    let button_size = Vec2::new(50.0, 50.0);

    let mut stacked_button_group = UiWidgetGroup::new(
        context,
        UiWidgetGroupParams {
            widget_spacing: 5.0,
            center_vertically: false,
            center_horizontally: true,
            stack_vertically: true,
        }
    );

    stacked_button_group.add_widget(UiSpriteButton::new(
        context,
        UiSpriteButtonParams {
            label: "palette/housing".into(),
            tooltip: Some(button_tooltip.clone()),
            show_tooltip_when_pressed: true,
            size: button_size,
            initial_state: UiSpriteButtonState::Idle,
            ..Default::default()
        }
    ));

    stacked_button_group.add_widget(UiSpriteButton::new(
        context,
        UiSpriteButtonParams {
            label: "palette/roads".into(),
            tooltip: Some(button_tooltip.clone()),
            show_tooltip_when_pressed: true,
            size: button_size,
            initial_state: UiSpriteButtonState::Idle,
            ..Default::default()
        }
    ));

    stacked_button_group.add_widget(UiSpriteButton::new(
        context,
        UiSpriteButtonParams {
            label: "palette/food_and_farming".into(),
            tooltip: Some(button_tooltip.clone()),
            show_tooltip_when_pressed: true,
            size: button_size,
            initial_state: UiSpriteButtonState::Disabled,
            ..Default::default()
        }
    ));

    let mut side_by_side_button_group = UiWidgetGroup::new(
        context,
        UiWidgetGroupParams {
            widget_spacing: 5.0,
            center_vertically: false,
            center_horizontally: true,
            stack_vertically: false,
        }
    );

    side_by_side_button_group.add_widget(UiSpriteButton::new(
        context,
        UiSpriteButtonParams {
            label: "palette/housing".into(),
            tooltip: Some(button_tooltip.clone()),
            show_tooltip_when_pressed: true,
            size: button_size,
            initial_state: UiSpriteButtonState::Idle,
            ..Default::default()
        }
    ));

    side_by_side_button_group.add_widget(UiSpriteButton::new(
        context,
        UiSpriteButtonParams {
            label: "palette/roads".into(),
            tooltip: Some(button_tooltip.clone()),
            show_tooltip_when_pressed: true,
            size: button_size,
            initial_state: UiSpriteButtonState::Idle,
            ..Default::default()
        }
    ));

    side_by_side_button_group.add_widget(UiSpriteButton::new(
        context,
        UiSpriteButtonParams {
            label: "palette/food_and_farming".into(),
            tooltip: Some(button_tooltip.clone()),
            show_tooltip_when_pressed: true,
            size: button_size,
            initial_state: UiSpriteButtonState::Disabled,
            ..Default::default()
        }
    ));

    let slideshow_frame_count = 3;
    let mut slideshow_frames = Vec::new();
    for i in 0..slideshow_frame_count {
        slideshow_frames.push(format!("misc/home_menu_anim/frame{i}.jpg"));
    }

    let slideshow = UiSlideshow::new(
        context,
        UiSlideshowParams {
            loop_mode: UiSlideshowLoopMode::WholeAnim,
            frame_duration_secs: 0.5,
            frames: &slideshow_frames,
            size: Some(Vec2::new(0.0, 250.0)),
            margin_left: 30.0,
            margin_right: 30.0,
            ..Default::default()
        }
    );

    let separator = UiSeparator::new(
        context,
        UiSeparatorParams {
            separator: Some("misc/brush_stroke_divider.png"),
            thickness: Some(15.0),
            ..Default::default()
        }
    );

    menu.add_widget(stacked_button_group);
    menu.add_widget(side_by_side_button_group);
    menu.add_widget(separator);
    menu.add_widget(slideshow);

    log::info!("Sample Menu 2 created.");
}

fn draw_sample_menu_2(context: &mut UiWidgetContext) {
    if let Some(menu) = SAMPLE_MENU_2_INSTANCE.as_mut() {
        menu.draw(context);
    }
}

// ----------------------------------------------
// draw_sample_menus():
// ----------------------------------------------

pub fn draw_sample_menus(context: &mut UiWidgetContext) {
    let prev_theme = context.ui_sys.current_ui_theme();
    context.ui_sys.set_ui_theme(UiTheme::InGame);

    create_sample_menu_1_once(context);
    create_sample_menu_2_once(context);

    draw_sample_menu_1(context);
    draw_sample_menu_2(context);

    context.ui_sys.set_ui_theme(prev_theme);
}
