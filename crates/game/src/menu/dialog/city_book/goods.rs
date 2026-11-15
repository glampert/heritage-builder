// The Goods page (city-goods-<realm>.jpg): the goods in the storage yards and granaries, by group
// (foods, consumer goods, raw materials), each stored against its room with its change since the
// month began; the storage buildings' totals; and the court's requests (none until M10's trade).

use common::Vec2;
use engine::ui::{self, UiTextAnchor, text::UiTextCategory, widgets::*};
use serde_json::json;

use super::{BODY, CityPage, PageContext, Paint, body_canvas, number};
use crate::{
    city::report::{GoodRow, GoodsGroup, StorageRow},
    ui_context::GameUiContext,
};

pub(super) struct Goods;

const GAP: f32 = 20.0;
const ROW: f32 = 56.0;
const CARD_HEAD: f32 = 82.0;

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

    fn body(&self, _context: &mut GameUiContext, page: &PageContext) -> UiWidgetGroup {
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
        let requests_title = get("book.requests");
        let no_requests = get("book.requests.none");
        let buildings_key = "book.storage.buildings";
        let storage_lines: Vec<(String, String, StorageRow)> = storage
            .into_iter()
            .map(|(name, row)| {
                (name, ui::text::format_count(UiTextCategory::City, buildings_key, i64::from(row.buildings), &[]), row)
            })
            .collect();

        let canvas = body_canvas(page, BODY.x, BODY.y, "goods", value, move |p: &Paint| {
            let width = (BODY.x - GAP) / 2.0;
            // Left: foods and consumer goods, then the storage.
            let mut y = 0.0;
            for (name, rows) in &groups[..2] {
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

            // Right: raw materials, then the requests.
            let x = width + GAP;
            let (name, rows) = &groups[2];
            let height = CARD_HEAD + rows.len() as f32 * ROW;
            p.card(x, 0.0, width, height, Some(name), Some(&column_note));
            goods_rows(p, x, 86.0, width, rows);
            let y = height + GAP;
            p.card(x, y, width, BODY.y - y, Some(&requests_title), None);
            p.text(x + 24.0, y + 90.0, &no_requests, 19.0, p.c.sub, UiTextAnchor::Start, false);
        });
        super::left_group().with_widget_spacing(Vec2::zero()).with_widget(canvas)
    }
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
