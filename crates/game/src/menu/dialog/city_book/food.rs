// The Food page (docs/plans/messages.md, Phase 3; city-food-<realm>.jpg): four cards (the food in
// the granaries, what was grown and eaten last month, how long it lasts without harvests), a row
// per food (stored, grown, eaten, their difference, and how long it lasts), and the food in the
// granaries over the last twelve months. Its figures are crate::city::food's.

use common::Vec2;
use engine::ui::{self, UiTextAnchor, text::UiTextCategory, widgets::*};
use serde_json::json;

use super::{BODY, CityPage, PageContext, Paint, body_canvas, number};
use crate::{menu::skin, ui_context::GameUiContext};

pub(super) struct Food;

const CARD_HEIGHT: f32 = 118.0;
const GAP: f32 = 20.0;
const TABLE_TOP: f32 = 138.0;
const HEAD: f32 = 74.0; // The column titles' baseline in the By food card.
const ROW: f32 = 60.0;
const LASTS_SCALE: f32 = 12.0; // A bar full at a year's food.

impl CityPage for Food {
    fn id(&self) -> &'static str {
        "food"
    }

    fn icon(&self) -> &'static str {
        "food"
    }

    fn note(&self, _context: &GameUiContext) -> String {
        get("book.note.food")
    }

    fn body(&self, context: &mut GameUiContext, page: &PageContext) -> UiWidgetGroup {
        let food = page.report.food.clone();
        let (grown_key, eaten_key) = match food.period {
            "last_month" => ("book.food.grown", "book.food.eaten"),
            "last_month_part" => ("book.food.grown_last", "book.food.eaten_last"),
            _ => ("book.food.grown_this", "book.food.eaten_this"),
        };

        // The four cards, each with its icon.
        let icons = ["food", "farm", "population", "food"].map(|icon| skin::city_icon(context, icon));
        let cards = [
            (get("book.food.in_granaries"), number(i64::from(food.stored), false)),
            (get(grown_key), number(i64::from(food.grown), false)),
            (get(eaten_key), number(i64::from(food.eaten), false)),
            (get("book.food.lasts"), lasts_text(food.lasts_months)),
        ];
        let value = json!({
            "in_granaries": food.stored,
            "grown": food.grown,
            "eaten": food.eaten,
            "lasts_months": food.lasts_months.map(|months| (months * 10.0).round() / 10.0),
            "period": food.period,
        });
        let cards = body_canvas(page, BODY.x, CARD_HEIGHT, "cards", value, move |p: &Paint| {
            let width = (BODY.x - 3.0 * GAP) / 4.0;
            for (k, ((label, value), icon)) in cards.iter().zip(icons).enumerate() {
                let x = k as f32 * (width + GAP);
                p.card(x, 0.0, width, CARD_HEIGHT, None, None);
                p.painter.image(icon, p.rect(x + 22.0, 24.0, 24.0, 24.0), p.c.accent);
                p.text(x + 56.0, 43.0, label, 19.0, p.c.sub, UiTextAnchor::Start, false);
                p.text(x + 24.0, 94.0, value, 36.0, p.c.text, UiTextAnchor::Start, true);
            }
        });

        // By food: a row per food.
        let rows = food.rows.clone();
        let table_height = HEAD + 18.0 + rows.len().max(1) as f32 * ROW;
        let value = json!(rows);
        let title = get("book.food.by_food");
        let columns =
            ["food", "stored", "grown", "eaten", "change", "lasts"].map(|key| get(&format!("book.food.column.{key}")));
        let none = get("book.food.none");
        let month_texts: Vec<String> = rows.iter().map(|row| lasts_text(row.lasts_months)).collect();
        let table = body_canvas(page, BODY.x, table_height, "foods", value, move |p: &Paint| {
            p.card(0.0, 0.0, BODY.x, table_height, Some(&title), None);
            let titles = [
                (48.0, UiTextAnchor::Start),
                (330.0, UiTextAnchor::End),
                (480.0, UiTextAnchor::End),
                (620.0, UiTextAnchor::End),
                (760.0, UiTextAnchor::End),
                (800.0, UiTextAnchor::Start),
            ];
            for ((x, anchor), text) in titles.iter().zip(&columns) {
                p.text(*x, HEAD, text, 17.0, p.c.sub, *anchor, false);
            }
            if rows.is_empty() {
                p.rule(24.0, BODY.x - 24.0, HEAD + 18.0);
                p.text(48.0, HEAD + 52.0, &none, 19.0, p.c.sub, UiTextAnchor::Start, false);
                return;
            }
            for (k, (row, months)) in rows.iter().zip(&month_texts).enumerate() {
                let y = HEAD + 52.0 + k as f32 * ROW;
                p.rule(24.0, BODY.x - 24.0, y - 34.0);
                p.painter.rect_filled(p.rect(27.0, y - 12.0, 10.0, 10.0), p.c.accent, 1.0 * p.k);
                p.text(48.0, y, &row.name, 21.0, p.c.text, UiTextAnchor::Start, false);
                p.text(330.0, y, &number(i64::from(row.stored), false), 20.0, p.c.text, UiTextAnchor::End, false);
                p.text(480.0, y, &number(i64::from(row.grown), false), 20.0, p.c.text, UiTextAnchor::End, false);
                p.text(620.0, y, &number(i64::from(row.eaten), false), 20.0, p.c.text, UiTextAnchor::End, false);
                p.change(760.0, y, Some(row.change), 19.0);
                let lasts = row.lasts_months.unwrap_or(0.0);
                p.bar(800.0, y - 13.0, BODY.x - 1000.0, 12.0, (lasts / LASTS_SCALE).min(1.0));
                let color = if row.short { p.c.accent } else { p.c.text };
                p.text(BODY.x - 24.0, y, months, 19.0, color, UiTextAnchor::End, false);
            }
        });

        // The food in the granaries over the last twelve months.
        let chart_top = TABLE_TOP + table_height + GAP;
        let chart_height = (BODY.y - chart_top).max(160.0);
        let value = json!({ "values": food.trend, "months": food.trend_months });
        let chart_title = get("book.food.chart");
        let chart_note = get("book.food.chart_note");
        let (trend, months) = (food.trend.clone(), food.trend_months.clone());
        let chart = body_canvas(page, BODY.x, chart_height, "chart", value, move |p: &Paint| {
            p.card(0.0, 0.0, BODY.x, chart_height, Some(&chart_title), Some(&chart_note));
            p.chart(84.0, 66.0, BODY.x - 120.0, chart_height - 130.0, &trend, true, Some(&months), 3.0);
        });

        super::left_group()
            .with_widget_spacing(Vec2::zero())
            .with_widget(cards)
            .with_widget(super::spacer(page.pt(TABLE_TOP - CARD_HEIGHT)))
            .with_widget(table)
            .with_widget(super::spacer(page.pt(GAP)))
            .with_widget(chart)
    }
}

fn get(key: &str) -> String {
    ui::text::get(UiTextCategory::City, key).to_string()
}

// "5 months", "Under a month", or "—" while none is eaten.
fn lasts_text(months: Option<f32>) -> String {
    match months {
        None => "—".to_string(),
        Some(months) if months < 1.0 => get("book.food.under_a_month"),
        Some(months) => ui::text::format_count(UiTextCategory::City, "book.food.months", months.round() as i64, &[]),
    }
}
