// The Ratings page (city-ratings-<realm>.jpg, docs/plans/ratings.md, Phase 1): a card per rating,
// two by two: its name, a ring with the mission's goal ticked on it and what's left to reach it,
// and its breakdown, the drivers that raise and lower it with their values, and the rating, their
// sum. What it shows is the city's ratings at the month's start (crate::ratings), in the UI realm's
// words.
//
// Paths: CityManagement/Ratings/<rating> (kind canvas, value `name`, `value`, `goal`, `met`,
// `drivers`: name and value each), the rating spelled as in data (`legitimacy`).

use common::Vec2;
use engine::ui::{self, UiTextAnchor, text::UiTextCategory, widgets::*};
use serde_json::json;

use super::{BODY, CityPage, PageContext, Paint, body_canvas, number};
use crate::{
    ratings::{self, RatingKind, RatingView},
    ui_context::GameUiContext,
};

pub(super) struct Ratings;

const GAP: f32 = 20.0;
const CARD: Vec2 = Vec2::new((BODY.x - GAP) / 2.0, (BODY.y - GAP) / 2.0);
const RING: (f32, f32, f32) = (112.0, 168.0, 66.0); // Centre x, y and radius, in the card.
const DRIVERS_X: f32 = 232.0; // The breakdown's left edge, in the card.
const FIRST_ROW: f32 = 92.0; // The first driver's baseline.
const ROW: f32 = 44.0; // The mockup's row, the most a row takes.
const TOTAL_GAP: f32 = 6.0; // From the last driver's row to the rating's.

impl CityPage for Ratings {
    fn id(&self) -> &'static str {
        "ratings"
    }

    fn icon(&self) -> &'static str {
        "star"
    }

    fn note(&self, _context: &GameUiContext) -> String {
        ui::text::get(UiTextCategory::City, "book.note.ratings").to_string()
    }

    fn body(&self, context: &mut GameUiContext, page: &PageContext) -> UiWidgetGroup {
        let views = ratings::views(context.sim.ratings(), page.realm);
        let names = crate::menu::in_game::rating_names(page.realm);
        let mut cards = Vec::new();
        for (index, kind) in [RatingKind::Prosperity, RatingKind::Culture, RatingKind::Security, RatingKind::Legitimacy]
            .into_iter()
            .enumerate()
        {
            let view = views.as_ref().and_then(|views| views.iter().find(|view| view.kind == kind)).cloned();
            cards.push(card(page, kind, &names[index], view));
        }

        let row = |left: UiCanvas, right: UiCanvas| {
            UiWidgetGroup::new()
                .with_stack_vertically(false)
                .with_center_horizontally(false)
                .with_widget_spacing(Vec2::new(page.pt(GAP), 0.0))
                .with_widget(left)
                .with_widget(right)
        };
        let mut cards = cards.into_iter();
        let (first, second, third, fourth) =
            (cards.next().unwrap(), cards.next().unwrap(), cards.next().unwrap(), cards.next().unwrap());
        super::left_group()
            .with_widget_spacing(Vec2::zero())
            .with_widget(row(first, second))
            .with_widget(super::spacer(page.pt(GAP)))
            .with_widget(row(third, fourth))
    }
}

// A rating's card; without a view while the city isn't rated, its ring empty and no breakdown.
fn card(page: &PageContext, kind: RatingKind, name: &str, view: Option<RatingView>) -> UiCanvas {
    let get = |key: &str| ui::text::get(UiTextCategory::City, key).to_string();
    let value = json!({
        "name": name,
        "value": view.as_ref().map(|view| view.value),
        "goal": view.as_ref().and_then(|view| view.goal),
        "met": view.as_ref().is_some_and(RatingView::is_goal_met),
        "drivers": view.as_ref().map_or_else(Vec::new, |view| {
            view.drivers.iter().map(|(name, value)| json!({ "name": name, "value": value })).collect()
        }),
    });
    let goal_text = match view.as_ref() {
        None => get("book.rating.not_rated"),
        Some(view) => match view.goal {
            Some(goal) if view.value >= goal => {
                ui::text::format(UiTextCategory::City, "book.rating.goal_met", &[("goal", &goal.to_string())])
            }
            Some(goal) => ui::text::format(UiTextCategory::City, "book.rating.goal_to_go", &[
                ("goal", &goal.to_string()),
                ("left", &(goal - view.value).to_string()),
            ]),
            None => get("book.rating.no_goal"),
        },
    };
    let total = get("book.rating.total");
    let name = name.to_string();
    body_canvas(page, CARD.x, CARD.y, &kind.to_string(), value, move |p: &Paint| {
        p.card(0.0, 0.0, CARD.x, CARD.y, Some(&name), None);
        let (cx, cy, r) = RING;
        p.ring(cx, cy, r, view.as_ref().map(|view| view.value), view.as_ref().and_then(|view| view.goal), 12.0, 40.0);
        p.text(cx, cy + 106.0, &goal_text, 17.0, p.c.sub, UiTextAnchor::Middle, false);

        let Some(view) = &view else {
            return;
        };
        // The breakdown, its rows closer together where a rating has more drivers than fit.
        let (x0, x1) = (DRIVERS_X, CARD.x - 24.0);
        let rows = view.drivers.len().max(1) as f32;
        let step = ROW.min((CARD.y - FIRST_ROW - TOTAL_GAP - 26.0) / rows);
        for (k, (label, value)) in view.drivers.iter().enumerate() {
            let y = FIRST_ROW + k as f32 * step;
            p.text(x0, y, label, 19.0, p.c.text, UiTextAnchor::Start, false);
            let color = if *value > 0 {
                p.c.good
            } else if *value < 0 {
                p.c.accent
            } else {
                p.c.sub
            };
            p.text(x1, y, &number(i64::from(*value), true), 19.0, color, UiTextAnchor::End, false);
            p.rule(x0, x1, y + 15.0);
        }
        let y = FIRST_ROW + view.drivers.len() as f32 * step + TOTAL_GAP;
        p.text(x0, y, &total, 19.0, p.c.text, UiTextAnchor::Start, true);
        p.text(x1, y, &view.value.to_string(), 19.0, p.c.text, UiTextAnchor::End, true);
    })
}
