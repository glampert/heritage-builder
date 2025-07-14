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
// Resources (Consumer Goods, Raw Materials)
// ----------------------------------------------

bitflags_with_display! {
    #[derive(Copy, Clone, Debug, PartialEq, Eq)]
    pub struct ResourceKind: u32 {
        // Foods:
        const Rice  = 1 << 0;
        const Meat  = 1 << 1;
        const Fish  = 1 << 2;

        // Raw materials:
        const Wood  = 1 << 3;
        const Metal = 1 << 4;
    }
}

impl ResourceKind {
    #[inline]
    pub const fn count() -> usize {
        Self::FLAGS.len()
    }

    #[inline]
    pub const fn any_food() -> Self {
        Self::from_bits_retain(
            Self::Rice.bits() |
            Self::Meat.bits() |
            Self::Fish.bits()
        )
    }
}

const RESOURCE_KIND_COUNT: usize = ResourceKind::count();
pub type ResourceKinds = ResourceList<ResourceKind, RESOURCE_KIND_COUNT>;

// ----------------------------------------------
// Services
// ----------------------------------------------

const SERVICE_KIND_COUNT: usize = BuildingKind::services_count();
pub type ServiceKinds = ResourceList<BuildingKind, SERVICE_KIND_COUNT>;

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
    let bits: u32 = kind.bits().into();
    debug_assert!(bits.count_ones() == 1);
    bits.trailing_zeros() as usize
}

impl ResourceStock {
    #[inline]
    #[must_use]
    pub fn with_accepted_resources(accepted_resources: &ResourceKinds) -> Self {
        let mut stock = Self {
            kinds: ResourceKind::empty(),
            counts: [0; RESOURCE_KIND_COUNT],
        };

        accepted_resources.for_each(|kind| {
            stock.kinds.set(kind, true);
            true
        });

        stock
    }

    #[inline]
    #[must_use]
    pub fn accept_all_resources() -> Self {
        let mut stock = Self {
            kinds: ResourceKind::empty(),
            counts: [0; RESOURCE_KIND_COUNT],
        };

        for flag in ResourceKind::FLAGS.iter() {
            stock.kinds.set(*flag.value(), true);
        }

        stock
    }

    #[inline]
    pub fn accepted_resources_count(&self) -> usize {
        self.counts.len()
    }

    #[inline]
    pub fn accepts_any_resource(&self) -> bool {
        self.accepted_resources_count() != 0
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        for count in self.counts {
            if count != 0 {
                return false;
            }
        }
        true
    }

    #[inline]
    pub fn clear(&mut self) {
        for count in &mut self.counts {
            *count = 0;
        }
    }

    #[inline]
    pub fn has(&self, kinds: ResourceKind) -> bool {
        // Break down flags that are ORed together (since T is bitflags),
        // so that has() can work with multiple wanted kinds, e.g.:
        // has(A | B | C) -> returns true if any A|B|C is non-zero
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
        let item = StockItem { kind: kind, count: count.into() };
        return Some((index, item));
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
    pub fn add(&mut self, kind: ResourceKind) {
        if !self.kinds.intersects(kind) {
            panic!("Failed to add resource of kind '{}' to Stock! Kind not accepted.", kind);
        }
        let index = bit_index(kind);
        self.counts[index] += 1;
    }

    #[inline]
    pub fn remove(&mut self, kinds: ResourceKind) -> Option<ResourceKind> {
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
                self.counts[index] = count - 1;
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
            let item = StockItem { kind: kind, count: count.into() };
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
            let mut item = StockItem { kind: kind, count: count.into() };
            visitor_fn(index, &mut item);
            self.counts[index] = item.count.try_into().expect("Value cannot fit into a u16!");
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
        where F: Fn(&StockItem) -> bool
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
// ResourceList generic
// ----------------------------------------------

pub struct ResourceList<T, const CAPACITY: usize> {
    kinds: ArrayVec<T, CAPACITY>, // Each item can be a single bitflag or multiple ORed together.
}

impl<T, const CAPACITY: usize> ResourceList<T, CAPACITY> 
    where T: Copy + Display + bitflags::Flags
{
    #[inline]
    #[must_use]
    pub fn empty() -> Self {
        Self {
            kinds: ArrayVec::new(),
        }
    }

    #[inline]
    #[must_use]
    pub fn all_kinds() -> Self {
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
        debug_assert!(!self.has(kind));
        self.kinds.push(kind);
    }

    #[inline]
    pub fn has(&self, kinds: T) -> bool {
        for kind in &self.kinds {
            if kind.intersects(kinds) {
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
        for kinds in &self.kinds {
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
