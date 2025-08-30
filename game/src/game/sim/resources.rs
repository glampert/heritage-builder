#![allow(clippy::blocks_in_conditions)]

use core::slice::Iter;
use std::fmt::Display;
use std::ops::{Deref, DerefMut};
use std::collections::{HashMap, hash_map::Entry};
use rand::{Rng, seq::IteratorRandom};
use arrayvec::ArrayVec;
use bitflags::{bitflags, Flags};
use proc_macros::DrawDebugUi;

use crate::{
    log,
    bitflags_with_display,
    imgui_ui::UiSystem,
    utils::Color,
    game::{
        sim::world::{World, BuildingId},
        building::{Building, BuildingKind, BuildingKindAndId}
    }
};

// ----------------------------------------------
// Resources (Consumer Goods, Raw Materials)
// ----------------------------------------------

bitflags_with_display! {
    #[derive(Copy, Clone, PartialEq, Eq)]
    pub struct ResourceKind: u32 {
        // Foods:
        const Rice  = 1 << 0;
        const Meat  = 1 << 1;
        const Fish  = 1 << 2;

        // Consumer Goods:
        const Wine  = 1 << 3;

        // Raw materials:
        const Wood  = 1 << 4;
        const Metal = 1 << 5;
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
        Self::from_bits_retain(
            Self::Rice.bits() |
            Self::Meat.bits() |
            Self::Fish.bits()
        )
    }

    #[inline]
    pub const fn consumer_goods() -> Self {
        Self::from_bits_retain(
            Self::Wine.bits()
        )
    }

    #[inline]
    pub const fn raw_materials() -> Self {
        Self::from_bits_retain(
            Self::Wood.bits() |
            Self::Metal.bits()
        )
    }

    #[inline]
    pub fn random<R: Rng>(rng: &mut R) -> Self {
        Self::all()
            .iter()
            .choose(rng)
            .unwrap_or(ResourceKind::Rice)
    }

    #[inline]
    pub fn random_food<R: Rng>(rng: &mut R) -> Self {
        Self::foods()
            .iter()
            .choose(rng)
            .unwrap_or(ResourceKind::Rice)
    }

    #[inline]
    pub fn random_consumer_good<R: Rng>(rng: &mut R) -> Self {
        Self::consumer_goods()
            .iter()
            .choose(rng)
            .unwrap_or(ResourceKind::Wine)
    }

    #[inline]
    pub fn random_raw_material<R: Rng>(rng: &mut R) -> Self {
        Self::raw_materials()
            .iter()
            .choose(rng)
            .unwrap_or(ResourceKind::Wood)
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
// Workers
// ----------------------------------------------

pub enum Workers {
    Household {
        // total = employed + unemployed
        employed: u8,
        unemployed: u8,
        employers: HashMap<BuildingKindAndId, u8>,
    },
    Employer {
        count: u8, // Current number of workers employed.
        min: u8,   // Minimum number of workers for service/production to run (at lower capacity).
        max: u8,   // Maximum number of workers it can employ (to run at full capacity).
        households: HashMap<BuildingId, u8>,
    }
}

impl Workers {
    #[inline]
    pub fn household(employed: u32, unemployed: u32) -> Self {
        Self::Household {
            employed: employed.try_into().expect("Workers count must be < 256"),
            unemployed: unemployed.try_into().expect("Workers count must be < 256"),
            employers: HashMap::new(),
        }
    }

    #[inline]
    pub fn is_household(&self) -> bool {
        matches!(self, Self::Household { .. })
    }

    #[inline]
    pub fn employed(&self) -> u32 {
        if let Self::Household { employed, .. } = self {
            return *employed as u32;
        }
        panic!("Not a Workers::Household!")
    }

    #[inline]
    pub fn unemployed(&self) -> u32 {
        if let Self::Household { unemployed, .. } = self {
            return *unemployed as u32;
        }
        panic!("Not a Workers::Household!")
    }

    #[inline]
    pub fn set_household_counts(&mut self, new_employed: u32, new_unemployed: u32) {
        if let Self::Household { employed, unemployed, .. } = self {
            *employed = new_employed.try_into().expect("Workers count must be < 256");
            *unemployed = new_unemployed.try_into().expect("Workers count must be < 256");
        } else {
            panic!("Not a Workers::Household!");
        }
    }

    #[inline]
    pub fn employer(min: u32, max: u32) -> Self {
        debug_assert!(min <= max);
        Self::Employer {
            count: 0,
            min: min.try_into().expect("Min workers must be < 256"),
            max: max.try_into().expect("Max workers must be < 256"),
            households: HashMap::new(),
        }
    }

    #[inline]
    pub fn is_employer(&self) -> bool {
        matches!(self, Self::Employer { .. })
    }

    #[inline]
    pub fn count(&self) -> u32 {
        match self {
            Self::Household { unemployed, .. } => *unemployed as u32,
            Self::Employer  { count, .. } => *count as u32,
        }
    }

    #[inline]
    pub fn min(&self) -> u32 {
        match self {
            Self::Household { .. } => 0,
            Self::Employer  { min, .. } => *min as u32,
        }
    }

    #[inline]
    pub fn is_min(&self) -> bool {
        self.count() == self.min()
    }

    #[inline]
    pub fn max(&self) -> u32 {
        match self {
            Self::Household { employed, unemployed, .. } => (*employed + *unemployed) as u32,
            Self::Employer  { max, .. } => *max as u32,
        }
    }

    #[inline]
    pub fn is_max(&self) -> bool {
        self.count() == self.max()
    }

    pub fn reset(&mut self) {
        match self {
            Self::Household { employed, unemployed, employers } => {
                let total = *employed + *unemployed;
                *employed = 0;
                *unemployed = total;
                employers.clear();
            },
            Self::Employer { count, households, .. } => {
                *count = 0;
                households.clear();
            },
        }
    }

    pub fn add(&mut self, amount: u32, source: BuildingKindAndId) -> u32 {
        let amount_u8: u8 = amount.try_into().expect("Workers count must be < 256");
        debug_assert!(amount_u8 != 0);
        debug_assert!(source.is_valid());

        match self {
            Self::Household { employed, unemployed, employers } => {
                let prev_employed = *employed;
                let new_employed  = prev_employed.saturating_sub(amount_u8);

                let prev_unemployed = *unemployed;
                let new_unemployed  = prev_unemployed + amount_u8;

                if new_employed + new_unemployed <= prev_employed + prev_unemployed {
                    *employed   = new_employed;
                    *unemployed = new_unemployed;

                    let unemployed_amount = new_unemployed - prev_unemployed;
                    if let Entry::Occupied(mut e) = employers.entry(source) {
                        if { *e.get_mut() = *e.get() - unemployed_amount; *e.get() == 0 } {
                            e.remove_entry();
                        }
                    } else {
                        panic!("Household: Expected to have an entry for ({}, {}); add({})", source.kind, source.id, amount);
                    }
                    unemployed_amount as u32 // Return amount added to unemployed count.
                } else {
                    0
                }
            },
            Self::Employer { count, max, households, .. } => {
                debug_assert!(source.kind == BuildingKind::House);

                let prev_count = *count;
                let new_count  = (prev_count + amount_u8).min(*max);
                *count = new_count;

                let employed_amount = new_count - prev_count;
                if employed_amount != 0 {
                    *households.entry(source.id).or_insert(0) += employed_amount;
                }
                employed_amount as u32 // Return amount added to employees.
            },
        }
    }

    pub fn subtract(&mut self, amount: u32, source: BuildingKindAndId) -> u32 {
        let amount_u8: u8 = amount.try_into().expect("Workers count must be < 256");
        debug_assert!(amount_u8 != 0);
        debug_assert!(source.is_valid());

        match self {
            Self::Household { employed, unemployed, employers } => {
                let prev_employed = *employed;
                let new_employed  = prev_employed + amount_u8;

                let prev_unemployed = *unemployed;
                let new_unemployed  = prev_unemployed.saturating_sub(amount_u8);

                if new_employed + new_unemployed <= prev_employed + prev_unemployed {
                    *employed   = new_employed;
                    *unemployed = new_unemployed;

                    let employed_amount = prev_unemployed - new_unemployed;
                    if employed_amount != 0 {
                        *employers.entry(source).or_insert(0) += employed_amount;
                    }
                    employed_amount as u32 // Return amount taken from unemployed count. i.e., amount employed.
                } else {
                    0
                }
            },
            Self::Employer { count, households, .. } => {
                debug_assert!(source.kind == BuildingKind::House);

                let prev_count = *count;
                let new_count  = prev_count.saturating_sub(amount_u8);
                *count = new_count;

                let unemployed_amount = prev_count - new_count;
                if let Entry::Occupied(mut e) = households.entry(source.id) {
                    if { *e.get_mut() = *e.get() - unemployed_amount; *e.get() == 0 } {
                        e.remove_entry();
                    }
                } else {
                    panic!("Employer: Expected to have an entry for ({}, {}); subtract({})", source.kind, source.id, amount);
                }
                unemployed_amount as u32 // Return amount subtracted from employees.
            },
        }
    }

    pub fn for_each<F>(&self, world: &mut World, mut visitor_fn: F)
        where F: FnMut(&mut Building, u32) -> bool
    {
        match self {
            Self::Household { employers, .. } => {
                for (employer_info, employed_count) in employers {
                    if let Some(employer) = world.find_building_mut(employer_info.kind, employer_info.id) {
                        if !visitor_fn(employer, *employed_count as u32) {
                            return;
                        }
                    } else {
                        log::error!("Household: Unknown employer record: ({}, {}); employed_count={}",
                                    employer_info.kind, employer_info.id, employed_count);
                    }
                }
            },
            Self::Employer { households, .. } => {
                for (house_id, employee_count) in households {
                    if let Some(house) = world.find_building_mut(BuildingKind::House, *house_id) {
                        if !visitor_fn(house, *employee_count as u32) {
                            return;
                        }
                    } else {
                        log::error!("Employer: Unknown employee household: {house_id}; employee_count={employee_count}");
                    }
                }
            },
        }
    }

    pub fn for_each_mut<F>(&mut self, world: &mut World, mut visitor_fn: F)
        where F: FnMut(&mut Building, &mut u32) -> bool
    {
        match self {
            Self::Household { employers, .. } => {
                for (employer_info, employed_count) in &mut *employers {
                    if let Some(employer) = world.find_building_mut(employer_info.kind, employer_info.id) {
                        let mut count = *employed_count as u32;
                        let should_continue = visitor_fn(employer, &mut count);
                        *employed_count = count.try_into().unwrap();
                        if !should_continue {
                            break;
                        }
                    } else {
                        log::error!("Household: Unknown employer record: ({}, {}); employed_count={}",
                                    employer_info.kind, employer_info.id, employed_count);
                    }
                }
                employers.retain(|_key, val| *val != 0);
            },
            Self::Employer { households, .. } => {
                for (house_id, employee_count) in &mut *households {
                    if let Some(house) = world.find_building_mut(BuildingKind::House, *house_id) {
                        let mut count = *employee_count as u32;
                        let should_continue = visitor_fn(house, &mut count);
                        *employee_count = count.try_into().unwrap();
                        if !should_continue {
                            break;
                        }
                    } else {
                        log::error!("Employer: Unknown employee household: {house_id}; employee_count={employee_count}");
                    }
                }
                households.retain(|_key, val| *val != 0);
            },
        }
    }

    pub fn draw_debug_ui(&self, world: &World, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();
        match self {
            Self::Household { employed, unemployed, employers } => {
                ui.text(format!("Employed   : {}", employed));
                ui.text(format!("Unemployed : {}", unemployed));
                ui.text(format!("Total      : {}", *employed + *unemployed));

                if !employers.is_empty() {
                    ui.text("Employers:");
                    ui.indent_by(10.0);

                    for (employer_info, employed_count) in employers {
                        if let Some(employer) = world.find_building(employer_info.kind, employer_info.id) {
                            ui.text(format!("- {} c={} id={}: {}", employer.name(), employer.base_cell(), employer.id(), employed_count));
                        } else {
                            ui.text_colored(Color::red().to_array(), "<unknown employer record>");
                        }
                    }

                    ui.unindent_by(10.0);
                }
            },
            Self::Employer { count, min, max, households } => {
                ui.text(format!("Workers      : {}", count));
                ui.text(format!("Min Required : {}", min));
                ui.text(format!("Max Employed : {}", max));

                if !households.is_empty() {
                    ui.text("Households:");
                    ui.indent_by(10.0);

                    for (house_id, employee_count) in households {
                        if let Some(house) = world.find_building(BuildingKind::House, *house_id) {
                            ui.text(format!("- {} c={} id={}: {}", house.name(), house.base_cell(), house.id(), employee_count));
                        } else {
                            ui.text_colored(Color::red().to_array(), "<unknown employee household>");
                        }
                    }

                    ui.unindent_by(10.0);
                }
            },
        }
    }
}

// ----------------------------------------------
// Population
// ----------------------------------------------

#[derive(Copy, Clone, DrawDebugUi)]
pub struct Population {
    #[debug_ui(label = "Population")]
    count: u8, // Current population number for household.

    #[debug_ui(label = "Max Residents")]
    max: u8, // Maximum population it can accommodate.
}

impl Population {
    pub fn new(count: u32, max: u32) -> Self {
        debug_assert!(max > 0);
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
        let new_count  = self.set_count(prev_count + amount);
        new_count - prev_count // Return amount added
    }

    #[inline]
    pub fn subtract(&mut self, amount: u32) -> u32 {
        let prev_count = self.count();
        let new_count  = self.set_count(prev_count.saturating_sub(amount));
        prev_count - new_count // Return amount subtracted
    }
}

// ----------------------------------------------
// ResourceStock
// ----------------------------------------------

#[derive(Copy, Clone)]
pub struct StockItem {
    pub kind: ResourceKind, // This is always a single bitflag; never ORed together.
    pub count: u32,
}

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
        Self {
            kinds: accepted_kinds,
            counts: [0; RESOURCE_KIND_COUNT],
        }
    }

    #[inline]
    #[must_use]
    pub fn accept_all() -> Self {
        Self {
            kinds: ResourceKind::all(),
            counts: [0; RESOURCE_KIND_COUNT],
        }
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
            debug_assert!(bit_index(kind) == index);
            let count = self.counts[index];
            let item = StockItem { kind, count: count.into() };
            visitor_fn(index, &item);
        }
    }

    #[inline]
    pub fn for_each_mut<F>(&mut self, mut visitor_fn: F)
        where F: FnMut(usize, &mut StockItem)
    {
        for (index, kind) in self.kinds.iter().enumerate() {
            debug_assert!(bit_index(kind) == index);
            let count = self.counts[index];
            let mut item = StockItem { kind, count: count.into() };
            visitor_fn(index, &mut item);
            self.counts[index] = item.count.try_into().expect("Value cannot fit into a u16!");
        }
    }

    // Read-only debug display.
    pub fn draw_debug_ui(&self, label: &str, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();
        if ui.collapsing_header(label, imgui::TreeNodeFlags::empty()) {
            ui.indent_by(5.0);
            self.for_each(|index, item| {
                ui.input_text(format!("{}##_stock_item_{}", item.kind, index), &mut format!("{}", item.count))
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

// ----------------------------------------------
// ResourceList
// ----------------------------------------------

pub struct ResourceList<T, const CAPACITY: usize> {
    kinds: ArrayVec<T, CAPACITY>, // Each item can be a single bitflag or multiple ORed together.
}

impl<T, const CAPACITY: usize> ResourceList<T, CAPACITY> 
    where T: Copy + Display + bitflags::Flags
{
    #[inline]
    #[must_use]
    pub fn none() -> Self {
        Self {
            kinds: ArrayVec::new(),
        }
    }

    #[inline]
    #[must_use]
    pub fn all() -> Self {
        let mut list = Self {
            kinds: ArrayVec::new(),
        };

        for flag in T::FLAGS.iter() {
            list.kinds.push(*flag.value());
        }

        list
    }

    #[inline]
    #[must_use]
    pub fn with_slice(kinds: &[T]) -> Self {
        Self {
            kinds: ArrayVec::try_from(kinds).expect("Cannot fit all kinds in ResourceList!"),
        }
    }

    #[inline]
    #[must_use]
    pub fn with_kinds(kinds: T) -> Self {
         let mut list = Self {
            kinds: ArrayVec::new(),
        };

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
#[derive(Default)]
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
