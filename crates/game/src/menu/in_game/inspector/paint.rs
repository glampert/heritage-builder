// Paints the inspector's sections as inspector-<kind>-<realm>.jpg lays them out
// (docs/design/mockups/generator/dialogs.py, Dialogs.inspector): in the mockup's px from a
// section's title baseline, scaled by the panel's `k` (Paint). `flow` lays a run of sections out,
// and paints them when given a painter, so the panel measures a run with the same code that
// paints it.

use common::{Color, Vec2};
use engine::{
    render::texture::TextureHandle,
    ui::{UiTextAnchor, text::UiTextCategory},
};

use crate::{
    inspect::{ErrandIcon, RiskRow, Section, Status, Tone, ValueRow},
    menu::dialog::city_book::Paint,
};

// Where a run's first section title's baseline sits below the run's top.
pub(super) const TOP: f32 = 22.0;

// The mockup's text sizes, px.
const BODY: f32 = 18.0;
const NOTE: f32 = 17.0;
const VALUE: f32 = 19.0;

// What a run of painted sections holds: the status line (first), then sections.
#[derive(Clone)]
pub(super) enum Item {
    Status(Option<Status>),
    Section(Section),
}

// The HUD's line icon the workers' figures and a walker's errand use.
#[derive(Copy, Clone)]
pub(super) struct Icons {
    pub population: Option<TextureHandle>,
}

// Lays `items` out in a run `w` px wide from its top, and paints them with `p`; returns the run's
// height in px (the next run's first title starts its own TOP below it).
pub(super) fn flow(p: Option<&Paint>, icons: Icons, items: &[Item], w: f32) -> f32 {
    let mut cy = TOP;
    let mut last_errand = false;
    let (mx, mw) = (190.0, w - 290.0); // A row's meter.
    for item in items {
        let section = match item {
            Item::Status(status) => {
                cy = status_line(p, status.as_ref(), w);
                continue;
            }
            Item::Section(section) => section,
        };
        let errand = matches!(section, Section::Errand { .. });
        if !errand && last_errand {
            cy += 20.0;
        }
        match section {
            Section::Workers { have, need, note, short, compact: false } => {
                if let Some(p) = p {
                    figures(p, icons, w, cy, "inspect.workers.title", *have, *need, note, *short);
                }
                cy += 116.0;
            }
            Section::Residents { have, room, note } => {
                if let Some(p) = p {
                    figures(p, icons, w, cy, "inspect.residents.title", *have, *room, note, false);
                }
                cy += 116.0;
            }
            Section::Workers { have, need, note, short, compact: true } => {
                // On one line (a store's).
                if let Some(p) = p {
                    p.section(0.0, cy, engine::ui::text::get(UiTextCategory::Hud, "inspect.workers.title"));
                    p.text(w, cy, &format!("{have} / {need}"), VALUE, p.c.text, UiTextAnchor::End, false);
                    let color = if *short { p.c.accent } else { p.c.sub };
                    p.text(0.0, cy + 30.0, note, NOTE, color, UiTextAnchor::Start, false);
                }
                cy += 76.0;
            }
            Section::Production { note, rows, progress } => {
                if let Some(p) = p {
                    p.section(0.0, cy, engine::ui::text::get(UiTextCategory::Hud, "inspect.production.title"));
                    p.text(w, cy, note, NOTE, p.c.sub, UiTextAnchor::End, false);
                    for (i, row) in rows.iter().enumerate() {
                        let ry = cy + 38.0 + i as f32 * 38.0;
                        p.text(0.0, ry, &row.label, BODY, p.c.sub, UiTextAnchor::Start, false);
                        bullet(p, 82.0, ry - 6.0);
                        p.text(98.0, ry, &row.good, VALUE, p.c.text, UiTextAnchor::Start, false);
                        p.bar(mx, ry - 12.0, mw, 10.0, ratio(row.have, row.capacity));
                        p.text(w, ry, &format!("{} / {}", row.have, row.capacity), BODY, p.c.text, UiTextAnchor::End, false);
                    }
                    let ry = cy + 38.0 + rows.len() as f32 * 38.0;
                    p.text(
                        0.0,
                        ry,
                        engine::ui::text::get(UiTextCategory::Hud, "inspect.production.next_batch"),
                        BODY,
                        p.c.sub,
                        UiTextAnchor::Start,
                        false,
                    );
                    p.bar(mx, ry - 12.0, mw, 10.0, *progress);
                    p.text(w, ry, &format!("{}%", (progress * 100.0).round()), BODY, p.c.text, UiTextAnchor::End, false);
                }
                cy += 38.0 + rows.len() as f32 * 38.0 + 54.0;
            }
            Section::Stock { title, note, rows, footer, warn_empty } => {
                if let Some(p) = p {
                    p.section(0.0, cy, title);
                    if let Some(note) = note {
                        p.text(w, cy, note, NOTE, p.c.sub, UiTextAnchor::End, false);
                    }
                    for (i, row) in rows.iter().enumerate() {
                        let ry = cy + 34.0 + i as f32 * 30.0;
                        bullet(p, 4.0, ry - 6.0);
                        p.text(20.0, ry, &row.good, VALUE, p.c.text, UiTextAnchor::Start, false);
                        p.bar(mx, ry - 12.0, mw, 10.0, ratio(row.have, row.capacity));
                        let missing = *warn_empty && row.have == 0;
                        let color = if missing { p.c.bad } else { p.c.text };
                        p.text(w, ry, &format!("{} / {}", row.have, row.capacity), BODY, color, UiTextAnchor::End, missing);
                    }
                }
                cy += 34.0 + rows.len() as f32 * 30.0;
                if let Some(footer) = footer {
                    if let Some(p) = p {
                        p.text(0.0, cy, footer, NOTE, p.c.sub, UiTextAnchor::Start, false);
                    }
                    cy += 28.0;
                }
                cy += 16.0;
            }
            Section::Ticks { title, note, rows, footer } => {
                let half = rows.len().div_ceil(2).max(1);
                if let Some(p) = p {
                    p.section(0.0, cy, title);
                    if let Some(note) = note {
                        p.text(w, cy, note, NOTE, p.c.sub, UiTextAnchor::End, false);
                    }
                    let column = w / 2.0;
                    for (i, row) in rows.iter().enumerate() {
                        let cx = (i / half) as f32 * column;
                        let ry = cy + 34.0 + (i % half) as f32 * 27.0;
                        tick(p, cx + 9.0, ry - 6.0, row.met);
                        p.text(cx + 28.0, ry, &row.name, NOTE, p.c.text, UiTextAnchor::Start, false);
                    }
                }
                cy += 34.0 + half as f32 * 27.0;
                if let Some(footer) = footer {
                    if let Some(p) = p {
                        tick(p, 9.0, cy - 6.0, false);
                        p.text(28.0, cy, footer, NOTE, p.c.text, UiTextAnchor::Start, false);
                    }
                    cy += 27.0;
                }
                cy += 18.0;
            }
            Section::Values { title, note, rows } => {
                if let Some(p) = p {
                    p.section(0.0, cy, title);
                    if let Some(note) = note {
                        p.text(w, cy, note, NOTE, p.c.sub, UiTextAnchor::End, false);
                    }
                    value_rows(p, 0.0, w, cy, rows);
                }
                cy += 34.0 + rows.len() as f32 * 29.0 + 16.0;
            }
            Section::Progress { title, note, value } => {
                if let Some(p) = p {
                    p.section(0.0, cy, title);
                    p.text(w, cy, note, NOTE, p.c.sub, UiTextAnchor::End, false);
                    p.bar(0.0, cy + 28.0, w - 70.0, 14.0, *value);
                    p.text(
                        w,
                        cy + 40.0,
                        &format!("{}%", (value * 100.0).round()),
                        VALUE,
                        p.c.text,
                        UiTextAnchor::End,
                        false,
                    );
                }
                cy += 82.0;
            }
            Section::Errand { icon, text } => {
                if !last_errand {
                    if let Some(p) = p {
                        p.rule(0.0, w, cy - 34.0);
                    }
                    cy += 4.0;
                }
                if let Some(p) = p {
                    match icon {
                        ErrandIcon::Cart => cart(p, 16.0, cy - 7.0),
                        ErrandIcon::Walker => figure(p, icons, 16.0, cy - 7.0, 20.0, false),
                    }
                    p.text(44.0, cy, text, BODY, p.c.text, UiTextAnchor::Start, false);
                }
                cy += 34.0;
            }
            Section::Risks { rows } => {
                if let Some(p) = p {
                    p.section(0.0, cy, engine::ui::text::get(UiTextCategory::Hud, "inspect.risks.title"));
                    for (i, row) in rows.iter().enumerate() {
                        let ry = cy + 34.0 + i as f32 * 29.0;
                        p.text(0.0, ry, &row.label, BODY, p.c.sub, UiTextAnchor::Start, false);
                        p.bar(mx, ry - 12.0, mw, 10.0, row.value / 100.0);
                        risk_level(p, w, ry, row);
                    }
                }
                cy += 34.0 + rows.len() as f32 * 29.0 + 16.0;
            }
            Section::CostsRisks { costs, risks } => {
                let rows = costs.len().max(risks.len());
                if let Some(p) = p {
                    let xm = w / 2.0;
                    p.section(0.0, cy, engine::ui::text::get(UiTextCategory::Hud, "inspect.costs.title"));
                    p.section(xm + 20.0, cy, engine::ui::text::get(UiTextCategory::Hud, "inspect.risks.title"));
                    value_rows(p, 0.0, xm - 20.0, cy, costs);
                    for (i, row) in risks.iter().enumerate() {
                        let ry = cy + 34.0 + i as f32 * 29.0;
                        p.text(xm + 20.0, ry, &row.label, BODY, p.c.sub, UiTextAnchor::Start, false);
                        risk_level(p, w, ry, row);
                    }
                    let bottom = cy + 34.0 + rows.saturating_sub(1) as f32 * 29.0 + 8.0;
                    p.painter.line(p.at(xm, cy - 16.0), p.at(xm, bottom), 1.0 * p.k, p.c.rule);
                }
                cy += 34.0 + rows as f32 * 29.0 + 16.0;
            }
            // Text and a store's goods are widgets of their own (the panel builds them).
            Section::Text { .. } | Section::Storage { .. } => {}
        }
        last_errand = errand;
    }
    if last_errand {
        cy += 20.0;
    }
    cy - TOP
}

// The status line: a dot and its word, the summary at the right, and the rule under them; or
// the rule alone. Returns the first section's baseline.
fn status_line(p: Option<&Paint>, status: Option<&Status>, w: f32) -> f32 {
    let Some(status) = status else {
        if let Some(p) = p {
            p.rule(0.0, w, 2.0);
        }
        return 44.0;
    };
    let sy = 12.0;
    if let Some(p) = p {
        let dot = match status.tone {
            Tone::Good => p.c.good,
            Tone::Pending => p.c.accent,
            Tone::Bad => p.c.bad,
        };
        p.painter.circle_filled(p.at(8.0, sy), 7.0 * p.k, dot);
        let word = if status.tone == Tone::Bad { p.c.bad } else { p.c.text };
        p.text(24.0, sy + 7.0, &status.word, 20.0, word, UiTextAnchor::Start, false);
        p.text(w, sy + 7.0, &status.summary, BODY, p.c.sub, UiTextAnchor::End, false);
        p.rule(0.0, w, sy + 28.0);
    }
    sy + 28.0 + 42.0
}

// A workforce's or a household's figures: the title and count, a figure per post (faded while
// empty), and the note under them (in the accent while short).
#[allow(clippy::too_many_arguments)]
fn figures(p: &Paint, icons: Icons, w: f32, cy: f32, title_key: &str, have: u32, need: u32, note: &str, short: bool) {
    p.section(0.0, cy, engine::ui::text::get(UiTextCategory::Hud, title_key));
    p.text(w, cy, &format!("{have} / {need}"), VALUE, p.c.text, UiTextAnchor::End, false);
    let step = (36.0_f32).min((w - 12.0) / need.max(1) as f32);
    let size = if step >= 30.0 { 22.0 } else { 17.0 };
    for i in 0..need {
        figure(p, icons, 12.0 + i as f32 * step, cy + 38.0, size, i >= have);
    }
    let color = if short { p.c.accent } else { p.c.sub };
    p.text(0.0, cy + 74.0, note, NOTE, color, UiTextAnchor::Start, false);
}

fn ratio(have: u32, capacity: u32) -> f32 {
    if capacity > 0 { have as f32 / capacity as f32 } else { 0.0 }
}

fn value_rows(p: &Paint, x0: f32, x1: f32, cy: f32, rows: &[ValueRow]) {
    for (i, row) in rows.iter().enumerate() {
        let ry = cy + 34.0 + i as f32 * 29.0;
        p.text(x0, ry, &row.label, BODY, p.c.sub, UiTextAnchor::Start, false);
        let color = if row.warn { p.c.bad } else { p.c.text };
        p.text(x1, ry, &row.value, BODY, color, UiTextAnchor::End, row.warn);
    }
}

// A risk's level at the right: in the realm's red when high.
fn risk_level(p: &Paint, x: f32, y: f32, row: &RiskRow) {
    let color = if row.high { p.c.bad } else { p.c.text };
    p.text(x, y, &row.level, BODY, color, UiTextAnchor::End, row.high);
}

// A good's small square, centred on (x + 4, y).
fn bullet(p: &Paint, x: f32, y: f32) {
    p.painter.rect_filled(p.rect(x - 1.0, y - 5.0, 9.0, 9.0), p.c.accent, 1.0 * p.k);
}

// A need met (a disc with a tick) or missing (a ring with a cross in the realm's red), centred on
// (x, y).
fn tick(p: &Paint, x: f32, y: f32, met: bool) {
    if met {
        p.painter.circle_filled(p.at(x, y), 9.0 * p.k, p.c.good);
        let points = [p.at(x - 4.5, y), p.at(x - 1.3, y + 3.4), p.at(x + 4.7, y - 3.6)];
        p.painter.polyline(&points, 2.2 * p.k, p.c.on_accent, false);
    } else {
        p.painter.circle_stroke(p.at(x, y), 8.5 * p.k, 2.0 * p.k, p.c.bad);
        p.painter.line(p.at(x - 3.5, y - 3.5), p.at(x + 3.5, y + 3.5), 2.0 * p.k, p.c.bad);
        p.painter.line(p.at(x + 3.5, y - 3.5), p.at(x - 3.5, y + 3.5), 2.0 * p.k, p.c.bad);
    }
}

// A worker's figure (the HUD's population icon), centred on (x, y): faded for an empty post.
fn figure(p: &Paint, icons: Icons, x: f32, y: f32, size: f32, faded: bool) {
    let tint = if faded { Color::new(1.0, 1.0, 1.0, 0.3) } else { Color::white() };
    match icons.population {
        Some(texture) => {
            let half = size * 0.5;
            p.painter.image(texture, p.rect(x - half, y - half, size, size), tint);
        }
        None => {
            let color = if faded { p.c.rule } else { p.c.accent };
            p.painter.circle_stroke(p.at(x, y - size * 0.22), size * 0.18 * p.k, 1.6 * p.k, color);
            p.painter.line(p.at(x - size * 0.3, y + size * 0.4), p.at(x + size * 0.3, y + size * 0.4), 1.6 * p.k, color);
        }
    }
}

// A handcart, its load and its wheel, centred on (x, y), in the accent.
fn cart(p: &Paint, x: f32, y: f32) {
    let c = p.c.accent;
    let w = 1.8 * p.k;
    let body = [p.at(x - 11.0, y - 7.0), p.at(x - 9.0, y + 2.0), p.at(x + 9.0, y + 2.0), p.at(x + 11.0, y - 7.0)];
    p.painter.polyline(&body, w, c, false);
    p.painter.line(p.at(x + 9.0, y + 2.0), p.at(x + 14.0, y - 4.0), w, c);
    p.painter.circle_stroke(p.at(x - 3.0, y + 7.0), 3.5 * p.k, w, c);
    let _ = Vec2::zero();
}
