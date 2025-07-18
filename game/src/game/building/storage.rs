use arrayvec::ArrayVec;
use smallvec::{smallvec, SmallVec};
use proc_macros::DrawDebugUi;

use crate::{
    building_debug_options,
    imgui_ui::UiSystem,
    utils::{
        Color,
        Seconds,
        hash::StringHash,
        coords::{CellRange, WorldToScreenTransform}
    },
    game::sim::resources::{
        ResourceKind,
        ResourceKinds,
        ResourceStock,
        StockItem,
        Workers
    }
};

use super::{
    BuildingKind,
    BuildingBehavior,
    BuildingContext,
    config::BuildingConfigs
};

// ----------------------------------------------
// StorageConfig
// ----------------------------------------------

#[derive(DrawDebugUi)]
pub struct StorageConfig {
    pub tile_def_name: String,

    #[debug_ui(skip)]
    pub tile_def_name_hash: StringHash,

    pub min_workers: u32,
    pub max_workers: u32,

    // Resources we can store.
    pub resources_accepted: ResourceKinds,

    // Number of storage slots and capacity of each slot.
    pub num_slots: u32,
    pub slot_capacity: u32,
}

// ----------------------------------------------
// StorageDebug
// ----------------------------------------------

building_debug_options!(
    StorageDebug,
);

// ----------------------------------------------
// StorageBuilding
// ----------------------------------------------

pub struct StorageBuilding<'config> {
    config: &'config StorageConfig,
    workers: Workers,

    // Stockpiles:
    storage_slots: Box<StorageSlots>,

    debug: StorageDebug,
}

impl<'config> BuildingBehavior<'config> for StorageBuilding<'config> {
    fn update(&mut self, _context: &mut BuildingContext, _delta_time_secs: Seconds) {
        // Nothing for now.
    }

    fn draw_debug_ui(&mut self, _context: &mut BuildingContext, ui_sys: &UiSystem) {
        if ui_sys.builder().collapsing_header("Config", imgui::TreeNodeFlags::empty()) {
            self.config.draw_debug_ui(ui_sys);
        }
        self.debug.draw_debug_ui(ui_sys);
        self.storage_slots.draw_debug_ui("Stock Slots", ui_sys);
    }

    fn draw_debug_popups(&mut self,
                         context: &BuildingContext<'config, '_, '_, '_, '_>,
                         ui_sys: &UiSystem,
                         transform: &WorldToScreenTransform,
                         visible_range: CellRange,
                         delta_time_secs: Seconds,
                         show_popup_messages: bool) {

        self.debug.draw_popup_messages(context, ui_sys, transform, visible_range, delta_time_secs, show_popup_messages);
    }
}

impl<'config> StorageBuilding<'config> {
    pub fn new(kind: BuildingKind, configs: &'config BuildingConfigs) -> Self {
        let config = configs.find_storage_config(kind);
        Self {
            config: config,
            workers: Workers::new(config.min_workers, config.max_workers),
            storage_slots: StorageSlots::new(
                &config.resources_accepted,
                config.num_slots,
                config.slot_capacity
            ),
            debug: StorageDebug::default(),
        }
    }

    pub fn is_full(&self) -> bool {
        self.storage_slots.are_all_slots_full()
    }

    // Returns number of received resources.
    pub fn receive_resources(&mut self, item: StockItem) -> u32 {
        let added = self.storage_slots.try_add_resources(item);
        if added != 0 {
            self.debug.log_resources_gained(item.kind, added);
        }
        added
    }

    pub fn shop(&mut self,
                shopping_basket: &mut ResourceStock,
                shopping_list: &ResourceKinds,
                all_or_nothing: bool) -> ResourceKind {

        if all_or_nothing {
            for &wanted_resource in shopping_list.iter() {
                let slot_index = match self.storage_slots.find_resource_slot(wanted_resource) {
                    Some(slot_index) => slot_index,
                    None => return ResourceKind::empty(),
                };
                // If we have a slot allocated for this resource its count must not be zero.
                debug_assert!(self.storage_slots.slot_resource_count(slot_index, wanted_resource) != 0);
            }      
        }

        let mut kinds_added_to_basked = ResourceKind::empty();

        for &wanted_resource in shopping_list.iter() {
            let slot_index = match self.storage_slots.find_resource_slot(wanted_resource) {
                Some(slot_index) => slot_index,
                None => continue,
            };

            let prev_count = self.storage_slots.slot_resource_count(slot_index, wanted_resource);
            let new_count  = self.storage_slots.decrement_slot_resource_count(slot_index, wanted_resource, 1);

            if new_count < prev_count {
                shopping_basket.add(wanted_resource);
                kinds_added_to_basked.insert(wanted_resource);
                self.debug.log_resources_lost(wanted_resource, 1);
            }
        }

        kinds_added_to_basked
    }
}

// ----------------------------------------------
// StorageSlots
// ----------------------------------------------

const MAX_STORAGE_SLOTS: usize = 8;

struct StorageSlot {
    stock: ResourceStock,
    allocated_resource_kind: Option<ResourceKind>,
}

struct StorageSlots {
    slots: ArrayVec<StorageSlot, MAX_STORAGE_SLOTS>,
    slot_capacity: u32,
}

impl StorageSlot {
    #[inline]
    fn is_free(&self) -> bool {
        self.allocated_resource_kind.is_none()
    }

    fn is_full(&self, slot_capacity: u32) -> bool {
        if let Some(kind) = self.allocated_resource_kind {
            let count = self.stock.count(kind);
            if count >= slot_capacity {
                return true;
            }
        }
        false
    }

    fn resource_index_and_count(&self, kind: ResourceKind) -> (usize, u32) {
        let (index, item) = self.stock.find(kind)
            .expect(&format!("Resource kind '{}' expected to exist in the stock!", kind));
        (index, item.count)
    }

    fn set_resource_count(&mut self, index: usize, count: u32) {
        let kind = self.allocated_resource_kind.unwrap();
        self.stock.set(index, StockItem { kind: kind, count: count });
    }

    fn increment_resource_count(&mut self, kind: ResourceKind, add_amount: u32, slot_capacity: u32) -> u32 {
        let (index, mut count) = self.resource_index_and_count(kind);

        let prev_count = count;
        count = (prev_count + add_amount).min(slot_capacity);

        if let Some(allocated_kind) = self.allocated_resource_kind {
            if allocated_kind != kind {
                panic!("Storage slot can only accept '{}'!", kind);
            }
        } else {
            debug_assert!(prev_count == 0);
            self.allocated_resource_kind = Some(kind);
        }

        if count != prev_count {
            self.set_resource_count(index, count);
        }

        count
    }

    fn decrement_resource_count(&mut self, kind: ResourceKind, sub_amount: u32) -> u32 {
        let (index, mut count) = self.resource_index_and_count(kind);

        if count != 0 {
            count = count.saturating_sub(sub_amount);

            // If we have a non-zero item count we must have an allocated item and its kind must match.
            if self.allocated_resource_kind.unwrap() != kind {
                panic!("Storage slot can only accept '{}'!", kind);
            }

            self.set_resource_count(index, count);

            if count == 0 {
                self.allocated_resource_kind = None;
            }
        }

        count
    }

    fn for_each_resource_kind<F>(&self, mut visitor_fn: F)
        where F: FnMut(ResourceKind)
    {
        self.stock.for_each(|_, item| {
            visitor_fn(item.kind);
        });
    }
}

impl StorageSlots {
    fn new(resources_accepted: &ResourceKinds, num_slots: u32, slot_capacity: u32) -> Box<Self> {
        if resources_accepted.is_empty() || num_slots == 0 || slot_capacity == 0 {
            panic!("Storage building must have a non-zero number of slots, slot capacity and a list of accepted resources!");
        }

        let mut slots = ArrayVec::new();

        for _ in 0..num_slots {
            slots.push(StorageSlot {
                stock: ResourceStock::with_accepted_list(resources_accepted),
                allocated_resource_kind: None,
            });
        }

        Box::new(Self { slots: slots, slot_capacity: slot_capacity })
    }

    #[inline]
    fn is_slot_free(&self, slot_index: usize) -> bool {
        self.slots[slot_index].is_free()
    }

    #[inline]
    fn is_slot_full(&self, slot_index: usize) -> bool {
        self.slots[slot_index].is_full(self.slot_capacity)
    }

    #[inline]
    fn slot_resource_count(&self, slot_index: usize, kind: ResourceKind) -> u32 {
        self.slots[slot_index].resource_index_and_count(kind).1
    }

    #[inline]
    fn increment_slot_resource_count(&mut self, slot_index: usize, kind: ResourceKind, add_amount: u32) -> u32 {
        self.slots[slot_index].increment_resource_count(kind, add_amount, self.slot_capacity)
    }

    #[inline]
    fn decrement_slot_resource_count(&mut self, slot_index: usize, kind: ResourceKind, sub_amount: u32) -> u32 {
        self.slots[slot_index].decrement_resource_count(kind, sub_amount)
    }

    #[inline]
    fn are_all_slots_full(&self) -> bool {
        for (slot_index, _) in self.slots.iter().enumerate() {
            if !self.is_slot_full(slot_index) {
                return false;
            }
        }
        true
    }

    #[inline]
    fn find_free_slot(&self) -> Option<usize> {
        for (slot_index, slot) in self.slots.iter().enumerate() {
            if slot.is_free() {
                return Some(slot_index);
            }
        }
        None
    }

    #[inline]
    fn find_resource_slot(&self, kind: ResourceKind) -> Option<usize> {
        for (slot_index, slot) in self.slots.iter().enumerate() {
            if let Some(allocated_kind) = slot.allocated_resource_kind {
                if allocated_kind == kind {
                    return Some(slot_index);
                }
            }
        }
        None
    }

    fn alloc_resource_slot(&mut self, kind: ResourceKind) -> Option<usize> {
        // Should be a single kind, never multiple ORed flags.
        debug_assert!(kind.bits().count_ones() == 1);

        // See if this resource kind is already being stored somewhere:
        for (slot_index, slot) in self.slots.iter().enumerate() {
            if let Some(allocated_kind) = slot.allocated_resource_kind {
                if allocated_kind == kind && !self.is_slot_full(slot_index) {
                    return Some(slot_index);
                }
            }
        }

        // Not in storage yet or other slots are full, see if we can allocate a new slot for it:
        self.find_free_slot()
    }

    // Returns number of added resources.
    fn try_add_resources(&mut self, item: StockItem) -> u32 {
        let kind = item.kind;
        let add_amount = item.count;

        let slot_index = match self.alloc_resource_slot(kind) {
            Some(slot_index) => slot_index,
            None => return 0,
        };

        let prev_count =
            self.slot_resource_count(slot_index, kind);

        let new_count =
            self.increment_slot_resource_count(slot_index, kind, add_amount);

        new_count - prev_count
    }
}

// ----------------------------------------------
// Debug UI
// ----------------------------------------------

impl StorageSlots {
    fn draw_debug_ui(&mut self, label: &str, ui_sys: &UiSystem) {
        if self.slots.is_empty() {
            return;
        }

        let ui = ui_sys.builder();

        if ui.collapsing_header(label, imgui::TreeNodeFlags::empty()) {
            let mut display_slots: SmallVec<[SmallVec<[ResourceKind; 8]>; MAX_STORAGE_SLOTS]> =
                smallvec![SmallVec::new(); MAX_STORAGE_SLOTS];

            for (slot_index, slot) in self.slots.iter().enumerate() {
                if let Some(allocated_kind) = slot.allocated_resource_kind {
                    // Display only the allocated resource kind.
                    display_slots[slot_index].push(allocated_kind);
                } else {
                    // No resource allocated for the slot, display all possible resource kinds accepted.
                    slot.for_each_resource_kind(|kind| {
                        display_slots[slot_index].push(kind);
                    });
                }
            }

            ui.indent_by(10.0);
            for (slot_index, slot) in display_slots.iter().enumerate() {
                let slot_label = {
                    if self.is_slot_free(slot_index) {
                        format!("Slot {} (Free)", slot_index)
                    } else {
                        format!("Slot {} ({})", slot_index, display_slots[slot_index].last().unwrap())
                    }
                };

                let header_label =
                    format!("{}##_stock_slot_{}", slot_label, slot_index);

                if ui.collapsing_header(header_label, imgui::TreeNodeFlags::DEFAULT_OPEN) {
                    for (res_index, res_kind) in slot.iter().enumerate() {
                        let res_label =
                            format!("{}##_stock_item_{}_slot_{}", res_kind, res_index, slot_index);

                        let prev_count = self.slot_resource_count(slot_index, *res_kind);
                        let mut new_count = prev_count;

                        if ui.input_scalar(res_label, &mut new_count).step(1).build() {
                            if new_count > prev_count {
                                new_count = self.increment_slot_resource_count(
                                    slot_index, *res_kind, new_count - prev_count);
                            } else if new_count < prev_count {
                                new_count = self.decrement_slot_resource_count(
                                    slot_index, *res_kind, prev_count - new_count);
                            }
                        }

                        let capacity_left = self.slot_capacity - new_count;
                        let is_full = new_count >= self.slot_capacity;

                        ui.same_line();
                        if is_full {
                            ui.text_colored(Color::red().to_array(), "(full)");
                        } else {
                            ui.text(format!("({} left)", capacity_left));
                        }
                    }
                }
            }
            ui.unindent_by(10.0);
        }
    }
}
