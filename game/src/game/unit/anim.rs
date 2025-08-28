use crate::{
    log,
    tile::{Tile, sets::TileDef},
    utils::hash::{
        StrHashPair,
        StringHash,
        PreHashedKeyMap
    }
};

// ----------------------------------------------
// UnitAnimSets
// ----------------------------------------------

pub type UnitAnimSetKey = StrHashPair;

#[derive(Clone, Default)]
pub struct UnitAnimSets {
    // Hash of current anim set we're playing.
    current_anim_set_key: UnitAnimSetKey,

    // Maps from anim set name hash to anim set index.
    anim_set_index_map: PreHashedKeyMap<StringHash, usize>,
}

impl UnitAnimSets {
    pub const IDLE:    UnitAnimSetKey = UnitAnimSetKey::from_str("idle");
    pub const WALK_NE: UnitAnimSetKey = UnitAnimSetKey::from_str("walk_ne");
    pub const WALK_NW: UnitAnimSetKey = UnitAnimSetKey::from_str("walk_nw");
    pub const WALK_SE: UnitAnimSetKey = UnitAnimSetKey::from_str("walk_se");
    pub const WALK_SW: UnitAnimSetKey = UnitAnimSetKey::from_str("walk_sw");

    pub fn new(tile: &mut Tile, new_anim_set_key: UnitAnimSetKey) -> Self {
        let mut anim_set = Self::default();
        anim_set.set_anim(tile, new_anim_set_key);
        anim_set
    }

    pub fn clear(&mut self) {
        self.current_anim_set_key = UnitAnimSetKey::default();
        self.anim_set_index_map.clear();
    }

    pub fn set_anim(&mut self, tile: &mut Tile, new_anim_set_key: UnitAnimSetKey) {
        if self.current_anim_set_key.hash != new_anim_set_key.hash {
            self.current_anim_set_key = new_anim_set_key;
            if let Some(index) = self.find_index(tile, new_anim_set_key) {
                tile.set_anim_set_index(index);
            }
        }
    }

    pub fn current_anim(&self) -> UnitAnimSetKey {
        self.current_anim_set_key
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
                log::error!(log::channel!("unit"), "Unit '{}': An entry for anim set '{}' ({:#X}) already exists at index: {index}!",
                            tile_def.name,
                            anim_set.name,
                            anim_set.hash);
            }
        }
    }
}
