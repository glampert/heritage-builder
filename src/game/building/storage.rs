use std::fmt::Display;
use arrayvec::ArrayVec;
use smallvec::{smallvec, SmallVec};

use crate::{
    imgui_ui::UiSystem,
    utils::{
        Color,
        hash::StringHash
    },
    game::sim::resources::{
        CONSUMER_GOOD_COUNT,
        ConsumerGoodKind,
        ConsumerGoodsList,
        RAW_MATERIAL_COUNT,
        RawMaterialKind,
        RawMaterialsList,
        Workers,
        List,
        Stock,
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
    pub goods_accepted: ConsumerGoodsList,
    pub raw_materials_accepted: RawMaterialsList,

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
    goods_stock: Option<Box<ConsumerGoodSlots>>,
    raw_materials_stock: Option<Box<RawMaterialSlots>>,
}

impl<'config> BuildingBehavior<'config> for StorageBuilding<'config> {
    fn update(&mut self, _update_ctx: &mut BuildingUpdateContext<'config, '_, '_, '_, '_>, _delta_time_secs: f32) {
        // Nothing for now.
    }

    fn draw_debug_ui(&mut self, ui_sys: &UiSystem) {
        self.draw_debug_ui_storage_config(ui_sys);

        if let Some(goods_stock) = &mut self.goods_stock {
            goods_stock.draw_debug_ui("Goods In Stock", ui_sys);
        }

        if let Some(raw_materials_stock) = &mut self.raw_materials_stock {
            raw_materials_stock.draw_debug_ui("Raw Materials In Stock", ui_sys);
        }
    }
}

impl<'config> StorageBuilding<'config> {
    pub fn new(kind: BuildingKind, configs: &'config BuildingConfigs) -> Self {
        let config = configs.find::<StorageConfig>(kind);
        Self {
            config: config,
            workers: Workers::new(config.min_workers, config.max_workers),
            goods_stock: StorageSlots::new(
                &config.goods_accepted,
                config.num_slots,
                config.slot_capacity
            ),
            raw_materials_stock: StorageSlots::new(
                &config.raw_materials_accepted,
                config.num_slots,
                config.slot_capacity
            ),
        }
    }
}

// ----------------------------------------------
// StorageSlots
// ----------------------------------------------

const MAX_STORAGE_SLOTS: usize = 8;

struct StorageSlot<T, const STOCK_CAPACITY: usize> {
    stock: Stock<T, STOCK_CAPACITY>,
    allocated_item_kind: Option<T>,
}

struct StorageSlots<T, const STOCK_CAPACITY: usize> {
    slots: ArrayVec<StorageSlot<T, STOCK_CAPACITY>, MAX_STORAGE_SLOTS>,
    slot_capacity: u32,
}

type ConsumerGoodSlots = StorageSlots<ConsumerGoodKind, CONSUMER_GOOD_COUNT>;
type RawMaterialSlots  = StorageSlots<RawMaterialKind,  RAW_MATERIAL_COUNT>;

impl<T, const STOCK_CAPACITY: usize> StorageSlots<T, STOCK_CAPACITY>
    where T: Copy + Display + bitflags::Flags + PartialEq, u32: From<<T as bitflags::Flags>::Bits>
{
    fn new(accepted_items: &List<T, STOCK_CAPACITY>, num_slots: u32, slot_capacity: u32) -> Option<Box<Self>> {
        if accepted_items.is_empty() || num_slots == 0 || slot_capacity == 0 {
            return None;
        }

        let mut slots = ArrayVec::new();
        for _ in 0..num_slots {
            slots.push(StorageSlot {
                stock: Stock::with_accepted_items(accepted_items),
                allocated_item_kind: None,
            });
        }

        Some(Box::new(Self { slots: slots, slot_capacity: slot_capacity }))
    }

    fn is_slot_free(&self, slot_index: usize) -> bool {
        let slot = &self.slots[slot_index];
        slot.allocated_item_kind.is_none()
    }

    fn is_slot_full(&self, slot_index: usize) -> bool {
        let slot = &self.slots[slot_index];
        if let Some(allocated_item_kind) = slot.allocated_item_kind {
            let count = slot.stock.count(allocated_item_kind);
            if count >= self.slot_capacity {
                return true;
            }
        }
        false
    }

    fn increment_item_count(&mut self, slot_index: usize, item_kind: T, add_amount: u32) -> u32 {
        let slot = &mut self.slots[slot_index];

        let (item_index, mut item) = slot.stock.find(item_kind)
            .expect(&format!("Item {} expected to exist in the stock!", item_kind));
        debug_assert!(item.kind == item_kind);

        let prev_count = item.count;
        item.count = (prev_count + add_amount).min(self.slot_capacity);

        if let Some(allocated_item_kind) = slot.allocated_item_kind {
            if allocated_item_kind != item_kind {
                panic!("Slot {} can only accept {}!", slot_index, item_kind);
            }
        } else {
            debug_assert!(prev_count == 0);
            slot.allocated_item_kind = Some(item_kind);
        }

        slot.stock.set(item_index, item);
        item.count
    }

    fn decrement_item_count(&mut self, slot_index: usize, item_kind: T, sub_amount: u32) -> u32 {
        let slot = &mut self.slots[slot_index];

        let (item_index, mut item) = slot.stock.find(item_kind)
            .expect(&format!("Item {} expected to exist in the stock!", item_kind));
        debug_assert!(item.kind == item_kind);

        if item.count != 0 {
            item.count = item.count.saturating_sub(sub_amount);

            // If we have a non-zero item count we must have an allocated item and its kind must match.
            if slot.allocated_item_kind.unwrap() != item_kind {
                panic!("Slot {} can only accept {}!", slot_index, item_kind);
            }

            if item.count == 0 {
                slot.allocated_item_kind = None;
            }
        }

        slot.stock.set(item_index, item);
        item.count
    }
}

// ----------------------------------------------
// Debug UI
// ----------------------------------------------

impl StorageConfig {
    fn draw_debug_ui(&self, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();
        ui.text(format!("Tile def name.....: '{}'", self.tile_def_name));
        ui.text(format!("Min workers.......: {}", self.min_workers));
        ui.text(format!("Max workers.......: {}", self.max_workers));
        ui.text(format!("Goods accepted....: {}", self.goods_accepted));
        ui.text(format!("Material accepted.: {}", self.raw_materials_accepted));
        ui.text(format!("Num slots.........: {}", self.num_slots));
        ui.text(format!("Slot capacity.....: {}", self.slot_capacity));
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

impl<T, const STOCK_CAPACITY: usize> StorageSlots<T, STOCK_CAPACITY>
    where T: Copy + Display + bitflags::Flags + PartialEq, u32: From<<T as bitflags::Flags>::Bits>
{
    fn draw_debug_ui(&mut self, label: &str, ui_sys: &UiSystem) {
        if self.slots.is_empty() {
            return;
        }

        let ui = ui_sys.builder();

        if ui.collapsing_header(label, imgui::TreeNodeFlags::empty()) {
            let mut display_slots: SmallVec<[SmallVec<[T; 8]>; MAX_STORAGE_SLOTS]> =
                smallvec![SmallVec::new(); MAX_STORAGE_SLOTS];

            for (slot_index, slot) in self.slots.iter().enumerate() {
                if let Some(allocated_item_kind) = slot.allocated_item_kind {
                    // Display only the allocated item kind.
                    display_slots[slot_index].push(allocated_item_kind);
                } else {
                    // No item allocated for the slot, display all possible item kinds accepted.
                    slot.stock.for_each(|_, item| {
                        display_slots[slot_index].push(item.kind);
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

                if ui.collapsing_header(format!("{}##_stock_slot_{}", slot_label, slot_index), imgui::TreeNodeFlags::DEFAULT_OPEN) {
                    for (item_index, item_kind) in slot.iter().enumerate() {
                        let item_label =
                            format!("{}##_stock_item_{}_slot_{}", item_kind, item_index, slot_index);

                        let (_, item) = self.slots[slot_index].stock.find(*item_kind)
                            .expect(&format!("Item {} expected to exist in the stock!", item_kind));

                        let prev_item_count = item.count;
                        let mut new_item_count = prev_item_count;

                        if ui.input_scalar(item_label, &mut new_item_count).step(1).build() {
                            if new_item_count > prev_item_count {
                                new_item_count = self.increment_item_count(
                                    slot_index, *item_kind, new_item_count - prev_item_count);
                            } else if new_item_count < prev_item_count {
                                new_item_count = self.decrement_item_count(
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
