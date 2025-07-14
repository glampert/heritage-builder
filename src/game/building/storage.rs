use arrayvec::ArrayVec;
use smallvec::{smallvec, SmallVec};

use crate::{
    imgui_ui::UiSystem,
    utils::{
        Color,
        Seconds,
        hash::StringHash
    },
    game::sim::resources::{
        ConsumerGoodKind,
        ConsumerGoodsList,
        ConsumerGoodsStock,
        RawMaterialKind,
        RawMaterialsList,
        RawMaterialsStock,
        Workers,
        StockItem,
    }
};

use super::{
    BuildingKind,
    BuildingBehavior,
    BuildingUpdateContext,
    config::BuildingConfigs
};

// ----------------------------------------------
// StorageConfig
// ----------------------------------------------

pub struct StorageConfig {
    pub tile_def_name: String,
    pub tile_def_name_hash: StringHash,

    pub min_workers: u32,
    pub max_workers: u32,

    // Goods/raw materials it can store.
    pub accepted_goods: ConsumerGoodsList,
    pub accepted_raw_materials: RawMaterialsList,

    // Number of storage slots and capacity of each slot.
    pub num_slots: u32,
    pub slot_capacity: u32,
}

// ----------------------------------------------
// StorageBuilding
// ----------------------------------------------

pub struct StorageBuilding<'config> {
    config: &'config StorageConfig,
    workers: Workers,

    // Stockpiles:
    storage_slots: Box<StorageSlots>,
}

impl<'config> BuildingBehavior<'config> for StorageBuilding<'config> {
    fn update(&mut self, _update_ctx: &mut BuildingUpdateContext, _delta_time_secs: Seconds) {
        // Nothing for now.
    }

    fn draw_debug_ui(&mut self, ui_sys: &UiSystem) {
        self.draw_debug_ui_storage_config(ui_sys);
        self.storage_slots.draw_debug_ui("Stock Items", ui_sys);
    }
}

impl<'config> StorageBuilding<'config> {
    pub fn new(kind: BuildingKind, configs: &'config BuildingConfigs) -> Self {
        let config = configs.find::<StorageConfig>(kind);
        Self {
            config: config,
            workers: Workers::new(config.min_workers, config.max_workers),
            storage_slots: StorageSlots::new(
                &config.accepted_goods,
                &config.accepted_raw_materials,
                config.num_slots,
                config.slot_capacity
            ),
        }
    }

    pub fn is_full(&self) -> bool {
        self.storage_slots.are_all_slots_full()
    }

    // Returns true if *any number* of items can be stored.
    // Decrements the number of stored items from the argument if successful.
    pub fn try_receive_materials(&mut self, materials: &mut StockItem<RawMaterialKind>) -> bool {
        self.storage_slots.try_add_materials(materials)
    }

    pub fn try_receive_goods(&mut self, goods: &mut StockItem<ConsumerGoodKind>) -> bool {
        self.storage_slots.try_add_goods(goods)
    }
}

// ----------------------------------------------
// StorageSlots
// ----------------------------------------------

const MAX_STORAGE_SLOTS: usize = 8;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum StorageItemKind {
    ConsumerGood(ConsumerGoodKind),
    RawMaterial(RawMaterialKind),
}

struct StorageSlot {
    goods: ConsumerGoodsStock,
    materials: RawMaterialsStock,
    allocated_item_kind: Option<StorageItemKind>,
}

struct StorageSlots {
    slots: ArrayVec<StorageSlot, MAX_STORAGE_SLOTS>,
    slot_capacity: u32,
}

impl StorageSlot {
    #[inline]
    fn is_free(&self) -> bool {
        self.allocated_item_kind.is_none()
    }

    fn is_full(&self, slot_capacity: u32) -> bool {
        if let Some(allocated_item_kind) = self.allocated_item_kind {
            let count = match allocated_item_kind {
                StorageItemKind::ConsumerGood(good) => {
                    self.goods.count(good)
                },
                StorageItemKind::RawMaterial(material) => {
                    self.materials.count(material)
                },
            };
            if count >= slot_capacity {
                return true;
            }
        }
        false
    }

    fn item_count(&self, item_kind: StorageItemKind) -> (usize, u32) {
        match item_kind {
            StorageItemKind::ConsumerGood(good) => {
                let (item_index, item) = self.goods.find(good)
                    .expect(&format!("Item {} expected to exist in the stock!", item_kind));
                (item_index, item.count)
            },
            StorageItemKind::RawMaterial(material) => {
                let (item_index, item) = self.materials.find(material)
                    .expect(&format!("Item {} expected to exist in the stock!", item_kind));
                (item_index, item.count)
            },
        }
    }

    fn set_item_count(&mut self, item_index: usize, item_count: u32) {
        match self.allocated_item_kind.unwrap() {
            StorageItemKind::ConsumerGood(good) => {
                self.goods.set(item_index, StockItem { kind: good, count: item_count });
            },
            StorageItemKind::RawMaterial(material) => {
                self.materials.set(item_index, StockItem { kind: material, count: item_count });
            },
        }
    }

    fn increment_item_count(&mut self, item_kind: StorageItemKind, add_amount: u32, slot_capacity: u32) -> u32 {
        let (item_index, mut item_count) = self.item_count(item_kind);

        let prev_count = item_count;
        item_count = (prev_count + add_amount).min(slot_capacity);

        if let Some(allocated_item_kind) = self.allocated_item_kind {
            if allocated_item_kind != item_kind {
                panic!("Storage slot can only accept {}!", item_kind);
            }
        } else {
            debug_assert!(prev_count == 0);
            self.allocated_item_kind = Some(item_kind);
        }

        if item_count != prev_count {
            self.set_item_count(item_index, item_count);
        }

        item_count
    }

    fn decrement_item_count(&mut self, item_kind: StorageItemKind, sub_amount: u32) -> u32 {
        let (item_index, mut item_count) = self.item_count(item_kind);

        if item_count != 0 {
            item_count = item_count.saturating_sub(sub_amount);

            // If we have a non-zero item count we must have an allocated item and its kind must match.
            if self.allocated_item_kind.unwrap() != item_kind {
                panic!("Storage slot can only accept {}!", item_kind);
            }

            self.set_item_count(item_index, item_count);

            if item_count == 0 {
                self.allocated_item_kind = None;
            }
        }

        item_count
    }

    fn for_each_item_kind<F>(&self, mut visitor_fn: F)
        where F: FnMut(StorageItemKind)
    {
        self.goods.for_each(|_, good| {
            visitor_fn(StorageItemKind::ConsumerGood(good.kind));
        });

        self.materials.for_each(|_, material| {
            visitor_fn(StorageItemKind::RawMaterial(material.kind));
        });
    }
}

impl StorageSlots {
    fn new(accepted_goods: &ConsumerGoodsList,
           accepted_raw_materials: &RawMaterialsList,
           num_slots: u32, slot_capacity: u32) -> Box<Self> {

        if (accepted_goods.is_empty() && accepted_raw_materials.is_empty()) ||
            num_slots == 0 || slot_capacity == 0 {
            panic!("Storage building must have a non-zero number of slots, slot capacity and a list of accepted goods/materials!");
        }

        let mut slots = ArrayVec::new();

        for _ in 0..num_slots {
            slots.push(StorageSlot {
                goods: ConsumerGoodsStock::with_accepted_items(accepted_goods),
                materials: RawMaterialsStock::with_accepted_items(accepted_raw_materials),
                allocated_item_kind: None,
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
    fn slot_item_count(&self, slot_index: usize, item_kind: StorageItemKind) -> u32 {
        self.slots[slot_index].item_count(item_kind).1
    }

    #[inline]
    fn increment_slot_item_count(&mut self, slot_index: usize, item_kind: StorageItemKind, add_amount: u32) -> u32 {
        self.slots[slot_index].increment_item_count(item_kind, add_amount, self.slot_capacity)
    }

    #[inline]
    fn decrement_slot_item_count(&mut self, slot_index: usize, item_kind: StorageItemKind, sub_amount: u32) -> u32 {
        self.slots[slot_index].decrement_item_count(item_kind, sub_amount)
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

    fn find_slot_for_material_kind(&self, kind: RawMaterialKind) -> Option<usize> {
        // Should be a single kind, never multiple ORed flags.
        debug_assert!(kind.bits().count_ones() == 1);

        // See if this item kind is already being stored somewhere:
        for (slot_index, slot) in self.slots.iter().enumerate() {
            if let Some(allocated_item_kind) = slot.allocated_item_kind {
                if let StorageItemKind::RawMaterial(item_kind) = allocated_item_kind {
                    if item_kind == kind && !self.is_slot_full(slot_index) {
                        return Some(slot_index);
                    }
                }
            }
        }

        // Not in storage yet or other slots are full, see if we can allocate a new slot for it:
        self.find_free_slot()
    }

    fn find_slot_for_good_kind(&self, kind: ConsumerGoodKind) -> Option<usize> {
        // Should be a single kind, never multiple ORed flags.
        debug_assert!(kind.bits().count_ones() == 1);

        // See if this item kind is already being stored somewhere:
        for (slot_index, slot) in self.slots.iter().enumerate() {
            if let Some(allocated_item_kind) = slot.allocated_item_kind {
                if let StorageItemKind::ConsumerGood(item_kind) = allocated_item_kind {
                    if item_kind == kind && !self.is_slot_full(slot_index) {
                        return Some(slot_index);
                    }
                }
            }
        }

        // Not in storage yet or other slots are full, see if we can allocate a new slot for it:
        self.find_free_slot()
    }

    fn try_add_materials(&mut self, materials: &mut StockItem<RawMaterialKind>) -> bool {
        let item_kind = materials.kind;
        let add_amount = materials.count;

        let slot_index = match self.find_slot_for_material_kind(item_kind) {
            Some(slot_index) => slot_index,
            None => return false
        };

        let prev_item_count =
            self.slot_item_count(slot_index, StorageItemKind::RawMaterial(item_kind));

        let new_item_count =
            self.increment_slot_item_count(slot_index, StorageItemKind::RawMaterial(item_kind), add_amount);

        let items_added = new_item_count - prev_item_count;
        materials.count -= items_added;

        items_added != 0
    }

    fn try_add_goods(&mut self, goods: &mut StockItem<ConsumerGoodKind>) -> bool {
        let item_kind = goods.kind;
        let add_amount = goods.count;

        let slot_index = match self.find_slot_for_good_kind(item_kind) {
            Some(slot_index) => slot_index,
            None => return false
        };

        let prev_item_count =
            self.slot_item_count(slot_index, StorageItemKind::ConsumerGood(item_kind));

        let new_item_count =
            self.increment_slot_item_count(slot_index, StorageItemKind::ConsumerGood(item_kind), add_amount);

        let items_added = new_item_count - prev_item_count;
        goods.count -= items_added;

        items_added != 0
    }
}

impl std::fmt::Display for StorageItemKind {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            StorageItemKind::RawMaterial(material) => {
                write!(f, "{}", material)
            },
            StorageItemKind::ConsumerGood(good) => {
                write!(f, "{}", good)
            }
        }
    }
}

// ----------------------------------------------
// Debug UI
// ----------------------------------------------

impl StorageConfig {
    fn draw_debug_ui(&self, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();
        ui.text(format!("Tile def name......: '{}'", self.tile_def_name));
        ui.text(format!("Min workers........: {}", self.min_workers));
        ui.text(format!("Max workers........: {}", self.max_workers));
        ui.text(format!("Accepted goods.....: {}", self.accepted_goods));
        ui.text(format!("Accepted materials.: {}", self.accepted_raw_materials));
        ui.text(format!("Num slots..........: {}", self.num_slots));
        ui.text(format!("Slot capacity......: {}", self.slot_capacity));
    }
}

impl<'config> StorageBuilding<'config> {
    fn draw_debug_ui_storage_config(&mut self, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();
        if ui.collapsing_header("Config##_building_config", imgui::TreeNodeFlags::empty()) {
            self.config.draw_debug_ui(ui_sys);
        }
    }
}

impl StorageSlots {
    fn draw_debug_ui(&mut self, label: &str, ui_sys: &UiSystem) {
        if self.slots.is_empty() {
            return;
        }

        let ui = ui_sys.builder();

        if ui.collapsing_header(label, imgui::TreeNodeFlags::empty()) {
            let mut display_slots: SmallVec<[SmallVec<[StorageItemKind; 8]>; MAX_STORAGE_SLOTS]> =
                smallvec![SmallVec::new(); MAX_STORAGE_SLOTS];

            for (slot_index, slot) in self.slots.iter().enumerate() {
                if let Some(allocated_item_kind) = slot.allocated_item_kind {
                    // Display only the allocated item kind.
                    display_slots[slot_index].push(allocated_item_kind);
                } else {
                    // No item allocated for the slot, display all possible item kinds accepted.
                    slot.for_each_item_kind(|item_kind| {
                        display_slots[slot_index].push(item_kind);
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
                    for (item_index, item_kind) in slot.iter().enumerate() {
                        let item_label =
                            format!("{}##_stock_item_{}_slot_{}", item_kind, item_index, slot_index);

                        let prev_item_count = self.slot_item_count(slot_index, *item_kind);
                        let mut new_item_count = prev_item_count;

                        if ui.input_scalar(item_label, &mut new_item_count).step(1).build() {
                            if new_item_count > prev_item_count {
                                new_item_count = self.increment_slot_item_count(
                                    slot_index, *item_kind, new_item_count - prev_item_count);
                            } else if new_item_count < prev_item_count {
                                new_item_count = self.decrement_slot_item_count(
                                    slot_index, *item_kind, prev_item_count - new_item_count);
                            }
                        }

                        let capacity_left = self.slot_capacity - new_item_count;
                        let is_full = new_item_count >= self.slot_capacity;

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
