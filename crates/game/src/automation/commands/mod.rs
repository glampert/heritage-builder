// Command tables, grouped by area. Add a group's table to COMMAND_GROUPS to register it.

use serde::Deserialize;
use serde_json::{Value, json};

use common::coords::Cell;

use super::{CommandDef, CommandError};

mod camera;
mod meta;
mod observe;
mod session;
mod sim;

pub(super) const COMMAND_GROUPS: &[&[CommandDef]] =
    &[meta::COMMANDS, session::COMMANDS, sim::COMMANDS, camera::COMMANDS, observe::COMMANDS];

// ----------------------------------------------
// Parameter helpers
// ----------------------------------------------

// A cell parameter, written as [x, y] or {"x": x, "y": y}.
#[derive(Copy, Clone, Deserialize)]
#[serde(try_from = "Value")]
pub(super) struct CellParam(pub Cell);

impl TryFrom<Value> for CellParam {
    type Error = String;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        let coord = |value: &Value| value.as_i64().and_then(|coord| i32::try_from(coord).ok());

        let cell = match &value {
            Value::Array(coords) if coords.len() == 2 => coord(&coords[0]).zip(coord(&coords[1])),
            Value::Object(fields) => fields.get("x").and_then(coord).zip(fields.get("y").and_then(coord)),
            _ => None,
        };

        cell.map(|(x, y)| CellParam(Cell::new(x, y)))
            .ok_or_else(|| format!("expected a cell as [x, y] or {{\"x\": x, \"y\": y}}, got {value}"))
    }
}

#[inline]
pub(super) fn cell_json(cell: Cell) -> Value {
    json!([cell.x, cell.y])
}

// Save names are file names inside saves/, without the extension.
pub(super) fn validate_save_name(name: &str) -> Result<(), CommandError> {
    let is_valid = !name.is_empty()
        && !name.starts_with('.')
        && !name.contains(['/', '\\', ':'])
        && !name.contains("..")
        && !name.chars().any(char::is_control);

    if is_valid {
        Ok(())
    } else {
        Err(CommandError::invalid_params(format!("'{name}' isn't a valid save name. Use a plain file name, without folders.")))
    }
}

// ----------------------------------------------
// Unit Tests
// ----------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cell_param() {
        let cell = |value: Value| CellParam::try_from(value).map(|param| param.0);

        assert_eq!(cell(json!([3, -4])), Ok(Cell::new(3, -4)));
        assert_eq!(cell(json!({"x": 5, "y": 6})), Ok(Cell::new(5, 6)));
        assert!(cell(json!([1])).is_err());
        assert!(cell(json!([1.5, 2])).is_err());
        assert!(cell(json!({"x": 1})).is_err());
        assert!(cell(json!([1, 9_999_999_999i64])).is_err());
        assert!(cell(json!("1,2")).is_err());
    }

    #[test]
    fn test_validate_save_name() {
        assert!(validate_save_name("my save 2").is_ok());
        assert!(validate_save_name("autosave").is_ok());

        for bad in ["", ".hidden", "../escape", "dir/save", "dir\\save", "c:save", "a..b"] {
            assert!(validate_save_name(bad).is_err(), "'{bad}' should be rejected.");
        }
    }
}
