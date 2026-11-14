// The Population page (city-population-<realm>.jpg): the population and its trend over the last
// twelve months, the workforce, and the houses by level with what the next level needs.

use common::Vec2;
use engine::ui::{self, UiTextAnchor, text::UiTextCategory, widgets::*};
use serde_json::json;

use super::{BODY, CityPage, PageContext, Paint, body_canvas, number};
use crate::{city::CityHistory, ui_context::GameUiContext};

pub(super) struct Population;

const TOP_HEIGHT: f32 = 340.0;
const LEFT_WIDTH: f32 = 690.0;
const GAP: f32 = 20.0;
const HOUSING_TOP: f32 = 362.0;
const ROW: f32 = 46.0;
const TREND_MONTHS: usize = 12;
const NEEDS_X: f32 = 790.0; // The next level's needs, from the card's left.

impl CityPage for Population {
    fn id(&self) -> &'static str {
        "population"
    }

    fn icon(&self) -> &'static str {
        "population"
    }

    fn note(&self, _context: &GameUiContext) -> String {
        ui::text::get(UiTextCategory::City, "book.note.population").to_string()
    }

    fn body(&self, context: &mut GameUiContext, page: &PageContext) -> UiWidgetGroup {
        let report = page.report;
        let get = |key: &str| ui::text::get(UiTextCategory::City, key).to_string();
        let month = crate::city::history::current_month(context.sim);
        let trend = CityHistory::get().population_trend(TREND_MONTHS, month, report.population);
        let months = CityHistory::get().trend_months(TREND_MONTHS, month);

        // The population and its trend; the workforce.
        let workforce = report.employed + report.unemployed;
        let employed_share = if workforce > 0 { report.employed as f32 / workforce as f32 } else { 0.0 };
        let rows = [
            (get("book.workforce.employed"), report.employed, Some(true)),
            (get("book.workforce.looking"), report.unemployed, Some(false)),
            (get("book.workforce.jobs_open"), report.jobs_not_filled, None),
            (get("book.workforce.not_working_age"), report.not_working_age, None),
        ];
        let value = json!({
            "population": report.population,
            "change": report.population_change,
            "trend": trend,
            "employed": report.employed,
            "looking_for_work": report.unemployed,
            "jobs_not_filled": report.jobs_not_filled,
            "not_working_age": report.not_working_age,
            "employment": (employed_share * 100.0).round(),
        });
        let population_title = get("book.card.population");
        let this_month = get("book.this_month_plain");
        let first_month = get("book.first_month");
        let workforce_title = get("book.workforce");
        let employed_note = get("book.workforce.note");
        let population = report.population;
        let change = report.population_change;
        let top = body_canvas(page, BODY.x, TOP_HEIGHT, "population", value, move |p: &Paint| {
            p.card(0.0, 0.0, LEFT_WIDTH, TOP_HEIGHT, Some(&population_title), None);
            let big = number(i64::from(population), false);
            p.text(24.0, 98.0, &big, 46.0, p.c.text, UiTextAnchor::Start, true);
            let x = 24.0 + p.width(&big, 46.0, true) + 36.0;
            match change {
                Some(change) => {
                    let text = number(change, true);
                    let width = p.width(&text, 20.0, false);
                    p.change(x + width + 18.0, 96.0, Some(change), 20.0);
                    p.text(x + width + 28.0, 96.0, &this_month, 18.0, p.c.sub, UiTextAnchor::Start, false);
                }
                None => {
                    p.text(x, 96.0, &first_month, 18.0, p.c.sub, UiTextAnchor::Start, false);
                }
            }
            p.chart(84.0, 130.0, LEFT_WIDTH - 112.0, 158.0, &trend, true, Some(&months), 3.0);

            let wx = LEFT_WIDTH + GAP;
            let ww = BODY.x - wx;
            p.card(wx, 0.0, ww, TOP_HEIGHT, Some(&workforce_title), None);
            let share = if workforce > 0 { format!("{}%", (employed_share * 100.0).round()) } else { "—".to_string() };
            p.text(wx + 24.0, 98.0, &share, 46.0, p.c.text, UiTextAnchor::Start, true);
            let note_x = wx + 24.0 + p.width(&share, 46.0, true) + 20.0;
            p.text(note_x, 96.0, &employed_note, 18.0, p.c.sub, UiTextAnchor::Start, false);
            let (bx, bw, by) = (wx + 24.0, ww - 48.0, 122.0);
            p.painter.rect_filled(p.rect(bx, by, bw, 20.0), p.c.track, 4.0 * p.k);
            if employed_share > 0.0 {
                p.painter.rect_filled(p.rect(bx, by, bw * employed_share, 20.0), p.c.accent, 4.0 * p.k);
            }
            for (k, (label, count, swatch)) in rows.iter().enumerate() {
                let y = 186.0 + k as f32 * 38.0;
                match swatch {
                    Some(true) => p.painter.rect_filled(p.rect(bx, y - 14.0, 16.0, 16.0), p.c.accent, 3.0 * p.k),
                    Some(false) => p.painter.rect_filled(p.rect(bx, y - 14.0, 16.0, 16.0), p.c.track, 3.0 * p.k),
                    None => p.painter.rect_stroke(p.rect(bx + 1.0, y - 13.0, 14.0, 14.0), 3.0 * p.k, 1.5 * p.k, p.c.sub),
                }
                p.text(bx + 28.0, y, label, 19.0, p.c.text, UiTextAnchor::Start, false);
                p.text(wx + ww - 24.0, y, &number(i64::from(*count), false), 19.0, p.c.text, UiTextAnchor::End, false);
                if k + 1 < rows.len() {
                    p.rule(bx, wx + ww - 24.0, y + 13.0);
                }
            }
        });

        // The houses by level.
        let names: Vec<String> = report
            .housing
            .iter()
            .map(|row| ui::text::get(UiTextCategory::Realm, &format!("house_level.{}.{}", page.realm, row.level)).to_string())
            .collect();
        let total_residents: u32 = report.housing.iter().map(|row| row.residents).sum();
        let housing: Vec<(u32, String, u32, u32, f32, String, bool)> = report
            .housing
            .iter()
            .zip(&names)
            .map(|(row, name)| {
                let share = if total_residents > 0 { row.residents as f32 / total_residents as f32 } else { 0.0 };
                let top = row.next_needs.is_none();
                let needs = row.next_needs.clone().unwrap_or_else(|| get("book.housing.top_level"));
                (row.level, name.clone(), row.houses, row.residents, share, needs, top)
            })
            .collect();
        let value = json!(housing
            .iter()
            .map(|(level, name, houses, residents, _, needs, _)| json!({ "level": level, "house": name, "houses": houses, "residents": residents, "next_level_needs": needs }))
            .collect::<Vec<_>>());
        let titles = ["level", "house", "houses", "residents", "share", "needs"].map(|key| get(&format!("book.housing.{key}")));
        let housing_title = get("book.housing");
        let height = BODY.y - HOUSING_TOP;
        let housing_canvas = body_canvas(page, BODY.x, height, "housing", value, move |p: &Paint| {
            p.card(0.0, 0.0, BODY.x, height, Some(&housing_title), None);
            let header = 74.0;
            let columns = [(24.0, UiTextAnchor::Start), (96.0, UiTextAnchor::Start), (400.0, UiTextAnchor::End), (540.0, UiTextAnchor::End), (580.0, UiTextAnchor::Start), (NEEDS_X, UiTextAnchor::Start)];
            for ((x, anchor), title) in columns.iter().zip(&titles) {
                p.text(*x, header, title, 17.0, p.c.sub, *anchor, false);
            }
            for (k, (level, name, houses, residents, share, needs, top)) in housing.iter().enumerate() {
                let y = header + ROW + k as f32 * ROW;
                p.rule(24.0, BODY.x - 24.0, y - 32.0);
                p.painter.circle_filled(p.at(40.0, y - 7.0), 15.0 * p.k, p.c.field);
                p.painter.circle_stroke(p.at(40.0, y - 7.0), 15.0 * p.k, 1.5 * p.k, p.c.field_outline);
                p.text(40.0, y - 1.0, &level.to_string(), 17.0, p.c.accent, UiTextAnchor::Middle, false);
                p.text(96.0, y, name, 20.0, p.c.text, UiTextAnchor::Start, false);
                p.text(400.0, y, &number(i64::from(*houses), false), 20.0, p.c.text, UiTextAnchor::End, false);
                p.text(540.0, y, &number(i64::from(*residents), false), 20.0, p.c.text, UiTextAnchor::End, false);
                p.bar(580.0, y - 12.0, 140.0, 10.0, (share / 0.4).min(1.0));
                p.text(730.0, y, &format!("{}%", (share * 100.0).round()), 16.0, p.c.sub, UiTextAnchor::Start, false);
                let color = if *top { p.c.sub } else { p.c.text };
                // The needs on a line, or set smaller on two that fit the row, the second cut at the
                // card's edge if it must be.
                let room = BODY.x - 24.0 - NEEDS_X;
                if p.width(needs, 19.0, false) <= room {
                    p.text(NEEDS_X, y, needs, 19.0, color, UiTextAnchor::Start, false);
                } else {
                    let look = p.look(15.0, color, false);
                    let lines = p.painter.wrap_label(needs, &look, room * p.k);
                    let clip = p.rect(NEEDS_X, y - 34.0, room, 46.0);
                    p.painter.clipped(clip, |painter| {
                        for (j, line) in lines.iter().take(2).enumerate() {
                            painter.label(p.at(NEEDS_X, y - 9.0 + j as f32 * 17.0), UiTextAnchor::Start, line, &look);
                        }
                    });
                }
            }
        });

        super::left_group()
            .with_widget_spacing(Vec2::zero())
            .with_widget(top)
            .with_widget(super::spacer(page.pt(HOUSING_TOP - TOP_HEIGHT)))
            .with_widget(housing_canvas)
    }
}
