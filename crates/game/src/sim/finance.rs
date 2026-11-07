// The city's money: one treasury balance, which can go negative (debt), the yearly accounts by
// category, and the player's tax level and wage. See docs/plans/finance.md.
//
// - Taxes come in when a tax collector returns to its office (`Finance::earn`).
// - Placement charges and refunds go through `spend` and `refund` (Spawner).
// - Wages and upkeep are charged monthly (Phase 1).
// - Cheats and debug edits change the balance without touching the accounts.

use serde::{Deserialize, Serialize};
use strum::{Display, EnumCount, EnumIter, EnumString, IntoEnumIterator};

use super::calendar::CalendarEvent;
use crate::{cheats, config::FinanceConfigs, world::stats::WorldStats};

// ----------------------------------------------
// LedgerCategory
// ----------------------------------------------

// A line of the accounts. Saves name the category ("Taxes"), never its position, so a new one
// can go anywhere in the list without a save-version bump; removing or renaming one needs a
// migration.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Display, EnumCount, EnumIter, EnumString)]
pub enum LedgerCategory {
    // Income:
    Taxes,

    // Spending:
    Construction,
    Wages,
    Upkeep,
}

pub const LEDGER_CATEGORY_COUNT: usize = LedgerCategory::COUNT;

impl LedgerCategory {
    #[inline]
    pub fn is_income(self) -> bool {
        matches!(self, Self::Taxes)
    }

    #[inline]
    pub fn index(self) -> usize {
        self as usize
    }

    #[inline]
    pub fn name(self) -> &'static str {
        match self {
            Self::Taxes => "Taxes",
            Self::Construction => "Construction",
            Self::Wages => "Wages",
            Self::Upkeep => "Upkeep",
        }
    }
}

// Per-category arrays saved as a map by category name, zero entries left out: { "Taxes": 412 }.
// An unknown name fails the load.
pub(crate) mod per_category_map {
    use std::{fmt, marker::PhantomData, str::FromStr};

    use serde::{
        Deserialize, Deserializer, Serialize, Serializer,
        de::{self, MapAccess, Visitor},
        ser::SerializeMap,
    };
    use strum::IntoEnumIterator;

    use super::{LEDGER_CATEGORY_COUNT, LedgerCategory};

    pub fn serialize<T, S>(values: &[T; LEDGER_CATEGORY_COUNT], ser: S) -> Result<S::Ok, S::Error>
    where
        T: Serialize + Default + PartialEq,
        S: Serializer,
    {
        let default = T::default();
        let mut map = ser.serialize_map(None)?;
        for category in LedgerCategory::iter() {
            let value = &values[category.index()];
            if *value != default {
                map.serialize_entry(category.name(), value)?;
            }
        }
        map.end()
    }

    pub fn deserialize<'de, T, D>(de: D) -> Result<[T; LEDGER_CATEGORY_COUNT], D::Error>
    where
        T: Deserialize<'de> + Default + Copy,
        D: Deserializer<'de>,
    {
        struct PerCategoryVisitor<T>(PhantomData<T>);

        impl<'de, T> Visitor<'de> for PerCategoryVisitor<T>
        where
            T: Deserialize<'de> + Default + Copy,
        {
            type Value = [T; LEDGER_CATEGORY_COUNT];

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a map from ledger category names to values")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> Result<Self::Value, A::Error> {
                let mut values = [T::default(); LEDGER_CATEGORY_COUNT];
                while let Some(name) = access.next_key::<String>()? {
                    let category = LedgerCategory::from_str(&name)
                        .map_err(|_| de::Error::custom(format!("unknown ledger category '{name}'")))?;
                    values[category.index()] = access.next_value()?;
                }
                Ok(values)
            }
        }

        de.deserialize_map(PerCategoryVisitor(PhantomData))
    }
}

// ----------------------------------------------
// TaxLevel
// ----------------------------------------------

// The tax level the player sets. Each one pays a percentage of the houses' base tax
// (FinanceConfigs::tax_level_percent); Normal pays the base amounts.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default, Display, EnumCount, EnumIter, EnumString, Serialize, Deserialize)]
pub enum TaxLevel {
    None,
    Low,
    #[default]
    Normal,
    High,
    Harsh,
}

impl TaxLevel {
    #[inline]
    pub fn index(self) -> usize {
        self as usize
    }

    // Percentage of the base tax this level pays.
    #[inline]
    pub fn percent(self, configs: &FinanceConfigs) -> u32 {
        configs.tax_level_percent[self.index()]
    }
}

// ----------------------------------------------
// YearAccounts
// ----------------------------------------------

// One year of the accounts: the balance it started with and each category's total. Income
// categories count what came in, spending categories what went out, both as positive amounts.
// A refund takes back off its category, so a total can go negative (a refund for a building
// paid for last year).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct YearAccounts {
    start_balance: i64,
    #[serde(with = "per_category_map")]
    totals: [i64; LEDGER_CATEGORY_COUNT],
}

impl YearAccounts {
    #[inline]
    pub fn new(start_balance: i64) -> Self {
        Self { start_balance, totals: [0; LEDGER_CATEGORY_COUNT] }
    }

    #[inline]
    pub fn start_balance(&self) -> i64 {
        self.start_balance
    }

    #[inline]
    pub fn total(&self, category: LedgerCategory) -> i64 {
        self.totals[category.index()]
    }

    pub fn income(&self) -> i64 {
        LedgerCategory::iter().filter(|c| c.is_income()).map(|c| self.total(c)).sum()
    }

    pub fn spending(&self) -> i64 {
        LedgerCategory::iter().filter(|c| !c.is_income()).map(|c| self.total(c)).sum()
    }

    #[inline]
    pub fn net(&self) -> i64 {
        self.income() - self.spending()
    }
}

// ----------------------------------------------
// Finance
// ----------------------------------------------

#[derive(Serialize, Deserialize)]
pub struct Finance {
    // Gold in the treasury. Negative is debt.
    balance: i64,

    // The player's settings.
    tax_level: TaxLevel,
    wage: u32, // Gold per employed worker per year.

    // Consecutive month-ends the balance was negative.
    months_in_debt: u32,

    // Yearly charges are paid monthly, a twelfth at a time; what a month's division leaves over
    // carries to the next, per category, so no gold is lost to rounding.
    #[serde(with = "per_category_map")]
    carries: [i64; LEDGER_CATEGORY_COUNT],

    this_year: YearAccounts,
    last_year: YearAccounts,
}

impl Finance {
    pub fn new(starting_gold: u32, configs: &FinanceConfigs) -> Self {
        let balance = i64::from(starting_gold);
        Self {
            balance,
            tax_level: configs.default_tax_level,
            wage: configs.default_wage,
            months_in_debt: 0,
            carries: [0; LEDGER_CATEGORY_COUNT],
            this_year: YearAccounts::new(balance),
            last_year: YearAccounts::new(balance),
        }
    }

    // ----------------------
    // Balance:
    // ----------------------

    #[inline]
    pub fn balance(&self) -> i64 {
        self.balance
    }

    #[inline]
    pub fn is_in_debt(&self) -> bool {
        self.balance < 0
    }

    // True if the treasury can pay `cost` now. The tile cost cheat makes everything affordable.
    #[inline]
    pub fn can_afford(&self, cost: u32) -> bool {
        cheats::get().ignore_tile_cost || i64::from(cost) <= self.balance
    }

    // Gold the treasury lacks to pay `cost`, 0 if it can.
    #[inline]
    pub fn shortfall(&self, cost: u32) -> u32 {
        if cheats::get().ignore_tile_cost {
            return 0;
        }
        (i64::from(cost) - self.balance.max(0)).clamp(0, i64::from(u32::MAX)) as u32
    }

    // Income: adds to the balance and to the category's total for the year.
    #[inline]
    pub fn earn(&mut self, category: LedgerCategory, amount: u32) {
        debug_assert!(category.is_income(), "{category} is not an income category!");
        self.balance += i64::from(amount);
        self.this_year.totals[category.index()] += i64::from(amount);
    }

    // Spending: takes from the balance, which may go negative, and adds to the category's total.
    #[inline]
    pub fn spend(&mut self, category: LedgerCategory, amount: u32) {
        debug_assert!(!category.is_income(), "{category} is not a spending category!");
        self.balance -= i64::from(amount);
        self.this_year.totals[category.index()] += i64::from(amount);
    }

    // Gives back spending (undo, a cancelled site): adds to the balance and takes the amount
    // back off the category's total for this year.
    #[inline]
    pub fn refund(&mut self, category: LedgerCategory, amount: u32) {
        debug_assert!(!category.is_income(), "{category} is not a spending category!");
        self.balance += i64::from(amount);
        self.this_year.totals[category.index()] -= i64::from(amount);
    }

    // Cheats, debug tools and automation: change the balance without recording it.
    #[inline]
    pub fn add_unrecorded(&mut self, amount: i64) {
        self.balance += amount;
    }

    #[inline]
    pub fn set_balance_unrecorded(&mut self, balance: i64) {
        self.balance = balance;
    }

    #[inline]
    pub fn tally(&self, stats: &mut WorldStats) {
        stats.treasury.gold_units_total += self.balance;
    }

    // ----------------------
    // Settings:
    // ----------------------

    #[inline]
    pub fn tax_level(&self) -> TaxLevel {
        self.tax_level
    }

    #[inline]
    pub fn set_tax_level(&mut self, level: TaxLevel) {
        self.tax_level = level;
    }

    #[inline]
    pub fn wage(&self) -> u32 {
        self.wage
    }

    #[inline]
    pub fn set_wage(&mut self, wage: u32) {
        self.wage = wage;
    }

    // ----------------------
    // Debt:
    // ----------------------

    #[inline]
    pub fn months_in_debt(&self) -> u32 {
        self.months_in_debt
    }

    // ----------------------
    // Accounts:
    // ----------------------

    #[inline]
    pub fn this_year(&self) -> &YearAccounts {
        &self.this_year
    }

    #[inline]
    pub fn last_year(&self) -> &YearAccounts {
        &self.last_year
    }

    #[inline]
    pub fn carry(&self, category: LedgerCategory) -> i64 {
        self.carries[category.index()]
    }

    // A new year: this year's accounts become last year's, and a new year starts from the
    // current balance.
    pub fn on_new_year(&mut self) {
        self.last_year = std::mem::replace(&mut self.this_year, YearAccounts::new(self.balance));
    }

    // A world step's calendar events, after its commands ran: each new month's charges, then
    // the new year's accounts, so a month's charges count in the year it belongs to.
    pub fn on_calendar_events(&mut self, events: &[CalendarEvent]) {
        for event in events {
            if let CalendarEvent::NewYear(_) = event {
                self.on_new_year();
            }
        }
    }
}

// ----------------------------------------------
// Unit tests
// ----------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn finance() -> Finance {
        Finance::new(100, &FinanceConfigs::default())
    }

    #[test]
    fn ledger_records_by_category() {
        let mut finance = finance();
        finance.earn(LedgerCategory::Taxes, 40);
        finance.spend(LedgerCategory::Construction, 15);
        finance.refund(LedgerCategory::Construction, 5);

        assert_eq!(finance.balance(), 130);
        assert_eq!(finance.this_year().total(LedgerCategory::Taxes), 40);
        assert_eq!(finance.this_year().total(LedgerCategory::Construction), 10);
        assert_eq!(finance.this_year().net(), 30);
    }

    #[test]
    fn spending_can_go_into_debt() {
        let mut finance = finance();
        finance.spend(LedgerCategory::Wages, 150);
        assert_eq!(finance.balance(), -50);
        assert!(finance.is_in_debt());
        // (can_afford and shortfall read the cheats, a main-thread global: tests/finance.rs.)
    }

    #[test]
    fn new_year_moves_the_accounts() {
        let mut finance = finance();
        finance.earn(LedgerCategory::Taxes, 20);
        finance.on_new_year();

        assert_eq!(finance.last_year().total(LedgerCategory::Taxes), 20);
        assert_eq!(finance.last_year().start_balance(), 100);
        assert_eq!(finance.this_year().total(LedgerCategory::Taxes), 0);
        assert_eq!(finance.this_year().start_balance(), 120);
    }

    #[test]
    fn accounts_save_by_name() {
        let mut finance = finance();
        finance.earn(LedgerCategory::Taxes, 7);
        finance.spend(LedgerCategory::Upkeep, 3);

        let json = serde_json::to_value(finance.this_year()).unwrap();
        assert_eq!(json, serde_json::json!({ "start_balance": 100, "totals": { "Taxes": 7, "Upkeep": 3 } }));

        // Keys in any order load onto the right categories.
        let loaded: YearAccounts =
            serde_json::from_value(serde_json::json!({ "start_balance": 1, "totals": { "Upkeep": 3, "Taxes": 7 } })).unwrap();
        assert_eq!(loaded.total(LedgerCategory::Taxes), 7);
        assert_eq!(loaded.total(LedgerCategory::Upkeep), 3);
    }

    #[test]
    fn accounts_reject_unknown_categories() {
        let result: Result<YearAccounts, _> =
            serde_json::from_value(serde_json::json!({ "start_balance": 0, "totals": { "Bribes": 1 } }));
        assert!(result.err().expect("an unknown category must fail").to_string().contains("unknown ledger category 'Bribes'"));
    }

    #[test]
    fn tax_levels_save_by_name() {
        assert_eq!(serde_json::to_value(TaxLevel::Harsh).unwrap(), serde_json::json!("Harsh"));
        let configs = FinanceConfigs::default();
        assert_eq!(TaxLevel::Normal.percent(&configs), 100);
        assert_eq!(TaxLevel::None.percent(&configs), 0);
    }
}
