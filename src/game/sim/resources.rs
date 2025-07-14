use core::slice::Iter;
use arrayvec::ArrayVec;
use bitflags::{bitflags, Flags};
use std::fmt::Display;

use crate::{
    bitflags_with_display,
    imgui_ui::UiSystem,
    game::building::BuildingKind
};

// ----------------------------------------------
// Stock generic
// ----------------------------------------------

#[derive(Copy, Clone)]
pub struct StockItem<T> {
    pub kind: T, // This is always a single bitflag; never ORed together.
    pub count: u32,
}

pub struct Stock<T, const CAPACITY: usize> {
    item_bits: T,
    item_counts: [u16; CAPACITY],
}

#[inline(always)]
fn bit_index<T>(bitflag: T) -> usize
    where T: Copy + bitflags::Flags, u32: From<<T as bitflags::Flags>::Bits>
{
    let bits: u32 = bitflag.bits().into();
    debug_assert!(bits.count_ones() == 1);
    bits.trailing_zeros() as usize
}

impl<T, const CAPACITY: usize> Stock<T, CAPACITY> 
    where T: Copy + Display + bitflags::Flags, u32: From<<T as Flags>::Bits>
{
    #[inline]
    #[must_use]
    pub fn with_accepted_items(accepted_items: &List<T, CAPACITY>) -> Self {
        let mut stock = Self {
            item_bits: T::empty(),
            item_counts: [0; CAPACITY],
        };

        accepted_items.for_each(|item| {
            stock.item_bits.set(item, true);
            true
        });

        stock
    }

    #[inline]
    #[must_use]
    pub fn accept_all_items() -> Self {
        let mut stock = Self {
            item_bits: T::empty(),
            item_counts: [0; CAPACITY],
        };

        for item in T::FLAGS.iter() {
            stock.item_bits.set(*item.value(), true);
        }

        stock
    }

    #[inline]
    pub fn accepted_items_count(&self) -> usize {
        self.item_counts.len()
    }

    #[inline]
    pub fn accepts_any_item(&self) -> bool {
        self.accepted_items_count() != 0
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        for item_count in self.item_counts {
            if item_count != 0 {
                return false;
            }
        }
        true
    }

    #[inline]
    pub fn clear(&mut self) {
        for item_count in &mut self.item_counts {
            *item_count = 0;
        }
    }

    #[inline]
    pub fn has(&self, wanted: T) -> bool {
        // Break down flags that are ORed together (since T is bitflags),
        // so that has() can work with multiple wanted items, e.g.:
        // has(A | B | C) -> returns true if any A|B|C is non-zero
        for single_flag in wanted.iter() {
            if self.count(single_flag) != 0 {
                return true;
            }
        }
        false
    }

    #[inline]
    pub fn find(&self, wanted: T) -> Option<(usize, StockItem<T>)> {
        if !self.item_bits.intersects(wanted) {
            return None;
        }
        let item_index = bit_index(wanted);
        let item_count = self.item_counts[item_index];
        let stock_item = StockItem { kind: wanted, count: item_count.into() };
        return Some((item_index, stock_item));
    }

    #[inline]
    pub fn set(&mut self, item_index: usize, new_item: StockItem<T>) {
        debug_assert!(self.item_bits.intersects(new_item.kind));
        debug_assert!(bit_index(new_item.kind) == item_index);
        self.item_counts[item_index] = new_item.count.try_into().expect("Value cannot fit into a u16!");
    }

    #[inline]
    pub fn count(&self, wanted: T) -> u32 {
        if !self.item_bits.intersects(wanted) {
            return 0;
        }
        let item_index = bit_index(wanted);
        self.item_counts[item_index].into()
    }

    #[inline]
    pub fn add(&mut self, new_item: T) {
        if !self.item_bits.intersects(new_item) {
            panic!("Failed to add item '{}' to Stock! Item not accepted.", new_item);
        }
        let item_index = bit_index(new_item);
        self.item_counts[item_index] += 1;
    }

    #[inline]
    pub fn remove(&mut self, wanted: T) -> Option<T> {
        // Break down flags that are ORed together (since T is bitflags),
        // so that remove() can work with multiple wanted items, e.g.:
        // remove(A | B | C) -> will remove the first of A|B|C that is
        // non-zero and return it.
        for single_flag in wanted.iter() {
            if self.item_bits.intersects(single_flag) {
                let item_index = bit_index(single_flag);
                let item_count = self.item_counts[item_index];
                if item_count == 0 {
                    continue;
                }
                self.item_counts[item_index] = item_count - 1;
                return Some(single_flag);
            }
        }
        None
    }

    #[inline]
    pub fn for_each<F>(&self, mut visitor_fn: F)
        where F: FnMut(usize, &StockItem<T>)
    {
        for (item_index, item_kind) in self.item_bits.iter().enumerate() {
            debug_assert!(bit_index(item_kind) == item_index);
            let item_count = self.item_counts[item_index];
            let stock_item = StockItem { kind: item_kind, count: item_count.into() };
            visitor_fn(item_index, &stock_item);
        }
    }

    #[inline]
    pub fn for_each_mut<F>(&mut self, mut visitor_fn: F)
        where F: FnMut(usize, &mut StockItem<T>)
    {
        for (item_index, item_kind) in self.item_bits.iter().enumerate() {
            debug_assert!(bit_index(item_kind) == item_index);
            let item_count = self.item_counts[item_index];
            let mut stock_item = StockItem { kind: item_kind, count: item_count.into() };
            visitor_fn(item_index, &mut stock_item);
            self.item_counts[item_index] = stock_item.count.try_into().expect("Value cannot fit into a u16!");
        }
    }

    pub fn draw_debug_ui(&mut self, label: &str, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();
        if ui.collapsing_header(format!("{}##_resource_stock", label), imgui::TreeNodeFlags::empty()) {
            self.for_each_mut(|index, item| {
                ui.input_scalar(format!("{}##_stock_item_{}", item.kind, index), &mut item.count)
                    .step(1)
                    .build();
            });
        }
    }

    pub fn draw_debug_ui_filtered<F>(&mut self, label: &str, ui_sys: &UiSystem, filter_fn: F)
        where F: Fn(&StockItem<T>) -> bool
    {
        let ui = ui_sys.builder();
        if ui.collapsing_header(format!("{}##_resource_stock", label), imgui::TreeNodeFlags::empty()) {
            self.for_each_mut(|index, item| {
                if filter_fn(item) {
                    ui.input_scalar(format!("{}##_stock_item_{}", item.kind, index), &mut item.count)
                        .step(1)
                        .build();
                }
            });
        }
    }
}

// ----------------------------------------------
// List generic
// ----------------------------------------------

pub struct List<T, const CAPACITY: usize> {
    items: ArrayVec<T, CAPACITY>, // Each item can be a single bitflag or multiple ORed together.
}

impl<T, const CAPACITY: usize> List<T, CAPACITY> 
    where T: Copy + Display + bitflags::Flags
{
    #[inline]
    #[must_use]
    pub fn empty() -> Self {
        Self {
            items: ArrayVec::new(),
        }
    }

    #[inline]
    #[must_use]
    pub fn with_all_items() -> Self {
        let mut list = Self {
            items: ArrayVec::new(),
        };

        for item in T::FLAGS.iter() {
            list.items.push(*item.value());
        }

        list
    }

    #[inline]
    #[must_use]
    pub fn with_items_slice(items: &[T]) -> Self {
        Self {
            items: ArrayVec::try_from(items).expect("Cannot fit all items into List!"),
        }
    }

    #[inline]
    #[must_use]
    pub fn with_items_expanded(items: T) -> Self {
         let mut list = Self {
            items: ArrayVec::new(),
        };

        // Break input into individual flags.
        for item in items.iter() {
            list.items.push(item);
        }

        list
    }

    #[inline]
    pub fn iter(&self) -> Iter<'_, T> {
        self.items.iter()
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    #[inline]
    pub fn clear(&mut self) {
        self.items.clear();
    }

    #[inline]
    pub fn add(&mut self, new_item: T) {
        debug_assert!(!self.has(new_item));
        self.items.push(new_item);
    }

    #[inline]
    pub fn has(&self, wanted: T) -> bool {
        for item in &self.items {
            if item.intersects(wanted) {
                return true;
            }
        }
        false
    }

    // This will break down any flags that are ORed together into
    // individual calls to visitor_fn, unlike iter() which yields
    // combined flags as they appear.
    #[inline]
    pub fn for_each<F>(&self, mut visitor_fn: F)
        where F: FnMut(T) -> bool
    {
        for items in &self.items {
            // Break down items that are ORed together (T is bitflags).
            for single_item in items.iter() {
                if !visitor_fn(single_item) {
                    break;
                }
            }
        }
    }
}

impl<T, const CAPACITY: usize> Display for List<T, CAPACITY>
    where T: Copy + Display + bitflags::Flags
{
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut first = true;
        write!(f, "[")?;
        for items in &self.items {
            if !first {
                write!(f, ", ")?;
            }
            write!(f, "{}", items)?;
            first = false
        }
        write!(f, "]")?;
        Ok(())
    }
}

// ----------------------------------------------
// Raw Materials
// ----------------------------------------------

bitflags_with_display! {
    #[derive(Copy, Clone, Debug, PartialEq, Eq)]
    pub struct RawMaterialKind: u32 {
        const Wood  = 1 << 0;
        const Metal = 1 << 1;
    }
}

impl RawMaterialKind {
    #[inline]
    pub const fn count() -> usize {
        Self::FLAGS.len()
    }
}

pub const RAW_MATERIAL_COUNT: usize = RawMaterialKind::count();
pub type RawMaterialsStock = Stock<RawMaterialKind, RAW_MATERIAL_COUNT>;
pub type RawMaterialsList  = List<RawMaterialKind,  RAW_MATERIAL_COUNT>;

// ----------------------------------------------
// Consumer Goods
// ----------------------------------------------

bitflags_with_display! {
    #[derive(Copy, Clone, Debug, PartialEq, Eq)]
    pub struct ConsumerGoodKind: u32 {
        // Foods:
        const Rice = 1 << 0;
        const Meat = 1 << 1;
        const Fish = 1 << 2;
    }
}

impl ConsumerGoodKind {
    #[inline]
    pub const fn count() -> usize {
        Self::FLAGS.len()
    }

    #[inline]
    pub fn any_food() -> Self {
        Self::Rice |
        Self::Meat |
        Self::Fish
    }
}

pub const CONSUMER_GOOD_COUNT: usize = ConsumerGoodKind::count();
pub type ConsumerGoodsStock = Stock<ConsumerGoodKind, CONSUMER_GOOD_COUNT>;
pub type ConsumerGoodsList  = List<ConsumerGoodKind,  CONSUMER_GOOD_COUNT>;

// ----------------------------------------------
// Services
// ----------------------------------------------

pub const SERVICES_COUNT: usize = BuildingKind::services_count();
pub type ServicesList = List<BuildingKind, SERVICES_COUNT>;

// ----------------------------------------------
// Workers
// ----------------------------------------------

pub struct Workers {
    pub count: u32, // Current number of workers employed.
    pub min: u32,   // Minimum number of workers for service/production to run (at lower capacity).
    pub max: u32,   // Maximum number of workers it can employ (to run at full capacity).
}

impl Workers {
    pub fn new(min: u32, max: u32) -> Self {
        debug_assert!(min <= max);
        Self {
            count: 0,
            min: min,
            max: max,
        }
    }
}
