use core::slice::{Iter, IterMut};
use arrayvec::ArrayVec;
use smallvec::SmallVec;
use bitflags::{bitflags, Flags};

use crate::{
    bitflags_with_display,
    game::building::BuildingKind
};

// ----------------------------------------------
// Stock generic
// ----------------------------------------------

pub struct StockItem<T> {
    pub kind: T,
    pub count: u32,
}

pub struct Stock<T, const CAPACITY: usize> {
    items: ArrayVec<StockItem<T>, CAPACITY>,
}

impl<T, const CAPACITY: usize> Stock<T, CAPACITY> 
    where
        T: Copy + std::fmt::Display + bitflags::Flags
{
    #[inline]
    pub fn new() -> Self {
        let mut stock = Self {
            items: ArrayVec::new(),
        };

        for kind in T::FLAGS.iter() {
            stock.items.push(StockItem { kind: *kind.value(), count: 0 });
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
    pub fn has(&self, wanted: T) -> bool {
        for item in &self.items {
            if item.kind.intersects(wanted) && item.count != 0 {
                return true;
            }
        }
        false
    }

    #[inline]
    pub fn consume(&mut self, wanted: T) -> Option<T> {
        for item in &mut self.items {
            if item.kind.intersects(wanted) && item.count != 0 {
                item.count -= 1;
                return Some(item.kind);
            }
        }
        None
    }

    #[inline]
    pub fn add(&mut self, new: T) {
        for item in &mut self.items {
            if item.kind.intersects(new) {
                item.count += 1;
                return;
            }
        }
        panic!("Failed to add item '{}' to Stock!", new);
    }
}

// ----------------------------------------------
// List generic
// ----------------------------------------------

#[derive(Debug)]
pub struct List<T> {
    items: SmallVec<[T; 1]>,
}

impl<T> List<T> 
    where 
        T: Copy + bitflags::Flags
{
    #[inline]
    pub fn new() -> Self {
        Self {
            items: SmallVec::new(),
        }
    }

    #[inline]
    pub fn from_slice(items: &[T]) -> Self {
        Self {
            items: SmallVec::from_slice(items),
        }
    }

    #[inline]
    pub fn iter(&self) -> Iter<'_, T> {
        self.items.iter()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.items.len()
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
    pub fn clear(&mut self) {
        self.items.clear();
    }

    #[inline]
    pub fn add(&mut self, new: T) {
        self.items.push(new);
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
        RawMaterialKind::FLAGS.len()
    }
}

const RAW_MATERIAL_COUNT: usize = RawMaterialKind::count();
pub type RawMaterialsStock = Stock<RawMaterialKind, RAW_MATERIAL_COUNT>;
pub type RawMaterialsList  = List<RawMaterialKind>;

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
        ConsumerGoodKind::FLAGS.len()
    }

    #[inline]
    pub fn any_food() -> Self {
        ConsumerGoodKind::Rice |
        ConsumerGoodKind::Meat |
        ConsumerGoodKind::Fish
    }
}

const CONSUMER_GOOD_COUNT: usize = ConsumerGoodKind::count();
pub type ConsumerGoodsStock = Stock<ConsumerGoodKind, CONSUMER_GOOD_COUNT>;
pub type ConsumerGoodsList  = List<ConsumerGoodKind>;

// ----------------------------------------------
// Services
// ----------------------------------------------

pub type ServicesList = List<BuildingKind>;

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
