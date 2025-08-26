use crate::{
    imgui_ui::UiSystem
};

use super::{
    sim::{Query, world::GenerationalIndex}
};

pub mod settlers;

// ----------------------------------------------
// GameSystem
// ----------------------------------------------

pub trait GameSystem {
    fn update(&mut self, query: &Query);
    fn reset(&mut self) {}
    fn draw_debug_ui(&mut self, _query: &Query, _ui_sys: &UiSystem) {}
}

// ----------------------------------------------
// GameSystems
// ----------------------------------------------

pub type GameSystemId = GenerationalIndex;

pub struct GameSystems {
    systems: Vec<(Box<dyn GameSystem>, &'static str, u32)>, // (system, debug_name, generation)
    generation: u32,
}

impl GameSystems {
    pub fn new() -> Self {
        Self {
            systems: Vec::new(),
            generation: 0,
        }
    }

    pub fn register<System>(&mut self, name: &'static str, system: System) -> GameSystemId
        where System: GameSystem + 'static
    {
        let index = self.systems.len();
        let generation = self.generation;

        self.systems.push((Box::new(system), name, generation));
        self.generation += 1;

        GameSystemId::new(generation, index)
    }

    pub fn find(&self, sys_id: GameSystemId) -> Option<&dyn GameSystem> {
        if sys_id.index() < self.systems.len() {
            let (ref system, _, generation) = self.systems[sys_id.index()];
            if sys_id.generation() == generation {
                return Some(system.as_ref());
            }
        }
        None
    }

    pub fn find_mut(&mut self, sys_id: GameSystemId) -> Option<&mut dyn GameSystem> {
        if sys_id.index() < self.systems.len() {
            let (ref mut system, _, generation) = self.systems[sys_id.index()];
            if sys_id.generation() == generation {
                return Some(system.as_mut());
            }
        }
        None
    }

    pub fn update(&mut self, query: &Query) {
        for (system, _, _) in &mut self.systems {
            system.update(query);
        }
    }

    pub fn reset(&mut self) {
        for (system, _, _) in &mut self.systems {
            system.reset();
        }
    }

    pub fn draw_debug_ui(&mut self, query: &Query, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();
        if let Some(_tab_bar) = ui.tab_bar("Game Systems Tab Bar") {
            for (system, name, _) in &mut self.systems {
                if let Some(_tab) = ui.tab_item(name) {
                    system.draw_debug_ui(query, ui_sys);
                }
            }
        }
    }
}
