use common::format_fixed_string;
use strum::{EnumCount, IntoEnumIterator};

use super::*;
use crate::{
    config::GameConfigs,
    sim::finance::{LedgerCategory, TaxLevel},
};

// ----------------------------------------------
// Enums / Constants
// ----------------------------------------------

#[repr(usize)]
#[derive(EnumCount)]
enum AccountsIdx {
    Heading,
    Taxes,
    Construction,
    Wages,
    Upkeep,
    Net,
}

#[repr(usize)]
#[derive(EnumCount)]
enum TreasuryIdx {
    Heading,
    Balance,
    TaxesOwed,
}

// The wage slider's range, in gold per worker per year.
const MAX_WAGE: u32 = 24;

const FMT_LEN: usize = 128;

// ----------------------------------------------
// FinancesManagement
// ----------------------------------------------

// The yearly accounts: income and spending by category, this year so far against last year,
// the treasury and the debt clock; and the tax level and wage the player sets (sim::finance).
pub struct FinancesManagement {
    menu: UiMenuRcMut,
    accounts_heading_index: UiMenuWidgetIndex,
    treasury_heading_index: UiMenuWidgetIndex,
}

implement_dialog_menu! { FinancesManagement, ["Finances"] }

impl FinancesManagement {
    pub fn new(context: &mut GameUiContext) -> Self {
        const ACCOUNTS_TEXT: [UiText; AccountsIdx::COUNT] =
            [PLACEHOLDER_HEADING, PLACEHOLDER_BODY, PLACEHOLDER_BODY, PLACEHOLDER_BODY, PLACEHOLDER_BODY, PLACEHOLDER_BODY];
        const TREASURY_TEXT: [UiText; TreasuryIdx::COUNT] = [PLACEHOLDER_HEADING, PLACEHOLDER_BODY, PLACEHOLDER_BODY];

        let mut menu =
            make_default_layout_dialog_menu(context, Self::KIND, Self::TITLE, DEFAULT_DIALOG_MENU_TEXT_SPACING, []);

        let accounts_heading_index =
            menu.add_widget(skin::with_heading_separator(context, UiMenuHeading::new(ACCOUNTS_TEXT), HeadingLevel::Section));
        let treasury_heading_index =
            menu.add_widget(skin::with_heading_separator(context, UiMenuHeading::new(TREASURY_TEXT), HeadingLevel::Section));

        // The player's settings, read from and written to the treasury as they change.
        let tax_level = UiDropdown::new(TaxLevel::iter())
            .with_font_scale(DEFAULT_DIALOG_MENU_WIDGET_FONT_SCALE)
            .on_get_current_selection(|_, context| context_as_mut::<GameUiContext>(context).sim.finance().tax_level().index())
            .on_selection_changed(|dropdown, context| {
                if let Some(level) = TaxLevel::iter().nth(dropdown.current_selection_index()) {
                    context_as_mut::<GameUiContext>(context).sim.finance_mut().set_tax_level(level);
                }
            });

        let wage = UiSlider::new(0..=MAX_WAGE)
            .with_font_scale(DEFAULT_DIALOG_MENU_WIDGET_FONT_SCALE)
            .on_read_value(|_, context| -> u32 { context_as_mut::<GameUiContext>(context).sim.finance().wage() })
            .on_update_value(|_, context, wage: u32| {
                context_as_mut::<GameUiContext>(context).sim.finance_mut().set_wage(wage);
            });

        let mut settings = UiLabeledWidgetGroup::new()
            .with_label_spacing(DEFAULT_DIALOG_MENU_WIDGET_LABEL_SPACING)
            .with_widget_spacing(DEFAULT_DIALOG_MENU_WIDGET_SPACING.y);
        settings.add_widget("Tax Level".to_string(), UiWidgetImpl::from(tax_level));
        settings.add_widget("Yearly Wage".to_string(), UiWidgetImpl::from(wage));
        menu.add_widget(settings);

        menu.add_widget(UiSeparator::new().with_thickness(DEFAULT_DIALOG_MENU_WIDGET_SPACING.x));
        menu.add_widget(UiWidgetGroup::new().with_widget(make_dialog_close_button(context, ButtonRole::Primary, "Ok")));

        // Refresh the accounts when the menu is opened.
        menu.set_open_close_callback(|_, context, is_open| {
            if is_open {
                let this_dialog = super::find::<FinancesManagement>();
                this_dialog.update_stats(context_as_mut::<GameUiContext>(context));
            }
        });

        Self { menu, accounts_heading_index, treasury_heading_index }
    }

    fn update_stats(&mut self, context: &GameUiContext) {
        let finance = context.sim.finance();
        let (this_year, last_year) = (finance.this_year(), finance.last_year());

        // "Wages: -453 (last year -380)": income counts up, spending down.
        let line = |label: &str, this: i64, last: i64| format_fixed_string!(FMT_LEN, "{label}: {this} (last year {last})");
        let category = |category: LedgerCategory| {
            let sign = if category.is_income() { 1 } else { -1 };
            line(category.name(), sign * this_year.total(category), sign * last_year.total(category))
        };

        {
            let heading = self.menu.widget_as_mut::<UiMenuHeading>(self.accounts_heading_index).unwrap();
            heading.set_line_string(AccountsIdx::Heading as usize, "This Year's Accounts");
            heading.set_line_string(AccountsIdx::Taxes as usize, &category(LedgerCategory::Taxes));
            heading.set_line_string(AccountsIdx::Construction as usize, &category(LedgerCategory::Construction));
            heading.set_line_string(AccountsIdx::Wages as usize, &category(LedgerCategory::Wages));
            heading.set_line_string(AccountsIdx::Upkeep as usize, &category(LedgerCategory::Upkeep));
            heading.set_line_string(AccountsIdx::Net as usize, &line("Net", this_year.net(), last_year.net()));
        }

        {
            let heading = self.menu.widget_as_mut::<UiMenuHeading>(self.treasury_heading_index).unwrap();
            heading.set_line_string(TreasuryIdx::Heading as usize, "Treasury");
            heading.set_line_string(
                TreasuryIdx::Balance as usize,
                &format_fixed_string!(FMT_LEN, "{} gold, {}", finance.balance(), debt_text(finance.months_in_debt())),
            );
            heading.set_line_string(
                TreasuryIdx::TaxesOwed as usize,
                &format_fixed_string!(FMT_LEN, "Taxes owed by houses: {}", context.world.stats().treasury.tax_available),
            );
        }
    }
}

// The debt clock, in words: how long the city has been in debt and how long it has left.
fn debt_text(months_in_debt: u32) -> arrayvec::ArrayString<FMT_LEN> {
    let limit = GameConfigs::get().sim.finance.debt_limit_months;
    let months = |count: u32| if count == 1 { "month" } else { "months" };

    if months_in_debt == 0 {
        format_fixed_string!(FMT_LEN, "not in debt")
    } else if limit == 0 {
        format_fixed_string!(FMT_LEN, "in debt for {months_in_debt} {}", months(months_in_debt))
    } else if months_in_debt >= limit {
        format_fixed_string!(FMT_LEN, "in debt for {months_in_debt} {}: overdue", months(months_in_debt))
    } else {
        let left = limit - months_in_debt;
        format_fixed_string!(FMT_LEN, "in debt: overdue in {left} {}", months(left))
    }
}
