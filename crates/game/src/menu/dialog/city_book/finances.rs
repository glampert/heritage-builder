// The Finances page (city-finances-<realm>.jpg): the yearly accounts, this year so far against
// last year; the treasury, the debt clock and the balance's trend; the tax level and the wage the
// player sets, with what they bring.

use common::Vec2;
use engine::ui::{self, UiString, UiTextAnchor, text::UiTextCategory, widgets::*};
use serde_json::json;
use strum::IntoEnumIterator;

use super::{BODY, CityPage, PageContext, Paint, body_canvas, number};
use crate::{
    city::CityHistory,
    config::GameConfigs,
    menu::skin,
    sim::finance::{LedgerCategory, TaxLevel, YearAccounts},
    ui_context::GameUiContext,
};

pub(super) struct Finances;

const ACCOUNTS_WIDTH: f32 = 640.0;
const GAP: f32 = 20.0;
const TREASURY_HEIGHT: f32 = 250.0;
// The wage's steps, in gold per worker a year (more if the treasury's is higher).
const MAX_WAGE: u32 = 24;
const TREND_MONTHS: usize = 12;

impl CityPage for Finances {
    fn id(&self) -> &'static str {
        "finances"
    }

    fn icon(&self) -> &'static str {
        "coin"
    }

    fn note(&self, context: &GameUiContext) -> String {
        let configs = &GameConfigs::get().sim;
        let date = context.sim.calendar().date(configs.days_per_month, configs.seconds_per_month);
        ui::text::format(UiTextCategory::City, "book.note.finances", &[
            ("year", &date.year.to_string()),
            ("month", &date.month.to_string()),
        ])
    }

    fn body(&self, context: &mut GameUiContext, page: &PageContext) -> UiWidgetGroup {
        let mut columns = UiColumns::new().with_spacing(page.pt(GAP)).with_width(page.pt(BODY.x));
        columns.add_column(
            UiColumnWidth::Fixed(page.pt(ACCOUNTS_WIDTH)),
            super::left_group().with_widget(accounts(context, page)),
        );
        let right = super::left_group()
            .with_widget_spacing(Vec2::zero())
            .with_widget(treasury(context, page))
            .with_widget(super::spacer(page.pt(GAP)))
            .with_widget(taxes_and_wages(context, page));
        columns.add_column(UiColumnWidth::Fraction(1.0), right);
        super::left_group().with_widget_spacing(Vec2::zero()).with_widget(columns)
    }
}

// How a ledger line looks.
#[derive(Copy, Clone, PartialEq, Eq)]
enum Line {
    Row,   // A category.
    Total, // Income, spending: ruled above, bold.
    Net,   // The year's net: ruled above, coloured by its sign.
    Note,  // The balance at the year's start and end.
}

// The accounts: a row per category and the totals, last year's and this year's.
fn accounts(context: &GameUiContext, page: &PageContext) -> UiCanvas {
    let finance = context.sim.finance();
    let (last, this) = (finance.last_year().clone(), finance.this_year().clone());
    let get = |key: &str| ui::text::get(UiTextCategory::City, key).to_string();
    let signed = |accounts: &YearAccounts, category: LedgerCategory| {
        let total = accounts.total(category);
        if category.is_income() { total } else { -total }
    };
    let both = |f: &dyn Fn(&YearAccounts) -> i64| [f(&last), f(&this)];
    let mut lines: Vec<(String, [i64; 2], Line)> = Vec::new();
    lines.push((get("book.accounts.taxes"), both(&|a| signed(a, LedgerCategory::Taxes)), Line::Row));
    lines.push((get("book.accounts.offerings"), both(&|a| signed(a, LedgerCategory::Offerings)), Line::Row));
    lines.push((get("book.accounts.income"), both(&|a| a.income()), Line::Total));
    for (key, category) in [
        ("construction", LedgerCategory::Construction),
        ("wages", LedgerCategory::Wages),
        ("upkeep", LedgerCategory::Upkeep),
    ] {
        lines.push((get(&format!("book.accounts.{key}")), both(&|a| signed(a, category)), Line::Row));
    }
    lines.push((get("book.accounts.spending"), both(&|a| -a.spending()), Line::Total));
    lines.push((get("book.accounts.net"), both(&|a| a.net()), Line::Net));
    lines.push((get("book.accounts.start"), both(&|a| a.start_balance()), Line::Note));
    lines.push((get("book.accounts.end"), both(&|a| a.start_balance() + a.net()), Line::Note));

    let configs = &GameConfigs::get().sim;
    let month = context.sim.calendar().date(configs.days_per_month, configs.seconds_per_month).month;
    let footnote = ui::text::format(UiTextCategory::City, "book.accounts.covers", &[("month", &month.to_string())]);
    let value = json!(
        lines
            .iter()
            .map(|(name, values, _)| json!({ "line": name, "last_year": values[0], "this_year": values[1] }))
            .collect::<Vec<_>>()
    );
    let title = get("book.accounts");
    let columns = [get("book.accounts.last_year"), get("book.accounts.this_year")];
    body_canvas(page, ACCOUNTS_WIDTH, BODY.y, "accounts", value, move |p: &Paint| {
        let w = ACCOUNTS_WIDTH;
        p.card(0.0, 0.0, w, BODY.y, Some(&title), None);
        let (c1, c2) = (w - 214.0, w - 28.0);
        let header = 84.0;
        p.text(c1, header, &columns[0], 18.0, p.c.sub, UiTextAnchor::End, false);
        p.text(c2, header, &columns[1], 18.0, p.c.sub, UiTextAnchor::End, false);
        let mut y = header + 52.0;
        for (name, values, line) in &lines {
            if matches!(line, Line::Total | Line::Net) {
                p.painter.line(p.at(24.0, y - 34.0), p.at(w - 24.0, y - 34.0), 1.5 * p.k, p.c.text);
            }
            if *line == Line::Note && name == &lines[lines.len() - 2].0 {
                y += 26.0;
            }
            let size = if *line == Line::Net { 22.0 } else { 20.0 };
            let indent = if *line == Line::Row { 48.0 } else { 24.0 };
            let color = if *line == Line::Note { p.c.sub } else { p.c.text };
            let heading = matches!(line, Line::Total | Line::Net);
            p.text(indent, y, name, size, color, UiTextAnchor::Start, heading);
            for (x, value) in [(c1, values[0]), (c2, values[1])] {
                let color = match line {
                    Line::Net if value >= 0 => p.c.good,
                    Line::Net => p.c.accent,
                    _ => p.c.text,
                };
                p.text(x, y, &number(value, *line == Line::Net), size, color, UiTextAnchor::End, heading);
            }
            if *line == Line::Row {
                p.rule(48.0, w - 24.0, y + 16.0);
            }
            y += 50.0;
        }
        p.text(24.0, BODY.y - 30.0, &footnote, 17.0, p.c.sub, UiTextAnchor::Start, false);
    })
}

// The treasury: its balance, the debt clock and the balance's trend.
fn treasury(context: &mut GameUiContext, page: &PageContext) -> UiCanvas {
    let finance = context.sim.finance();
    let balance = finance.balance();
    let months_in_debt = finance.months_in_debt();
    let limit = GameConfigs::get().sim.finance.debt_limit_months;
    let in_debt = finance.is_in_debt();
    let month = crate::city::history::current_month(context.sim);
    let trend = CityHistory::get().balance_trend(TREND_MONTHS, month, balance);
    let months = CityHistory::get().trend_months(TREND_MONTHS, month);
    let get = |key: &str| ui::text::get(UiTextCategory::City, key).to_string();
    let status = get(if in_debt { "book.treasury.in_debt" } else { "book.treasury.not_in_debt" });
    let clock = ui::text::format(UiTextCategory::City, "book.treasury.clock", &[
        ("months", &months_in_debt.to_string()),
        ("limit", &limit.to_string()),
    ]);
    let title = get("book.treasury");
    let coin = skin::hud_icon(context, "coin").0;
    let width = BODY.x - ACCOUNTS_WIDTH - GAP;
    let value = json!({ "balance": balance, "in_debt": in_debt, "months_in_debt": months_in_debt, "trend": trend });
    body_canvas(page, width, TREASURY_HEIGHT, "treasury", value, move |p: &Paint| {
        p.card(0.0, 0.0, width, TREASURY_HEIGHT, Some(&title), None);
        p.icon(coin, 40.0, 86.0, 34.0);
        p.text(
            68.0,
            100.0,
            &number(balance, false),
            44.0,
            if balance < 0 { p.c.accent } else { p.c.text },
            UiTextAnchor::Start,
            true,
        );
        let status_width = p.width(&status, 19.0, false);
        p.check(width - 44.0 - status_width, 84.0, !in_debt);
        p.text(width - 24.0, 91.0, &status, 19.0, p.c.text, UiTextAnchor::End, false);
        p.text(width - 24.0, 117.0, &clock, 16.0, p.c.sub, UiTextAnchor::End, false);
        p.chart(80.0, 140.0, width - 104.0, 64.0, &trend, true, Some(&months), 2.5);
    })
}

// The tax level and the wage: their controls, and what they bring.
fn taxes_and_wages(context: &mut GameUiContext, page: &PageContext) -> UiFrame {
    let width = BODY.x - ACCOUNTS_WIDTH - GAP;
    let height = BODY.y - TREASURY_HEIGHT - GAP;
    let c = page.colors;
    let finance = context.sim.finance();
    let configs = &GameConfigs::get().sim.finance;
    let level = finance.tax_level();
    let wage = finance.wage();
    let employed = context.world.stats().population.employed;
    let owed = context.world.stats().treasury.tax_available;
    let get = |key: &str| ui::text::get(UiTextCategory::City, key).to_string();

    // The title, "Tax level" and what the level pays.
    let tax_title = get("book.taxes");
    let tax_label = get("book.taxes.level");
    let tax_note =
        ui::text::format(UiTextCategory::City, "book.taxes.pays", &[("percent", &level.percent(configs).to_string())]);
    let top =
        body_canvas(page, width, 104.0, "tax_level_line", json!({ "percent": level.percent(configs) }), move |p: &Paint| {
            p.section(24.0, 36.0, &tax_title);
            p.text(24.0, 84.0, &tax_label, 21.0, p.c.text, UiTextAnchor::Start, false);
            p.text(width - 24.0, 84.0, &tax_note, 17.0, p.c.sub, UiTextAnchor::End, false);
        });

    // The tax level's buttons.
    let steps = TaxLevel::iter().map(|level| UiString::key(UiTextCategory::City, tax_key(level)));
    let look = UiStepLook {
        fill: c.accent,
        track: c.field,
        outline: Some(c.field_outline),
        segment_height: page.pt(46.0),
        selected_text: Some(c.on_accent),
        rounding: page.pt(5.0),
    };
    let levels = skin::step_selector(get("book.taxes.level"), steps)
        .with_style(UiStepSelectorStyle::Buttons)
        .with_look(look)
        .with_text_colors(Some(c.text), Some(c.on_accent))
        .with_font_scale(page.scales.heading_font(19.0))
        .with_width(page.pt(width - 48.0))
        .with_current(level.index())
        .on_changed(|selector, context, _| {
            if let Some(level) = TaxLevel::iter().nth(selector.current()) {
                context_as_mut::<GameUiContext>(context).sim.finance_mut().set_tax_level(level);
            }
        });

    // What the houses owe, and "Wage" with its note.
    let owed_label = get("book.taxes.owed");
    let owed_text = ui::text::format(UiTextCategory::City, "book.gold", &[("gold", &number(i64::from(owed), false))]);
    let wage_label = get("book.wage");
    let wage_note = get("book.wage.note");
    let middle = body_canvas(page, width, 94.0, "taxes_owed", json!(owed), move |p: &Paint| {
        p.text(24.0, 38.0, &owed_label, 19.0, p.c.sub, UiTextAnchor::Start, false);
        p.text(width - 24.0, 38.0, &owed_text, 19.0, p.c.text, UiTextAnchor::End, false);
        p.rule(24.0, width - 24.0, 62.0);
    });
    let wage_line = body_canvas(page, width - 200.0, 46.0, "wage_line", json!(null), move |p: &Paint| {
        p.text(24.0, 30.0, &wage_label, 21.0, p.c.text, UiTextAnchor::Start, false);
        let x = 24.0 + p.width(&wage_label, 21.0, false) + 14.0;
        p.text(x, 30.0, &wage_note, 17.0, p.c.sub, UiTextAnchor::Start, false);
    });
    let top_wage = MAX_WAGE.max(wage);
    let wages = skin::step_selector(get("book.wage"), (0..=top_wage).map(|wage| UiString::literal(wage.to_string())))
        .with_style(UiStepSelectorStyle::Arrows)
        .with_label_hidden(true)
        .with_text_colors(Some(c.text), Some(c.text))
        .with_font_scale(page.scales.heading_font(23.0))
        .with_width(page.pt(150.0))
        .with_current(wage as usize)
        .on_changed(|selector, context, _| {
            context_as_mut::<GameUiContext>(context).sim.finance_mut().set_wage(selector.current() as u32);
        });
    let mut wage_row = UiColumns::new().with_spacing(0.0).with_width(page.pt(width));
    wage_row.add_column(UiColumnWidth::Fraction(1.0), super::left_group().with_widget(wage_line));
    wage_row.add_column(UiColumnWidth::Fixed(page.pt(174.0)), super::left_group().with_widget(wages));

    // What the wage costs.
    let monthly = (u64::from(employed) * u64::from(wage)).div_ceil(12);
    let rows = [
        (get("book.wage.workers"), number(i64::from(employed), false)),
        (get("book.wage.month"), number(monthly as i64, false)),
        (get("book.wage.year"), number(i64::from(employed) * i64::from(wage), false)),
    ];
    let value =
        json!({ "workers": employed, "wage": wage, "a_month": monthly, "a_year": u64::from(employed) * u64::from(wage) });
    let bottom = body_canvas(page, width, 130.0, "wages", value, move |p: &Paint| {
        for (k, (label, value)) in rows.iter().enumerate() {
            let y = 36.0 + k as f32 * 38.0;
            p.text(24.0, y, label, 19.0, p.c.sub, UiTextAnchor::Start, false);
            p.text(width - 24.0, y, value, 19.0, p.c.text, UiTextAnchor::End, false);
        }
    });

    let contents = UiWidgetGroup::new()
        .with_widget_spacing(Vec2::zero())
        .with_widget(top)
        .with_widget(levels) // Centred: 24 points in from each side.
        .with_widget(middle)
        .with_widget(wage_row)
        .with_widget(bottom);
    UiFrame::new(contents, Vec2::new(page.pt(width), page.pt(height)))
        .with_fill(c.card, page.pt(6.0))
        .with_outline(UiFrameOutline { color: c.card_outline, width: page.pt(1.5), inset: 0.0, dash: None })
}

fn tax_key(level: TaxLevel) -> &'static str {
    match level {
        TaxLevel::None => "book.taxes.none",
        TaxLevel::Low => "book.taxes.low",
        TaxLevel::Normal => "book.taxes.normal",
        TaxLevel::High => "book.taxes.high",
        TaxLevel::Harsh => "book.taxes.harsh",
    }
}
