// Command tables, grouped by area. Add a group's table to COMMAND_GROUPS to register it.

use serde::Deserialize;
use serde_json::{Value, json};

use common::{Size, coords::Cell};

use super::{CommandContext, CommandError};
use crate::{
    building::{BuildingArchetypeKind, BuildingKind},
    world::object::GenerationalIndex,
};

mod assert;
mod camera;
mod config;
mod input;
mod meta;
mod observe;
mod session;
mod sim;
mod time;
mod ui;
mod world;

pub(super) const COMMAND_GROUPS: &[&[CommandDef]] = &[
    meta::COMMANDS,
    session::COMMANDS,
    sim::COMMANDS,
    time::COMMANDS,
    world::COMMANDS,
    camera::COMMANDS,
    input::COMMANDS,
    ui::COMMANDS,
    config::COMMANDS,
    observe::COMMANDS,
    assert::COMMANDS,
];

use super::CommandDef;

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

// A size parameter, written as [width, height] or "WIDTHxHEIGHT". Both must be positive.
#[derive(Copy, Clone, Deserialize)]
#[serde(try_from = "Value")]
pub(super) struct SizeParam(pub Size);

impl TryFrom<Value> for SizeParam {
    type Error = String;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        let dimension = |value: &Value| value.as_i64().and_then(|dimension| i32::try_from(dimension).ok());

        let size = match &value {
            Value::Array(dimensions) if dimensions.len() == 2 => {
                dimension(&dimensions[0]).zip(dimension(&dimensions[1])).map(|(width, height)| Size::new(width, height))
            }
            Value::String(text) => engine::cli::parse_size(text).ok(),
            _ => None,
        };

        size.filter(|size| size.width > 0 && size.height > 0)
            .map(SizeParam)
            .ok_or_else(|| format!("expected a size as [width, height] or \"WxH\", both positive, got {value}"))
    }
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

// Recounts the world stats (treasury, population, ...) from the current state. They're only
// recounted on simulation steps otherwise, so they read zero right after a load, and placement
// checks the treasury through them.
pub(super) fn refresh_world_stats(context: &mut CommandContext) {
    context.session.refresh_world_stats();
}

// ----------------------------------------------
// ObjectId
// ----------------------------------------------

// Ids of buildings, units and props in command results and parameters: "<pool>:<index>:<generation>",
// where the pool is house, producer, storage or service for buildings, or unit or prop.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(super) enum ObjectId {
    Building(BuildingArchetypeKind, GenerationalIndex),
    Unit(GenerationalIndex),
    Prop(GenerationalIndex),
}

const BUILDING_POOLS: [(&str, BuildingArchetypeKind); 4] = [
    ("house", BuildingArchetypeKind::HouseBuilding),
    ("producer", BuildingArchetypeKind::ProducerBuilding),
    ("storage", BuildingArchetypeKind::StorageBuilding),
    ("service", BuildingArchetypeKind::ServiceBuilding),
];

impl ObjectId {
    pub fn parse(text: &str) -> Result<Self, CommandError> {
        let error = || {
            CommandError::invalid_params(format!(
                "'{text}' isn't an object id. Ids look like unit:12:40 (pool:index:generation); \
                 world.list and world.building_at return them."
            ))
        };

        let mut parts = text.split(':');
        let (Some(pool), Some(index), Some(generation), None) = (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(error());
        };

        let index = index.parse::<u32>().ok().filter(|index| *index < u32::MAX).ok_or_else(error)?;
        let generation = generation.parse::<u32>().ok().filter(|generation| *generation < u32::MAX).ok_or_else(error)?;
        let id = GenerationalIndex::new(generation, index as usize);

        match pool {
            "unit" => Ok(Self::Unit(id)),
            "prop" => Ok(Self::Prop(id)),
            _ => BUILDING_POOLS
                .iter()
                .find(|(name, _)| *name == pool)
                .map(|(_, archetype)| Self::Building(*archetype, id))
                .ok_or_else(error),
        }
    }

    pub fn building(archetype: BuildingArchetypeKind, id: GenerationalIndex) -> Self {
        Self::Building(archetype, id)
    }

    pub fn to_json(self) -> Value {
        let (pool, id) = match self {
            Self::Building(archetype, id) => {
                let pool = BUILDING_POOLS.iter().find(|(_, kind)| *kind == archetype).map_or("building", |(name, _)| name);
                (pool, id)
            }
            Self::Unit(id) => ("unit", id),
            Self::Prop(id) => ("prop", id),
        };

        Value::String(format!("{pool}:{}:{}", id.index(), id.generation()))
    }
}

// BuildingKinds of one archetype, for World lookups that pick the pool from a kind.
pub(super) fn archetype_building_kinds(archetype: BuildingArchetypeKind) -> BuildingKind {
    match archetype {
        BuildingArchetypeKind::HouseBuilding => BuildingKind::House,
        BuildingArchetypeKind::ProducerBuilding => BuildingKind::producers(),
        BuildingArchetypeKind::StorageBuilding => BuildingKind::storage(),
        BuildingArchetypeKind::ServiceBuilding => BuildingKind::services(),
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
    fn test_size_param() {
        let size = |value: Value| SizeParam::try_from(value).map(|param| param.0);

        assert_eq!(size(json!([64, 32])), Ok(Size::new(64, 32)));
        assert_eq!(size(json!("128x96")), Ok(Size::new(128, 96)));
        assert!(size(json!([0, 32])).is_err());
        assert!(size(json!("12")).is_err());
        assert!(size(json!(64)).is_err());
    }

    #[test]
    fn test_validate_save_name() {
        assert!(validate_save_name("my save 2").is_ok());
        assert!(validate_save_name("autosave").is_ok());

        for bad in ["", ".hidden", "../escape", "dir/save", "dir\\save", "c:save", "a..b"] {
            assert!(validate_save_name(bad).is_err(), "'{bad}' should be rejected.");
        }
    }

    #[test]
    fn test_object_ids() {
        let unit = ObjectId::parse("unit:12:40").unwrap();
        assert_eq!(unit, ObjectId::Unit(GenerationalIndex::new(40, 12)));
        assert_eq!(unit.to_json(), json!("unit:12:40"));

        let house = ObjectId::parse("house:3:7").unwrap();
        assert_eq!(house, ObjectId::Building(BuildingArchetypeKind::HouseBuilding, GenerationalIndex::new(7, 3)));
        assert_eq!(house.to_json(), json!("house:3:7"));

        for bad in ["", "unit", "unit:1", "unit:1:2:3", "tree:1:2", "unit:-1:2", "unit:4294967295:2", "unit:a:b"] {
            assert!(ObjectId::parse(bad).is_err(), "'{bad}' should be rejected.");
        }
    }
}
