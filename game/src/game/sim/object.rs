#![allow(clippy::while_let_on_iterator)]

use core::iter;
use core::slice;
use bitvec::vec::BitVec;

use crate::{
    imgui_ui::UiSystem,
    utils::coords::{CellRange, WorldToScreenTransform}
};

use super::{
    Query,
    world::WorldStats,
    debug::DebugUiMode
};

// ----------------------------------------------
// GenerationalIndex
// ----------------------------------------------

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
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
    pub fn new(capacity: usize) -> Self {
        let default_instance = T::default();
        Self {
            instances: vec![default_instance; capacity],
            spawned: BitVec::repeat(false, capacity),
            generation: 0,
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
