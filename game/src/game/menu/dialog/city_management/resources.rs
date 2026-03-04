#![allow(clippy::same_item_push)]

use super::*;
use crate::{
    utils::fixed_string::format_fixed_string,
    game::{
        sim::resources::ResourceKind,
        menu::TEXT_BUTTON_HOVERED_SPRITE,
    },
};

// ----------------------------------------------
// ResourcesManagement
// ----------------------------------------------

pub struct ResourcesManagement {
    menu: UiMenuRcMut,
    heading_group_index: UiMenuWidgetIndex,
    storage_yard_resources_heading_index: UiWidgetGroupWidgetIndex,
    granary_resources_heading_index: UiWidgetGroupWidgetIndex,
}

implement_dialog_menu! { ResourcesManagement, ["Resources"] }

impl ResourcesManagement {
    pub fn new(context: &mut UiWidgetContext) -> Self {
        let mut storage_yard_resources_text = vec![PLACEHOLDER_HEADING];
        for _ in ResourceKind::all_except(ResourceKind::Gold) {
            storage_yard_resources_text.push(PLACEHOLDER_BODY);
        }

        let mut granary_resources_text = vec![PLACEHOLDER_HEADING];
        for _ in ResourceKind::foods() {
            granary_resources_text.push(PLACEHOLDER_BODY);
        }

        // Layout the following two headings side-by-side.
        let mut side_by_side_heading_group = UiWidgetGroup::new(
            context,
            UiWidgetGroupParams {
                widget_spacing: Vec2::new(30.0, DEFAULT_DIALOG_MENU_WIDGET_SPACING.y),
                center_vertically: false,
                center_horizontally: true,
                stack_vertically: false,
                ..Default::default()
            }
        );

        let storage_yard_resources_heading = UiMenuHeading::new(
            context,
            UiMenuHeadingParams {
                lines: storage_yard_resources_text,
                ..Default::default()
            }
        );

        let storage_yard_resources_heading_index =
            side_by_side_heading_group.add_widget(storage_yard_resources_heading);

        let granary_resources_heading = UiMenuHeading::new(
            context,
            UiMenuHeadingParams {
                lines: granary_resources_text,
                ..Default::default()
            }
        );

        let granary_resources_heading_index =
            side_by_side_heading_group.add_widget(granary_resources_heading);

        let mut button_group = UiWidgetGroup::new(
            context,
            UiWidgetGroupParams {
                center_vertically: false,
                ..Default::default()
            }
        );

        let ok_button = UiTextButton::new(
            context,
            UiTextButtonParams {
                label: "Ok".into(),
                hover: Some(TEXT_BUTTON_HOVERED_SPRITE),
                enabled: true,
                on_pressed: UiTextButtonPressed::with_fn(|_, context| {
                    super::close_current(context);
                }),
                ..Default::default()
            }
        );

        button_group.add_widget(ok_button);

        let spacing = UiSeparator::new(
            context,
            UiSeparatorParams {
                thickness: Some(DEFAULT_DIALOG_MENU_WIDGET_SPACING.x),
                ..Default::default()
            }
        );

        let mut menu = make_default_layout_dialog_menu(
            context,
            Self::KIND,
            Self::TITLE,
            DEFAULT_DIALOG_MENU_WIDGET_SPACING,
            Option::<Vec<UiWidgetImpl>>::None
        );

        let heading_group_index = menu.add_widget(side_by_side_heading_group);
        menu.add_widget(spacing);
        menu.add_widget(button_group);

        // Refresh body text when menu is opened.
        menu.set_open_close_callback(UiMenuOpenClose::with_fn(
            |_, context, is_open| {
                if is_open {
                    let this_dialog = DialogMenusSingleton::get_mut()
                        .find_dialog_as::<ResourcesManagement>();

                    this_dialog.update_stats(context);
                }
            }
        ));

        Self {
            menu,
            heading_group_index,
            storage_yard_resources_heading_index,
            granary_resources_heading_index,
        }
    }

    fn update_stats(&mut self, context: &UiWidgetContext) {
        const FMT_LEN: usize = 128;
        let resources = &context.world.stats().resources;

        let group =
            self.menu.widget_as_mut::<UiWidgetGroup>(self.heading_group_index)
            .unwrap();

        {
            let heading =
                group.widget_as_mut::<UiMenuHeading>(self.storage_yard_resources_heading_index)
                .unwrap();

            let mut index = 0;
            heading.set_line_string(index, "Storage Yards"); // Title/Heading
            index += 1;

            resources.storage_yards.for_each(|_, item| {
                heading.set_line_string(
                    index,
                    &format_fixed_string!(FMT_LEN, "{}: {}", item.kind, item.count)
                );
                index += 1;
            });
        }

        {
            let heading =
                group.widget_as_mut::<UiMenuHeading>(self.granary_resources_heading_index)
                .unwrap();

            let mut index = 0;
            heading.set_line_string(index, "Granaries"); // Title/Heading
            index += 1;

            resources.granaries.for_each(|_, item| {
                heading.set_line_string(
                    index,
                    &format_fixed_string!(FMT_LEN, "{}: {}", item.kind, item.count)
                );
                index += 1;
            });
        }
    }
}
