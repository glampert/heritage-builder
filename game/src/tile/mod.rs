#![allow(clippy::enum_variant_names)]

use rand::Rng;
use slab::Slab;
use arrayvec::ArrayVec;
use bitflags::bitflags;
use enum_dispatch::enum_dispatch;
use num_enum::TryFromPrimitive;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use strum::{EnumCount, EnumProperty, IntoEnumIterator};
use strum_macros::{Display, EnumCount, EnumIter, EnumProperty, VariantNames, VariantArray};
use std::{ops::{Index, IndexMut}, path::PathBuf};

pub use placement::PlacementOp;
use minimap::Minimap;
use selection::TileSelection;
use sets::{TileAnimSet, TileDef, SerializableTileDefHandle, TileSets, TileTexInfo};

use crate::{
    bitflags_with_display,
    engine::time::Seconds,
    pathfind::NodeKind as PathNodeKind,
    save::*,
    utils::{
        coords::{self, Cell, CellRange, IsoPoint, WorldToScreenTransform, IsoPointF32},
        platform::paths,
        hash::StringHash,
        mem, Color, Rect, Size, Vec2,
    },
};

pub mod camera;
pub mod minimap;
pub mod rendering;
pub mod selection;
pub mod sets;
pub mod road;
pub mod water;

mod atlas;
mod placement;

// ----------------------------------------------
// Constants / Enums
// ----------------------------------------------

pub const BASE_TILE_SIZE: Size = Size { width: 64, height: 32 };

#[repr(u8)]
#[derive(Copy, Clone, Default, VariantArray, VariantNames, Serialize, Deserialize)]
pub enum TileDepthSortOverride {
    #[default]
    None,
    Topmost,
    Bottommost,
}

pub type TileVariationIndex = u8;

// ----------------------------------------------
// TileKind
// ----------------------------------------------

bitflags_with_display! {
    #[derive(Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub struct TileKind: u8 {
        // Base Archetypes:
        const Terrain    = 1 << 0;
        const Object     = 1 << 1;
        const Blocker    = 1 << 2; // Draws nothing; blocker for multi-tile buildings, placed in the Objects layer.

        // Specialized tile kinds (Object Archetype & Objects Layer):
        const Building   = 1 << 3;
        const Unit       = 1 << 4;
        const Rocks      = 1 << 5;
        const Vegetation = 1 << 6;

        // Aliases:
        const Prop       = Self::Vegetation.bits(); // Only harvestable trees for now.
    }
}

impl TileKind {
    #[inline]
    fn specialized_kind_for_category(category_hash: StringHash) -> Self {
        if category_hash == sets::OBJECTS_BUILDINGS_CATEGORY.hash {
            TileKind::Building
        } else if category_hash == sets::OBJECTS_UNITS_CATEGORY.hash {
            TileKind::Unit
        } else if category_hash == sets::OBJECTS_ROCKS_CATEGORY.hash {
            TileKind::Rocks
        } else if category_hash == sets::OBJECTS_VEGETATION_CATEGORY.hash {
            TileKind::Vegetation
        } else {
            panic!("Unknown Tile Category hash!");
        }
    }
}

// ----------------------------------------------
// TileFlags
// ----------------------------------------------

bitflags_with_display! {
    #[derive(Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct TileFlags: u16 {
        const Hidden             = 1 << 0;
        const Highlighted        = 1 << 1;
        const Invalidated        = 1 << 2;
        const OccludesTerrain    = 1 << 3;
        const BuildingRoadLink   = 1 << 4;
        const SettlersSpawnPoint = 1 << 5;
        const DirtRoadPlacement  = 1 << 6;
        const PavedRoadPlacement = 1 << 7;
        const RandomizePlacement = 1 << 8;

        // Debug flags:
        const DrawDebugInfo      = 1 << 9;
        const DrawDebugBounds    = 1 << 10;
        const DrawBlockerInfo    = 1 << 11;
    }
}

// ----------------------------------------------
// TileGameObjectHandle
// ----------------------------------------------

// Index into associated GameObject.
#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub struct TileGameObjectHandle {
    // Index into SpawnPool.
    index: u32,
    // For buildings this holds the BuildingKind, for Units the generation count.
    kind_or_generation: u32,
}

impl TileGameObjectHandle {
    #[inline]
    pub fn new_building(index: usize, kind: u32) -> Self {
        // Reserved value for invalid.
        debug_assert!(index < u32::MAX as usize);
        debug_assert!(kind < u32::MAX);
        Self { index: index.try_into().expect("Index cannot fit into u32!"),
               kind_or_generation: kind }
    }

    #[inline]
    pub fn new_unit(index: usize, generation: u32) -> Self {
        // Reserved value for invalid.
        debug_assert!(index < u32::MAX as usize);
        debug_assert!(generation < u32::MAX);
        Self { index: index.try_into().expect("Index cannot fit into u32!"),
               kind_or_generation: generation }
    }

    #[inline]
    pub fn new_prop(index: usize, generation: u32) -> Self {
        // Reserved value for invalid.
        debug_assert!(index < u32::MAX as usize);
        debug_assert!(generation < u32::MAX);
        Self { index: index.try_into().expect("Index cannot fit into u32!"),
               kind_or_generation: generation }
    }

    #[inline]
    pub const fn invalid() -> Self {
        Self { index: u32::MAX, kind_or_generation: u32::MAX }
    }

    #[inline]
    pub fn is_valid(self) -> bool {
        self.index < u32::MAX && self.kind_or_generation < u32::MAX
    }

    #[inline]
    pub fn index(self) -> usize {
        debug_assert!(self.index < u32::MAX);
        self.index as usize
    }

    #[inline]
    pub fn kind(self) -> u32 {
        debug_assert!(self.kind_or_generation < u32::MAX);
        self.kind_or_generation
    }

    #[inline]
    pub fn generation(self) -> u32 {
        debug_assert!(self.kind_or_generation < u32::MAX);
        self.kind_or_generation
    }
}

impl Default for TileGameObjectHandle {
    #[inline]
    fn default() -> Self {
        TileGameObjectHandle::invalid()
    }
}

// ----------------------------------------------
// TileAnimState
// ----------------------------------------------

#[derive(Copy, Clone, Default, Serialize, Deserialize)]
struct TileAnimState {
    anim_set_index: u16,
    frame_index: u16,
    frame_play_time_secs: Seconds,
}

impl TileAnimState {
    const DEFAULT: Self = Self { anim_set_index: 0, frame_index: 0, frame_play_time_secs: 0.0 };
}

// ----------------------------------------------
// TileMapLayerPtr
// ----------------------------------------------

#[derive(Copy, Clone, Default)]
struct TileMapLayerPtr {
    opt_ptr: Option<mem::RawPtr<TileMapLayer>>,
}

impl TileMapLayerPtr {
    #[inline]
    fn new(layer: &TileMapLayer) -> Self {
        Self { opt_ptr: Some(mem::RawPtr::from_ref(layer)) }
    }

    #[inline]
    fn as_ref(&self) -> &TileMapLayer {
        self.opt_ptr.as_ref().expect("TileMapLayer reference is unset! Missing post_load()?")
    }

    #[inline]
    fn as_mut(&mut self) -> &mut TileMapLayer {
        self.opt_ptr.as_mut().expect("TileMapLayer reference is unset! Missing post_load()?")
    }
}

// ----------------------------------------------
// TileDefRef
// ----------------------------------------------

#[derive(Copy, Clone)]
enum TileDefRef {
    Ref(&'static TileDef),
    Handle(SerializableTileDefHandle),
}

impl TileDefRef {
    #[inline]
    fn new(def: &'static TileDef) -> Self {
        Self::Ref(def)
    }

    #[inline]
    fn as_ref(&self) -> &'static TileDef {
        match self {
            Self::Ref(def) => def,
            _ => panic!("TileDefRef does not hold a TileDef reference! Check deserialization/post_load()..."),
        }
    }

    #[inline]
    fn as_handle(&self) -> SerializableTileDefHandle {
        match self {
            Self::Handle(handle) => *handle,
            _ => panic!("TileDefRef does not hold a SerializableTileDefHandle! Check deserialization/post_load()..."),
        }
    }

    #[inline]
    fn post_load(&mut self) {
        let handle = self.as_handle();
        let def = TileSets::get().serializable_handle_to_tile_def(handle)
                                 .expect("Invalid SerializableTileDefHandle! Check serialization code...");
        *self = Self::Ref(def);
    }
}

impl Serialize for TileDefRef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where S: Serializer
    {
        let handle: SerializableTileDefHandle = match self {
            Self::Ref(def) => SerializableTileDefHandle::from_tile_def(def),
            Self::Handle(handle) => *handle,
        };

        // Serialize as handle and fix-up back to reference on post_load().
        handle.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for TileDefRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: Deserializer<'de>
    {
        // Always deserialize as handle. post_load() converts back to reference.
        let handle = SerializableTileDefHandle::deserialize(deserializer)?;
        Ok(TileDefRef::Handle(handle))
    }
}

// ----------------------------------------------
// Tile / TileArchetype
// ----------------------------------------------

// Tile is tied to the lifetime of the TileSets that owns the underlying
// TileDef. We also may keep a reference to the owning TileMapLayer inside
// TileArchetype for building blockers and objects.
#[derive(Serialize, Deserialize)]
pub struct Tile {
    kind: TileKind,
    flags: TileFlags,
    variation_index: TileVariationIndex,
    depth_sort_override: TileDepthSortOverride,
    self_index: TilePoolIndex,
    next_index: TilePoolIndex,
    archetype: TileArchetype,
}

#[enum_dispatch]
#[derive(Serialize, Deserialize)]
enum TileArchetype {
    TerrainTile(TerrainTile),
    ObjectTile(ObjectTile),
    BlockerTile(BlockerTile),
}

// ----------------------------------------------
// TileBehavior
// ----------------------------------------------

// Common behavior for all Tile archetypes.
#[enum_dispatch(TileArchetype)]
trait TileBehavior {
    fn post_load(&mut self, layer: TileMapLayerPtr);

    fn set_flags(&mut self, current_flags: &mut TileFlags, new_flags: TileFlags, value: bool);
    fn set_base_cell(&mut self, cell: Cell);
    fn set_iso_coords_f32(&mut self, iso_coords: IsoPointF32);
    fn set_variation_index(&mut self, index: usize);

    fn game_object_handle(&self) -> TileGameObjectHandle;
    fn set_game_object_handle(&mut self, handle: TileGameObjectHandle);

    fn iso_coords_f32(&self) -> IsoPointF32;
    fn actual_base_cell(&self) -> Cell;
    fn cell_range(&self) -> CellRange;

    fn tile_def(&self) -> &'static TileDef;
    fn is_valid(&self) -> bool;

    // Animations:
    fn anim_state(&self) -> &TileAnimState;
    fn anim_state_mut(&mut self) -> &mut TileAnimState;
}

// ----------------------------------------------
// TerrainTile
// ----------------------------------------------

// NOTES:
//  - Terrain tiles do not store game object handles.
//  - Terrain tile are always 1x1.
//  - Terrain tile logical size is fixed (BASE_TILE_SIZE).
//  - Terrain tile draw size can be customized.
//  - No variations or animations.
//
#[derive(Copy, Clone, Serialize, Deserialize)]
struct TerrainTile {
    def: TileDefRef,

    // Terrain tiles always occupy a single cell (of BASE_TILE_SIZE size).
    cell: Cell,

    // Cached on construction.
    iso_coords_f32: IsoPointF32,
}

impl TerrainTile {
    fn new(cell: Cell, tile_def: &'static TileDef) -> Self {
        Self { def: TileDefRef::new(tile_def),
               cell,
               iso_coords_f32: IsoPointF32::from_integer_iso(coords::cell_to_iso(cell, BASE_TILE_SIZE)) }
    }
}

impl TileBehavior for TerrainTile {
    #[inline]
    fn post_load(&mut self, _layer: TileMapLayerPtr) {
        debug_assert!(self.cell.is_valid());
        self.def.post_load();
    }

    #[inline]
    fn set_flags(&mut self, current_flags: &mut TileFlags, new_flags: TileFlags, value: bool) {
        current_flags.set(new_flags, value);
    }

    #[inline]
    fn set_base_cell(&mut self, cell: Cell) {
        self.cell = cell;
        self.iso_coords_f32 = IsoPointF32::from_integer_iso(coords::cell_to_iso(cell, BASE_TILE_SIZE));
    }

    #[inline]
    fn set_iso_coords_f32(&mut self, iso_coords: IsoPointF32) {
        self.iso_coords_f32 = iso_coords;
    }

    #[inline]
    fn set_variation_index(&mut self, _index: usize) {}

    #[inline]
    fn game_object_handle(&self) -> TileGameObjectHandle {
        TileGameObjectHandle::invalid()
    }

    #[inline]
    fn set_game_object_handle(&mut self, _handle: TileGameObjectHandle) {}

    #[inline]
    fn iso_coords_f32(&self) -> IsoPointF32 {
        self.iso_coords_f32
    }

    #[inline]
    fn actual_base_cell(&self) -> Cell {
        self.cell
    }

    #[inline]
    fn cell_range(&self) -> CellRange {
        CellRange::new(self.cell, self.cell)
    }

    #[inline]
    fn tile_def(&self) -> &'static TileDef {
        self.def.as_ref()
    }

    #[inline]
    fn is_valid(&self) -> bool {
        self.cell.is_valid() && self.def.as_ref().is_valid()
    }

    // No support for animations on Terrain.
    #[inline]
    fn anim_state(&self) -> &TileAnimState {
        // Return a valid dummy value for Tile::anim_set_index(),
        // Tile::anim_frame_index(), etc that has all fields set to defaults.
        &TileAnimState::DEFAULT
    }

    #[inline]
    fn anim_state_mut(&mut self) -> &mut TileAnimState {
        // This is method is only called from Tile::update_anim() and
        // Tile::set_anim_set_index(), so should never be used for Terrain.
        unimplemented!("Terrain Tiles are not animated! Do not call this on a Terrain Tile.");
    }
}

// ----------------------------------------------
// ObjectTile
// ----------------------------------------------

#[derive(Copy, Clone, Serialize, Deserialize)]
struct ObjectTile {
    def: TileDefRef,

    // Owning layer so we can propagate flags from a building to all of its blocker tiles.
    // SAFETY: This ref will always be valid as long as the Tile instance is, since the Tile
    // belongs to its parent layer.
    #[serde(skip)]
    layer: TileMapLayerPtr,

    // Buildings can occupy multiple cells. `cell_range.start` is the start or "base" cell.
    cell_range: CellRange,
    game_object_handle: TileGameObjectHandle,
    anim_state: TileAnimState,

    // Cached on construction.
    iso_coords_f32: IsoPointF32,
}

impl ObjectTile {
    fn new(cell: Cell, tile_def: &'static TileDef, layer: &TileMapLayer) -> Self {
        Self { def: TileDefRef::new(tile_def),
               layer: TileMapLayerPtr::new(layer),
               cell_range: tile_def.cell_range(cell),
               game_object_handle: TileGameObjectHandle::default(),
               anim_state: TileAnimState::default(),
               iso_coords_f32: calc_object_iso_coords(tile_def.kind(),
                                                      cell,
                                                      tile_def.logical_size,
                                                      tile_def.draw_size) }
    }
}

impl TileBehavior for ObjectTile {
    #[inline]
    fn post_load(&mut self, layer: TileMapLayerPtr) {
        debug_assert!(self.cell_range.is_valid());
        self.def.post_load();
        self.layer = layer;
    }

    #[inline]
    fn set_flags(&mut self, _current_flags: &mut TileFlags, new_flags: TileFlags, value: bool) {
        let layer = self.layer.as_mut();

        // Propagate flags to any child blockers in its cell range (including self).
        for cell in &self.cell_range {
            let next_tile_index = {
                let tile = layer.tile_mut(cell);
                tile.flags.set(new_flags, value);
                tile.next_index
            };

            layer.visit_next_tiles_mut(next_tile_index, |next_tile| {
                     next_tile.flags.set(new_flags, value)
                 });
        }
    }

    #[inline]
    fn set_base_cell(&mut self, cell: Cell) {
        let def = self.def.as_ref();
        self.cell_range = def.cell_range(cell);
        self.iso_coords_f32 =
            calc_object_iso_coords(def.kind(), cell, def.logical_size, def.draw_size);
    }

    #[inline]
    fn set_iso_coords_f32(&mut self, iso_coords: IsoPointF32) {
        self.iso_coords_f32 = iso_coords;
    }

    #[inline]
    fn set_variation_index(&mut self, _index: usize) {}

    #[inline]
    fn game_object_handle(&self) -> TileGameObjectHandle {
        self.game_object_handle
    }

    #[inline]
    fn set_game_object_handle(&mut self, handle: TileGameObjectHandle) {
        self.game_object_handle = handle;
    }

    #[inline]
    fn iso_coords_f32(&self) -> IsoPointF32 {
        self.iso_coords_f32
    }

    #[inline]
    fn actual_base_cell(&self) -> Cell {
        self.cell_range.start
    }

    #[inline]
    fn cell_range(&self) -> CellRange {
        self.cell_range
    }

    #[inline]
    fn tile_def(&self) -> &'static TileDef {
        self.def.as_ref()
    }

    #[inline]
    fn is_valid(&self) -> bool {
        self.cell_range.is_valid() && self.def.as_ref().is_valid()
    }

    // Animations:
    #[inline]
    fn anim_state(&self) -> &TileAnimState {
        &self.anim_state
    }

    #[inline]
    fn anim_state_mut(&mut self) -> &mut TileAnimState {
        &mut self.anim_state
    }
}

#[inline]
pub fn calc_unit_iso_coords(base_cell: Cell, draw_size: Size) -> IsoPointF32 {
    calc_object_iso_coords(TileKind::Unit, base_cell, BASE_TILE_SIZE, draw_size)
}

#[inline]
pub fn calc_object_iso_coords(kind: TileKind,
                              base_cell: Cell,
                              logical_size: Size,
                              draw_size: Size)
                              -> IsoPointF32 {
    // Convert the anchor (bottom tile for buildings) to isometric coordinates:
    let mut tile_iso_coords = coords::cell_to_iso(base_cell, BASE_TILE_SIZE);

    if kind.intersects(TileKind::Building) {
        // Center the sprite horizontally:
        tile_iso_coords.x += (BASE_TILE_SIZE.width / 2) - (logical_size.width / 2);

        // Vertical offset: move up the full sprite height *minus* 1 tile's height.
        // Since the anchor is the bottom tile, and cell_to_iso gives us the *bottom*,
        // we must offset up by (image_height - one_tile_height).
        tile_iso_coords.y -= draw_size.height - BASE_TILE_SIZE.height;
    } else if kind.intersects(TileKind::Unit) {
        // Adjust to center the unit sprite to the tile:
        tile_iso_coords.x += (BASE_TILE_SIZE.width / 2) - (draw_size.width / 2);
        tile_iso_coords.y -= draw_size.height - (BASE_TILE_SIZE.height / 2);
    } else if kind.intersects(TileKind::Rocks | TileKind::Vegetation) {
        // Adjust to center the prop sprite to the tile:
        tile_iso_coords.x += (BASE_TILE_SIZE.width / 2) - (draw_size.width / 2);
        tile_iso_coords.y -= draw_size.height - (BASE_TILE_SIZE.height / 2) - (BASE_TILE_SIZE.height / 4);
    }

    IsoPointF32::from_integer_iso(tile_iso_coords)
}

// ----------------------------------------------
// BlockerTile
// ----------------------------------------------

// Buildings have an origin tile and zero or more associated Blocker Tiles
// if they occupy multiple tiles.
//
// For instance, a 2x2 house tile `H` will have the house at its origin
// cell, and 3 other blocker tiles `B` that backreference the house tile.
//  +---+---+
//  | B | B |
//  +---+---+
//  | B | H | <-- origin tile, AKA base tile
//  +---+---+
//
#[derive(Copy, Clone, Serialize, Deserialize)]
struct BlockerTile {
    // Weak reference to owning map layer so we can seamlessly resolve blockers into buildings.
    // SAFETY: This ref will always be valid as long as the Tile instance is, since the Tile
    // belongs to its parent layer.
    #[serde(skip)]
    layer: TileMapLayerPtr,

    // Building blocker tiles occupy a single cell and have a backreference to the owner start
    // cell. `owner_cell` must be always valid.
    cell: Cell,
    owner_cell: Cell,
}

impl BlockerTile {
    fn new(blocker_cell: Cell, owner_cell: Cell, layer: &TileMapLayer) -> Self {
        Self { layer: TileMapLayerPtr::new(layer), cell: blocker_cell, owner_cell }
    }

    #[inline]
    fn owner(&self) -> &Tile {
        let layer = self.layer.as_ref();
        layer.find_blocker_owner(self.owner_cell)
    }

    #[inline]
    fn owner_mut(&mut self) -> &mut Tile {
        let layer = self.layer.as_mut();
        layer.find_blocker_owner_mut(self.owner_cell)
    }
}

impl TileBehavior for BlockerTile {
    #[inline]
    fn post_load(&mut self, layer: TileMapLayerPtr) {
        debug_assert!(self.cell.is_valid());
        debug_assert!(self.owner_cell.is_valid());
        self.layer = layer;
    }

    #[inline]
    fn set_flags(&mut self, _current_flags: &mut TileFlags, new_flags: TileFlags, value: bool) {
        // Propagate back to owner tile:
        self.owner_mut().set_flags(new_flags, value);
    }

    #[inline]
    fn set_base_cell(&mut self, _cell: Cell) {
        unimplemented!("Not implemented for BlockerTile!");
    }

    #[inline]
    fn set_iso_coords_f32(&mut self, _iso_coords: IsoPointF32) {
        unimplemented!("Not implemented for BlockerTile!");
    }

    #[inline]
    fn set_variation_index(&mut self, index: usize) {
        // Propagate back to owner tile:
        self.owner_mut().set_variation_index(index);
    }

    #[inline]
    fn game_object_handle(&self) -> TileGameObjectHandle {
        self.owner().game_object_handle()
    }

    #[inline]
    fn set_game_object_handle(&mut self, handle: TileGameObjectHandle) {
        self.owner_mut().set_game_object_handle(handle);
    }

    #[inline]
    fn iso_coords_f32(&self) -> IsoPointF32 {
        self.owner().iso_coords_f32()
    }

    #[inline]
    fn actual_base_cell(&self) -> Cell {
        self.cell
    }

    #[inline]
    fn cell_range(&self) -> CellRange {
        self.owner().cell_range()
    }

    #[inline]
    fn tile_def(&self) -> &'static TileDef {
        self.owner().tile_def()
    }

    #[inline]
    fn is_valid(&self) -> bool {
        self.cell.is_valid() && self.owner_cell.is_valid() && self.owner().is_valid()
    }

    // Animations:
    #[inline]
    fn anim_state(&self) -> &TileAnimState {
        self.owner().anim_state()
    }

    #[inline]
    fn anim_state_mut(&mut self) -> &mut TileAnimState {
        self.owner_mut().anim_state_mut()
    }
}

// ----------------------------------------------
// Tile impl
// ----------------------------------------------

impl Tile {
    fn new(cell: Cell,
           index: TilePoolIndex,
           tile_def: &'static TileDef,
           layer: &TileMapLayer)
           -> Self {
        let archetype = match layer.kind() {
            TileMapLayerKind::Terrain => {
                debug_assert!(tile_def.kind() == TileKind::Terrain); // Only Terrain.
                let terrain = TerrainTile::new(cell, tile_def);
                TileArchetype::from(terrain)
            }
            TileMapLayerKind::Objects => {
                debug_assert!(tile_def.kind().intersects(TileKind::Object)); // Object | Building, Prop, etc...
                let object = ObjectTile::new(cell, tile_def, layer);
                TileArchetype::from(object)
            }
        };

        Self { kind: tile_def.kind(),
               flags: tile_def.flags(),
               variation_index: 0,
               depth_sort_override: TileDepthSortOverride::default(),
               self_index: index,
               next_index: TilePoolIndex::invalid(),
               archetype }
    }

    fn new_blocker(blocker_cell: Cell,
                   index: TilePoolIndex,
                   owner_cell: Cell,
                   owner_kind: TileKind,
                   owner_flags: TileFlags,
                   layer: &TileMapLayer)
                   -> Self {
        debug_assert!(owner_kind == TileKind::Object | TileKind::Building);
        Self { kind: TileKind::Object | TileKind::Blocker,
               flags: owner_flags,
               variation_index: 0, // unused
               depth_sort_override: TileDepthSortOverride::default(), // unused
               self_index: index,
               next_index: TilePoolIndex::invalid(),
               archetype: TileArchetype::from(BlockerTile::new(blocker_cell, owner_cell, layer)) }
    }

    #[inline]
    pub fn index(&self) -> TilePoolIndex {
        self.self_index
    }

    #[inline]
    pub fn set_flags(&mut self, new_flags: TileFlags, value: bool) {
        self.archetype.set_flags(&mut self.flags, new_flags, value);
        debug_assert!(self.has_flags(new_flags) == value);
    }

    #[inline]
    pub fn has_flags(&self, flags: TileFlags) -> bool {
        self.flags.intersects(flags)
    }

    #[inline]
    pub fn flags(&self) -> TileFlags {
        self.flags
    }

    #[inline]
    pub fn is(&self, kinds: TileKind) -> bool {
        self.kind.intersects(kinds)
    }

    #[inline]
    pub fn is_valid(&self) -> bool {
        !self.kind.is_empty() && self.archetype.is_valid() && self.self_index.is_valid()
    }

    #[inline]
    pub fn kind(&self) -> TileKind {
        self.kind
    }

    #[inline]
    pub fn layer_kind(&self) -> TileMapLayerKind {
        TileMapLayerKind::from_tile_kind(self.kind)
    }

    #[inline]
    pub fn game_object_handle(&self) -> TileGameObjectHandle {
        self.archetype.game_object_handle()
    }

    #[inline]
    pub fn set_game_object_handle(&mut self, handle: TileGameObjectHandle) {
        self.archetype.set_game_object_handle(handle);
    }

    #[inline]
    pub fn tile_def(&self) -> &'static TileDef {
        self.archetype.tile_def()
    }

    #[inline]
    pub fn path_kind(&self) -> PathNodeKind {
        self.archetype.tile_def().path_kind
    }

    #[inline]
    pub fn name(&self) -> &'static str {
        &self.tile_def().name
    }

    #[inline]
    pub fn logical_size(&self) -> Size {
        self.tile_def().logical_size
    }

    #[inline]
    pub fn draw_size(&self) -> Size {
        self.tile_def().draw_size
    }

    #[inline]
    pub fn size_in_cells(&self) -> Size {
        self.tile_def().size_in_cells()
    }

    #[inline]
    pub fn tint_color(&self) -> Color {
        self.tile_def().color
    }

    #[inline]
    pub fn occupies_multiple_cells(&self) -> bool {
        self.tile_def().occupies_multiple_cells()
    }

    #[inline]
    pub fn depth_sort_override(&self) -> TileDepthSortOverride {
        self.depth_sort_override
    }

    #[inline]
    pub fn set_depth_sort_override(&mut self, depth_sort_override: TileDepthSortOverride) {
        self.depth_sort_override = depth_sort_override;
    }

    #[inline]
    pub fn iso_coords(&self) -> IsoPoint {
        let coords_f32 = self.iso_coords_f32();
        coords_f32.to_integer_iso()
    }

    #[inline]
    pub fn iso_coords_f32(&self) -> IsoPointF32 {
        self.archetype.iso_coords_f32()
    }

    #[inline]
    pub fn set_iso_coords(&mut self, iso_coords: IsoPoint) {
        let coords_f32 = IsoPointF32::from_integer_iso(iso_coords);
        self.set_iso_coords_f32(coords_f32);
    }

    #[inline]
    pub fn set_iso_coords_f32(&mut self, iso_coords: IsoPointF32) {
        // Native internal format is f32.
        self.archetype.set_iso_coords_f32(iso_coords);
    }

    #[inline]
    pub fn screen_rect(&self, transform: WorldToScreenTransform, apply_variation_offset: bool) -> Rect {
        let draw_size = self.draw_size();
        let mut iso_position = self.iso_coords_f32();
        if apply_variation_offset {
            iso_position.0 += self.variation_offset();
        }
        coords::iso_to_screen_rect_f32(iso_position, draw_size, transform)
    }

    #[inline]
    pub fn is_stacked(&self) -> bool {
        self.next_index != INVALID_TILE_INDEX
    }

    // Base cell without resolving blocker tiles into their owner cell.
    #[inline]
    pub fn actual_base_cell(&self) -> Cell {
        self.archetype.actual_base_cell()
    }

    #[inline]
    pub fn base_cell(&self) -> Cell {
        self.cell_range().start
    }

    #[inline]
    pub fn cell_range(&self) -> CellRange {
        self.archetype.cell_range()
    }

    #[inline]
    pub fn for_each_cell<F>(&self, reverse: bool, mut visitor_fn: F)
        where F: FnMut(Cell)
    {
        if reverse {
            for cell in self.cell_range().iter_rev() {
                visitor_fn(cell);
            }
        } else {
            for cell in &self.cell_range() {
                visitor_fn(cell);
            }
        }
    }

    pub fn is_screen_point_inside_base_cell(&self,
                                            screen_point: Vec2,
                                            transform: WorldToScreenTransform)
                                            -> bool {
        let cell = self.actual_base_cell();
        let tile_size = self.logical_size();

        coords::is_screen_point_inside_cell(screen_point,
                                            cell,
                                            tile_size,
                                            BASE_TILE_SIZE,
                                            transform)
    }

    pub fn category_name(&self) -> &'static str {
        TileSets::get().find_category_for_tile_def(self.tile_def())
                       .map_or("<none>", |cat| &cat.name)
    }

    pub fn try_get_editable_tile_def(&self) -> Option<&'static mut TileDef> {
        TileSets::get().try_get_editable_tile_def(self.tile_def())
    }

    // ----------------------
    // Variations:
    // ----------------------

    #[inline]
    pub fn has_variations(&self) -> bool {
        self.tile_def().has_variations()
    }

    #[inline]
    pub fn variation_count(&self) -> usize {
        self.tile_def().variations.len()
    }

    #[inline]
    pub fn variation_name(&self) -> &'static str {
        self.tile_def().variation_name(self.variation_index())
    }

    #[inline]
    pub fn variation_offset(&self) -> Vec2 {
        let tile_def = self.tile_def();
        let variation_index = self.variation_index();
        if variation_index < tile_def.variations.len() {
            let variation = &tile_def.variations[variation_index];
            variation.iso_offset
        } else {
            Vec2::zero()
        }
    }

    #[inline]
    pub fn variation_index(&self) -> usize {
        self.variation_index.into()
    }

    #[inline]
    pub fn set_variation_index(&mut self, index: usize) {
        self.variation_index = index.min(self.variation_count() - 1)
            .try_into()
            .expect("Value cannot fit into a TileVariationIndex!");

        // Propagate to owner tile in case this is a blocker.
        self.archetype.set_variation_index(self.variation_index());
    }

    #[inline]
    pub fn set_random_variation_index<R: Rng>(&mut self, rng: &mut R) {
        let variation_count = self.variation_count();
        if variation_count > 1 {
            let rand_variation_index = rng.random_range(0..variation_count);
            self.set_variation_index(rand_variation_index);
        }
    }

    // ----------------------
    // Animations:
    // ----------------------

    #[inline]
    pub fn has_animations(&self) -> bool {
        let anim_set_index = self.anim_set_index();
        let variation_index = self.variation_index();

        if let Some(anim_set) = self.tile_def().anim_set_by_index(variation_index, anim_set_index) {
            if anim_set.frames.len() > 1 {
                return true;
            }
        }

        false
    }

    #[inline]
    pub fn anim_sets_count(&self) -> usize {
        self.tile_def().anim_sets_count(self.variation_index())
    }

    #[inline]
    pub fn anim_set_name(&self) -> &'static str {
        self.tile_def().anim_set_name(self.variation_index(), self.anim_set_index())
    }

    #[inline]
    pub fn anim_set(&self) -> &TileAnimSet {
        self.tile_def().anim_set_by_index(self.variation_index(), self.anim_set_index()).unwrap()
    }

    #[inline]
    pub fn anim_frames_count(&self) -> usize {
        self.tile_def().anim_frames_count(self.variation_index(), self.anim_set_index())
    }

    #[inline]
    pub fn set_anim_set_index(&mut self, index: usize) {
        let max_index = self.anim_sets_count() - 1;
        let anim_state = self.anim_state_mut();
        let new_anim_set_index: u16 =
            index.min(max_index).try_into().expect("Anim Set index must be <= u16::MAX!");
        if new_anim_set_index != anim_state.anim_set_index {
            anim_state.anim_set_index = new_anim_set_index;
            anim_state.frame_index = 0;
            anim_state.frame_play_time_secs = 0.0;
        }
    }

    #[inline]
    pub fn anim_set_index(&self) -> usize {
        self.anim_state().anim_set_index as usize
    }

    #[inline]
    pub fn anim_frame_index(&self) -> usize {
        self.anim_state().frame_index as usize
    }

    #[inline]
    pub fn anim_frame_play_time_secs(&self) -> f32 {
        self.anim_state().frame_play_time_secs
    }

    #[inline]
    pub fn anim_frame_tex_info(&self) -> Option<&'static TileTexInfo> {
        let anim_set_index = self.anim_set_index();
        let variation_index = self.variation_index();

        if let Some(anim_set) = self.tile_def().anim_set_by_index(variation_index, anim_set_index) {
            let anim_frame_index = self.anim_frame_index();
            if anim_frame_index < anim_set.frames.len() {
                return Some(&anim_set.frames[anim_frame_index].tex_info);
            }
        }

        None
    }

    #[inline]
    fn update_anim(&mut self, delta_time_secs: Seconds) {
        if !self.is_animated_archetype() {
            return; // Not animated.
        }

        let def = self.tile_def();
        let anim_set_index = self.anim_set_index();
        let variation_index = self.variation_index();

        if let Some(anim_set) = def.anim_set_by_index(variation_index, anim_set_index) {
            if anim_set.frames.len() <= 1 {
                // Single frame sprite, nothing to update.
                return;
            }

            let anim_state = self.anim_state_mut();
            anim_state.frame_play_time_secs += delta_time_secs;

            if anim_state.frame_play_time_secs >= anim_set.frame_duration_secs() {
                if (anim_state.frame_index as usize) < anim_set.frames.len() - 1 {
                    // Move to next frame.
                    anim_state.frame_index += 1;
                } else {
                    // Played the whole anim.
                    if anim_set.looping {
                        anim_state.frame_index = 0;
                    }
                }
                // Reset the clock.
                anim_state.frame_play_time_secs = 0.0;
            }
        }
    }

    // ----------------------
    // Internal helpers:
    // ----------------------

    // Update cached states when the underlying TileDef is edited.
    pub fn on_tile_def_edited(&mut self) {
        // Re-setting the base cell takes care of updating cached iso coords.
        let base_cell = self.base_cell();
        self.archetype.set_base_cell(base_cell);
    }

    #[inline]
    fn is_animated_archetype(&self) -> bool {
        !self.is(TileKind::Terrain | TileKind::Blocker)
    }

    #[inline]
    fn anim_state(&self) -> &TileAnimState {
        self.archetype.anim_state()
    }

    #[inline]
    fn anim_state_mut(&mut self) -> &mut TileAnimState {
        self.archetype.anim_state_mut()
    }

    #[inline]
    fn set_base_cell(&mut self, cell: Cell) {
        // We would have to update all blocker cells here and point its owner cell back
        // to the new cell.
        assert!(!self.occupies_multiple_cells(), "This does not support multi-cell tiles yet!");

        // This will also update the cached iso coords in the archetype.
        self.archetype.set_base_cell(cell);
    }

    #[inline]
    fn post_load(&mut self, layer: TileMapLayerPtr) {
        debug_assert!(!self.kind.is_empty());
        self.archetype.post_load(layer);
    }
}

// ----------------------------------------------
// TileMapLayerKind
// ----------------------------------------------

#[repr(u8)]
#[derive(Copy,
         Clone,
         PartialEq,
         Eq,
         Display,
         EnumCount,
         EnumIter,
         EnumProperty,
         TryFromPrimitive,
         Serialize,
         Deserialize)]
pub enum TileMapLayerKind {
    #[strum(props(AssetsPath = "tiles/terrain"))]
    Terrain,

    #[strum(props(AssetsPath = "tiles/objects"))]
    Objects,
}

pub const TILE_MAP_LAYER_COUNT: usize = TileMapLayerKind::COUNT;

impl TileMapLayerKind {
    #[inline]
    pub fn assets_path(self) -> PathBuf {
        let path = self.get_str("AssetsPath").unwrap();
        paths::asset_path(path)
    }

    #[inline]
    pub fn from_tile_kind(tile_kind: TileKind) -> Self {
        if tile_kind.intersects(TileKind::Terrain) {
            TileMapLayerKind::Terrain
        } else if tile_kind.intersects(TileKind::Object) {
            TileMapLayerKind::Objects
        } else {
            panic!("Unknown TileKind!");
        }
    }

    #[inline]
    pub fn to_tile_archetype_kind(self) -> TileKind {
        match self {
            TileMapLayerKind::Terrain => TileKind::Terrain,
            TileMapLayerKind::Objects => TileKind::Object,
        }
    }
}

// ----------------------------------------------
// TileMapLayerRefs / TileMapLayerMutRefs
// ----------------------------------------------

// These are bound to the TileMap's lifetime (which in turn is bound to the
// TileSets).
#[derive(Copy, Clone)]
pub struct TileMapLayerRefs {
    ptrs: [mem::RawPtr<TileMapLayer>; TILE_MAP_LAYER_COUNT],
}

#[derive(Copy, Clone)]
pub struct TileMapLayerMutRefs {
    ptrs: [mem::RawPtr<TileMapLayer>; TILE_MAP_LAYER_COUNT],
}

impl TileMapLayerRefs {
    #[inline(always)]
    pub fn get(&self, kind: TileMapLayerKind) -> &TileMapLayer {
        self.ptrs[kind as usize].as_ref()
    }
}

impl TileMapLayerMutRefs {
    #[inline(always)]
    pub fn get(&mut self, kind: TileMapLayerKind) -> &mut TileMapLayer {
        self.ptrs[kind as usize].as_mut()
    }

    // Mutable -> immutable conversion.
    #[inline]
    pub fn to_refs(self) -> TileMapLayerRefs {
        TileMapLayerRefs { ptrs: self.ptrs }
    }
}

// ----------------------------------------------
// TilePoolIndex
// ----------------------------------------------

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)] // Serialized as a newtype/tuple.
pub struct TilePoolIndex {
    value: u32,
}

impl TilePoolIndex {
    #[inline]
    fn new(index: usize) -> Self {
        debug_assert!(index < u32::MAX as usize);
        Self { value: index.try_into().expect("Index cannot fit into u32!") }
    }

    #[inline]
    const fn invalid() -> Self {
        Self { value: u32::MAX }
    }

    #[inline]
    pub fn is_valid(self) -> bool {
        self.value < u32::MAX
    }

    #[inline(always)]
    fn as_usize(self) -> usize {
        debug_assert!(self.is_valid());
        self.value as usize
    }
}

impl std::fmt::Display for TilePoolIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if self.is_valid() {
            write!(f, "[{}]", self.value)
        } else {
            write!(f, "[invalid]")
        }
    }
}

impl Default for TilePoolIndex {
    #[inline]
    fn default() -> Self {
        TilePoolIndex::invalid()
    }
}

const INVALID_TILE_INDEX: TilePoolIndex = TilePoolIndex::invalid();

#[derive(Copy, Clone)]
#[repr(transparent)]
struct CellIndex(usize);

impl CellIndex {
    #[inline(always)]
    fn as_usize(self) -> usize {
        self.0
    }
}

// ----------------------------------------------
// TilePool
// ----------------------------------------------

#[derive(Serialize, Deserialize)]
struct TilePool {
    layer_kind: TileMapLayerKind,
    layer_size_in_cells: Size,

    // WxH tiles, INVALID_TILE_INDEX if empty. Idx to 1st tile in the tiles Slab pool.
    cell_to_slab_idx: Vec<TilePoolIndex>,
    slab: Slab<Tile>,
}

impl TilePool {
    fn new(layer_kind: TileMapLayerKind, size_in_cells: Size) -> Self {
        debug_assert!(size_in_cells.is_valid());
        let tile_count = (size_in_cells.width * size_in_cells.height) as usize;

        Self { layer_kind,
               layer_size_in_cells: size_in_cells,
               cell_to_slab_idx: vec![INVALID_TILE_INDEX; tile_count],
               slab: Slab::new() }
    }

    #[inline(always)]
    fn is_cell_within_bounds(&self, cell: Cell) -> bool {
        if (cell.x < 0 || cell.x >= self.layer_size_in_cells.width)
           || (cell.y < 0 || cell.y >= self.layer_size_in_cells.height)
        {
            return false;
        }
        true
    }

    #[inline(always)]
    fn cell_to_index(&self, cell: Cell) -> CellIndex {
        let cell_index = cell.x + (cell.y * self.layer_size_in_cells.width);
        CellIndex(cell_index as usize)
    }

    #[inline(always)]
    fn cell_index_to_slab(&self, cell_index: CellIndex) -> TilePoolIndex {
        self.cell_to_slab_idx[cell_index.as_usize()]
    }

    #[inline]
    fn try_get_tile(&self, cell: Cell) -> Option<&Tile> {
        if !self.is_cell_within_bounds(cell) {
            return None;
        }

        let cell_index = self.cell_to_index(cell);
        let slab_index = self.cell_index_to_slab(cell_index);

        if slab_index == INVALID_TILE_INDEX {
            return None; // empty cell.
        }

        let tile = self.tile_at_index(slab_index);
        debug_assert!(tile.actual_base_cell() == cell);
        Some(tile)
    }

    #[inline]
    fn try_get_tile_mut(&mut self, cell: Cell) -> Option<&mut Tile> {
        if !self.is_cell_within_bounds(cell) {
            return None;
        }

        let cell_index = self.cell_to_index(cell);
        let slab_index = self.cell_index_to_slab(cell_index);

        if slab_index == INVALID_TILE_INDEX {
            return None; // empty cell.
        }

        let tile_mut = self.tile_at_index_mut(slab_index);
        debug_assert!(tile_mut.actual_base_cell() == cell);
        Some(tile_mut)
    }

    #[inline]
    fn tile_at_index(&self, index: TilePoolIndex) -> &Tile {
        let tile = &self.slab[index.as_usize()];
        debug_assert!(tile.layer_kind() == self.layer_kind);
        tile
    }

    #[inline]
    fn tile_at_index_mut(&mut self, index: TilePoolIndex) -> &mut Tile {
        let tile = &mut self.slab[index.as_usize()];
        debug_assert!(tile.layer_kind() == self.layer_kind);
        tile
    }

    #[inline]
    fn next_index(&self) -> TilePoolIndex {
        TilePoolIndex::new(self.slab.vacant_key())
    }

    fn insert_tile(&mut self, cell: Cell, new_tile: Tile, allow_stacking: bool) -> bool {
        if !self.is_cell_within_bounds(cell) {
            return false;
        }

        let cell_index = self.cell_to_index(cell);
        let mut slab_index = self.cell_index_to_slab(cell_index);

        if slab_index == INVALID_TILE_INDEX {
            // Empty cell; allocate new tile.
            slab_index = TilePoolIndex::new(self.slab.insert(new_tile));
            self.cell_to_slab_idx[cell_index.as_usize()] = slab_index;
        } else {
            // Cell is already occupied.
            // Append to the head of the linked list if we allow stacking tiles, fail
            // otherwise.
            if allow_stacking {
                let new_tile_index = TilePoolIndex::new(self.slab.insert(new_tile));
                self.cell_to_slab_idx[cell_index.as_usize()] = new_tile_index;

                let new_tile = self.tile_at_index_mut(new_tile_index);
                new_tile.next_index = slab_index;
                return true;
            }

            return false;
        }

        true
    }

    fn remove_tile(&mut self, cell: Cell) -> bool {
        if !self.is_cell_within_bounds(cell) {
            return false;
        }

        let cell_index = self.cell_to_index(cell);
        let mut slab_index = self.cell_index_to_slab(cell_index);

        if slab_index == INVALID_TILE_INDEX {
            // Empty cell; do nothing.
            return false;
        }

        // Remove all tiles in this cell.
        while slab_index != INVALID_TILE_INDEX {
            let next_index = self.tile_at_index(slab_index).next_index;
            self.slab.remove(slab_index.as_usize());
            slab_index = next_index;
        }

        self.cell_to_slab_idx[cell_index.as_usize()] = INVALID_TILE_INDEX;

        true
    }

    fn remove_tile_by_index(&mut self, index_to_remove: TilePoolIndex, cell: Cell) -> bool {
        if index_to_remove == INVALID_TILE_INDEX || !self.is_cell_within_bounds(cell) {
            return false;
        }

        let cell_index = self.cell_to_index(cell);
        let slab_index = self.cell_index_to_slab(cell_index);

        if slab_index == INVALID_TILE_INDEX {
            // Empty cell; do nothing.
            return false;
        }

        // Find tile in the chained stack and remove it:
        let mut curr_tile_index = slab_index;
        let mut prev_tile_index = INVALID_TILE_INDEX;
        let mut found_tile = false;

        while curr_tile_index != INVALID_TILE_INDEX {
            if curr_tile_index == index_to_remove {
                if prev_tile_index == INVALID_TILE_INDEX {
                    // list head
                    self.cell_to_slab_idx[cell_index.as_usize()] =
                        self.slab[curr_tile_index.as_usize()].next_index;
                } else {
                    // middle
                    self.slab[prev_tile_index.as_usize()].next_index =
                        self.slab[curr_tile_index.as_usize()].next_index;
                }

                debug_assert!(self.slab[curr_tile_index.as_usize()].self_index == curr_tile_index);
                self.slab[curr_tile_index.as_usize()].self_index = INVALID_TILE_INDEX;
                self.slab[curr_tile_index.as_usize()].next_index = INVALID_TILE_INDEX;
                self.slab.remove(curr_tile_index.as_usize());
                found_tile = true;
                break;
            }

            prev_tile_index = curr_tile_index;
            curr_tile_index = self.slab[curr_tile_index.as_usize()].next_index;
        }

        debug_assert!(found_tile);
        found_tile
    }
}

// ----------------------------------------------
// TileMapLayer
// ----------------------------------------------

#[derive(Serialize, Deserialize)]
pub struct TileMapLayer {
    pool: TilePool,
}

impl TileMapLayer {
    fn new(layer_kind: TileMapLayerKind,
           size_in_cells: Size,
           fill_with_def: Option<&'static TileDef>,
           allow_stacking: bool)
           -> Box<TileMapLayer> {
        let mut layer = Box::new(Self { pool: TilePool::new(layer_kind, size_in_cells) });

        // Optionally initialize all cells:
        if let Some(fill_tile_def) = fill_with_def {
            // Make sure TileDef is compatible with this layer.
            debug_assert!(fill_tile_def.layer_kind() == layer_kind);

            let tile_count = (size_in_cells.width * size_in_cells.height) as usize;
            layer.pool.slab.reserve_exact(tile_count);

            for y in 0..size_in_cells.height {
                for x in 0..size_in_cells.width {
                    let did_insert_tile =
                        layer.insert_tile(Cell::new(x, y), fill_tile_def, allow_stacking);
                    assert!(did_insert_tile);
                }
            }
        } else {
            // Else layer is left empty. Pre-reserve some memory for future tile placements.
            layer.pool.slab.reserve(512);
        }

        layer
    }

    #[inline]
    pub fn memory_usage_estimate(&self) -> usize {
        let mut estimate = std::mem::size_of::<Self>();
        estimate += self.pool.cell_to_slab_idx.capacity() * std::mem::size_of::<usize>();
        estimate += self.pool.slab.capacity() * std::mem::size_of::<Tile>();
        estimate
    }

    #[inline]
    pub fn pool_capacity(&self) -> usize {
        self.pool.slab.capacity()
    }

    #[inline]
    pub fn tile_count(&self) -> usize {
        self.pool.slab.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.pool.slab.is_empty()
    }

    #[inline]
    pub fn size_in_cells(&self) -> Size {
        self.pool.layer_size_in_cells
    }

    #[inline]
    pub fn kind(&self) -> TileMapLayerKind {
        self.pool.layer_kind
    }

    #[inline]
    pub fn is_cell_within_bounds(&self, cell: Cell) -> bool {
        self.pool.is_cell_within_bounds(cell)
    }

    // Get tile or panic if the cell indices are not within bounds or if the tile
    // cell is empty.
    #[inline]
    pub fn tile(&self, cell: Cell) -> &Tile {
        self.pool
            .try_get_tile(cell)
            .expect("TileMapLayer::tile(): Out of bounds cell or empty map cell!")
    }

    #[inline]
    pub fn tile_mut(&mut self, cell: Cell) -> &mut Tile {
        self.pool
            .try_get_tile_mut(cell)
            .expect("TileMapLayer::tile_mut(): Out of bounds cell or empty map cell!")
    }

    // Fails with None if the cell indices are not within bounds or if the tile cell
    // is empty.
    #[inline]
    pub fn try_tile(&self, cell: Cell) -> Option<&Tile> {
        self.pool.try_get_tile(cell)
    }

    #[inline]
    pub fn try_tile_mut(&mut self, cell: Cell) -> Option<&mut Tile> {
        self.pool.try_get_tile_mut(cell)
    }

    // These test against a set of TileKinds and only succeed if
    // the tile at the give cell matches any of the TileKinds.
    #[inline]
    pub fn has_tile(&self, cell: Cell, tile_kinds: TileKind) -> bool {
        self.find_tile(cell, tile_kinds).is_some()
    }

    #[inline]
    pub fn find_tile(&self, cell: Cell, tile_kinds: TileKind) -> Option<&Tile> {
        if let Some(tile) = self.try_tile(cell) {
            if tile.is(tile_kinds) {
                return Some(tile);
            }
        }
        None
    }

    #[inline]
    pub fn find_tile_mut(&mut self, cell: Cell, tile_kinds: TileKind) -> Option<&mut Tile> {
        if let Some(tile) = self.try_tile_mut(cell) {
            if tile.is(tile_kinds) {
                return Some(tile);
            }
        }
        None
    }

    // Get the 8 neighboring tiles plus self cell (optionally).
    pub fn tile_neighbors(&self, cell: Cell, include_self: bool) -> ArrayVec<Option<&Tile>, 9> {
        let mut neighbors = ArrayVec::new();

        if include_self {
            neighbors.push(self.try_tile(cell));
        }

        // left/right
        neighbors.push(self.try_tile(Cell::new(cell.x, cell.y - 1)));
        neighbors.push(self.try_tile(Cell::new(cell.x, cell.y + 1)));

        // top
        neighbors.push(self.try_tile(Cell::new(cell.x + 1, cell.y)));
        neighbors.push(self.try_tile(Cell::new(cell.x + 1, cell.y + 1)));
        neighbors.push(self.try_tile(Cell::new(cell.x + 1, cell.y - 1)));

        // bottom
        neighbors.push(self.try_tile(Cell::new(cell.x - 1, cell.y)));
        neighbors.push(self.try_tile(Cell::new(cell.x - 1, cell.y + 1)));
        neighbors.push(self.try_tile(Cell::new(cell.x - 1, cell.y - 1)));

        neighbors
    }

    pub fn tile_neighbors_mut(&mut self,
                              cell: Cell,
                              include_self: bool)
                              -> ArrayVec<Option<&mut Tile>, 9> {
        let mut neighbors: ArrayVec<_, 9> = ArrayVec::new();

        // Helper closure to get a raw pointer from try_tile_mut().
        let mut raw_tile_ptr = |c: Cell| {
            self.try_tile_mut(c).map(|tile| tile as *mut Tile) // Convert to raw
                                                               // pointer
        };

        if include_self {
            neighbors.push(raw_tile_ptr(cell));
        }

        neighbors.push(raw_tile_ptr(Cell::new(cell.x, cell.y - 1)));
        neighbors.push(raw_tile_ptr(Cell::new(cell.x, cell.y + 1)));

        neighbors.push(raw_tile_ptr(Cell::new(cell.x + 1, cell.y)));
        neighbors.push(raw_tile_ptr(Cell::new(cell.x + 1, cell.y + 1)));
        neighbors.push(raw_tile_ptr(Cell::new(cell.x + 1, cell.y - 1)));

        neighbors.push(raw_tile_ptr(Cell::new(cell.x - 1, cell.y)));
        neighbors.push(raw_tile_ptr(Cell::new(cell.x - 1, cell.y + 1)));
        neighbors.push(raw_tile_ptr(Cell::new(cell.x - 1, cell.y - 1)));

        // SAFETY: We assume all cell coordinates are unique, so no aliasing.
        neighbors.into_iter().map(|opt_ptr| opt_ptr.map(|ptr| unsafe { &mut *ptr })).collect()
    }

    pub fn find_exact_cell_for_point(&self,
                                     screen_point: Vec2,
                                     transform: WorldToScreenTransform)
                                     -> Cell {
        let iso_point = coords::screen_to_iso_point(screen_point, transform, BASE_TILE_SIZE);
        let approx_cell = coords::iso_to_cell(iso_point, BASE_TILE_SIZE);

        if !self.is_cell_within_bounds(approx_cell) {
            return Cell::invalid();
        }

        // Get the 8 possible neighboring tiles + self and test cursor intersection
        // against each so we can know precisely which tile the cursor is hovering.
        let neighbor_cells = [
            // center
            approx_cell,
            // left/right
            Cell::new(approx_cell.x, approx_cell.y - 1),
            Cell::new(approx_cell.x, approx_cell.y + 1),
            // top
            Cell::new(approx_cell.x + 1, approx_cell.y),
            Cell::new(approx_cell.x + 1, approx_cell.y + 1),
            Cell::new(approx_cell.x + 1, approx_cell.y - 1),
            // bottom
            Cell::new(approx_cell.x - 1, approx_cell.y),
            Cell::new(approx_cell.x - 1, approx_cell.y + 1),
            Cell::new(approx_cell.x - 1, approx_cell.y - 1),
        ];

        for cell in neighbor_cells {
            if coords::is_screen_point_inside_cell(screen_point,
                                                   cell,
                                                   BASE_TILE_SIZE,
                                                   BASE_TILE_SIZE,
                                                   transform)
            {
                return cell;
            }
        }

        Cell::invalid()
    }

    #[inline]
    pub fn for_each_tile<F>(&self, tile_kinds: TileKind, mut visitor_fn: F)
        where F: FnMut(&Tile)
    {
        for (_, tile) in &self.pool.slab {
            if tile.is(tile_kinds) {
                visitor_fn(tile);
            }
        }
    }

    #[inline]
    pub fn for_each_tile_mut<F>(&mut self, tile_kinds: TileKind, mut visitor_fn: F)
        where F: FnMut(&mut Tile)
    {
        for (_, tile) in &mut self.pool.slab {
            if tile.is(tile_kinds) {
                visitor_fn(tile);
            }
        }
    }

    // ----------------------
    // Insertion/Removal:
    // ----------------------

    // NOTE: Inserting/removing a tile will not insert/remove any child blocker
    // tiles. Blocker insertion/removal is handled explicitly by the tile
    // placement code.

    pub fn insert_tile(&mut self,
                       cell: Cell,
                       tile_def: &'static TileDef,
                       allow_stacking: bool)
                       -> bool {
        debug_assert!(tile_def.layer_kind() == self.kind());
        let new_tile = Tile::new(cell, self.pool.next_index(), tile_def, self);
        self.pool.insert_tile(cell, new_tile, allow_stacking)
    }

    pub fn insert_blocker_tiles(&mut self, blocker_cells: CellRange, owner_cell: Cell) -> bool {
        // Only building blockers in the Objects layer for now.
        debug_assert!(self.kind() == TileMapLayerKind::Objects);

        for blocker_cell in &blocker_cells {
            if blocker_cell == owner_cell {
                continue;
            }

            let owner_tile = self.tile(owner_cell);
            let blocker_tile = Tile::new_blocker(blocker_cell,
                                                 self.pool.next_index(),
                                                 owner_cell,
                                                 owner_tile.kind,
                                                 owner_tile.flags,
                                                 self);

            const ALLOW_STACKING: bool = false;
            if !self.pool.insert_tile(blocker_cell, blocker_tile, ALLOW_STACKING) {
                return false;
            }
        }

        true
    }

    #[inline]
    pub fn remove_tile(&mut self, cell: Cell) -> bool {
        self.pool.remove_tile(cell)
    }

    #[inline]
    pub fn remove_tile_by_index(&mut self, index: TilePoolIndex, cell: Cell) -> bool {
        self.pool.remove_tile_by_index(index, cell)
    }

    // ----------------------
    // Internal helpers:
    // ----------------------

    #[inline]
    fn visit_next_tiles<F>(&self, mut next_tile_index: TilePoolIndex, mut visitor_fn: F)
        where F: FnMut(&Tile)
    {
        while next_tile_index != INVALID_TILE_INDEX {
            let next_tile = &self[next_tile_index];
            visitor_fn(next_tile);
            next_tile_index = next_tile.next_index;
        }
    }

    #[inline]
    fn visit_next_tiles_mut<F>(&mut self, mut next_tile_index: TilePoolIndex, mut visitor_fn: F)
        where F: FnMut(&mut Tile)
    {
        while next_tile_index != INVALID_TILE_INDEX {
            let next_tile = &mut self[next_tile_index];
            visitor_fn(next_tile);
            next_tile_index = next_tile.next_index;
        }
    }

    #[inline]
    fn find_blocker_owner(&self, owner_cell: Cell) -> &Tile {
        // A blocker tile must always have a valid `owner_cell`. Panic if not.
        self.pool.try_get_tile(owner_cell).expect("Blocker tile must have a valid owner cell!")
    }

    #[inline]
    fn find_blocker_owner_mut(&mut self, owner_cell: Cell) -> &mut Tile {
        // A blocker tile must always have a valid `owner_cell`. Panic if not.
        self.pool.try_get_tile_mut(owner_cell).expect("Blocker tile must have a valid owner cell!")
    }

    #[inline]
    fn update_anims(&mut self, visible_range: CellRange, delta_time_secs: Seconds) {
        for cell in &visible_range {
            let next_tile_index = {
                if let Some(tile) = self.try_tile_mut(cell) {
                    tile.update_anim(delta_time_secs);
                    tile.next_index
                } else {
                    INVALID_TILE_INDEX
                }
            };

            // Update next tiles in the stack chain.
            self.visit_next_tiles_mut(next_tile_index, |next_tile| {
                    next_tile.update_anim(delta_time_secs);
                });
        }
    }

    fn post_load(&mut self) {
        debug_assert!(self.pool.layer_size_in_cells.is_valid());

        // Fix up references:
        {
            let layer = TileMapLayerPtr::new(self);
            for (_, tile) in &mut self.pool.slab {
                tile.post_load(layer);
            }
        }

        // Check pool integrity:
        if cfg!(debug_assertions) {
            for (index, tile) in &self.pool.slab {
                debug_assert!(tile.is_valid());

                let cell = tile.actual_base_cell();
                let cell_index = self.pool.cell_to_index(cell);
                let slab_index = self.pool.cell_index_to_slab(cell_index);

                debug_assert!(slab_index != INVALID_TILE_INDEX);
                debug_assert!(self.pool.is_cell_within_bounds(cell));
                debug_assert!(tile.self_index.as_usize() == index);
                debug_assert!(std::ptr::eq(&self[tile.self_index], tile)); // Ensure addresses are the same.
            }
        }
    }
}

// Immutable indexing
impl Index<TilePoolIndex> for TileMapLayer {
    type Output = Tile;

    #[inline]
    fn index(&self, index: TilePoolIndex) -> &Self::Output {
        self.pool.tile_at_index(index)
    }
}

// Mutable indexing
impl IndexMut<TilePoolIndex> for TileMapLayer {
    #[inline]
    fn index_mut(&mut self, index: TilePoolIndex) -> &mut Self::Output {
        self.pool.tile_at_index_mut(index)
    }
}

// ----------------------------------------------
// Optional callbacks used for editing and dev
// ----------------------------------------------

pub type TilePlacedCallback = fn(&mut Tile, bool);
pub type RemovingTileCallback = fn(&mut Tile);
pub type TileMapResetCallback = fn(&mut TileMap);

// ----------------------------------------------
// TileMap
// ----------------------------------------------

#[derive(Default, Serialize, Deserialize)]
pub struct TileMap {
    size_in_cells: Size,
    layers: ArrayVec<Box<TileMapLayer>, TILE_MAP_LAYER_COUNT>,

    // Minimap is reconstructed on post_load().
    #[serde(skip)]
    minimap: Minimap,

    // NOTE: TileMap callbacks are *not* serialized. These must be manually
    // reset on the user's post_load() after deserialization.

    // Called *after* a tile is placed with the new tile instance.
    #[serde(skip)]
    on_tile_placed_callback: Option<TilePlacedCallback>,

    // Called *before* the tile is removed with the instance about to be removed.
    #[serde(skip)]
    on_removing_tile_callback: Option<RemovingTileCallback>,

    // Called *after* the TileMap has been reset. Any existing tile references/cells are
    // invalidated.
    #[serde(skip)]
    on_map_reset_callback: Option<TileMapResetCallback>,
}

impl TileMap {
    pub fn new(size_in_cells: Size, fill_with_def: Option<&'static TileDef>) -> Self {
        debug_assert!(size_in_cells.is_valid());

        let mut tile_map = Self {
            size_in_cells,
            layers: ArrayVec::new(),
            minimap: Minimap::new(size_in_cells),
            on_tile_placed_callback: None,
            on_removing_tile_callback: None,
            on_map_reset_callback: None
        };

        tile_map.reset(fill_with_def, None);
        tile_map
    }

    pub fn with_terrain_tile(size_in_cells: Size,
                             category_name_hash: StringHash,
                             tile_name_hash: StringHash)
                             -> Self {
        let fill_with_def = TileSets::get().find_tile_def_by_hash(TileMapLayerKind::Terrain,
                                                                  category_name_hash,
                                                                  tile_name_hash);

        Self::new(size_in_cells, fill_with_def)
    }

    pub fn reset(&mut self, fill_with_def: Option<&'static TileDef>, new_map_size: Option<Size>) {
        self.layers.clear();
        self.minimap.reset(fill_with_def, new_map_size);

        if let Some(callback) = self.on_map_reset_callback {
            callback(self);
        }

        if let Some(size_in_cells) = new_map_size {
            self.size_in_cells = size_in_cells;
        }

        for layer_kind in TileMapLayerKind::iter() {
            // Find which layer this tile belong to if we're not just setting everything to empty.
            let fill_opt = {
                if let Some(fill_tile_def) = fill_with_def {
                    if fill_tile_def.layer_kind() == layer_kind {
                        fill_with_def
                    } else {
                        None
                    }
                } else {
                    None
                }
            };

            const ALLOW_STACKING: bool = false;
            self.layers.push(TileMapLayer::new(layer_kind, self.size_in_cells, fill_opt, ALLOW_STACKING));
        }
    }

    pub fn memory_usage_estimate(&self) -> usize {
        let mut estimate = self.minimap.memory_usage_estimate();
        for layer in &self.layers {
            estimate += layer.memory_usage_estimate();
        }
        estimate
    }

    pub fn tile_count(&self) -> usize {
        let mut count = 0;
        for layer in &self.layers {
            count += layer.tile_count();
        }
        count
    }

    pub fn is_empty(&self) -> bool {
        for layer in &self.layers {
            if !layer.is_empty() {
                return false;
            }
        }
        true
    }

    #[inline]
    pub fn size_in_cells(&self) -> Size {
        self.size_in_cells
    }

    #[inline]
    pub fn is_cell_within_bounds(&self, cell: Cell) -> bool {
        if (cell.x < 0 || cell.x >= self.size_in_cells.width)
           || (cell.y < 0 || cell.y >= self.size_in_cells.height)
        {
            return false;
        }
        true
    }

    #[inline]
    pub fn layers(&self) -> TileMapLayerRefs {
        TileMapLayerRefs {
            ptrs: [
                mem::RawPtr::from_ref(self.layer(TileMapLayerKind::Terrain)),
                mem::RawPtr::from_ref(self.layer(TileMapLayerKind::Objects)),
            ]
        }
    }

    #[inline]
    pub fn layers_mut(&mut self) -> TileMapLayerMutRefs {
        TileMapLayerMutRefs {
            ptrs: [
                mem::RawPtr::from_ref(self.layer_mut(TileMapLayerKind::Terrain)),
                mem::RawPtr::from_ref(self.layer_mut(TileMapLayerKind::Objects)),
            ]
        }
    }

    #[inline]
    pub fn layer(&self, kind: TileMapLayerKind) -> &TileMapLayer {
        debug_assert!(self.layers[kind as usize].kind() == kind);
        &self.layers[kind as usize]
    }

    #[inline]
    pub fn layer_mut(&mut self, kind: TileMapLayerKind) -> &mut TileMapLayer {
        debug_assert!(self.layers[kind as usize].kind() == kind);
        &mut self.layers[kind as usize]
    }

    #[inline]
    pub fn try_tile_from_layer(&self, cell: Cell, kind: TileMapLayerKind) -> Option<&Tile> {
        if self.layers.is_empty() {
            return None;
        }

        let layer = self.layer(kind);
        debug_assert!(layer.kind() == kind);
        layer.try_tile(cell)
    }

    #[inline]
    pub fn try_tile_from_layer_mut(&mut self,
                                   cell: Cell,
                                   kind: TileMapLayerKind)
                                   -> Option<&mut Tile> {
        if self.layers.is_empty() {
            return None;
        }

        let layer = self.layer_mut(kind);
        debug_assert!(layer.kind() == kind);
        layer.try_tile_mut(cell)
    }

    #[inline]
    pub fn has_tile(&self, cell: Cell, layer_kind: TileMapLayerKind, tile_kinds: TileKind) -> bool {
        if self.layers.is_empty() {
            return false;
        }

        self.layer(layer_kind).has_tile(cell, tile_kinds)
    }

    #[inline]
    pub fn find_tile(&self,
                     cell: Cell,
                     layer_kind: TileMapLayerKind,
                     tile_kinds: TileKind)
                     -> Option<&Tile> {
        if self.layers.is_empty() {
            return None;
        }

        self.layer(layer_kind).find_tile(cell, tile_kinds)
    }

    #[inline]
    pub fn find_tile_mut(&mut self,
                         cell: Cell,
                         layer_kind: TileMapLayerKind,
                         tile_kinds: TileKind)
                         -> Option<&mut Tile> {
        if self.layers.is_empty() {
            return None;
        }

        self.layer_mut(layer_kind).find_tile_mut(cell, tile_kinds)
    }

    #[inline]
    pub fn find_exact_cell_for_point(&self,
                                     layer_kind: TileMapLayerKind,
                                     screen_point: Vec2,
                                     transform: WorldToScreenTransform)
                                     -> Cell {
        if self.layers.is_empty() {
            return Cell::invalid();
        }

        self.layer(layer_kind).find_exact_cell_for_point(screen_point, transform)
    }

    #[inline]
    pub fn for_each_tile<F>(&self,
                            layer_kind: TileMapLayerKind,
                            tile_kinds: TileKind,
                            visitor_fn: F)
        where F: FnMut(&Tile)
    {
        if !self.layers.is_empty() {
            let layer = self.layer(layer_kind);
            layer.for_each_tile(tile_kinds, visitor_fn);
        }
    }

    #[inline]
    pub fn for_each_tile_mut<F>(&mut self,
                                layer_kind: TileMapLayerKind,
                                tile_kinds: TileKind,
                                visitor_fn: F)
        where F: FnMut(&mut Tile)
    {
        if !self.layers.is_empty() {
            let layer = self.layer_mut(layer_kind);
            layer.for_each_tile_mut(tile_kinds, visitor_fn);
        }
    }

    #[inline]
    pub fn minimap(&self) -> &Minimap {
        &self.minimap
    }

    #[inline]
    pub fn minimap_mut(&mut self) -> &mut Minimap {
        &mut self.minimap
    }

    #[inline]
    pub fn update_anims(&mut self, visible_range: CellRange, delta_time_secs: Seconds) {
        if !self.layers.is_empty() {
            // NOTE: Terrain layer is not animated by design. Only objects animate.
            let objects_layer = self.layer_mut(TileMapLayerKind::Objects);
            objects_layer.update_anims(visible_range, delta_time_secs);
        }
    }

    // ----------------------
    // Tile stacking:
    // ----------------------

    #[inline]
    pub fn tile_at_index(&self, index: TilePoolIndex, layer_kind: TileMapLayerKind) -> &Tile {
        debug_assert!(index != INVALID_TILE_INDEX);
        let layer = self.layer(layer_kind);
        &layer[index]
    }

    #[inline]
    pub fn tile_at_index_mut(&mut self,
                             index: TilePoolIndex,
                             layer_kind: TileMapLayerKind)
                             -> &mut Tile {
        debug_assert!(index != INVALID_TILE_INDEX);
        let layer = self.layer_mut(layer_kind);
        &mut layer[index]
    }

    #[inline]
    pub fn next_tile(&self, tile: &Tile) -> Option<&Tile> {
        if tile.next_index == INVALID_TILE_INDEX {
            return None;
        }

        let layer = self.layer(tile.layer_kind());
        Some(&layer[tile.next_index])
    }

    #[inline]
    pub fn next_tile_mut(&mut self, tile: &Tile) -> Option<&mut Tile> {
        if tile.next_index == INVALID_TILE_INDEX {
            return None;
        }

        let layer = self.layer_mut(tile.layer_kind());
        Some(&mut layer[tile.next_index])
    }

    #[inline]
    pub fn visit_next_tiles<F>(&self, tile: &Tile, visitor_fn: F)
        where F: FnMut(&Tile)
    {
        let layer = self.layer(tile.layer_kind());
        layer.visit_next_tiles(tile.next_index, visitor_fn);
    }

    #[inline]
    pub fn visit_next_tiles_mut<F>(&mut self, tile: &Tile, visitor_fn: F)
        where F: FnMut(&mut Tile)
    {
        let layer = self.layer_mut(tile.layer_kind());
        layer.visit_next_tiles_mut(tile.next_index, visitor_fn);
    }

    // ----------------------
    // Tile placement:
    // ----------------------

    #[inline]
    pub fn try_place_tile(&mut self,
                          target_cell: Cell,
                          tile_def_to_place: &'static TileDef)
                          -> Result<&mut Tile, String> {
        self.try_place_tile_in_layer(target_cell,
                                     tile_def_to_place.layer_kind(), // Guess layer from TileDef.
                                     tile_def_to_place)
    }

    #[inline]
    pub fn try_place_tile_in_layer(&mut self,
                                   target_cell: Cell,
                                   layer_kind: TileMapLayerKind,
                                   tile_def_to_place: &'static TileDef)
                                   -> Result<&mut Tile, String> {
        if self.layers.is_empty() {
            return Err("Map has no layers".into());
        }

        // Prevent placing objects/props over non-walkable terrain tiles (water/roads, etc).
        placement::is_placement_on_terrain_valid(self.layers(),
                                                 target_cell,
                                                 tile_def_to_place)?;

        let mut minimap = mem::RawPtr::from_ref(&self.minimap);

        let tile_placed_callback = self.on_tile_placed_callback;
        let layer = self.layer_mut(layer_kind);
        let prev_pool_capacity = layer.pool_capacity();

        let result = placement::try_place_tile_in_layer(layer, target_cell, tile_def_to_place)
            .map(|(tile, new_pool_capacity)| {
                if let Some(callback) = tile_placed_callback {
                    let did_reallocate = new_pool_capacity != prev_pool_capacity;
                    callback(tile, did_reallocate);
                }
                tile
            });

        if result.is_ok() {
            minimap.place_tile(target_cell, tile_def_to_place);
        }

        result
    }

    #[inline]
    pub fn try_clear_tile_from_layer(&mut self,
                                     target_cell: Cell,
                                     layer_kind: TileMapLayerKind)
                                     -> Result<(), String> {
        if self.layers.is_empty() {
            return Err("Map has no layers".into());
        }

        if let Some(callback) = self.on_removing_tile_callback {
            if let Some(tile) = self.try_tile_from_layer_mut(target_cell, layer_kind) {
                callback(tile);
            }
        }

        let result =
            placement::try_clear_tile_from_layer(self.layer_mut(layer_kind), target_cell);

        if let Ok(tile_def) = result {
            self.minimap.clear_tile(target_cell, tile_def);
        }

        result.map(|_| ())
    }

    #[inline]
    pub fn try_clear_tile_from_layer_by_index(&mut self,
                                              target_index: TilePoolIndex,
                                              target_cell: Cell,
                                              layer_kind: TileMapLayerKind)
                                              -> Result<(), String> {
        debug_assert!(target_index != INVALID_TILE_INDEX);

        if self.layers.is_empty() {
            return Err("Map has no layers".into());
        }

        if let Some(callback) = self.on_removing_tile_callback {
            let layer = self.layer_mut(layer_kind);
            if layer.try_tile_mut(target_cell).is_some() {
                callback(&mut layer[target_index]);
            }
        }

        let result =
            placement::try_clear_tile_from_layer_by_index(self.layer_mut(layer_kind),
                                                          target_index,
                                                          target_cell);

        if let Ok(tile_def) = result {
            self.minimap.clear_tile(target_cell, tile_def);
        }

        result.map(|_| ())
    }

    pub fn can_move_tile(&self,
                         from: Cell,
                         to: Cell,
                         layer_kind: TileMapLayerKind,
                         allow_stacking: bool)
                         -> bool {
        if from == to || self.layers.is_empty() {
            return false;
        }

        if !self.is_cell_within_bounds(from) || !self.is_cell_within_bounds(to) {
            return false;
        }

        let layer = self.layer(layer_kind);

        let from_tile = {
            if let Some(from_tile) = layer.try_tile(from) {
                from_tile
            } else {
                return false; // No tile at 'from' cell!
            }
        };

        if let Some(to_tile) = layer.try_tile(to) {
            if allow_stacking && from_tile.is(TileKind::Unit) && to_tile.is(TileKind::Unit) {
                // Allow stacking units on the same cell.
                return true;
            }
            return false; // 'to' tile cell is occupied!
        }

        true
    }

    // Move tile from one cell to another if destination is free.
    pub fn try_move_tile(&mut self, from: Cell, to: Cell, layer_kind: TileMapLayerKind) -> bool {
        const ALLOW_STACKING: bool = false;
        if !self.can_move_tile(from, to, layer_kind, ALLOW_STACKING) {
            return false;
        }

        let layer = self.layer_mut(layer_kind);

        let from_cell_index = layer.pool.cell_to_index(from);
        let from_slab_index = layer.pool.cell_index_to_slab(from_cell_index);
        debug_assert!(from_slab_index != INVALID_TILE_INDEX); // Can't be empty, we have the 'from' tile.

        let to_cell_index = layer.pool.cell_to_index(to);
        let to_slab_index = layer.pool.cell_index_to_slab(to_cell_index);
        debug_assert!(to_slab_index == INVALID_TILE_INDEX); // Should be empty, we've checked the destination is free.

        // Swap indices.
        layer.pool.cell_to_slab_idx[from_cell_index.as_usize()] = to_slab_index;
        layer.pool.cell_to_slab_idx[to_cell_index.as_usize()] = from_slab_index;

        // Update cached tile states:
        let tile = &mut layer[from_slab_index];
        tile.set_base_cell(to);

        true
    }

    // Move tile from one cell to another, stacking unit tiles if they overlap.
    pub fn try_move_tile_with_stacking(&mut self,
                                       from_idx: TilePoolIndex,
                                       from_cell: Cell,
                                       to_cell: Cell,
                                       layer_kind: TileMapLayerKind)
                                       -> bool {
        debug_assert!(from_idx != INVALID_TILE_INDEX);

        const ALLOW_STACKING: bool = true;
        if !self.can_move_tile(from_cell, to_cell, layer_kind, ALLOW_STACKING) {
            return false;
        }

        let layer = self.layer_mut(layer_kind);

        // from cell: either becomes empty or pops one from the stack.
        {
            debug_assert!(from_idx != INVALID_TILE_INDEX);
            debug_assert!(layer[from_idx].is(TileKind::Unit));
            debug_assert!(layer[from_idx].layer_kind() == layer_kind);

            let from_cell_index = layer.pool.cell_to_index(from_cell);

            let mut curr_tile_index = layer.pool.cell_index_to_slab(from_cell_index);
            let mut prev_tile_index = INVALID_TILE_INDEX;
            let mut found_tile = false;

            while curr_tile_index != INVALID_TILE_INDEX {
                if curr_tile_index == from_idx {
                    if prev_tile_index == INVALID_TILE_INDEX {
                        // list head
                        layer.pool.cell_to_slab_idx[from_cell_index.as_usize()] =
                            layer[curr_tile_index].next_index;
                    } else {
                        // middle
                        layer[prev_tile_index].next_index = layer[curr_tile_index].next_index;
                    }

                    debug_assert!(layer[curr_tile_index].self_index == curr_tile_index);
                    layer[curr_tile_index].next_index = INVALID_TILE_INDEX;
                    found_tile = true;
                    break;
                }

                prev_tile_index = curr_tile_index;
                curr_tile_index = layer[curr_tile_index].next_index;
            }

            if !found_tile {
                debug_assert!(false, "Failed to find tile index {from_idx:?} for cell {from_cell}");
                return false;
            }
        }

        // Destination may be empty or may contain single tile or a stack.
        {
            let to_cell_index = layer.pool.cell_to_index(to_cell);
            let to_slab_index = layer.pool.cell_index_to_slab(to_cell_index);

            // Only units can stack.
            debug_assert!(to_slab_index == INVALID_TILE_INDEX
                          || layer[to_slab_index].is(TileKind::Unit));

            layer.pool.cell_to_slab_idx[to_cell_index.as_usize()] = from_idx;

            let from_tile = &mut layer[from_idx];
            from_tile.set_base_cell(to_cell);
            from_tile.next_index = to_slab_index;

            debug_assert!(from_tile.self_index == from_idx);
        }

        true
    }

    // ----------------------
    // Tile selection:
    // ----------------------

    #[inline]
    pub fn update_selection(&mut self,
                            selection: &mut TileSelection,
                            cursor_screen_pos: Vec2,
                            transform: WorldToScreenTransform,
                            placement_op: PlacementOp) {
        if self.layers.is_empty() {
            return;
        }

        let map_size_in_cells = self.size_in_cells();

        selection.update(self.layers_mut(),
                         map_size_in_cells,
                         cursor_screen_pos,
                         transform,
                         placement_op);
    }

    #[inline]
    pub fn clear_selection(&mut self, selection: &mut TileSelection) {
        if self.layers.is_empty() {
            return;
        }
        selection.clear(self.layers_mut());
    }

    pub fn topmost_selected_tile(&self, selection: &TileSelection) -> Option<&Tile> {
        let selected_cell = selection.last_cell();
        // Returns the tile at the topmost layer if it is not empty
        // (object, terrain), or nothing if all layers are empty.
        for layer_kind in TileMapLayerKind::iter().rev() {
            let tile = self.try_tile_from_layer(selected_cell, layer_kind);
            if tile.is_some() {
                return tile;
            }
        }
        None
    }

    pub fn topmost_tile_at_cursor(&self,
                                  cursor_screen_pos: Vec2,
                                  transform: WorldToScreenTransform)
                                  -> Option<&Tile> {
        debug_assert!(transform.is_valid());

        if self.layers.is_empty() {
            return None;
        }

        // Find topmost layer tile under the target cell.
        for layer_kind in TileMapLayerKind::iter().rev() {
            let layer = self.layer(layer_kind);

            let target_cell = layer.find_exact_cell_for_point(cursor_screen_pos, transform);

            let tile = layer.try_tile(target_cell);
            if tile.is_some() {
                return tile;
            }
        }
        None
    }

    // ----------------------
    // Editor callbacks:
    // ----------------------

    pub fn set_tile_placed_callback(&mut self, callback: Option<TilePlacedCallback>) {
        self.on_tile_placed_callback = callback;
    }

    pub fn set_removing_tile_callback(&mut self, callback: Option<RemovingTileCallback>) {
        self.on_removing_tile_callback = callback;
    }

    pub fn set_map_reset_callback(&mut self, callback: Option<TileMapResetCallback>) {
        self.on_map_reset_callback = callback;
    }
}

// ----------------------------------------------
// Save/Load for TileMap
// ----------------------------------------------

impl Save for TileMap {
    fn pre_save(&mut self) {
        // Reset selection state. We don't save TileSelection.
        self.for_each_tile_mut(TileMapLayerKind::Terrain, TileKind::all(), |tile| {
            tile.set_flags(TileFlags::Highlighted | TileFlags::Invalidated, false);
        });

        self.for_each_tile_mut(TileMapLayerKind::Objects, TileKind::all(), |tile| {
            tile.set_flags(TileFlags::Highlighted | TileFlags::Invalidated, false);
        });
    }

    fn save(&self, state: &mut SaveStateImpl) -> SaveResult {
        state.save(self)
    }
}

impl Load for TileMap {
    fn pre_load(&mut self, context: &PreLoadContext) {
        self.minimap.pre_load(context);
    }

    fn load(&mut self, state: &SaveStateImpl) -> LoadResult {
        state.load(self)
    }

    fn post_load(&mut self, context: &PostLoadContext) {
        debug_assert!(self.size_in_cells >= Size::zero());

        // These are *not* serialized, so should be unset.
        // TileMap users have to reassign these on their post_load().
        debug_assert!(self.on_tile_placed_callback.is_none());
        debug_assert!(self.on_removing_tile_callback.is_none());
        debug_assert!(self.on_map_reset_callback.is_none());

        for layer in &mut self.layers {
            layer.post_load();
        }

        self.minimap.post_load(context);
    }
}
