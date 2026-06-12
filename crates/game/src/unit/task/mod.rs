use std::any::Any;
use std::fmt;
use strum::Display;
use serde::{Deserialize, Serialize};

use ::common::coords::Cell;
use engine::ui::UiSystem;

use crate::{
    debug,
    building::{BuildingKind, BuildingKindAndId, BuildingTileInfo},
    sim::SimContext,
    tile::TileMapLayerKind,
    unit::Unit,
};

mod common;
mod despawn;
mod deliver;
mod fetch;
mod follow_path;
mod harvest;
mod manager;
mod patrol;
mod settler;
mod state_machine;

pub use common::*;
pub use despawn::*;
pub use deliver::*;
pub use fetch::*;
pub use follow_path::*;
pub use harvest::*;
pub use manager::*;
pub use patrol::*;
pub use settler::*;
pub use state_machine::*;

// ----------------------------------------------
// UnitTaskOriginBuildingDebug
// ----------------------------------------------

// Debug-only `Display` helper bundling a task's origin-building info (kind, name
// and base cell) into a single line. Shared by the task `draw_debug_ui` view
// structs so the repeated "Origin Building : Kind, 'Name', Cell" block is
// expressed once and rendered automatically by `#[derive(DrawDebugUi)]`.
pub(crate) struct UnitTaskOriginBuildingDebug {
    kind: BuildingKind,
    base_cell: Cell,
}

impl UnitTaskOriginBuildingDebug {
    #[inline]
    pub(crate) fn new(kind_and_id: BuildingKindAndId, tile: BuildingTileInfo) -> Self {
        Self { kind: kind_and_id.kind, base_cell: tile.base_cell }
    }
}

impl fmt::Display for UnitTaskOriginBuildingDebug {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let name = debug::tile_name_at(self.base_cell, TileMapLayerKind::Objects);
        write!(f, "{}, '{}', {}", self.kind, name, self.base_cell)
    }
}

// ----------------------------------------------
// UnitTaskArchetype
// ----------------------------------------------

// One variant per concrete task type. Serializes as `{ "UnitTaskXxx": <task> }`.
#[derive(Display, Serialize, Deserialize)]
#[allow(clippy::enum_variant_names)]
pub enum UnitTaskArchetype {
    UnitTaskDespawn(UnitTaskDespawn),
    UnitTaskDespawnWithCallback(UnitTaskDespawnWithCallback),
    UnitTaskRandomizedPatrol(UnitTaskRandomizedPatrol),
    UnitTaskDeliverToStorage(UnitTaskDeliverToStorage),
    UnitTaskFetchFromStorage(UnitTaskFetchFromStorage),
    UnitTaskSettler(UnitTaskSettler),
    UnitTaskHarvestWood(UnitTaskHarvestWood),
    UnitTaskFollowPath(UnitTaskFollowPath),
}

// Dispatches a method call to the wrapped concrete task, for every variant.
macro_rules! archetype_dispatch {
    ($self:expr, $task:ident => $body:expr) => {
        match $self {
            UnitTaskArchetype::UnitTaskDespawn($task) => $body,
            UnitTaskArchetype::UnitTaskDespawnWithCallback($task) => $body,
            UnitTaskArchetype::UnitTaskRandomizedPatrol($task) => $body,
            UnitTaskArchetype::UnitTaskDeliverToStorage($task) => $body,
            UnitTaskArchetype::UnitTaskFetchFromStorage($task) => $body,
            UnitTaskArchetype::UnitTaskSettler($task) => $body,
            UnitTaskArchetype::UnitTaskHarvestWood($task) => $body,
            UnitTaskArchetype::UnitTaskFollowPath($task) => $body,
        }
    };
}

// Type-erased forwarding to the wrapped task's `UnitTaskFsm` impl.
impl UnitTaskArchetype {
    #[inline]
    pub fn initialize(&mut self, ctx: &mut UnitTaskContext) {
        archetype_dispatch!(self, task => UnitTaskFsm::initialize(task, ctx))
    }

    #[inline]
    pub fn terminate(&mut self, pool: &mut UnitTaskPool) {
        archetype_dispatch!(self, task => UnitTaskFsm::terminate(task, pool))
    }

    #[inline]
    pub fn run(&mut self, ctx: &mut UnitTaskContext) -> UnitTaskFlow {
        archetype_dispatch!(self, task => UnitTaskFsm::run(task, ctx))
    }

    #[inline]
    pub fn post_load(&mut self) {
        archetype_dispatch!(self, task => UnitTaskFsm::post_load(task))
    }

    #[inline]
    pub fn draw_debug_ui(&mut self, unit: &mut Unit, context: &SimContext, ui: &UiSystem) {
        archetype_dispatch!(self, task => UnitTaskFsm::draw_debug_ui(task, unit, context, ui))
    }

    #[inline]
    pub fn as_any(&self) -> &dyn Any {
        archetype_dispatch!(self, task => UnitTaskFsm::as_any(task))
    }
}

// `From<Task>` for each variant, so `UnitTaskManager::new_task` can wrap a task.
macro_rules! archetype_from {
    ($($variant:ident),+ $(,)?) => {
        $(
            impl From<$variant> for UnitTaskArchetype {
                #[inline]
                fn from(task: $variant) -> Self {
                    UnitTaskArchetype::$variant(task)
                }
            }
        )+
    };
}

archetype_from!(
    UnitTaskDespawn,
    UnitTaskDespawnWithCallback,
    UnitTaskRandomizedPatrol,
    UnitTaskDeliverToStorage,
    UnitTaskFetchFromStorage,
    UnitTaskSettler,
    UnitTaskHarvestWood,
    UnitTaskFollowPath,
);
