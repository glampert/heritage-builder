#![allow(clippy::same_item_push)]

use common::format_fixed_string;

use super::*;
use crate::sim::resources::ResourceKind;

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
    pub fn new(context: &mut GameUiContext) -> Self {
        let mut storage_yard_resources_text = vec![PLACEHOLDER_HEADING];
        for _ in ResourceKind::all_except(ResourceKind::Gold) {
            storage_yard_resources_text.push(PLACEHOLDER_BODY);
        }

        let mut granary_resources_text = vec![PLACEHOLDER_HEADING];
        for _ in ResourceKind::foods() {
            granary_resources_text.push(PLACEHOLDER_BODY);
        }

        // Layout the following two headings side-by-side.
        let mut side_by_side_heading_group = UiWidgetGroup::new()
            .with_widget_spacing(Vec2::new(30.0, DEFAULT_DIALOG_MENU_WIDGET_SPACING.y))
            .with_stack_vertically(false);

        let storage_yard_resources_heading_index =
            side_by_side_heading_group.add_widget(UiMenuHeading::new(storage_yard_resources_text));
        let granary_resources_heading_index =
            side_by_side_heading_group.add_widget(UiMenuHeading::new(granary_resources_text));

        let button_group = UiWidgetGroup::new().with_widget(make_dialog_close_button(context, "Ok"));

        let mut menu =
            make_default_layout_dialog_menu(context, Self::KIND, Self::TITLE, DEFAULT_DIALOG_MENU_TEXT_SPACING, []);

        let heading_group_index = menu.add_widget(side_by_side_heading_group);
        menu.add_widget(UiSeparator::new().with_thickness(DEFAULT_DIALOG_MENU_WIDGET_SPACING.x));
        menu.add_widget(button_group);

        // Refresh body text when menu is opened.
        menu.set_open_close_callback(|_, context, is_open| {
            if is_open {
                let this_dialog = super::find::<ResourcesManagement>();
                this_dialog.update_stats(context_as_mut::<GameUiContext>(context));
            }
        });

        Self { menu, heading_group_index, storage_yard_resources_heading_index, granary_resources_heading_index }
    }

    fn update_stats(&mut self, context: &GameUiContext) {
        const FMT_LEN: usize = 128;
        let resources = &context.world.stats().resources;

        let group = self.menu.widget_as_mut::<UiWidgetGroup>(self.heading_group_index).unwrap();

        {
            let heading = group.widget_as_mut::<UiMenuHeading>(self.storage_yard_resources_heading_index).unwrap();

            let mut index = 0;
            heading.set_line_string(index, "Storage Yards"); // Title/Heading
            index += 1;

            resources.storage_yards.for_each(|_, item| {
                heading.set_line_string(index, &format_fixed_string!(FMT_LEN, "{}: {}", item.kind, item.count));
                index += 1;
            });
        }

        {
            let heading = group.widget_as_mut::<UiMenuHeading>(self.granary_resources_heading_index).unwrap();

            let mut index = 0;
            heading.set_line_string(index, "Granaries"); // Title/Heading
            index += 1;

            resources.granaries.for_each(|_, item| {
                heading.set_line_string(index, &format_fixed_string!(FMT_LEN, "{}: {}", item.kind, item.count));
                index += 1;
            });
        }
    }
}
