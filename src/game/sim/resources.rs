use core::slice::{Iter, IterMut};
use bitflags::{bitflags, Flags};
use arrayvec::ArrayVec;
use std::fmt::Display;

use crate::{
    bitflags_with_display,
    imgui_ui::UiSystem,
    game::building::BuildingKind
};

// ----------------------------------------------
// Stock generic
// ----------------------------------------------

pub struct StockItem<T> {
    pub kind: T, // This is always a single bitflag; never multiple ORed together.
    pub count: u32,
}

pub struct Stock<T, const CAPACITY: usize> {
    items: ArrayVec<StockItem<T>, CAPACITY>,
}

impl<T, const CAPACITY: usize> Stock<T, CAPACITY> 
    where
        T: Copy + Display + bitflags::Flags
{
    #[inline]
    pub fn new(items_accepted: &List<T, CAPACITY>) -> Self {
        let mut stock = Self {
            items: ArrayVec::new(),
        };

        for item in items_accepted.iter() {
            stock.items.push(StockItem { kind: *item, count: 0 });
        }

        stock
    }

    #[inline]
    pub fn accept_all() -> Self {
        let mut stock = Self {
            items: ArrayVec::new(),
        };

        for item in T::FLAGS.iter() {
            stock.items.push(StockItem { kind: *item.value(), count: 0 });
        }

        stock
    }

    #[inline]
    pub fn iter(&self) -> Iter<'_, StockItem<T>> {
        self.items.iter()
    }

    #[inline]
    pub fn iter_mut(&mut self) -> IterMut<'_, StockItem<T>> {
        self.items.iter_mut()
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        for item in &self.items {
            if item.count != 0 {
                return false;
            }
        }
        true
    }

    #[inline]
    pub fn clear(&mut self) {
        for item in &mut self.items {
            item.count = 0;
        }
    }

    #[inline]
    pub fn has(&self, wanted: T) -> bool {
        for item in &self.items {
            if item.kind.intersects(wanted) && item.count != 0 {
                return true;
            }
        }
        false
    }

    #[inline]
    pub fn count(&mut self, wanted: T) -> u32 {
        for item in &self.items {
            if item.kind.intersects(wanted) {
                return item.count;
            }
        }
        panic!("Failed to find item '{}' to Stock!", wanted);
    }

    #[inline]
    pub fn add(&mut self, new_item: T) {
        for item in &mut self.items {
            if item.kind.intersects(new_item) {
                item.count += 1;
                return;
            }
        }
        panic!("Failed to add item '{}' to Stock!", new_item);
    }

    #[inline]
    pub fn remove(&mut self, wanted: T) -> Option<T> {
        for item in &mut self.items {
            if item.kind.intersects(wanted) && item.count != 0 {
                item.count -= 1;
                return Some(item.kind);
            }
        }
        None
    }

    pub fn draw_debug_ui(&mut self, label: &str, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();
        if ui.collapsing_header(format!("{}##_resource_stock", label), imgui::TreeNodeFlags::empty()) {
            for (index, item) in self.iter_mut().enumerate() {
                ui.input_scalar(format!("{}##_stock_item_{}", item.kind, index), &mut item.count)
                    .step(1)
                    .build();
            }
        }
    }

    pub fn draw_debug_ui_filtered<F>(&mut self, label: &str, ui_sys: &UiSystem, filter_fn: F)
        where F: Fn(&StockItem<T>) -> bool
    {
        let ui = ui_sys.builder();
        if ui.collapsing_header(format!("{}##_resource_stock", label), imgui::TreeNodeFlags::empty()) {
            for (index, item) in self.iter_mut().enumerate() {
                if filter_fn(item) {
                    ui.input_scalar(format!("{}##_stock_item_{}", item.kind, index), &mut item.count)
                        .step(1)
                        .build();
                }
            }
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
    where 
        T: Copy + Display + bitflags::Flags
{
    #[inline]
    pub fn empty() -> Self {
        Self {
            items: ArrayVec::new(),
        }
    }

    #[inline]
    pub fn all() -> Self {
        let mut list = Self {
            items: ArrayVec::new(),
        };

        for item in T::FLAGS.iter() {
            list.items.push(*item.value());
        }

        list
    }

    #[inline]
    pub fn new(items: &[T]) -> Self {
        Self {
            items: ArrayVec::try_from(items).expect("Cannot fit all items into List!"),
        }
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
    pub fn has(&self, wanted: T) -> bool {
        for item in &self.items {
            if item.intersects(wanted) {
                return true;
            }
        }
        false
    }

    #[inline]
    pub fn add(&mut self, new_item: T) {
        debug_assert!(!self.has(new_item));
        self.items.push(new_item);
    }

    // This will break down any flags that are ORed together into
    // individual calls to visitor_fn, unlike iter() which yields
    // combined flags as they are.
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
    where 
    T: Copy + Display + bitflags::Flags
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

const RAW_MATERIAL_COUNT: usize = RawMaterialKind::count();
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

const CONSUMER_GOOD_COUNT: usize = ConsumerGoodKind::count();
pub type ConsumerGoodsStock = Stock<ConsumerGoodKind, CONSUMER_GOOD_COUNT>;
pub type ConsumerGoodsList  = List<ConsumerGoodKind,  CONSUMER_GOOD_COUNT>;

// ----------------------------------------------
// Services
// ----------------------------------------------

const SERVICES_COUNT: usize = BuildingKind::services_count();
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
