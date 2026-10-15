// UI automation commands: list the game's widgets, click them and set their values by name, and
// open or close dialogs. Widgets come from the engine's widget registry (engine::ui::registry),
// which records what the retained widgets drew each frame. These commands turn it on; the first
// one waits a frame for it to fill.

use serde::Deserialize;
use serde_json::{Map, Value, json};
use strum::IntoEnumIterator;

use engine::{app::input::MouseButton, automation::input as gestures, ui::UiWidgetInfo};

use crate::{
    automation::{CommandContext, CommandDef, CommandError, CommandResult, ErrorCode, ParamDef, done, parse_params, pending},
    menu::dialog::{self, DialogMenuKind},
};

pub(super) const COMMANDS: &[CommandDef] = &[
    CommandDef {
        name: "ui.tree",
        help: "The game's widgets drawn last frame: path, kind, label, rect [x, y, width, height] in window points, \
               enabled, value. Use the paths with ui.click and ui.set.",
        params: &[
            ParamDef { name: "filter", kind: "string", required: false, help: "Only widgets whose path contains this (any case)." },
            ParamDef { name: "kind", kind: "string", required: false, help: "Only this kind, e.g. text_button, sprite_button." },
        ],
        run: cmd_tree,
    },
    CommandDef {
        name: "ui.click",
        help: "Click a widget by path, with the simulated mouse: hover, press, release, as a player would.",
        params: &[
            ParamDef {
                name: "path",
                kind: "string",
                required: true,
                help: "Widget path from ui.tree, the end of one (e.g. \"New Game\"), or its tooltip.",
            },
            ParamDef { name: "button", kind: "string", required: false, help: "left (default) or right." },
        ],
        run: cmd_click,
    },
    CommandDef {
        name: "ui.set",
        help: "Set a checkbox, slider, number or text input, dropdown or list through the widget's own update.",
        params: &[
            ParamDef { name: "path", kind: "string", required: true, help: "Widget path from ui.tree, or the end of one." },
            ParamDef { name: "value", kind: "any", required: true, help: "Bool, number, text, or a list item's text or index." },
        ],
        run: cmd_set,
    },
    CommandDef { name: "dialog.list", help: "The game's dialogs, and which are open.", params: &[], run: cmd_dialog_list },
    CommandDef { name: "dialog.current", help: "The dialog on top, or null.", params: &[], run: cmd_dialog_current },
    CommandDef {
        name: "dialog.open",
        help: "Open a dialog, closing the others, e.g. NewGame, LoadGame, MainSettings.",
        params: &[ParamDef { name: "name", kind: "string", required: true, help: "Dialog name, see dialog.list." }],
        run: cmd_dialog_open,
    },
    CommandDef {
        name: "dialog.close",
        help: "Close a dialog, or the one on top.",
        params: &[ParamDef { name: "name", kind: "string", required: false, help: "Dialog name. Default: the current one." }],
        run: cmd_dialog_close,
    },
];

const SETTABLE_KINDS: &[&str] = &["checkbox", "slider", "int_input", "text_input", "dropdown", "item_list"];

// How long ui.click and ui.set look for a widget that isn't there (yet), e.g. in a dialog that
// opens on the next frame.
const FIND_WIDGET_TIMEOUT_FRAMES: u64 = 30;

// ----------------------------------------------
// Registry access
// ----------------------------------------------

// The frame from which the registry holds a full frame of widgets. Turns it on if needed.
fn registry_ready_frame(context: &mut CommandContext) -> u64 {
    let ui_sys = context.engine.ui_system();
    if ui_sys.is_widget_registry_enabled() {
        context.frame()
    } else {
        ui_sys.set_widget_registry_enabled(true);
        context.frame() + 1 // Widgets record during this frame's draw.
    }
}

fn widget_json(widget: &UiWidgetInfo) -> Value {
    let rect = widget.rect;
    let mut json = json!({
        "path": widget.path,
        "kind": widget.kind,
        "rect": [rect.x(), rect.y(), rect.width(), rect.height()],
        "enabled": widget.enabled,
    });

    if !widget.label.is_empty() && !widget.path.ends_with(&widget.label) {
        json["label"] = json!(widget.label);
    }
    if let Some(tooltip) = &widget.tooltip {
        json["tooltip"] = json!(tooltip);
    }
    if widget.hovered {
        json["hovered"] = json!(true);
    }
    if let Some(value) = &widget.value {
        json["value"] = value.clone();
    }

    json
}

// find_widget, giving a missing widget until `deadline_frame` to appear. None: keep waiting.
fn find_widget_by(context: &CommandContext, query: &str, deadline_frame: u64) -> Option<Result<UiWidgetInfo, CommandError>> {
    match find_widget(context, query) {
        Err(err) if err.code == ErrorCode::NotFound && context.frame() < deadline_frame => None,
        result => Some(result),
    }
}

// Finds one widget: by exact path, then by the end of a path, a label or a tooltip (any case).
fn find_widget(context: &CommandContext, query: &str) -> Result<UiWidgetInfo, CommandError> {
    let widgets = context.engine.ui_system().widgets();

    if let Some(widget) = widgets.iter().find(|widget| widget.path == query) {
        return Ok(widget.clone());
    }

    let query_lower = query.to_lowercase();
    let suffix = format!("/{query_lower}");
    let matches: Vec<&UiWidgetInfo> = widgets
        .iter()
        .filter(|widget| {
            let path = widget.path.to_lowercase();
            path == query_lower
                || path.ends_with(&suffix)
                || widget.label.to_lowercase() == query_lower
                || widget.tooltip.as_ref().is_some_and(|tooltip| tooltip.to_lowercase() == query_lower)
        })
        .collect();

    match matches.as_slice() {
        [widget] => Ok((*widget).clone()),
        [] => Err(CommandError::not_found(format!("No widget matches '{query}'. ui.tree lists them."))),
        _ => {
            let paths: Vec<&str> = matches.iter().take(10).map(|widget| widget.path.as_str()).collect();
            Err(CommandError::invalid_params(format!("'{query}' matches several widgets: {}. Give a full path.", paths.join(", "))))
        }
    }
}

// ----------------------------------------------
// Commands
// ----------------------------------------------

fn cmd_tree(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        filter: Option<String>,
        kind: Option<String>,
    }

    let params: Params = parse_params(params)?;
    let ready_frame = registry_ready_frame(context);
    let filter = params.filter.map(|filter| filter.to_lowercase());

    pending(move |context| {
        if context.frame() < ready_frame {
            return None;
        }

        let widgets: Vec<Value> = context
            .engine
            .ui_system()
            .widgets()
            .iter()
            .filter(|widget| filter.as_deref().is_none_or(|filter| widget.path.to_lowercase().contains(filter)))
            .filter(|widget| params.kind.as_deref().is_none_or(|kind| widget.kind == kind))
            .map(widget_json)
            .collect();

        let window_size = context.engine.app().window_size();
        Some(Ok(json!({ "window_size": [window_size.width, window_size.height], "widgets": widgets })))
    })
}

fn cmd_click(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        path: String,
        button: Option<String>,
    }

    let params: Params = parse_params(params)?;
    let button = match params.button.as_deref().unwrap_or("left").parse::<MouseButton>() {
        Ok(button @ (MouseButton::Left | MouseButton::Right)) => button,
        _ => return Err(CommandError::invalid_params("button must be left or right.")),
    };

    let ready_frame = registry_ready_frame(context);
    let deadline_frame = ready_frame + FIND_WIDGET_TIMEOUT_FRAMES;
    let mut gesture = None;
    let mut done_frame = None;

    pending(move |context| {
        if let Some((id, reply)) = &gesture {
            // One more frame after the gesture, so whatever the click opened has drawn once and
            // the next ui command finds it.
            if done_frame.is_none() && context.engine.is_input_gesture_done(*id) {
                done_frame = Some(context.frame() + 1);
            }
            return done_frame.is_some_and(|frame| context.frame() >= frame).then(|| Ok(Value::clone(reply)));
        }
        if context.frame() < ready_frame {
            return None;
        }

        let widget = match find_widget_by(context, &params.path, deadline_frame)? {
            Ok(widget) => widget,
            Err(err) => return Some(Err(err)),
        };

        if !widget.enabled {
            return Some(Err(CommandError::failed(format!("'{}' is disabled.", widget.path))));
        }

        let center = widget.rect.center();
        let window_size = context.engine.app().window_size().to_vec2();
        if !(center.x >= 0.0 && center.y >= 0.0 && center.x < window_size.x && center.y < window_size.y) {
            return Some(Err(CommandError::failed(format!("'{}' is off the window (scrolled away?).", widget.path))));
        }

        let id = context.engine.queue_input_gesture(gestures::click_gesture(center, button));
        gesture = Some((id, json!({ "path": widget.path, "kind": widget.kind, "point": [center.x, center.y] })));
        None
    })
}

fn cmd_set(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        path: String,
        value: Value,
    }

    let params: Params = parse_params(params)?;
    let ready_frame = registry_ready_frame(context);
    let deadline_frame = ready_frame + FIND_WIDGET_TIMEOUT_FRAMES;
    let mut queued: Option<(String, u64)> = None; // Path and the frame its new value shows from.

    pending(move |context| {
        if let Some((path, shown_frame)) = &queued {
            if context.frame() < *shown_frame {
                return None;
            }
            // Read the value back: the widget records it on the frame after applying it.
            let value = context.engine.ui_system().widgets().iter().find(|widget| widget.path == *path).and_then(|widget| widget.value.clone());
            return Some(Ok(json!({ "path": path, "value": value })));
        }
        if context.frame() < ready_frame {
            return None;
        }

        let widget = match find_widget_by(context, &params.path, deadline_frame)? {
            Ok(widget) => widget,
            Err(err) => return Some(Err(err)),
        };

        if let Err(err) = check_value(&widget, &params.value) {
            return Some(Err(err));
        }

        context.engine.ui_system().queue_widget_value(widget.path.clone(), params.value.clone());
        queued = Some((widget.path, context.frame() + 2));
        None
    })
}

fn check_value(widget: &UiWidgetInfo, value: &Value) -> Result<(), CommandError> {
    if !SETTABLE_KINDS.contains(&widget.kind) {
        return Err(CommandError::invalid_params(format!(
            "'{}' is a {}; ui.set takes {}. Click buttons with ui.click.",
            widget.path,
            widget.kind,
            SETTABLE_KINDS.join(", ")
        )));
    }

    let fits = match widget.kind {
        "checkbox" => value.is_boolean(),
        "slider" | "int_input" => value.is_number(),
        "text_input" => value.is_string(),
        _ => value.is_string() || value.is_u64(), // Dropdowns and lists: item text or index.
    };

    if fits {
        Ok(())
    } else {
        Err(CommandError::invalid_params(format!("{value} doesn't fit '{}', a {}.", widget.path, widget.kind)))
    }
}

// ----------------------------------------------
// Dialogs
// ----------------------------------------------

fn require_dialogs() -> Result<(), CommandError> {
    if dialog::is_initialized() {
        Ok(())
    } else {
        Err(CommandError::new(ErrorCode::NotInGame, "No dialogs here: they come with the home and in-game menus."))
    }
}

fn dialog_kind(name: &str) -> Result<DialogMenuKind, CommandError> {
    DialogMenuKind::iter().find(|kind| kind.to_string().eq_ignore_ascii_case(name)).ok_or_else(|| {
        let names: Vec<String> = DialogMenuKind::iter().map(|kind| kind.to_string()).collect();
        CommandError::invalid_params(format!("No dialog '{name}'. Dialogs: {}.", names.join(", ")))
    })
}

fn cmd_dialog_list(_context: &mut CommandContext, _params: Value) -> CommandResult {
    require_dialogs()?;
    let dialogs: Map<String, Value> =
        DialogMenuKind::iter().map(|kind| (kind.to_string(), Value::Bool(dialog::is_open(kind)))).collect();
    done(json!({ "dialogs": dialogs }))
}

fn cmd_dialog_current(_context: &mut CommandContext, _params: Value) -> CommandResult {
    require_dialogs()?;
    done(json!({ "current": dialog::current().map(|kind| kind.to_string()) }))
}

fn cmd_dialog_open(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        name: String,
    }

    let params: Params = parse_params(params)?;
    require_dialogs()?;
    let kind = dialog_kind(&params.name)?;

    let opened = context.session.with_ui_context(context.engine, |ui_context| dialog::open(kind, true, ui_context));
    if !opened && !dialog::is_open(kind) {
        return Err(CommandError::failed(format!("{kind} didn't open.")));
    }

    done(json!({ "current": dialog::current().map(|kind| kind.to_string()) }))
}

fn cmd_dialog_close(context: &mut CommandContext, params: Value) -> CommandResult {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        name: Option<String>,
    }

    let params: Params = parse_params(params)?;
    require_dialogs()?;

    let closed = match params.name {
        Some(name) => {
            let kind = dialog_kind(&name)?;
            context.session.with_ui_context(context.engine, |ui_context| dialog::close(kind, ui_context))
        }
        None => context.session.with_ui_context(context.engine, dialog::close_current),
    };

    done(json!({ "closed": closed, "current": dialog::current().map(|kind| kind.to_string()) }))
}

#[cfg(test)]
mod tests {
    use common::{Rect, Vec2};

    use super::*;

    fn widget(kind: &'static str) -> UiWidgetInfo {
        UiWidgetInfo {
            path: "Settings/Thing".into(),
            kind,
            label: String::new(),
            tooltip: None,
            rect: Rect::from_pos_and_size(Vec2::zero(), Vec2::new(1.0, 1.0)),
            enabled: true,
            hovered: false,
            value: None,
        }
    }

    #[test]
    fn test_check_value() {
        assert!(check_value(&widget("checkbox"), &json!(true)).is_ok());
        assert!(check_value(&widget("checkbox"), &json!(1)).is_err());
        assert!(check_value(&widget("slider"), &json!(0.5)).is_ok());
        assert!(check_value(&widget("text_input"), &json!("hi")).is_ok());
        assert!(check_value(&widget("dropdown"), &json!(2)).is_ok());
        assert!(check_value(&widget("dropdown"), &json!("Dirt")).is_ok());
        assert!(check_value(&widget("text_button"), &json!(true)).is_err());
    }
}
