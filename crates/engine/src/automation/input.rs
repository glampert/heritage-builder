// Simulated input for automation: gestures (clicks, drags, key presses, typing, scrolling) that
// play out over several frames on a virtual mouse and keyboard, the way a player's input would.
//
// While input capture is on (Engine::set_input_capture), the InputSystem reports the virtual
// state instead of the real devices, real input events are dropped (except quitting and window
// resizes), and cursor confinement stops moving the real cursor.
//
// Mouse state is polled once per frame, so a press and release on the same frame would never be
// seen: a click is a move, a press on the next frame and a release on the frame after. The
// timeline applies one frame's steps at the start of Engine::begin_frame, before events are
// polled and before the UI reads the input.

use std::collections::VecDeque;

use common::Vec2;

use crate::app::{
    ApplicationEvent,
    ApplicationEventList,
    input::{InputAction, InputKey, MouseButton, VirtualInput},
};

pub type GestureId = u64;

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum InputStep {
    MoveCursor(Vec2),
    MouseButton(MouseButton, bool), // Pressed?
    Key(InputKey, bool),            // Pressed?
    Char(char),
    Scroll(Vec2),
    EndFrame, // The steps after this wait for the next frame.
}

// ----------------------------------------------
// Gestures
// ----------------------------------------------

pub fn move_gesture(point: Vec2) -> Vec<InputStep> {
    vec![InputStep::MoveCursor(point), InputStep::EndFrame]
}

// Hover, press on the next frame, release on the frame after.
pub fn click_gesture(point: Vec2, button: MouseButton) -> Vec<InputStep> {
    vec![
        InputStep::MoveCursor(point),
        InputStep::EndFrame,
        InputStep::MouseButton(button, true),
        InputStep::EndFrame,
        InputStep::MouseButton(button, false),
        InputStep::EndFrame,
    ]
}

// Press at `from`, move to `to` in `frames` even steps, release there.
pub fn drag_gesture(from: Vec2, to: Vec2, button: MouseButton, frames: u32) -> Vec<InputStep> {
    let frames = frames.max(1);
    let mut steps = vec![
        InputStep::MoveCursor(from),
        InputStep::EndFrame,
        InputStep::MouseButton(button, true),
        InputStep::EndFrame,
    ];

    for frame in 1..=frames {
        let t = frame as f32 / frames as f32;
        steps.push(InputStep::MoveCursor(from + (to - from) * t));
        steps.push(InputStep::EndFrame);
    }

    steps.push(InputStep::MouseButton(button, false));
    steps.push(InputStep::EndFrame);
    steps
}

// Holds the modifier keys, presses and releases `key`, then lets go of the modifiers.
pub fn key_gesture(key: InputKey, modifiers: &[InputKey]) -> Vec<InputStep> {
    let mut steps: Vec<InputStep> = modifiers.iter().map(|modifier| InputStep::Key(*modifier, true)).collect();
    steps.push(InputStep::Key(key, true));
    steps.push(InputStep::EndFrame);
    steps.push(InputStep::Key(key, false));
    steps.extend(modifiers.iter().rev().map(|modifier| InputStep::Key(*modifier, false)));
    steps.push(InputStep::EndFrame);
    steps
}

pub fn type_gesture(text: &str) -> Vec<InputStep> {
    let mut steps: Vec<InputStep> = text.chars().map(InputStep::Char).collect();
    steps.push(InputStep::EndFrame);
    steps
}

pub fn scroll_gesture(point: Vec2, amount: Vec2) -> Vec<InputStep> {
    vec![InputStep::MoveCursor(point), InputStep::EndFrame, InputStep::Scroll(amount), InputStep::EndFrame]
}

// ----------------------------------------------
// InputTimeline
// ----------------------------------------------

enum TimelineEntry {
    Step(InputStep),
    GestureDone(GestureId),
}

// Owned by the Engine.
#[derive(Default)]
pub struct InputTimeline {
    entries: VecDeque<TimelineEntry>,
    last_gesture_id: GestureId,
    last_done_id: GestureId, // Gestures finish in order.
}

impl InputTimeline {
    // A gesture is done once the frame after its last step has started: the game handles input
    // during the frame it arrives, after automation commands run.
    pub fn push_gesture(&mut self, steps: Vec<InputStep>) -> GestureId {
        self.last_gesture_id += 1;
        self.entries.extend(steps.into_iter().map(TimelineEntry::Step));
        self.entries.push_back(TimelineEntry::Step(InputStep::EndFrame));
        self.entries.push_back(TimelineEntry::GestureDone(self.last_gesture_id));
        self.last_gesture_id
    }

    #[inline]
    pub fn is_gesture_done(&self, id: GestureId) -> bool {
        id <= self.last_done_id
    }

    #[inline]
    pub fn is_idle(&self) -> bool {
        self.entries.is_empty()
    }

    // Drops the steps not applied yet. Their gestures count as done.
    pub fn cancel(&mut self) {
        self.entries.clear();
        self.last_done_id = self.last_gesture_id;
    }

    // Applies the steps up to the next EndFrame to `input`, and returns the events they make.
    pub fn apply_frame(&mut self, input: &mut VirtualInput) -> ApplicationEventList {
        let mut events = ApplicationEventList::new();

        while let Some(entry) = self.entries.pop_front() {
            let step = match entry {
                TimelineEntry::GestureDone(id) => {
                    self.last_done_id = id;
                    continue;
                }
                TimelineEntry::Step(step) => step,
            };

            match step {
                InputStep::MoveCursor(point) => input.cursor_pos = point,
                InputStep::MouseButton(button, pressed) => {
                    input.set_mouse_button(button, pressed);
                    events.push(ApplicationEvent::MouseButton(button, InputAction::from_pressed(pressed), input.modifiers()));
                }
                InputStep::Key(key, pressed) => {
                    input.set_key(key, pressed);
                    events.push(ApplicationEvent::KeyInput(key, InputAction::from_pressed(pressed), input.modifiers()));
                }
                InputStep::Char(c) => events.push(ApplicationEvent::CharInput(c)),
                InputStep::Scroll(amount) => events.push(ApplicationEvent::Scroll(amount)),
                InputStep::EndFrame => {
                    // A gesture that ends here is done once its last frame is applied.
                    while let Some(TimelineEntry::GestureDone(id)) = self.entries.front() {
                        self.last_done_id = *id;
                        self.entries.pop_front();
                    }
                    break;
                }
            }
        }

        events
    }
}

// Modifier names for key gestures: shift, ctrl/control, alt/option, super/cmd/command.
pub fn modifier_key(name: &str) -> Option<InputKey> {
    match name.to_ascii_lowercase().as_str() {
        "shift" => Some(InputKey::LeftShift),
        "ctrl" | "control" => Some(InputKey::LeftControl),
        "alt" | "option" => Some(InputKey::LeftAlt),
        "super" | "cmd" | "command" => Some(InputKey::LeftSuper),
        _ => None,
    }
}

// ----------------------------------------------
// Unit Tests
// ----------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::input::InputModifiers;

    #[test]
    fn test_click_spans_three_frames() {
        let mut timeline = InputTimeline::default();
        let mut input = VirtualInput::new();
        let id = timeline.push_gesture(click_gesture(Vec2::new(10.0, 20.0), MouseButton::Left));

        // Frame 1: the cursor moves, nothing is pressed.
        assert!(timeline.apply_frame(&mut input).is_empty());
        assert_eq!(input.cursor_pos, Vec2::new(10.0, 20.0));
        assert!(!input.is_mouse_button_pressed(MouseButton::Left));
        assert!(!timeline.is_gesture_done(id));

        // Frame 2: press.
        let events = timeline.apply_frame(&mut input);
        assert!(matches!(events[..], [ApplicationEvent::MouseButton(MouseButton::Left, InputAction::Press, _)]));
        assert!(input.is_mouse_button_pressed(MouseButton::Left));

        // Frame 3: release.
        let events = timeline.apply_frame(&mut input);
        assert!(matches!(events[..], [ApplicationEvent::MouseButton(MouseButton::Left, InputAction::Release, _)]));
        assert!(!timeline.is_gesture_done(id));

        // Frame 4: the game has handled the release; the gesture is done.
        assert!(timeline.apply_frame(&mut input).is_empty());
        assert!(timeline.is_gesture_done(id));
        assert!(timeline.is_idle());
    }

    #[test]
    fn test_key_gesture_reports_modifiers() {
        let mut timeline = InputTimeline::default();
        let mut input = VirtualInput::new();
        timeline.push_gesture(key_gesture(InputKey::Z, &[InputKey::LeftControl]));

        let events = timeline.apply_frame(&mut input);
        let ApplicationEvent::KeyInput(InputKey::Z, InputAction::Press, modifiers) = events[1] else {
            panic!("Expected Z pressed second, got {events:?}");
        };
        assert!(modifiers.intersects(InputModifiers::Control));

        timeline.apply_frame(&mut input);
        assert!(!input.is_key_pressed(InputKey::Z) && !input.is_key_pressed(InputKey::LeftControl));
    }

    #[test]
    fn test_drag_and_cancel() {
        let mut timeline = InputTimeline::default();
        let mut input = VirtualInput::new();
        let id = timeline.push_gesture(drag_gesture(Vec2::zero(), Vec2::new(100.0, 0.0), MouseButton::Left, 4));

        for _ in 0..3 {
            timeline.apply_frame(&mut input);
        }
        assert_eq!(input.cursor_pos, Vec2::new(25.0, 0.0));

        timeline.cancel();
        assert!(timeline.is_gesture_done(id) && timeline.is_idle());
    }

    #[test]
    fn test_parse_key_names() {
        assert_eq!("escape".parse::<InputKey>(), Ok(InputKey::Escape));
        assert_eq!("F12".parse::<InputKey>(), Ok(InputKey::F12));
        assert_eq!("right".parse::<MouseButton>(), Ok(MouseButton::Right));
        assert!("NotAKey".parse::<InputKey>().is_err());
        assert_eq!(modifier_key("Cmd"), Some(InputKey::LeftSuper));
    }
}
