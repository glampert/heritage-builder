use bitflags::bitflags;
use arrayvec::ArrayVec;
use smallvec::SmallVec;
use strum::IntoEnumIterator;
use serde::Deserialize;

use std::{
    fs,
    path::{Path, MAIN_SEPARATOR, MAIN_SEPARATOR_STR}
};

use crate::{
    bitflags_with_display,
    render::{
        TextureCache,
        TextureHandle
    },
    utils::{
        Size,
        Color,
        RectTexCoords,
        UnsafeMutable,
        coords::{
            Cell,
            CellRange
        },
        hash::{
            self,
            PreHashedKeyMap,
            StrHashPair,
            StringHash,
            NULL_HASH
        }
    }
};

use super::{
    map::{
        TileMapLayerKind,
        TileFlags,
        TILE_MAP_LAYER_COUNT
    }
};

// ----------------------------------------------
// Constants
// ----------------------------------------------

pub const BASE_TILE_SIZE: Size = Size{ width: 64, height: 32 };

// Terrain Layer:
pub const TERRAIN_GROUND_CATEGORY: StrHashPair = StrHashPair::from_str("ground");
pub const TERRAIN_WATER_CATEGORY:  StrHashPair = StrHashPair::from_str("water");

// Objects Layer:
pub const OBJECTS_BUILDINGS_CATEGORY:  StrHashPair = StrHashPair::from_str("buildings");
pub const OBJECTS_PROPS_CATEGORY:      StrHashPair = StrHashPair::from_str("props");
pub const OBJECTS_UNITS_CATEGORY:      StrHashPair = StrHashPair::from_str("units");
pub const OBJECTS_VEGETATION_CATEGORY: StrHashPair = StrHashPair::from_str("vegetation");

// ----------------------------------------------
// TileKind
// ----------------------------------------------

bitflags_with_display! {
    #[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
    pub struct TileKind: u8 {
        // Base Archetypes:
        const Terrain    = 1 << 0;
        const Object     = 1 << 1;
        const Blocker    = 1 << 2; // Draws nothing; blocker for multi-tile buildings, placed in the Objects layer.

        // Specialized tile kinds (Object Archetype & Objects Layer):
        const Building   = 1 << 3;
        const Prop       = 1 << 4;
        const Unit       = 1 << 5;
        const Vegetation = 1 << 6;
    }
}

impl TileKind {
    #[inline]
    fn specialized_kind_for_category(category_hash: StringHash) -> Self {
        if category_hash == OBJECTS_BUILDINGS_CATEGORY.hash {
            TileKind::Building
        } else if category_hash == OBJECTS_PROPS_CATEGORY.hash {
            TileKind::Prop
        } else if category_hash == OBJECTS_UNITS_CATEGORY.hash {
            TileKind::Unit
        } else if category_hash == OBJECTS_VEGETATION_CATEGORY.hash {
            TileKind::Vegetation
        } else {
            panic!("Unknown Tile Category hash!");
        }
    }
}

// ----------------------------------------------
// TileTexInfo
// ----------------------------------------------

#[derive(Default)]
pub struct TileTexInfo {
    pub texture: TextureHandle,
    pub coords: RectTexCoords,
}

impl TileTexInfo {
    pub fn new(texture: TextureHandle) -> Self {
        Self {
            texture: texture,
            coords: RectTexCoords::default(),
        }
    }

    pub fn is_valid(&self) -> bool {
        self.texture.is_valid()
    }
}

// ----------------------------------------------
// TileSprite
// ----------------------------------------------

#[derive(Deserialize)]
pub struct TileSprite {
    // Name of the tile texture. Resolved into a TextureHandle post load.
    pub name: String,

    // Not stored in serialized data.
    #[serde(skip)]
    pub tex_info: TileTexInfo,
}

// ----------------------------------------------
// TileAnimSet
// ----------------------------------------------

#[derive(Deserialize)]
pub struct TileAnimSet {
    #[serde(default)]
    pub name: String,

    // Duration of the whole anim in seconds.
    // Optional, can be zero if there's only a single frame.
    #[serde(default)]
    duration: f32,

    // True if the animation will loop, false for play only once.
    // Ignored when there's only one frame.
    #[serde(default)]
    pub looping: bool,

    // Textures for each animation frame. Texture handles are resolved after loading.
    // SmallVec optimizes for Terrain (single frame anim).
    pub frames: SmallVec<[TileSprite; 1]>,
}

impl TileAnimSet {
    #[inline]
    pub fn anim_duration_secs(&self) -> f32 {
        self.duration
    }

    #[inline]
    pub fn frame_duration_secs(&self) -> f32 {
        let frame_count = self.frames.len();
        debug_assert!(frame_count != 0, "At least one animation frame required");
        self.duration / (frame_count as f32)
    }
}

// ----------------------------------------------
// TileVariation
// ----------------------------------------------

#[derive(Deserialize)]
pub struct TileVariation {
    // Variation name is optional for Terrain and Units.
    #[serde(default)]
    pub name: String,

    // AnimSet may contain one or more animation frames.
    pub anim_sets: SmallVec<[TileAnimSet; 1]>,
}

// ----------------------------------------------
// TileDef
// ----------------------------------------------

#[derive(Deserialize)]
pub struct TileDef {
    // Friendly display name.
    pub name: String,

    // Hash of `name`, computed post-load.
    #[serde(skip)]
    pub hash: StringHash,

    // Logical size for the tile map. Always a multiple of the base tile size.
    // Optional for Terrain tiles (always = BASE_TILE_SIZE), required otherwise.
    #[serde(default = "default_tile_size")]
    pub logical_size: Size,

    // Draw size for tile rendering. Can be any size ratio.
    // Optional in serialized data. Defaults to the value of `logical_size` if missing.
    #[serde(default)]
    pub draw_size: Size,

    // Tint color is optional in serialized data. Default to white if missing.
    #[serde(default)]
    pub color: Color,

    // Tile variations for buildings.
    // SmallVec optimizes for Terrain/Units with single variation.
    pub variations: SmallVec<[TileVariation; 1]>,

    // True if the tile fully occludes the terrain tiles below, so we can cull them.
    // Defaults to true for all Buildings, false for Units. Ignored for Terrain.
    #[serde(default = "default_occludes_terrain")]
    pub occludes_terrain: bool,

    // Tile kind & archetype combined, also defines which layer the tile can be placed on.
    // Resolved post-load based on layer and category.
    #[serde(skip, default = "default_tile_kind")]
    kind: TileKind,

    // Internal runtime index into TileCategory.
    #[serde(skip)]
    category_tiledef_index: i32,

    // Internal runtime index into TileSet.
    #[serde(skip)]
    tileset_category_index: i32,
}

impl TileDef {
    #[inline]
    pub fn kind(&self) -> TileKind {
        self.kind
    }

    #[inline]
    pub fn layer_kind(&self) -> TileMapLayerKind {
        TileMapLayerKind::from_tile_kind(self.kind)
    }

    #[inline]
    pub fn is(&self, kinds: TileKind) -> bool {
        self.kind.intersects(kinds)
    }

    #[inline]
    pub fn is_valid(&self) -> bool {
        !self.kind.is_empty() && self.logical_size.is_valid() && self.draw_size.is_valid()
    }

    #[inline]
    pub fn flags(&self) -> TileFlags {
        let mut flags = TileFlags::empty();
        flags.set(TileFlags::OccludesTerrain, self.occludes_terrain);
        flags
    }

    #[inline]
    pub fn size_in_cells(&self) -> Size {
        // `logical_size` is assumed to be a multiple of the base tile size.
        Size::new(self.logical_size.width / BASE_TILE_SIZE.width, self.logical_size.height / BASE_TILE_SIZE.height)
    }

    #[inline]
    pub fn has_multi_cell_footprint(&self) -> bool {
        let size = self.size_in_cells();
        size.width > 1 || size.height > 1 // Multi-tile building?
    }

    #[inline]
    pub fn calc_footprint_cells(&self, start_cell: Cell) -> CellRange {
        // Buildings can occupy multiple cells; Find which ones:
        let size = self.size_in_cells();
        let end_cell = Cell::new(start_cell.x + size.width - 1, start_cell.y + size.height - 1);
        CellRange::new(start_cell, end_cell)
    }

    #[inline]
    pub fn texture_by_index(&self,
                            variation_index: usize,
                            anim_set_index: usize,
                            frame_index: usize) -> TextureHandle {
        if let Some(frame) = self.anim_frame_by_index(variation_index, anim_set_index, frame_index) {
            return frame.tex_info.texture;
        }
        TextureHandle::invalid()
    }

    #[inline]
    pub fn anim_frame_by_index(&self,
                               variation_index: usize,
                               anim_set_index: usize,
                               frame_index: usize) -> Option<&TileSprite> {
        if let Some(anim_set) = self.anim_set_by_index(variation_index, anim_set_index) {
            if frame_index < anim_set.frames.len() {
                return Some(&anim_set.frames[frame_index])
            }
        }
        None
    }

    #[inline]
    pub fn anim_set_by_index(&self,
                             variation_index: usize,
                             anim_set_index: usize) -> Option<&TileAnimSet> {
        if variation_index >= self.variations.len() {
            return None;
        }

        let variation = &self.variations[variation_index];
        if anim_set_index >= variation.anim_sets.len() {
            return None;
        }

        Some(&variation.anim_sets[anim_set_index])
    }

    pub fn anim_sets_count(&self, variation_index: usize) -> usize {
        if variation_index >= self.variations.len() {
            return 0;
        }
        self.variations[variation_index].anim_sets.len()
    }

    pub fn anim_frames_count(&self, variation_index: usize) -> usize {
        if variation_index >= self.variations.len() {
            return 0;
        }

        let variation = &self.variations[variation_index];
        let mut count = 0;
        for anim_set in &variation.anim_sets {
            count += anim_set.frames.len();
        }
        count
    }

    pub fn anim_set_name(&self, variation_index: usize, anim_set_index: usize) -> &str {
        if variation_index >= self.variations.len() {
            return "";
        }
        let variation = &self.variations[variation_index];
        if anim_set_index >= variation.anim_sets.len() {
            return "";
        }
        &variation.anim_sets[anim_set_index].name
    }

    pub fn variation_name(&self, variation_index: usize) -> &str {
        if variation_index >= self.variations.len() {
            return "";
        }
        &self.variations[variation_index].name
    }

    fn post_load(&mut self,
                 tex_cache: &mut impl TextureCache,
                 tile_set_path_with_category: &str,
                 layer: TileMapLayerKind,
                 category_hash: StringHash) -> bool {

        debug_assert!(self.hash != NULL_HASH);
        debug_assert!(category_hash != NULL_HASH);

        let archetype = layer.to_tile_archetype_kind();
        let specialized_type = {
            if layer == TileMapLayerKind::Objects {
                TileKind::specialized_kind_for_category(category_hash)
            } else {
                TileKind::empty() // No specialization for Terrain.
            }
        };

        self.kind = archetype | specialized_type;

        if self.name.is_empty() {
            eprintln!("TileDef '{}' name is missing! A name is required.", self.kind);
            return false;
        }

        if !self.logical_size.is_valid() {
            eprintln!("Invalid/missing TileDef logical size: '{}' - '{}'",
                      self.kind,
                      self.name);
            return false;
        }

        if (self.logical_size.width  % BASE_TILE_SIZE.width)  != 0 ||
           (self.logical_size.height % BASE_TILE_SIZE.height) != 0 {
            eprintln!("Invalid TileDef logical size ({:?})! Must be a multiple of BASE_TILE_SIZE: '{}' - '{}'",
                      self.logical_size,
                      self.kind,
                      self.name);
            return false;
        }

        if self.is(TileKind::Terrain) {
            // For terrain logical_size must be BASE_TILE_SIZE.
            if self.logical_size != BASE_TILE_SIZE {
                eprintln!("Terrain TileDef logical size must be equal to BASE_TILE_SIZE: '{}' - '{}'",
                          self.kind,
                          self.name);
                return false;
            }
            self.occludes_terrain = false;
        }

        if !self.draw_size.is_valid() {
            // Default to logical_size.
            self.draw_size = self.logical_size;
        }

        if self.variations.is_empty() {
            eprintln!("At least one variation is required! TileDef: '{}' - '{}'", self.kind, self.name);
            return false;
        }

        // Validate deserialized data and resolve texture handles:
        for variation in &mut self.variations {
            for anim_set in &mut variation.anim_sets {
                if anim_set.frames.is_empty() {
                    eprintln!("At least one animation frame is required! TileDef: '{}' - '{}'", self.kind, self.name);
                    return false;
                }

                for (frame_index, frame) in anim_set.frames.iter_mut().enumerate() {
                    if frame.name.is_empty() {
                        eprintln!("Missing sprite frame name for index [{frame_index}]. AnimSet: '{}', TileDef: '{}' - '{}'",
                                  anim_set.name,
                                  self.kind,
                                  self.name);
                        return false;
                    }

                    // Path formats:
                    //  terrain/<category>/<tile>.png
                    //  objects/<category>/<object_name>/<variation>/<anim_set>/<frame[N]>.png
                    let texture_path = match layer {
                        TileMapLayerKind::Terrain => {
                            format!("{}{}{}.png",
                                    tile_set_path_with_category,
                                    MAIN_SEPARATOR,
                                    frame.name)
                        },
                        TileMapLayerKind::Objects => {
                            // objects/<category>/<object_name>/
                            let mut path = format!("{}{}{}{}",
                                tile_set_path_with_category,
                                MAIN_SEPARATOR,
                                self.name,
                                MAIN_SEPARATOR);

                            // Do we have a variation? If not the anim_set name follows directly.
                            // + <variation>/
                            if !variation.name.is_empty() {
                                path += &variation.name;
                                path += MAIN_SEPARATOR_STR;
                            }

                            // Do we have an anim_set? If not the sprite frame image follows directly.
                            // + <anim_set>/
                            if !anim_set.name.is_empty() {
                                path += &anim_set.name;
                                path += MAIN_SEPARATOR_STR;
                            }

                            // + <frame[N]>.png
                            path += &frame.name;
                            path += ".png";
                            path
                        }
                    };

                    let frame_texture = tex_cache.load_texture(&texture_path);
                    frame.tex_info = TileTexInfo::new(frame_texture);
                }
            }
        }

        true
    }
}

// ----------------------------------------------
// Deserialization defaults
// ----------------------------------------------

#[inline]
const fn default_tile_kind() -> TileKind { TileKind::empty() }

#[inline]
const fn default_tile_size() -> Size { BASE_TILE_SIZE }

#[inline]
const fn default_occludes_terrain() -> bool { true }

// ----------------------------------------------
// EditableTileDef
// ----------------------------------------------

// This allows returning a mutable TileDef reference in try_get_editable_tile_def()
// for runtime editing purposes. We only require this functionality for debug and development.
type EditableTileDef = UnsafeMutable<TileDef>;

// ----------------------------------------------
// TileCategory
// ----------------------------------------------

#[derive(Deserialize)]
pub struct TileCategory {
    pub name: String, // E.g.: buildings, props, units, etc...

    #[serde(skip)]
    pub hash: StringHash, // Hash of `name`, computed post-load.

    // List of associated tiles.
    tile_defs: Vec<EditableTileDef>,

    // Internal runtime index into TileSet.
    #[serde(skip)]
    tileset_category_index: i32,

    // Maps from tile name to TileDef index in self.tiles[].
    #[serde(skip)]
    mapping: PreHashedKeyMap<StringHash, usize>,
}

impl TileCategory {
    pub fn is_empty(&self) -> bool {
        self.tile_defs.is_empty()
    }

    pub fn find_tile_def_by_name(&self, tile_name: &str) -> Option<&TileDef> {
        let tile_name_hash: StringHash = hash::fnv1a_from_str(tile_name);
        let entry_index = match self.mapping.get(&tile_name_hash) {
            Some(entry_index) => *entry_index,
            None => {
                eprintln!("TileCategory '{}': Couldn't find TileDef for '{}'.", self.name, tile_name);
                return None;
            }
        };
        Some(&self.tile_defs[entry_index])
    }

    pub fn find_tile_def_by_hash(&self, tile_name_hash: StringHash) -> Option<&TileDef> {
        let entry_index = match self.mapping.get(&tile_name_hash) {
            Some(entry_index) => *entry_index,
            None => {
                eprintln!("TileCategory '{}': Couldn't find TileDef for '{:#X}'.", self.name, tile_name_hash);
                return None;
            }
        };
        Some(&self.tile_defs[entry_index])
    }

    fn post_load(&mut self,
                 tex_cache: &mut impl TextureCache,
                 tile_set_path: &str,
                 layer: TileMapLayerKind) -> bool {

        debug_assert!(self.mapping.is_empty());
        debug_assert!(self.hash != NULL_HASH);

        if self.name.is_empty() {
            eprintln!("TileCategory name is missing! A name is required.");
            return false;
        }

        let tile_set_path_with_category =
            format!("{}{}{}", tile_set_path, MAIN_SEPARATOR, self.name);

        for (entry_index, editable_def) in self.tile_defs.iter_mut().enumerate() {
            let tile_def = editable_def.as_mut();

            if tile_def.name.is_empty() {
                eprintln!("TileCategory '{}': Invalid empty TileDef name! Index: [{}]",
                          self.name,
                          entry_index);
                return false;   
            }

            tile_def.category_tiledef_index = entry_index as i32;
            tile_def.tileset_category_index = self.tileset_category_index;

            let tile_name_hash: StringHash = hash::fnv1a_from_str(&tile_def.name);
            tile_def.hash = tile_name_hash;

            if !tile_def.post_load(tex_cache, &tile_set_path_with_category, layer, self.hash) {
                return false;
            }

            debug_assert!(tile_def.kind.is_empty() == false, "Missing TileKind flags!");

            if let Some(_) = self.mapping.insert(tile_name_hash, entry_index) {
                eprintln!("TileCategory '{}': An entry for key '{}' ({:#X}) already exists at index: {}!",
                          self.name,
                          tile_def.name,
                          tile_name_hash,
                          entry_index);
                return false;
            }
        }

        true
    }
}

// ----------------------------------------------
// TileSet
// ----------------------------------------------

#[derive(Deserialize)]
pub struct TileSet {
    // Layer, e.g.: Terrain, Object.
    // Also internal runtime index into TileSets.
    pub layer: TileMapLayerKind,

    // List of associated categories.
    categories: Vec<TileCategory>,

    // Maps from category name to TileCategory index in self.categories[].
    #[serde(skip)]
    mapping: PreHashedKeyMap<StringHash, usize>,
}

impl TileSet {
    pub fn is_empty(&self) -> bool {
        self.categories.is_empty()
    }

    pub fn find_category_by_name(&self, category_name: &str) -> Option<&TileCategory> {
        let category_name_hash: StringHash = hash::fnv1a_from_str(category_name);
        let entry_index = match self.mapping.get(&category_name_hash) {
            Some(entry_index) => *entry_index,
            None => {
                eprintln!("TileSet '{}': Couldn't find TileCategory for '{}'.",
                          self.layer,
                          category_name);
                return None;
            }
        };
        Some(&self.categories[entry_index])
    }

    pub fn find_category_by_hash(&self, category_name_hash: StringHash) -> Option<&TileCategory> {
        let entry_index = match self.mapping.get(&category_name_hash) {
            Some(entry_index) => *entry_index,
            None => {
                eprintln!("TileSet '{}': Couldn't find TileCategory for '{:#X}'.",
                          self.layer,
                          category_name_hash);
                return None;
            }
        };
        Some(&self.categories[entry_index])
    }

    fn post_load(&mut self, tex_cache: &mut impl TextureCache, tile_set_path: &str) -> bool {
        debug_assert!(self.mapping.is_empty());

        for (entry_index, category) in self.categories.iter_mut().enumerate() {
            if category.name.is_empty() {
                eprintln!("TileSet '{}': Invalid empty category name! Index: [{}]",
                          self.layer,
                          entry_index);
                return false;   
            }

            category.tileset_category_index = entry_index as i32;

            let category_name_hash: StringHash = hash::fnv1a_from_str(&category.name);
            category.hash = category_name_hash;

            if !category.post_load(tex_cache, tile_set_path, self.layer) {
                return false;
            }

            if let Some(_) = self.mapping.insert(category_name_hash, entry_index) {
                eprintln!("TileSet '{}': An entry for key '{}' ({:#X}) already exists at index: {}!",
                          self.layer,
                          category.name,
                          category_name_hash,
                          entry_index);
                return false;
            }
        }

        true
    }
}

// ----------------------------------------------
// TileDefHandle
// ----------------------------------------------

#[derive(Copy, Clone)]
pub struct TileDefHandle {
    tileset_index: u16,
    tileset_category_index: u16,
    category_tiledef_index: u16,
}

impl TileDefHandle {
    #[inline]
    pub fn new(tile_set: &TileSet, tile_category: &TileCategory, tile_def: &TileDef) -> Self {
        Self {
            tileset_index: tile_set.layer as u16,
            tileset_category_index: tile_category.tileset_category_index.try_into().expect("Index cannot fit in a u16"),
            category_tiledef_index: tile_def.category_tiledef_index.try_into().expect("Index cannot fit in a u16"),
        }
    }
}

// ----------------------------------------------
// TileSets
// ----------------------------------------------

pub struct TileSets {
    sets: ArrayVec<TileSet, TILE_MAP_LAYER_COUNT>,
}

impl TileSets {
    pub fn load(tex_cache: &mut impl TextureCache) -> Self {
        let mut tile_sets = Self {
            sets: ArrayVec::new(),
        };
        tile_sets.load_all_layers(tex_cache);
        tile_sets
    }

    pub fn is_empty(&self) -> bool {
        self.sets.is_empty()
    }

    #[inline]
    pub fn handle_to_tile_def(&self, handle: TileDefHandle) -> Option<&TileDef> {
            let set_idx  = handle.tileset_index as usize;          // TileSet index into TileSets.
            let cat_idx  = handle.tileset_category_index as usize; // TileCategory index into TileSet.
            let tile_idx = handle.category_tiledef_index as usize; // TileDef index into TileCategory.

            if set_idx >= self.sets.len() {
                return None;
            }

            let set = &self.sets[set_idx];
            if cat_idx >= set.categories.len() {
                return None;
            }

            let cat = &set.categories[cat_idx];
            if tile_idx >= cat.tile_defs.len() {
                return None;
            }

            let tile_def = &*cat.tile_defs[tile_idx];
            debug_assert!(set.layer as usize == set_idx);
            debug_assert!(cat.tileset_category_index as usize == cat_idx);
            debug_assert!(tile_def.tileset_category_index as usize == cat_idx);
            debug_assert!(tile_def.category_tiledef_index as usize == tile_idx);
            Some(tile_def)
    }

    pub fn find_category_for_tile_def(&self, tile_def: &TileDef) -> Option<&TileCategory> {
        let layer_idx = tile_def.layer_kind() as usize;
        let set_idx = tile_def.tileset_category_index as usize;
        let cat_idx = tile_def.category_tiledef_index as usize;

        let set = &self.sets[layer_idx];
        if set_idx >= set.categories.len() {
            return None;
        }

        let cat = &self.sets[layer_idx].categories[set_idx];
        if cat_idx >= cat.tile_defs.len() {
            return None;
        }

        debug_assert!(cat.tile_defs[cat_idx].category_tiledef_index == tile_def.category_tiledef_index);
        debug_assert!(cat.tile_defs[cat_idx].tileset_category_index == tile_def.tileset_category_index);
        Some(cat)
    }

    pub fn find_set_for_tile_def(&self, tile_def: &TileDef) -> Option<&TileSet> {
        let layer = tile_def.layer_kind();
        let set = &self.sets[layer as usize];
        debug_assert!(set.layer == layer);
        Some(set)
    }

    pub fn find_set_by_layer(&self, layer: TileMapLayerKind) -> Option<&TileSet> {
        let index = layer as usize;

        if index >= self.sets.len() {
            return None;
        }
        if self.sets[index].layer != layer {
            return None;
        }

        Some(&self.sets[index])
    }

    pub fn find_category_by_name(&self,
                                 layer: TileMapLayerKind,
                                 category_name: &str) -> Option<&TileCategory> {
        let set = self.find_set_by_layer(layer)?;
        set.find_category_by_name(category_name)
    }

    pub fn find_category_by_hash(&self,
                                 layer: TileMapLayerKind,
                                 category_name_hash: StringHash) -> Option<&TileCategory> {
        let set = self.find_set_by_layer(layer)?;
        set.find_category_by_hash(category_name_hash)
    }

    pub fn find_tile_def_by_name(&self,
                                 layer: TileMapLayerKind,
                                 category_name: &str,
                                 tile_name: &str) -> Option<&TileDef> {
        let cat = self.find_category_by_name(layer, category_name)?;
        cat.find_tile_def_by_name(tile_name)
    }

    pub fn find_tile_def_by_hash(&self,
                                 layer: TileMapLayerKind,
                                 category_name_hash: StringHash,
                                 tile_name_hash: StringHash) -> Option<&TileDef> {
        let cat = self.find_category_by_hash(layer, category_name_hash)?;
        cat.find_tile_def_by_hash(tile_name_hash)
    }

    pub fn for_each_set<F>(&self, mut visitor_fn: F)
        where
            F: FnMut(&TileSet) -> bool
    {
        for set in &self.sets {
            let should_continue = visitor_fn(set);
            if !should_continue {
                return;
            }
        }
    }

    pub fn for_each_category<F>(&self, mut visitor_fn: F)
        where
            F: FnMut(&TileSet, &TileCategory) -> bool
    {
        for set in &self.sets {
            for cat in &set.categories {
                let should_continue = visitor_fn(set, cat);
                if !should_continue {
                    return;
                }
            }
        }
    }

    pub fn for_each_tile_def<F>(&self, mut visitor_fn: F)
        where 
            F: FnMut(&TileSet, &TileCategory, &TileDef) -> bool
    {
        for set in &self.sets {
            for cat in &set.categories {
                for editable_def in &cat.tile_defs {
                    let should_continue = visitor_fn(set, cat, editable_def);
                    if !should_continue {
                        return;
                    }
                }
            }
        }
    }

    // Get back a mutable reference for the given TileDef.
    // This function is only intended for development/debug
    // and use within the ImGui TileInspector widget.
    pub fn try_get_editable_tile_def(&self, tile_def: &TileDef) -> Option<&mut TileDef> {
        if let Some(cat) = self.find_category_for_tile_def(tile_def) {
            let editable_def = &cat.tile_defs[tile_def.category_tiledef_index as usize];
            // SAFETY: We're assuming that mutable access is sound here
            // (e.g., no overlapping accesses to the same TileDef elsewhere)
            let mutable_def = editable_def.as_mut();
            return Some(mutable_def);
        }
        None
    }

    // Terrain file structure:
    // -----------------------
    //  * Simple, no animations or variations. Each tile is a single .png image.
    // Structure:
    //  terrain/tile_set.json
    //  terrain/<category>/<tile>.png,*
    // Example:
    //  terrain/ground/dirt.png
    //  terrain/ground/grass.png
    //  ...
    //  terrain/water/blue.png
    //  terrain/water/green.png
    //
    // Objects file structure:
    // -------------------------
    //  * Objects can have variations and animations.
    // Structure:
    //  objects/tile_set.json
    //  objects/<category>/<objects_name>/<variation>/<anim_set>/<frame[N]>.png,*
    // Example:
    //  objects/buildings/house/var0/build
    //  objects/buildings/house/var0/fire
    // ...
    //  objects/buildings/house/var1/build
    //  objects/buildings/house/var1/fire
    // ...
    //  objects/buildings/house/var0/build/frame0.png
    //  objects/buildings/house/var0/build/frame1.png
    //  objects/buildings/house/var0/build/frame2.png
    //
    // Variations and animations are optional so the structure can also be:
    //
    //  objects/<category>/<object_name>/<anim_set>/<frame[N]>.png,*
    // Or:
    //  objects/<category>/<object_name>/<variation>/<frame[N]>.png,*
    //
    // Example:
    //  objects/units/ped/idle/frame0.png
    //  objects/units/ped/idle/frame1.png
    // ...
    //  objects/units/ped/walk_left/frame0.png
    //  objects/units/ped/walk_left/frame1.png
    //
    fn load_all_layers(&mut self, tex_cache: &mut impl TextureCache) {
        for layer in TileMapLayerKind::iter() {
            let tile_set_path = layer.assets_path();
            if !self.load_tile_set(tex_cache, tile_set_path, layer) {
                eprintln!("TileSet '{layer}' ({tile_set_path}) didn't load!");
            }
        }
    }

    fn load_tile_set(&mut self,
                     tex_cache: &mut impl TextureCache,
                     tile_set_path: &str,
                     layer: TileMapLayerKind) -> bool {

        debug_assert!(tile_set_path.is_empty() == false);
        let tile_set_json_path = Path::new(tile_set_path).join("tile_set.json");

        let json = match fs::read_to_string(&tile_set_json_path) {
            Ok(json) => json,
            Err(err) => {
                eprintln!("Failed to read TileSet json file from path {:?}: {}", tile_set_json_path, err);
                return false;
            }
        };

        let mut tile_set: TileSet = match serde_json::from_str(&json) {
            Ok(tile_set) => tile_set,
            Err(err) => {
                eprintln!("Failed to deserialize TileSet from path {:?}: {}", tile_set_json_path, err);
                return false;
            }
        };

        if tile_set.layer != layer {
            eprintln!("TileSet layer kind mismatch! Json specifies '{}' but expected '{}' for this set.",
                      tile_set.layer,
                      layer);
            return false;
        }

        if !tile_set.post_load(tex_cache, tile_set_path) {
            eprintln!("Post load failed for TileSet '{}' - {:?}!", layer, tile_set_json_path);
            return false;
        }

        debug_assert!(self.sets.len() == (layer as usize));

        println!("Successfully loaded TileSet '{layer}' from path {:?}.", tile_set_json_path);
    
        self.sets.push(tile_set);

        true
    }
}
