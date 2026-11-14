// The HUD's message log (docs/plans/realm-ui.md, Phase 5: P5d; hud-<realm>.jpg): the notices the
// game raised lately (a building completed, a fire or collapse, debt and its warnings), newest
// first, which the HUD shows at the bottom of the screen. A small store until M11's messages
// replace it with typed messages and their popups. Runtime only: a new or loaded game starts empty.

use std::collections::VecDeque;

use common::mem::SingleThreadStatic;

// How many messages the log keeps.
pub const CAPACITY: usize = 32;

// What a message is about: the HUD's icon for it.
#[derive(Copy, Clone, PartialEq, Eq, Debug, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    Construction, // A building completed.
    Warning,      // A fire, a collapse, an outbreak, debt.
    Finance,      // Good news about the treasury.
}

impl MessageKind {
    // The HUD's line icon (skin::hud_icon).
    pub fn icon(self) -> &'static str {
        match self {
            Self::Construction => "monument",
            Self::Warning => "warn",
            Self::Finance => "coin",
        }
    }
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct Message {
    pub kind: MessageKind,
    pub text: String,
    pub id: u64, // Counts up from the first message of the run.
}

struct MessageLog {
    messages: VecDeque<Message>,
    next_id: u64,
    dismissed: u64, // The newest message's id when the player closed the HUD's log: it shows again for a newer one.
}

static LOG: SingleThreadStatic<MessageLog> = SingleThreadStatic::new(MessageLog { messages: VecDeque::new(), next_id: 1, dismissed: 0 });

// A new message, the newest.
pub fn push(kind: MessageKind, text: impl Into<String>) {
    let log = LOG.as_mut();
    let id = log.next_id;
    log.next_id += 1;
    log.messages.push_front(Message { kind, text: text.into(), id });
    log.messages.truncate(CAPACITY);
}

// The latest `count` messages, newest first.
pub fn latest(count: usize) -> Vec<Message> {
    LOG.as_ref().messages.iter().take(count).cloned().collect()
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
}
