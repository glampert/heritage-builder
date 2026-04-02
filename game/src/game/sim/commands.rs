use super::SimContext;
use crate::{
    log,
    utils::coords::Cell,
    tile::sets::TileDef,
    game::{
        prop::PropId,
        world::object::Spawner,
        unit::{Unit, UnitId, config::UnitConfigKey},
        building::{Building, BuildingKindAndId},
    },
};

// ----------------------------------------------
// SimCmd
// ----------------------------------------------

enum SimCmd {
    // -- Unit operations -----------------------

    SpawnUnitWithConfig {
        origin: Cell,
        config: UnitConfigKey,
    },

    SpawnUnitWithTileDef {
        origin: Cell,
        tile_def: &'static TileDef,
    },

    DespawnUnitWithId {
        id: UnitId,
    },

    // -- Building operations -------------------

    SpawnBuildingWithTileDef {
        base_cell: Cell,
        tile_def: &'static TileDef,
    },

    DespawnBuildingWithId {
        kind_and_id: BuildingKindAndId,
    },

    // -- Prop operations -----------------------

    SpawnPropWithTileDef {
        origin: Cell,
        tile_def: &'static TileDef,
    },

    DespawnPropWithId {
        id: PropId,
    },

    // TODO: Add other commands

    /*
    // -- Cross-entity interactions -------------

    /// Queue a `Building::visited_by(unit)` call.
    ///
    /// Resolved in the apply phase where `&mut Unit` and `&mut Building` can be
    /// obtained without borrow-checker conflicts (they live in separate pools).
    VisitBuilding {
        unit_id: UnitId,
        building: BuildingKindAndId,
    },

    // -- Building upgrades ---------------------

    /// Attempt to expand and upgrade a house by one level.
    UpgradeHouse { id: BuildingId },

    // -- Economy -------------------------------

    /// Add gold to the global treasury.
    AddGold(u32),
    /// Remove gold from the global treasury (clamped to 0).
    RemoveGold(u32),
    */
}

// ----------------------------------------------
// SimCmds
// ----------------------------------------------

const SIM_CMDS_INITIAL_CAPACITY: usize = 64;

// Deferred command buffer populated during simulation updates.
// Any world or tile map modification is done via a deferred command.
// Commands are applied after all game objects have been updated.
pub struct SimCmds {
    cmds: Vec<SimCmd>,
}

impl SimCmds {
    pub fn new() -> Self {
        Self { cmds: Vec::with_capacity(SIM_CMDS_INITIAL_CAPACITY) }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.cmds.is_empty()
    }

    // -- Unit operations -----------------------

    #[inline]
    pub fn spawn_unit_with_config(&mut self, origin: Cell, config: UnitConfigKey) {
        self.cmds.push(SimCmd::SpawnUnitWithConfig { origin, config });
    }

    #[inline]
    pub fn spawn_unit_with_tile_def(&mut self, origin: Cell, tile_def: &'static TileDef) {
        self.cmds.push(SimCmd::SpawnUnitWithTileDef { origin, tile_def });
    }

    #[inline]
    pub fn despawn_unit_with_id(&mut self, id: UnitId) {
        self.cmds.push(SimCmd::DespawnUnitWithId { id });
    }

    // -- Building operations -------------------

    #[inline]
    pub fn spawn_building_with_tile_def(&mut self, base_cell: Cell, tile_def: &'static TileDef) {
        self.cmds.push(SimCmd::SpawnBuildingWithTileDef { base_cell, tile_def });
    }

    #[inline]
    pub fn despawn_building_with_id(&mut self, kind_and_id: BuildingKindAndId) {
        self.cmds.push(SimCmd::DespawnBuildingWithId { kind_and_id });
    }

    // -- Prop operations -----------------------

    #[inline]
    pub fn spawn_prop_with_tile_def(&mut self, origin: Cell, tile_def: &'static TileDef) {
        self.cmds.push(SimCmd::SpawnPropWithTileDef { origin, tile_def });
    }

    #[inline]
    pub fn despawn_prop_with_id(&mut self, id: PropId) {
        self.cmds.push(SimCmd::DespawnPropWithId { id });
    }

    /*
    // ── Cross-entity interactions ────────────────────────────────────

    /// Queue a `Building::visited_by(unit)` call.
    ///
    /// Use this whenever a task needs to interact with a building while it
    /// already holds `&mut Unit`. The apply phase acquires both refs safely
    /// since units and buildings live in separate, non-overlapping pools.
    #[inline]
    pub fn visit_building(&mut self, unit_id: UnitId, building: BuildingKindAndId) {
        self.cmds.push(SimCmd::VisitBuilding { unit_id, building });
    }

    // ── Building upgrades ────────────────────────────────────────────

    /// Attempt to expand and upgrade the house with `id` by one level.
    #[inline]
    pub fn upgrade_house(&mut self, id: BuildingId) {
        self.cmds.push(SimCmd::UpgradeHouse { id });
    }

    // ── Economy ─────────────────────────────────────────────────────

    /// Add `amount` gold to the global treasury.
    #[inline]
    pub fn add_gold(&mut self, amount: u32) {
        self.cmds.push(SimCmd::AddGold(amount));
    }

    /// Remove `amount` gold from the global treasury (clamped to 0).
    #[inline]
    pub fn remove_gold(&mut self, amount: u32) {
        self.cmds.push(SimCmd::RemoveGold(amount));
    }

    // ────────────────────────────────────────────────────────────────
    */

    /// Apply all queued commands.
    ///
    /// Called by `Simulation::update()` after every entity has been updated for
    /// this tick. `context` must have been freshly created for this frame so
    /// that its internal `world_mut()` / `tile_map_mut()` pointers are the only
    /// live mutable handles into the simulation state.

    pub fn execute(self, context: &SimContext) {
        if self.cmds.is_empty() {
            return;
        }

        let spawner = Spawner::new(context);

        for cmd in self.cmds {
            Self::execute_cmd(cmd, &spawner);
        }
    }

    fn execute_cmd(cmd: SimCmd, spawner: &Spawner) {
        match cmd {
            //
            // Units:
            //
            SimCmd::SpawnUnitWithConfig { origin, config } => {
                /*
                if let Err(err) = context.world_mut()
                                            .try_spawn_unit_with_config(context, origin, config)
                {
                    log::error!(log::channel!("sim_cmds"),
                                "SpawnUnit failed @ {origin}: {}", err.message);
                }
                */
            }
            SimCmd::SpawnUnitWithTileDef { origin, tile_def } => {
                /*
                if let Err(err) = context.world_mut()
                                            .try_spawn_unit_with_tile_def(context, origin, tile_def)
                {
                    log::error!(log::channel!("sim_cmds"),
                                "SpawnUnitWithTileDef '{}' failed @ {origin}: {}",
                                tile_def.name, err.message);
                }
                */
            }
            SimCmd::DespawnUnitWithId { id } => {
                /*
                // SAFETY: we obtain a raw pointer to the unit *before* calling
                // despawn so the borrow from find_unit_mut ends. The unit pool
                // and the rest of World do not overlap, so no aliasing occurs.
                let ptr = context.world_mut()
                                    .find_unit_mut(id)
                                    .map(|u| u as *mut Unit);
                if let Some(ptr) = ptr {
                    Spawner::new(context).despawn_unit(unsafe { &mut *ptr });
                }
                */
            }
            //
            // Buildings:
            //
            SimCmd::SpawnBuildingWithTileDef { base_cell, tile_def } => {
                /*
                if let Err(err) = context.world_mut()
                                            .try_spawn_building_with_tile_def(context, cell, tile_def)
                {
                    log::error!(log::channel!("sim_cmds"),
                                "SpawnBuilding '{}' failed @ {cell}: {}",
                                tile_def.name, err.message);
                }
                */
            }
            SimCmd::DespawnBuildingWithId { kind_and_id } => {
                /*
                let ptr = context.world_mut()
                                    .find_building_mut(kind, id)
                                    .map(|b| b as *mut Building);
                if let Some(ptr) = ptr {
                    Spawner::new(context).despawn_building(unsafe { &mut *ptr });
                }
                */
            }
            //
            // Props:
            //
            SimCmd::SpawnPropWithTileDef { origin, tile_def } => {
                /*
                if let Err(err) = context.world_mut()
                                            .try_spawn_prop_with_tile_def(context, cell, tile_def)
                {
                    log::error!(log::channel!("sim_cmds"),
                                "SpawnProp '{}' failed @ {cell}: {}",
                                tile_def.name, err.message);
                }
                */
            }
            SimCmd::DespawnPropWithId { id } => {
                /*
                let ptr = context.world_mut()
                                    .find_prop_mut(id)
                                    .map(|p| p as *mut _);
                if let Some(ptr) = ptr {
                    Spawner::new(context).despawn_prop(unsafe { &mut *ptr });
                }
                */
            }

            /* 
            // ── Cross-entity ─────────────────────────────────────
            SimCmd::VisitBuilding { unit_id, building } => {
                // SAFETY: units live in `unit_spawn_pool` and buildings live
                // in `building_spawn_pools`. These are distinct, non-overlapping
                // fields of `World`, so the two mutable borrows do not alias.
                let world = context.world_mut();
                let unit_ptr = world.find_unit_mut(unit_id)
                                    .map(|u| u as *mut Unit);
                if let Some(unit_ptr) = unit_ptr {
                    if let Some(bldg) = world.find_building_mut(building.kind, building.id) {
                        bldg.visited_by(unsafe { &mut *unit_ptr }, context);
                    }
                }
            }

            // ── Building upgrades ────────────────────────────────
            SimCmd::UpgradeHouse { id } => {
                //crate::game::building::house_upgrade::try_upgrade_by_id(id, context);
            }

            // ── Economy ─────────────────────────────────────────
            SimCmd::AddGold(amount) => {
                context.treasury_mut().add_gold_units(amount);
            }
            SimCmd::RemoveGold(amount) => {
                context.treasury_mut().subtract_gold_units(amount);
            }
            */
        }
    }
}
