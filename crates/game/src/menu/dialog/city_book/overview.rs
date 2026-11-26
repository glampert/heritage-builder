// The Overview page (city-overview-<realm>.jpg): four cards (population, treasury, food,
// employment), the four ratings (each a ring, a tick at the mission's goal for it) and the mission's
// goals, and what needs attention.

use common::Vec2;
use engine::ui::{self, UiTextAnchor, text::UiTextCategory, widgets::*};
use serde_json::json;

use super::{BODY, CityPage, PageContext, Paint, body_canvas, number};
use crate::{
    campaign::{self, config::CampaignConfigs},
    city::CityHistory,
    menu::skin,
    ratings,
    ui_context::GameUiContext,
};

pub(super) struct Overview;

const CARD_HEIGHT: f32 = 184.0;
const GAP: f32 = 20.0;
const RATINGS_WIDTH: f32 = 620.0;
const MIDDLE_HEIGHT: f32 = 252.0;
const WARNING_ROW: f32 = 46.0;
const TREND_MONTHS: usize = 12;

impl CityPage for Overview {
    fn id(&self) -> &'static str {
        "overview"
    }

    fn icon(&self) -> &'static str {
        "learning"
    }

    fn note(&self, _context: &GameUiContext) -> String {
        ui::text::get(UiTextCategory::City, "book.note.overview").to_string()
    }

    fn body(&self, context: &mut GameUiContext, page: &PageContext) -> UiWidgetGroup {
        let report = page.report;
        let get = |key: &str| ui::text::get(UiTextCategory::City, key).to_string();
        let history = CityHistory::get();
        let month = crate::city::history::current_month(context.sim);

        // The four cards.
        let population_trend = history.population_trend(TREND_MONTHS, month, report.population);
        let balance_trend = history.balance_trend(TREND_MONTHS, month, report.balance);
        let population_sub = match report.population_change {
            Some(change) => ui::text::format(UiTextCategory::City, "book.this_month", &[("change", &number(change, true))]),
            None => get("book.first_month"),
        };
        let treasury_sub = ui::text::format(UiTextCategory::City, "book.so_far_this_year", &[(
            "change",
            &number(report.net_this_year, true),
        )]);
        let (food_value, food_ratio) = match report.food_months {
            Some(months) => {
                (ui::text::format_count(UiTextCategory::City, "book.months", months.floor() as i64, &[]), months / 12.0)
            }
            None => ("—".to_string(), 0.0),
        };
        let food_sub = ui::text::format_count(
            UiTextCategory::City,
            "book.food_in_granaries",
            i64::from(report.granaries.buildings),
            &[("food", &number(i64::from(report.granary_food), false))],
        );
        let employment = report.employment_ratio();
        let employment_value = if report.employed + report.unemployed > 0 {
            format!("{}%", (employment * 100.0).round())
        } else {
            "—".to_string()
        };
        let employment_sub =
            ui::text::format_count(UiTextCategory::City, "book.looking_for_work", i64::from(report.unemployed), &[]);

        let icons = [
            skin::hud_icon(context, "population").0,
            skin::hud_icon(context, "coin").0,
            skin::hud_icon(context, "food").0,
            skin::hud_icon(context, "population").0,
        ];
        let cards = [
            (get("book.card.population"), number(i64::from(report.population), false), population_sub),
            (get("book.card.treasury"), number(report.balance, false), treasury_sub),
            (get("book.card.food"), food_value, food_sub),
            (get("book.card.employment"), employment_value, employment_sub),
        ];
        let value = json!({
            "population": report.population,
            "population_change": report.population_change,
            "treasury": report.balance,
            "net_this_year": report.net_this_year,
            "food_months": report.food_months.map(f32::floor),
            "food_stored": report.food_stored,
            "employment": (employment * 100.0).round(),
            "unemployed": report.unemployed,
        });
        let cards_canvas = body_canvas(page, BODY.x, CARD_HEIGHT, "cards", value, move |p: &Paint| {
            let width = (BODY.x - 3.0 * GAP) / 4.0;
            for (k, (label, value, sub)) in cards.iter().enumerate() {
                let x = k as f32 * (width + GAP);
                p.card(x, 0.0, width, CARD_HEIGHT, None, None);
                p.icon(icons[k], x + 34.0, 40.0, 30.0);
                p.text(x + 62.0, 47.0, label, 19.0, p.c.sub, UiTextAnchor::Start, false);
                p.text(x + 24.0, 106.0, value, 40.0, p.c.text, UiTextAnchor::Start, true);
                p.text(x + 24.0, 136.0, sub, 17.0, p.c.sub, UiTextAnchor::Start, false);
                match k {
                    0 => p.chart(x + 24.0, 146.0, width - 48.0, 24.0, &population_trend, false, None, 2.5),
                    1 => p.chart(x + 24.0, 146.0, width - 48.0, 24.0, &balance_trend, false, None, 2.5),
                    2 => p.bar(x + 24.0, 154.0, width - 48.0, 10.0, food_ratio),
                    _ => p.bar(x + 24.0, 154.0, width - 48.0, 10.0, employment),
                }
            }
        });

        // The ratings and the goals.
        let names = crate::menu::in_game::rating_names(page.realm);
        let views = ratings::views(context.sim.ratings(), page.realm);
        let rings: Vec<(Option<u32>, Option<u32>)> = match &views {
            Some(views) => views.iter().map(|view| (Some(view.value), view.goal)).collect(),
            None => vec![(None, None); names.len()],
        };
        let goals = goals(context, page.realm);
        let goals_note = goals.0.clone();
        let goals_rows = goals.1.clone();
        let middle_value = json!({
            "ratings": {
                "names": names,
                "rated": views.is_some(),
                "values": rings.iter().map(|(value, _)| *value).collect::<Vec<_>>(),
                "goals": rings.iter().map(|(_, goal)| *goal).collect::<Vec<_>>(),
            },
            "goals": goals_rows.iter().map(|(goal, target, achieved, met)| json!({ "goal": goal, "target": target, "achieved": achieved, "met": met })).collect::<Vec<_>>(),
        });
        let ratings_title = get("book.ratings");
        let ratings_note = get("book.ratings.note");
        let no_goal = get("book.ratings.no_goal");
        let goals_title = get("book.goals");
        let no_goals = get("book.goals.none");
        let middle = body_canvas(page, BODY.x, MIDDLE_HEIGHT, "standing", middle_value, move |p: &Paint| {
            p.card(0.0, 0.0, RATINGS_WIDTH, MIDDLE_HEIGHT, Some(&ratings_title), Some(&ratings_note));
            for (k, (name, (value, goal))) in names.iter().zip(&rings).enumerate() {
                let cx = 88.0 + k as f32 * 148.0;
                p.ring(cx, 122.0, 48.0, *value, *goal, 9.0, 30.0);
                p.text(cx, 202.0, name, 18.0, p.c.text, UiTextAnchor::Middle, false);
                let goal_text = match goal {
                    Some(goal) => {
                        ui::text::format(UiTextCategory::City, "book.ratings.goal", &[("goal", &goal.to_string())])
                    }
                    None => no_goal.clone(),
                };
                p.text(cx, 228.0, &goal_text, 16.0, p.c.sub, UiTextAnchor::Middle, false);
            }
            let gx = RATINGS_WIDTH + GAP;
            let gw = BODY.x - gx;
            p.card(gx, 0.0, gw, MIDDLE_HEIGHT, Some(&goals_title), goals_note.as_deref());
            if goals_rows.is_empty() {
                p.text(gx + 24.0, 96.0, &no_goals, 19.0, p.c.sub, UiTextAnchor::Start, false);
            }
            for (k, (goal, target, achieved, met)) in goals_rows.iter().take(3).enumerate() {
                let y = 88.0 + k as f32 * 56.0;
                p.check(gx + 34.0, y - 7.0, *met);
                p.text(gx + 58.0, y, goal, 20.0, p.c.text, UiTextAnchor::Start, false);
                let progress = format!("{} / {}", number(*achieved, false), number(*target, false));
                p.text(gx + gw - 24.0, y, &progress, 18.0, p.c.sub, UiTextAnchor::End, false);
                let ratio = if *target > 0 { (*achieved as f32 / *target as f32).clamp(0.0, 1.0) } else { 1.0 };
                p.bar(gx + 58.0, y + 14.0, gw - 82.0, 8.0, ratio);
            }
        });

        // What needs attention.
        let warnings: Vec<(engine::render::texture::TextureHandle, String)> =
            report.warnings.iter().map(|warning| (skin::hud_icon(context, warning.icon).0, warning.text.clone())).collect();
        let warnings_value = json!(report.warnings.iter().map(|warning| &warning.text).collect::<Vec<_>>());
        let attention_title = get("book.attention");
        let nothing = get("book.attention.none");
        let more_key = "book.attention.more";
        let warnings_top = CARD_HEIGHT + MIDDLE_HEIGHT + 2.0 * GAP;
        let height = BODY.y - warnings_top;
        let rows = (((height - 80.0) / WARNING_ROW).floor() as usize + 1).max(1);
        let more = warnings.len().saturating_sub(rows);
        let more_text = (more > 0).then(|| ui::text::format_count(UiTextCategory::City, more_key, (more + 1) as i64, &[]));
        let attention = body_canvas(page, BODY.x, height, "attention", warnings_value, move |p: &Paint| {
            p.card(0.0, 0.0, BODY.x, height, Some(&attention_title), None);
            if warnings.is_empty() {
                p.text(24.0, 80.0, &nothing, 20.0, p.c.sub, UiTextAnchor::Start, false);
                return;
            }
            let shown = if more > 0 { rows - 1 } else { rows };
            for (k, (icon, text)) in warnings.iter().take(shown).enumerate() {
                let y = 80.0 + k as f32 * WARNING_ROW;
                p.icon(*icon, 40.0, y - 7.0, 26.0);
                p.text(70.0, y, text, 20.0, p.c.text, UiTextAnchor::Start, false);
                if k + 1 < shown || more_text.is_some() {
                    p.rule(24.0, BODY.x - 24.0, y + 19.0);
                }
            }
            if let Some(more_text) = &more_text {
                let y = 80.0 + shown as f32 * WARNING_ROW;
                p.text(70.0, y, more_text, 19.0, p.c.sub, UiTextAnchor::Start, false);
            }
        });

        let gap = || super::spacer(page.pt(GAP));
        super::left_group()
            .with_widget_spacing(Vec2::zero())
            .with_widget(cards_canvas)
            .with_widget(gap())
            .with_widget(middle)
            .with_widget(gap())
            .with_widget(attention)
    }
}

// A goal's row: its name, target, what the city has and whether it's met.
type GoalRow = (String, i64, i64, bool);

// The game's goals: a mission's ("Mission 2" and a row per goal), a custom game's goal set's (its
// name and a row per goal), or none in a game without goals.
fn goals(context: &GameUiContext, realm: crate::realm::Realm) -> (Option<String>, Vec<GoalRow>) {
    let (note, requirements) = match campaign::active_mission() {
        Some(active) => {
            let configs = CampaignConfigs::get();
            let (Some(campaign), Some(mission)) =
                (configs.campaign(active.campaign_id), configs.mission(active.campaign_id, active.mission_index))
            else {
                return (None, Vec::new());
            };
            let number = campaign.mission_number(active.mission_index).to_string();
            (ui::text::format(UiTextCategory::City, "book.goals.mission", &[("number", &number)]), &mission.requirements)
        }
        None => {
            let Some(set) = crate::custom_game::active().and_then(|game| game.goal_set()) else {
                return (None, Vec::new());
            };
            (set.display_name().to_string(), &set.requirements)
        }
    };
    let stats = context.world.stats();
    let rows = requirements
        .goals
        .iter()
        .map(|goal| {
            let (text, target, achieved) = super::super::goal_line(goal, stats, realm);
            (text, target, achieved, goal.is_met(stats))
        })
        .collect();
    (Some(note), rows)
}
