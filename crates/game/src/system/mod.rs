use std::any::{Any, TypeId};

use common::mem;
use engine::{
    Engine,
    log,
    save::*,
};
use enum_dispatch::enum_dispatch;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumCount, EnumIter, IntoEnumIterator, VariantNames};

use super::{
    constants::*,
    sim::{SimCmds, SimContext, calendar::{CalendarDate, CalendarEvent, Season}},
    world::object::GenerationalIndex,
};
use crate::save_context::*;

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

pub mod campaign;
use self::campaign::CampaignSystem;

// ----------------------------------------------
// GameSystem
// ----------------------------------------------

#[enum_dispatch(GameSystemImpl)]
pub trait GameSystem: Any {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        // Reuse the non-mutable method to avoid duplication.
        mem::mut_ref_cast(self.as_any())
    }

    // Required overrides:
    fn as_any(&self) -> &dyn Any;
    fn update(&mut self, engine: &mut Engine, cmds: &mut SimCmds, context: &SimContext);

    // Optional overrides:
    fn paused_update(&mut self, _engine: &mut Engine, _context: &SimContext) {}

    // Calendar events, after the world step's update that crossed into the new month. When one
    // month boundary brings several, they come in this order: month, season, year.
    fn on_new_month(&mut self, _engine: &mut Engine, _cmds: &mut SimCmds, _context: &SimContext, _date: CalendarDate) {}
    fn on_new_season(&mut self, _engine: &mut Engine, _cmds: &mut SimCmds, _context: &SimContext, _season: Season) {}
    fn on_new_year(&mut self, _engine: &mut Engine, _cmds: &mut SimCmds, _context: &SimContext, _year: u32) {}
    fn reset(&mut self, _engine: &mut Engine) {}
    fn post_load(&mut self, _context: &mut PostLoadContext) {}
    fn draw_debug_ui(&mut self, _engine: &mut Engine, _cmds: &mut SimCmds, _context: &SimContext) {}
    fn register_callbacks(&self) {}
}

#[enum_dispatch]
#[derive(EnumCount, EnumIter, VariantNames, Display, Serialize, Deserialize)]
#[allow(clippy::enum_variant_names)]
pub enum GameSystemImpl {
    SettlersSpawnSystem,
    AmbientEffectsSystem,
    AmbientMusicSystem,
    AmbientSoundsSystem,
    CampaignSystem,
}

// ----------------------------------------------
// GameSystems
// ----------------------------------------------

pub type GameSystemId = GenerationalIndex;

#[derive(Serialize, Deserialize)]
pub(crate) struct GameSystemEntry {
    pub(crate) system: GameSystemImpl,
    generation: u32,
}

#[derive(Serialize, Deserialize)]
pub struct GameSystems {
    pub(crate) systems: Vec<GameSystemEntry>,
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
    where
        System: GameSystem + 'static,
        GameSystemImpl: From<System>,
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
    where
        System: GameSystem + 'static,
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
    where
        System: GameSystem + 'static,
    {
        if sys_id.index() < self.systems.len() {
            let entry = &mut self.systems[sys_id.index()];
            if sys_id.generation() == entry.generation {
                return entry.system.as_any_mut().downcast_mut::<System>();
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
    pub fn update(&mut self, engine: &mut Engine, cmds: &mut SimCmds, context: &SimContext) {
        for entry in &mut self.systems {
            entry.system.update(engine, cmds, context);
        }
    }

    // Broadcasts the calendar events of a world step to every system, in order.
    pub fn on_calendar_events(&mut self, engine: &mut Engine, cmds: &mut SimCmds, context: &SimContext, events: &[CalendarEvent]) {
        for event in events {
            match *event {
                CalendarEvent::NewMonth(date) => {
                    log::verbose!(log::channel!("calendar"), "New month: {}, {}.", date.month, context.calendar().year_name());
                    for entry in &mut self.systems {
                        entry.system.on_new_month(engine, cmds, context, date);
                    }
                }
                CalendarEvent::NewSeason(season) => {
                    log::info!(log::channel!("calendar"), "The {} season begins.", season.to_string().to_lowercase());
                    for entry in &mut self.systems {
                        entry.system.on_new_season(engine, cmds, context, season);
                    }
                }
                CalendarEvent::NewYear(year) => {
                    log::info!(log::channel!("calendar"), "New year: {} (World Year {year}).", context.calendar().year_name());
                    for entry in &mut self.systems {
                        entry.system.on_new_year(engine, cmds, context, year);
                    }
                }
            }
        }
    }

    // Update called every simulation tick *only when the game IS paused*.
    pub fn paused_update(&mut self, engine: &mut Engine, context: &SimContext) {
        for entry in &mut self.systems {
            entry.system.paused_update(engine, context);
        }
    }

    pub fn reset(&mut self, engine: &mut Engine) {
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

    // NOTE: `GameSystems::draw_debug_ui` lives in `crate::debug::systems`.

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

    fn post_load(&mut self, context: &mut PostLoadContext) {
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
