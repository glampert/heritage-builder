// The city's stock of construction materials: everything in its storage yards. Placing a building
// takes its materials from here at once (docs/plans/construction.md §2), so unlike walker
// deliveries it doesn't need the yards to be staffed.

use std::fmt;

use arrayvec::ArrayVec;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::{
    building::{Building, BuildingId, BuildingKind, construction::{ConstructionMaterials, MAX_CONSTRUCTION_MATERIALS}},
    cheats,
    sim::resources::{ResourceKind, StockItem},
    world::{World, object::GameObject},
};

// The buildings whose stock counts: storage yards. Granaries hold only food.
const CITY_STOCK_BUILDINGS: BuildingKind = BuildingKind::StorageYard;

// ----------------------------------------------
// MissingCost
// ----------------------------------------------

// What a placement lacks: gold and materials, each as the amount still needed.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct MissingCost {
    pub gold: u32,
    pub materials: ArrayVec<StockItem, MAX_CONSTRUCTION_MATERIALS>,
}

impl MissingCost {
    #[inline]
    pub fn is_only_gold(&self) -> bool {
        self.materials.is_empty()
    }
}

// "5 gold, 3 Bricks".
impl fmt::Display for MissingCost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;
        if self.gold != 0 {
            write!(f, "{} gold", self.gold)?;
            first = false;
        }
        for item in &self.materials {
            if !first {
                f.write_str(", ")?;
            }
            write!(f, "{} {}", item.count, item.kind.display_name())?;
            first = false;
        }
        Ok(())
    }
}

impl fmt::Debug for MissingCost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MissingCost({self})")
    }
}

// ----------------------------------------------
// Withdrawal
// ----------------------------------------------

// Which storage yard gave what, so a refund can put the materials back where they came from.
#[derive(Clone, Default, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Withdrawal {
    pub taken: SmallVec<[WithdrawnItem; 4]>,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct WithdrawnItem {
    pub storage_id: BuildingId,
    pub kind: ResourceKind,
    pub count: u32,
}

impl Withdrawal {
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.taken.is_empty()
    }

    // Total taken of `kind`, from every yard.
    pub fn count(&self, kind: ResourceKind) -> u32 {
        self.taken.iter().filter(|item| item.kind == kind).map(|item| item.count).sum()
    }
}

// ----------------------------------------------
// CityStock
// ----------------------------------------------

pub struct CityStock;

impl CityStock {
    // Units of `kind` in all storage yards.
    pub fn available(world: &World, kind: ResourceKind) -> u32 {
        debug_assert!(kind.is_single_resource());

        let mut count = 0;
        world.for_each_building(CITY_STOCK_BUILDINGS, |building| {
            if Self::is_stock_building(building) {
                count += building.as_storage().storage_slots.available_resources(kind);
            }
            true
        });
        count
    }

    // Room for `kind` in all storage yards: the most a deposit can add.
    pub fn room_for(world: &World, kind: ResourceKind) -> u32 {
        debug_assert!(kind.is_single_resource());

        let mut room = 0;
        world.for_each_building(CITY_STOCK_BUILDINGS, |building| {
            if Self::is_stock_building(building) {
                room += building.as_storage().storage_slots.receivable_resources(kind);
            }
            true
        });
        room
    }

    // Ok if the city has `gold` (as the world stats count it, tax offices included) and every
    // material; otherwise what's missing. The tile cost cheat makes everything affordable.
    pub fn can_afford(world: &World, gold: u32, materials: &ConstructionMaterials) -> Result<(), MissingCost> {
        if cheats::get().ignore_tile_cost {
            return Ok(());
        }

        let mut missing = MissingCost {
            gold: gold.saturating_sub(world.stats().treasury.gold_units_total),
            materials: ArrayVec::new(),
        };

        for item in materials.iter() {
            let available = Self::available(world, item.kind);
            if available < item.count {
                missing.materials.push(StockItem { kind: item.kind, count: item.count - available });
            }
        }

        if missing.gold == 0 && missing.materials.is_empty() { Ok(()) } else { Err(missing) }
    }

    // Takes `materials` from the storage yards, each kind from the first yards that hold it, in
    // pool order so it's the same on every run. Call can_afford first: a shortfall is taken as
    // far as it goes. The world stats are updated straight away.
    pub fn withdraw(world: &mut World, materials: &ConstructionMaterials) -> Withdrawal {
        let mut withdrawal = Withdrawal::default();

        for item in materials.iter() {
            let mut remaining = item.count;

            world.for_each_building_mut(CITY_STOCK_BUILDINGS, |building| {
                if Self::is_stock_building(building) {
                    let storage_id = building.id();
                    let taken = building.as_storage_mut().storage_slots.remove_resources(item.kind, remaining);
                    if taken != 0 {
                        withdrawal.taken.push(WithdrawnItem { storage_id, kind: item.kind, count: taken });
                        remaining -= taken;
                    }
                }
                remaining != 0 // Continue until satisfied.
            });

            debug_assert!(remaining == 0 || cheats::get().ignore_tile_cost, "Withdrew {} of {} {}.", item.count - remaining, item.count, item.kind);
            Self::subtract_from_stats(world, item.kind, item.count - remaining);
        }

        withdrawal
    }

    // Puts a withdrawal back into the yards it came from. What doesn't fit there, because a yard
    // was removed or filled up since, goes to any yard with room. Returns what was lost: the
    // materials no yard could take.
    pub fn refund(world: &mut World, withdrawal: &Withdrawal) -> ArrayVec<StockItem, MAX_CONSTRUCTION_MATERIALS> {
        let mut leftover = ConstructionMaterialsAccumulator::default();

        for item in &withdrawal.taken {
            let mut returned = 0;
            if let Some(building) = world.find_building_mut(CITY_STOCK_BUILDINGS, item.storage_id) {
                if Self::is_stock_building(building) {
                    returned = building.as_storage_mut().storage_slots.receive_resources(item.kind, item.count);
                }
            }
            Self::add_to_stats(world, item.kind, returned);
            leftover.add(item.kind, item.count - returned);
        }

        let mut lost = ArrayVec::new();
        for item in leftover.items {
            let unplaced = Self::deposit_resource(world, item.kind, item.count);
            if unplaced != 0 {
                lost.push(StockItem { kind: item.kind, count: unplaced });
            }
        }
        lost
    }

    // Puts `materials` into any yards with room, in pool order. Returns what didn't fit.
    pub fn deposit(world: &mut World, materials: &ConstructionMaterials) -> ArrayVec<StockItem, MAX_CONSTRUCTION_MATERIALS> {
        let mut lost = ArrayVec::new();
        for item in materials.iter() {
            let unplaced = Self::deposit_resource(world, item.kind, item.count);
            if unplaced != 0 {
                lost.push(StockItem { kind: item.kind, count: unplaced });
            }
        }
        lost
    }

    // ----------------------
    // Internal:
    // ----------------------

    // Every finished storage yard counts, staffed or not.
    #[inline]
    fn is_stock_building(building: &Building) -> bool {
        building.is(CITY_STOCK_BUILDINGS) && !building.is_under_construction()
    }

    // Puts `count` units of `kind` into any yards with room, in pool order. Returns the count
    // that didn't fit anywhere.
    pub fn deposit_resource(world: &mut World, kind: ResourceKind, count: u32) -> u32 {
        debug_assert!(kind.is_single_resource());
        let mut remaining = count;
        if remaining != 0 {
            world.for_each_building_mut(CITY_STOCK_BUILDINGS, |building| {
                if Self::is_stock_building(building) {
                    remaining -= building.as_storage_mut().storage_slots.receive_resources(kind, remaining);
                }
                remaining != 0
            });
        }
        Self::add_to_stats(world, kind, count - remaining);
        remaining
    }

    // The world stats are recounted on each simulation step; these keep them right in between,
    // so the HUD and the next placement see the change at once.
    fn subtract_from_stats(world: &mut World, kind: ResourceKind, count: u32) {
        if count != 0 {
            let resources = &mut world.stats_mut().resources;
            resources.storage_yards.remove(kind, count);
            resources.all.remove(kind, count);
        }
    }

    fn add_to_stats(world: &mut World, kind: ResourceKind, count: u32) {
        if count != 0 {
            world.stats_mut().add_storage_yard_resources(kind, count);
        }
    }
}

// Sums counts per kind, in first-seen order.
#[derive(Default)]
struct ConstructionMaterialsAccumulator {
    items: ArrayVec<StockItem, MAX_CONSTRUCTION_MATERIALS>,
}

impl ConstructionMaterialsAccumulator {
    fn add(&mut self, kind: ResourceKind, count: u32) {
        if count == 0 {
            return;
        }
        if let Some(item) = self.items.iter_mut().find(|item| item.kind == kind) {
            item.count += count;
        } else {
            self.items.push(StockItem { kind, count });
        }
    }
}
