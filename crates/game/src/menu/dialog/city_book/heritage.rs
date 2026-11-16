// The Heritage page (city-heritage-<realm>.jpg): faith and monuments (GDD 5.11 and 5.15) in three
// tabs, Monument, Patrons and Festivals (M8, faith and festivals; docs/plans/faith.md §5). The
// Patrons tab shows the realm's patrons: a row each after the mockup generator's patron_row, with
// its favour, mood and what it sends, and the shrines and temples that tend it. The Festivals tab
// is the mockup's: the festival year as a month strip, the festivals with where each stands this
// year, and the planner (its scale, Hold or Cancel, its costs against what the city holds and its
// effects). A mission's monument comes with game Phase 2 and fills its tab.

use std::hash::{DefaultHasher, Hash, Hasher};

use common::{Color, Vec2};
use engine::{
    render::texture::TextureHandle,
    ui::{self, UiString, UiTextAnchor, text::UiTextCategory, widgets::*},
};
use serde_json::json;

use super::{BODY, CityPage, PageContext, Paint, STATE, body_canvas, number};
use crate::{
    config::GameConfigs,
    faith::{
        self,
        FaithConfigs,
        Mood,
        config as faith_config,
        festivals::{self, FestivalState, Request},
    },
    menu::{ButtonRole, skin},
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
        let festivals = festival_tab(context);
        for tab in TABS {
            let title = ui::text::get(UiTextCategory::City, &format!("book.heritage.{tab}")).to_string();
            if tab == "patrons" && !patrons.rows.is_empty() {
                let canvas = patrons_canvas(page, height, title.clone(), patrons.clone());
                tabs = tabs.with_tab_contents(title, super::left_group().with_widget(canvas));
                continue;
            }
            if tab == "festivals"
                && let Some(festivals) = &festivals
            {
                let contents = festivals_contents(context, page, height, festivals.clone());
                tabs = tabs.with_tab_contents(title, contents);
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

// ----------------------------------------------
// Festivals
// ----------------------------------------------

// The Festivals tab's layout, in mockup px (pages.py's heritage()).
const STRIP_HEIGHT: f32 = 172.0;
const GAP: f32 = 20.0;
const LIST_WIDTH: f32 = 470.0;
const LIST_HEAD: f32 = 56.0;
const ROW_HEIGHT: f32 = 66.0;
const ROW_SPACING: f32 = 10.0;
const LAST_HELD: f32 = 64.0;
const PLANNER_HEAD: f32 = 190.0;
const CONTROLS: f32 = 58.0;
const HOLD_WIDTH: f32 = 230.0;

// A festival as the tab shows it.
#[derive(Clone)]
struct FestivalRow {
    id: String,
    name: String,
    month: u32,
    patrons: String, // "For the Water Lord".
    state: FestivalState,
    state_text: String,
}

// What the planner shows: the festival picked, at the scale picked.
#[derive(Clone)]
struct Planner {
    id: String,
    name: String,
    month: u32,
    venue: String,
    text: String,
    state: FestivalState,
    scales: Vec<(String, String)>, // (id, name).
    scale: usize,
    costs: Vec<(String, u32, u32)>, // What, how much, how much the city holds.
    effects: Vec<(String, String)>, // What, by how much.
    button: String,                 // Hold in month N, Hold now or Cancel.
    enabled: bool,                  // Festivals are on for the mission.
    status: Option<(String, bool)>, // A line under the terms, and whether it's a warning.
}

// The tab, read when the book is built: the realm's festival year, this month, the planner.
#[derive(Clone)]
struct FestivalTab {
    rows: Vec<FestivalRow>,
    month: u32,
    selected: usize, // In rows: the planner's festival.
    last_held: String,
    planner: Planner,
}

// What the festivals tab is built from, for the book's rebuild key: the plan, the festivals held,
// the switch, the staffed squares and the planner's pick.
pub(super) fn festivals_key(context: &GameUiContext) -> u64 {
    let faith = context.sim.faith();
    let state = STATE.as_ref();
    let mut hasher = DefaultHasher::new();
    faith.plan().map(|plan| (&plan.festival, &plan.scale)).hash(&mut hasher);
    faith.log().len().hash(&mut hasher);
    faith.is_festivals_enabled().hash(&mut hasher);
    festivals::venues(context.world).len().hash(&mut hasher);
    state.festival.hash(&mut hasher);
    state.scale.hash(&mut hasher);
    hasher.finish()
}

fn city_text(key: &str) -> String {
    ui::text::get(UiTextCategory::City, key).to_string()
}

// The game's festivals, None for a realm without any (the tab shows its empty card then).
fn festival_tab(context: &mut GameUiContext) -> Option<FestivalTab> {
    let realm = context.sim.realm();
    let def = FaithConfigs::get().for_realm(realm).filter(|def| !def.festivals.is_empty())?;
    let faith = context.sim.faith();
    let date = festivals::date_of(context.sim.calendar());

    // The planner's festival: the one picked, else the next; its scale: the one picked, else the
    // plan's, else the middle one.
    let state = STATE.as_mut();
    let festival =
        state.festival.as_deref().and_then(|id| def.festival(id)).or_else(|| festivals::next_festival(def, faith, date))?;
    state.festival = Some(festival.id.clone());
    let planned_scale = faith.plan().filter(|plan| plan.festival == festival.id).map(|plan| plan.scale.as_str());
    let scale = state
        .scale
        .as_deref()
        .and_then(|id| def.scale(id))
        .or_else(|| planned_scale.and_then(|id| def.scale(id)))
        .or_else(|| festivals::default_scale(def))?;

    let patrons_of = |festival: &faith::config::FestivalDef| {
        if festival.is_for_every_patron() {
            city_text("book.festivals.for_all")
        } else {
            ui::text::format(UiTextCategory::City, "book.festivals.for", &[(
                "patron",
                &faith_config::patron_name(realm, &festival.patron),
            )])
        }
    };
    let rows: Vec<FestivalRow> = def
        .festivals
        .iter()
        .map(|row| {
            let row_state = festivals::state(faith, row, date);
            let state_text = match row_state {
                FestivalState::Planned => {
                    let scale = faith.plan().map(|plan| faith_config::scale_name(realm, &plan.scale)).unwrap_or_default();
                    ui::text::format(UiTextCategory::City, "book.festivals.state.planned", &[(
                        "scale",
                        &scale.to_lowercase(),
                    )])
                }
                other => city_text(&format!("book.festivals.state.{other}")),
            };
            FestivalRow {
                id: row.id.clone(),
                name: faith_config::festival_name(realm, &row.id),
                month: row.month,
                patrons: patrons_of(row),
                state: row_state,
                state_text,
            }
        })
        .collect();
    let selected = rows.iter().position(|row| row.id == festival.id).unwrap_or_default();

    let last_held = match faith.log().last() {
        Some(record) => ui::text::format(UiTextCategory::City, "book.festivals.last_held.line", &[
            ("festival", &faith_config::festival_name(realm, &record.festival)),
            ("month", &(record.ordinal_month % 12 + 1).to_string()),
            ("scale", &faith_config::scale_name(realm, &record.scale).to_lowercase()),
            ("culture", &format!("{:.0}", record.culture)),
        ]),
        None => city_text("book.festivals.last_held.none"),
    };

    // The planner.
    let terms = festivals::terms(festival, scale);
    let balance = context.sim.finance().balance().max(0);
    let mut costs = vec![(city_text("book.festivals.gold"), terms.gold, balance.min(i64::from(u32::MAX)) as u32)];
    for item in &terms.goods {
        costs.push((item.kind.display_name().to_string(), item.count, festivals::held(context.world, item.kind)));
    }
    let mut effects = vec![(festivals::patrons_text(realm, festival), number(terms.favour.round() as i64, true))];
    if terms.culture.round() != 0.0 {
        effects.push((
            ui::text::get(UiTextCategory::Hud, "rating.culture").to_string(),
            number(terms.culture.round() as i64, true),
        ));
    }
    if terms.legitimacy.round() != 0.0 {
        effects.push((
            ui::text::get(UiTextCategory::Realm, &format!("rating.{realm}.fourth")).to_string(),
            number(terms.legitimacy.round() as i64, true),
        ));
    }
    effects.push((city_text("book.festivals.unrest"), city_text("book.festivals.calmed")));

    let enabled = faith.is_festivals_enabled();
    let festival_state = festivals::state(faith, festival, date);
    let button = match festival_state {
        _ if !enabled => city_text("book.festivals.off"),
        FestivalState::Planned => city_text("book.festivals.cancel"),
        FestivalState::Now => city_text("book.festivals.hold_now"),
        _ => ui::text::format(UiTextCategory::City, "book.festivals.hold_in", &[("month", &festival.month.to_string())]),
    };
    let held_this_year =
        faith.log().iter().rev().find(|record| record.festival == festival.id && record.ordinal_month / 12 == date.year);
    let status = if !enabled {
        Some((city_text("book.festivals.status.off"), true))
    } else if festivals::venues(context.world).is_empty() {
        Some((city_text("book.festivals.status.no_venue"), true))
    } else {
        match festival_state {
            FestivalState::Planned => Some((
                ui::text::format(UiTextCategory::City, "book.festivals.status.planned", &[(
                    "month",
                    &festival.month.to_string(),
                )]),
                false,
            )),
            FestivalState::Now => Some((city_text("book.festivals.status.now"), false)),
            _ => held_this_year.map(|record| {
                let text = ui::text::format(UiTextCategory::City, "book.festivals.status.held", &[
                    ("month", &(record.ordinal_month % 12 + 1).to_string()),
                    ("scale", &faith_config::scale_name(realm, &record.scale).to_lowercase()),
                ]);
                (text, false)
            }),
        }
    };

    let planner = Planner {
        id: festival.id.clone(),
        name: faith_config::festival_name(realm, &festival.id),
        month: festival.month,
        venue: ui::text::get(
            UiTextCategory::Realm,
            &format!("{}.venue", faith_config::festival_name_key(realm, &festival.id)),
        )
        .to_string(),
        text: ui::text::get(
            UiTextCategory::Story,
            &format!("{}.text", faith_config::festival_name_key(realm, &festival.id)),
        )
        .to_string(),
        state: festival_state,
        scales: def.scales.iter().map(|scale| (scale.id.clone(), faith_config::scale_name(realm, &scale.id))).collect(),
        scale: def.scales.iter().position(|other| other.id == scale.id).unwrap_or_default(),
        costs,
        effects,
        button,
        enabled,
        status,
    };
    Some(FestivalTab { rows, month: date.month, selected, last_held, planner })
}

// The tab: the year's strip over the festivals' list and the planner side by side.
fn festivals_contents(context: &mut GameUiContext, page: &PageContext, height: f32, tab: FestivalTab) -> UiWidgetGroup {
    let lower = (height - STRIP_HEIGHT - GAP).max(200.0);
    let mut columns = UiColumns::new().with_spacing(page.pt(GAP)).with_width(page.pt(BODY.x));
    columns.add_column(
        UiColumnWidth::Fixed(page.pt(LIST_WIDTH)),
        super::left_group().with_widget(festival_list(page, lower, &tab)),
    );
    columns.add_column(UiColumnWidth::Fraction(1.0), super::left_group().with_widget(planner(context, page, lower, &tab)));
    super::left_group()
        .with_widget_spacing(Vec2::zero())
        .with_widget(year_strip(page, &tab))
        .with_widget(super::spacer(page.pt(GAP)))
        .with_widget(columns)
}

// A card's frame: the book's soft fill and outline, `width` × `height` px.
fn card_frame(page: &PageContext, contents: UiWidgetGroup, width: f32, height: f32) -> UiFrame {
    let c = page.colors;
    UiFrame::new(contents, Vec2::new(page.pt(width), page.pt(height)))
        .with_fill(c.card, page.pt(6.0))
        .with_outline(UiFrameOutline { color: c.card_outline, width: page.pt(1.5), inset: 0.0, dash: None })
}

// The festival year: a strip of the twelve months, "now" over this one, a dot per festival (larger
// for the planner's, faded once held or missed) and its name under the strip.
fn year_strip(page: &PageContext, tab: &FestivalTab) -> UiCanvas {
    let value = json!({
        "month": tab.month,
        "selected": tab.rows.get(tab.selected).map(|row| row.id.clone()),
        "festivals": tab.rows.iter().map(|row| json!({
            "id": row.id, "name": row.name, "month": row.month, "state": row.state.to_string(),
        })).collect::<Vec<_>>(),
    });
    let title = city_text("book.festivals.year");
    let note = city_text("book.festivals.year.note");
    let now = city_text("book.festivals.now");
    let (rows, month, selected) = (tab.rows.clone(), tab.month, tab.selected);
    body_canvas(page, BODY.x, STRIP_HEIGHT, "year", value, move |p: &Paint| {
        p.card(0.0, 0.0, BODY.x, STRIP_HEIGHT, Some(&title), Some(&note));
        let (x0, x1, ly) = (40.0, BODY.x - 40.0, 84.0);
        let step = (x1 - x0) / 11.0;
        p.painter.line(p.at(x0, ly), p.at(x1, ly), 3.0 * p.k, p.c.rule);
        for m in 1..=12 {
            let mx = x0 + (m - 1) as f32 * step;
            p.painter.line(p.at(mx, ly - 6.0), p.at(mx, ly + 6.0), 1.5 * p.k, p.c.sub);
            let this = m == month;
            p.text(mx, ly + 30.0, &m.to_string(), 15.0, if this { p.c.text } else { p.c.sub }, UiTextAnchor::Middle, this);
        }
        let nx = x0 + (month.max(1) - 1) as f32 * step;
        let arrow = [p.at(nx, ly - 14.0), p.at(nx + 7.0, ly - 24.0), p.at(nx - 7.0, ly - 24.0)];
        p.painter.convex_polygon(&arrow, p.c.text, 0.0, Color::transparent());
        p.text(nx, ly - 30.0, &now, 14.0, p.c.text, UiTextAnchor::Middle, false);
        for (n, row) in rows.iter().enumerate() {
            let mx = x0 + (row.month.max(1) - 1) as f32 * step;
            let on = n == selected;
            let fill = match row.state {
                FestivalState::Held | FestivalState::Missed => p.c.sub,
                _ => p.c.accent,
            };
            let radius = if on { 11.0 } else { 8.0 };
            p.painter.circle_filled(p.at(mx, ly), radius * p.k, fill);
            p.painter.circle_stroke(p.at(mx, ly), radius * p.k, 2.0 * p.k, p.c.card);
            // Names near the strip's ends keep to the dot's inner side, inside the card.
            let (lx, anchor) = match row.month {
                ..=1 => (mx - 12.0, UiTextAnchor::Start),
                12.. => (mx + 12.0, UiTextAnchor::End),
                _ => (mx, UiTextAnchor::Middle),
            };
            let color = if on { p.c.accent } else { p.c.text };
            p.text(lx, ly + 54.0 + (n % 2) as f32 * 22.0, &row.name, 15.0, color, anchor, on);
        }
    })
}

// The festivals: a row each (a card that picks it for the planner), and the last one held.
fn festival_list(page: &PageContext, height: f32, tab: &FestivalTab) -> UiFrame {
    let title = city_text("book.festivals.list");
    let head = body_canvas(page, LIST_WIDTH, LIST_HEAD, "list", json!(null), move |p: &Paint| {
        p.section(22.0, 36.0, &title);
    });
    let c = page.colors;
    let hover = Color::new(c.hover.r, c.hover.g, c.hover.b, c.hover.a * 0.6);
    let row_width = LIST_WIDTH - 24.0;
    let mut rows = super::left_group()
        .with_widget_spacing(Vec2::new(0.0, page.pt(ROW_SPACING)))
        .with_margins(page.pt(12.0), page.pt(12.0));
    for (index, row) in tab.rows.iter().enumerate() {
        let selected = index == tab.selected;
        let accent_state = matches!(row.state, FestivalState::Planned | FestivalState::Now);
        let value = json!({ "id": row.id, "month": row.month, "state": row.state.to_string(), "text": row.state_text });
        let month = ui::text::format(UiTextCategory::City, "book.festivals.month", &[("month", &row.month.to_string())]);
        let shown = row.clone();
        let canvas = body_canvas(page, row_width, ROW_HEIGHT, "row", value, move |p: &Paint| {
            if selected {
                let rect = p.rect(0.0, 0.0, row_width, ROW_HEIGHT);
                p.painter.rect_filled(rect, p.c.field, 6.0 * p.k);
                p.painter.rect_stroke(rect, 6.0 * p.k, 2.0 * p.k, p.c.accent);
            }
            p.text(12.0, 30.0, &shown.name, 21.0, p.c.text, UiTextAnchor::Start, true);
            p.text(row_width - 12.0, 30.0, &month, 17.0, p.c.sub, UiTextAnchor::End, false);
            p.text(12.0, 54.0, &shown.patrons, 16.0, p.c.sub, UiTextAnchor::Start, false);
            let color = if accent_state { p.c.accent } else { p.c.sub };
            p.text(row_width - 12.0, 54.0, &shown.state_text, 16.0, color, UiTextAnchor::End, accent_state);
        });
        let (read_id, press_id) = (row.id.clone(), row.id.clone());
        let card = UiCard::new(row.name.clone(), super::left_group().with_widget_spacing(Vec2::zero()).with_widget(canvas))
            .with_padding(Vec2::zero())
            .with_tints(hover, Color::new(0.9, 0.9, 0.9, 1.0), Color::new(1.0, 1.0, 1.0, 0.6))
            .on_read_selected(move |_, _| STATE.as_ref().festival.as_deref() == Some(read_id.as_str()))
            .on_pressed(move |_, _| {
                let state = STATE.as_mut();
                if state.festival.as_deref() != Some(press_id.as_str()) {
                    state.festival = Some(press_id.clone());
                    state.scale = None;
                }
            });
        rows.add_widget(card);
    }

    let label = city_text("book.festivals.last_held");
    let line = tab.last_held.clone();
    let last = body_canvas(page, LIST_WIDTH, LAST_HELD, "last_held", json!({ "text": tab.last_held }), move |p: &Paint| {
        p.text(24.0, 18.0, &label, 15.0, p.c.sub, UiTextAnchor::Start, true);
        p.text(24.0, 42.0, &line, 15.0, p.c.sub, UiTextAnchor::Start, false);
    });
    let count = tab.rows.len() as f32;
    let used = LIST_HEAD + count * ROW_HEIGHT + (count - 1.0).max(0.0) * ROW_SPACING + LAST_HELD;
    let contents = super::left_group()
        .with_widget_spacing(Vec2::zero())
        .with_widget(head)
        .with_widget(rows)
        .with_widget(super::spacer(page.pt((height - used - 4.0).max(0.0))))
        .with_widget(last);
    card_frame(page, contents, LIST_WIDTH, height)
}

// The planner: the festival's name, venue and text, its scales and Hold (or Cancel), and its costs
// against what the city holds and its effects.
fn planner(context: &mut GameUiContext, page: &PageContext, height: f32, tab: &FestivalTab) -> UiFrame {
    let width = BODY.x - LIST_WIDTH - GAP;
    let c = page.colors;
    let plan = tab.planner.clone();

    let title = city_text("book.festivals.plan");
    let month = ui::text::format(UiTextCategory::City, "book.festivals.month", &[("month", &plan.month.to_string())]);
    let value = json!({
        "festival": plan.id,
        "name": plan.name,
        "month": plan.month,
        "venue": plan.venue,
        "state": plan.state.to_string(),
        "scale": plan.scales.get(plan.scale).map(|scale| scale.0.clone()),
    });
    let shown = plan.clone();
    let head = body_canvas(page, width, PLANNER_HEAD, "planner", value, move |p: &Paint| {
        p.section(22.0, 36.0, &title);
        p.text(width - 22.0, 36.0, &month, 17.0, p.c.sub, UiTextAnchor::End, false);
        p.text(24.0, 84.0, &shown.name, 28.0, p.c.text, UiTextAnchor::Start, true);
        p.text(24.0, 110.0, &shown.venue, 17.0, p.c.sub, UiTextAnchor::Start, false);
        // The text on two lines at most, set smaller if it needs more.
        let mut px = 18.0;
        let mut lines = p.painter.wrap_label(&shown.text, &p.look(px, p.c.text, false), (width - 48.0) * p.k);
        while lines.len() > 2 && px > 13.0 {
            px -= 1.0;
            lines = p.painter.wrap_label(&shown.text, &p.look(px, p.c.text, false), (width - 48.0) * p.k);
        }
        for (k, line) in lines.iter().take(2).enumerate() {
            p.text(24.0, 144.0 + k as f32 * 26.0, line, px, p.c.text, UiTextAnchor::Start, false);
        }
    });

    // The scales, and Hold or Cancel.
    let scale_ids: Vec<String> = plan.scales.iter().map(|(id, _)| id.clone()).collect();
    let festival = plan.id.clone();
    let planned = plan.state == FestivalState::Planned;
    let look = UiStepLook {
        fill: c.accent,
        track: c.field,
        outline: Some(c.field_outline),
        segment_height: page.pt(46.0),
        selected_text: Some(c.on_accent),
        rounding: page.pt(5.0),
    };
    let scales = skin::step_selector(
        city_text("book.festivals.scale"),
        plan.scales.iter().map(|(_, name)| UiString::literal(name.clone())),
    )
    .with_style(UiStepSelectorStyle::Buttons)
    .with_look(look)
    .with_label_hidden(true)
    .with_text_colors(Some(c.text), Some(c.on_accent))
    .with_font_scale(page.scales.heading_font(19.0))
    .with_width(page.pt(width - 72.0 - HOLD_WIDTH))
    .with_current(plan.scale)
    .on_changed(move |selector, _, _| {
        let Some(scale) = scale_ids.get(selector.current()) else {
            return;
        };
        STATE.as_mut().scale = Some(scale.clone());
        // A planned festival takes the new scale.
        if planned {
            festivals::request(Request::Plan { festival: festival.clone(), scale: scale.clone() });
        }
    });
    let festival = plan.id.clone();
    let scale = plan.scales.get(plan.scale).map(|scale| scale.0.clone()).unwrap_or_default();
    let hold = skin::button(context, ButtonRole::Primary, plan.button.clone(), move |_, _| {
        if planned {
            festivals::request(Request::Cancel);
        } else {
            festivals::request(Request::Plan { festival: festival.clone(), scale: scale.clone() });
        }
    })
    .with_width(UiTextButtonWidth::Fixed(page.pt(HOLD_WIDTH)))
    .with_min_height(page.pt(52.0))
    .with_enabled(plan.enabled);
    let hold = page.scales.button_text(hold, 20.0);
    let mut controls = UiColumns::new().with_spacing(0.0).with_width(page.pt(width));
    controls.add_column(UiColumnWidth::Fixed(page.pt(24.0)), super::left_group());
    controls.add_column(UiColumnWidth::Fixed(page.pt(width - 72.0 - HOLD_WIDTH)), super::left_group().with_widget(scales));
    controls.add_column(UiColumnWidth::Fraction(1.0), super::left_group());
    controls.add_column(UiColumnWidth::Fixed(page.pt(HOLD_WIDTH)), super::left_group().with_widget(hold));
    controls.add_column(UiColumnWidth::Fixed(page.pt(24.0)), super::left_group());

    // The costs and the effects.
    let terms_height = (height - PLANNER_HEAD - CONTROLS).max(120.0);
    let value = json!({
        "costs": plan.costs.iter().map(|(what, need, held)| json!({ "what": what, "need": need, "held": held, "short": held < need })).collect::<Vec<_>>(),
        "effects": plan.effects.iter().map(|(what, by)| json!([what, by])).collect::<Vec<_>>(),
        "status": plan.status.as_ref().map(|(text, _)| text.clone()),
    });
    let (costs_title, effects_title) = (city_text("book.festivals.costs"), city_text("book.festivals.effects"));
    let held_key = city_text("book.festivals.held");
    let terms = body_canvas(page, width, terms_height, "terms", value, move |p: &Paint| {
        p.text(24.0, 30.0, &costs_title, 17.0, p.c.sub, UiTextAnchor::Start, true);
        p.text(width / 2.0 + 10.0, 30.0, &effects_title, 17.0, p.c.sub, UiTextAnchor::Start, true);
        for (k, (what, need, held)) in plan.costs.iter().enumerate() {
            let y = 60.0 + k as f32 * 28.0;
            let short = held < need;
            p.text(24.0, y, what, 18.0, p.c.text, UiTextAnchor::Start, false);
            let amount = if short {
                format!(
                    "{}  ·  {}",
                    number(i64::from(*need), false),
                    held_key.replace("{count}", &number(i64::from(*held), false))
                )
            } else {
                number(i64::from(*need), false)
            };
            p.text(width / 2.0 - 20.0, y, &amount, 18.0, if short { p.c.bad } else { p.c.text }, UiTextAnchor::End, true);
        }
        for (k, (what, by)) in plan.effects.iter().enumerate() {
            let y = 60.0 + k as f32 * 28.0;
            p.text(width / 2.0 + 10.0, y, what, 18.0, p.c.text, UiTextAnchor::Start, false);
            p.text(width - 24.0, y, by, 18.0, p.c.good, UiTextAnchor::End, by.starts_with('+'));
        }
        if let Some((status, warn)) = &plan.status {
            p.text(
                24.0,
                terms_height - 18.0,
                status,
                16.0,
                if *warn { p.c.bad } else { p.c.sub },
                UiTextAnchor::Start,
                false,
            );
        }
    });

    let contents = super::left_group()
        .with_widget_spacing(Vec2::zero())
        .with_widget(head)
        .with_widget(controls)
        .with_widget(super::spacer(page.pt(4.0)))
        .with_widget(terms);
    card_frame(page, contents, width, height)
}
