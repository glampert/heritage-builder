// The Heritage page (city-heritage-<realm>.jpg): faith and monuments (GDD 5.11 and 5.15) in three
// tabs, Monument, Patrons and Festivals. The Patrons tab shows the realm's patrons (M8, faith and
// festivals; docs/plans/faith.md §5): a row each after the mockup generator's patron_row, with its
// favour, mood and what it sends, and the shrines and temples that tend it. A mission's monument
// comes with game Phase 2 and the festivals with M8's Phase 4, each filling its tab.

use common::{Color, Vec2};
use engine::{
    render::texture::TextureHandle,
    ui::{self, UiTextAnchor, text::UiTextCategory, widgets::*},
};
use serde_json::json;

use super::{BODY, CityPage, PageContext, Paint, STATE, body_canvas};
use crate::{
    config::GameConfigs,
    faith::{self, Mood, config as faith_config},
    menu::skin,
    ui_context::GameUiContext,
};

pub(super) struct Heritage;

const TABS: [&str; 3] = ["monument", "patrons", "festivals"];

impl CityPage for Heritage {
    fn id(&self) -> &'static str {
        "heritage"
    }

    fn icon(&self) -> &'static str {
        "monument"
    }

    fn note(&self, _context: &GameUiContext) -> String {
        ui::text::get(UiTextCategory::City, "book.note.heritage").to_string()
    }

    fn body(&self, context: &mut GameUiContext, page: &PageContext) -> UiWidgetGroup {
        let size = Vec2::new(page.pt(200.0), page.pt(50.0));
        let mut tabs = skin::tab_bar_sized(context, size, 1.0)
            .with_label(ui::text::get(UiTextCategory::City, "book.heritage.tabs").to_string())
            .with_selected(STATE.as_ref().heritage_tab.min(TABS.len() - 1))
            .on_tab_changed(|bar, _, _| STATE.as_mut().heritage_tab = bar.selected_index());
        // The tabs' row and the gap under it, as a bar with one tab measures it.
        let tabs_height = skin::tab_bar_sized(context, size, 1.0).with_tab("Tab").measure(context).y;
        let height = (BODY.y - tabs_height / page.scales.size).max(100.0);
        let patrons = patron_rows(context);
        for tab in TABS {
            let title = ui::text::get(UiTextCategory::City, &format!("book.heritage.{tab}")).to_string();
            if tab == "patrons" && !patrons.rows.is_empty() {
                let canvas = patrons_canvas(page, height, title.clone(), patrons.clone());
                tabs = tabs.with_tab_contents(title, super::left_group().with_widget(canvas));
                continue;
            }
            let empty = ui::text::get(UiTextCategory::City, &format!("book.heritage.{tab}.none")).to_string();
            let later = ui::text::get(UiTextCategory::City, &format!("book.heritage.{tab}.later")).to_string();
            let card_title = title.clone();
            let canvas =
                body_canvas(page, BODY.x, height, tab, json!({ "empty": true, "text": empty }), move |p: &Paint| {
                    p.card(0.0, 0.0, BODY.x, height, Some(&card_title), None);
                    p.text(BODY.x * 0.5, height * 0.45, &empty, 24.0, p.c.text, UiTextAnchor::Middle, true);
                    p.text(BODY.x * 0.5, height * 0.45 + 40.0, &later, 18.0, p.c.sub, UiTextAnchor::Middle, false);
                });
            tabs = tabs.with_tab_contents(title, super::left_group().with_widget(canvas));
        }
        super::left_group().with_widget_spacing(Vec2::zero()).with_widget(tabs)
    }
}

// ----------------------------------------------
// Patrons
// ----------------------------------------------

// A patron's row, as read when the book is built.
#[derive(Clone)]
struct PatronRow {
    id: String,
    name: String,
    domain: String,
    icon: TextureHandle,
    favour: f32,
    mood: String,
    low: bool,      // Below Content: its mood and the ring in the realm's red.
    effect: String, // What it sends: "Above 70: a rich harvest", or below 30 a misfortune.
    tended: String, // Its shrines and temples, and the residents they reach.
    shrines: u32,
    temples: u32,
    residents: u32,
    coverage: f32,
}

#[derive(Clone)]
struct Patrons {
    rows: Vec<PatronRow>,
    note: String,
}

// The game's patrons: their favour from the city's faith, their coverage as the month's drift reads
// it (faith::coverage), and their first blessing and misfortune that can happen now (else the first).
fn patron_rows(context: &mut GameUiContext) -> Patrons {
    let rules = &GameConfigs::get().sim.faith;
    let realm = context.sim.realm();
    let faith_state = context.sim.faith();
    let note_key = if faith_state.is_favour_enabled() { "book.heritage.patrons.note" } else { "book.heritage.patrons.held" };
    let note = ui::text::get(UiTextCategory::City, note_key).to_string();
    let Some(def) = faith::FaithConfigs::get().for_realm(realm) else {
        return Patrons { rows: Vec::new(), note };
    };
    let coverage = faith::coverage(context.world, context.tile_map, realm, rules);
    let population = faith::population(context.world);
    let favours: Vec<f32> = def.patrons.iter().map(|patron| faith_state.favour(&patron.id).unwrap_or_default()).collect();

    let event_name = |ids: &[String]| -> Option<String> {
        let id = ids.iter().find(|id| def.event(id).is_some_and(|event| event.is_available())).or(ids.first())?;
        Some(ui::text::get(UiTextCategory::Realm, &faith_config::event_name_key(realm, id)).to_lowercase())
    };

    let rows = def
        .patrons
        .iter()
        .zip(favours)
        .map(|(patron, favour)| {
            let tended = coverage.get(&patron.id).cloned().unwrap_or_default();
            let mood = Mood::from_favour(favour);
            let low = favour < 40.0;
            let effect = if low {
                event_name(&patron.misfortunes).map(|event| {
                    ui::text::format(UiTextCategory::City, "book.heritage.patron.below", &[
                        ("favour", &format!("{:.0}", rules.misfortune_favour)),
                        ("event", &event),
                    ])
                })
            } else {
                event_name(&patron.blessings).map(|event| {
                    ui::text::format(UiTextCategory::City, "book.heritage.patron.above", &[
                        ("favour", &format!("{:.0}", rules.blessing_favour)),
                        ("event", &event),
                    ])
                })
            };
            let tended_text = format!(
                "{} · {} · {}",
                ui::text::format_count(UiTextCategory::City, "book.heritage.patron.shrines", i64::from(tended.shrines), &[]),
                ui::text::format_count(UiTextCategory::City, "book.heritage.patron.temples", i64::from(tended.temples), &[]),
                ui::text::format(UiTextCategory::City, "book.heritage.patron.reach", &[
                    ("residents", &super::number(i64::from(tended.residents), false)),
                    ("population", &super::number(i64::from(population), false)),
                ]),
            );
            PatronRow {
                id: patron.id.clone(),
                name: faith_config::patron_name(realm, &patron.id),
                domain: ui::text::get(UiTextCategory::Realm, &faith_config::patron_domain_key(realm, &patron.id))
                    .to_string(),
                icon: skin::city_icon(context, &patron.icon),
                favour,
                mood: mood.display_name(),
                low,
                effect: effect.unwrap_or_default(),
                tended: tended_text,
                shrines: tended.shrines,
                temples: tended.temples,
                residents: tended.residents,
                coverage: tended.coverage,
            }
        })
        .collect();
    Patrons { rows, note }
}

// The patrons' card: a row each, its icon in a ring, its name and domain, its mood, favour and
// meter at the right with what it sends, and the shrines and temples that tend it under its name.
fn patrons_canvas(page: &PageContext, height: f32, title: String, patrons: Patrons) -> UiCanvas {
    let value = json!({
        "note": patrons.note,
        "patrons": patrons.rows.iter().map(|row| json!({
            "id": row.id,
            "name": row.name,
            "favour": (f64::from(row.favour) * 100.0).round() / 100.0,
            "mood": row.mood,
            "effect": row.effect,
            "shrines": row.shrines,
            "temples": row.temples,
            "residents": row.residents,
            "coverage": (f64::from(row.coverage) * 100.0).round() / 100.0,
        })).collect::<Vec<_>>(),
    });
    body_canvas(page, BODY.x, height, "patrons", value, move |p: &Paint| {
        p.card(0.0, 0.0, BODY.x, height, Some(&title), Some(&patrons.note));
        let (x, w) = (24.0, BODY.x - 48.0);
        let top = 96.0;
        let step = ((height - top - 24.0) / patrons.rows.len().max(1) as f32).clamp(96.0, 150.0);
        for (index, row) in patrons.rows.iter().enumerate() {
            let y = top + index as f32 * step + 12.0;
            if index > 0 {
                p.rule(x, x + w, y - step * 0.5 + 6.0);
            }
            let ring = if row.low { p.c.sub } else { p.c.accent };
            p.painter.circle_filled(p.at(x + 30.0, y + 2.0), 22.0 * p.k, p.c.field);
            p.painter.circle_stroke(p.at(x + 30.0, y + 2.0), 22.0 * p.k, 2.0 * p.k, ring);
            p.painter.image(row.icon, p.rect(x + 14.0, y - 14.0, 32.0, 32.0), p.c.accent);
            p.text(x + 66.0, y - 2.0, &row.name, 20.0, p.c.text, UiTextAnchor::Start, true);
            p.text(x + 66.0, y + 22.0, &row.domain, 16.0, p.c.sub, UiTextAnchor::Start, false);
            p.text(x + 66.0, y + 46.0, &row.tended, 15.0, p.c.sub, UiTextAnchor::Start, false);

            let mood_color: Color = if row.low { p.c.bad } else { p.c.good };
            let favour = format!("{} · {:.0}", row.mood, row.favour);
            p.text(x + w - 24.0, y - 2.0, &favour, 17.0, mood_color, UiTextAnchor::End, false);
            let bar = 200.0;
            p.bar(x + w - 24.0 - bar, y + 12.0, bar, 9.0, row.favour / 100.0);
            if !row.effect.is_empty() {
                p.text(x + w - 44.0 - bar, y + 22.0, &row.effect, 15.0, p.c.sub, UiTextAnchor::End, false);
            }
        }
    })
}
