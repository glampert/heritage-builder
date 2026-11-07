#![allow(clippy::blocks_in_conditions)]

use core::slice::Iter;
use std::{
    collections::{BTreeMap, btree_map::Entry},
    fmt::Display,
    ops::{Deref, DerefMut},
};

use arrayvec::ArrayVec;
use bitflags::Flags;
use common::bitflags_with_display;
use engine::log;
use proc_macros::DrawDebugUi;
use rand::{Rng, seq::IteratorRandom};
use serde::{Deserialize, Serialize};

use crate::{
    building::{Building, BuildingId, BuildingKind, BuildingKindAndId},
    world::World,
};

// ----------------------------------------------
// Resources (Consumer Goods, Raw Materials)
// ----------------------------------------------

// Saves store goods by name, never by bit position (see `per_kind_map`), so a good can be
// added anywhere in this list without breaking older saves. Removing or renaming one breaks
// them and needs a migration script. The bits must stay contiguous from 0 (checked below),
// since a good's bit is its index in `[_; RESOURCE_KIND_COUNT]` arrays. Bit order is the
// order goods are visited in, so changing it changes which of `A | B` a request takes first.
bitflags_with_display! {
    #[derive(Copy, Clone, Default, PartialEq, Eq, Debug, Serialize, Deserialize)]
    pub struct ResourceKind: u64 {
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
        const RoofTiles = 1 << 9;

        // Gold (used as currency only):
        const Gold    = 1 << 10;
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
        Self::from_bits_retain(
            Self::Wood.bits() | Self::Metal.bits() | Self::Clay.bits() | Self::Bricks.bits() | Self::RoofTiles.bits(),
        )
    }

    #[inline]
    pub fn random<R: Rng>(rng: &mut R) -> Self {
        Self::all().iter().choose(rng).unwrap_or(Self::Rice)
    }

    #[inline]
    pub fn random_food<R: Rng>(rng: &mut R) -> Self {
        Self::foods().iter().choose(rng).unwrap_or(Self::Rice)
    }

    #[inline]
    pub fn random_consumer_good<R: Rng>(rng: &mut R) -> Self {
        Self::consumer_goods().iter().choose(rng).unwrap_or(Self::Wine)
    }

    #[inline]
    pub fn random_raw_material<R: Rng>(rng: &mut R) -> Self {
        Self::raw_materials().iter().choose(rng).unwrap_or(Self::Wood)
    }

    #[inline]
    pub fn all_except(exclude: Self) -> Self {
        let mut all = Self::all();
        all.remove(exclude);
        all
    }

    // Name of a single-resource flag, as saves and configs spell it.
    #[inline]
    pub fn name(self) -> &'static str {
        debug_assert!(self.is_single_resource());
        Self::FLAGS.iter().find(|flag| *flag.value() == self).map_or("", |flag| flag.name())
    }

    // Name of a single-resource flag as the player reads it, with spaces between its words:
    // "Roof Tiles" for `RoofTiles`. Saves and configs use `name()`.
    pub fn display_name(self) -> std::borrow::Cow<'static, str> {
        let name = self.name();
        if !name.chars().skip(1).any(|c| c.is_ascii_uppercase()) {
            return name.into();
        }
        let mut spaced = String::with_capacity(name.len() + 2);
        for (i, c) in name.chars().enumerate() {
            if i != 0 && c.is_ascii_uppercase() {
                spaced.push(' ');
            }
            spaced.push(c);
        }
        spaced.into()
    }

    // Index of a single-resource flag, for use with `[_; RESOURCE_KIND_COUNT]` arrays.
    #[inline]
    pub fn index(self) -> usize {
        debug_assert!(self.is_single_resource());
        self.bits().trailing_zeros() as usize
    }
}

pub const RESOURCE_KIND_COUNT: usize = ResourceKind::count();

const _: () = assert!(
    RESOURCE_KIND_COUNT < 64 && ResourceKind::all().bits() == (1u64 << RESOURCE_KIND_COUNT) - 1,
    "ResourceKind bits must be contiguous from bit 0"
);
pub type ResourceKinds = ResourceList<ResourceKind, RESOURCE_KIND_COUNT>;

// ----------------------------------------------
// Services
// ----------------------------------------------

pub type ServiceKind = BuildingKind;

pub const SERVICE_KIND_COUNT: usize = ServiceKind::services_count();
pub type ServiceKinds = ResourceList<ServiceKind, SERVICE_KIND_COUNT>;

// ----------------------------------------------
// Custom Serde Serialization for BTreeMap
// ----------------------------------------------

// serde_json does not support maps indexed by structure (json maps must be
// keyed by a string). This converts a map into a Vec of pairs when serializing
// and builds back a map during deserialization.
//
// The worker maps below are BTreeMaps rather than HashMaps so that iterating them
// (often with an early exit) visits entries in the same order on every run, which
// keeps the simulation deterministic for a given random seed.
mod serialize_map_as_pairs {
    use std::collections::BTreeMap;

    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<K, V, S>(map: &BTreeMap<K, V>, ser: S) -> Result<S::Ok, S::Error>
    where
        K: Serialize + Ord,
        V: Serialize,
        S: Serializer,
    {
        // Turn into Vec of pairs and serialize that:
        let vec: Vec<(&K, &V)> = map.iter().collect();
        vec.serialize(ser)
    }

    pub fn deserialize<'de, K, V, D>(de: D) -> Result<BTreeMap<K, V>, D::Error>
    where
        K: Deserialize<'de> + Ord,
        V: Deserialize<'de>,
        D: Deserializer<'de>,
    {
        let vec = Vec::<(K, V)>::deserialize(de)?;
        Ok(vec.into_iter().collect())
    }
}

// ----------------------------------------------
// Per-kind arrays saved by name
// ----------------------------------------------

// Serializes a `[T; RESOURCE_KIND_COUNT]` indexed by `ResourceKind::index()` as a map from
// each good's name to its value, leaving out default (zero) entries: `{ "Rice": 2 }`.
// A save then never depends on a good's position, so goods can be added or reordered
// without breaking it. A name the code doesn't know is an error: the good was removed or
// renamed, which needs a migration script.
pub(crate) mod per_kind_map {
    use std::{fmt, marker::PhantomData};

    use serde::{
        Deserialize, Deserializer, Serialize, Serializer,
        de::{self, MapAccess, Visitor},
        ser::SerializeMap,
    };

    use super::{RESOURCE_KIND_COUNT, ResourceKind};

    pub fn serialize<T, S>(values: &[T; RESOURCE_KIND_COUNT], ser: S) -> Result<S::Ok, S::Error>
    where
        T: Serialize + Default + PartialEq,
        S: Serializer,
    {
        let default = T::default();
        let mut map = ser.serialize_map(None)?;
        for kind in ResourceKind::all().iter() {
            let value = &values[kind.index()];
            if *value != default {
                map.serialize_entry(&kind.name(), value)?;
            }
        }
        map.end()
    }

    pub fn deserialize<'de, T, D>(de: D) -> Result<[T; RESOURCE_KIND_COUNT], D::Error>
    where
        T: Deserialize<'de> + Default + Copy,
        D: Deserializer<'de>,
    {
        struct PerKindVisitor<T>(PhantomData<T>);

        impl<'de, T> Visitor<'de> for PerKindVisitor<T>
        where
            T: Deserialize<'de> + Default + Copy,
        {
            type Value = [T; RESOURCE_KIND_COUNT];

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a map from resource kind names to values")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> Result<Self::Value, A::Error> {
                let mut values = [T::default(); RESOURCE_KIND_COUNT];
                while let Some(name) = access.next_key::<String>()? {
                    let kind = ResourceKind::from_name(&name)
                        .ok_or_else(|| de::Error::custom(format!("unknown resource kind '{name}'")))?;
                    values[kind.index()] = access.next_value()?;
                }
                Ok(values)
            }
        }

        de.deserialize_map(PerKindVisitor(PhantomData))
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
    #[serde(with = "serialize_map_as_pairs")]
    employers: BTreeMap<BuildingKindAndId, u32>,
}

impl HouseholdWorkerPool {
    #[inline]
    pub fn new(employed_count: u32, unemployed_count: u32) -> Self {
        Self { employed_count, unemployed_count, employers: BTreeMap::new() }
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
    where
        F: FnMut(&mut Building, u32) -> bool,
    {
        for (employer_info, employed_count) in &self.employers {
            if let Some(employer) = world.find_building_mut(employer_info.kind, employer_info.id) {
                if !visitor_fn(employer, *employed_count) {
                    return;
                }
            } else {
                log::error!(
                    log::channel!("Household"),
                    "Unknown employer record: ({}, {}); employed_count={}",
                    employer_info.kind,
                    employer_info.id,
                    employed_count
                );
            }
        }
    }

    pub fn for_each_employer_mut<F>(&mut self, world: &mut World, mut visitor_fn: F)
    where
        F: FnMut(&mut Building, &mut u32) -> bool,
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
                log::error!(
                    log::channel!("Household"),
                    "Unknown employer record: ({}, {}); employed_count={}",
                    employer_info.kind,
                    employer_info.id,
                    employed_count
                );
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
                log::error!(
                    log::channel!("Household"),
                    "Expected to have an entry for ({}, {}); add({})",
                    source.kind,
                    source.id,
                    amount
                );
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
                log::error!(
                    "HouseholdWorkerPool merge exceeds workers available! Unemployed: {}, trying to merge: {}, merged only: {}",
                    self.unemployed_count(),
                    *employed_count,
                    merged_count
                );
                return false;
            }
        }
        true
    }

    // Iterator over (employer, employed_count) pairs, for the debug UI (see `crate::debug::sim`).
    pub(crate) fn employers_iter(&self) -> impl Iterator<Item = (BuildingKindAndId, u32)> + '_ {
        self.employers.iter().map(|(info, count)| (*info, *count))
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
    #[serde(with = "serialize_map_as_pairs")]
    employee_households: BTreeMap<BuildingId, u32>, // Households we source our workers from.
}

impl Employer {
    #[inline]
    pub fn new(min_employees: u32, max_employees: u32) -> Self {
        debug_assert!(min_employees <= max_employees);
        Self { employee_count: 0, min_employees, max_employees, employee_households: BTreeMap::new() }
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

    // Fraction [0,1] of the workforce relative to maximum capacity. Buildings use
    // this to scale how fast they run when below max workers. An employer that
    // needs no workers at all (max_employees == 0) is considered fully efficient.
    #[inline]
    pub fn work_efficiency(&self) -> f32 {
        if self.max_employees == 0 {
            return 1.0;
        }
        (self.employee_count as f32 / self.max_employees as f32).clamp(0.0, 1.0)
    }

    pub fn for_each_employee_household<F>(&self, world: &mut World, mut visitor_fn: F)
    where
        F: FnMut(&mut Building, u32) -> bool,
    {
        for (house_id, employee_count) in &self.employee_households {
            if let Some(house) = world.find_building_mut(BuildingKind::House, *house_id) {
                if !visitor_fn(house, *employee_count) {
                    return;
                }
            } else {
                log::error!(
                    log::channel!("Employer"),
                    "Unknown employee household: {house_id}; employee_count={employee_count}"
                );
            }
        }
    }

    pub fn for_each_employee_household_mut<F>(&mut self, world: &mut World, mut visitor_fn: F)
    where
        F: FnMut(&mut Building, &mut u32) -> bool,
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
                log::error!(
                    log::channel!("Employer"),
                    "Unknown employee household: {house_id}; employee_count={employee_count}"
                );
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
            log::error!(
                log::channel!("Employer"),
                "Expected to have an entry for ({}, {}); subtract({})",
                source.kind,
                source.id,
                amount
            );
        }

        unemployed_amount // Return amount subtracted from employees.
    }

    pub fn merge(&mut self, other: &Employer) -> bool {
        for (house_id, employee_count) in &other.employee_households {
            let merged_count =
                self.add_employee(*employee_count, BuildingKindAndId { kind: BuildingKind::House, id: *house_id });

            if merged_count != *employee_count {
                log::error!(
                    "Employer merge exceeds maximum! Max employees: {}, trying to merge: {}, merged only: {}",
                    self.max_employees(),
                    *employee_count,
                    merged_count
                );
                return false;
            }
        }
        true
    }

    // Iterator over (household_id, employed_count) pairs, for the debug UI (see `crate::debug::sim`).
    pub(crate) fn employee_households_iter(&self) -> impl Iterator<Item = (BuildingId, u32)> + '_ {
        self.employee_households.iter().map(|(id, count)| (*id, *count))
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
            Self::HouseholdWorkerPool(inner) => inner.merge(other.as_household_worker_pool().unwrap()),
            Self::Employer(inner) => inner.merge(other.as_employer().unwrap()),
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
        Self {
            count: count.min(max).try_into().expect("Population count must be < 256"),
            max: max.try_into().expect("Max population must be < 256"),
        }
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
            log::error!(
                "Population merge exceeds max capacity! Capacity: {}, trying to merge: {}, merged only: {}",
                self.max(),
                other.count(),
                merged_count
            );
            return false;
        }
        true
    }
}

// ----------------------------------------------
// ResourceStock
// ----------------------------------------------

#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct StockItem {
    pub kind: ResourceKind, // This is always a single bitflag; never ORed together.
    pub count: u32,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ResourceStock {
    kinds: ResourceKind, // If the kind flag bit is set, the stock accepts that resource.
    #[serde(with = "per_kind_map")] // Saved by name: { "Rice": 2 }.
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
        let mut stock = Self { kinds: ResourceKind::empty(), counts: [0; RESOURCE_KIND_COUNT] };

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
    #[must_use]
    pub fn accept_all_except(exclude: ResourceKind) -> Self {
        Self { kinds: ResourceKind::all_except(exclude), counts: [0; RESOURCE_KIND_COUNT] }
    }

    // Accepts `kinds` from now on, plus any good it still holds, so nothing held is lost.
    // Used after a load, to take the accepted goods from the owner's config.
    pub fn set_accepted_kinds_keeping_held(&mut self, kinds: ResourceKind) {
        let mut held = ResourceKind::empty();
        self.for_each(|_, item| {
            if item.count != 0 {
                held.insert(item.kind);
            }
        });
        let accepted = kinds | held;
        // Counts of goods no longer accepted are zero (see above); clear any leftovers.
        for kind in ResourceKind::all().difference(accepted).iter() {
            self.counts[kind.index()] = 0;
        }
        self.kinds = accepted;
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

    // Visits each accepted kind in bit order, with its count and its index: the kind's bit, as in
    // `[_; RESOURCE_KIND_COUNT]` arrays (find() returns the same index).
    #[inline]
    pub fn for_each<F>(&self, mut visitor_fn: F)
    where
        F: FnMut(usize, &StockItem),
    {
        for kind in self.kinds.iter() {
            let index = bit_index(kind);
            if index < self.counts.len() {
                let item = StockItem { kind, count: self.counts[index].into() };
                visitor_fn(index, &item);
            }
        }
    }

    // As for_each, then stores each item's count back.
    #[inline]
    pub fn for_each_mut<F>(&mut self, mut visitor_fn: F)
    where
        F: FnMut(usize, &mut StockItem),
    {
        for kind in self.kinds.iter() {
            let index = bit_index(kind);
            if index < self.counts.len() {
                let mut item = StockItem { kind, count: self.counts[index].into() };
                visitor_fn(index, &mut item);
                self.counts[index] = item.count.try_into().expect("Value cannot fit into a u16!");
            }
        }
    }

}

impl Display for StockItem {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Item(kind: {}, count: {})", self.kind, self.count)
    }
}

// ----------------------------------------------
// ResourceList
// ----------------------------------------------

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct ResourceList<T, const CAPACITY: usize> {
    kinds: ArrayVec<T, CAPACITY>, // Each item can be a single bitflag or multiple ORed together.
}

impl<T, const CAPACITY: usize> ResourceList<T, CAPACITY>
where
    T: Copy + Display + bitflags::Flags,
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

    // Every kind in the list, ORed together.
    #[inline]
    pub fn combined(&self) -> T {
        self.kinds.iter().fold(T::empty(), |all, kind| all.union(*kind))
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
    where
        F: FnMut(T) -> bool,
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
where
    T: Copy + Display + bitflags::Flags,
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

#[cfg(test)]
mod tests {
    use super::*;

    // A stock that accepts goods which don't start ResourceKind's bit order, as a storage yard
    // saved before it took pottery and bricks does.
    fn wood_and_bricks() -> ResourceStock {
        let mut stock = ResourceStock::with_accepted_kinds(ResourceKind::Wood | ResourceKind::Bricks);
        stock.add(ResourceKind::Wood, 3);
        stock.add(ResourceKind::Bricks, 5);
        stock
    }

    #[test]
    fn for_each_reports_each_kind_with_its_count_and_index() {
        let stock = wood_and_bricks();
        let mut visited = Vec::new();
        stock.for_each(|index, item| visited.push((index, *item)));
        assert_eq!(visited, [
            (ResourceKind::Wood.index(), StockItem { kind: ResourceKind::Wood, count: 3 }),
            (ResourceKind::Bricks.index(), StockItem { kind: ResourceKind::Bricks, count: 5 }),
        ]);
    }

    #[test]
    fn for_each_mut_writes_each_kind_back() {
        let mut stock = wood_and_bricks();
        stock.for_each_mut(|_, item| item.count *= 2);
        assert_eq!(stock.count(ResourceKind::Wood), 6);
        assert_eq!(stock.count(ResourceKind::Bricks), 10);

        stock.for_each_mut(|_, item| item.count = 0);
        assert!(stock.is_empty());
    }

    // Saves hold goods by name, never by position (docs/plans/goods.md, Phase 0).
    #[test]
    fn stock_saves_counts_by_name() {
        let json = serde_json::to_value(wood_and_bricks()).unwrap();
        assert_eq!(json, serde_json::json!({ "kinds": "Wood | Bricks", "counts": { "Wood": 3, "Bricks": 5 } }));

        let stock: ResourceStock = serde_json::from_value(json).unwrap();
        assert_eq!(stock.count(ResourceKind::Wood), 3);
        assert_eq!(stock.count(ResourceKind::Bricks), 5);
        assert_eq!(stock.accepted_kinds(), ResourceKind::Wood | ResourceKind::Bricks);
    }

    // Key order and missing goods don't matter, so a save made before a good was added, or
    // with goods declared in another order, loads with every count on its own good.
    #[test]
    fn stock_loads_counts_in_any_order() {
        let stock: ResourceStock = serde_json::from_value(serde_json::json!({
            "kinds": "Bricks | Rice | Pottery",
            "counts": { "Pottery": 1, "Bricks": 7, "Rice": 2 },
        }))
        .unwrap();
        assert_eq!(stock.count(ResourceKind::Rice), 2);
        assert_eq!(stock.count(ResourceKind::Pottery), 1);
        assert_eq!(stock.count(ResourceKind::Bricks), 7);
        assert_eq!(stock.count(ResourceKind::Wood), 0);

        let empty: ResourceStock = serde_json::from_value(serde_json::json!({ "kinds": "Rice", "counts": {} })).unwrap();
        assert!(empty.is_empty());
    }

    // A good the code doesn't know was removed or renamed: that needs a migration script.
    #[test]
    fn stock_rejects_unknown_goods() {
        let result: Result<ResourceStock, _> =
            serde_json::from_value(serde_json::json!({ "kinds": "Rice", "counts": { "Unobtainium": 1 } }));
        assert!(result.err().expect("unknown good must fail").to_string().contains("unknown resource kind 'Unobtainium'"));
    }

    #[test]
    fn set_accepted_kinds_keeps_held_goods() {
        let mut stock = wood_and_bricks();
        stock.remove(ResourceKind::Wood, 3);
        stock.set_accepted_kinds_keeping_held(ResourceKind::Clay | ResourceKind::Pottery);

        // Bricks are still held, so still accepted; Wood is empty and dropped.
        assert_eq!(stock.accepted_kinds(), ResourceKind::Clay | ResourceKind::Pottery | ResourceKind::Bricks);
        assert_eq!(stock.count(ResourceKind::Bricks), 5);
        stock.add(ResourceKind::Pottery, 2);
        assert_eq!(stock.count(ResourceKind::Pottery), 2);
    }

    #[test]
    fn display_names_have_spaces() {
        assert_eq!(ResourceKind::RoofTiles.display_name(), "Roof Tiles");
        assert_eq!(ResourceKind::Rice.display_name(), "Rice");
    }

    #[test]
    fn kind_names_round_trip() {
        for kind in ResourceKind::all().iter() {
            assert_eq!(ResourceKind::from_name(kind.name()), Some(kind));
        }
    }
}
