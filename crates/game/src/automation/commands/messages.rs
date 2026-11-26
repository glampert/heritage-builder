// The game's messages (crate::messages, docs/plans/messages.md, M11 Phase 0; the HUD's log,
// docs/plans/realm-ui.md, P5d): what the log holds and the event popups waiting to open (M11
// Phase 1), a message added by hand for tests, and a message's place or popup opened as a click on
// its line opens it.

use common::coords::Cell;
use serde::Deserialize;
use serde_json::{Value, json};
use strum::IntoEnumIterator;

use crate::{
    automation::{CommandContext, CommandDef, CommandError, CommandResult, ParamDef, done, parse_params},
    config::GameConfigs,
    menu::{dialog, in_game::message_log},
    messages::{self, EventLine, EventPopup, MAX_EVENT_LINES, Message, MessageKind, MessagePriority, MessageTarget, popups},
};

pub(super) const COMMANDS: &[CommandDef] = &[
    CommandDef {
        name: "messages.state",
        help: "The message log's messages, newest first (id; kind: construction, warning, finance, faith, season, harvest, \
               trade or request; priority: info, notice, warning or event; text; target: null or {kind: cell, cell}, \
               {kind: building, building, cell}, {kind: page, page}, {kind: world_map, city}; month, the ordinal month \
               it came in; popup: null or an important event's {kicker, title, text, icon, lines: [{icon, text}]}), \
               count, queue (the ids of the messages whose popups wait to open, oldest first) and shown (the id of the \
               message whose popup is open, or null). A new or loaded game starts it empty.",
        params: &[],
        run: cmd_state,
    },
    CommandDef {
        name: "messages.push",
        help: "Adds a message to the log, the newest, as the game's notices do (no sound). Its priority is its kind's \
               unless given; with a popup it's an important event's (priority event), its popup queued to open while \
               event popups are on.",
        params: &[
            ParamDef {
                name: "kind",
                kind: "string",
                required: true,
                help: "construction, warning, finance, faith, season, harvest, trade or request.",
            },
            ParamDef { name: "text", kind: "string", required: true, help: "The message's text." },
            ParamDef { name: "priority", kind: "string", required: false, help: "info, notice, warning or event." },
            ParamDef {
                name: "target",
                kind: "object",
                required: false,
                help: "Its place: {kind: cell, cell: [x, y]}, {kind: building, cell: [x, y]} (the building there), \
                       {kind: page, page: overview|population|goods|finances|ratings|heritage}, or {kind: world_map, \
                       city: <id>} (city optional).",
            },
            ParamDef {
                name: "popup",
                kind: "object",
                required: false,
                help: "An important event's popup: {kicker, title, text, icon, lines: [[icon, text], ...]} (icon: a \
                       line icon, messages::popups::LINE_ICONS; at most 3 lines).",
            },
        ],
        run: cmd_push,
    },
    CommandDef {
        name: "messages.open",
        help: "As a click on a message's line in the HUD's log: an important event's popup opens again; any other \
               message takes the player to its place: the camera to its cell or building, the City book on its page, or \
               the world map. Reports whether it went.",
        params: &[ParamDef { name: "id", kind: "integer", required: true, help: "The message's id (messages.state)." }],
        run: cmd_open,
    },
];

fn cmd_state(_context: &mut CommandContext, _params: Value) -> CommandResult {
    let latest = messages::latest(messages::CAPACITY);
    let listed: Vec<Value> = latest.iter().map(Message::to_json).collect();
    let shown = dialog::event_popup_shown().map(|message| message.id);
    done(json!({ "count": latest.len(), "messages": listed, "queue": messages::queued_popups(), "shown": shown }))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PushParams {
    kind: MessageKind,
    text: String,
    #[serde(default)]
    priority: Option<MessagePriority>,
    #[serde(default)]
    target: Option<TargetParams>,
    #[serde(default)]
    popup: Option<PopupParams>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PopupParams {
    kicker: String,
    title: String,
    #[serde(default)]
    text: String,
    icon: String,
    #[serde(default)]
    lines: Vec<(String, String)>,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum TargetParams {
    Cell {
        cell: [i32; 2],
    },
    Building {
        cell: [i32; 2],
    },
    Page {
        page: String,
    },
    WorldMap {
        #[serde(default)]
        city: Option<String>,
    },
}

fn names<T: serde::Serialize>(values: impl Iterator<Item = T>) -> String {
    values
        .map(|value| serde_json::to_value(value).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default())
        .collect::<Vec<_>>()
        .join(", ")
}

fn cmd_push(context: &mut CommandContext, params: Value) -> CommandResult {
    // Named kinds and priorities, listed when one is unknown.
    let params: PushParams = parse_params(params).map_err(|error| {
        CommandError::invalid_params(format!(
            "{} (kinds: {}; priorities: {})",
            error.message,
            names(MessageKind::iter()),
            names(MessagePriority::iter())
        ))
    })?;
    let target = match params.target {
        None => None,
        Some(TargetParams::Cell { cell }) => Some(MessageTarget::Cell(map_cell(context, cell)?)),
        Some(TargetParams::Building { cell }) => {
            let cell = map_cell(context, cell)?;
            let building = context.session.with_sim_context(0.0, |sim| {
                sim.find_building_for_cell(cell).map(|building| (building.kind_and_id(), building.base_cell()))
            });
            let Some((building, base_cell)) = building else {
                return Err(CommandError::invalid_params(format!("No building at {cell}.")));
            };
            Some(MessageTarget::Building(building, base_cell))
        }
        Some(TargetParams::Page { page }) => {
            let Some(id) = dialog::city_book::page_ids().find(|id| *id == page) else {
                return Err(CommandError::invalid_params(format!(
                    "Unknown page '{page}': {}.",
                    dialog::city_book::page_ids().collect::<Vec<_>>().join(", ")
                )));
            };
            Some(MessageTarget::CityPage(id))
        }
        Some(TargetParams::WorldMap { city }) => Some(MessageTarget::WorldMap(city)),
    };
    let popup = match params.popup {
        None => None,
        Some(popup) => {
            let icons = std::iter::once(&popup.icon).chain(popup.lines.iter().map(|(icon, _)| icon));
            if let Some(icon) = icons.into_iter().find(|icon| !popups::LINE_ICONS.contains(&icon.as_str())) {
                return Err(CommandError::invalid_params(format!(
                    "Unknown icon '{icon}': {}.",
                    popups::LINE_ICONS.join(", ")
                )));
            }
            if popup.lines.len() > MAX_EVENT_LINES {
                return Err(CommandError::invalid_params(format!("A popup has at most {MAX_EVENT_LINES} lines.")));
            }
            Some(EventPopup {
                kicker: popup.kicker,
                title: popup.title,
                text: popup.text,
                icon: popup.icon,
                lines: popup.lines.into_iter().map(|(icon, text)| EventLine { icon, text }).collect(),
            })
        }
    };
    let has_popup = popup.is_some();
    let mut message = Message::new(params.kind, params.text).with_target(target).with_popup(popup);
    if let Some(priority) = params.priority {
        message = message.with_priority(priority);
    }
    let month = context.session.sim().calendar().ordinal_month();
    let id = messages::post(message, month);
    if has_popup && GameConfigs::get().display.event_popups {
        messages::queue_popup(id);
    }
    done(json!({ "id": id, "count": messages::latest(messages::CAPACITY).len(), "queue": messages::queued_popups() }))
}

fn map_cell(context: &mut CommandContext, cell: [i32; 2]) -> Result<Cell, CommandError> {
    let cell = Cell::new(cell[0], cell[1]);
    if !context.session.tile_map().is_cell_within_bounds(cell) {
        return Err(CommandError::invalid_params(format!("{cell} is off the map.")));
    }
    Ok(cell)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OpenParams {
    id: u64,
}

fn cmd_open(context: &mut CommandContext, params: Value) -> CommandResult {
    let params: OpenParams = parse_params(params)?;
    let Some(message) = messages::find(params.id) else {
        return Err(CommandError::not_found(format!("No message {} in the log.", params.id)));
    };
    if message.target.is_none() && message.popup.is_none() {
        return Err(CommandError::failed(format!("Message {} has no place to go to nor a popup.", params.id)));
    }
    let opened = context.session.with_ui_context(context.engine, |ui_context| message_log::open(&message, ui_context));
    if !opened {
        return Err(CommandError::failed(format!("Message {}'s place couldn't be shown.", params.id)));
    }
    done(json!({
        "opened": true,
        "popup": message.popup.is_some(),
        "target": message.target.as_ref().map(MessageTarget::to_json),
    }))
}
