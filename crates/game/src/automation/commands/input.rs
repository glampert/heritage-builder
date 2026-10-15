// Simulated input commands: mouse and keyboard gestures played over several frames on a virtual
// mouse and keyboard (see engine::automation::input). Gestures turn input capture on: the real
// mouse and keyboard are ignored until input.capture on=false.

use serde::Deserialize;
use serde_json::{Value, json};

use common::{Vec2, coords::Cell};
use engine::{
    app::input::{InputKey, MouseButton, VirtualInput},
    automation::input::{self as gestures, GestureId, InputStep},
};

use super::{CellParam, camera::cell_to_screen};
use crate::automation::{CommandContext, CommandDef, CommandError, CommandResult, ParamDef, done, parse_params, pending};

pub(super) const COMMANDS: &[CommandDef] = &[
    CommandDef {
        name: "input.capture",
        help: "Turn input capture on or off. While on, the game ignores the real mouse and keyboard and takes \
               input gestures; off gives them back. Gestures turn it on.",
        params: &[ParamDef { name: "on", kind: "bool", required: false, help: "Default true." }],
        run: cmd_capture,
    },
    CommandDef {
        name: "input.move",
        help: "Move the virtual cursor to a window point or a map cell, or park it off the window.",
        params: &[
            ParamDef { name: "point", kind: "point", required: false, help: "[x, y] in window points." },
            ParamDef { name: "cell", kind: "cell", required: false, help: "[x, y] map cell; centers the camera on it if needed." },
            ParamDef { name: "park", kind: "bool", required: false, help: "True moves it off the window, hovering nothing." },
        ],
        run: cmd_move,
    },
    CommandDef {
        name: "input.click",
        help: "Click at a window point or a map cell: hover, press, release, over several frames.",
        params: &[
            ParamDef { name: "point", kind: "point", required: false, help: "[x, y] in window points." },
            ParamDef { name: "cell", kind: "cell", required: false, help: "[x, y] map cell; centers the camera on it if needed." },
            ParamDef { name: "button", kind: "string", required: false, help: "left (default), right or middle." },
        ],
        run: cmd_click,
    },
    CommandDef {
        name: "input.drag",
        help: "Press at one place, move to another over some frames, release. Roads are drawn this way.",
        params: &[
            ParamDef { name: "from", kind: "point", required: true, help: "[x, y] window point, or a cell with cells=true." },
            ParamDef { name: "to", kind: "point", required: true, help: "[x, y] window point, or a cell with cells=true." },
            ParamDef { name: "cells", kind: "bool", required: false, help: "True: from and to are map cells. Default false." },
            ParamDef { name: "button", kind: "string", required: false, help: "left (default), right or middle." },
            ParamDef { name: "frames", kind: "int", required: false, help: "Frames to move over. Default 10." },
        ],
        run: cmd_drag,
    },
    CommandDef {
        name: "input.key",
        help: "Press and release a key, with modifier keys held.",
        params: &[
            ParamDef { name: "key", kind: "string", required: true, help: "Key name, e.g. Escape, Enter, A, F12, Slash, GraveAccent." },
            ParamDef { name: "modifiers", kind: "string[]", required: false, help: "Any of shift, ctrl, alt, cmd." },
        ],
        run: cmd_key,
    },
    CommandDef {
        name: "input.type",
        help: "Type text into the focused text field.",
        params: &[ParamDef { name: "text", kind: "string", required: true, help: "Text to type." }],
        run: cmd_type,
    },
    CommandDef {
        name: "input.scroll",
        help: "Scroll the mouse wheel over a window point or cell (default: the window center). Zooming the map with the wheel is off in the default configs.",
        params: &[
            ParamDef { name: "amount", kind: "number", required: true, help: "Wheel steps; positive scrolls up." },
            ParamDef { name: "point", kind: "point", required: false, help: "[x, y] in window points." },
            ParamDef { name: "cell", kind: "cell", required: false, help: "[x, y] map cell." },
        ],
        run: cmd_scroll,
    },
];

const DEFAULT_DRAG_FRAMES: u32 = 10;

// ----------------------------------------------
// Targets
// ----------------------------------------------

struct Target {
    point: Vec2,
    camera_moved: bool,
}

// A window point, as given or for a map cell. The point must be in the window.
fn resolve_target(context: &mut CommandContext, point: Option<[f32; 2]>, cell: Option<CellParam>) -> Result<Target, CommandError> {
    match (point, cell) {
        (Some(point), None) => {
            let point = Vec2::new(point[0], point[1]);
            require_point_in_window(context, point)?;
            Ok(Target { point, camera_moved: false })
        }
        (None, Some(cell)) => cell_target(context, cell.0),
        (Some(_), Some(_)) => Err(CommandError::invalid_params("Give either point or cell, not both.")),
        (None, None) => Err(CommandError::invalid_params("Give a point or a cell.")),
    }
}

// Centers the camera on the cell first unless it's in the middle of the screen, away from the
// HUD panels along the edges and the edge-scrolling margin.
fn cell_target(context: &mut CommandContext, cell: Cell) -> Result<Target, CommandError> {
    context.require_map()?;
    context.require_cell_in_map(cell)?;

    let window_size = context.engine.app().window_size().to_vec2();
    let in_middle = |point: Vec2| {
        (window_size.x * 0.2..=window_size.x * 0.8).contains(&point.x)
            && (window_size.y * 0.2..=window_size.y * 0.8).contains(&point.y)
    };

    let mut point = cell_to_screen(context, cell);
    let mut camera_moved = false;

    if !in_middle(point) {
        context.session.camera_mut().teleport(cell);
        point = cell_to_screen(context, cell);
        camera_moved = true;
    }

    require_point_in_window(context, point)?;
    Ok(Target { point, camera_moved })
}

fn require_point_in_window(context: &CommandContext, point: Vec2) -> Result<(), CommandError> {
    let window_size = context.engine.app().window_size().to_vec2();
    if (0.0..window_size.x).contains(&point.x) && (0.0..window_size.y).contains(&point.y) {
        Ok(())
    } else {
        Err(CommandError::invalid_params(format!(
            "Point [{}, {}] is outside the {}x{} window.",
            point.x, point.y, window_size.x, window_size.y
        )))
    }
}

fn button_param(button: Option<&str>) -> Result<MouseButton, CommandError> {
    match button.unwrap_or("left").parse::<MouseButton>() {
        Ok(button @ (MouseButton::Left | MouseButton::Right | MouseButton::Middle)) => Ok(button),
        _ => Err(CommandError::invalid_params(format!("button must be left, right or middle, got '{}'.", button.unwrap_or_default()))),
    }
}

// Finishes once the gesture has played and the game has handled it.
fn wait_for_gesture(context: &mut CommandContext, steps: Vec<InputStep>, reply: Value) -> CommandResult {
    let id: GestureId = context.engine.queue_input_gesture(steps);
    let start_frame = context.frame();

    pending(move |context| {
        context.engine.is_input_gesture_done(id).then(|| {
            let mut reply = reply.clone();
            reply["frames"] = json!(context.frame() - start_frame);
            Ok(reply)
        })
    })
}

fn point_json(point: Vec2) -> Value {
    json!([point.x, point.y])
}

// ----------------------------------------------
// Commands
// ----------------------------------------------

fn cmd_capture(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        on: Option<bool>,
    }

    let params: Params = parse_params(params)?;
    context.engine.set_input_capture(params.on.unwrap_or(true));

    let cursor = context.engine.input_system().virtual_input().map(|input| point_json(input.cursor_pos));
    done(json!({ "captured": context.engine.is_input_captured(), "cursor": cursor }))
}

fn cmd_move(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        point: Option<[f32; 2]>,
        cell: Option<CellParam>,
        #[serde(default)]
        park: bool,
    }

    let params: Params = parse_params(params)?;

    if params.park {
        if params.point.is_some() || params.cell.is_some() {
            return Err(CommandError::invalid_params("park doesn't take a point or cell."));
        }
        let steps = gestures::move_gesture(VirtualInput::PARKED_CURSOR_POS);
        return wait_for_gesture(context, steps, json!({ "parked": true }));
    }

    let target = resolve_target(context, params.point, params.cell)?;
    let reply = json!({ "point": point_json(target.point), "camera_moved": target.camera_moved });
    wait_for_gesture(context, gestures::move_gesture(target.point), reply)
}

fn cmd_click(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        point: Option<[f32; 2]>,
        cell: Option<CellParam>,
        button: Option<String>,
    }

    let params: Params = parse_params(params)?;
    let button = button_param(params.button.as_deref())?;
    let target = resolve_target(context, params.point, params.cell)?;

    let reply = json!({ "point": point_json(target.point), "camera_moved": target.camera_moved });
    wait_for_gesture(context, gestures::click_gesture(target.point, button), reply)
}

fn cmd_drag(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        from: [f32; 2],
        to: [f32; 2],
        #[serde(default)]
        cells: bool,
        button: Option<String>,
        frames: Option<u32>,
    }

    let params: Params = parse_params(params)?;
    let button = button_param(params.button.as_deref())?;

    let frames = params.frames.unwrap_or(DEFAULT_DRAG_FRAMES);
    if !(1..=600).contains(&frames) {
        return Err(CommandError::invalid_params(format!("frames must be from 1 to 600, got {frames}.")));
    }

    let (from, to, camera_moved) = if params.cells {
        let cell = |coords: [f32; 2]| {
            let is_whole = coords.iter().all(|coord| coord.fract() == 0.0 && coord.abs() < 1e6);
            is_whole
                .then(|| Cell::new(coords[0] as i32, coords[1] as i32))
                .ok_or_else(|| CommandError::invalid_params(format!("With cells=true, from and to are whole cells, got {coords:?}.")))
        };
        let (from_cell, to_cell) = (cell(params.from)?, cell(params.to)?);

        // Center the camera between the two cells, then make sure both are on screen.
        context.require_map()?;
        context.require_cell_in_map(from_cell)?;
        context.require_cell_in_map(to_cell)?;

        let middle = Cell::new((from_cell.x + to_cell.x) / 2, (from_cell.y + to_cell.y) / 2);
        context.session.camera_mut().teleport(middle);

        let from = cell_to_screen(context, from_cell);
        let to = cell_to_screen(context, to_cell);
        require_point_in_window(context, from)?;
        require_point_in_window(context, to)?;
        (from, to, true)
    } else {
        let (from, to) = (Vec2::new(params.from[0], params.from[1]), Vec2::new(params.to[0], params.to[1]));
        require_point_in_window(context, from)?;
        require_point_in_window(context, to)?;
        (from, to, false)
    };

    let reply = json!({ "from": point_json(from), "to": point_json(to), "camera_moved": camera_moved });
    wait_for_gesture(context, gestures::drag_gesture(from, to, button, frames), reply)
}

fn cmd_key(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        key: String,
        #[serde(default)]
        modifiers: Vec<String>,
    }

    let params: Params = parse_params(params)?;

    let key: InputKey = params
        .key
        .parse()
        .ok()
        .filter(|key| *key != InputKey::Unknown)
        .ok_or_else(|| CommandError::invalid_params(format!("Unknown key '{}'. Names look like Escape, A, Num1, F12.", params.key)))?;

    let modifiers = params
        .modifiers
        .iter()
        .map(|name| {
            gestures::modifier_key(name)
                .ok_or_else(|| CommandError::invalid_params(format!("Unknown modifier '{name}'. Use shift, ctrl, alt or cmd.")))
        })
        .collect::<Result<Vec<InputKey>, CommandError>>()?;

    let reply = json!({ "key": format!("{key:?}"), "modifiers": params.modifiers });
    wait_for_gesture(context, gestures::key_gesture(key, &modifiers), reply)
}

fn cmd_type(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        text: String,
    }

    let params: Params = parse_params(params)?;
    if params.text.is_empty() || params.text.chars().count() > 1000 {
        return Err(CommandError::invalid_params("text must have from 1 to 1000 characters."));
    }

    let reply = json!({ "typed": params.text.chars().count() });
    wait_for_gesture(context, gestures::type_gesture(&params.text), reply)
}

fn cmd_scroll(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        amount: f32,
        point: Option<[f32; 2]>,
        cell: Option<CellParam>,
    }

    let params: Params = parse_params(params)?;
    if !(params.amount.is_finite() && params.amount.abs() <= 100.0) {
        return Err(CommandError::invalid_params(format!("amount must be from -100 to 100, got {}.", params.amount)));
    }

    let target = if params.point.is_none() && params.cell.is_none() {
        Target { point: context.engine.app().window_size().to_vec2() * 0.5, camera_moved: false }
    } else {
        resolve_target(context, params.point, params.cell)?
    };

    let reply = json!({ "point": point_json(target.point), "camera_moved": target.camera_moved });
    let steps = gestures::scroll_gesture(target.point, Vec2::new(0.0, params.amount));
    wait_for_gesture(context, steps, reply)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_button_param() {
        assert_eq!(button_param(None).unwrap(), MouseButton::Left);
        assert_eq!(button_param(Some("Right")).unwrap(), MouseButton::Right);
        assert!(button_param(Some("back")).is_err());
        assert!(button_param(Some("thumb")).is_err());
    }
}
