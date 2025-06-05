use smallvec::{SmallVec, smallvec};
use strum::{EnumCount, IntoEnumIterator};
use strum_macros::{Display, EnumCount, EnumIter};
use serde::Deserialize;

use std::{
    fs,
    path::{Path, MAIN_SEPARATOR}
};

use crate::{
    render::{TextureCache, TextureHandle},
    utils::{Size, Cell, Color, RectTexCoords},
    utils::hash::{self, PreHashedKeyMap, StringHash}
};

use super::{
    map::{self, TileMapLayerKind, TileFlags, TILE_MAP_LAYER_COUNT}
};

// ----------------------------------------------
// Constants / helper types
// ----------------------------------------------

pub const BASE_TILE_SIZE: Size = Size{ width: 64, height: 32 };

// Can fit a 6x6 tile without allocating.
pub type TileFootprintList = SmallVec<[Cell; 36]>;

// ----------------------------------------------
// TileKind
// ----------------------------------------------

#[repr(u32)]
#[derive(Copy, Clone, PartialEq, Debug, Display, EnumCount, EnumIter, Deserialize)]
pub enum TileKind {
    Empty,   // No tile, draws nothing.
    Blocker, // Draws nothing; blocker for multi-tile buildings, placed in the Buildings layer.
    Terrain,
    Building,
    Unit,
}

pub const TILE_KIND_COUNT: usize = TileKind::COUNT;

// ----------------------------------------------
// TileTexInfo
// ----------------------------------------------

#[derive(Clone)]
pub struct TileTexInfo {
    pub texture: TextureHandle,
    pub coords: RectTexCoords,
}

impl Default for TileTexInfo {
    fn default() -> Self { Self::default() }
}

impl TileTexInfo {
    // NOTE: This needs to be const for static declarations, so we don't just derive from Default.
    pub const fn default() -> Self {
        Self {
            texture: TextureHandle::invalid(),
            coords: *RectTexCoords::default(),
        }
    }

    pub fn new(texture: TextureHandle) -> Self {
        Self {
            texture: texture,
            coords: *RectTexCoords::default(),
        }
    }

    pub fn is_valid(&self) -> bool {
        self.texture.is_valid()
    }
}

// ----------------------------------------------
// TileSprite
// ----------------------------------------------

#[derive(Clone, Deserialize)]
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

#[derive(Clone, Deserialize)]
pub struct TileAnimSet {
    #[serde(default)]
    pub name: String,

    // Duration of the whole anim in seconds.
    // Optional, can be zero if there's only a single frame.
    #[serde(default)]
    pub duration: f32,

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

#[derive(Clone, Deserialize)]
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

#[derive(Clone, Deserialize)]
pub struct TileDef {
    // Friendly display name.
    pub name: String,

    // Tile kind, also defines which layer the tile can be placed on.
    #[serde(default = "default_tile_kind")]
    pub kind: TileKind,

    // Internal runtime index into TileCategory.
    #[serde(skip)]
    category_tile_index: i32,

    // Internal runtime index into TileSet.
    #[serde(skip)]
    tileset_category_index: i32,

    // True if the tile fully occludes the terrain tiles below, so we can cull them.
    // Defaults to true for all Buildings, false for Units. Ignored for Terrain.
    #[serde(default = "default_occludes_terrain")]
    pub occludes_terrain: bool,

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
}

impl TileDef {
    const fn new(tile_kind: TileKind) -> Self {
        Self {
            name: String::new(),
            kind: tile_kind,
            category_tile_index: -1,
            tileset_category_index: -1,
            occludes_terrain: false,
            logical_size: BASE_TILE_SIZE,
            draw_size: BASE_TILE_SIZE,
            color: Color::white(),
            variations: SmallVec::new_const(),
        }
    }

    pub const fn empty() -> &'static Self {
        static EMPTY_TILE: TileDef = TileDef::new(TileKind::Empty);
        &EMPTY_TILE
    }

    pub const fn blocker() -> &'static Self {
        static BLOCKER_TILE: TileDef = TileDef::new(TileKind::Blocker);
        &BLOCKER_TILE
    }

    #[inline]
    pub fn is_valid(&self) -> bool {
        self.logical_size.is_valid() && self.draw_size.is_valid()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.kind == TileKind::Empty
    }

    #[inline]
    pub fn is_terrain(&self) -> bool {
        self.kind == TileKind::Terrain
    }

    #[inline]
    pub fn is_building(&self) -> bool {
        self.kind == TileKind::Building
    }

    #[inline]
    pub fn is_blocker(&self) -> bool {
        self.kind == TileKind::Blocker
    }

    #[inline]
    pub fn is_unit(&self) -> bool {
        self.kind == TileKind::Unit
    }

    #[inline]
    pub fn tile_flags(&self) -> TileFlags {
        if self.occludes_terrain { 
            TileFlags::OccludesTerrain
        } else {
            TileFlags::empty()
        }
    }

    #[inline]
    pub fn size_in_cells(&self) -> Size {
        // `logical_size` is assumed to be a multiple of the base tile size.
        Size::new(
            self.logical_size.width / BASE_TILE_SIZE.width,
            self.logical_size.height / BASE_TILE_SIZE.height)
    }

    #[inline]
    pub fn has_multi_cell_footprint(&self) -> bool {
        let size = self.size_in_cells();
        size.width > 1 || size.height > 1 // Multi-tile building?
    }

    pub fn calc_footprint_cells(&self, base_cell: Cell) -> TileFootprintList {
        let mut footprint = TileFootprintList::new();

        if !self.is_empty() {
            let size = self.size_in_cells();
            debug_assert!(size.is_valid());

            // Buildings can occupy multiple cells; Find which ones:
            let start_cell = base_cell;
            let end_cell = Cell::new(start_cell.x + size.width - 1, start_cell.y + size.height - 1);

            for y in (start_cell.y..=end_cell.y).rev() {
                for x in (start_cell.x..=end_cell.x).rev() {
                    footprint.push(Cell::new(x, y));
                }
            }

            // Last cell should be the original starting cell (selection relies on this).
            debug_assert!(*footprint.last().unwrap() == base_cell);
        } else {
            // Empty tiles always occupy one cell.
            footprint.push(base_cell);
        }

        footprint
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

    pub fn variation_name(&self, variation_index: usize) -> &str {
        if variation_index >= self.variations.len() {
            return "";
        }
        &self.variations[variation_index].name
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

    fn post_load(&mut self,
                 tex_cache: &mut TextureCache,
                 tile_set_path_with_category: &str,
                 layer: TileMapLayerKind) -> bool {

        self.kind = map::layer_to_tile_kind(layer);

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

        if self.kind == TileKind::Terrain {
            // For terrain logical_size must be BASE_TILE_SIZE.
            if self.logical_size != BASE_TILE_SIZE {
                eprintln!("Terrain TileDef logical size must be equal to BASE_TILE_SIZE: '{}' - '{}'",
                          self.kind,
                          self.name);
                return false;
            }

            self.occludes_terrain = false;
        } else if self.kind == TileKind::Unit {
            // Units always have transparent backgrounds that won't fully cover underlying terrain tiles.
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
                if layer == TileMapLayerKind::Buildings {
                    if variation.name.is_empty() {
                        eprintln!("Variation name missing for TileDef: '{}' - '{}'", self.kind, self.name);
                        return false;
                    }
                    if anim_set.name.is_empty() {
                        eprintln!("AnimSet name missing for TileDef: '{}' - '{}'", self.kind, self.name);
                        return false;
                    }
                } else if layer == TileMapLayerKind::Units {
                    if anim_set.name.is_empty() {
                        eprintln!("AnimSet name missing for TileDef: '{}' - '{}'", self.kind, self.name);
                        return false;
                    }
                }

                if anim_set.frames.is_empty() {
                    eprintln!("At least one animation frame is required! TileDef: '{}' - '{}'", self.kind, self.name);
                    return false;
                }

                for (frame_index, frame) in anim_set.frames.iter_mut().enumerate() {
                    if frame.name.is_empty() {
                        eprintln!("Missing sprite frame name for index [{}]. AnimSet: '{}', TileDef: '{}' - '{}'",
                                  frame_index,
                                  anim_set.name,
                                  self.kind,
                                  self.name);
                        return false;
                    }

                    // Path formats:
                    //  terrain/<category>/<tile>.png
                    //  buildings/<category>/<building_name>/<variation>/<anim_set>/<frame[N]>.png
                    //  units/<category>/<unit_name>/<anim_set>/<frame[N]>.png
                    let texture_path = match layer {
                        TileMapLayerKind::Terrain => {
                            format!("{}{}{}.png",
                                    tile_set_path_with_category,
                                    MAIN_SEPARATOR,
                                    frame.name)
                        },
                        TileMapLayerKind::Buildings => {
                            format!("{}{}{}{}{}{}{}{}{}.png",
                                    tile_set_path_with_category,
                                    MAIN_SEPARATOR,
                                    self.name,
                                    MAIN_SEPARATOR,
                                    variation.name,
                                    MAIN_SEPARATOR,
                                    anim_set.name,
                                    MAIN_SEPARATOR,
                                    frame.name)
                        },
                        TileMapLayerKind::Units => {
                            format!("{}{}{}{}{}{}{}.png",
                                    tile_set_path_with_category,
                                    MAIN_SEPARATOR,
                                    self.name,
                                    MAIN_SEPARATOR,
                                    anim_set.name,
                                    MAIN_SEPARATOR,
                                    frame.name)
                        },
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
const fn default_tile_size() -> Size { BASE_TILE_SIZE }

#[inline]
const fn default_tile_kind() -> TileKind { TileKind::Empty }

#[inline]
const fn default_occludes_terrain() -> bool { true }

// ----------------------------------------------
// TileCategory
// ----------------------------------------------

#[derive(Clone, Deserialize)]
pub struct TileCategory {
    pub name: String, // E.g.: ground, water, residential, etc...
    pub tiles: Vec<TileDef>,

    // Internal runtime index into TileSet.
    #[serde(skip)]
    tileset_category_index: i32,

    // Maps from tile name to TileDef index in self.tiles[].
    #[serde(skip)]
    mapping: PreHashedKeyMap<StringHash, usize>,
}

impl TileCategory {
    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }

    pub fn find_tile_by_name(&self, tile_name: &str) -> Option<&TileDef> {
        let tile_name_hash: StringHash = hash::fnv1a_from_str(tile_name);
        let entry_index = match self.mapping.get(&tile_name_hash) {
            Some(entry_index) => *entry_index,
            None => {
                eprintln!("TileCategory '{}': Couldn't find TileDef for '{}'.", self.name, tile_name);
                return None;
            }
        };
        Some(&self.tiles[entry_index])
    }

    pub fn find_tile_by_hash(&self, tile_name_hash: StringHash) -> Option<&TileDef> {
        let entry_index = match self.mapping.get(&tile_name_hash) {
            Some(entry_index) => *entry_index,
            None => {
                eprintln!("TileCategory '{}': Couldn't find TileDef for '{:#X}'.", self.name, tile_name_hash);
                return None;
            }
        };
        Some(&self.tiles[entry_index])
    }

    fn post_load(&mut self,
                 tex_cache: &mut TextureCache,
                 tile_set_path: &str,
                 layer: TileMapLayerKind) -> bool {

        debug_assert!(self.mapping.is_empty());

        if self.name.is_empty() {
            eprintln!("TileCategory name is missing! A name is required.");
            return false;
        }

        let tile_set_path_with_category =
            format!("{}{}{}", tile_set_path, MAIN_SEPARATOR, self.name);

        for (entry_index, tile_def) in self.tiles.iter_mut().enumerate() {
            tile_def.category_tile_index = entry_index as i32;
            tile_def.tileset_category_index = self.tileset_category_index;

            if !tile_def.post_load(tex_cache, &tile_set_path_with_category, layer) {
                return false;
            }

            let tile_name_hash: StringHash = hash::fnv1a_from_str(&tile_def.name);
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

#[derive(Clone, Deserialize)]
pub struct TileSet {
    // Layer, e.g.: Terrain, Building, Unit.
    // Also internal runtime index into TileSets.
    pub layer: TileMapLayerKind,
    pub categories: Vec<TileCategory>,

    // Maps from category name to TileCategory index in self.categories[].
    #[serde(skip)]
    mapping: PreHashedKeyMap<StringHash, usize>,
}

impl TileSet {
    const fn empty() -> Self {
        Self {
            // NOTE: Layer kind is irrelevant here.
            layer: TileMapLayerKind::Terrain,
            categories: Vec::new(),
            mapping: hash::new_const_hash_map(),
        }
    }

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

    fn post_load(&mut self, tex_cache: &mut TextureCache, tile_set_path: &str) -> bool {
        debug_assert!(self.mapping.is_empty());

        for (entry_index, category) in self.categories.iter_mut().enumerate() {
            category.tileset_category_index = entry_index as i32;

            if !category.post_load(tex_cache, tile_set_path, self.layer) {
                return false;
            }

            let category_name_hash: StringHash = hash::fnv1a_from_str(&category.name);
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
pub enum TileDefHandle {
    Empty,
    Blocker,
    // (tileset_index, tileset_category_index, category_tile_index)
    Indices(u16, u16, u16)
}

impl TileDefHandle {
    #[inline]
    pub fn new(tile_set: &TileSet, tile_category: &TileCategory, tile_def: &TileDef) -> Self {
        TileDefHandle::Indices(
            tile_set.layer as u16,
            tile_category.tileset_category_index.try_into().expect("Index cannot fit in a u16"),
            tile_def.category_tile_index.try_into().expect("Index cannot fit in a u16"),
        )
    }

    #[inline]
    pub const fn empty() -> Self {
        TileDefHandle::Empty
    }

    #[inline]
    pub const fn blocker() -> Self {
        TileDefHandle::Blocker
    }   
}

// ----------------------------------------------
// TileSets
// ----------------------------------------------

pub struct TileSets {
    sets: SmallVec<[TileSet; TILE_MAP_LAYER_COUNT]>,
}

impl TileSets {
    pub fn load(tex_cache: &mut TextureCache) -> Self {
        let mut tile_sets = Self {
            sets: smallvec![TileSet::empty(); TILE_MAP_LAYER_COUNT],
        };
        tile_sets.load_all_layers(tex_cache);
        tile_sets
    }

    pub fn is_empty(&self) -> bool {
        self.sets.is_empty()
    }

    #[inline]
    pub fn handle_to_tile(&self, handle: TileDefHandle) -> Option<&TileDef> {
        match handle {
            TileDefHandle::Empty   => Some(TileDef::empty()),
            TileDefHandle::Blocker => Some(TileDef::blocker()),
            TileDefHandle::Indices(tileset_index, tileset_category_index, category_tile_index) => {
                let set_idx  = tileset_index as usize;          // TileSet index into TileSets.
                let cat_idx  = tileset_category_index as usize; // TileCategory index into TileSet.
                let tile_idx = category_tile_index as usize;    // TileDef index into TileCategory.

                if set_idx >= self.sets.len() {
                    return None;
                }

                let set = &self.sets[set_idx];
                if cat_idx >= set.categories.len() {
                    return None;
                }

                let cat = &set.categories[cat_idx];
                if tile_idx >= cat.tiles.len() {
                    return None;
                }

                let tile_def = &cat.tiles[tile_idx];
                debug_assert!(set.layer as usize == set_idx);
                debug_assert!(cat.tileset_category_index as usize == cat_idx);
                debug_assert!(tile_def.tileset_category_index as usize == cat_idx);
                debug_assert!(tile_def.category_tile_index as usize == tile_idx);
                Some(tile_def)
            }
        }
    }

    pub fn find_category_for_tile(&self, tile_def: &TileDef) -> Option<&TileCategory> {
        if tile_def.is_empty() || tile_def.is_blocker() {
            return None;
        }

        let layer_idx = map::tile_kind_to_layer(tile_def.kind) as usize;
        let set_idx = tile_def.tileset_category_index as usize;
        let cat_idx = tile_def.category_tile_index as usize;

        let set = &self.sets[layer_idx];
        if set_idx >= set.categories.len() {
            return None;
        }

        let cat = &self.sets[layer_idx].categories[set_idx];
        if cat_idx >= cat.tiles.len() {
            return None;
        }

        debug_assert!(cat.tiles[cat_idx].category_tile_index == tile_def.category_tile_index);
        debug_assert!(cat.tiles[cat_idx].tileset_category_index == tile_def.tileset_category_index);
        Some(cat)
    }

    pub fn find_set_for_tile(&self, tile_def: &TileDef) -> Option<&TileSet> {
        let layer = map::tile_kind_to_layer(tile_def.kind); 
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

    pub fn find_tile_by_name(&self,
                             layer: TileMapLayerKind,
                             category_name: &str,
                             tile_name: &str) -> Option<&TileDef> {
        let cat = self.find_category_by_name(layer, category_name)?;
        cat.find_tile_by_name(tile_name)
    }

    pub fn find_tile_by_hash(&self,
                             layer: TileMapLayerKind,
                             category_name_hash: StringHash,
                             tile_name_hash: StringHash) -> Option<&TileDef> {
        let cat = self.find_category_by_hash(layer, category_name_hash)?;
        cat.find_tile_by_hash(tile_name_hash)
    }

    pub fn for_each_set<F>(&self, mut visitor_fn: F) where F: FnMut(&TileSet) -> bool {
        for set in &self.sets {
            let should_continue = visitor_fn(set);
            if !should_continue {
                return;
            }
        }
    }

    pub fn for_each_category<F>(&self, mut visitor_fn: F) where F: FnMut(&TileSet, &TileCategory) -> bool {
        for set in &self.sets {
            for cat in &set.categories {
                let should_continue = visitor_fn(set, cat);
                if !should_continue {
                    return;
                }
            }
        }
    }

    pub fn for_each_tile<F>(&self, mut visitor_fn: F) where F: FnMut(&TileSet, &TileCategory, &TileDef) -> bool {
        for set in &self.sets {
            for cat in &set.categories {
                for tile in &cat.tiles {
                    let should_continue = visitor_fn(set, cat, tile);
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
    pub fn try_get_editable_tile_debug(&self, tile_def: &TileDef) -> Option<&mut TileDef> {
        if let Some(cat) = self.find_category_for_tile(tile_def) {
            let const_def = &cat.tiles[tile_def.category_tile_index as usize];
            // SAFETY: Here there be Dragons!
            #[allow(invalid_reference_casting)]
            let mutable_def = unsafe {
                &mut *(const_def as *const TileDef as *mut TileDef)
            };
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
    // Buildings file structure:
    // -------------------------
    //  * Buildings have variations and animations.
    // Structure:
    //  buildings/tile_set.json
    //  buildings/<category>/<building_name>/<variation>/<anim_set>/<frame[N]>.png,*
    // Example:
    //  buildings/residential/house/var0/build
    //  buildings/residential/house/var0/fire
    // ...
    //  buildings/residential/house/var1/build
    //  buildings/residential/house/var1/fire
    // ...
    //  buildings/residential/house/var0/build/frame0.png
    //  buildings/residential/house/var0/build/frame1.png
    //  buildings/residential/house/var0/build/frame2.png
    //
    // Units file structure:
    // ---------------------
    //  * Units don’t have variations, only animations.
    //  * Several different walk directions.
    // Structure:
    //  units/tile_set.json
    //  units/<category>/<unit_name>/<anim_set>/<frame[N]>.png,*
    // Example:
    //  units/on_foot/ped/idle/frame0.png
    //  units/on_foot/ped/idle/frame1.png
    // ...
    //  units/on_foot/ped/walk_left/frame0.png
    //  units/on_foot/ped/walk_left/frame1.png
    //
    fn tile_set_path_for_kind(layer: TileMapLayerKind) -> &'static str {
        const TILE_SET_PATHS: [(TileMapLayerKind, &str); TILE_MAP_LAYER_COUNT] = [
            (TileMapLayerKind::Terrain,   "assets/tiles/terrain"),
            (TileMapLayerKind::Buildings, "assets/tiles/buildings"),
            (TileMapLayerKind::Units,     "assets/tiles/units")
        ];
        debug_assert!(TILE_SET_PATHS[layer as usize].0 == layer); // Ensure enum order.
        TILE_SET_PATHS[layer as usize].1
    }

    fn load_all_layers(&mut self, tex_cache: &mut TextureCache) {
        for layer in TileMapLayerKind::iter() {
            let tile_set_path = Self::tile_set_path_for_kind(layer);
            if !self.load_tile_set(tex_cache, tile_set_path, layer) {
                eprintln!("TileSet '{}' ({}) didn't load!", layer, tile_set_path);
            }
        }
    }

    fn load_tile_set(&mut self,
                     tex_cache: &mut TextureCache,
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

        println!("Successfully loaded TileSet '{}' from {:?}.", layer, tile_set_json_path);

        self.sets[layer as usize] = tile_set;
        true
    }
}
