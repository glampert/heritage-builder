// The game's messages (docs/plans/messages.md, M11 Phase 0; the HUD's log, realm-ui Phase 5's P5d,
// hud-<realm>.jpg): the notices the game raised lately (a building completed, a fire or collapse,
// debt and its warnings, a patron's blessing or misfortune, the river, the harvests, trade and
// requests, the warnings that a mission may be lost), newest first. Each has a kind (its icon), a
// priority, the month it came in and, where it has one, a place: a cell or building on the map, a
// City book page or the world map, which the HUD's log lines take the player to
// (menu::in_game::message_log::open). An important event's message (priority Event, M11 Phase 1)
// carries its popup (EventPopup, built by `popups`), queued as it's posted while event popups are
// on (GameLoop::post_message) and opened by the in-game menus once no other dialog is open
// (dialog::open_event_popup). Runtime only: a new or loaded game starts empty, its queue too.

pub mod popups;

use std::collections::VecDeque;

use common::{coords::Cell, mem::SingleThreadStatic};
use serde_json::{Value, json};

use crate::building::BuildingKindAndId;

// How many messages the log keeps.
pub const CAPACITY: usize = 32;

// What a message is about: the HUD's icon for it.
#[derive(Copy, Clone, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize, strum::EnumIter)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    Construction, // A building completed.
    Warning,      // A fire, a collapse, an outbreak, debt.
    Finance,      // Good news about the treasury.
    Faith,        // A patron's blessing (a misfortune is a Warning).
    Season,       // The river: rising with the wet season, the water going down (docs/plans/seasons-floods.md).
    Harvest,      // The month's harvests reaped (a flood is a Warning).
    Trade,        // A caravan or ship traded (docs/plans/trade.md).
    Request,      // A request opened or was met (a failed one is a Warning).
}

impl MessageKind {
    // The HUD's line icon (skin::hud_icon).
    pub fn icon(self) -> &'static str {
        match self {
            Self::Construction => "monument",
            Self::Warning => "warn",
            Self::Finance => "coin",
            Self::Faith => "temple",
            Self::Season => "water",
            Self::Harvest => "food",
            Self::Trade => "trade",
            Self::Request => "request",
        }
    }

    // A message's priority unless it says otherwise.
    pub fn default_priority(self) -> MessagePriority {
        match self {
            Self::Construction | Self::Trade => MessagePriority::Info,
            Self::Warning => MessagePriority::Warning,
            Self::Finance | Self::Faith | Self::Season | Self::Harvest | Self::Request => MessagePriority::Notice,
        }
    }
}

// How much a message matters, least first.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, serde::Serialize, serde::Deserialize, strum::EnumIter)]
#[serde(rename_all = "snake_case")]
pub enum MessagePriority {
    Info,    // A building done, a caravan.
    Notice,  // A harvest, a festival held, the river rising, a request.
    Warning, // A fire, debt, a breach.
    Event,   // An important event: its popup (M11 Phase 1).
}

// Where a message takes the player.
#[derive(Clone, PartialEq)]
pub enum MessageTarget {
    // A cell on the map: the camera goes there.
    Cell(Cell),
    // A building, and its base cell when the message came: the camera goes to it, or to that cell
    // once it's gone.
    Building(BuildingKindAndId, Cell),
    // A City book page, by its id ("goods").
    CityPage(&'static str),
    // The world map, with a city picked if one is given (a trade partner).
    WorldMap(Option<String>),
}

impl MessageTarget {
    pub fn to_json(&self) -> Value {
        match self {
            Self::Cell(cell) => json!({ "kind": "cell", "cell": [cell.x, cell.y] }),
            Self::Building(building, cell) => json!({
                "kind": "building",
                "building": building.kind.to_string(),
                "cell": [cell.x, cell.y],
            }),
            Self::CityPage(page) => json!({ "kind": "page", "page": page }),
            Self::WorldMap(city) => json!({ "kind": "world_map", "city": city }),
        }
    }
}

// An important event's popup (docs/plans/messages.md, §2; event-<realm>.jpg): a kicker ("Flood"),
// a title, a paragraph, up to MAX_EVENT_LINES lines of what it brings, each with a line icon, and
// the icon on its picture's disc. Its button to the event's place is its message's target's.
#[derive(Clone, PartialEq, Debug)]
pub struct EventPopup {
    pub kicker: String,
    pub title: String,
    pub text: String,
    pub icon: String, // A line icon's name (popups::LINE_ICONS).
    pub lines: Vec<EventLine>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct EventLine {
    pub icon: String,
    pub text: String,
}

pub const MAX_EVENT_LINES: usize = 3;

impl EventPopup {
    pub fn to_json(&self) -> Value {
        json!({
            "kicker": self.kicker,
            "title": self.title,
            "text": self.text,
            "icon": self.icon,
            "lines": self.lines.iter().map(|line| json!({ "icon": line.icon, "text": line.text })).collect::<Vec<_>>(),
        })
    }
}

#[derive(Clone)]
pub struct Message {
    pub id: u64, // Counts up from the first message of the run; 0 until it's posted.
    pub kind: MessageKind,
    pub priority: MessagePriority,
    pub text: String,
    pub target: Option<MessageTarget>,
    pub month: u32, // The ordinal month it came in (GameCalendar::ordinal_month).
    pub popup: Option<Box<EventPopup>>,
}

impl Message {
    // A message of `kind` at its default priority, with no place.
    pub fn new(kind: MessageKind, text: impl Into<String>) -> Self {
        Self { id: 0, kind, priority: kind.default_priority(), text: text.into(), target: None, month: 0, popup: None }
    }

    pub fn with_priority(mut self, priority: MessagePriority) -> Self {
        self.priority = priority;
        self
    }

    pub fn with_target(mut self, target: impl Into<Option<MessageTarget>>) -> Self {
        self.target = target.into();
        self
    }

    // An important event's message, with its popup, at priority Event; None changes nothing.
    pub fn with_popup(mut self, popup: Option<EventPopup>) -> Self {
        if let Some(mut popup) = popup {
            popup.lines.truncate(MAX_EVENT_LINES);
            self.popup = Some(Box::new(popup));
            self.priority = MessagePriority::Event;
        }
        self
    }

    // What messages.state reports of it.
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "kind": self.kind,
            "priority": self.priority,
            "text": self.text,
            "target": self.target.as_ref().map(MessageTarget::to_json),
            "month": self.month,
            "popup": self.popup.as_ref().map(|popup| popup.to_json()),
        })
    }
}

struct MessageLog {
    messages: VecDeque<Message>,
    next_id: u64,
    dismissed: u64, // The newest message's id when the player closed the HUD's log: it shows again for a newer one.
    popups: VecDeque<Message>, // Events whose popups wait to open, oldest first.
}

static LOG: SingleThreadStatic<MessageLog> =
    SingleThreadStatic::new(MessageLog { messages: VecDeque::new(), next_id: 1, dismissed: 0, popups: VecDeque::new() });

// A new message, the newest, raised in the ordinal month `month`. Returns its id.
pub fn post(mut message: Message, month: u32) -> u64 {
    let log = LOG.as_mut();
    message.id = log.next_id;
    message.month = month;
    log.next_id += 1;
    log.messages.push_front(message);
    log.messages.truncate(CAPACITY);
    log.next_id - 1
}

// Queues the popup of the message `id` (posted, with a popup) to open once nothing else is open.
pub fn queue_popup(id: u64) {
    let log = LOG.as_mut();
    if let Some(message) = log.messages.iter().find(|message| message.id == id && message.popup.is_some()) {
        let message = message.clone();
        log.popups.push_back(message);
    }
}

// The oldest popup waiting to open, taken from the queue.
pub fn take_popup() -> Option<Message> {
    LOG.as_mut().popups.pop_front()
}

// Drops the popups waiting to open: event popups turned off in Settings.
pub fn clear_popups() {
    LOG.as_mut().popups.clear();
}

// Whether a popup waits to open.
pub fn has_popup() -> bool {
    !LOG.as_ref().popups.is_empty()
}

// The ids of the messages whose popups wait to open, oldest first.
pub fn queued_popups() -> Vec<u64> {
    LOG.as_ref().popups.iter().map(|message| message.id).collect()
}

// The latest `count` messages, newest first.
pub fn latest(count: usize) -> Vec<Message> {
    LOG.as_ref().messages.iter().take(count).cloned().collect()
}

// The message with that id, while the log keeps it.
pub fn find(id: u64) -> Option<Message> {
    LOG.as_ref().messages.iter().find(|message| message.id == id).cloned()
}

// The newest message's id, 0 when there's none: the HUD watches it to draw again.
pub fn newest_id() -> u64 {
    LOG.as_ref().messages.front().map_or(0, |message| message.id)
}

// The player closed the HUD's log: hidden until a newer message comes.
pub fn dismiss() {
    let log = LOG.as_mut();
    log.dismissed = log.messages.front().map_or(0, |message| message.id);
}

// Whether the HUD's log was closed since the newest message came.
pub fn is_dismissed() -> bool {
    let log = LOG.as_ref();
    log.messages.front().is_none_or(|message| message.id <= log.dismissed)
}

// Empties the log: a new game, a load, the home menu.
pub fn clear() {
    let log = LOG.as_mut();
    log.messages.clear();
    log.dismissed = 0;
    log.popups.clear();
}
