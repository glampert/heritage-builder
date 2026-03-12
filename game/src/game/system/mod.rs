#![allow(clippy::enum_variant_names)]

use std::any::{Any, TypeId};
use enum_dispatch::enum_dispatch;
use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, VariantNames, EnumIter, Display};
use strum::{EnumCount, VariantNames, IntoEnumIterator};

use super::{constants::*, sim::SimContext, world::object::GenerationalIndex};
use crate::{engine::Engine, ui::{UiSystem, UiStaticVar}, save::*, utils::mem};

// ----------------------------------------------
// Game System Implementations
// ----------------------------------------------

pub mod settlers;
use settlers::SettlersSpawnSystem;

pub mod ambient_effects;
use ambient_effects::AmbientEffectsSystem;

pub mod ambient_music;
use ambient_music::AmbientMusicSystem;

pub mod ambient_sounds;
use ambient_sounds::AmbientSoundsSystem;

// ----------------------------------------------
// GameSystem
// ----------------------------------------------

#[enum_dispatch(GameSystemImpl)]
pub trait GameSystem: Any {
    // Required overrides:
    fn as_any(&self) -> &dyn Any;
    fn update(&mut self, engine: &mut dyn Engine, context: &SimContext);

    // Optional overrides:
    fn paused_update(&mut self, _engine: &mut dyn Engine, _query: &SimContext) {}
    fn reset(&mut self, _engine: &mut dyn Engine) {}
    fn post_load(&mut self, _context: &PostLoadContext) {}
    fn draw_debug_ui(&mut self, _engine: &mut dyn Engine, _query: &SimContext) {}
    fn register_callbacks(&self) {}
}

#[enum_dispatch]
#[derive(EnumCount, EnumIter, VariantNames, Display, Serialize, Deserialize)]
pub enum GameSystemImpl {
    SettlersSpawnSystem,
    AmbientEffectsSystem,
    AmbientMusicSystem,
    AmbientSoundsSystem,
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
        Self { systems: Vec::with_capacity(GameSystemImpl::COUNT), generation: INITIAL_GENERATION }
    }

    pub fn register_all() -> Self {
        let mut systems = Self::new();
        for system in GameSystemImpl::iter() {
            systems.register(system);
        }
        systems
    }

    pub fn register<System>(&mut self, system: System) -> GameSystemId
        where System: GameSystem + 'static,
              GameSystemImpl: From<System>
    {
        let sys_impl = GameSystemImpl::from(system);
        debug_assert!(!self.has(sys_impl.as_any().type_id()), "System {sys_impl} already registered!");

        let index = self.systems.len();
        let generation = self.generation;

        self.systems.push(GameSystemEntry { system: sys_impl, generation });
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

    pub fn has(&self, system_type: TypeId) -> bool {
        for entry in &self.systems {
            if entry.system.as_any().type_id() == system_type {
                return true;
            }
        }
        false
    }

    // Regular update, called every simulation tick *when the game is NOT paused*.
    pub fn update(&mut self, engine: &mut dyn Engine, context: &SimContext) {
        for entry in &mut self.systems {
            entry.system.update(engine, context);
        }
    }

    // Update called every simulation tick *only when the game IS paused*.
    pub fn paused_update(&mut self, engine: &mut dyn Engine, context: &SimContext) {
        for entry in &mut self.systems {
            entry.system.paused_update(engine, context);
        }
    }

    pub fn reset(&mut self, engine: &mut dyn Engine) {
        for entry in &mut self.systems {
            entry.system.reset(engine);
        }
    }

    fn create_missing(&mut self) {
        for system in GameSystemImpl::iter() {
            if !self.has(system.as_any().type_id()) {
                self.register(system);
            }
        }
    }

    // ----------------------
    // Debug UI:
    // ----------------------

    pub fn draw_debug_ui(&mut self, engine: &mut dyn Engine, context: &SimContext, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();
        if let Some(_tab_bar) = ui.tab_bar("Game Systems Tab Bar") {
            for entry in &mut self.systems {
                if let Some(_tab) = ui.tab_item(entry.system.to_string()) {
                    entry.system.draw_debug_ui(engine, context);
                }
            }

            if let Some(_tab) = ui.tab_item("Create Systems") {
                ui.text("Create and register system if not already created.");

                static SYSTEM_INDEX: UiStaticVar<usize> = UiStaticVar::new(0);
                ui.combo_simple_string("Systems", SYSTEM_INDEX.as_mut(), GameSystemImpl::VARIANTS);

                if ui.button("Create") {
                    if let Some(system) = GameSystemImpl::iter().nth(*SYSTEM_INDEX) {
                        if !self.has(system.as_any().type_id()) {
                            self.register(system);
                        }
                    }
                }
            }
        }
    }

    // ----------------------
    // Callbacks:
    // ----------------------

    pub fn register_callbacks() {
        for system in GameSystemImpl::iter() {
            system.register_callbacks();
        }
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

impl Load for GameSystems {
    fn load(&mut self, state: &SaveStateImpl) -> LoadResult {
        state.load(self)
    }

    fn post_load(&mut self, context: &PostLoadContext) {
        debug_assert!(self.generation != RESERVED_GENERATION);

        for entry in &mut self.systems {
            debug_assert!(entry.generation != RESERVED_GENERATION);
            debug_assert!(entry.generation < self.generation);

            entry.system.post_load(context);
        }

        // NOTE: Workaround for backwards compatibility with old saves
        // that might not have newly added game systems. Manually
        // instantiate any missing systems here.
        self.create_missing();
    }
}
