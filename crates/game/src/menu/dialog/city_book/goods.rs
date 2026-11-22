// The Goods page (city-goods-<realm>.jpg): the goods in the storage yards and granaries, by group
// (foods, consumer goods, raw materials), each stored against its room with its change since the
// month began; the storage buildings' totals; and the requests under way (crate::trade::requests,
// docs/plans/trade.md §6), each with what the city holds for it and Send now.
//
// Paths: CityManagement/Goods/goods (canvas: every good and the storage), raw_materials (canvas),
// and the Requests card, CityManagement/Goods/Requests: its canvas `requests` (value: a row per
// request shown, and `more`) and a frame per request by its id, with its `row` canvas and its Send
// now button (CityManagement/Goods/Requests/<id>/Send now), enabled once the stores hold the amount.

use common::Vec2;
use engine::ui::{self, UiString, UiTextAnchor, text::UiTextCategory, widgets::*};
use serde_json::json;

use super::{BODY, CityPage, PageContext, Paint, body_canvas, card_frame, number};
use crate::{
    city::report::{GoodRow, GoodsGroup, StorageRow},
    menu::{ButtonRole, skin},
    trade::requests::{self, RequestRow},
    ui_context::GameUiContext,
};

pub(super) struct Goods;

const GAP: f32 = 20.0;
const ROW: f32 = 56.0;
const CARD_HEAD: f32 = 82.0;

// The Requests card: its title's height, a request's row, Send now's width and height, and the line
// that says how many more are under way.
const REQUEST_HEAD: f32 = 56.0;
const REQUEST_ROW: f32 = 80.0;
const SEND_WIDTH: f32 = 160.0;
const SEND_HEIGHT: f32 = 46.0;
const MORE_LINE: f32 = 36.0;

impl CityPage for Goods {
    fn id(&self) -> &'static str {
        "goods"
    }

    fn icon(&self) -> &'static str {
        "jar"
    }

    fn note(&self, _context: &GameUiContext) -> String {
        ui::text::get(UiTextCategory::City, "book.note.goods").to_string()
    }

    fn body(&self, context: &mut GameUiContext, page: &PageContext) -> UiWidgetGroup {
        let report = page.report;
        let get = |key: &str| ui::text::get(UiTextCategory::City, key).to_string();
        let groups: Vec<(String, Vec<GoodRow>)> = GoodsGroup::ALL
            .iter()
            .map(|&group| (get(&format!("book.goods.{}", group.key())), report.goods_in(group).to_vec()))
            .collect();
        let storage = [
            (get("book.storage.yards"), report.storage_yards.clone()),
            (get("book.storage.granaries"), report.granaries.clone()),
        ];
        let value = json!({
            "goods": report.goods.iter().flat_map(|(group, rows)| rows.iter().map(move |row| json!({
                "group": group, "good": row.kind.name(), "stored": row.stored, "room": row.room, "change": row.change,
            }))).collect::<Vec<_>>(),
            "storage_yards": report.storage_yards,
            "granaries": report.granaries,
        });
        let column_note = get("book.goods.columns");
        let storage_title = get("book.storage");
        let buildings_key = "book.storage.buildings";
        let storage_lines: Vec<(String, String, StorageRow)> = storage
            .into_iter()
            .map(|(name, row)| {
                (name, ui::text::format_count(UiTextCategory::City, buildings_key, i64::from(row.buildings), &[]), row)
            })
            .collect();
        let width = (BODY.x - GAP) / 2.0;

        // Left: foods and consumer goods, then the storage.
        let left_groups = groups[..2].to_vec();
        let left = body_canvas(page, width, BODY.y, "goods", value, move |p: &Paint| {
            let mut y = 0.0;
            for (name, rows) in &left_groups {
                let height = CARD_HEAD + rows.len() as f32 * ROW;
                p.card(0.0, y, width, height, Some(name), Some(&column_note));
                goods_rows(p, 0.0, y + 86.0, width, rows);
                y += height + GAP;
            }
            p.card(0.0, y, width, BODY.y - y, Some(&storage_title), None);
            for (k, (name, buildings, row)) in storage_lines.iter().enumerate() {
                let ry = y + 82.0 + k as f32 * 64.0;
                p.text(24.0, ry, name, 20.0, p.c.text, UiTextAnchor::Start, false);
                p.text(24.0, ry + 24.0, buildings, 16.0, p.c.sub, UiTextAnchor::Start, false);
                let ratio = if row.capacity > 0 { row.stored as f32 / row.capacity as f32 } else { 0.0 };
                p.bar(190.0, ry - 12.0, width - 370.0, 10.0, ratio);
                let text = format!("{} / {}", number(i64::from(row.stored), false), number(i64::from(row.capacity), false));
                p.text(width - 24.0, ry, &text, 18.0, p.c.text, UiTextAnchor::End, false);
            }
        });

        // Right: raw materials, then the requests.
        let (name, rows) = groups[2].clone();
        let raw_height = CARD_HEAD + rows.len() as f32 * ROW;
        let column_note = get("book.goods.columns");
        let raw = body_canvas(page, width, raw_height, "raw_materials", serde_json::Value::Null, move |p: &Paint| {
            p.card(0.0, 0.0, width, raw_height, Some(&name), Some(&column_note));
            goods_rows(p, 0.0, 86.0, width, &rows);
        });
        let right = super::left_group()
            .with_widget_spacing(Vec2::zero())
            .with_widget(raw)
            .with_widget(super::spacer(page.pt(GAP)))
            .with_widget(requests_card(context, page, &report.requests, width, BODY.y - raw_height - GAP));

        let mut columns = UiColumns::new().with_spacing(page.pt(GAP)).with_width(page.pt(BODY.x));
        columns.add_column(UiColumnWidth::Fixed(page.pt(width)), super::left_group().with_widget(left));
        columns.add_column(UiColumnWidth::Fixed(page.pt(width)), right);
        super::left_group().with_widget_spacing(Vec2::zero()).with_widget(columns)
    }
}

// The Requests card (the mockup's: a request's name, when it's due, what the city holds of it
// against the amount, and Send now): a row per request under way, soonest due first, as many as fit,
// then how many more there are.
fn requests_card(context: &mut GameUiContext, page: &PageContext, rows: &[RequestRow], width: f32, height: f32) -> UiFrame {
    let fit = (((height - REQUEST_HEAD) / REQUEST_ROW).floor() as usize).max(1);
    let shown = if rows.len() > fit { fit - 1 } else { rows.len() };
    let more = rows.len() - shown;

    let title = ui::text::get(UiTextCategory::City, "book.requests").to_string();
    let none = ui::text::get(UiTextCategory::City, "book.requests.none").to_string();
    let empty = rows.is_empty();
    let value = json!({
        "requests": rows[..shown].iter().map(|row| json!({
            "id": row.id, "title": row.title, "good": row.good.name(), "amount": row.amount, "held": row.held,
            "months_left": row.months_left, "can_send": row.can_send(),
        })).collect::<Vec<_>>(),
        "more": more,
    });
    let head_height = if empty { REQUEST_HEAD + 50.0 } else { REQUEST_HEAD };
    let head = body_canvas(page, width, head_height, "requests", value, move |p: &Paint| {
        p.section(22.0, 36.0, &title);
        if empty {
            p.text(24.0, 90.0, &none, 19.0, p.c.sub, UiTextAnchor::Start, false);
        }
    });
    let mut contents = super::left_group().with_widget_spacing(Vec2::zero()).with_widget(head);

    let text_width = width - 24.0 - SEND_WIDTH - 16.0;
    for (index, row) in rows[..shown].iter().enumerate() {
        let last = index + 1 == shown;
        contents = contents.with_widget(request_row(context, page, row, text_width, last));
    }
    if more > 0 {
        let line = ui::text::format_count(UiTextCategory::City, "book.requests.more", more as i64, &[]);
        contents = contents.with_widget(body_canvas(page, width, MORE_LINE, "more", json!(more), move |p: &Paint| {
            p.text(24.0, 24.0, &line, 17.0, p.c.sub, UiTextAnchor::Start, false);
        }));
    }
    card_frame(page, contents, width, height).with_label("Requests")
}

// A request's row: its title, a bar of what the city holds against the amount and the two numbers,
// its months left (in the bad colour in its last month), and Send now.
fn request_row(context: &mut GameUiContext, page: &PageContext, row: &RequestRow, text_width: f32, last: bool) -> UiFrame {
    let value = json!({
        "id": row.id, "title": row.title, "held": row.held, "amount": row.amount, "months_left": row.months_left,
    });
    let title = row.title.clone();
    let left = ui::text::format_count(UiTextCategory::City, "book.requests.left", i64::from(row.months_left), &[]);
    let counts =
        format!("{} / {}", number(i64::from(row.held.min(row.amount)), false), number(i64::from(row.amount), false));
    let ratio = if row.amount > 0 { row.held as f32 / row.amount as f32 } else { 0.0 };
    let urgent = row.months_left <= 1;
    let rule_width = text_width + 16.0 + SEND_WIDTH;
    let text = body_canvas(page, text_width, REQUEST_ROW, "row", value, move |p: &Paint| {
        p.painter.rect_filled(p.rect(25.0, 14.0, 10.0, 10.0), p.c.accent, 1.0 * p.k);
        p.text(44.0, 26.0, &title, 20.0, p.c.text, UiTextAnchor::Start, false);
        p.bar(44.0, 38.0, text_width - 140.0, 10.0, ratio);
        p.text(text_width - 4.0, 48.0, &counts, 17.0, p.c.text, UiTextAnchor::End, true);
        p.text(44.0, 70.0, &left, 16.0, if urgent { p.c.bad } else { p.c.sub }, UiTextAnchor::Start, false);
        if !last {
            p.rule(24.0, rule_width, REQUEST_ROW - 1.0);
        }
    });

    let id = row.id.clone();
    let send_label = UiString::key(UiTextCategory::City, "book.requests.send");
    let mut send = skin::button(context, ButtonRole::Primary, send_label, move |_, _| {
        requests::request_send(&id);
    })
    .with_width(UiTextButtonWidth::Fixed(page.pt(SEND_WIDTH)))
    .with_min_height(page.pt(SEND_HEIGHT))
    .with_enabled(row.can_send());
    if !row.can_send() {
        let short = ui::text::format(UiTextCategory::City, "book.requests.short", &[
            ("held", &row.held.to_string()),
            ("amount", &row.amount.to_string()),
        ]);
        send = send.with_tooltip(skin::tooltip(context, short));
    }
    let send = page.scales.button_text(send, 19.0);
    let button = super::left_group()
        .with_widget_spacing(Vec2::zero())
        .with_widget(super::spacer(page.pt((REQUEST_ROW - SEND_HEIGHT) / 2.0)))
        .with_widget(send);

    let width = text_width + 16.0 + SEND_WIDTH + 24.0;
    let mut columns = UiColumns::new().with_spacing(0.0).with_width(page.pt(width));
    columns.add_column(UiColumnWidth::Fixed(page.pt(text_width + 16.0)), super::left_group().with_widget(text));
    columns.add_column(UiColumnWidth::Fixed(page.pt(SEND_WIDTH)), button);
    columns.add_column(UiColumnWidth::Fraction(1.0), super::left_group());
    UiFrame::new(super::left_group().with_widget(columns), Vec2::new(page.pt(width), page.pt(REQUEST_ROW)))
        .with_label(row.id.clone())
}

// A card's goods, a row each: a bullet and the good's name, a bar of what's stored against its
// room, the two numbers, and the change since the month began.
fn goods_rows(p: &Paint, x: f32, y: f32, width: f32, rows: &[GoodRow]) {
    for (k, row) in rows.iter().enumerate() {
        let ry = y + k as f32 * ROW;
        p.painter.rect_filled(p.rect(x + 25.0, ry - 12.0, 10.0, 10.0), p.c.accent, 1.0 * p.k);
        p.text(x + 48.0, ry, &row.name, 20.0, p.c.text, UiTextAnchor::Start, false);
        let (bx, bw) = (x + 190.0, width - 420.0);
        let ratio = if row.room > 0 { row.stored as f32 / row.room as f32 } else { 0.0 };
        p.bar(bx, ry - 12.0, bw, 10.0, ratio);
        let text = format!("{} / {}", number(i64::from(row.stored), false), number(i64::from(row.room), false));
        p.text(bx + bw + 100.0, ry, &text, 18.0, p.c.text, UiTextAnchor::End, false);
        p.change(x + width - 24.0, ry, row.change, 18.0);
        if k + 1 < rows.len() {
            p.rule(x + 24.0, x + width - 24.0, ry + 24.0);
        }
    }
}
