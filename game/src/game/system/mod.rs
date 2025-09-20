use std::any::Any;
use strum::EnumCount;
use strum_macros::EnumCount;
use enum_dispatch::enum_dispatch;

use serde::{
    Serialize,
    Deserialize
};

use crate::{
    utils::mem,
    save::*,
    imgui_ui::UiSystem,
};

use super::{
    constants::*,
    sim::Query,
    world::object::GenerationalIndex
};

pub mod settlers;
use settlers::SettlersSpawnSystem;

// ----------------------------------------------
// GameSystem
// ----------------------------------------------

#[enum_dispatch(GameSystemImpl)]
pub trait GameSystem {
    // Required overrides:
    fn name(&self) -> &str;
    fn as_any(&self) -> &dyn Any;
    fn update(&mut self, query: &Query);

    // Optional overrides:
    fn reset(&mut self) {}
    fn post_load(&mut self, _context: &PostLoadContext) {}
    fn draw_debug_ui(&mut self, _query: &Query, _ui_sys: &UiSystem) {}
}

#[enum_dispatch]
#[derive(EnumCount, Serialize, Deserialize)]
pub enum GameSystemImpl {
    SettlersSpawnSystem,
}

// ----------------------------------------------
// GameSystems
// ----------------------------------------------

pub type GameSystemId = GenerationalIndex;

#[derive(Serialize, Deserialize)]
struct GameSystemEntry {
    system: GameSystemImpl,
    generation: u32,
}

#[derive(Serialize, Deserialize)]
pub struct GameSystems {
    systems: Vec<GameSystemEntry>,
    generation: u32,
}

impl GameSystems {
    pub fn new() -> Self {
        Self {
            systems: Vec::with_capacity(GameSystemImpl::COUNT),
            generation: INITIAL_GENERATION,
        }
    }

    pub fn register<System>(&mut self, system: System) -> GameSystemId
        where System: GameSystem + 'static,
              GameSystemImpl: From<System>
    {
        let index = self.systems.len();
        let generation = self.generation;

        self.systems.push(GameSystemEntry { system: GameSystemImpl::from(system), generation });
        self.generation += 1;

        GameSystemId::new(generation, index)
    }

    pub fn find<System>(&self, sys_id: GameSystemId) -> Option<&System>
        where System: GameSystem + 'static
    {
        if sys_id.index() < self.systems.len() {
            let entry = &self.systems[sys_id.index()];
            if sys_id.generation() == entry.generation {
                return entry.system.as_any().downcast_ref::<System>();
            }
        }
        None
    }

    pub fn find_mut<System>(&mut self, sys_id: GameSystemId) -> Option<&mut System>
        where System: GameSystem + 'static
    {
        if sys_id.index() < self.systems.len() {
            let entry = &mut self.systems[sys_id.index()];
            if sys_id.generation() == entry.generation {
                // Reuse the non-mutable method.
                let sys_mut = mem::mut_ref_cast(entry.system.as_any());
                return sys_mut.downcast_mut::<System>();
            }
        }
        None
    }

    pub fn update(&mut self, query: &Query) {
        for entry in &mut self.systems {
            entry.system.update(query);
        }
    }

    pub fn reset(&mut self) {
        for entry in &mut self.systems {
            entry.system.reset();
        }
    }

    pub fn draw_debug_ui(&mut self, query: &Query, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();
        if let Some(_tab_bar) = ui.tab_bar("Game Systems Tab Bar") {
            for entry in &mut self.systems {
                if let Some(_tab) = ui.tab_item(entry.system.name()) {
                    entry.system.draw_debug_ui(query, ui_sys);
                }
            }
        }
    }

    // ----------------------
    // Callbacks:
    // ----------------------

    pub fn register_callbacks() {
        SettlersSpawnSystem::register_callbacks();
    }
}

// ----------------------------------------------
// Save/Load for GameSystems
// ----------------------------------------------

impl Save for GameSystems {
    fn save(&self, state: &mut SaveStateImpl) -> SaveResult {
        state.save(self)
    }
}

impl Load<'_> for GameSystems {
    fn load(&mut self, state: &SaveStateImpl) -> LoadResult {
        state.load(self)
    }

    fn post_load(&mut self, context: &PostLoadContext) {
        debug_assert!(self.generation != RESERVED_GENERATION);

        for entry in &mut self.systems {
            debug_assert!(entry.generation != RESERVED_GENERATION);
            debug_assert!(entry.generation  < self.generation);
 
            entry.system.post_load(context);
        }
    }
}
