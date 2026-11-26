// The important events' popups (docs/plans/messages.md, §2, M11 Phase 1; event-<realm>.jpg): what
// each one says, built from its notice when GameLoop posts its message (Message::with_popup). The
// events (decision Q3): the river's flood (its breaches in its paragraph), a request opened and one
// failed, a patron's blessing or misfortune, an outbreak starting, and the last warnings before a
// game is lost: Legitimacy's clock starting and its last month, a time limit's last 3 months and
// its last month, and the debt's (while the game can be lost to it) 3 months and 1 month before
// it's overdue. Fires and collapses stay log lines.
//
// Their words are Messages keys (`event.*`), in each realm's words where the game has them (the
// fourth rating's name, a patron's, the river's).

use engine::ui::text::{self, UiTextCategory};

use super::{EventLine, EventPopup};
use crate::{
    building::risks::{RiskNotice, RiskNoticeKind},
    config::GameConfigs,
    faith::config::{self as faith_config, FaithConfigs, FaithEffectDef},
    floods::FloodNotice,
    ratings::{failure::FailureNotice, rating_name},
    realm::Realm,
    sim::{
        faith::{FaithNotice, FaithNoticeKind},
        finance::{DebtNotice, DebtNoticeKind},
        ratings::RatingKind,
    },
    trade::{TradeNotice, requests::RequestEvent},
};

// The line icons a popup can show: the HUD's line icons (skin::HUD_ICONS), drawn as they are, and
// the City book's page icons (skin::CITY_ICONS), tinted with the accent.
pub const LINE_ICONS: &[&str] = &[
    "population",
    "coin",
    "food",
    "request",
    "trade",
    "warn",
    "monument",
    "temple",
    "water",
    "learning",
    "jar",
    "star",
    "industry",
    "farm",
    "house",
];

fn get(key: &str) -> String {
    text::get(UiTextCategory::Messages, key).to_string()
}

fn format(key: &str, args: &[(&str, &str)]) -> String {
    text::format(UiTextCategory::Messages, key, args)
}

fn format_count(key: &str, count: u32, args: &[(&str, &str)]) -> String {
    text::format_count(UiTextCategory::Messages, key, i64::from(count), args)
}

// "2 months".
fn months_text(months: u32) -> String {
    format_count("failure.months", months, &[])
}

fn line(icon: &str, text: String) -> EventLine {
    EventLine { icon: icon.to_string(), text }
}

fn popup(kicker: String, title: String, text: String, icon: &str, lines: Vec<EventLine>) -> EventPopup {
    EventPopup { kicker, title, text, icon: icon.to_string(), lines }
}

// ----------------------------------------------
// The river
// ----------------------------------------------

// The flood: how much land it covered and whether the dykes held (`dykes`: the city has any), the
// fields under water and for how long (`months`), the buildings damaged and the silt. The other
// river notices (rising, breaches, silt let in, the water going) are log lines.
pub fn flood(notice: &FloodNotice, river: &str, months: u32, dykes: bool) -> Option<EventPopup> {
    let FloodNotice::Flood { cells, buildings_damaged, fields_flooded, breaches, .. } = *notice else {
        return None;
    };
    let river = crate::floods::sentence_start(river);
    let mut paragraph = format("event.flood.text", &[("cells", &format_count("event.cells", cells, &[]))]);
    if breaches > 0 {
        paragraph = format!("{paragraph} {}", format_count("event.flood.breached", breaches, &[]));
    } else if dykes {
        paragraph = format!("{paragraph} {}", get("event.flood.held"));
    }
    let months = months_text(months);
    let water = if fields_flooded > 0 {
        line("warn", format_count("event.flood.fields", fields_flooded, &[("months", &months)]))
    } else {
        line("water", format("event.flood.water", &[("months", &months)]))
    };
    let damage = if buildings_damaged > 0 {
        line("house", format_count("event.flood.damaged", buildings_damaged, &[]))
    } else {
        line("house", get("event.flood.undamaged"))
    };
    Some(popup(get("event.kicker.flood"), format("event.flood.title", &[("river", &river)]), paragraph, "water", vec![
        water,
        damage,
        line("farm", get("event.flood.silt")),
    ]))
}

// ----------------------------------------------
// Requests
// ----------------------------------------------

// A request opened (what's asked and by when, what meeting it brings, what lateness costs) or
// failed (what it cost, and the court asking again); `city` names its asker. A request met is a log
// line.
pub fn request(notice: &TradeNotice, city: &str, realm: Realm) -> Option<EventPopup> {
    let TradeNotice::Request { event, request, court } = notice else {
        return None;
    };
    let good = request.good.display_name().to_lowercase();
    let goods = crate::trade::goods_text(&[(request.good, request.amount)]);
    let rating = rating_name(realm, RatingKind::Legitimacy);
    let rules = &GameConfigs::get().sim.ratings;
    let lost = (request.legitimacy as f32 * rules.request_failed_scale).round() as i32;
    match event {
        RequestEvent::Opened => {
            let months = months_text(request.due.saturating_sub(request.opened));
            let (kicker, title, text) = if *court {
                let text_key = if request.again > 0 { "event.request.text_court_again" } else { "event.request.text_court" };
                (
                    get("event.kicker.request_court"),
                    format("event.request.title_court", &[("good", &good)]),
                    format(text_key, &[("goods", &goods)]),
                )
            } else {
                (
                    format("event.kicker.request", &[("city", city)]),
                    format("event.request.title", &[("city", city), ("good", &good)]),
                    format("event.request.text", &[("city", city), ("goods", &goods)]),
                )
            };
            let mut lines = vec![line("request", format("event.request.send", &[("goods", &goods), ("months", &months)]))];
            if request.legitimacy > 0 {
                let met = format!("+{}", request.legitimacy);
                lines.push(line("star", format("event.request.met", &[("rating", &rating), ("change", &met)])));
                let late = format!("−{lost}");
                lines.push(line("warn", format("event.request.late", &[("rating", &rating), ("change", &late)])));
            }
            Some(popup(kicker, title, text, "request", lines))
        }
        RequestEvent::Failed => {
            let title = if *court {
                get("event.request.failed.title_court")
            } else {
                format("event.request.failed.title", &[("city", city)])
            };
            let text = crate::trade::requests::notice_text(RequestEvent::Failed, request, *court, city);
            let mut lines = Vec::new();
            if lost > 0 {
                let change = format!("−{lost}");
                lines.push(line("warn", format("event.request.lost", &[("rating", &rating), ("change", &change)])));
            }
            if *court {
                let amount = (request.amount as f32 * rules.ask_again_scale).ceil() as u32;
                let again = crate::trade::goods_text(&[(request.good, amount)]);
                let months = months_text(rules.ask_again_months);
                lines.push(line("request", format("event.request.again", &[("goods", &again), ("months", &months)])));
            }
            Some(popup(get("event.kicker.request_failed"), title, text, "request", lines))
        }
        RequestEvent::Met => None,
    }
}

// ----------------------------------------------
// Faith
// ----------------------------------------------

// A patron's blessing or misfortune: its name, the message, and what it does and for how long. A
// festival held or not is a log line.
pub fn faith(notice: &FaithNotice, realm: Realm) -> Option<EventPopup> {
    let kicker = match notice.kind {
        FaithNoticeKind::Blessing => get("event.kicker.blessing"),
        FaithNoticeKind::Misfortune => get("event.kicker.misfortune"),
        _ => return None,
    };
    let title = text::get(UiTextCategory::Realm, &faith_config::event_name_key(realm, &notice.id)).to_string();
    let lines = FaithConfigs::get_or_load()
        .for_realm(realm)
        .and_then(|def| def.event(&notice.id))
        .map(|event| effect_lines(&event.effect))
        .unwrap_or_default();
    Some(popup(kicker, title, notice.text.clone(), "temple", lines))
}

// What a blessing or misfortune does, as a popup's lines.
pub fn effect_lines(effect: &FaithEffectDef) -> Vec<EventLine> {
    let percent = |scale: f32| ((scale - 1.0).abs() * 100.0).round().to_string();
    match *effect {
        FaithEffectDef::Harvest { scale, months, .. } => {
            let key = if scale >= 1.0 { "event.faith.harvest_more" } else { "event.faith.harvest_less" };
            vec![line("farm", format(key, &[("percent", &percent(scale)), ("months", &months_text(months))]))]
        }
        FaithEffectDef::Windfall { gold } => {
            vec![line("coin", format("event.faith.windfall", &[("gold", &gold.to_string())]))]
        }
        FaithEffectDef::Fire { count } => vec![line("warn", format_count("event.faith.fire", count, &[]))],
        FaithEffectDef::Sickness { count } => vec![line("warn", format_count("event.faith.sickness", count, &[]))],
        FaithEffectDef::Collapse { count } => vec![line("warn", format_count("event.faith.collapse", count, &[]))],
        FaithEffectDef::Upkeep { scale, months } => {
            let key = if scale >= 1.0 { "event.faith.upkeep_more" } else { "event.faith.upkeep_less" };
            vec![line("coin", format(key, &[("percent", &percent(scale)), ("months", &months_text(months))]))]
        }
        FaithEffectDef::Flood { severity } => {
            let key = if severity >= 1.0 { "event.faith.flood_higher" } else { "event.faith.flood_gentler" };
            vec![line("water", get(key))]
        }
        FaithEffectDef::Envoys { goods } => vec![line("trade", format_count("event.faith.envoys", goods, &[]))],
        FaithEffectDef::TradeWindfall { scale, months } => {
            let percent = (scale.abs() * 100.0).round().to_string();
            vec![line("trade", format("event.faith.trade", &[("percent", &percent), ("months", &months_text(months))]))]
        }
        FaithEffectDef::Scholars { .. } | FaithEffectDef::Novices => Vec::new(),
    }
}

// ----------------------------------------------
// Sickness
// ----------------------------------------------

// An outbreak starting: when its people start to die, that it spreads, and what cures it. Fires,
// collapses and an outbreak's end are log lines.
pub fn outbreak(notice: &RiskNotice) -> Option<EventPopup> {
    if notice.kind != RiskNoticeKind::Outbreak {
        return None;
    }
    let grace = crate::inspect::duration_text(GameConfigs::get().sim.risks.death_grace_secs);
    Some(popup(get("event.kicker.sickness"), get("event.sickness.title"), notice.text.clone(), "warn", vec![
        line("warn", format("event.sickness.deaths", &[("span", &grace)])),
        line("house", get("event.sickness.spreads")),
        line("population", get("event.sickness.cure")),
    ]))
}

// ----------------------------------------------
// Failure
// ----------------------------------------------

// What a failure warning's popup lists besides its notice: the fourth rating's lowest drivers
// (their names and values) and the game's goals not met yet.
#[derive(Clone, Default)]
pub struct FailureContext {
    pub drivers: Vec<(String, i32)>,
    pub unmet: Vec<UnmetGoal>,
}

// A goal not met yet: its text, what's achieved and its target, as the player reads them.
#[derive(Clone)]
pub struct UnmetGoal {
    pub goal: String,
    pub achieved: String,
    pub target: String,
}

// The last warnings before the game is lost: Legitimacy's clock starting and its last month (its
// lowest drivers as lines: their names and values), a time limit's last 3 months and last month
// (the goals not met yet). `text` is the warning's message.
pub fn failure(notice: &FailureNotice, text: &str, realm: Realm, context: &FailureContext) -> Option<EventPopup> {
    match *notice {
        FailureNotice::LegitimacyLow { months_left, started } => {
            if !started && months_left != 1 {
                return None;
            }
            let rating = rating_name(realm, RatingKind::Legitimacy);
            let title = if started {
                format("event.legitimacy.title", &[("rating", &rating)])
            } else {
                format("event.legitimacy.last.title", &[("rating", &rating)])
            };
            let lines = context
                .drivers
                .iter()
                .map(|(name, value)| {
                    let icon = if *value < 0 { "warn" } else { "star" };
                    let value = if *value < 0 { format!("−{}", -value) } else { format!("+{value}") };
                    line(icon, format("event.driver", &[("driver", name), ("value", &value)]))
                })
                .collect();
            Some(popup(get("event.kicker.warning"), title, text.to_string(), "star", lines))
        }
        FailureNotice::TimeLeft { months_left } => {
            if months_left != 3 && months_left != 1 {
                return None;
            }
            let lines = context
                .unmet
                .iter()
                .map(|unmet| {
                    let args = [("goal", unmet.goal.as_str()), ("achieved", &unmet.achieved), ("target", &unmet.target)];
                    line("warn", format("event.time.goal", &args))
                })
                .collect();
            Some(popup(
                get("event.kicker.warning"),
                format_count("event.time.title", months_left, &[]),
                text.to_string(),
                "warn",
                lines,
            ))
        }
    }
}

// The debt's warnings 3 months and 1 month before it's overdue, while the game can be lost to it
// (`can_lose`): the treasury's balance, and what happens then.
pub fn debt(notice: &DebtNotice, balance: i64, can_lose: bool) -> Option<EventPopup> {
    let DebtNoticeKind::Warning { months_left } = notice.kind else {
        return None;
    };
    if !can_lose || (months_left != 3 && months_left != 1) {
        return None;
    }
    let months = months_text(months_left);
    Some(popup(
        get("event.kicker.debt"),
        format_count("event.debt.title", months_left, &[]),
        notice.text.clone(),
        "coin",
        vec![
            line("coin", format("event.debt.balance", &[("balance", &balance.to_string())])),
            line("warn", format("event.debt.lost", &[("months", &months)])),
        ],
    ))
}
