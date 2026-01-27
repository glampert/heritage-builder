//use super::*;
use crate::ui::widgets::*;

// TODO: WIP: build sample menus
fn build_sample_menu(_context: &mut UiWidgetContext) {
    /*
    context.ui_sys.set_ui_theme(UiTheme::InGame);

    use crate::utils::Vec2;
    use crate::log;

    let slideshow = UiSlideshow::new(context,
        UiSlideshowParams {
            flags: UiSlideshowFlags::default(),
            loop_mode: UiSlideshowLoopMode::WholeAnim,
            frame_duration_secs: 0.5,
            frames: &["misc/home_menu_anim/frame0.jpg", "misc/home_menu_anim/frame1.jpg", "misc/home_menu_anim/frame2.jpg"],
            size: Some(Vec2::new(0.0, 256.0)),
            margin_left: 50.0,
            margin_right: 50.0,
        }
    );

    let test_menu_rc = UiMenu::new(
        context,
        UiMenuParams {
            label: Some("Test Window".into()),
            flags: UiMenuFlags::IsOpen | UiMenuFlags::AlignCenter,
            size: Some(Vec2::new(512.0, 700.0)),
            background: Some("misc/wide_page_bg.png"),
            ..Default::default()
        }
    );

    let mut slideshow_group = UiWidgetGroup::new(UiWidgetGroupParams {
        widget_spacing: 0.0,
        center_vertically: true,
        center_horizontally: true,
        stack_vertically: true,
    });
    slideshow_group.add_widget(slideshow);
    test_menu_rc.as_mut().add_widget(slideshow_group);

    if false {

    let test_menu = test_menu_rc.as_mut();

    let mut group = UiLabeledWidgetGroup::new(
        UiLabeledWidgetGroupParams {
            label_spacing: 5.0,
            widget_spacing: 5.0,
            center_vertically: false,
            center_horizontally: true,
        }
    );

    test_menu.add_widget(UiMenuHeading::new(
        context,
        UiMenuHeadingParams {
            font_scale: 1.8,
            lines: vec!["Settings".into()],
            separator: Some("misc/brush_stroke_divider_2.png"),
            margin_top: 50.0,
            margin_bottom: 0.0,
        }
    ));
    let slider1 = UiSlider::from_u32(
        None,
        1.0,
        0,
        100,
        |_slider, _context| -> u32 { 42 },
        |_slider, _context, new_value: u32| { log::info!("Updated slider value: {new_value}") },
    );
    group.add_widget("Master Volume:".into(), slider1);

    let slider2 = UiSlider::from_u32(
        None,
        1.0,
        0,
        100,
        |_slider, _context| -> u32 { 42 },
        |_slider, _context, new_value: u32| { log::info!("Updated slider value: {new_value}") },
    );
    group.add_widget("Sfx Volume:".into(), slider2);

    let slider3 = UiSlider::from_u32(
        None,
        1.0,
        0,
        100,
        |_slider, _context| -> u32 { 42 },
        |_slider, _context, new_value: u32| { log::info!("Updated slider value: {new_value}") },
    );
    group.add_widget("Music Volume:".into(), slider3);

    let checkbox = UiCheckbox::new(
        None,
        1.0,
        |_checkbox, _context| -> bool { true },
        |_checkbox, _context, new_value: bool| { log::info!("Updated checkbox value: {new_value}") },
    );
    group.add_widget("Enable Volume:".into(), checkbox);

    let text_input = UiTextInput::new(
        None,
        1.0,
        |_input, _context| -> String { "Hello".into() },
        |_input, _context, new_value: String| { log::info!("Updated text input value: {new_value}") },
    );
    group.add_widget("Player Name:".into(), text_input);

    use strum::VariantArray;
    let dropdown = UiDropdown::from_values(
        None,
        1.0,
        0,
        crate::render::TextureFilter::VARIANTS,
        |_dropdown, _context, selection_index, selection_string| { log::info!("Updated dropdown: {selection_index}, {selection_string}") }
    );
    group.add_widget("Texture Filter:".into(), dropdown);

    test_menu.add_widget(UiMenuHeading::new(
        context,
        1.8,
        vec!["Test Heading Line".into(), "Second Line".into()],
        Some("misc/brush_stroke_divider_2.png"),
        50.0,
        0.0
    ));

    let slider = UiSlider::from_u32(
        Some("Master Volume".into()),
        1.0,
        0,
        100,
        |_slider, _context| -> u32 { 42 },
        |_slider, _context, new_value: u32| { log::info!("Updated slider value: {new_value}") },
    );

    let checkbox = UiCheckbox::new(
        Some("Enable Volume".into()),
        1.0,
        |_checkbox, _context| -> bool { true },
        |_checkbox, _context, new_value: bool| { log::info!("Updated checkbox value: {new_value}") },
    );

    let text_input = UiTextInput::new(
        Some("Name".into()),
        1.0,
        |_input, _context| -> String { "Hello".into() },
        |_input, _context, new_value: String| { log::info!("Updated text input value: {new_value}") },
    );

    use strum::VariantArray;
    let dropdown = UiDropdown::from_values(
        Some("Texture Filter".into()),
        1.0,
        0,
        crate::render::TextureFilter::VARIANTS,
        |_dropdown, _context, selection_index, selection_string| { log::info!("Updated dropdown: {selection_index}, {selection_string}") }
    );

    let mut group = UiWidgetGroup::new(15.0, false, true);

    group.add_widget(slider);
    group.add_widget(checkbox);
    group.add_widget(text_input);
    group.add_widget(dropdown);

    let tooltip = UiTooltipText::new(context, "This is a button".into(), 0.8, Some("misc/wide_page_bg.png"));
    group.add_widget(UiSpriteButton::new(
        context,
        "palette/housing".into(),
        Some(tooltip.clone()),
        true,
        Vec2::new(50.0, 50.0),
        UiSpriteButtonState::Idle,
        0.0,
    ));

    group.add_widget(UiSpriteButton::new(
        context,
        "palette/roads".into(),
        Some(tooltip.clone()),
        true,
        Vec2::new(50.0, 50.0),
        UiSpriteButtonState::Idle,
        0.0,
    ));

    group.add_widget(UiSpriteButton::new(
        context,
        "palette/food_and_farming".into(),
        Some(tooltip.clone()),
        true,
        Vec2::new(50.0, 50.0),
        UiSpriteButtonState::Disabled,
        0.0,
    ));

    group.add_widget(UiTextButton::new(
        context,
        "Small Button".into(),
        UiTextButtonSize::Small,
        Some("misc/brush_stroke_divider_2.png"),
        true,
        |button, _context| log::info!("Pressed: {}", button.label())
    ));

    group.add_widget(UiTextButton::new(
        context,
        "Normal Button".into(),
        UiTextButtonSize::Small,
        Some("misc/brush_stroke_divider_2.png"),
        true,
        |button, _context| log::info!("Pressed: {}", button.label())
    ));

    group.add_widget(UiTextButton::new(
        context,
        "Large Button".into(),
        UiTextButtonSize::Small,
        Some("misc/brush_stroke_divider_2.png"),
        true,
        |button, _context| log::info!("Pressed: {}", button.label())
    ));

    group.add_widget(UiTextButton::new(
        context,
        "Disabled Button".into(),
        UiTextButtonSize::Small,
        Some("misc/brush_stroke_divider_2.png"),
        false,
        |button, _context| log::info!("Pressed: {}", button.label())
    ));

    test_menu.add_widget(group);

    let item_list = UiItemList::from_strings(
        Some("Item List".into()),
        1.0,
        Some(Vec2::new(0.0, 128.0)), // use whole parent window width - margin, fixed height
        30.0,
        30.0,
        UiItemListFlags::Border | UiItemListFlags::TextInputField,
        Some(2),
        vec!["One".into(), "Two".into(), "Three".into()],
        |_list, _context, selection_index, selection_string| { log::info!("Updated list: {selection_index:?}, {selection_string}") }
    );

    test_menu.add_widget(item_list);

    let weak_menu = Rc::downgrade(&test_menu_rc);

    test_menu.add_widget(UiTextButton::new(
        context,
        "Open Message Box".into(),
        UiTextButtonSize::Normal,
        Some("misc/brush_stroke_divider_2.png"),
        true,
        move |button, context| {
            log::info!("Pressed: {}", button.label());

            if let Some(menu_rc) = weak_menu.upgrade() {
                let menu_ref_ok_btn = weak_menu.clone();
                let menu_ref_cancel_btn = weak_menu.clone();

                let params = UiMessageBoxParams {
                    label: Some("Test Popup".into()),
                    background: Some("misc/wide_page_bg.png"),
                    contents: vec![
                        UiWidgetImpl::from(UiMenuHeading::new(
                            context,
                            UiMenuHeadingParams {
                                font_scale: 1.2,
                                lines: vec!["Quit to main menu?".into(), "Unsaved progress will be lost".into()],
                                separator: Some("misc/brush_stroke_divider_2.png"),
                                margin_top: 20.0,
                                margin_bottom: 0.0,
                            }
                        ))
                    ],
                    buttons: vec![
                        UiWidgetImpl::from(UiTextButton::new(
                            context,
                            "Ok".into(),
                            UiTextButtonSize::Small,
                            Some("misc/brush_stroke_divider_2.png"),
                            true,
                            move |button, context| {
                                log::info!("Pressed: {}", button.label());
                                if let Some(menu) = menu_ref_ok_btn.upgrade() {
                                    menu.as_mut().close_message_box(context);
                                }
                            }
                        )),
                        UiWidgetImpl::from(UiTextButton::new(
                            context,
                            "Cancel".into(),
                            UiTextButtonSize::Small,
                            Some("misc/brush_stroke_divider_2.png"),
                            true,
                            move |button, context| {
                                log::info!("Pressed: {}", button.label());
                                if let Some(menu) = menu_ref_cancel_btn.upgrade() {
                                    menu.as_mut().close_message_box(context);
                                }
                            }
                        ))
                    ],
                    ..Default::default()
                };

                menu_rc.as_mut().open_message_box(context, params);
            }
        }
    ));

    }
    */
}
