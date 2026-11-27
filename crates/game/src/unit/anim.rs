use common::hash::{self, PreHashedKeyMap, StrHashPair, StringHash};
use engine::log;
use serde::{Deserialize, Serialize};

use super::navigation::UnitDirection;
use crate::tile::{Tile, sets::TileDef};

// ----------------------------------------------
// UnitAnimSets
// ----------------------------------------------

pub type UnitAnimSetKey = StrHashPair;

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct UnitAnimSets {
    // Hash of current anim set we're playing.
    current_anim_set_key: StringHash,

    // Maps from anim set name hash to anim set index.
    anim_set_index_map: PreHashedKeyMap<StringHash, usize>,
}

impl UnitAnimSets {
    pub const IDLE: UnitAnimSetKey = UnitAnimSetKey::from_str("idle"); // Generic idle anim.
    pub const IDLE_NE: UnitAnimSetKey = UnitAnimSetKey::from_str("idle_ne");
    pub const IDLE_NW: UnitAnimSetKey = UnitAnimSetKey::from_str("idle_nw");
    pub const IDLE_SE: UnitAnimSetKey = UnitAnimSetKey::from_str("idle_se");
    pub const IDLE_SW: UnitAnimSetKey = UnitAnimSetKey::from_str("idle_sw");
    pub const IDLE_N: UnitAnimSetKey = UnitAnimSetKey::from_str("idle_n");
    pub const IDLE_S: UnitAnimSetKey = UnitAnimSetKey::from_str("idle_s");
    pub const IDLE_E: UnitAnimSetKey = UnitAnimSetKey::from_str("idle_e");
    pub const IDLE_W: UnitAnimSetKey = UnitAnimSetKey::from_str("idle_w");

    pub const WALK_NE: UnitAnimSetKey = UnitAnimSetKey::from_str("walk_ne");
    pub const WALK_NW: UnitAnimSetKey = UnitAnimSetKey::from_str("walk_nw");
    pub const WALK_SE: UnitAnimSetKey = UnitAnimSetKey::from_str("walk_se");
    pub const WALK_SW: UnitAnimSetKey = UnitAnimSetKey::from_str("walk_sw");
    pub const WALK_N: UnitAnimSetKey = UnitAnimSetKey::from_str("walk_n");
    pub const WALK_S: UnitAnimSetKey = UnitAnimSetKey::from_str("walk_s");
    pub const WALK_E: UnitAnimSetKey = UnitAnimSetKey::from_str("walk_e");
    pub const WALK_W: UnitAnimSetKey = UnitAnimSetKey::from_str("walk_w");

    // Every set the game plays by name, with the direction a walk or directional idle faces.
    const KNOWN: [(UnitAnimSetKey, UnitDirection); 17] = [
        (Self::IDLE, UnitDirection::Idle),
        (Self::IDLE_NE, UnitDirection::NE),
        (Self::IDLE_NW, UnitDirection::NW),
        (Self::IDLE_SE, UnitDirection::SE),
        (Self::IDLE_SW, UnitDirection::SW),
        (Self::IDLE_N, UnitDirection::N),
        (Self::IDLE_S, UnitDirection::S),
        (Self::IDLE_E, UnitDirection::E),
        (Self::IDLE_W, UnitDirection::W),
        (Self::WALK_NE, UnitDirection::NE),
        (Self::WALK_NW, UnitDirection::NW),
        (Self::WALK_SE, UnitDirection::SE),
        (Self::WALK_SW, UnitDirection::SW),
        (Self::WALK_N, UnitDirection::N),
        (Self::WALK_S, UnitDirection::S),
        (Self::WALK_E, UnitDirection::E),
        (Self::WALK_W, UnitDirection::W),
    ];

    pub fn new(tile: &mut Tile, new_anim_set_key: UnitAnimSetKey) -> Self {
        let mut anim_set = Self::default();
        anim_set.set_anim(tile, new_anim_set_key);
        anim_set
    }

    pub fn clear(&mut self) {
        self.current_anim_set_key = hash::NULL_HASH;
        self.anim_set_index_map.clear();
    }

    pub fn set_anim(&mut self, tile: &mut Tile, new_anim_set_key: UnitAnimSetKey) -> bool {
        if self.current_anim_set_key != new_anim_set_key.hash {
            if let Some(index) = self.find_index(tile, new_anim_set_key) {
                self.current_anim_set_key = new_anim_set_key.hash;
                tile.set_anim_set_index(index);
                return true;
            } else {
                return false;
            }
        }
        true // already playing this anim set.
    }

    pub fn current_anim_name(&self) -> &'static str {
        let curr = self.current_anim_set_key;
        if curr == hash::NULL_HASH {
            return "<none>";
        }
        match Self::KNOWN.iter().find(|(key, _)| key.hash == curr) {
            Some((key, _)) => key.string,
            None => panic!("Unknown current animation!"),
        }
    }

    // The direction the set playing faces (a walk or directional idle), if any.
    pub fn current_direction(&self) -> Option<UnitDirection> {
        let curr = self.current_anim_set_key;
        Self::KNOWN.iter().find(|(key, _)| key.hash == curr).map(|(_, direction)| *direction)
    }

    // Whether the unit's tile has the anim set (keys are pre-hashed: a map lookup, no hashing).
    pub fn has_anim(&mut self, tile: &Tile, anim_set_key: UnitAnimSetKey) -> bool {
        self.find_index(tile, anim_set_key).is_some()
    }

    fn find_index(&mut self, tile: &Tile, anim_set_key: UnitAnimSetKey) -> Option<usize> {
        if self.anim_set_index_map.is_empty() {
            // Lazily init on demand.
            self.build_mapping(tile.tile_def(), tile.variation_index());
        }

        self.anim_set_index_map.get(&anim_set_key.hash).copied()
    }

    fn build_mapping(&mut self, tile_def: &TileDef, variation_index: usize) {
        debug_assert!(self.anim_set_index_map.is_empty());

        if variation_index >= tile_def.variations.len() {
            return;
        }

        let variation = &tile_def.variations[variation_index];

        for (index, anim_set) in variation.anim_sets.iter().enumerate() {
            if self.anim_set_index_map.insert(anim_set.hash, index).is_some() {
                log::error!(
                    log::channel!("unit"),
                    "Unit '{}': An entry for anim set '{}' ({:#X}) already exists at index: {index}!",
                    tile_def.name,
                    anim_set.name,
                    anim_set.hash
                );
            }
        }
    }
}
