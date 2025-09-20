use rand::seq::IteratorRandom;
use arrayvec::ArrayVec;
use smallvec::{smallvec, SmallVec};
use proc_macros::DrawDebugUi;

use serde::{
    Serialize,
    Deserialize
};

use crate::{
    game_object_debug_options,
    building_config,
    imgui_ui::UiSystem,
    save::PostLoadContext,
    tile::Tile,
    utils::{
        Color,
        hash::{self, StringHash}
    },
    game::{
        cheats,
        world::stats::WorldStats,
        unit::{
            Unit,
            task::{
                UnitTaskDeliverToStorage,
                UnitTaskFetchFromStorage,
            }
        },
        sim::resources::{
            ResourceKind,
            ResourceKinds,
            ResourceStock,
            StockItem,
            Workers
        }
    }
};

use super::{
    BuildingKind,
    BuildingBehavior,
    BuildingContext,
    config::BuildingConfig
};

// ----------------------------------------------
// StorageConfig
// ----------------------------------------------

#[derive(DrawDebugUi, Serialize, Deserialize)]
pub struct StorageConfig {
    pub kind: BuildingKind,

    pub name: String,
    pub tile_def_name: String,

    #[debug_ui(skip)]
    #[serde(skip)] // Not serialized. Computed on post_load.
    pub tile_def_name_hash: StringHash,

    pub min_workers: u32,
    pub max_workers: u32,

    // Resources we can store.
    pub resources_accepted: ResourceKinds,

    // Number of storage slots and capacity of each slot.
    pub num_slots: u32,
    pub slot_capacity: u32,
}

impl Default for StorageConfig {
    #[inline]
    fn default() -> Self {
        Self {
            kind: BuildingKind::StorageYard,
            name: "Storage Yard".into(),
            tile_def_name: "storage_yard".into(),
            tile_def_name_hash: hash::fnv1a_from_str("storage_yard"),
            min_workers: 1,
            max_workers: 4,
            resources_accepted: ResourceKinds::all(),
            num_slots: 8,
            slot_capacity: 4,
        }
    }
}

building_config! {
    StorageConfig
}

// ----------------------------------------------
// StorageDebug
// ----------------------------------------------

game_object_debug_options! {
    StorageDebug,
}

// ----------------------------------------------
// StorageBuilding
// ----------------------------------------------

#[derive(Clone, Serialize, Deserialize)]
pub struct StorageBuilding<'config> {
    #[serde(skip)] config: Option<&'config StorageConfig>,

    workers: Workers,

    // Stockpiles:
    storage_slots: Box<StorageSlots>,

    #[serde(skip)] debug: StorageDebug,
}

// ----------------------------------------------
// BuildingBehavior for StorageBuilding
// ----------------------------------------------

impl<'config> BuildingBehavior<'config> for StorageBuilding<'config> {
    // ----------------------
    // World Callbacks:
    // ----------------------

    fn name(&self) -> &str {
        &self.config.unwrap().name
    }

    fn configs(&self) -> &dyn BuildingConfig {
        self.config.unwrap()
    }

    fn update(&mut self, _context: &BuildingContext) {
        debug_assert!(self.config.is_some());
        // Nothing for now.
    }

    fn visited_by(&mut self, unit: &mut Unit, context: &BuildingContext) {
        if !self.has_min_required_workers() {
            return;
        }

        let task_manager = context.query.task_manager();

        if let Some(task) = unit.current_task_as::<UnitTaskDeliverToStorage>(task_manager) {
            debug_assert!(context.kind.intersects(task.storage_buildings_accepted));

            // Try unload cargo:
            if let Some(item) = unit.peek_inventory() {
                let received_count = self.receive_resources(item.kind, item.count);
                if received_count != 0 {
                    let removed_count = unit.remove_resources(item.kind, received_count);
                    debug_assert!(removed_count == received_count);

                    self.debug.popup_msg(format!("{} delivered {} {}", unit.name(), received_count, item.kind));
                }
            }
        } else if let Some(task) = unit.current_task_as::<UnitTaskFetchFromStorage>(task_manager) {
            debug_assert!(context.kind.intersects(task.storage_buildings_accepted));

            // Try give resources:
            for item in task.resources_to_fetch.iter() {
                let available_count = self.available_resources(item.kind);
                if available_count != 0 {
                    let max_fetch_count = available_count.min(item.count);
                    let removed_count = self.remove_resources(item.kind, max_fetch_count);

                    unit.receive_resources(item.kind, removed_count);
                    debug_assert!(removed_count == max_fetch_count);

                    self.debug.popup_msg(format!("{} fetched {} {}", unit.name(), max_fetch_count, item.kind));
                    break;
                }
            }
        } else {
            panic!("Unhandled Unit Task in StorageBuilding::visited_by()!");
        }
    }

    fn post_load(&mut self, context: &PostLoadContext<'_, 'config>, kind: BuildingKind, _tile: &Tile) {
        debug_assert!(kind.intersects(BuildingKind::storage()));
        self.config = Some(context.building_configs.find_storage_config(kind));
    }

    // ----------------------
    // Resources/Stock:
    // ----------------------

    fn is_stock_full(&self) -> bool {
        self.storage_slots.are_all_slots_full()
    }

    fn available_resources(&self, kind: ResourceKind) -> u32 {
        if self.has_min_required_workers() {
            return self.storage_slots.available_resources(kind);
        }
        0
    }

    fn receivable_resources(&self, kind: ResourceKind) -> u32 {
        if self.has_min_required_workers() {
            return self.storage_slots.receivable_resources(kind);
        }
        0
    }

    // Returns number of resources it was able to accommodate, which can be less than `count`.
    fn receive_resources(&mut self, kind: ResourceKind, count: u32) -> u32 {
        if count != 0 && self.has_min_required_workers() {
            let received_count = self.storage_slots.receive_resources(kind, count);
            self.debug.log_resources_gained(kind, received_count);
            return received_count;
        }
        0
    }

    fn remove_resources(&mut self, kind: ResourceKind, count: u32) -> u32 {
        if count != 0 && self.has_min_required_workers() {
            let removed_count = self.storage_slots.remove_resources(kind, count);
            self.debug.log_resources_lost(kind, removed_count);
            return removed_count;
        }
        0
    }

    fn tally(&self, stats: &mut WorldStats, kind: BuildingKind) {
        if kind.intersects(BuildingKind::StorageYard) {
            self.storage_slots.for_each_resource(|item| {
                stats.add_storage_yard_resources(item.kind, item.count);
            });
        } else if kind.intersects(BuildingKind::Granary) {
            self.storage_slots.for_each_resource(|item| {
                stats.add_granary_resources(item.kind, item.count);
            });
        } else {
            unimplemented!("Missing resource tally for storage kind {kind}.");
        }
    }

    // ----------------------
    // Workers:
    // ----------------------

    fn workers(&self) -> Option<&Workers> { Some(&self.workers) }
    fn workers_mut(&mut self) -> Option<&mut Workers> { Some(&mut self.workers) }

    #[inline]
    fn has_min_required_workers(&self) -> bool {
        if cheats::get().ignore_worker_requirements {
            return true;
        }
        self.workers.as_employer().unwrap().has_min_required()
    }

    // ----------------------
    // Debug:
    // ----------------------

    fn debug_options(&mut self) -> &mut dyn GameObjectDebugOptions {
        &mut self.debug
    }

    fn draw_debug_ui(&mut self, _context: &BuildingContext, ui_sys: &UiSystem) {
        self.storage_slots.draw_debug_ui("Stock Slots", ui_sys);
    }
}

// ----------------------------------------------
// StorageBuilding
// ----------------------------------------------

impl<'config> StorageBuilding<'config> {
    pub fn new(config: &'config StorageConfig) -> Self {
        Self {
            config: Some(config),
            workers: Workers::employer(config.min_workers, config.max_workers),
            storage_slots: StorageSlots::new(
                &config.resources_accepted,
                config.num_slots,
                config.slot_capacity
            ),
            debug: StorageDebug::default(),
        }
    }

    pub fn register_callbacks() {
    }
}

// ----------------------------------------------
// StorageSlots
// ----------------------------------------------

const MAX_STORAGE_SLOTS: usize = 8;

#[derive(Clone, Serialize, Deserialize)]
struct StorageSlot {
    stock: ResourceStock,
    allocated_resource_kind: Option<ResourceKind>,
}

#[derive(Clone, Serialize, Deserialize)]
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

    fn remaining_capacity(&self, slot_capacity: u32) -> u32 {
        if let Some(kind) = self.allocated_resource_kind {
            let count = self.stock.count(kind);
            debug_assert!(count <= slot_capacity);
            return slot_capacity - count;
        }
        slot_capacity // free
    }

    fn clear(&mut self) {
        self.stock.clear();
        self.allocated_resource_kind = None;
    }

    fn resource_index_and_count(&self, kind: ResourceKind) -> (usize, u32) {
        debug_assert!(kind.is_single_resource());
        let (index, item) = self.stock.find(kind)
            .unwrap_or_else(|| panic!("Resource kind '{}' expected to exist in the stock!", kind));
        (index, item.count)
    }

    fn set_resource_count_internal(&mut self, index: usize, count: u32) {
        let kind = self.allocated_resource_kind.unwrap();
        self.stock.set(index, StockItem { kind, count });
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
            self.set_resource_count_internal(index, count);
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

            self.set_resource_count_internal(index, count);

            if count == 0 {
                self.allocated_resource_kind = None;
            }
        }

        count
    }

    fn for_each_accepted_resource<F>(&self, mut visitor_fn: F)
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

        Box::new(Self { slots, slot_capacity })
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
        debug_assert!(kind.is_single_resource());
        self.slots[slot_index].resource_index_and_count(kind).1
    }

    #[inline]
    fn increment_slot_resource_count(&mut self, slot_index: usize, kind: ResourceKind, add_amount: u32) -> u32 {
        debug_assert!(kind.is_single_resource());
        self.slots[slot_index].increment_resource_count(kind, add_amount, self.slot_capacity)
    }

    #[inline]
    fn decrement_slot_resource_count(&mut self, slot_index: usize, kind: ResourceKind, sub_amount: u32) -> u32 {
        debug_assert!(kind.is_single_resource());
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
    fn find_resource_slot(&self, kind: ResourceKind) -> Option<usize> {
        // Should be a single kind, never multiple ORed flags.
        debug_assert!(kind.is_single_resource());

        for (slot_index, slot) in self.slots.iter().enumerate() {
            if let Some(allocated_kind) = slot.allocated_resource_kind {
                if allocated_kind == kind {
                    return Some(slot_index);
                }
            }
        }
        None
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

    fn alloc_resource_slot(&mut self, kind: ResourceKind) -> Option<usize> {
        // Should be a single kind, never multiple ORed flags.
        debug_assert!(kind.is_single_resource());

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

    fn available_resources(&self, kind: ResourceKind) -> u32 {
        if let Some(slot_index) = self.find_resource_slot(kind) {
            self.slot_resource_count(slot_index, kind)
        } else {
            0
        }
    }

    fn receivable_resources(&self, kind: ResourceKind) -> u32 {
        // Should be a single kind, never multiple ORed flags.
        debug_assert!(kind.is_single_resource());
        let mut count = 0;

        for slot in &self.slots {
            if slot.is_free() {
                count += self.slot_capacity;
            } else if let Some(allocated_kind) = slot.allocated_resource_kind {
                if allocated_kind == kind {
                    count += slot.remaining_capacity(self.slot_capacity);
                }
            }
        }

        count
    }

    fn receive_resources(&mut self, kind: ResourceKind, count: u32) -> u32 {
        let slot_index = match self.alloc_resource_slot(kind) {
            Some(slot_index) => slot_index,
            None => return 0,
        };

        let prev_count =
            self.slot_resource_count(slot_index, kind);

        let new_count =
            self.increment_slot_resource_count(slot_index, kind, count);

        new_count - prev_count
    }

    fn remove_resources(&mut self, kind: ResourceKind, count: u32) -> u32 {
        let slot_index = match self.find_resource_slot(kind) {
            Some(slot_index) => slot_index,
            None => return 0,
        };

        let prev_count =
            self.slot_resource_count(slot_index, kind);

        let new_count =
            self.decrement_slot_resource_count(slot_index, kind, count);

        prev_count - new_count
    }

    fn for_each_resource<F>(&self, mut visitor_fn: F)
        where F: FnMut(&StockItem)
    {
        for slot in &self.slots {
            if let Some(allocated_kind) = slot.allocated_resource_kind {
                let (_, item) = slot.stock.find(allocated_kind).unwrap();
                visitor_fn(&item);
            }
        }
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
        if !ui.collapsing_header(label, imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        if ui.button("Fill up all slots") {
            let add_amount = self.slot_capacity;
            let slot_capacity = self.slot_capacity;

            for slot in &mut self.slots {
                if let Some(allocated_kind) = slot.allocated_resource_kind {
                    // Fill up slot with existing resource.
                    slot.increment_resource_count(allocated_kind, add_amount, slot_capacity);    
                } else {
                    let accepted_kinds = slot.stock.accepted_kinds();
    
                    // Pick a random resource kind from the accepted kinds.
                    let mut rng = rand::rng();
                    let random_kind = accepted_kinds
                        .iter()
                        .choose(&mut rng)
                        .unwrap_or(ResourceKind::Rice);

                    slot.increment_resource_count(random_kind, add_amount, slot_capacity);
                }
            }
        }

        if ui.button("Clear all slots") {
            for slot in &mut self.slots {
                slot.clear();
            }
        }

        ui.separator();

        let mut display_slots: SmallVec<[SmallVec<[ResourceKind; 8]>; MAX_STORAGE_SLOTS]> =
            smallvec![SmallVec::new(); MAX_STORAGE_SLOTS];

        for (slot_index, slot) in self.slots.iter().enumerate() {
            if let Some(allocated_kind) = slot.allocated_resource_kind {
                // Display only the allocated resource kind.
                display_slots[slot_index].push(allocated_kind);
            } else {
                // No resource allocated for the slot, display all possible resource kinds accepted.
                slot.for_each_accepted_resource(|kind| {
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
                        match new_count.cmp(&prev_count) {
                            std::cmp::Ordering::Greater => {
                                new_count = self.increment_slot_resource_count(
                                    slot_index, *res_kind, new_count - prev_count);
                            },
                            std::cmp::Ordering::Less => {
                                new_count = self.decrement_slot_resource_count(
                                    slot_index, *res_kind, prev_count - new_count);
                            },
                            std::cmp::Ordering::Equal => {} // nothing
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
