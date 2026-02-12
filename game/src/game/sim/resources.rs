#![allow(clippy::blocks_in_conditions)]

use core::slice::Iter;
use std::{
    fmt::Display,
    ops::{Deref, DerefMut},
    collections::{hash_map::Entry, HashMap},
};

use arrayvec::ArrayVec;
use smallvec::SmallVec;
use bitflags::{bitflags, Flags};
use proc_macros::DrawDebugUi;
use rand::{seq::IteratorRandom, Rng};
use serde::{de, Deserialize, Deserializer, Serialize};

use crate::{
    bitflags_with_display,
    log,
    utils::Color,
    ui::UiSystem,
    game::{
        cheats,
        world::{object::GameObject, stats::WorldStats, World},
        building::{Building, BuildingId, BuildingKind, BuildingKindAndId},
    },
};

// ----------------------------------------------
// Resources (Consumer Goods, Raw Materials)
// ----------------------------------------------

bitflags_with_display! {
    #[derive(Copy, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
    pub struct ResourceKind: u32 {
        // Foods:
        const Rice    = 1 << 0;
        const Meat    = 1 << 1;
        const Fish    = 1 << 2;

        // Consumer Goods:
        const Wine    = 1 << 3;
        const Pottery = 1 << 4;

        // Raw materials:
        const Wood    = 1 << 5;
        const Metal   = 1 << 6;
        const Clay    = 1 << 7;
        const Bricks  = 1 << 8;

        // Gold (used as currency only):
        const Gold    = 1 << 9;
    }
}

impl ResourceKind {
    #[inline]
    pub const fn is_single_resource(self) -> bool {
        self.bits().count_ones() == 1
    }

    #[inline]
    pub const fn count() -> usize {
        Self::FLAGS.len()
    }

    #[inline]
    pub const fn foods() -> Self {
        Self::from_bits_retain(Self::Rice.bits() | Self::Meat.bits() | Self::Fish.bits())
    }

    #[inline]
    pub const fn consumer_goods() -> Self {
        Self::from_bits_retain(Self::Wine.bits() | Self::Pottery.bits())
    }

    #[inline]
    pub const fn raw_materials() -> Self {
        Self::from_bits_retain(Self::Wood.bits() | Self::Metal.bits() | Self::Clay.bits() | Self::Bricks.bits())
    }

    #[inline]
    pub fn random<R: Rng>(rng: &mut R) -> Self {
        Self::all().iter().choose(rng).unwrap_or(ResourceKind::Rice)
    }

    #[inline]
    pub fn random_food<R: Rng>(rng: &mut R) -> Self {
        Self::foods().iter().choose(rng).unwrap_or(ResourceKind::Rice)
    }

    #[inline]
    pub fn random_consumer_good<R: Rng>(rng: &mut R) -> Self {
        Self::consumer_goods().iter().choose(rng).unwrap_or(ResourceKind::Wine)
    }

    #[inline]
    pub fn random_raw_material<R: Rng>(rng: &mut R) -> Self {
        Self::raw_materials().iter().choose(rng).unwrap_or(ResourceKind::Wood)
    }
}

pub const RESOURCE_KIND_COUNT: usize = ResourceKind::count();
pub type ResourceKinds = ResourceList<ResourceKind, RESOURCE_KIND_COUNT>;

// ----------------------------------------------
// Services
// ----------------------------------------------

pub type ServiceKind = BuildingKind;

pub const SERVICE_KIND_COUNT: usize = ServiceKind::services_count();
pub type ServiceKinds = ResourceList<ServiceKind, SERVICE_KIND_COUNT>;

// ----------------------------------------------
// Custom Serde Serialization for HashMap
// ----------------------------------------------

// serde_json does not support maps indexed by structure (json maps must be
// keyed by a string). This converts a map into a Vec of pairs when serializing
// and builds back a map during deserialization.
mod serialize_hash_map_as_pairs {
    use std::{collections::HashMap, hash::Hash};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<K, V, S>(map: &HashMap<K, V>, ser: S) -> Result<S::Ok, S::Error>
        where K: Serialize + Eq + Hash,
              V: Serialize,
              S: Serializer
    {
        // Turn into Vec of pairs and serialize that:
        let vec: Vec<(&K, &V)> = map.iter().collect();
        vec.serialize(ser)
    }

    pub fn deserialize<'de, K, V, D>(de: D) -> Result<HashMap<K, V>, D::Error>
        where K: Deserialize<'de> + Eq + Hash,
              V: Deserialize<'de>,
              D: Deserializer<'de>
    {
        let vec = Vec::<(K, V)>::deserialize(de)?;
        Ok(vec.into_iter().collect())
    }
}

// ----------------------------------------------
// HouseholdWorkerPool
// ----------------------------------------------

#[derive(Clone, Serialize, Deserialize)]
pub struct HouseholdWorkerPool {
    // Total workers = employed + unemployed
    employed_count: u32,
    unemployed_count: u32,

    // NOTE: Convert to Vec of pairs when serializing to support json format.
    #[serde(with = "serialize_hash_map_as_pairs")]
    employers: HashMap<BuildingKindAndId, u32>,
}

impl HouseholdWorkerPool {
    #[inline]
    pub fn new(employed_count: u32, unemployed_count: u32) -> Self {
        Self { employed_count, unemployed_count, employers: HashMap::new() }
    }

    #[inline]
    pub fn employed_count(&self) -> u32 {
        self.employed_count
    }

    #[inline]
    pub fn unemployed_count(&self) -> u32 {
        self.unemployed_count
    }

    #[inline]
    pub fn total_workers(&self) -> u32 {
        self.employed_count + self.unemployed_count
    }

    #[inline]
    pub fn set_counts(&mut self, new_employed: u32, new_unemployed: u32) {
        self.employed_count = new_employed;
        self.unemployed_count = new_unemployed;
    }

    pub fn for_each_employer<F>(&self, world: &mut World, mut visitor_fn: F)
        where F: FnMut(&mut Building, u32) -> bool
    {
        for (employer_info, employed_count) in &self.employers {
            if let Some(employer) = world.find_building_mut(employer_info.kind, employer_info.id) {
                if !visitor_fn(employer, *employed_count) {
                    return;
                }
            } else {
                log::error!(log::channel!("Household"),
                            "Unknown employer record: ({}, {}); employed_count={}",
                            employer_info.kind,
                            employer_info.id,
                            employed_count);
            }
        }
    }

    pub fn for_each_employer_mut<F>(&mut self, world: &mut World, mut visitor_fn: F)
        where F: FnMut(&mut Building, &mut u32) -> bool
    {
        for (employer_info, employed_count) in &mut self.employers {
            if let Some(employer) = world.find_building_mut(employer_info.kind, employer_info.id) {
                let mut count = *employed_count;
                let should_continue = visitor_fn(employer, &mut count);
                *employed_count = count;
                if !should_continue {
                    break;
                }
            } else {
                log::error!(log::channel!("Household"),
                            "Unknown employer record: ({}, {}); employed_count={}",
                            employer_info.kind,
                            employer_info.id,
                            employed_count);
            }
        }

        self.employers.retain(|_key, val| *val != 0);
    }

    pub fn clear(&mut self) {
        let total = self.total_workers();
        self.employed_count = 0;
        self.unemployed_count = total;
        self.employers.clear();
    }

    pub fn add_unemployed(&mut self, amount: u32, source: BuildingKindAndId) -> u32 {
        debug_assert!(amount != 0);
        debug_assert!(source.is_valid());

        let prev_employed = self.employed_count;
        let new_employed = prev_employed.saturating_sub(amount);

        let prev_unemployed = self.unemployed_count;
        let new_unemployed = prev_unemployed + amount;

        if new_employed + new_unemployed <= prev_employed + prev_unemployed {
            self.employed_count = new_employed;
            self.unemployed_count = new_unemployed;

            let unemployed_amount = new_unemployed - prev_unemployed;

            if let Entry::Occupied(mut e) = self.employers.entry(source) {
                if {
                    *e.get_mut() = e.get().saturating_sub(unemployed_amount);
                    *e.get() == 0
                } {
                    e.remove_entry();
                }
            } else {
                log::error!(log::channel!("Household"),
                            "Expected to have an entry for ({}, {}); add({})",
                            source.kind, source.id, amount);
            }

            return unemployed_amount; // Return amount added to unemployed count.
        }
        0
    }

    pub fn remove_unemployed(&mut self, amount: u32, source: BuildingKindAndId) -> u32 {
        debug_assert!(amount != 0);
        debug_assert!(source.is_valid());

        let prev_employed = self.employed_count;
        let new_employed = prev_employed + amount;

        let prev_unemployed = self.unemployed_count;
        let new_unemployed = prev_unemployed.saturating_sub(amount);

        if new_employed + new_unemployed <= prev_employed + prev_unemployed {
            self.employed_count = new_employed;
            self.unemployed_count = new_unemployed;

            let employed_amount = prev_unemployed - new_unemployed;

            if employed_amount != 0 {
                *self.employers.entry(source).or_insert(0) += employed_amount;
            }

            return employed_amount; // Return amount taken from unemployed
                                    // count. i.e., amount employed.
        }
        0
    }

    pub fn merge(&mut self, other: &HouseholdWorkerPool) -> bool {
        for (employer_info, employed_count) in &other.employers {
            let merged_count = self.remove_unemployed(*employed_count, *employer_info);
            if merged_count != *employed_count {
                log::error!("HouseholdWorkerPool merge exceeds workers available! Unemployed: {}, trying to merge: {}, merged only: {}",
                            self.unemployed_count(), *employed_count, merged_count);
                return false;
            }
        }
        true
    }

    pub fn draw_debug_ui(&self, world: &World, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();
        ui.text(format!("Employed   : {}", self.employed_count));
        ui.text(format!("Unemployed : {}", self.unemployed_count));
        ui.text(format!("Total      : {}", self.total_workers()));

        if !self.employers.is_empty() {
            ui.text("Employers:");
            ui.indent_by(10.0);

            for (employer_info, employed_count) in &self.employers {
                if let Some(employer) = world.find_building(employer_info.kind, employer_info.id) {
                    ui.text(format!("- {} cell={} id={}: {}",
                                    employer.name(),
                                    employer.base_cell(),
                                    employer.id(),
                                    employed_count));
                } else {
                    ui.text_colored(Color::red().to_array(), "<unknown employer record>");
                }
            }

            ui.unindent_by(10.0);
        }
    }
}

// ----------------------------------------------
// Employer
// ----------------------------------------------

#[derive(Clone, Serialize, Deserialize)]
pub struct Employer {
    employee_count: u32, // Current number of workers employed.
    min_employees: u32,  // Minimum number of workers for service/production to run (at lower capacity).
    max_employees: u32,  // Maximum number of workers it can employ (to run at full capacity).

    // NOTE: Convert to Vec of pairs when serializing to support json format.
    #[serde(with = "serialize_hash_map_as_pairs")]
    employee_households: HashMap<BuildingId, u32>, // Households we source our workers from.
}

impl Employer {
    #[inline]
    pub fn new(min_employees: u32, max_employees: u32) -> Self {
        debug_assert!(min_employees <= max_employees);
        Self { employee_count: 0,
               min_employees,
               max_employees,
               employee_households: HashMap::new() }
    }

    #[inline]
    pub fn employee_count(&self) -> u32 {
        self.employee_count
    }

    #[inline]
    pub fn min_employees(&self) -> u32 {
        self.min_employees
    }

    #[inline]
    pub fn max_employees(&self) -> u32 {
        self.max_employees
    }

    #[inline]
    pub fn is_at_max_capacity(&self) -> bool {
        self.employee_count == self.max_employees
    }

    #[inline]
    pub fn is_below_min_required(&self) -> bool {
        self.employee_count < self.min_employees
    }

    #[inline]
    pub fn has_min_required(&self) -> bool {
        self.employee_count >= self.min_employees
    }

    pub fn for_each_employee_household<F>(&self, world: &mut World, mut visitor_fn: F)
        where F: FnMut(&mut Building, u32) -> bool
    {
        for (house_id, employee_count) in &self.employee_households {
            if let Some(house) = world.find_building_mut(BuildingKind::House, *house_id) {
                if !visitor_fn(house, *employee_count) {
                    return;
                }
            } else {
                log::error!(log::channel!("Employer"),
                            "Unknown employee household: {house_id}; employee_count={employee_count}");
            }
        }
    }

    pub fn for_each_employee_household_mut<F>(&mut self, world: &mut World, mut visitor_fn: F)
        where F: FnMut(&mut Building, &mut u32) -> bool
    {
        for (house_id, employee_count) in &mut self.employee_households {
            if let Some(house) = world.find_building_mut(BuildingKind::House, *house_id) {
                let mut count = *employee_count;
                let should_continue = visitor_fn(house, &mut count);
                *employee_count = count;
                if !should_continue {
                    break;
                }
            } else {
                log::error!(log::channel!("Employer"),
                            "Unknown employee household: {house_id}; employee_count={employee_count}");
            }
        }

        self.employee_households.retain(|_key, val| *val != 0);
    }

    pub fn clear(&mut self) {
        self.employee_count = 0;
        self.employee_households.clear();
    }

    pub fn add_employee(&mut self, amount: u32, source: BuildingKindAndId) -> u32 {
        debug_assert!(amount != 0);
        debug_assert!(source.is_valid());
        debug_assert!(source.kind == BuildingKind::House);

        let prev_count = self.employee_count;
        let new_count = (prev_count + amount).min(self.max_employees);
        self.employee_count = new_count;

        let employed_amount = new_count - prev_count;

        if employed_amount != 0 {
            *self.employee_households.entry(source.id).or_insert(0) += employed_amount;
        }

        employed_amount // Return amount added to employees.
    }

    pub fn remove_employee(&mut self, amount: u32, source: BuildingKindAndId) -> u32 {
        debug_assert!(amount != 0);
        debug_assert!(source.is_valid());
        debug_assert!(source.kind == BuildingKind::House);

        let prev_count = self.employee_count;
        let new_count = prev_count.saturating_sub(amount);
        self.employee_count = new_count;

        let unemployed_amount = prev_count - new_count;

        if let Entry::Occupied(mut e) = self.employee_households.entry(source.id) {
            if {
                *e.get_mut() = e.get().saturating_sub(unemployed_amount);
                *e.get() == 0
            } {
                e.remove_entry();
            }
        } else {
            log::error!(log::channel!("Employer"),
                        "Expected to have an entry for ({}, {}); subtract({})",
                        source.kind, source.id, amount);
        }

        unemployed_amount // Return amount subtracted from employees.
    }

    pub fn merge(&mut self, other: &Employer) -> bool {
        for (house_id, employee_count) in &other.employee_households {
            let merged_count = self.add_employee(*employee_count,
                                                 BuildingKindAndId { kind: BuildingKind::House,
                                                                     id: *house_id });

            if merged_count != *employee_count {
                log::error!("Employer merge exceeds maximum! Max employees: {}, trying to merge: {}, merged only: {}",
                            self.max_employees(), *employee_count, merged_count);
                return false;
            }
        }
        true
    }

    pub fn draw_debug_ui(&self, world: &World, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();
        ui.text(format!("Workers Employed : {}", self.employee_count));
        ui.text(format!("Min Required     : {}", self.min_employees));
        ui.text(format!("Max Employed     : {}", self.max_employees));

        if cheats::get().ignore_worker_requirements {
            ui.text_colored(Color::green().to_array(), "CHEAT ignore_worker_requirements ON");
        } else if self.is_below_min_required() {
            ui.text_colored(Color::red().to_array(), "Below Min Required Workers");
        } else if self.is_at_max_capacity() {
            ui.text_colored(Color::green().to_array(), "Has All Required Workers");
        }

        if !self.employee_households.is_empty() {
            ui.text("Worker Households:");
            ui.indent_by(10.0);

            for (house_id, employee_count) in &self.employee_households {
                if let Some(house) = world.find_building(BuildingKind::House, *house_id) {
                    ui.text(format!("- {} cell={} id={}: {}",
                                    house.name(),
                                    house.base_cell(),
                                    house.id(),
                                    employee_count));
                } else {
                    ui.text_colored(Color::red().to_array(), "<unknown employee household>");
                }
            }

            ui.unindent_by(10.0);
        }
    }
}

// ----------------------------------------------
// Workers
// ----------------------------------------------

#[derive(Clone, Serialize, Deserialize)]
pub enum Workers {
    HouseholdWorkerPool(HouseholdWorkerPool),
    Employer(Employer),
}

impl Workers {
    // Household Worker Pool:
    pub fn household_worker_pool(employed_count: u32, unemployed_count: u32) -> Self {
        Self::HouseholdWorkerPool(HouseholdWorkerPool::new(employed_count, unemployed_count))
    }

    #[inline]
    pub fn is_household_worker_pool(&self) -> bool {
        matches!(self, Self::HouseholdWorkerPool(_))
    }

    #[inline]
    pub fn as_household_worker_pool(&self) -> Option<&HouseholdWorkerPool> {
        match self {
            Self::HouseholdWorkerPool(inner) => Some(inner),
            _ => None,
        }
    }

    #[inline]
    pub fn as_household_worker_pool_mut(&mut self) -> Option<&mut HouseholdWorkerPool> {
        match self {
            Self::HouseholdWorkerPool(inner) => Some(inner),
            _ => None,
        }
    }

    // Employer:
    pub fn employer(min_employees: u32, max_employees: u32) -> Self {
        Self::Employer(Employer::new(min_employees, max_employees))
    }

    #[inline]
    pub fn is_employer(&self) -> bool {
        matches!(self, Self::Employer(_))
    }

    #[inline]
    pub fn as_employer(&self) -> Option<&Employer> {
        match self {
            Self::Employer(inner) => Some(inner),
            _ => None,
        }
    }

    #[inline]
    pub fn as_employer_mut(&mut self) -> Option<&mut Employer> {
        match self {
            Self::Employer(inner) => Some(inner),
            _ => None,
        }
    }

    // Common interface:
    pub fn count(&self) -> u32 {
        match self {
            Self::HouseholdWorkerPool(inner) => inner.unemployed_count(),
            Self::Employer(inner) => inner.employee_count(),
        }
    }

    pub fn is_max(&self) -> bool {
        match self {
            Self::HouseholdWorkerPool(inner) => inner.unemployed_count() == inner.total_workers(),
            Self::Employer(inner) => inner.is_at_max_capacity(),
        }
    }

    pub fn clear(&mut self) {
        match self {
            Self::HouseholdWorkerPool(inner) => inner.clear(),
            Self::Employer(inner) => inner.clear(),
        }
    }

    pub fn add(&mut self, amount: u32, source: BuildingKindAndId) -> u32 {
        match self {
            Self::HouseholdWorkerPool(inner) => inner.add_unemployed(amount, source),
            Self::Employer(inner) => inner.add_employee(amount, source),
        }
    }

    pub fn remove(&mut self, amount: u32, source: BuildingKindAndId) -> u32 {
        match self {
            Self::HouseholdWorkerPool(inner) => inner.remove_unemployed(amount, source),
            Self::Employer(inner) => inner.remove_employee(amount, source),
        }
    }

    pub fn merge(&mut self, other: &Workers) -> bool {
        match self {
            Self::HouseholdWorkerPool(inner) => {
                inner.merge(other.as_household_worker_pool().unwrap())
            }
            Self::Employer(inner) => inner.merge(other.as_employer().unwrap()),
        }
    }

    pub fn draw_debug_ui(&self, world: &World, ui_sys: &UiSystem) {
        match self {
            Self::HouseholdWorkerPool(inner) => inner.draw_debug_ui(world, ui_sys),
            Self::Employer(inner) => inner.draw_debug_ui(world, ui_sys),
        }
    }
}

// ----------------------------------------------
// Population
// ----------------------------------------------

#[derive(Copy, Clone, DrawDebugUi, Serialize, Deserialize)]
pub struct Population {
    #[debug_ui(label = "Population")]
    count: u8, // Current population number for household.

    #[debug_ui(label = "Max Residents")]
    max: u8, // Maximum population it can accommodate.
}

impl Population {
    pub fn new(count: u32, max: u32) -> Self {
        debug_assert!(max != 0);
        Self { count: count.min(max).try_into().expect("Population count must be < 256"),
               max: max.try_into().expect("Max population must be < 256") }
    }

    #[inline]
    pub fn count(&self) -> u32 {
        self.count as u32
    }

    #[inline]
    pub fn max(&self) -> u32 {
        self.max as u32
    }

    #[inline]
    pub fn is_max(&self) -> bool {
        self.count == self.max
    }

    #[inline]
    pub fn set_count(&mut self, count: u32) -> u32 {
        let count_u8: u8 = count.try_into().expect("Population count must be < 256");
        self.count = count_u8.min(self.max); // Clamp to maximum
        self.count() // Return new count
    }

    #[inline]
    pub fn set_max(&mut self, max: u32) -> u32 {
        debug_assert!(max != 0);
        self.max = max.try_into().expect("Max population must be < 256");
        self.count = self.count.min(self.max); // Clamp to new maximum
        self.count() // Return new count
    }

    #[inline]
    pub fn set_max_and_count(&mut self, max: u32, count: u32) -> u32 {
        self.set_max(max);
        self.set_count(count) // Return new count
    }

    #[inline]
    pub fn add(&mut self, amount: u32) -> u32 {
        let prev_count = self.count();
        let new_count = self.set_count(prev_count + amount);
        new_count - prev_count // Return amount added
    }

    #[inline]
    pub fn remove(&mut self, amount: u32) -> u32 {
        let prev_count = self.count();
        let new_count = self.set_count(prev_count.saturating_sub(amount));
        prev_count - new_count // Return amount subtracted
    }

    #[inline]
    pub fn clear(&mut self) {
        self.count = 0;
    }

    #[inline]
    pub fn merge(&mut self, other: &Population) -> bool {
        let merged_count = self.add(other.count());
        if merged_count != other.count() {
            log::error!("Population merge exceeds max capacity! Capacity: {}, trying to merge: {}, merged only: {}",
                        self.max(), other.count(), merged_count);
            return false;
        }
        true
    }
}

// ----------------------------------------------
// GlobalTreasury
// ----------------------------------------------

// - world.stats.treasury.gold_units_total: Total sum of all gold units
//   available in the world.
// - BuildingKind::treasury() buildings (e.g.: TaxOffice): Keeps a local
//   treasury of its earnings.
// - GlobalTreasury: Global stash of gold units for initial gold count, gold
//   cheats, etc.
#[derive(Serialize, Deserialize)]
pub struct GlobalTreasury {
    gold_units: u32,
}

impl GlobalTreasury {
    #[inline]
    pub fn new(starting_gold_units: u32) -> Self {
        Self { gold_units: starting_gold_units }
    }

    #[inline]
    pub fn gold_units(&self) -> u32 {
        self.gold_units
    }

    #[inline]
    pub fn add_gold_units(&mut self, amount: u32) {
        self.gold_units += amount;
    }

    #[inline]
    pub fn subtract_gold_units(&mut self, amount: u32) -> u32 {
        let prev_gold_units = self.gold_units;
        self.gold_units = self.gold_units.saturating_sub(amount);
        prev_gold_units - self.gold_units // units subtracted
    }

    #[inline]
    pub fn tally(&self, stats: &mut WorldStats) {
        stats.treasury.gold_units_total += self.gold_units;
    }

    #[inline]
    pub fn can_afford(&self, world: &World, cost: u32) -> bool {
        if cheats::get().ignore_tile_cost {
            return true;
        }

        cost <= world.stats().treasury.gold_units_total
    }

    #[inline]
    pub fn subtract_gold_units_global(&mut self, world: &mut World, mut amount: u32) {
        // First subtract from our local treasury:
        let mut gold_units_subtracted = self.subtract_gold_units(amount);
        amount -= gold_units_subtracted;

        // If we didn't have enough, look for a treasury
        // building that has more gold we can take from.
        if amount != 0 {
            world.for_each_building_mut(BuildingKind::treasury(), |building| {
                let removed_amount = building.remove_resources(ResourceKind::Gold, amount);
                debug_assert!(removed_amount <= amount);
                gold_units_subtracted += removed_amount;
                amount -= removed_amount;
                if amount == 0 {
                    return false; // stop
                }
                true // continue
            });
        }

        // Must be kept up to date because the world update frequency (and tally) might
        // not be high enough.
        world.stats_mut().treasury.gold_units_total -= gold_units_subtracted;

        debug_assert!(amount == 0,
                      "Should have found enough gold units in the available treasury buildings!");
    }
}

// ----------------------------------------------
// ResourceStock
// ----------------------------------------------

#[derive(Copy, Clone, Serialize, Deserialize)]
pub struct StockItem {
    pub kind: ResourceKind, // This is always a single bitflag; never ORed together.
    pub count: u32,
}

#[derive(Clone, Serialize)]
pub struct ResourceStock {
    kinds: ResourceKind, // If the kind flag bit is set, the stock accepts that resource.
    counts: [u16; RESOURCE_KIND_COUNT],
}

#[inline(always)]
fn bit_index(kind: ResourceKind) -> usize {
    debug_assert!(kind.is_single_resource());
    kind.bits().trailing_zeros() as usize
}

impl ResourceStock {
    #[inline]
    #[must_use]
    pub fn with_accepted_list(accepted_resources: &ResourceKinds) -> Self {
        let mut stock = Self {
            kinds: ResourceKind::empty(),
            counts: [0; RESOURCE_KIND_COUNT],
        };

        accepted_resources.for_each(|kind| {
            stock.kinds.insert(kind);
            true
        });

        stock
    }

    #[inline]
    #[must_use]
    pub fn with_accepted_kinds(accepted_kinds: ResourceKind) -> Self {
        Self { kinds: accepted_kinds, counts: [0; RESOURCE_KIND_COUNT] }
    }

    #[inline]
    #[must_use]
    pub fn accept_all() -> Self {
        Self { kinds: ResourceKind::all(), counts: [0; RESOURCE_KIND_COUNT] }
    }

    #[inline]
    pub fn accepted_count(&self) -> usize {
        self.kinds.bits().count_ones() as usize
    }

    #[inline]
    pub fn accepted_kinds(&self) -> ResourceKind {
        self.kinds
    }

    #[inline]
    pub fn accepts_any(&self) -> bool {
        !self.kinds.is_empty()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.counts.iter().all(|count| *count == 0)
    }

    #[inline]
    pub fn clear(&mut self) {
        self.counts = [0; RESOURCE_KIND_COUNT];
    }

    #[inline]
    pub fn has_any_of(&self, kinds: ResourceKind) -> bool {
        // Break down flags that are ORed together (since T is bitflags),
        // so that has_any_of() can work with multiple wanted kinds, e.g.:
        // has_any_of(A | B | C) -> returns true if any A|B|C is non-zero
        for single_kind in kinds.iter() {
            if self.count(single_kind) != 0 {
                return true;
            }
        }
        false
    }

    #[inline]
    pub fn find(&self, kind: ResourceKind) -> Option<(usize, StockItem)> {
        if !self.kinds.intersects(kind) {
            return None;
        }
        let index = bit_index(kind);
        let count = self.counts[index];
        let item = StockItem { kind, count: count.into() };
        Some((index, item))
    }

    #[inline]
    pub fn set(&mut self, index: usize, item: StockItem) {
        debug_assert!(self.kinds.intersects(item.kind));
        debug_assert!(bit_index(item.kind) == index);
        self.counts[index] = item.count.try_into().expect("Value cannot fit into a u16!");
    }

    #[inline]
    pub fn count(&self, kind: ResourceKind) -> u32 {
        if !self.kinds.intersects(kind) {
            return 0;
        }
        let index = bit_index(kind);
        self.counts[index].into()
    }

    #[inline]
    pub fn add(&mut self, kind: ResourceKind, count: u32) {
        if !self.kinds.intersects(kind) {
            panic!("Failed to add resource of kind '{}' to Stock! Kind not accepted.", kind);
        }
        let add_amount: u16 = count.try_into().expect("Value cannot fit into a u16!");
        let index = bit_index(kind);
        self.counts[index] += add_amount;
    }

    #[inline]
    pub fn remove(&mut self, kinds: ResourceKind, count: u32) -> Option<ResourceKind> {
        let sub_amount: u16 = count.try_into().expect("Value cannot fit into a u16!");

        // Break down flags that are ORed together (since T is bitflags),
        // so that remove() can work with multiple wanted kinds, e.g.:
        // remove(A | B | C) -> will remove the first of A|B|C that is
        // non-zero and return it.
        for single_kind in kinds.iter() {
            if self.kinds.intersects(single_kind) {
                let index = bit_index(single_kind);
                let count = self.counts[index];
                if count == 0 {
                    continue;
                }
                self.counts[index] = count.saturating_sub(sub_amount);
                return Some(single_kind);
            }
        }
        None
    }

    #[inline]
    pub fn for_each<F>(&self, mut visitor_fn: F)
        where F: FnMut(usize, &StockItem)
    {
        for (index, kind) in self.kinds.iter().enumerate() {
            if index < self.counts.len() {
                let count = self.counts[index];
                let item = StockItem { kind, count: count.into() };
                visitor_fn(index, &item);
            }
        }
    }

    #[inline]
    pub fn for_each_mut<F>(&mut self, mut visitor_fn: F)
        where F: FnMut(usize, &mut StockItem)
    {
        for (index, kind) in self.kinds.iter().enumerate() {
            if index < self.counts.len() {
                let count = self.counts[index];
                let mut item = StockItem { kind, count: count.into() };
                visitor_fn(index, &mut item);
                self.counts[index] = item.count.try_into().expect("Value cannot fit into a u16!");
            }
        }
    }

    // Read-only debug display.
    pub fn draw_debug_ui(&self, label: &str, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();
        if ui.collapsing_header(label, imgui::TreeNodeFlags::empty()) {
            ui.indent_by(5.0);
            self.for_each(|index, item| {
                ui.input_text(format!("{}##_stock_item_{}", item.kind, index),
                                &mut format!("{}", item.count))
                    .read_only(true)
                    .build();
            });
            ui.unindent_by(5.0);
        }
    }
}

impl Display for StockItem {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Item(kind: {}, count: {})", self.kind, self.count)
    }
}

// NOTE:
//  Custom deserialize allows us to change RESOURCE_KIND_COUNT
//  and keep backwards compatibility with older save games.
impl<'de> Deserialize<'de> for ResourceStock {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: Deserializer<'de>
    {
        #[derive(Deserialize)]
        struct SerializedStock {
            kinds: ResourceKind,
            counts: SmallVec<[u16; RESOURCE_KIND_COUNT]>, // allow flexible length
        }

        let stock = SerializedStock::deserialize(deserializer)?;

        if stock.counts.len() > RESOURCE_KIND_COUNT {
            return Err(de::Error::invalid_length(
                stock.counts.len(),
                &format!("at most {RESOURCE_KIND_COUNT} entries for ResourceStock").as_str(),
            ));
        }

        let mut counts = [0u16; RESOURCE_KIND_COUNT];
        for (i, value) in stock.counts.into_iter().enumerate() {
            counts[i] = value;
        }

        Ok(ResourceStock { kinds: stock.kinds, counts })
    }
}

// ----------------------------------------------
// ResourceList
// ----------------------------------------------

#[derive(Default, Serialize, Deserialize)]
pub struct ResourceList<T, const CAPACITY: usize> {
    kinds: ArrayVec<T, CAPACITY>, // Each item can be a single bitflag or multiple ORed together.
}

impl<T, const CAPACITY: usize> ResourceList<T, CAPACITY>
    where T: Copy + Display + bitflags::Flags
{
    #[inline]
    #[must_use]
    pub fn none() -> Self {
        Self { kinds: ArrayVec::new() }
    }

    #[inline]
    #[must_use]
    pub fn all() -> Self {
        let mut list = Self { kinds: ArrayVec::new() };

        for flag in T::FLAGS.iter() {
            list.kinds.push(*flag.value());
        }

        list
    }

    #[inline]
    #[must_use]
    pub fn with_slice(kinds: &[T]) -> Self {
        Self { kinds: ArrayVec::try_from(kinds).expect("Cannot fit all kinds in ResourceList!") }
    }

    #[inline]
    #[must_use]
    pub fn with_kinds(kinds: T) -> Self {
        let mut list = Self { kinds: ArrayVec::new() };

        // Break input into individual flags.
        for single_kind in kinds.iter() {
            list.kinds.push(single_kind);
        }

        list
    }

    #[inline]
    pub fn iter(&self) -> Iter<'_, T> {
        self.kinds.iter()
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.kinds.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.kinds.is_empty()
    }

    #[inline]
    pub fn clear(&mut self) {
        self.kinds.clear();
    }

    #[inline]
    pub fn add(&mut self, kind: T) {
        debug_assert!(!self.has_any_of(kind));
        self.kinds.push(kind);
    }

    #[inline]
    pub fn remove(&mut self, kind: T) {
        let mut index_to_remove = None;

        for (index, resource) in self.kinds.iter().enumerate() {
            if resource.intersects(kind) {
                index_to_remove = Some(index);
                break;
            }
        }

        if let Some(index) = index_to_remove {
            let removed = self.kinds.remove(index);
            assert!(removed.intersects(kind));
        }
    }

    #[inline]
    pub fn has_any_of(&self, kinds: T) -> bool {
        self.kinds.iter().any(|kind| kind.intersects(kinds))
    }

    // This will break down any flags that are ORed together into
    // individual calls to visitor_fn, unlike iter() which yields
    // combined flags as they appear.
    #[inline]
    pub fn for_each<F>(&self, mut visitor_fn: F)
        where F: FnMut(T) -> bool
    {
        for kinds in self.kinds.iter() {
            // Break down flags that are ORed together (T is bitflags).
            for single_kind in kinds.iter() {
                if !visitor_fn(single_kind) {
                    break;
                }
            }
        }
    }
}

impl<T, const CAPACITY: usize> Display for ResourceList<T, CAPACITY>
    where T: Copy + Display + bitflags::Flags
{
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut first = true;
        write!(f, "[")?;
        for kind in &self.kinds {
            if !first {
                write!(f, ", ")?;
            }
            write!(f, "{}", kind)?;
            first = false
        }
        write!(f, "]")?;
        Ok(())
    }
}

// ----------------------------------------------
// ShoppingList
// ----------------------------------------------

// List of resources to fetch + desired count.
// Implemented as a transparent newtype proxy over an ArrayVec.
#[derive(Default, Serialize, Deserialize)]
pub struct ShoppingList(ArrayVec<StockItem, RESOURCE_KIND_COUNT>);

impl ShoppingList {
    pub fn from_items(items: &[StockItem]) -> Self {
        Self(ArrayVec::try_from(items).expect("Cannot fit all items in ShoppingList!"))
    }
}

impl Deref for ShoppingList {
    type Target = ArrayVec<StockItem, RESOURCE_KIND_COUNT>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for ShoppingList {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Display for ShoppingList {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut first = true;
        write!(f, "[")?;
        for item in self.iter() {
            if !first {
                write!(f, ", ")?;
            }
            write!(f, "({},{})", item.kind, item.count)?;
            first = false
        }
        write!(f, "]")?;
        Ok(())
    }
}
