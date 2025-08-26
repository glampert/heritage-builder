use crate::{
    imgui_ui::UiSystem
};

use super::{
    sim::{Query, world::GenerationalIndex}
};

pub type GameSystemId = GenerationalIndex;

// ----------------------------------------------
// GameSystem
// ----------------------------------------------

pub trait GameSystem {
    fn update(&mut self, query: &Query);
    fn reset(&mut self);
    fn draw_debug_ui(&mut self, query: &Query, ui_sys: &UiSystem);
}

// ----------------------------------------------
// GameSystems
// ----------------------------------------------

pub struct GameSystems {
    systems: Vec<(Box<dyn GameSystem>, u32)>, // (system, generation)
    generation: u32,
}

impl GameSystems {
    pub fn new() -> Self {
        Self {
            systems: Vec::new(),
            generation: 0,
        }
    }

    pub fn register<System>(&mut self, system: System) -> GameSystemId
        where System: GameSystem + 'static
    {
        let index = self.systems.len();
        let generation = self.generation;

        self.systems.push((Box::new(system), generation));
        self.generation += 1;

        GameSystemId::new(generation, index)
    }

    pub fn find(&self, sys_id: GameSystemId) -> Option<&dyn GameSystem> {
        if sys_id.index() < self.systems.len() {
            let (ref system, generation) = self.systems[sys_id.index()];
            if sys_id.generation() == generation {
                return Some(system.as_ref());
            }
        }
        None
    }

    pub fn find_mut(&mut self, sys_id: GameSystemId) -> Option<&mut dyn GameSystem> {
        if sys_id.index() < self.systems.len() {
            let (ref mut system, generation) = self.systems[sys_id.index()];
            if sys_id.generation() == generation {
                return Some(system.as_mut());
            }
        }
        None
    }

    pub fn update(&mut self, query: &Query) {
        for (system, _) in &mut self.systems {
            system.update(query);
        }
    }

    pub fn reset(&mut self) {
        for (system, _) in &mut self.systems {
            system.reset();
        }
    }

    pub fn draw_debug_ui(&mut self, query: &Query, ui_sys: &UiSystem) {
        for (system, _) in &mut self.systems {
            system.draw_debug_ui(query, ui_sys);
        }
    }
}
