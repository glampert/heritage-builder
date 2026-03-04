use strum_macros::EnumCount;

use super::*;
use crate::{
    utils::fixed_string::format_fixed_string,
    game::menu::TEXT_BUTTON_HOVERED_SPRITE,
};

// ----------------------------------------------
// Enums / Constants
// ----------------------------------------------

#[repr(usize)]
#[derive(EnumCount)]
enum TaxStatsIdx {
    Taxes,
    TaxGenerated,
    TaxAvailable,
    TaxCollected,
}

#[repr(usize)]
#[derive(EnumCount)]
enum TreasuryStatsIdx {
    Treasury,
    TotalGoldUnits,
    GoldInGlobalTreasury,
    GoldInBuildings,
}

// ----------------------------------------------
// FinancesManagement
// ----------------------------------------------

pub struct FinancesManagement {
    menu: UiMenuRcMut,
    tax_stats_heading_index: UiMenuWidgetIndex,
    treasury_stats_heading_index: UiMenuWidgetIndex,
}

implement_dialog_menu! { FinancesManagement, ["Finances"] }

impl FinancesManagement {
    pub fn new(context: &mut UiWidgetContext) -> Self {
        const FONT_SCALE_HEADING: UiFontScale = UiFontScale(1.2);
        const FONT_SCALE_BODY:    UiFontScale = UiFontScale(1.0);

        const PLACEHOLDER_HEADING: UiText = UiText::empty(FONT_SCALE_HEADING);
        const PLACEHOLDER_BODY:    UiText = UiText::empty(FONT_SCALE_BODY);

        // Tax stats placeholder text.
        const TAX_STATS_TEXT: [UiText; TaxStatsIdx::COUNT] = [
            PLACEHOLDER_HEADING,
            PLACEHOLDER_BODY,
            PLACEHOLDER_BODY,
            PLACEHOLDER_BODY,
        ];

        // Treasury stats placeholder text.
        const TREASURY_STATS_TEXT: [UiText; TreasuryStatsIdx::COUNT] = [
            PLACEHOLDER_HEADING,
            PLACEHOLDER_BODY,
            PLACEHOLDER_BODY,
            PLACEHOLDER_BODY,
        ];

        let mut menu = make_default_layout_dialog_menu(
            context,
            Self::KIND,
            Self::TITLE,
            DEFAULT_DIALOG_MENU_WIDGET_SPACING,
            Option::<Vec<UiWidgetImpl>>::None
        );

        let tax_stats_heading = UiMenuHeading::new(
            context,
            UiMenuHeadingParams {
                lines: TAX_STATS_TEXT.into(),
                separator: Some(LARGE_HORIZONTAL_SEPARATOR_SPRITE),
                ..Default::default()
            }
        );

        let tax_stats_heading_index = menu.add_widget(tax_stats_heading);

        let treasury_stats_heading = UiMenuHeading::new(
            context,
            UiMenuHeadingParams {
                lines: TREASURY_STATS_TEXT.into(),
                separator: Some(LARGE_HORIZONTAL_SEPARATOR_SPRITE),
                ..Default::default()
            }
        );

        let treasury_stats_heading_index = menu.add_widget(treasury_stats_heading);

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

        // Refresh body text when menu is opened.
        menu.set_open_close_callback(UiMenuOpenClose::with_fn(
            |_, context, is_open| {
                if is_open {
                    let this_dialog = DialogMenusSingleton::get_mut()
                        .find_dialog_as::<FinancesManagement>();

                    this_dialog.update_stats(context);
                }
            }
        ));

        button_group.add_widget(ok_button);
        menu.add_widget(button_group);

        Self {
            menu,
            tax_stats_heading_index,
            treasury_stats_heading_index,
        }
    }

    fn update_stats(&mut self, context: &UiWidgetContext) {
        const FMT_LEN: usize = 128;

        let global_treasury = context.sim.treasury();
        let treasury_stats  = &context.world.stats().treasury;

        {
            let heading =
                self.menu.widget_as_mut::<UiMenuHeading>(self.tax_stats_heading_index)
                .unwrap();

            heading.set_line_string(TaxStatsIdx::Taxes as usize, "Taxes");

            heading.set_line_string(
                TaxStatsIdx::TaxGenerated as usize,
                &format_fixed_string!(FMT_LEN, "Tax Generated: {}", treasury_stats.tax_generated));

            heading.set_line_string(
                TaxStatsIdx::TaxAvailable as usize,
                &format_fixed_string!(FMT_LEN, "Tax Available: {}", treasury_stats.tax_available));

            heading.set_line_string(
                TaxStatsIdx::TaxCollected as usize,
                &format_fixed_string!(FMT_LEN, "Tax Collected: {}", treasury_stats.tax_collected));
        }

        {
            let heading =
                self.menu.widget_as_mut::<UiMenuHeading>(self.treasury_stats_heading_index)
                .unwrap();

            heading.set_line_string(TreasuryStatsIdx::Treasury as usize, "Treasury");

            heading.set_line_string(
                TreasuryStatsIdx::TotalGoldUnits as usize,
                &format_fixed_string!(FMT_LEN, "Total Gold Units: {}", treasury_stats.gold_units_total));

            heading.set_line_string(
                TreasuryStatsIdx::GoldInGlobalTreasury as usize,
                &format_fixed_string!(FMT_LEN, "Gold Units In City Treasury: {}", global_treasury.gold_units()));

            heading.set_line_string(
                TreasuryStatsIdx::GoldInBuildings as usize,
                &format_fixed_string!(FMT_LEN, "Gold Units In Tax Offices: {}", treasury_stats.gold_units_in_buildings));
        }
    }
}
