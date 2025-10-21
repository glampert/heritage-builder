use std::path::{Path, MAIN_SEPARATOR_STR};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use strum::IntoEnumIterator;

use super::{
    atlas::*,
    TileFlags, TileKind, TileMapLayerKind, BASE_TILE_SIZE, TILE_MAP_LAYER_COUNT,
};
use crate::{
    log,
    pathfind::NodeKind as PathNodeKind,
    render::{TextureCache, TextureHandle},
    save::{self, SaveState},
    singleton_late_init,
    utils::{
        coords::{Cell, CellRange},
        hash::{self, PreHashedKeyMap, StrHashPair, StringHash},
        mem, Color, RectTexCoords, Size,
    },
};

// ----------------------------------------------
// Constants
// ----------------------------------------------

// Terrain Layer:
pub const TERRAIN_GROUND_CATEGORY: StrHashPair = StrHashPair::from_str("ground");
pub const TERRAIN_WATER_CATEGORY: StrHashPair = StrHashPair::from_str("water");

// Objects Layer:
pub const OBJECTS_BUILDINGS_CATEGORY: StrHashPair = StrHashPair::from_str("buildings");
pub const OBJECTS_PROPS_CATEGORY: StrHashPair = StrHashPair::from_str("props");
pub const OBJECTS_UNITS_CATEGORY: StrHashPair = StrHashPair::from_str("units");
pub const OBJECTS_VEGETATION_CATEGORY: StrHashPair = StrHashPair::from_str("vegetation");

// ----------------------------------------------
// TileTexInfo
// ----------------------------------------------

#[derive(Copy, Clone, Default)]
pub struct TileTexInfo {
    pub texture: TextureHandle,
    pub coords: RectTexCoords,
}

impl TileTexInfo {
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

    // Hash of `name`, computed post-load.
    #[serde(skip)]
    pub hash: StringHash,

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

    // Hash of `name`, computed post-load.
    #[serde(skip)]
    pub hash: StringHash,

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

    // Hash of `name`, computed post-load.
    #[serde(skip)]
    pub hash: StringHash,

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
    // Defaults to true. Ignored for Terrain.
    #[serde(default = "default_occludes_terrain")]
    pub occludes_terrain: bool,

    // If true, always select a random variation when placing this tile.
    #[serde(default)]
    pub randomize_placement: bool,

    #[serde(default = "default_path_kind")]
    pub path_kind: PathNodeKind,

    // Cost/price to place this tile in the world. Optional, can be zero.
    #[serde(default)]
    pub cost: u32,

    // Tile kind & archetype combined, also defines which layer the tile can be placed on.
    // Resolved post-load based on layer and category.
    #[serde(skip, default = "default_tile_kind")]
    kind: TileKind,

    // Internal runtime index into TileCategory.
    #[serde(skip)]
    category_tiledef_index: u32,

    // Internal runtime index into TileSet.
    #[serde(skip)]
    tileset_category_index: u32,
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
        flags.set(TileFlags::RandomizePlacement, self.randomize_placement);
        flags.set(TileFlags::SettlersSpawnPoint, self.path_kind.intersects(PathNodeKind::SettlersSpawnPoint));
        flags
    }

    #[inline]
    pub fn size_in_cells(&self) -> Size {
        // `logical_size` is assumed to be a multiple of the base tile size.
        Size::new(self.logical_size.width / BASE_TILE_SIZE.width,
                  self.logical_size.height / BASE_TILE_SIZE.height)
    }

    #[inline]
    pub fn occupies_multiple_cells(&self) -> bool {
        let size = self.size_in_cells();
        size.width > 1 || size.height > 1 // Multi-cell building?
    }

    #[inline]
    pub fn cell_range(&self, start_cell: Cell) -> CellRange {
        // Buildings can occupy multiple cells; Find which ones:
        let size = self.size_in_cells();
        let end_cell = Cell::new(start_cell.x + size.width - 1, start_cell.y + size.height - 1);
        CellRange::new(start_cell, end_cell)
    }

    #[inline]
    pub fn texture_by_index(&self,
                            variation_index: usize,
                            anim_set_index: usize,
                            frame_index: usize)
                            -> TileTexInfo {
        if let Some(frame) = self.anim_frame_by_index(variation_index, anim_set_index, frame_index) {
            return frame.tex_info;
        }
        TileTexInfo::default()
    }

    #[inline]
    pub fn anim_frame_by_index(&self,
                               variation_index: usize,
                               anim_set_index: usize,
                               frame_index: usize)
                               -> Option<&TileSprite> {
        if let Some(anim_set) = self.anim_set_by_index(variation_index, anim_set_index) {
            if frame_index < anim_set.frames.len() {
                return Some(&anim_set.frames[frame_index]);
            }
        }
        None
    }

    #[inline]
    pub fn anim_set_by_index(&self,
                             variation_index: usize,
                             anim_set_index: usize)
                             -> Option<&TileAnimSet> {
        if variation_index >= self.variations.len() {
            return None;
        }

        let variation = &self.variations[variation_index];
        if anim_set_index >= variation.anim_sets.len() {
            return None;
        }

        Some(&variation.anim_sets[anim_set_index])
    }

    #[inline]
    pub fn anim_sets_count(&self, variation_index: usize) -> usize {
        if variation_index >= self.variations.len() {
            return 0;
        }
        self.variations[variation_index].anim_sets.len()
    }

    #[inline]
    pub fn anim_frames_count(&self, variation_index: usize, anim_set_index: usize) -> usize {
        if variation_index >= self.variations.len() {
            return 0;
        }
        let variation = &self.variations[variation_index];
        if anim_set_index >= variation.anim_sets.len() {
            return 0;
        }
        variation.anim_sets[anim_set_index].frames.len()
    }

    #[inline]
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

    #[inline]
    pub fn variation_name(&self, variation_index: usize) -> &str {
        if variation_index >= self.variations.len() {
            return "";
        }
        &self.variations[variation_index].name
    }

    #[inline]
    pub fn has_variations(&self) -> bool {
        self.variations.len() > 1
    }

    fn post_load(&mut self,
                 tex_cache: &mut dyn TextureCache,
                 tex_atlas: &mut impl TextureAtlas,
                 tile_set_path_with_category: &str,
                 layer: TileMapLayerKind,
                 category_hash: StringHash)
                 -> bool {
        debug_assert!(self.hash != hash::NULL_HASH);
        debug_assert!(category_hash != hash::NULL_HASH);

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
            log::error!(log::channel!("tileset"),
                        "TileDef '{}' name is missing! A name is required.",
                        self.kind);
            return false;
        }

        if !self.logical_size.is_valid() {
            log::error!(log::channel!("tileset"),
                        "Invalid/missing TileDef logical size: '{}' - '{}'",
                        self.kind,
                        self.name);
            return false;
        }

        if (self.logical_size.width  % BASE_TILE_SIZE.width)  != 0 ||
           (self.logical_size.height % BASE_TILE_SIZE.height) != 0 {
            log::error!(log::channel!("tileset"),
                        "Invalid TileDef logical size ({})! Must be a multiple of BASE_TILE_SIZE: '{}' - '{}'",
                        self.logical_size,
                        self.kind,
                        self.name);
            return false;
        }

        if self.is(TileKind::Terrain) {
            // For terrain logical_size must be BASE_TILE_SIZE.
            if self.logical_size != BASE_TILE_SIZE {
                log::error!(log::channel!("tileset"),
                            "Terrain TileDef logical size must be equal to BASE_TILE_SIZE: '{}' - '{}'",
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
            log::error!(log::channel!("tileset"),
                        "At least one variation is required! TileDef: '{}' - '{}'",
                        self.kind,
                        self.name);
            return false;
        }

        // Validate deserialized data and resolve texture handles:
        for variation in &mut self.variations {
            variation.hash = hash::fnv1a_from_str(&variation.name);

            for anim_set in &mut variation.anim_sets {
                if anim_set.frames.is_empty() {
                    log::error!(log::channel!("tileset"),
                                "At least one animation frame is required! TileDef: '{}' - '{}'",
                                self.kind,
                                self.name);
                    return false;
                }

                anim_set.hash = hash::fnv1a_from_str(&anim_set.name);

                for (frame_index, frame) in anim_set.frames.iter_mut().enumerate() {
                    if frame.name.is_empty() {
                        log::error!(log::channel!("tileset"),
                                    "Missing sprite frame name for index [{frame_index}]. AnimSet: '{}', TileDef: '{}' - '{}'",
                                    anim_set.name,
                                    self.kind,
                                    self.name);
                        return false;
                    }

                    frame.hash = hash::fnv1a_from_str(&frame.name);

                    // Path format:
                    //  <layer>/<category>/<tile_name>/<variation>/<anim_set>/<frame[N]>.png
                    //
                    let texture_path = {
                        // <layer>/<category>/<tile_name>/
                        let mut path = format!("{}{}{}{}",
                                                       tile_set_path_with_category,
                                                       MAIN_SEPARATOR_STR,
                                                       self.name,
                                                       MAIN_SEPARATOR_STR);

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
                    };

                    frame.tex_info = tex_atlas.load_texture(tex_cache, &texture_path);
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
const fn default_tile_kind() -> TileKind {
    TileKind::empty()
}

#[inline]
const fn default_tile_size() -> Size {
    BASE_TILE_SIZE
}

#[inline]
const fn default_occludes_terrain() -> bool {
    true
}

#[inline]
const fn default_path_kind() -> PathNodeKind {
    PathNodeKind::empty()
}

// ----------------------------------------------
// EditableTileDef
// ----------------------------------------------

// This allows returning a mutable TileDef reference in
// try_get_editable_tile_def() for runtime editing purposes. We only require
// this functionality for debug and development.
type EditableTileDef = mem::Mutable<TileDef>;

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
    tileset_category_index: u32,

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
                log::error!(log::channel!("tileset"),
                            "TileCategory '{}': Couldn't find TileDef for '{}'.",
                            self.name,
                            tile_name);
                return None;
            }
        };
        Some(&self.tile_defs[entry_index])
    }

    pub fn find_tile_def_by_hash(&self, tile_name_hash: StringHash) -> Option<&TileDef> {
        let entry_index = match self.mapping.get(&tile_name_hash) {
            Some(entry_index) => *entry_index,
            None => {
                log::error!(log::channel!("tileset"),
                            "TileCategory '{}': Couldn't find TileDef for '{:#X}'.",
                            self.name,
                            tile_name_hash);
                return None;
            }
        };
        Some(&self.tile_defs[entry_index])
    }

    fn post_load(&mut self,
                 tex_cache: &mut dyn TextureCache,
                 tex_atlas: &mut impl TextureAtlas,
                 tile_set_path: &str,
                 layer: TileMapLayerKind)
                 -> bool {
        debug_assert!(self.mapping.is_empty());
        debug_assert!(self.hash != hash::NULL_HASH);

        if self.name.is_empty() {
            log::error!(log::channel!("tileset"),
                        "TileCategory name is missing! A name is required.");
            return false;
        }

        let tile_set_path_with_category =
            format!("{}{}{}", tile_set_path, MAIN_SEPARATOR_STR, self.name);

        for (index, editable_def) in self.tile_defs.iter_mut().enumerate() {
            let tile_def = editable_def.as_mut();

            if tile_def.name.is_empty() {
                log::error!(log::channel!("tileset"),
                            "TileCategory '{}': Invalid empty TileDef name! Index: [{index}]",
                            self.name);
                continue;
            }

            tile_def.category_tiledef_index = index as u32;
            tile_def.tileset_category_index = self.tileset_category_index;

            let tile_name_hash = hash::fnv1a_from_str(&tile_def.name);
            tile_def.hash = tile_name_hash;

            if !tile_def.post_load(tex_cache, tex_atlas, &tile_set_path_with_category, layer, self.hash) {
                continue;
            }

            if tile_def.kind.is_empty() {
                log::error!(log::channel!("tileset"),
                            "TileCategory '{}': Invalid empty TileDef kind! Index: [{index}]",
                            self.name);
                continue;
            }

            if self.mapping.insert(tile_name_hash, index).is_some() {
                log::error!(log::channel!("tileset"),
                            "TileCategory '{}': An entry for key '{}' ({:#X}) already exists at index: {index}!",
                            self.name,
                            tile_def.name,
                            tile_name_hash);
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
    fn new(layer: TileMapLayerKind) -> Self {
        Self { layer, categories: Vec::new(), mapping: PreHashedKeyMap::default() }
    }

    pub fn is_empty(&self) -> bool {
        self.categories.is_empty()
    }

    pub fn find_category_by_name(&self, category_name: &str) -> Option<&TileCategory> {
        let category_name_hash: StringHash = hash::fnv1a_from_str(category_name);
        let entry_index = match self.mapping.get(&category_name_hash) {
            Some(entry_index) => *entry_index,
            None => {
                log::error!(log::channel!("tileset"),
                            "TileSet '{}': Couldn't find TileCategory for '{}'.",
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
                log::error!(log::channel!("tileset"),
                            "TileSet '{}': Couldn't find TileCategory for '{:#X}'.",
                            self.layer,
                            category_name_hash);
                return None;
            }
        };
        Some(&self.categories[entry_index])
    }

    fn post_load(&mut self,
                 tex_cache: &mut dyn TextureCache,
                 tex_atlas: &mut impl TextureAtlas,
                 tile_set_path: &str)
                 -> bool {
        debug_assert!(self.mapping.is_empty());

        for (index, category) in self.categories.iter_mut().enumerate() {
            if category.name.is_empty() {
                log::error!(log::channel!("tileset"),
                            "TileSet '{}': Invalid empty category name! Index: [{index}]",
                            self.layer);
                continue;
            }

            category.tileset_category_index = index as u32;

            let category_name_hash = hash::fnv1a_from_str(&category.name);
            category.hash = category_name_hash;

            if !category.post_load(tex_cache, tex_atlas, tile_set_path, self.layer) {
                continue;
            }

            if self.mapping.insert(category_name_hash, index).is_some() {
                log::error!(log::channel!("tileset"),
                            "TileSet '{}': An entry for key '{}' ({:#X}) already exists at index: {index}!",
                            self.layer,
                            category.name,
                            category_name_hash);
            }
        }

        true
    }
}

// ----------------------------------------------
// TileDefHandle
// ----------------------------------------------

// (tileset_index, tileset_category_index, category_tiledef_index)
#[derive(Copy, Clone, Serialize, Deserialize)]
pub struct TileDefHandle(u16, u16, u16);

impl TileDefHandle {
    #[inline]
    pub fn new(tile_set: &TileSet, tile_category: &TileCategory, tile_def: &TileDef) -> Self {
        Self(tile_set.layer as u16,
             tile_category.tileset_category_index.try_into().expect("Index cannot fit in a u16"),
             tile_def.category_tiledef_index.try_into().expect("Index cannot fit in a u16"))
    }

    #[inline]
    pub fn from_tile_def(tile_def: &TileDef) -> Self {
        Self(tile_def.layer_kind() as u16,
             tile_def.tileset_category_index.try_into().expect("Index cannot fit in a u16"),
             tile_def.category_tiledef_index.try_into().expect("Index cannot fit in a u16"))
    }
}

// ----------------------------------------------
// TileSets
// ----------------------------------------------

pub struct TileSets {
    sets: [TileSet; TILE_MAP_LAYER_COUNT],
}

impl TileSets {
    pub fn load(tex_cache: &mut dyn TextureCache, use_packed_texture_atlas: bool) -> &'static Self {
        let mut instance = Self {
            sets: [
                TileSet::new(TileMapLayerKind::Terrain), // 0
                TileSet::new(TileMapLayerKind::Objects), // 1
            ],
        };
        instance.load_all_layers(tex_cache, use_packed_texture_atlas);
        TileSets::initialize(instance); // Set global instance.
        TileSets::get()
    }

    pub fn is_empty(&'static self) -> bool {
        self.sets.is_empty()
    }

    #[inline]
    pub fn handle_to_tile_def(&'static self, handle: TileDefHandle) -> Option<&'static TileDef> {
        let set_idx  = handle.0 as usize; // TileSet index into TileSets.
        let cat_idx  = handle.1 as usize; // TileCategory index into TileSet.
        let tile_idx = handle.2 as usize; // TileDef index into TileCategory.

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

    pub fn find_category_for_tile_def(&'static self,
                                      tile_def: &'static TileDef)
                                      -> Option<&'static TileCategory> {
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

    pub fn find_set_for_tile_def(&'static self,
                                 tile_def: &'static TileDef)
                                 -> Option<&'static TileSet> {
        let layer = tile_def.layer_kind();
        let set = &self.sets[layer as usize];
        debug_assert!(set.layer == layer);
        Some(set)
    }

    pub fn find_set_by_layer(&'static self, layer: TileMapLayerKind) -> Option<&'static TileSet> {
        let index = layer as usize;

        if index >= self.sets.len() {
            return None;
        }
        if self.sets[index].layer != layer {
            return None;
        }

        Some(&self.sets[index])
    }

    pub fn find_category_by_name(&'static self,
                                 layer: TileMapLayerKind,
                                 category_name: &str)
                                 -> Option<&'static TileCategory> {
        let set = self.find_set_by_layer(layer)?;
        set.find_category_by_name(category_name)
    }

    pub fn find_category_by_hash(&'static self,
                                 layer: TileMapLayerKind,
                                 category_name_hash: StringHash)
                                 -> Option<&'static TileCategory> {
        let set = self.find_set_by_layer(layer)?;
        set.find_category_by_hash(category_name_hash)
    }

    pub fn find_tile_def_by_name(&'static self,
                                 layer: TileMapLayerKind,
                                 category_name: &str,
                                 tile_name: &str)
                                 -> Option<&'static TileDef> {
        let cat = self.find_category_by_name(layer, category_name)?;
        cat.find_tile_def_by_name(tile_name)
    }

    pub fn find_tile_def_by_hash(&'static self,
                                 layer: TileMapLayerKind,
                                 category_name_hash: StringHash,
                                 tile_name_hash: StringHash)
                                 -> Option<&'static TileDef> {
        let cat = self.find_category_by_hash(layer, category_name_hash)?;
        cat.find_tile_def_by_hash(tile_name_hash)
    }

    pub fn for_each_set<F>(&'static self, mut visitor_fn: F)
        where F: FnMut(&'static TileSet) -> bool
    {
        for set in &self.sets {
            let should_continue = visitor_fn(set);
            if !should_continue {
                return;
            }
        }
    }

    pub fn for_each_category<F>(&'static self, mut visitor_fn: F)
        where F: FnMut(&'static TileSet, &'static TileCategory) -> bool
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

    pub fn for_each_tile_def<F>(&'static self, mut visitor_fn: F)
        where F: FnMut(&'static TileSet, &'static TileCategory, &'static TileDef) -> bool
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
    pub fn try_get_editable_tile_def(&'static self,
                                     tile_def: &'static TileDef)
                                     -> Option<&'static mut TileDef> {
        if let Some(cat) = self.find_category_for_tile_def(tile_def) {
            let editable_def = &cat.tile_defs[tile_def.category_tiledef_index as usize];
            // SAFETY: We're assuming that mutable access is sound here
            // (e.g., no overlapping accesses to the same TileDef elsewhere)
            let mutable_def = editable_def.as_mut();
            return Some(mutable_def);
        }
        None
    }

    // TileSet file structure:
    // -------------------------
    //  <layer>/tile_set.json
    //  <layer>/<category>/<tile_name>/<variation>/<anim_set>/<frame[N]>.png,*
    //
    // Examples:
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
    //  <layer>/<category>/<tile_name>/<anim_set>/<frame[N]>.png,*
    // Or:
    //  <layer>/<category>/<tile_name>/<variation>/<frame[N]>.png,*
    //
    // Examples:
    //  objects/units/ped/idle/frame0.png
    //  objects/units/ped/idle/frame1.png
    // ...
    //  objects/units/ped/walk_sw/frame0.png
    //  objects/units/ped/walk_sw/frame1.png
    //
    fn load_all_layers(&mut self, tex_cache: &mut dyn TextureCache, use_packed_texture_atlas: bool) {
        for layer in TileMapLayerKind::iter() {
            let tile_set_path = layer.assets_path();
            if !self.load_tile_set(tex_cache, tile_set_path, layer, use_packed_texture_atlas) {
                log::error!(log::channel!("tileset"),
                            "TileSet '{layer}' ({tile_set_path}) didn't load!");
            }
        }
    }

    fn load_tile_set(&mut self,
                     tex_cache: &mut dyn TextureCache,
                     tile_set_path: &str,
                     layer: TileMapLayerKind,
                     use_packed_texture_atlas: bool)
                     -> bool {
        debug_assert!(!tile_set_path.is_empty());

        let tile_set_json_path = Path::new(tile_set_path).join("tile_set.json");

        let mut state = save::backend::new_json_save_state(false);

        if let Err(err) = state.read_file(&tile_set_json_path) {
            log::error!(log::channel!("tileset"),
                        "Failed to read TileSet json file from path {tile_set_json_path:?}: {err}");
            return false;
        }

        let mut tile_set: TileSet = match state.load_new_instance() {
            Ok(tile_set) => tile_set,
            Err(err) => {
                log::error!(log::channel!("tileset"),
                            "Failed to deserialize TileSet layer '{layer}' from path {tile_set_json_path:?}: {err}");
                return false;
            }
        };

        if tile_set.layer != layer {
            log::error!(log::channel!("tileset"),
                        "TileSet layer kind mismatch! File specifies '{}' but expected '{layer}' for this set.",
                        tile_set.layer);
            return false;
        }

        if use_packed_texture_atlas {
            log::info!(log::channel!("tileset"), "Texture Atlas Packing: YES");
            let mut tex_atlas = PackedTextureAtlas::new(layer);

            if !tile_set.post_load(tex_cache, &mut tex_atlas, tile_set_path) {
                log::error!(log::channel!("tileset"), "Post load failed for TileSet '{layer}' - {tile_set_json_path:?}!");
                return false;
            }

            tex_atlas.commit_textures(tex_cache);
        } else {
            log::info!(log::channel!("tileset"), "Texture Atlas Packing: NO");
            let mut tex_atlas = PassthroughTextureAtlas::new();

            if !tile_set.post_load(tex_cache, &mut tex_atlas, tile_set_path) {
                log::error!(log::channel!("tileset"), "Post load failed for TileSet '{layer}' - {tile_set_json_path:?}!");
                return false;
            }
        }

        log::info!(log::channel!("tileset"),
                   "Successfully loaded TileSet '{layer}' from path {tile_set_json_path:?}.");

        self.sets[layer as usize] = tile_set;
        true
    }
}

// ----------------------------------------------
// TileSets Global Singleton
// ----------------------------------------------

singleton_late_init! { TILE_SETS_SINGLETON, TileSets }
