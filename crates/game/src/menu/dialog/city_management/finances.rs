use common::format_fixed_string;
use strum::EnumCount;

use super::*;

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
    pub fn new(context: &mut GameUiContext) -> Self {
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

        let mut menu =
            make_default_layout_dialog_menu(context, Self::KIND, Self::TITLE, DEFAULT_DIALOG_MENU_TEXT_SPACING, []);

        let tax_stats_heading =
            UiMenuHeading::new(TAX_STATS_TEXT).with_separator(context, LARGE_HORIZONTAL_SEPARATOR_SPRITE);
        let tax_stats_heading_index = menu.add_widget(tax_stats_heading);

        let treasury_stats_heading =
            UiMenuHeading::new(TREASURY_STATS_TEXT).with_separator(context, LARGE_HORIZONTAL_SEPARATOR_SPRITE);
        let treasury_stats_heading_index = menu.add_widget(treasury_stats_heading);

        menu.add_widget(UiWidgetGroup::new().with_widget(make_dialog_close_button(context, "Ok")));

        // Refresh body text when menu is opened.
        menu.set_open_close_callback(|_, context, is_open| {
            if is_open {
                let this_dialog = super::find::<FinancesManagement>();
                this_dialog.update_stats(context_as_mut::<GameUiContext>(context));
            }
        });

        Self { menu, tax_stats_heading_index, treasury_stats_heading_index }
    }

    fn update_stats(&mut self, context: &GameUiContext) {
        const FMT_LEN: usize = 128;

        let global_treasury = context.sim.treasury();
        let treasury_stats = &context.world.stats().treasury;

        {
            let heading = self.menu.widget_as_mut::<UiMenuHeading>(self.tax_stats_heading_index).unwrap();

            heading.set_line_string(TaxStatsIdx::Taxes as usize, "Taxes");

            heading.set_line_string(
                TaxStatsIdx::TaxGenerated as usize,
                &format_fixed_string!(FMT_LEN, "Tax Generated: {}", treasury_stats.tax_generated),
            );

            heading.set_line_string(
                TaxStatsIdx::TaxAvailable as usize,
                &format_fixed_string!(FMT_LEN, "Tax Available: {}", treasury_stats.tax_available),
            );

            heading.set_line_string(
                TaxStatsIdx::TaxCollected as usize,
                &format_fixed_string!(FMT_LEN, "Tax Collected: {}", treasury_stats.tax_collected),
            );
        }

        {
            let heading = self.menu.widget_as_mut::<UiMenuHeading>(self.treasury_stats_heading_index).unwrap();

            heading.set_line_string(TreasuryStatsIdx::Treasury as usize, "Treasury");

            heading.set_line_string(
                TreasuryStatsIdx::TotalGoldUnits as usize,
                &format_fixed_string!(FMT_LEN, "Total Gold Units: {}", treasury_stats.gold_units_total),
            );

            heading.set_line_string(
                TreasuryStatsIdx::GoldInGlobalTreasury as usize,
                &format_fixed_string!(FMT_LEN, "Gold Units In City Treasury: {}", global_treasury.gold_units()),
            );

            heading.set_line_string(
                TreasuryStatsIdx::GoldInBuildings as usize,
                &format_fixed_string!(FMT_LEN, "Gold Units In Tax Offices: {}", treasury_stats.gold_units_in_buildings),
            );
        }
    }
}
