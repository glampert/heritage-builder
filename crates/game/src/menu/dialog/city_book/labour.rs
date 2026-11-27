// The Labour page (docs/plans/messages.md, Phase 3; city-labour-<realm>.jpg): the workforce's four
// cards, and the sectors in priority order, each with its − and + (a step selector of its
// priority), the jobs needed and filled, a bar and "Full" or "N short" (crate::labour's report).
// A − or + moves the sector at the start of the next frame (labour::request_priority), and the book
// is built again with the new order (its BuildKey holds it).

use common::Vec2;
use engine::ui::{self, UiString, UiTextAnchor, text::UiTextCategory, widgets::*};
use serde_json::json;

use super::{BODY, CityPage, PageContext, Paint, body_canvas, card_frame, number};
use crate::{
    labour::{self, LabourReport},
    menu::skin,
    sim::labour::{LABOUR_SECTOR_COUNT, LabourSector},
    ui_context::GameUiContext,
};

pub(super) struct Labour;

const CARD_HEIGHT: f32 = 118.0;
const GAP: f32 = 20.0;
const HEAD: f32 = 98.0; // The sectors card's title and column titles, down to the first row's rule.
const ROW: f32 = 64.0;
const STEPPER_X: f32 = 24.0;
const STEPPER_WIDTH: f32 = 130.0;
const STEPPER_BOX: f32 = 36.0;
const TEXT_X: f32 = STEPPER_X + STEPPER_WIDTH + 16.0; // Where a row's painted part starts.

impl CityPage for Labour {
    fn id(&self) -> &'static str {
        "labour"
    }

    fn icon(&self) -> &'static str {
        "industry"
    }

    fn note(&self, _context: &GameUiContext) -> String {
        ui::text::get(UiTextCategory::City, "book.note.labour").to_string()
    }

    fn body(&self, context: &mut GameUiContext, page: &PageContext) -> UiWidgetGroup {
        let report = {
            let sim = context.new_sim_context();
            labour::report(&sim)
        };
        let get = |key: &str| ui::text::get(UiTextCategory::City, key).to_string();

        // The four cards.
        let cards = [
            (get("book.labour.workers"), report.workforce),
            (get("book.labour.employed"), report.employed),
            (get("book.labour.looking"), report.looking_for_work),
            (get("book.labour.jobs_not_filled"), report.jobs_not_filled),
        ];
        let value = json!({
            "workers": report.workforce,
            "employed": report.employed,
            "looking_for_work": report.looking_for_work,
            "jobs_not_filled": report.jobs_not_filled,
        });
        let cards = body_canvas(page, BODY.x, CARD_HEIGHT, "cards", value, move |p: &Paint| {
            let width = (BODY.x - 3.0 * GAP) / 4.0;
            for (k, (label, count)) in cards.iter().enumerate() {
                let x = k as f32 * (width + GAP);
                p.card(x, 0.0, width, CARD_HEIGHT, None, None);
                p.text(x + 24.0, 42.0, label, 19.0, p.c.sub, UiTextAnchor::Start, false);
                p.text(x + 24.0, 92.0, &number(i64::from(*count), false), 36.0, p.c.text, UiTextAnchor::Start, true);
            }
        });

        super::left_group()
            .with_widget_spacing(Vec2::zero())
            .with_widget(cards)
            .with_widget(super::spacer(page.pt(GAP)))
            .with_widget(sectors_card(page, &report, BODY.y - CARD_HEIGHT - GAP))
    }
}

// The sectors card: its title and the columns' titles, a row per sector in priority order, and the
// note at its foot.
fn sectors_card(page: &PageContext, report: &LabourReport, height: f32) -> UiFrame {
    let get = |key: &str| ui::text::get(UiTextCategory::City, key).to_string();
    let value = json!(
        report
            .sectors
            .iter()
            .map(|sector| json!({
                "sector": sector.sector,
                "name": sector_name(sector.sector),
                "priority": sector.priority,
                "needed": sector.needed,
                "employed": sector.employed,
                "short": sector.short,
                "paused": sector.paused,
            }))
            .collect::<Vec<_>>()
    );
    let title = get("book.labour.sectors");
    let columns = ["priority", "sector", "needed", "employed"].map(|key| get(&format!("book.labour.{key}")));
    let head = body_canvas(page, BODY.x, HEAD, "sectors", value, move |p: &Paint| {
        p.section(22.0, 36.0, &title);
        let titles = [
            (STEPPER_X, UiTextAnchor::Start),
            (200.0, UiTextAnchor::Start),
            (560.0, UiTextAnchor::End),
            (690.0, UiTextAnchor::End),
        ];
        for ((x, anchor), text) in titles.iter().zip(&columns) {
            p.text(*x, 76.0, text, 17.0, p.c.sub, *anchor, false);
        }
    });

    let mut contents = super::left_group().with_widget_spacing(Vec2::zero()).with_widget(head);
    for sector in &report.sectors {
        contents = contents.with_widget(sector_row(page, sector));
    }
    let note = get("book.labour.note");
    let rows_height = HEAD + ROW * LABOUR_SECTOR_COUNT as f32;
    let foot = (height - rows_height).max(40.0);
    contents = contents.with_widget(body_canvas(page, BODY.x, foot, "note", json!(note), move |p: &Paint| {
        p.text(24.0, foot - 28.0, &note, 17.0, p.c.sub, UiTextAnchor::Start, false);
    }));
    card_frame(page, contents, BODY.x, height)
}

// A sector's row: its priority's − and +, then its name, the jobs needed and filled, a bar and
// "Full" or "N short".
fn sector_row(page: &PageContext, sector: &labour::SectorReport) -> UiFrame {
    let c = page.colors;
    let id = sector.sector;
    let name = sector_name(id);
    let steps = (1..=LABOUR_SECTOR_COUNT).map(|priority| UiString::literal(priority.to_string()));
    let look = UiStepLook {
        fill: c.accent,
        track: c.field,
        outline: Some(c.field_outline),
        segment_height: page.pt(STEPPER_BOX),
        selected_text: None,
        rounding: page.pt(5.0),
    };
    let stepper = skin::step_selector(name.clone(), steps)
        .with_style(UiStepSelectorStyle::PlusMinus)
        .with_look(look)
        .with_label_hidden(true)
        .with_text_colors(Some(c.text), Some(c.text))
        .with_font_scale(page.scales.heading_font(23.0))
        .with_width(page.pt(STEPPER_WIDTH))
        .with_current(sector.priority - 1)
        .on_changed(move |selector, _, _| labour::request_priority(id, selector.current() + 1));
    let stepper = super::left_group()
        .with_widget_spacing(Vec2::zero())
        .with_widget(super::spacer(page.pt((ROW - STEPPER_BOX) * 0.5)))
        .with_widget(stepper);

    let (needed, employed, short) = (sector.needed, sector.employed, sector.short);
    let full = get_city("book.labour.full");
    let short_text = ui::text::format_count(UiTextCategory::City, "book.labour.short", i64::from(short), &[]);
    let painted_name = name.clone();
    let text_width = BODY.x - TEXT_X;
    let value = json!({ "sector": id, "priority": sector.priority, "needed": needed, "employed": employed });
    let text = body_canvas(page, text_width, ROW, id.to_string().as_str(), value, move |p: &Paint| {
        // The row's x in the card, less the stepper's column (the canvas starts after it).
        let x = |card_x: f32| card_x - TEXT_X;
        let y = 38.0;
        p.rule(x(24.0), x(BODY.x - 24.0), 0.0);
        p.text(x(200.0), y, &painted_name, 21.0, p.c.text, UiTextAnchor::Start, false);
        p.text(x(560.0), y, &number(i64::from(needed), false), 20.0, p.c.text, UiTextAnchor::End, false);
        p.text(x(690.0), y, &number(i64::from(employed), false), 20.0, p.c.text, UiTextAnchor::End, false);
        let share = if needed > 0 { employed as f32 / needed as f32 } else { 0.0 };
        p.bar(x(730.0), y - 13.0, BODY.x - 930.0, 12.0, share);
        let (status, color) = match (needed, short) {
            (0, _) => ("—".to_string(), p.c.sub),
            (_, 0) => (full.clone(), p.c.sub),
            _ => (short_text.clone(), p.c.accent),
        };
        p.text(x(BODY.x - 24.0), y, &status, 19.0, color, UiTextAnchor::End, false);
    });

    let mut columns = UiColumns::new().with_spacing(0.0).with_width(page.pt(BODY.x));
    columns.add_column(UiColumnWidth::Fixed(page.pt(STEPPER_X)), super::left_group());
    columns.add_column(UiColumnWidth::Fixed(page.pt(STEPPER_WIDTH)), stepper);
    columns.add_column(UiColumnWidth::Fixed(page.pt(TEXT_X - STEPPER_X - STEPPER_WIDTH)), super::left_group());
    columns.add_column(UiColumnWidth::Fraction(1.0), super::left_group().with_widget(text));
    UiFrame::new(super::left_group().with_widget(columns), Vec2::new(page.pt(BODY.x), page.pt(ROW)))
}

fn get_city(key: &str) -> String {
    ui::text::get(UiTextCategory::City, key).to_string()
}

// A sector's name (Hud `labour.sector.<id>`).
pub(crate) fn sector_name(sector: LabourSector) -> String {
    ui::text::get(UiTextCategory::Hud, &sector.name_key()).to_string()
}
