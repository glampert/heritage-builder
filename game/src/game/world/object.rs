#![allow(clippy::while_let_on_iterator)]

use core::iter;
use core::slice;
use bitvec::vec::BitVec;

use serde::{
    Serialize,
    Serializer,
    ser::SerializeSeq,
    Deserialize,
    Deserializer,
    de::{SeqAccess, Visitor}
};

use crate::{
    log,
    imgui_ui::UiSystem,
    save::PostLoadContext,
    tile::{Tile, TileKind, TileMapLayerKind, sets::TileDef},
    utils::coords::{Cell, CellRange, WorldToScreenTransform},
    game::{
        constants::*,
        building::Building,
        unit::{Unit, config::UnitConfigKey},
        sim::{Query, debug::DebugUiMode}
    }
};

use super::{
    stats::WorldStats,
};

// ----------------------------------------------
// GenerationalIndex
// ----------------------------------------------

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GenerationalIndex {
    generation: u32,
    index: u32, // Index into spawn pool; u32::MAX = invalid.
}

impl GenerationalIndex {
    #[inline]
    pub fn new(generation: u32, index: usize) -> Self {
        // Reserved value for invalid.
        debug_assert!(generation < u32::MAX);
        debug_assert!(index < u32::MAX as usize);
        Self {
            generation,
            index: index.try_into().expect("Index cannot fit into u32!"),
        }
    }

    #[inline]
    pub const fn invalid() -> Self {
        Self {
            generation: u32::MAX,
            index: u32::MAX,
        }
    }

    #[inline]
    pub fn is_valid(self) -> bool {
        self.generation < u32::MAX && self.index < u32::MAX
    }

    #[inline]
    pub fn generation(self) -> u32 {
        self.generation
    }

    #[inline]
    pub fn index(self) -> usize {
        self.index as usize
    }
}

impl Default for GenerationalIndex {
    #[inline]
    fn default() -> Self {
        Self::invalid()
    }
}

impl std::fmt::Display for GenerationalIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if self.is_valid() {
            write!(f, "[{},{}]", self.generation, self.index)
        } else {
            write!(f, "[invalid]")
        }
    }
}

// ----------------------------------------------
// GameObject
// ----------------------------------------------

pub trait GameObject<'config> {
    fn id(&self) -> GenerationalIndex;

    #[inline]
    fn is_spawned(&self) -> bool {
        self.id().is_valid()
    }

    fn update(&mut self, query: &Query<'config, '_>);
    fn tally(&self, stats: &mut WorldStats);
    fn post_load(&mut self, context: &PostLoadContext<'_, 'config>);

    fn draw_debug_ui(&mut self,
                     query: &Query<'config, '_>,
                     ui_sys: &UiSystem,
                     mode: DebugUiMode);

    fn draw_debug_popups(&mut self,
                         query: &Query,
                         ui_sys: &UiSystem,
                         transform: &WorldToScreenTransform,
                         visible_range: CellRange);
}

// ----------------------------------------------
// SpawnPool
// ----------------------------------------------

pub struct SpawnPool<T> {
    instances: Vec<T>,
    spawned: BitVec,
    generation: u32,
}

pub struct SpawnPoolIter<'a, T> {
    instances: iter::Enumerate<slice::Iter<'a, T>>,
    spawned: &'a BitVec,
}

pub struct SpawnPoolIterMut<'a, T> {
    instances: iter::Enumerate<slice::IterMut<'a, T>>,
    spawned: &'a BitVec,
}

impl<'a, T> Iterator for SpawnPoolIter<'a, T> {
    type Item = &'a T;
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        // Yields only *spawned* instances.
        while let Some((index, instance)) = self.instances.next() {
            if self.spawned[index] {
                return Some(instance);
            }
        }
        None
    }
}

impl<T> iter::FusedIterator for SpawnPoolIter<'_, T> {}

impl<'a, T> Iterator for SpawnPoolIterMut<'a, T> {
    type Item = &'a mut T;
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        // Yields only *spawned* instances.
        while let Some((index, instance)) = self.instances.next() {
            if self.spawned[index] {
                return Some(instance);
            }
        }
        None
    }
}

impl<T> iter::FusedIterator for SpawnPoolIterMut<'_, T> {}

impl<'config, T> SpawnPool<T>
    where T: GameObject<'config> + Clone + Default
{
    pub fn new(capacity: usize, generation: u32) -> Self {
        let default_instance = T::default();
        Self {
            instances: vec![default_instance; capacity],
            spawned: BitVec::repeat(false, capacity),
            generation,
        }
    }

    pub fn clear<F>(&mut self, query: &Query, on_despawned_fn: F)
        where F: Fn(&mut T, &Query)
    {
        debug_assert!(self.is_valid());

        for instance in self.iter_mut() {
            on_despawned_fn(instance, query);
        }

        self.instances.fill(T::default());
        self.spawned.fill(false);
    }

    pub fn spawn<F>(&mut self, query: &Query, on_spawned_fn: F) -> &mut T
        where F: FnOnce(&mut T, &Query, GenerationalIndex)
    {
        debug_assert!(self.is_valid());

        let generation = self.generation;
        self.generation += 1;

        // Try find a free slot to reuse:
        if let Some(recycled_index) = self.spawned.first_zero() {
            let recycled_instance = &mut self.instances[recycled_index];

            debug_assert!(!recycled_instance.is_spawned());
            on_spawned_fn(recycled_instance, query, GenerationalIndex::new(generation, recycled_index));

            self.spawned.set(recycled_index, true);

            return recycled_instance;
        }

        // Need to instantiate a new one.
        let new_index = self.instances.len();
        let mut new_instance = T::default();

        debug_assert!(!new_instance.is_spawned());
        on_spawned_fn(&mut new_instance, query, GenerationalIndex::new(generation, new_index));

        self.instances.push(new_instance);
        self.spawned.push(true);

        &mut self.instances[new_index]
    }

    pub fn despawn<F>(&mut self, instance: &mut T, query: &Query, on_despawned_fn: F)
        where F: FnOnce(&mut T, &Query)
    {
        debug_assert!(self.is_valid());
        debug_assert!(instance.is_spawned());

        let index = instance.id().index();
        debug_assert!(self.spawned[index]);
        debug_assert!(std::ptr::eq(&self.instances[index], instance)); // Ensure addresses are the same.

        on_despawned_fn(instance, query);
        self.spawned.set(index, false);
    }

    #[inline]
    pub fn spawned_count(&self) -> usize {
        self.spawned.count_ones()
    }

    #[inline]
    pub fn is_valid(&self) -> bool {
        self.instances.len() == self.spawned.len()
    }

    #[inline]
    pub fn iter(&self) -> SpawnPoolIter<'_, T> {
        SpawnPoolIter {
            instances: self.instances.iter().enumerate(),
            spawned: &self.spawned,
        }
    }

    #[inline]
    pub fn iter_mut(&mut self) -> SpawnPoolIterMut<'_, T> {
        SpawnPoolIterMut {
            instances: self.instances.iter_mut().enumerate(),
            spawned: &self.spawned,
        }
    }

    #[inline]
    pub fn try_get(&self, id: GenerationalIndex) -> Option<&T> {
        debug_assert!(self.is_valid());

        if !id.is_valid() {
            return None;
        }

        let index = id.index();
        if !self.spawned[index] {
            return None;
        }

        let instance = &self.instances[index];
        debug_assert!(instance.is_spawned());

        if instance.id().generation != id.generation() {
            return None;
        }

        Some(instance)
    }

    #[inline]
    pub fn try_get_mut(&mut self, id: GenerationalIndex) -> Option<&mut T> {
        debug_assert!(self.is_valid());

        if !id.is_valid() {
            return None;
        }

        let index = id.index();
        if !self.spawned[index] {
            return None;
        }

        let instance = &mut self.instances[index];
        debug_assert!(instance.is_spawned());

        if instance.id().generation != id.generation() {
            return None;
        }

        Some(instance)
    }

    #[inline]
    pub fn try_get_at(&self, index: usize) -> Option<&T> {
        debug_assert!(self.is_valid());

        if !self.spawned[index] {
            return None;
        }

        let instance = &self.instances[index];
        debug_assert!(instance.is_spawned());
        Some(instance)
    }

    #[inline]
    pub fn try_get_at_mut(&mut self, index: usize) -> Option<&mut T> {
        debug_assert!(self.is_valid());

        if !self.spawned[index] {
            return None;
        }

        let instance = &mut self.instances[index];
        debug_assert!(instance.is_spawned());
        Some(instance)
    }
}

// ----------------------------------------------
// SpawnPool Serialization
// ----------------------------------------------

#[derive(Serialize, Deserialize)]
struct SpawnPoolSerializedHeader {
    spawned_count: usize,
    instance_count: usize,
    generation: u32,
}

impl<'config, T> Serialize for SpawnPool<T>
    where T: GameObject<'config> + Clone + Default + Serialize
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where S: Serializer
    {
        debug_assert!(self.is_valid());
        debug_assert!(self.generation != RESERVED_GENERATION);

        let header = SpawnPoolSerializedHeader {
            spawned_count: self.spawned_count(),
            instance_count: self.instances.len(),
            generation: self.generation,
        };

        let mut serialized_count = 0;

        // NOTE: + 1 for the header.
        let mut seq = serializer.serialize_seq(Some(header.spawned_count + 1))?;

        seq.serialize_element(&header)?;

        for (index, instance) in self.instances.iter().enumerate() {
            // Serialize only *spawned* instances.
            if self.spawned[index] {
                debug_assert!(instance.is_spawned());
                debug_assert!(instance.id().index() == index);
                debug_assert!(instance.id().generation() != RESERVED_GENERATION);

                seq.serialize_element(instance)?;
                serialized_count += 1;
            }
        }

        if header.spawned_count != serialized_count {
            log::error!("Expected to serialize {} spawned instances but found {} instead.", header.spawned_count, serialized_count);
            return Err(serde::ser::Error::custom("unexpected number of GameObject instances found"));
        }

        seq.end()
    }
}

impl<'de, 'config, T> Deserialize<'de> for SpawnPool<T>
    where T: GameObject<'config> + Clone + Default + Deserialize<'de>
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: Deserializer<'de>
    {
        struct PoolVisitor<T> {
            marker: std::marker::PhantomData<T>,
        }

        impl<'de, 'config, T> Visitor<'de> for PoolVisitor<T>
            where T: GameObject<'config> + Clone + Default + Deserialize<'de>
        {
            type Value = SpawnPool<T>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a sequence starting with a header, followed by spawned GameObject instances")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
                where A: SeqAccess<'de>
            {
                // First element: info header
                let header: SpawnPoolSerializedHeader = seq
                    .next_element()?
                    .ok_or_else(|| {
                        log::error!("Failed to deserialize SpawnPoolSerializedHeader!");
                        serde::de::Error::custom("missing SpawnPoolSerializedHeader")
                    })?;

                if header.instance_count == 0 {
                    return Err(serde::de::Error::custom("SpawnPoolSerializedHeader::instance_count == 0"));
                }
                if header.generation == RESERVED_GENERATION {
                    return Err(serde::de::Error::custom("SpawnPoolSerializedHeader::generation == RESERVED_GENERATION"));
                }

                // Remaining elements: spawned instances
                let mut pool = SpawnPool::<T>::new(header.instance_count, header.generation);

                let mut deserialized_count = 0;
                loop {
                    let next = match seq.next_element::<T>() {
                        Ok(next) => next,
                        Err(err) => {
                            log::error!("Failed to deserialize SpawnPool instance [{deserialized_count}]: {err}");
                            return Err(err);
                        },
                    };

                    if let Some(instance) = next {
                        let index = instance.id().index();
                        debug_assert!(instance.id().generation() != RESERVED_GENERATION);
                        debug_assert!(instance.id().generation()  < header.generation);

                        pool.instances[index] = instance;
                        pool.spawned.set(index, true);

                        deserialized_count += 1;
                    } else {
                        // Finished deserializing the sequence.
                        break;
                    }
                }

                if header.spawned_count != deserialized_count {
                    log::error!("Expected to deserialize {} spawned instanced but found {} instead.", header.instance_count, deserialized_count);
                    return Err(serde::de::Error::custom("unexpected number of GameObject instances found"));
                }

                Ok(pool)
            }
        }

        deserializer.deserialize_seq(PoolVisitor { marker: std::marker::PhantomData })
    }
}

// ----------------------------------------------
// Spawner
// ----------------------------------------------

pub struct Spawner<'config, 'tile_sets, 'query> {
    query: &'query Query<'config, 'tile_sets>,
}

pub enum SpawnerResult<'config, 'tile_sets> {
    Building(&'config mut Building<'config>),
    Unit(&'config mut Unit<'config>),
    Tile(&'tile_sets mut Tile<'tile_sets>),
    Err(String),
}

impl SpawnerResult<'_, '_> {
    #[inline]
    pub fn is_err(&self) -> bool {
        matches!(self, Self::Err(_))
    }

    #[inline]
    pub fn is_ok(&self) -> bool {
        !self.is_err()
    }
}

impl<'config, 'tile_sets, 'query> Spawner<'config, 'tile_sets, 'query> {
    #[inline]
    pub fn new(query: &'query Query<'config, 'tile_sets>) -> Self {
        Self { query }
    }

    // Spawn a GameObject (Building, Unit) or place a Tile without associated game state.
    pub fn try_spawn_tile_with_def(&self, target_cell: Cell, tile_def: &'tile_sets TileDef) -> SpawnerResult {
        debug_assert!(target_cell.is_valid());
        debug_assert!(tile_def.is_valid());

        if tile_def.is(TileKind::Building) {
            // Spawn Building:
            match self.try_spawn_building_with_tile_def(target_cell, tile_def) {
                Ok(building) => SpawnerResult::Building(building),
                Err(err) => SpawnerResult::Err(err),
            }
        } else if tile_def.is(TileKind::Unit) {
            // Spawn Unit:
            match self.try_spawn_unit_with_tile_def(target_cell, tile_def) {
                Ok(unit) => SpawnerResult::Unit(unit),
                Err(err) => SpawnerResult::Err(err),
            }
        } else {
            // No associated GameObject, place plain tile.
            match self.query.tile_map().try_place_tile(target_cell, tile_def) {
                Ok(tile) => SpawnerResult::Tile(tile),
                Err(err) => SpawnerResult::Err(err),
            }
        }
    }

    // Despawns a GameObject at the given cell and removes the Tile from the map.
    pub fn despawn_tile(&self, tile: &Tile) {
        debug_assert!(tile.is_valid());

        let base_cell = tile.base_cell();
        let has_game_object = tile.game_object_handle().is_valid();

        if tile.is(TileKind::Building | TileKind::Blocker) && has_game_object {
            // Despawn Building:
            self.despawn_building_at_cell(base_cell);
        } else if tile.is(TileKind::Unit) && has_game_object {
            // Despawn Unit:
            self.despawn_unit_at_cell(base_cell);
        } else {
            // No GameObject, just remove the tile directly.
            if let Err(err) = self.query.tile_map().try_clear_tile_from_layer(base_cell, tile.layer_kind()) {
                despawn_error("Tile", &err);
            }
        }
    }

    pub fn despawn_tile_at_cell(&self, tile_base_cell: Cell, layer_kind: TileMapLayerKind) {
        debug_assert!(tile_base_cell.is_valid());
        if let Some(tile) = self.query.tile_map().try_tile_from_layer(tile_base_cell, layer_kind) {
            self.despawn_tile(tile);
        }
    }

    // ----------------------
    // Buildings:
    // ----------------------

    pub fn try_spawn_building_with_tile_def(&self,
                                            building_base_cell: Cell,
                                            building_tile_def: &TileDef)
                                            -> Result<&'query mut Building<'config>, String> {

        self.query.world().try_spawn_building_with_tile_def(
            self.query,
            building_base_cell,
            building_tile_def)
    }

    pub fn despawn_building(&self, building: &mut Building<'config>) {
        if let Err(err) = self.query.world().despawn_building(self.query, building) {
            despawn_error("Building", &err);
        }
    }

    pub fn despawn_building_at_cell(&self, building_base_cell: Cell) {
        if let Err(err) = self.query.world().despawn_building_at_cell(self.query, building_base_cell) {
            despawn_error("Building", &err);
        }
    }

    // ----------------------
    // Units:
    // ----------------------

    pub fn try_spawn_unit_with_tile_def(&self,
                                        unit_origin: Cell,
                                        unit_tile_def: &TileDef)
                                        -> Result<&'query mut Unit<'config>, String> {

        self.query.world().try_spawn_unit_with_tile_def(
            self.query,
            unit_origin,
            unit_tile_def)
    }

    pub fn try_spawn_unit_with_config(&self,
                                      unit_origin: Cell,
                                      unit_config_key: UnitConfigKey)
                                      -> Result<&'query mut Unit<'config>, String> {

        self.query.world().try_spawn_unit_with_config(
            self.query,
            unit_origin,
            unit_config_key)
    }

    pub fn despawn_unit(&self, unit: &mut Unit<'config>) {
        if let Err(err) = self.query.world().despawn_unit(self.query, unit) {
            despawn_error("Unit", &err);
        }
    }

    pub fn despawn_unit_at_cell(&self, unit_base_cell: Cell) {
        if let Err(err) = self.query.world().despawn_unit_at_cell(self.query, unit_base_cell) {
            despawn_error("Unit", &err);
        }
    }
}

#[cold]
fn despawn_error(what: &str, err: &str) {
    if cfg!(debug_assertions) {
        panic!("Despawn {what} Failed: {err}");
    } else {
        log::error!(log::channel!("world"), "Despawn {what} Failed: {err}");
    }
}
