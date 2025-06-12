use core::slice::Iter;
use std::marker::PhantomData;
use arrayvec::ArrayVec;
use smallvec::SmallVec;
use num_enum::IntoPrimitive;
use strum::{EnumCount, IntoEnumIterator};
use strum_macros::{EnumCount, EnumIter};

use crate::{
    game::building::{
        BuildingKind
    }
};

// ----------------------------------------------
// Stock generic
// ----------------------------------------------

pub struct StockItem {
    pub count: u32,
}

pub struct Stock<T, const CAPACITY: usize> {
    items: ArrayVec<StockItem, CAPACITY>,
    _marker: PhantomData<T>, // Prevents a compilation error since T is not referenced here.
}

impl<T, const CAPACITY: usize> Stock<T, CAPACITY> 
where
    T: IntoEnumIterator + Into<u32> + Copy {

    #[inline]
    pub fn new() -> Self {
        let mut stock = Self {
            items: ArrayVec::new(),
            _marker: PhantomData,
        };

        for _ in T::iter() {
            stock.items.push(StockItem { count: 0 });
        }

        stock
    }

    #[inline]
    pub fn iter(&self) -> Iter<'_, StockItem> {
        self.items.iter()
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    #[inline]
    pub fn has(&self, wanted: T) -> bool {
        let index: u32 = wanted.into();
        self.items[index as usize].count != 0
    }
}

// ----------------------------------------------
// List generic
// ----------------------------------------------

pub struct List<T> {
    items: SmallVec<[T; 1]>,
}

impl<T> List<T> 
where 
    T: Copy + PartialEq {

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
    pub fn len(&self) -> usize {
        self.items.len()
    }

    #[inline]
    pub fn has(&self, wanted: T) -> bool {
        for item in &self.items {
            if *item == wanted {
                return true;
            }
        }
        false
    }
}

// ----------------------------------------------
// Raw Materials
// ----------------------------------------------

#[repr(u32)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, EnumCount, EnumIter, IntoPrimitive)]
pub enum RawMaterialKind {
    Wood,
    Metal,
}

pub const RAW_MATERIAL_COUNT: usize = RawMaterialKind::COUNT;
pub type RawMaterialsStock = Stock<RawMaterialKind, RAW_MATERIAL_COUNT>;
pub type RawMaterialsList = List<RawMaterialKind>;

// ----------------------------------------------
// Consumer Goods
// ----------------------------------------------

#[repr(u32)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, EnumCount, EnumIter, IntoPrimitive)]
pub enum ConsumerGoodKind {
    Rice,
    Meat,
    Fish,
}

pub const CONSUMER_GOOD_COUNT: usize = ConsumerGoodKind::COUNT;
pub type ConsumerGoodsStock = Stock<ConsumerGoodKind, CONSUMER_GOOD_COUNT>;
pub type ConsumerGoodsList = List<ConsumerGoodKind>;

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
