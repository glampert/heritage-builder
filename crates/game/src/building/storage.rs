use std::cmp::Reverse;

use arrayvec::ArrayVec;
use common::{
    callback::{self, Callback},
    coords::Cell,
    hash::{self, StringHash},
    time::UpdateTimer,
};
use engine::ui::UiSystem;
use proc_macros::DrawDebugUi;
use serde::{Deserialize, Serialize};

use super::{
    Building,
    BuildingBehavior,
    BuildingContext,
    BuildingContextMut,
    BuildingKind,
    BuildingKindAndId,
    BuildingVisitResult,
    config::{BuildingConfig, BuildingConfigs, building_config},
    risks::RiskRates,
};
use crate::{
    cheats,
    config::GameConfigs,
    debug::game_object_debug::{GameObjectDebugOptions, debug_popup_msg, game_object_debug_options},
    inspect::StorageOrder,
    pathfind::NodeKind as PathNodeKind,
    save_context::PostLoadContext,
    sim::{
        SimCmds,
        SimContext,
        SimContextMut,
        resources::{
            RESOURCE_KIND_COUNT,
            ResourceKind,
            ResourceKinds,
            ResourceStock,
            ShoppingList,
            StockItem,
            Workers,
            per_kind_map,
        },
    },
    tile::Tile,
    undo_redo::{GameObjectSavedState, game_object_undo_redo_state},
    unit::{
        Unit,
        UnitTaskHelper,
        runner::Runner,
        task::{
            UnitTaskDeliverToStorage,
            UnitTaskDeliveryCompletionCallback,
            UnitTaskFetchCompletionCallback,
            UnitTaskFetchFromStorage,
            UnitTaskTrade,
        },
    },
    world::{object::GameObject, stats::WorldStats},
};

// ----------------------------------------------
// StorageConfig
// ----------------------------------------------

#[derive(Clone, DrawDebugUi, Serialize, Deserialize)]
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

    // What its runner carries a trip (Get and Empty).
    #[serde(default = "default_runner_load")]
    pub runner_load: u32,

    // How fast its fire and collapse risks build up, per calendar month (building::risks).
    #[serde(default)]
    #[debug_ui(nested)]
    pub risks: RiskRates,
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
            slot_capacity: 8,
            runner_load: default_runner_load(),
            risks: RiskRates::ZERO,
        }
    }
}

const fn default_runner_load() -> u32 {
    8
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
// StorageUndoRedoSavedState
// ----------------------------------------------

game_object_undo_redo_state! {
    StorageUndoRedoSavedState,

    storage_slots: StorageSlots,
}

// ----------------------------------------------
// StorageBuilding
// ----------------------------------------------

#[derive(Clone, Serialize, Deserialize)]
pub struct StorageBuilding {
    #[serde(skip)]
    config: Option<&'static StorageConfig>,

    workers: Workers,

    // Stockpiles:
    pub(crate) storage_slots: Box<StorageSlots>,

    // Its runner, sent on the Get and Empty orders' errands one at a time, and the clock it looks
    // for one by (SimConfigs::storage_errand_secs).
    pub(crate) runner: Runner,
    errand_timer: UpdateTimer,

    // The goods it's set to Empty that no store in reach takes, as of its last look (its panel
    // says so).
    #[serde(skip)]
    pub(crate) cannot_empty: ResourceKind,

    #[serde(skip)]
    debug: StorageDebug,
}

// ----------------------------------------------
// BuildingBehavior for StorageBuilding
// ----------------------------------------------

impl BuildingBehavior for StorageBuilding {
    // ----------------------
    // World Callbacks:
    // ----------------------

    fn name(&self) -> &'static str {
        &self.config.unwrap().name
    }

    fn configs(&self) -> &dyn BuildingConfig {
        self.config.unwrap()
    }

    fn despawned(&mut self, context: &BuildingContextMut) {
        self.runner.discard_spawn_promise(context.sim_ctx.cmds_mut());
    }

    fn update(&mut self, cmds: &mut SimCmds, context: &BuildingContext) {
        debug_assert!(self.config.is_some());

        self.runner.update(cmds);

        if self.errand_timer.tick(context.sim_ctx.delta_time_secs()).should_update() {
            self.send_on_errand(cmds, context);
        }
    }

    fn visited_by(&mut self, unit: &mut Unit, context: &BuildingContextMut) -> BuildingVisitResult {
        if !self.has_min_required_workers() {
            return BuildingVisitResult::Refused;
        }

        let task_manager = context.sim_ctx.task_manager();

        if let Some(task) = unit.current_task_as::<UnitTaskDeliverToStorage>(task_manager) {
            debug_assert!(context.kind.intersects(task.storage_buildings_accepted));

            // Try unload cargo:
            if let Some(item) = unit.peek_inventory() {
                let received_count = self.receive_resources(item.kind, item.count);
                if received_count != 0 {
                    let removed_count = unit.remove_resources(item.kind, received_count);
                    debug_assert!(removed_count == received_count);

                    debug_popup_msg!(self.debug, "{} delivered {} {}", unit.name(), received_count, item.kind);
                    return BuildingVisitResult::Accepted;
                }
            }
        } else if let Some(task) = unit.current_task_as::<UnitTaskFetchFromStorage>(task_manager) {
            debug_assert!(context.kind.intersects(task.storage_buildings_accepted));

            // Recovery path: the fetch task could not return to its origin (origin
            // unreachable, destroyed, or unable to accept the delivery), so the unit
            // is depositing its cargo at any storage that will take it.
            if let Some(item) = unit.peek_inventory() {
                let received_count = self.receive_resources(item.kind, item.count);
                if received_count != 0 {
                    let removed_count = unit.remove_resources(item.kind, received_count);
                    debug_assert!(removed_count == received_count);

                    debug_popup_msg!(self.debug, "{} deposited surplus {} {}", unit.name(), received_count, item.kind);
                    return BuildingVisitResult::Accepted;
                }
                return BuildingVisitResult::Refused;
            }

            // Normal path: try give resources from this storage to the unit.
            for item in task.resources_to_fetch.iter() {
                let available_count = self.available_resources(item.kind);
                if available_count != 0 {
                    let max_fetch_count = available_count.min(item.count);
                    let removed_count = self.remove_resources(item.kind, max_fetch_count);

                    unit.receive_resources(item.kind, removed_count);
                    debug_assert!(removed_count == max_fetch_count);

                    debug_popup_msg!(self.debug, "{} fetched {} {}", unit.name(), max_fetch_count, item.kind);
                    return BuildingVisitResult::Accepted;
                }
            }
        } else if unit.current_task_as::<UnitTaskTrade>(task_manager).is_some() {
            // A caravan: it trades in its visit's callback (trade::trade_at_store), with a staffed
            // store only.
            return BuildingVisitResult::Accepted;
        } else {
            panic!("Unhandled Unit Task in StorageBuilding::visited_by()!");
        }

        BuildingVisitResult::Refused
    }

    fn pre_save(&mut self, cmds: &mut SimCmds) {
        self.runner.pre_save(cmds);
    }

    fn post_save(&mut self) {
        self.runner.post_save();
    }

    fn post_load(&mut self, context: &mut PostLoadContext, kind: BuildingKind, _tile: &Tile) {
        debug_assert!(kind.intersects(BuildingKind::storage()));
        let config = BuildingConfigs::get().find_storage_config(kind);
        self.storage_slots.refresh_from_config(&config.resources_accepted, config.num_slots, config.slot_capacity);
        self.runner.post_load();
        self.errand_timer.post_load(context.configs().sim.storage_errand_secs);
        self.config = Some(config);
    }

    // ----------------------
    // Resources/Stock:
    // ----------------------

    fn has_stock(&self) -> bool {
        true
    }

    fn is_stock_full(&self) -> bool {
        self.storage_slots.are_all_slots_full()
    }

    // One item per kind held, its units summed over the slots holding it.
    fn stock(&self) -> ArrayVec<StockItem, RESOURCE_KIND_COUNT> {
        let mut items: ArrayVec<StockItem, RESOURCE_KIND_COUNT> = ArrayVec::new();
        self.storage_slots.for_each_resource(|item| match items.iter_mut().find(|held| held.kind == item.kind) {
            Some(held) => held.count += item.count,
            None => items.push(*item),
        });
        items
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

    // Returns number of resources it was able to accommodate, which can be less
    // than `count`.
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

    fn workers(&self) -> Option<&Workers> {
        Some(&self.workers)
    }

    fn workers_mut(&mut self) -> Option<&mut Workers> {
        Some(&mut self.workers)
    }

    fn active_runner(&mut self) -> Option<&mut Runner> {
        Some(&mut self.runner)
    }

    #[inline]
    fn has_min_required_workers(&self) -> bool {
        if cheats::get().ignore_worker_requirements {
            return true;
        }
        self.workers.as_employer().unwrap().has_min_required()
    }

    // ----------------------
    // Undo/Redo:
    // ----------------------

    fn undo_redo_record(&self) -> Option<Box<dyn GameObjectSavedState>> {
        StorageUndoRedoSavedState::new_state(StorageUndoRedoSavedState {
            storage_slots: self.storage_slots.as_ref().clone(),
        })
    }

    fn undo_redo_apply(&mut self, state: &dyn GameObjectSavedState) {
        let saved_state = StorageUndoRedoSavedState::downcast(state);

        // NOTE: Only stock is preserved on undo/redo. Runners and workers are reset.
        *self.storage_slots = saved_state.storage_slots.clone();
    }

    // ----------------------
    // Debug:
    // ----------------------

    fn debug_options(&mut self) -> &mut dyn GameObjectDebugOptions {
        &mut self.debug
    }

    fn draw_debug_ui(&mut self, cmds: &mut SimCmds, context: &BuildingContextMut, ui_sys: &UiSystem) {
        // Debug-UI drawing lives in `crate::debug::building`.
        self.draw_debug_ui_dispatch(cmds, context, ui_sys);
    }
}

// ----------------------------------------------
// StorageBuilding
// ----------------------------------------------

impl StorageBuilding {
    pub fn new(config: &'static StorageConfig) -> Self {
        Self {
            config: Some(config),
            workers: Workers::employer(config.min_workers, config.max_workers),
            storage_slots: StorageSlots::new(&config.resources_accepted, config.num_slots, config.slot_capacity),
            runner: Runner::default(),
            errand_timer: UpdateTimer::new(GameConfigs::get().sim.storage_errand_secs),
            cannot_empty: ResourceKind::empty(),
            debug: StorageDebug::default(),
        }
    }

    pub fn register_callbacks() {
        let _: Callback<UnitTaskDeliveryCompletionCallback> = callback::register!(StorageBuilding::on_goods_carried_out);
        let _: Callback<UnitTaskFetchCompletionCallback> = callback::register!(StorageBuilding::on_goods_fetched);
    }

    // The player's order for a good.
    #[inline]
    pub fn order(&self, kind: ResourceKind) -> StorageOrder {
        self.storage_slots.order(kind)
    }

    // Sets the order for each good of `kinds` (the inspector's rows, storage.orders).
    #[inline]
    pub fn set_orders(&mut self, kinds: ResourceKind, order: StorageOrder) {
        self.storage_slots.set_orders(kinds, order);
    }

    // The goods it's set to Empty that no store in reach takes, as of its last look.
    #[inline]
    pub fn cannot_empty(&self) -> ResourceKind {
        self.cannot_empty
    }

    #[inline]
    pub fn is_runner_out(&self) -> bool {
        self.runner.is_spawned_or_pending_spawn()
    }

    // Its runner's errand while it's out: the order it runs for (Get or Empty), the good and how
    // many it carries, or is sent for.
    pub fn errand(&self, context: &SimContext) -> Option<(StorageOrder, ResourceKind, u32)> {
        let unit = self.runner.try_unit(context)?;
        let task_manager = context.task_manager();
        let carried = unit.peek_inventory().filter(|item| item.count != 0);
        if let Some(task) = unit.current_task_as::<UnitTaskDeliverToStorage>(task_manager) {
            let count = carried.map_or(task.resource_count, |item| item.count);
            return Some((StorageOrder::Empty, task.resource_kind_to_deliver, count));
        }
        if let Some(task) = unit.current_task_as::<UnitTaskFetchFromStorage>(task_manager) {
            let wanted = task.resources_to_fetch.first()?;
            let count = carried.map_or(wanted.count, |item| item.count);
            return Some((StorageOrder::Get, wanted.kind, count));
        }
        None
    }

    // The orders' errands (docs/plans/messages.md, Phase 2), one at a time while it's staffed:
    // first a good set to Empty that it holds, carried to the nearest other store in reach that
    // takes it; else a good set to Get that it has room for, fetched from the nearest other store
    // in reach that holds it and doesn't get it too. A load at most a trip.
    fn send_on_errand(&mut self, cmds: &mut SimCmds, context: &BuildingContext) {
        self.cannot_empty = ResourceKind::empty();
        if !self.has_min_required_workers() || self.is_runner_out() {
            return;
        }
        let Some(road_link) = context.road_link else {
            return; // Its runner walks the roads.
        };
        let config = self.config.unwrap();
        let load = config.runner_load.max(1);
        let this = context.kind_and_id();

        // Empty: the goods it holds, in the goods' order.
        let mut to_empty = None;
        for kind in config.resources_accepted.combined().iter() {
            let held = self.storage_slots.available_resources(kind);
            if held == 0 || self.storage_slots.order(kind) != StorageOrder::Empty {
                continue;
            }
            if store_in_reach(context, road_link, this, |store| store.receivable_resources(kind) != 0) {
                to_empty = Some((kind, held.min(load)));
                break;
            }
            self.cannot_empty |= kind;
        }
        if let Some((kind, count)) = to_empty {
            self.runner.try_carry_to_storage(
                cmds,
                context,
                road_link,
                BuildingKind::storage(),
                kind,
                count,
                callback::create!(StorageBuilding::on_goods_carried_out),
            );
            // Handed to the runner, as a producer's output is.
            let removed = self.storage_slots.remove_resources(kind, count);
            debug_assert_eq!(removed, count);
            debug_popup_msg!(self.debug, "Carrying out {count} {kind}");
            return;
        }

        // Get: the goods it has room for.
        for kind in config.resources_accepted.combined().iter() {
            if self.storage_slots.order(kind) != StorageOrder::Get {
                continue;
            }
            let room = self.storage_slots.receivable_resources(kind);
            if room == 0 {
                continue;
            }
            let found = store_in_reach(context, road_link, this, |store| {
                store.as_storage().order(kind) != StorageOrder::Get && store.available_resources(kind) != 0
            });
            if found {
                let count = room.min(load);
                self.runner.try_fetch_from_storage(
                    cmds,
                    context,
                    road_link,
                    BuildingKind::storage(),
                    ShoppingList::from_items(&[StockItem { kind, count }]),
                    callback::create!(StorageBuilding::on_goods_fetched),
                );
                debug_popup_msg!(self.debug, "Fetching {count} {kind}");
                return;
            }
        }
    }

    fn on_goods_carried_out(_context: &SimContextMut, this_building: &mut Building, runner_unit: &mut Unit) {
        let this_storage = this_building.as_storage_mut();
        debug_assert!(runner_unit.inventory_is_empty(), "Runner Unit should have delivered all resources by now!");
        debug_assert!(this_storage.runner.unit_id() == runner_unit.id());
        this_storage.runner.reset();
    }

    fn on_goods_fetched(_context: &SimContextMut, this_building: &mut Building, runner_unit: &mut Unit) {
        let this_storage = this_building.as_storage_mut();
        debug_assert!(this_storage.runner.unit_id() == runner_unit.id());

        // Unload what it brought. What doesn't fit (the order changed, the slots filled) stays with
        // the runner, whose task takes it to a store that will.
        if let Some(item) = runner_unit.peek_inventory() {
            let received = this_storage.receive_resources(item.kind, item.count);
            if received != 0 {
                let removed = runner_unit.remove_resources(item.kind, received);
                debug_assert!(removed == received);
            }
        }
        this_storage.runner.reset();
    }
}

// Whether another staffed store, linked to a road, lies within the stores' reach along the roads
// from `road_link` and passes `accepts`.
fn store_in_reach(
    context: &BuildingContext,
    road_link: Cell,
    this: BuildingKindAndId,
    accepts: impl Fn(&Building) -> bool,
) -> bool {
    let reach = GameConfigs::get().sim.storage_reach;
    context
        .sim_ctx
        .find_nearest_buildings(road_link, BuildingKind::storage(), PathNodeKind::Road, Some(reach), |store, _path| {
            // Accept and stop at the first that will do.
            !(store.kind_and_id() != this && store.is_linked_to_road() && accepts(store))
        })
        .is_some()
}

// ----------------------------------------------
// StorageSlots
// ----------------------------------------------

pub(crate) const MAX_STORAGE_SLOTS: usize = 8;

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct StorageSlot {
    stock: ResourceStock,
    allocated_resource_kind: Option<ResourceKind>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct StorageSlots {
    pub(crate) slots: ArrayVec<StorageSlot, MAX_STORAGE_SLOTS>,
    slot_capacity: u32,
    // The player's order for each good (the inspector's rows): it takes in a good only while
    // accepting or getting it. Saved by good name, accepted goods left out ({ "Rice": "refuse" }).
    #[serde(with = "per_kind_map")]
    orders: [StorageOrder; RESOURCE_KIND_COUNT],
}

impl StorageSlot {
    #[inline]
    fn is_free(&self) -> bool {
        self.allocated_resource_kind.is_none()
    }

    #[inline]
    pub(crate) fn allocated_resource_kind(&self) -> Option<ResourceKind> {
        self.allocated_resource_kind
    }

    #[inline]
    pub(crate) fn accepted_kinds(&self) -> ResourceKind {
        self.stock.accepted_kinds()
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

    pub(crate) fn clear(&mut self) {
        self.stock.clear();
        self.allocated_resource_kind = None;
    }

    fn resource_index_and_count(&self, kind: ResourceKind) -> (usize, u32) {
        debug_assert!(kind.is_single_resource());
        let (index, item) =
            self.stock.find(kind).unwrap_or_else(|| panic!("Resource kind '{}' expected to exist in the stock!", kind));
        (index, item.count)
    }

    fn set_resource_count_internal(&mut self, index: usize, count: u32) {
        let kind = self.allocated_resource_kind.unwrap();
        self.stock.set(index, StockItem { kind, count });
    }

    pub(crate) fn increment_resource_count(&mut self, kind: ResourceKind, add_amount: u32, slot_capacity: u32) -> u32 {
        let (index, mut count) = self.resource_index_and_count(kind);

        let prev_count = count;
        count = prev_count.saturating_add(add_amount).min(slot_capacity);

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

            // If we have a non-zero item count we must have an allocated item and its kind
            // must match.
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

    pub(crate) fn for_each_accepted_resource<F>(&self, mut visitor_fn: F)
    where
        F: FnMut(ResourceKind),
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

        Box::new(Self { slots, slot_capacity, orders: [StorageOrder::Accept; RESOURCE_KIND_COUNT] })
    }

    // After a load: the goods, slot count and slot capacity the config asks for now, so config
    // changes reach saved buildings. A slot keeps the good it holds, clamped to the capacity;
    // slots are added, never removed.
    fn refresh_from_config(&mut self, resources_accepted: &ResourceKinds, num_slots: u32, slot_capacity: u32) {
        let accepted_kinds = resources_accepted.combined();
        self.slot_capacity = slot_capacity;

        for slot in &mut self.slots {
            slot.stock.set_accepted_kinds_keeping_held(accepted_kinds);
            slot.stock.for_each_mut(|_, item| item.count = item.count.min(slot_capacity));
            // A slot allocated to a good it no longer holds or accepts is free again.
            if let Some(kind) = slot.allocated_resource_kind
                && slot.stock.count(kind) == 0
                && !accepted_kinds.intersects(kind)
            {
                slot.allocated_resource_kind = None;
            }
        }

        while self.slots.len() < (num_slots as usize).min(MAX_STORAGE_SLOTS) {
            self.slots.push(StorageSlot {
                stock: ResourceStock::with_accepted_list(resources_accepted),
                allocated_resource_kind: None,
            });
        }
    }

    // The store's order for a good (the inspector's rows).
    pub(crate) fn order(&self, kind: ResourceKind) -> StorageOrder {
        debug_assert!(kind.is_single_resource());
        self.orders[kind.index()]
    }

    // Sets the order for each good of `kinds`. Goods already held stay until they're taken (Empty's
    // runner carries them out).
    pub(crate) fn set_orders(&mut self, kinds: ResourceKind, order: StorageOrder) {
        for kind in kinds.iter() {
            self.orders[kind.index()] = order;
        }
    }

    #[inline]
    pub(crate) fn slot_capacity(&self) -> u32 {
        self.slot_capacity
    }

    // The room the slots have for each good (the City book's goods): its own slots' capacity, and
    // every free slot's that accepts it (shared, so the goods' rooms add up to more than the whole);
    // and their whole capacity.
    pub(crate) fn room(&self) -> ([u32; RESOURCE_KIND_COUNT], u32) {
        let mut room = [0; RESOURCE_KIND_COUNT];
        for slot in &self.slots {
            match slot.allocated_resource_kind {
                Some(kind) => room[kind.index()] += self.slot_capacity,
                None => {
                    for kind in slot.accepted_kinds().iter() {
                        room[kind.index()] += self.slot_capacity;
                    }
                }
            }
        }
        (room, self.slot_capacity * self.slots.len() as u32)
    }

    #[inline]
    pub(crate) fn is_slot_free(&self, slot_index: usize) -> bool {
        self.slots[slot_index].is_free()
    }

    #[inline]
    fn is_slot_full(&self, slot_index: usize) -> bool {
        self.slots[slot_index].is_full(self.slot_capacity)
    }

    #[inline]
    pub(crate) fn slot_resource_count(&self, slot_index: usize, kind: ResourceKind) -> u32 {
        debug_assert!(kind.is_single_resource());
        self.slots[slot_index].resource_index_and_count(kind).1
    }

    #[inline]
    pub(crate) fn increment_slot_resource_count(&mut self, slot_index: usize, kind: ResourceKind, add_amount: u32) -> u32 {
        debug_assert!(kind.is_single_resource());
        self.slots[slot_index].increment_resource_count(kind, add_amount, self.slot_capacity)
    }

    #[inline]
    pub(crate) fn decrement_slot_resource_count(&mut self, slot_index: usize, kind: ResourceKind, sub_amount: u32) -> u32 {
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
            if let Some(allocated_kind) = slot.allocated_resource_kind
                && allocated_kind == kind
                && !self.is_slot_full(slot_index)
            {
                return Some(slot_index);
            }
        }

        // Not in storage yet or other slots are full, see if we can allocate a new slot
        // for it:
        self.find_free_slot()
    }

    // The slot holding the fewest units of `kind`, the last of them on a tie. Taking from it
    // first empties slots soonest, so a kind keeps to as few slots as it can and leaves the
    // others free for other goods.
    fn find_emptiest_resource_slot(&self, kind: ResourceKind) -> Option<usize> {
        // Should be a single kind, never multiple ORed flags.
        debug_assert!(kind.is_single_resource());

        self.slots
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot.allocated_resource_kind == Some(kind) && slot.stock.count(kind) != 0)
            .min_by_key(|(slot_index, slot)| (slot.stock.count(kind), Reverse(*slot_index)))
            .map(|(slot_index, _)| slot_index)
    }

    // Every slot accepts the same kinds; the player's order may refuse one (Refuse, Empty).
    fn accepts(&self, kind: ResourceKind) -> bool {
        self.slots.first().is_some_and(|slot| slot.accepted_kinds().intersects(kind)) && self.orders[kind.index()].takes_in()
    }

    // ----------------------
    // Stock across all slots:
    // ----------------------

    // Units of `kind` in all slots.
    pub(crate) fn available_resources(&self, kind: ResourceKind) -> u32 {
        // Should be a single kind, never multiple ORed flags.
        debug_assert!(kind.is_single_resource());

        self.slots.iter().filter(|slot| slot.allocated_resource_kind == Some(kind)).map(|slot| slot.stock.count(kind)).sum()
    }

    // Room for `kind`: what's left in the slots holding it, plus every free slot. None for a kind
    // the storage doesn't accept, whatever its free slots.
    pub(crate) fn receivable_resources(&self, kind: ResourceKind) -> u32 {
        // Should be a single kind, never multiple ORed flags.
        debug_assert!(kind.is_single_resource());

        if !self.accepts(kind) {
            return 0;
        }

        let mut count = 0;
        for slot in &self.slots {
            if slot.is_free() {
                count += self.slot_capacity;
            } else if slot.allocated_resource_kind == Some(kind) {
                count += slot.remaining_capacity(self.slot_capacity);
            }
        }
        count
    }

    // Adds up to `count` units of `kind`, topping up the slots that hold it before taking free
    // ones. Returns the count added: less than `count` when the slots fill up, and 0 for a kind
    // the storage doesn't accept.
    pub(crate) fn receive_resources(&mut self, kind: ResourceKind, count: u32) -> u32 {
        // Should be a single kind, never multiple ORed flags.
        debug_assert!(kind.is_single_resource());

        if !self.accepts(kind) {
            return 0;
        }

        let mut received = 0;
        while received < count {
            let Some(slot_index) = self.alloc_resource_slot(kind) else {
                break; // No room left.
            };

            let prev_count = self.slot_resource_count(slot_index, kind);
            let new_count = self.increment_slot_resource_count(slot_index, kind, count - received);
            if new_count == prev_count {
                break;
            }
            received += new_count - prev_count;
        }
        received
    }

    // Removes up to `count` units of `kind` from as many slots as it takes, emptiest first.
    // Returns the count removed.
    pub(crate) fn remove_resources(&mut self, kind: ResourceKind, count: u32) -> u32 {
        // Should be a single kind, never multiple ORed flags.
        debug_assert!(kind.is_single_resource());

        let mut removed = 0;
        while removed < count {
            let Some(slot_index) = self.find_emptiest_resource_slot(kind) else {
                break; // None left.
            };

            let prev_count = self.slot_resource_count(slot_index, kind);
            let new_count = self.decrement_slot_resource_count(slot_index, kind, count - removed);
            if new_count == prev_count {
                break;
            }
            removed += prev_count - new_count;
        }
        removed
    }

    fn for_each_resource<F>(&self, mut visitor_fn: F)
    where
        F: FnMut(&StockItem),
    {
        for slot in &self.slots {
            if let Some(allocated_kind) = slot.allocated_resource_kind {
                let (_, item) = slot.stock.find(allocated_kind).unwrap();
                visitor_fn(&item);
            }
        }
    }
}
