use std::time::{self};
use slab::Slab;
use bitflags::bitflags;
use arrayvec::ArrayVec;
use serde::Deserialize;
use strum::{EnumCount, EnumProperty, IntoEnumIterator};
use strum_macros::{Display, EnumCount, EnumProperty, EnumIter};

use crate::{
    bitflags_with_display,
    utils::{
        Size, Rect, Vec2, Color,
        UnsafeWeakRef,
        coords::{
            self,
            Cell,
            CellRange,
            IsoPoint,
            WorldToScreenTransform
        }
    }
};

use super::{
    placement::{self},
    selection::TileSelection,
    sets::{
        TileSets,
        TileDef,
        TileKind,
        TileTexInfo,
        BASE_TILE_SIZE
    }
};

// ----------------------------------------------
// GameStateHandle
// ----------------------------------------------

// Index into associated game state.
#[derive(Copy, Clone)]
pub struct GameStateHandle {
    index: u32,
    kind:  u32,
}

impl GameStateHandle {
    #[inline]
    pub fn new(index: usize, kind: u32) -> Self {
        debug_assert!(index < u32::MAX as usize);
        debug_assert!(kind  < u32::MAX); // Reserved value for invalid.
        Self {
            index: index as u32,
            kind:  kind,
        }
    }

    #[inline]
    pub const fn invalid() -> Self {
        Self {
            index: u32::MAX,
            kind:  u32::MAX,
        }
    }

    #[inline]
    pub fn is_valid(&self) -> bool {
        self.index < u32::MAX &&
        self.kind  < u32::MAX
    }

    #[inline]
    pub fn index(&self) -> usize {
        debug_assert!(self.is_valid());
        self.index as usize
    }

    #[inline]
    pub fn kind(&self) -> u32 {
        debug_assert!(self.is_valid());
        self.kind
    }
}

impl Default for GameStateHandle {
    fn default() -> Self { GameStateHandle::invalid() }
}

// ----------------------------------------------
// TileFlags
// ----------------------------------------------

bitflags_with_display! {
    #[derive(Copy, Clone, Default, PartialEq, Eq)]
    pub struct TileFlags: u8 {
        const Hidden          = 1 << 0;
        const Highlighted     = 1 << 1;
        const Invalidated     = 1 << 2;
        const OccludesTerrain = 1 << 3;
    
        // Debug flags:
        const DrawDebugInfo   = 1 << 4;
        const DrawDebugBounds = 1 << 5;
        const DrawBlockerInfo = 1 << 6;
    }
}

// ----------------------------------------------
// TileAnimState
// ----------------------------------------------

#[derive(Copy, Clone, Default)]
struct TileAnimState {
    anim_set_index: u16,
    frame_index: u16,
    frame_play_time_secs: f32,
}

// ----------------------------------------------
// Tile / TileArchetype
// ----------------------------------------------

// Tile is tied to the lifetime of the TileSets that owns the underlying TileDef.
// We also may keep a reference to the owning TileMapLayer inside TileArchetype
// for building blockers and objects.
pub struct Tile<'tile_sets> {
    kind: TileKind,
    flags: TileFlags,
    archetype: TileArchetype<'tile_sets>,
}

// NOTE: Using a raw union here to avoid some padding and since we can
// derive the tile archetype from the `Tile::kind` field.
#[repr(C)]
union TileArchetype<'tile_sets> {
    terrain: TerrainTile<'tile_sets>,
    object:  ObjectTile<'tile_sets>,
    blocker: BlockerTile<'tile_sets>,
}

impl<'tile_sets> TileArchetype<'tile_sets> {
    #[inline]
    fn new_terrain(terrain: TerrainTile<'tile_sets>) -> Self {
        Self { terrain: terrain }
    }

    #[inline]
    fn new_object(object: ObjectTile<'tile_sets>) -> Self {
        Self { object: object }
    }

    #[inline]
    fn new_blocker(blocker: BlockerTile<'tile_sets>) -> Self {
        Self { blocker: blocker }
    }
}

// Call the specified method in the active member of the union.
macro_rules! delegate_to_archetype {
    ($self:ident, $method:ident $(, $arg:expr )* ) => {
        unsafe {
            if      $self.is(TileKind::Terrain) { $self.archetype.terrain.$method( $( $arg ),* ) }
            else if $self.is(TileKind::Object)  { $self.archetype.object.$method(  $( $arg ),* ) }
            else if $self.is(TileKind::Blocker) { $self.archetype.blocker.$method( $( $arg ),* ) }
            else { panic!("Invalid TileKind!"); }
        }
    };
}

// ----------------------------------------------
// TileBehavior
// ----------------------------------------------

// Common behavior for all Tile archetypes.
trait TileBehavior<'tile_sets> {
    fn game_state_handle(&self) -> GameStateHandle;
    fn set_game_state_handle(&mut self, handle: GameStateHandle);

    fn calc_z_sort(&self) -> i32;
    fn calc_adjusted_iso_coords(&self, _: TileKind) -> IsoPoint;

    fn actual_base_cell(&self) -> Cell;
    fn cell_range(&self) -> CellRange;

    fn tile_def(&self) -> &'tile_sets TileDef;
    fn is_valid(&self) -> bool;
    fn set_flags(&mut self, current_flags: &mut TileFlags, new_flags: TileFlags, value: bool);

    // Variations:
    fn variation_index(&self) -> usize;
    fn set_variation_index(&mut self, index: usize);

    // Animations:
    fn anim_state_ref(&self) -> &TileAnimState;
    fn anim_state_mut_ref(&mut self) -> &mut TileAnimState;
}

// ----------------------------------------------
// TerrainTile
// ----------------------------------------------

#[derive(Copy, Clone)]
struct TerrainTile<'tile_sets> {
    def: &'tile_sets TileDef,

    // Terrain tiles always occupy a single cell (of BASE_TILE_SIZE size).
    cell: Cell,
}

// NOTES:
//  - Terrain tiles cannot store game state.
//  - Terrain tile are always 1x1.
//  - Terrain tile logical size is fixed (BASE_TILE_SIZE).
//  - Terrain tile draw size can be customized.
//  - No variations or animations.
//
impl<'tile_sets> TileBehavior<'tile_sets> for TerrainTile<'tile_sets> {
    #[inline] fn game_state_handle(&self) -> GameStateHandle { GameStateHandle::invalid() }
    #[inline] fn set_game_state_handle(&mut self, _: GameStateHandle) {}

    #[inline] fn calc_z_sort(&self) -> i32 { coords::cell_to_iso(self.cell, BASE_TILE_SIZE).y }
    #[inline] fn calc_adjusted_iso_coords(&self, _: TileKind) -> IsoPoint { coords::cell_to_iso(self.cell, BASE_TILE_SIZE) }

    #[inline] fn actual_base_cell(&self) -> Cell { self.cell }
    #[inline] fn cell_range(&self) -> CellRange { CellRange::new(self.cell, self.cell) }

    #[inline] fn tile_def(&self) -> &'tile_sets TileDef { self.def }
    #[inline] fn is_valid(&self) -> bool { self.cell.is_valid() && self.def.is_valid() }

    #[inline]
    fn set_flags(&mut self, current_flags: &mut TileFlags, new_flags: TileFlags, value: bool) {
        current_flags.set(new_flags, value);
    }

    // No support for variations on Terrain.
    #[inline] fn variation_index(&self) -> usize { 0 }
    #[inline] fn set_variation_index(&mut self, _: usize) {}

    // No support for animations on Terrain.
    #[inline]
    fn anim_state_ref(&self) -> &TileAnimState {
        const DUMMY_ANIM_STATE: TileAnimState = TileAnimState {
            anim_set_index: 0,
            frame_index: 0,
            frame_play_time_secs: 0.0,
        };
        // Return a valid dummy value for Tile::anim_set_index()/Tile::anim_frame_index()/etc.
        &DUMMY_ANIM_STATE
    }

    #[inline]
    fn anim_state_mut_ref(&mut self) -> &mut TileAnimState {
        // This is method is only called from Tile::update_anim(), so should never be used for Terrain.
        panic!("Terrain Tiles are not animated! Do not call update_anim() on a Terrain Tile.");
    }
}

// ----------------------------------------------
// ObjectTile
// ----------------------------------------------

#[derive(Copy, Clone)]
struct ObjectTile<'tile_sets> {
    def: &'tile_sets TileDef,

    // Owning layer so we can propagate flags from a building to all of its blocker tiles.
    // SAFETY: This ref will always be valid as long as the Tile instance is, since the Tile
    // belongs to its parent layer.
    layer: UnsafeWeakRef<TileMapLayer<'tile_sets>>,

    // Buildings can occupy multiple cells. `cell_range.start` is the start or "base" cell.
    cell_range: CellRange,
    game_state: GameStateHandle,
    anim_state: TileAnimState,
    variation_index: u32,
}

impl<'tile_sets> TileBehavior<'tile_sets> for ObjectTile<'tile_sets> {
    #[inline] fn game_state_handle(&self) -> GameStateHandle { self.game_state }
    #[inline] fn set_game_state_handle(&mut self, handle: GameStateHandle) { self.game_state = handle; }

    #[inline] fn actual_base_cell(&self) -> Cell { self.cell_range.start }
    #[inline] fn cell_range(&self) -> CellRange { self.cell_range }

    #[inline] fn tile_def(&self) -> &'tile_sets TileDef { self.def }
    #[inline] fn is_valid(&self) -> bool { self.cell_range.is_valid() && self.def.is_valid() }

    #[inline]
    fn set_flags(&mut self, _: &mut TileFlags, new_flags: TileFlags, value: bool) {
        // Propagate flags to any child blockers in its cell range (including self).
        for cell in &self.cell_range {
            let tile = self.layer.tile_mut(cell);
            tile.flags.set(new_flags, value);
        }
    }

    #[inline]
    fn calc_z_sort(&self) -> i32 {
        let height = self.def.logical_size.height;
        coords::cell_to_iso(self.cell_range.start, BASE_TILE_SIZE).y - height
    }

    #[inline]
    fn calc_adjusted_iso_coords(&self, kind: TileKind) -> IsoPoint {
        // Convert the anchor (bottom tile for buildings) to isometric coordinates:
        let mut tile_iso_coords = coords::cell_to_iso(self.cell_range.start, BASE_TILE_SIZE);

        if kind.intersects(TileKind::Building | TileKind::Prop | TileKind::Vegetation) {
            // Center the sprite horizontally:
            tile_iso_coords.x += (BASE_TILE_SIZE.width / 2) - (self.def.logical_size.width / 2);

            // Vertical offset: move up the full sprite height *minus* 1 tile's height.
            // Since the anchor is the bottom tile, and cell_to_iso gives us the *bottom*,
            // we must offset up by (image_height - one_tile_height).
            tile_iso_coords.y -= self.def.draw_size.height - BASE_TILE_SIZE.height;
        } else if kind.intersects(TileKind::Unit) {
            // Adjust to center the unit sprite:
            tile_iso_coords.x += (BASE_TILE_SIZE.width / 2) - (self.def.draw_size.width / 2);
            tile_iso_coords.y -= self.def.draw_size.height - (BASE_TILE_SIZE.height / 2);
        }

        tile_iso_coords
    }

    // Variations:
    #[inline] fn variation_index(&self) -> usize { self.variation_index as usize }
    #[inline] fn set_variation_index(&mut self, index: usize) { self.variation_index = index.min(self.def.variations.len() - 1) as u32; }

    // Animations:
    #[inline] fn anim_state_ref(&self) -> &TileAnimState { &self.anim_state }
    #[inline] fn anim_state_mut_ref(&mut self) -> &mut TileAnimState { &mut self.anim_state }
}

// ----------------------------------------------
// BlockerTile
// ----------------------------------------------

#[derive(Copy, Clone)]
struct BlockerTile<'tile_sets> {
    // Weak reference to owning map layer so we can seamlessly resolve blockers into buildings.
    // SAFETY: This ref will always be valid as long as the Tile instance is, since the Tile
    // belongs to its parent layer.
    layer: UnsafeWeakRef<TileMapLayer<'tile_sets>>,

    // Building blocker tiles occupy a single cell and have a backreference to the owner start cell.
    // `owner_cell` must be always valid.
    cell: Cell,
    owner_cell: Cell,
}

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
impl<'tile_sets> BlockerTile<'tile_sets> {
    #[inline]
    fn owner(&self) -> &Tile<'tile_sets> {
        self.layer.find_blocker_owner(self.owner_cell)
    }

    #[inline]
    fn owner_mut(&mut self) -> &mut Tile<'tile_sets> {
        self.layer.find_blocker_owner_mut(self.owner_cell)
    }
}

impl<'tile_sets> TileBehavior<'tile_sets> for BlockerTile<'tile_sets> {
    #[inline] fn game_state_handle(&self) -> GameStateHandle { self.owner().game_state_handle() }
    #[inline] fn set_game_state_handle(&mut self, handle: GameStateHandle) { self.owner_mut().set_game_state_handle(handle); }

    #[inline] fn actual_base_cell(&self) -> Cell { self.cell }
    #[inline] fn cell_range(&self) -> CellRange { self.owner().cell_range() }

    #[inline] fn calc_z_sort(&self) -> i32 { self.owner().calc_z_sort() }
    #[inline] fn calc_adjusted_iso_coords(&self, _: TileKind) -> IsoPoint { self.owner().calc_adjusted_iso_coords() }

    #[inline]
    fn tile_def(&self) -> &'tile_sets TileDef {
        self.owner().tile_def()
    }

    #[inline]
    fn is_valid(&self) -> bool {
        if !self.cell.is_valid() || !self.owner_cell.is_valid() {
            return false;
        }
        self.owner().is_valid()
    }

    #[inline]
    fn set_flags(&mut self, _: &mut TileFlags, new_flags: TileFlags, value: bool) {
        // Propagate back to owner tile:
        self.owner_mut().set_flags(new_flags, value);
    }

    // Variations:
    #[inline] fn variation_index(&self) -> usize { self.owner().variation_index() }
    #[inline] fn set_variation_index(&mut self, index: usize) { self.owner_mut().set_variation_index(index); }

    // Animations:
    #[inline] fn anim_state_ref(&self) -> &TileAnimState { self.owner().anim_state_ref() }
    #[inline] fn anim_state_mut_ref(&mut self) -> &mut TileAnimState {
        // This is method is only called from Tile::update_anim(), so should never be used for Blocker.
        panic!("Blocker Tiles are not animated! Do not call update_anim() on a Blocker Tile.");
    }
}

// ----------------------------------------------
// Tile impl
// ----------------------------------------------

impl<'tile_sets> Tile<'tile_sets> {
    fn new(cell: Cell,
           tile_def: &'tile_sets TileDef,
           layer: &TileMapLayer<'tile_sets>) -> Self {

        let archetype = match layer.kind() {
            TileMapLayerKind::Terrain => {
                TileArchetype::new_terrain(TerrainTile {
                    def: tile_def,
                    cell: cell,
                })
            },
            TileMapLayerKind::Objects => {
                TileArchetype::new_object(ObjectTile {
                    def: tile_def,
                    layer: UnsafeWeakRef::new(layer),
                    cell_range: tile_def.calc_footprint_cells(cell),
                    game_state: GameStateHandle::default(),
                    anim_state: TileAnimState::default(),
                    variation_index: 0,
                })
            }
        };

        Self {
            kind: tile_def.kind(),
            flags: tile_def.flags(),
            archetype: archetype
        }
    }

    fn new_blocker(blocker_cell: Cell,
                   owner_cell: Cell,
                   owner_kind: TileKind,
                   owner_flags: TileFlags,
                   layer: &TileMapLayer<'tile_sets>) -> Self {
        debug_assert!(owner_kind == TileKind::Object | TileKind::Building);
        Self {
            kind: TileKind::Blocker | TileKind::Building,
            flags: owner_flags,
            archetype: TileArchetype::new_blocker(BlockerTile {
                layer: UnsafeWeakRef::new(layer),
                cell: blocker_cell,
                owner_cell: owner_cell,
            }),
        }
    }

    #[inline]
    pub fn set_flags(&mut self, new_flags: TileFlags, value: bool) {
        delegate_to_archetype!(self, set_flags, &mut self.flags, new_flags, value);
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
        if self.kind.is_empty() {
            return false;
        }
        delegate_to_archetype!(self, is_valid)
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
    pub fn game_state_handle(&self) -> GameStateHandle {
        delegate_to_archetype!(self, game_state_handle)
    }

    #[inline]
    pub fn set_game_state_handle(&mut self, handle: GameStateHandle) {
        delegate_to_archetype!(self, set_game_state_handle, handle)
    }

    #[inline]
    pub fn tile_def(&self) -> &'tile_sets TileDef {
        delegate_to_archetype!(self, tile_def)
    }

    #[inline]
    pub fn name(&self) -> &'tile_sets str {
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
    pub fn has_multi_cell_footprint(&self) -> bool {
        self.tile_def().has_multi_cell_footprint()
    }

    #[inline]
    pub fn calc_z_sort(&self) -> i32 {
        delegate_to_archetype!(self, calc_z_sort)
    }

    #[inline]
    pub fn calc_adjusted_iso_coords(&self) -> IsoPoint {
        delegate_to_archetype!(self, calc_adjusted_iso_coords, self.kind)
    }

    #[inline]
    pub fn calc_screen_rect(&self, transform: &WorldToScreenTransform) -> Rect {
        let draw_size = self.draw_size();
        let iso_position = self.calc_adjusted_iso_coords();
        coords::iso_to_screen_rect(iso_position, draw_size, transform)
    }

    // Base cell without resolving blocker tiles into their owner cell.
    #[inline]
    pub fn actual_base_cell(&self) -> Cell {
        delegate_to_archetype!(self, actual_base_cell)
    }

    #[inline]
    pub fn base_cell(&self) -> Cell {
        self.cell_range().start
    }

    #[inline]
    pub fn cell_range(&self) -> CellRange {
        delegate_to_archetype!(self, cell_range)
    }

    #[inline]
    pub fn for_each_cell<F>(&self, reverse: bool, mut visitor_fn: F)
        where
            F: FnMut(Cell)
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
                                            transform: &WorldToScreenTransform) -> bool {

        let cell = self.actual_base_cell();
        let tile_size = self.logical_size();

        coords::is_screen_point_inside_cell(screen_point,
                                            cell,
                                            tile_size,
                                            BASE_TILE_SIZE,
                                            transform)
    }

    pub fn category_name(&self, tile_sets: &'tile_sets TileSets) -> &'tile_sets str {
        tile_sets.find_category_for_tile_def(self.tile_def())
            .map_or("<none>", |cat| &cat.name)
    }

    pub fn try_get_editable_tile_def(&self, tile_sets: &'tile_sets TileSets) -> Option<&'tile_sets mut TileDef> {
        tile_sets.try_get_editable_tile_def(self.tile_def())
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
    pub fn variation_name(&self) -> &'tile_sets str {
        self.tile_def().variation_name(self.variation_index())
    }

    #[inline]
    pub fn variation_index(&self) -> usize {
        delegate_to_archetype!(self, variation_index)
    }

    #[inline]
    pub fn set_variation_index(&mut self, index: usize) {
        delegate_to_archetype!(self, set_variation_index, index)
    }

    // ----------------------
    // Animations:
    // ----------------------

    #[inline]
    pub fn has_animations(&self) -> bool {
        let anim_set_index  = self.anim_set_index();
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
    pub fn anim_set_name(&self) -> &'tile_sets str {
        self.tile_def().anim_set_name(self.variation_index(), self.anim_set_index())
    }

    #[inline]
    pub fn anim_frames_count(&self) -> usize {
        self.tile_def().anim_frames_count(self.variation_index())
    }

    #[inline]
    pub fn anim_set_index(&self) -> usize {
        self.anim_state_ref().anim_set_index as usize
    }

    #[inline]
    pub fn anim_frame_index(&self) -> usize {
        self.anim_state_ref().frame_index as usize
    }

    #[inline]
    pub fn anim_frame_play_time_secs(&self) -> f32 {
        self.anim_state_ref().frame_play_time_secs
    }

    #[inline]
    pub fn anim_frame_tex_info(&self) -> Option<&'tile_sets TileTexInfo> {
        let anim_set_index  = self.anim_set_index();
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
    fn update_anim(&mut self, delta_time_secs: f32) {
        if self.is(TileKind::Terrain | TileKind::Blocker) {
            return; // Not animated.
        }

        let def = self.tile_def();
        let anim_set_index  = self.anim_set_index();
        let variation_index = self.variation_index();

        if let Some(anim_set) = def.anim_set_by_index(variation_index, anim_set_index) {
            if anim_set.frames.len() <= 1 {
                // Single frame sprite, nothing to update.
                return;
            }

            let anim_state = delegate_to_archetype!(self, anim_state_mut_ref);
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

    #[inline]
    fn anim_state_ref(&self) -> &TileAnimState {
        delegate_to_archetype!(self, anim_state_ref)
    }
}

// ----------------------------------------------
// TileMapLayerKind
// ----------------------------------------------

#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Eq, Debug, Display, EnumCount, EnumIter, EnumProperty, Deserialize)]
pub enum TileMapLayerKind {
    #[strum(props(AssetsPath = "assets/tiles/terrain"))]
    Terrain,

    #[strum(props(AssetsPath = "assets/tiles/objects"))]
    Objects,
}

pub const TILE_MAP_LAYER_COUNT: usize = TileMapLayerKind::COUNT;

impl TileMapLayerKind {
    #[inline]
    pub fn assets_path(self) -> &'static str {
        self.get_str("AssetsPath").unwrap()
    }

    #[inline]
    pub fn from_tile_kind(tile_kind: TileKind) -> Self {
        if tile_kind.intersects(TileKind::Terrain) {
            TileMapLayerKind::Terrain
        } else if tile_kind.intersects(TileKind::Object   |
                                       TileKind::Blocker  |
                                       TileKind::Building |
                                       TileKind::Prop     |
                                       TileKind::Unit     |
                                       TileKind::Vegetation) {
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

// These are bound to the TileMap's lifetime (which in turn is bound to the TileSets).
#[derive(Copy, Clone)]
pub struct TileMapLayerRefs<'tile_sets> {
    refs: [UnsafeWeakRef<TileMapLayer<'tile_sets>>; TILE_MAP_LAYER_COUNT],
}

#[derive(Copy, Clone)]
pub struct TileMapLayerMutRefs<'tile_sets> {
    refs: [UnsafeWeakRef<TileMapLayer<'tile_sets>>; TILE_MAP_LAYER_COUNT],
}

impl<'tile_sets> TileMapLayerRefs<'tile_sets> {
    #[inline(always)]
    pub fn get(&self, kind: TileMapLayerKind) -> &TileMapLayer<'tile_sets> {
        self.refs[kind as usize].as_ref()
    }
}

impl<'tile_sets> TileMapLayerMutRefs<'tile_sets> {
    #[inline(always)]
    pub fn get(&mut self, kind: TileMapLayerKind) -> &mut TileMapLayer<'tile_sets> {
        self.refs[kind as usize].as_mut()
    }
}

// ----------------------------------------------
// TilePool
// ----------------------------------------------

const INVALID_TILE_INDEX: usize = usize::MAX;

struct TilePool<'tile_sets> {
    layer_kind: TileMapLayerKind,
    layer_size_in_cells: Size,

    // WxH tiles, INVALID_TILE_INDEX if empty. Idx to 1st tile in the tiles Slab pool.
    cell_to_slab_idx: Vec<usize>,
    slab: Slab<Tile<'tile_sets>>,
}

impl<'tile_sets> TilePool<'tile_sets> {
    fn new(layer_kind: TileMapLayerKind, size_in_cells: Size) -> Self {
        debug_assert!(size_in_cells.is_valid());
        let tile_count = (size_in_cells.width * size_in_cells.height) as usize;

        Self {
            layer_kind: layer_kind,
            layer_size_in_cells: size_in_cells,
            cell_to_slab_idx: vec![INVALID_TILE_INDEX; tile_count],
            slab: Slab::new(),
        }
    }

    #[inline(always)]
    fn is_cell_within_bounds(&self, cell: Cell) -> bool {
         if (cell.x < 0 || cell.x >= self.layer_size_in_cells.width) ||
            (cell.y < 0 || cell.y >= self.layer_size_in_cells.height) {
            return false;
        }
        true
    }

    #[inline(always)]
    fn map_cell_to_index(&self, cell: Cell) -> usize {
        let cell_index = cell.x + (cell.y * self.layer_size_in_cells.width);
        cell_index as usize
    }

    #[inline]
    fn try_get_tile(&self, cell: Cell) -> Option<&Tile<'tile_sets>> {
        if !self.is_cell_within_bounds(cell) {
            return None;
        }

        let cell_index = self.map_cell_to_index(cell);
        let slab_index = self.cell_to_slab_idx[cell_index];

        if slab_index == INVALID_TILE_INDEX {
            return None; // empty cell.
        }

        let tile = &self.slab[slab_index];

        debug_assert!(tile.layer_kind() == self.layer_kind);
        debug_assert!(tile.actual_base_cell() == cell);

        Some(tile)
    }

    #[inline]
    fn try_get_tile_mut(&mut self, cell: Cell) -> Option<&mut Tile<'tile_sets>> {
        if !self.is_cell_within_bounds(cell) {
            return None;
        }

        let cell_index = self.map_cell_to_index(cell);
        let slab_index = self.cell_to_slab_idx[cell_index];

        if slab_index == INVALID_TILE_INDEX {
            return None; // empty cell.
        }

        let tile_mut = &mut self.slab[slab_index];

        debug_assert!(tile_mut.layer_kind() == self.layer_kind);
        debug_assert!(tile_mut.actual_base_cell() == cell);

        Some(tile_mut)
    }

    #[inline]
    fn insert_tile(&mut self, cell: Cell, new_tile: Tile<'tile_sets>) -> bool {
        if !self.is_cell_within_bounds(cell) {
            return false;
        }

        let cell_index = self.map_cell_to_index(cell);
        let mut slab_index = self.cell_to_slab_idx[cell_index];

        if slab_index == INVALID_TILE_INDEX {
            // Empty cell; allocate new tile.
            slab_index = self.slab.insert(new_tile);
            self.cell_to_slab_idx[cell_index] = slab_index;
        } else {
            // Cell is already occupied.
            return false;
        }

        true
    }

    #[inline]
    fn remove_tile(&mut self, cell: Cell) -> bool {
        if !self.is_cell_within_bounds(cell) {
            return false;
        }

        let cell_index = self.map_cell_to_index(cell);
        let slab_index = self.cell_to_slab_idx[cell_index];

        if slab_index == INVALID_TILE_INDEX {
            // Empty cell; do nothing.
            return false;
        }

        self.cell_to_slab_idx[cell_index] = INVALID_TILE_INDEX;
        self.slab.remove(slab_index);

        true
    }
}

// ----------------------------------------------
// TileMapLayer
// ----------------------------------------------

pub struct TileMapLayer<'tile_sets> {
    pool: TilePool<'tile_sets>
}

impl<'tile_sets> TileMapLayer<'tile_sets> {
    fn new(layer_kind: TileMapLayerKind,
           size_in_cells: Size,
           fill_with_def: Option<&'tile_sets TileDef>) -> Box<TileMapLayer<'tile_sets>> {

        let mut layer = Box::new(Self {
            pool: TilePool::new(layer_kind, size_in_cells),
        });

        // Optionally initialize all cells:
        if let Some(fill_tile_def) = fill_with_def {
            // Make sure TileDef is compatible with this layer.
            debug_assert!(fill_tile_def.layer_kind() == layer_kind);

            let tile_count = (size_in_cells.width * size_in_cells.height) as usize;
            layer.pool.slab.reserve_exact(tile_count);

            for y in 0..size_in_cells.height {
                for x in 0..size_in_cells.width {
                    let cell = Cell::new(x, y);
                    let did_insert_tile = layer.insert_tile(cell, fill_tile_def);
                    assert!(did_insert_tile);
                }
            }
        }
        // else layer is left empty.

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

    // Get tile or panic if the cell indices are not within bounds or if the tile cell is empty.
    #[inline]
    pub fn tile(&self, cell: Cell) -> &Tile<'tile_sets> {
        self.pool.try_get_tile(cell)
            .expect("TileMapLayer::tile(): Out of bounds cell or empty map cell!")
    }

    #[inline]
    pub fn tile_mut(&mut self, cell: Cell) -> &mut Tile<'tile_sets> {
        self.pool.try_get_tile_mut(cell)
            .expect("TileMapLayer::tile_mut(): Out of bounds cell or empty map cell!")
    }

    // Fails with None if the cell indices are not within bounds or if the tile cell is empty.
    #[inline]
    pub fn try_tile(&self, cell: Cell) -> Option<&Tile<'tile_sets>> {
        self.pool.try_get_tile(cell)
    }

    #[inline]
    pub fn try_tile_mut(&mut self, cell: Cell) -> Option<&mut Tile<'tile_sets>> {
        self.pool.try_get_tile_mut(cell)
    }

    // These test against a set of TileKinds and only succeed if
    // the tile at the give cell matches any of the TileKinds.
    #[inline]
    pub fn has_tile(&self, cell: Cell, tile_kinds: TileKind) -> bool {
        self.find_tile(cell, tile_kinds).is_some()
    }

    #[inline]
    pub fn find_tile(&self, cell: Cell, tile_kinds: TileKind) -> Option<&Tile<'tile_sets>> {
        if let Some(tile) = self.try_tile(cell) {
            if tile.is(tile_kinds) {
                return Some(tile);
            }
        }
        None
    }

    #[inline]
    pub fn find_tile_mut(&mut self, cell: Cell, tile_kinds: TileKind) -> Option<&mut Tile<'tile_sets>> {
        if let Some(tile) = self.try_tile_mut(cell) {
            if tile.is(tile_kinds) {
                return Some(tile);
            }
        }
        None
    }

    // Get the 8 neighboring tiles plus self cell (optionally).
    pub fn tile_neighbors(&self, cell: Cell, include_self: bool)
        -> ArrayVec<Option<&Tile<'tile_sets>>, 9> {

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

    pub fn tile_neighbors_mut(&mut self, cell: Cell, include_self: bool)
        -> ArrayVec<Option<&mut Tile<'tile_sets>>, 9> {

        let mut neighbors: ArrayVec<_, 9> = ArrayVec::new();

        // Helper closure to get a raw pointer from try_tile_mut().
        let mut raw_tile_ptr = |c: Cell| {
            self.try_tile_mut(c)
                .map(|tile| tile as *mut Tile<'tile_sets>) // Convert to raw pointer
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
        neighbors
            .into_iter()
            .map(|opt_ptr| opt_ptr.map(|ptr| unsafe { &mut *ptr }))
            .collect()
    }

    pub fn find_exact_cell_for_point(&self,
                                     screen_point: Vec2,
                                     transform: &WorldToScreenTransform) -> Cell {

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
                                                   transform) {
                return cell;
            }
        }

        Cell::invalid()
    }

    #[inline]
    pub fn for_each_tile<F>(&self, tile_kinds: TileKind, mut visitor_fn: F)
        where
            F: FnMut(&Tile<'tile_sets>)
    {
        for (_, tile) in &self.pool.slab {
            if tile.is(tile_kinds) {
                visitor_fn(tile);
            }
        }
    }

    #[inline]
    pub fn for_each_tile_mut<F>(&mut self, tile_kinds: TileKind, mut visitor_fn: F)
        where
            F: FnMut(&mut Tile<'tile_sets>)
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

    // NOTE: Inserting/removing a tile will not insert/remove any child blocker tiles.
    // Blocker insertion/removal is handled explicitly by the tile placement code.

    pub fn insert_tile(&mut self, cell: Cell, tile_def: &'tile_sets TileDef) -> bool {
        debug_assert!(tile_def.layer_kind() == self.kind());
        let new_tile = Tile::new(cell, tile_def, self);
        self.pool.insert_tile(cell, new_tile)
    }

    pub fn insert_blocker_tiles(&mut self, blocker_cells: CellRange, owner_cell: Cell) -> bool {
        // Only building blockers in the Objects layer for now.
        debug_assert!(self.kind() == TileMapLayerKind::Objects);

        for blocker_cell in &blocker_cells {
            if blocker_cell == owner_cell {
                continue;
            }

            let owner_tile  = self.tile(owner_cell);
            let blocker_tile = Tile::new_blocker(
                blocker_cell,
                owner_cell,
                owner_tile.kind,
                owner_tile.flags,
                self);

            if !self.pool.insert_tile(blocker_cell, blocker_tile) {
                return false;
            }
        }

        true
    }

    pub fn remove_tile(&mut self, cell: Cell) -> bool {
        self.pool.remove_tile(cell)
    }

    // ----------------------
    // Internal helpers:
    // ----------------------

    #[inline]
    fn find_blocker_owner(&self, owner_cell: Cell) -> &Tile<'tile_sets> {
        // A blocker tile must always have a valid `owner_cell`. Panic if not.
        self.pool.try_get_tile(owner_cell)
            .expect("Blocker tile must have a valid owner cell!")
    }

    #[inline]
    fn find_blocker_owner_mut(&mut self, owner_cell: Cell) -> &mut Tile<'tile_sets> {
        // A blocker tile must always have a valid `owner_cell`. Panic if not.
        self.pool.try_get_tile_mut(owner_cell)
            .expect("Blocker tile must have a valid owner cell!")
    }

    #[inline]
    fn update_anims(&mut self, visible_range: CellRange, delta_time_secs: f32) {
        for cell in &visible_range {
            if let Some(tile) = self.try_tile_mut(cell) {
                tile.update_anim(delta_time_secs);
            }
        }
    }
}

// ----------------------------------------------
// TileMap
// ----------------------------------------------

pub struct TileMap<'tile_sets> {
    size_in_cells: Size,
    layers: ArrayVec<Box<TileMapLayer<'tile_sets>>, TILE_MAP_LAYER_COUNT>,
}

impl<'tile_sets> TileMap<'tile_sets> {
    pub fn new(size_in_cells: Size, fill_with_def: Option<&'tile_sets TileDef>) -> Self {
        debug_assert!(size_in_cells.is_valid());
        let mut tile_map = Self {
            size_in_cells: size_in_cells,
            layers: ArrayVec::new(),
        };
        tile_map.reset(fill_with_def);
        tile_map
    }

    pub fn reset(&mut self, fill_with_def: Option<&'tile_sets TileDef>) {
        self.layers.clear();
        for layer_kind in TileMapLayerKind::iter() {
            // Find which layer this tile belong to if we're not just setting everything to empty.
            let fill_opt =
                if let Some(fill_tile_def) = fill_with_def {
                    if fill_tile_def.layer_kind() == layer_kind {
                        fill_with_def
                    } else {
                        None
                    }
                } else {
                    None
                };
            self.layers.push(TileMapLayer::new(layer_kind, self.size_in_cells, fill_opt));
        }
    }

    pub fn memory_usage_estimate(&self) -> usize {
        let mut estimate = 0;
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
         if (cell.x < 0 || cell.x >= self.size_in_cells.width) ||
            (cell.y < 0 || cell.y >= self.size_in_cells.height) {
            return false;
        }
        true
    }

    #[inline]
    pub fn layers(&self) -> TileMapLayerRefs<'tile_sets> {
        TileMapLayerRefs {
            refs: [
                UnsafeWeakRef::new(self.layer(TileMapLayerKind::Terrain)),
                UnsafeWeakRef::new(self.layer(TileMapLayerKind::Objects)),
            ]
        }
    }

    #[inline]
    pub fn layers_mut(&mut self) -> TileMapLayerMutRefs<'tile_sets> {
        TileMapLayerMutRefs {
            refs: [
                UnsafeWeakRef::new(self.layer_mut(TileMapLayerKind::Terrain)),
                UnsafeWeakRef::new(self.layer_mut(TileMapLayerKind::Objects)),
            ]
        }
    }

    #[inline]
    pub fn layer(&self, kind: TileMapLayerKind) -> &TileMapLayer<'tile_sets> {
        debug_assert!(self.layers[kind as usize].kind() == kind);
        &self.layers[kind as usize]
    }

    #[inline]
    pub fn layer_mut(&mut self, kind: TileMapLayerKind) -> &mut TileMapLayer<'tile_sets> {
        debug_assert!(self.layers[kind as usize].kind() == kind);
        &mut self.layers[kind as usize]
    }

    #[inline]
    pub fn try_tile_from_layer(&self,
                               cell: Cell,
                               kind: TileMapLayerKind) -> Option<&Tile<'tile_sets>> {
        let layer = self.layer(kind);
        debug_assert!(layer.kind() == kind);
        layer.try_tile(cell)
    }

    #[inline]
    pub fn try_tile_from_layer_mut(&mut self,
                                   cell: Cell,
                                   kind: TileMapLayerKind) -> Option<&mut Tile<'tile_sets>> {
        let layer = self.layer_mut(kind);
        debug_assert!(layer.kind() == kind);
        layer.try_tile_mut(cell)
    }

    #[inline]
    pub fn has_tile(&self,
                    cell: Cell,
                    layer_kind: TileMapLayerKind,
                    tile_kinds: TileKind) -> bool {
        self.layer(layer_kind).has_tile(cell, tile_kinds)
    }

    #[inline]
    pub fn find_tile(&self,
                     cell: Cell,
                     layer_kind: TileMapLayerKind,
                     tile_kinds: TileKind) -> Option<&Tile<'tile_sets>> {
        self.layer(layer_kind).find_tile(cell, tile_kinds)
    }

    #[inline]
    pub fn find_tile_mut(&mut self,
                         cell: Cell,
                         layer_kind: TileMapLayerKind,
                         tile_kinds: TileKind) -> Option<&mut Tile<'tile_sets>> {
        self.layer_mut(layer_kind).find_tile_mut(cell, tile_kinds)
    }

    #[inline]
    pub fn find_exact_cell_for_point(&self,
                                     layer_kind: TileMapLayerKind,
                                     screen_point: Vec2,
                                     transform: &WorldToScreenTransform) -> Cell {
        self.layer(layer_kind).find_exact_cell_for_point(screen_point, transform)
    }

    #[inline]
    pub fn for_each_tile<F>(&self, layer_kind: TileMapLayerKind, tile_kinds: TileKind, visitor_fn: F)
        where
            F: FnMut(&Tile<'tile_sets>)
    {
        let layer = self.layer(layer_kind);
        layer.for_each_tile(tile_kinds, visitor_fn);
    }

    #[inline]
    pub fn for_each_tile_mut<F>(&mut self, layer_kind: TileMapLayerKind, tile_kinds: TileKind, visitor_fn: F)
        where
            F: FnMut(&mut Tile<'tile_sets>)
    {
        let layer = self.layer_mut(layer_kind);
        layer.for_each_tile_mut(tile_kinds, visitor_fn);
    }

    #[inline]
    pub fn update_anims(&mut self, visible_range: CellRange, delta_time: time::Duration) {
        let delta_time_secs = delta_time.as_secs_f32();

        // NOTE: Terrain layer is not animated by design. Only objects animate.
        let objects_layer = self.layer_mut(TileMapLayerKind::Objects);
        objects_layer.update_anims(visible_range, delta_time_secs);
    }

    // ----------------------
    // Tile placement:
    // ----------------------

    #[inline]
    pub fn try_place_tile(&mut self,
                          target_cell: Cell,
                          tile_def_to_place: &'tile_sets TileDef) -> bool {
        placement::try_place_tile_in_layer(
            self.layer_mut(tile_def_to_place.layer_kind()), // Guess layer from TileDef.
            target_cell,
            tile_def_to_place)
    }

    #[inline]
    pub fn try_place_tile_in_layer(&mut self,
                                   target_cell: Cell,
                                   layer_kind: TileMapLayerKind,
                                   tile_def_to_place: &'tile_sets TileDef) -> bool {
        placement::try_place_tile_in_layer(self.layer_mut(layer_kind), target_cell, tile_def_to_place)
    }

    #[inline]
    pub fn try_place_tile_at_cursor(&mut self,
                                    cursor_screen_pos: Vec2,
                                    transform: &WorldToScreenTransform,
                                    tile_def_to_place: &'tile_sets TileDef) -> bool {
        placement::try_place_tile_at_cursor(self, cursor_screen_pos, transform, tile_def_to_place)
    }

    #[inline]
    pub fn try_clear_tile_from_layer(&mut self,
                                     target_cell: Cell,
                                     layer_kind: TileMapLayerKind) -> bool {
        placement::try_clear_tile_from_layer(self.layer_mut(layer_kind), target_cell)
    }

    #[inline]
    pub fn try_clear_tile_at_cursor(&mut self,
                                    cursor_screen_pos: Vec2,
                                    transform: &WorldToScreenTransform) -> bool {
        placement::try_clear_tile_at_cursor(self, cursor_screen_pos, transform)
    }

    // ----------------------
    // Tile selection:
    // ----------------------

    #[inline]
    pub fn update_selection(&mut self,
                            selection: &mut TileSelection,
                            cursor_screen_pos: Vec2,
                            transform: &WorldToScreenTransform,
                            placement_candidate: Option<&'tile_sets TileDef>) {
        let map_size_in_cells = self.size_in_cells();
        selection.update(
            self.layers_mut(),
            map_size_in_cells,
            cursor_screen_pos,
            transform, 
            placement_candidate);
    }

    #[inline]
    pub fn clear_selection(&mut self, selection: &mut TileSelection) {
        selection.clear(self.layers_mut());
    }

    #[inline]
    pub fn topmost_selected_tile(&self, selection: &TileSelection) -> Option<&Tile<'tile_sets>> {
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
}
