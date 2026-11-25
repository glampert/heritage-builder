// Finance commands: the treasury, the accounts, the tax level and wage, and the debt clock
// (sim::finance, docs/plans/finance.md).

use std::str::FromStr;

use serde::Deserialize;
use serde_json::{Map, Value, json};
use strum::IntoEnumIterator;

use crate::{
    automation::{CommandContext, CommandDef, CommandError, CommandResult, ParamDef, done, parse_params},
    config::GameConfigs,
    sim::finance::{self, LedgerCategory, TaxLevel, YearAccounts},
};

// The highest wage finance.set takes, in gold per worker per year.
const MAX_WAGE: u32 = 1_000;

pub(super) const COMMANDS: &[CommandDef] = &[
    CommandDef {
        name: "finance.state",
        help: "The treasury's balance, the tax level and wage, the debt clock, the city's yearly upkeep, and the \
               accounts for this year and last year by category.",
        params: &[],
        run: cmd_state,
    },
    CommandDef {
        name: "finance.set",
        help: "Set the tax level, the wage, the balance or the debt clock, then report finance.state. Setting the \
               balance isn't recorded in the accounts.",
        params: &[
            ParamDef { name: "tax_level", kind: "string", required: false, help: "None, Low, Normal, High or Harsh." },
            ParamDef { name: "wage", kind: "int", required: false, help: "Gold per employed worker per year, 0 to 1000." },
            ParamDef { name: "balance", kind: "int", required: false, help: "Gold in the treasury; negative is debt." },
            ParamDef {
                name: "months_in_debt",
                kind: "u32",
                required: false,
                help: "The debt clock: month-ends in debt in a row (the next month-end in debt counts on from it).",
            },
        ],
        run: cmd_set,
    },
];

fn cmd_state(context: &mut CommandContext, _params: Value) -> CommandResult {
    context.require_map()?;
    done(state_json(context))
}

fn cmd_set(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        tax_level: Option<String>,
        wage: Option<u32>,
        balance: Option<i64>,
        months_in_debt: Option<u32>,
    }

    let params: Params = parse_params(params)?;
    context.require_map()?;

    // Validate everything before changing anything.
    let tax_level = match &params.tax_level {
        Some(name) => Some(TaxLevel::from_str(name).map_err(|_| {
            let names: Vec<String> = TaxLevel::iter().map(|level| level.to_string()).collect();
            CommandError::invalid_params(format!("Unknown tax level '{name}': use one of {}.", names.join(", ")))
        })?),
        None => None,
    };
    if let Some(wage) = params.wage
        && wage > MAX_WAGE
    {
        return Err(CommandError::invalid_params(format!("The wage must be from 0 to {MAX_WAGE}, got {wage}.")));
    }

    let finance = context.session.sim_mut().finance_mut();
    if let Some(level) = tax_level {
        finance.set_tax_level(level);
    }
    if let Some(wage) = params.wage {
        finance.set_wage(wage);
    }
    if let Some(balance) = params.balance {
        finance.set_balance_unrecorded(balance);
    }
    if let Some(months) = params.months_in_debt {
        finance.set_months_in_debt(months);
    }

    done(state_json(context))
}

fn state_json(context: &CommandContext) -> Value {
    let configs = &GameConfigs::get().sim.finance;
    let finance = context.session.sim().finance();

    json!({
        "balance": finance.balance(),
        "in_debt": finance.is_in_debt(),
        "months_in_debt": finance.months_in_debt(),
        "debt_limit_months": configs.debt_limit_months,
        "debt_overdue": finance.is_debt_overdue(configs),
        "tax_level": finance.tax_level().to_string(),
        "tax_percent": finance.tax_level().percent(configs),
        "wage": finance.wage(),
        "yearly_upkeep": finance::yearly_upkeep(context.session.world(), context.session.tile_map()),
        "this_year": year_json(finance.this_year()),
        "last_year": year_json(finance.last_year()),
    })
}

// One year of accounts. Every category is listed, zero or not, so a query's path always exists.
fn year_json(year: &YearAccounts) -> Value {
    let mut categories = Map::new();
    for category in LedgerCategory::iter() {
        categories.insert(category.name().to_string(), json!(year.total(category)));
    }

    json!({
        "start_balance": year.start_balance(),
        "income": year.income(),
        "spending": year.spending(),
        "net": year.net(),
        "categories": categories,
    })
}
