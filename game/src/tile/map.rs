use slab::Slab;
use bitflags::bitflags;
use arrayvec::ArrayVec;
use serde::Deserialize;
use strum::{EnumCount, EnumProperty, IntoEnumIterator};
use strum_macros::{Display, EnumCount, EnumProperty, EnumIter};

use crate::{
    bitflags_with_display,
    utils::{
        coords::{
            self,
            Cell,
            CellRange,
            IsoPoint,
            WorldToScreenTransform
        },
        hash::StrHashPair,
        UnsafeWeakRef,
        Seconds,
        Color,
        Rect,
        Size,
        Vec2
    }
};

use super::{
    placement::{self, PlacementOp},
    selection::TileSelection,
    sets::{
        TileSets,
        TileAnimSet,
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
            kind,
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
    frame_play_time_secs: Seconds,
}

impl TileAnimState {
    const DEFAULT: Self = Self {
        anim_set_index: 0,
        frame_index: 0,
        frame_play_time_secs: 0.0
    };
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
    variation_index: u16,
    z_sort_key: i32,
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
        Self { terrain }
    }

    #[inline]
    fn new_object(object: ObjectTile<'tile_sets>) -> Self {
        Self { object }
    }

    #[inline]
    fn new_blocker(blocker: BlockerTile<'tile_sets>) -> Self {
        Self { blocker }
    }
}

// Call the specified method in the active member of the union.
macro_rules! delegate_to_archetype {
    ( $self:ident, $method:ident $(, $arg:expr )* ) => {
        unsafe {
            if      $self.is(TileKind::Terrain) { $self.archetype.terrain.$method( $( $arg ),* ) }
            else if $self.is(TileKind::Blocker) { $self.archetype.blocker.$method( $( $arg ),* ) }
            else if $self.is(TileKind::Object)  { $self.archetype.object.$method(  $( $arg ),* ) }
            else { panic!("Invalid TileKind!"); }
        }
    };
}

// ----------------------------------------------
// TileBehavior
// ----------------------------------------------

// Common behavior for all Tile archetypes.
trait TileBehavior<'tile_sets> {
    fn set_flags(&mut self, current_flags: &mut TileFlags, new_flags: TileFlags, value: bool);
    fn set_base_cell(&mut self, cell: Cell);
    fn set_iso_coords_f32(&mut self, iso_coords: Vec2);

    fn game_state_handle(&self) -> GameStateHandle;
    fn set_game_state_handle(&mut self, handle: GameStateHandle);

    fn z_sort_key(&self) -> i32;
    fn iso_coords_f32(&self) -> Vec2;

    fn actual_base_cell(&self) -> Cell;
    fn cell_range(&self) -> CellRange;

    fn tile_def(&self) -> &'tile_sets TileDef;
    fn is_valid(&self) -> bool;

    // Animations:
    fn anim_state_ref(&self) -> &TileAnimState;
    fn anim_state_mut_ref(&mut self) -> &mut TileAnimState;
}

// ----------------------------------------------
// TerrainTile
// ----------------------------------------------

// NOTES:
//  - Terrain tiles cannot store game state.
//  - Terrain tile are always 1x1.
//  - Terrain tile logical size is fixed (BASE_TILE_SIZE).
//  - Terrain tile draw size can be customized.
//  - No variations or animations.
//
#[derive(Copy, Clone)]
struct TerrainTile<'tile_sets> {
    def: &'tile_sets TileDef,

    // Terrain tiles always occupy a single cell (of BASE_TILE_SIZE size).
    cell: Cell,

    // Cached on construction.
    iso_coords_f32: Vec2,
}

impl<'tile_sets> TerrainTile<'tile_sets> {
    fn new(cell: Cell, tile_def: &'tile_sets TileDef) -> Self {
        Self {
            def: tile_def,
            cell,
            iso_coords_f32: coords::cell_to_iso(cell, BASE_TILE_SIZE).to_vec2(),
        }
    }
}

impl<'tile_sets> TileBehavior<'tile_sets> for TerrainTile<'tile_sets> {
    #[inline]
    fn set_flags(&mut self, current_flags: &mut TileFlags, new_flags: TileFlags, value: bool) {
        current_flags.set(new_flags, value);
    }

    #[inline]
    fn set_base_cell(&mut self, cell: Cell) {
        self.cell = cell;
        self.iso_coords_f32 = coords::cell_to_iso(cell, BASE_TILE_SIZE).to_vec2();
    }

    #[inline] fn set_iso_coords_f32(&mut self, iso_coords: Vec2) { self.iso_coords_f32 = iso_coords; }

    #[inline] fn game_state_handle(&self) -> GameStateHandle { GameStateHandle::invalid() }
    #[inline] fn set_game_state_handle(&mut self, _handle: GameStateHandle) {}

    #[inline] fn z_sort_key(&self) -> i32 { self.iso_coords_f32.y as i32 }
    #[inline] fn iso_coords_f32(&self) -> Vec2 { self.iso_coords_f32 }

    #[inline] fn actual_base_cell(&self) -> Cell { self.cell }
    #[inline] fn cell_range(&self) -> CellRange { CellRange::new(self.cell, self.cell) }

    #[inline] fn tile_def(&self) -> &'tile_sets TileDef { self.def }
    #[inline] fn is_valid(&self) -> bool { self.cell.is_valid() && self.def.is_valid() }

    // No support for animations on Terrain.
    #[inline]
    fn anim_state_ref(&self) -> &TileAnimState {
        // Return a valid dummy value for Tile::anim_set_index(),
        // Tile::anim_frame_index(), etc that has all fields set to defaults.
        &TileAnimState::DEFAULT
    }

    #[inline]
    fn anim_state_mut_ref(&mut self) -> &mut TileAnimState {
        // This is method is only called from Tile::update_anim() and
        // Tile::set_anim_set_index(), so should never be used for Terrain.
        panic!("Terrain Tiles are not animated! Do not call this on a Terrain Tile.");
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

    // Cached on construction.
    iso_coords_f32: Vec2,
}

impl<'tile_sets> ObjectTile<'tile_sets> {
    fn new(cell: Cell,
           tile_def: &'tile_sets TileDef,
           layer: &TileMapLayer<'tile_sets>) -> Self {
        Self {
            def: tile_def,
            layer: UnsafeWeakRef::new(layer),
            cell_range: tile_def.cell_range(cell),
            game_state: GameStateHandle::default(),
            anim_state: TileAnimState::default(),
            iso_coords_f32: calc_object_iso_coords(tile_def.kind(), cell, tile_def.logical_size, tile_def.draw_size),
        }
    }
}

impl<'tile_sets> TileBehavior<'tile_sets> for ObjectTile<'tile_sets> {
    #[inline]
    fn set_flags(&mut self, _current_flags: &mut TileFlags, new_flags: TileFlags, value: bool) {
        // Propagate flags to any child blockers in its cell range (including self).
        for cell in &self.cell_range {
            let tile = self.layer.tile_mut(cell);
            tile.flags.set(new_flags, value);
        }
    }

    #[inline]
    fn set_base_cell(&mut self, cell: Cell) {
        self.cell_range = self.def.cell_range(cell);
        self.iso_coords_f32 = calc_object_iso_coords(self.def.kind(), cell, self.def.logical_size, self.def.draw_size);
    }

    #[inline] fn set_iso_coords_f32(&mut self, iso_coords: Vec2) { self.iso_coords_f32 = iso_coords; }

    #[inline] fn game_state_handle(&self) -> GameStateHandle { self.game_state }
    #[inline] fn set_game_state_handle(&mut self, handle: GameStateHandle) { self.game_state = handle; }

    #[inline] fn z_sort_key(&self) -> i32 { calc_object_z_sort_key(self.cell_range.start, self.def.logical_size.height) }
    #[inline] fn iso_coords_f32(&self) -> Vec2 { self.iso_coords_f32 }

    #[inline] fn actual_base_cell(&self) -> Cell { self.cell_range.start }
    #[inline] fn cell_range(&self) -> CellRange { self.cell_range }

    #[inline] fn tile_def(&self) -> &'tile_sets TileDef { self.def }
    #[inline] fn is_valid(&self) -> bool { self.cell_range.is_valid() && self.def.is_valid() }

    // Animations:
    #[inline] fn anim_state_ref(&self) -> &TileAnimState { &self.anim_state }
    #[inline] fn anim_state_mut_ref(&mut self) -> &mut TileAnimState { &mut self.anim_state }
}

#[inline]
fn calc_object_z_sort_key(base_cell: Cell, logical_height: i32) -> i32 {
    coords::cell_to_iso(base_cell, BASE_TILE_SIZE).y - logical_height
}

pub fn calc_object_iso_coords(kind: TileKind, base_cell: Cell, logical_size: Size, draw_size: Size) -> Vec2 {
    // Convert the anchor (bottom tile for buildings) to isometric coordinates:
    let mut tile_iso_coords = coords::cell_to_iso(base_cell, BASE_TILE_SIZE);

    if kind.intersects(TileKind::Building | TileKind::Prop | TileKind::Vegetation) {
        // Center the sprite horizontally:
        tile_iso_coords.x += (BASE_TILE_SIZE.width / 2) - (logical_size.width / 2);

        // Vertical offset: move up the full sprite height *minus* 1 tile's height.
        // Since the anchor is the bottom tile, and cell_to_iso gives us the *bottom*,
        // we must offset up by (image_height - one_tile_height).
        tile_iso_coords.y -= draw_size.height - BASE_TILE_SIZE.height;
    } else if kind.intersects(TileKind::Unit) {
        // Adjust to center the unit sprite:
        tile_iso_coords.x += (BASE_TILE_SIZE.width / 2) - (draw_size.width / 2);
        tile_iso_coords.y -= draw_size.height - (BASE_TILE_SIZE.height / 2);
    }

    tile_iso_coords.to_vec2()
}

pub fn calc_unit_iso_coords(base_cell: Cell, draw_size: Size) -> Vec2 {
    // Convert the anchor (bottom tile for buildings) to isometric coordinates:
    let mut tile_iso_coords = coords::cell_to_iso(base_cell, BASE_TILE_SIZE);

    // Adjust to center the unit sprite:
    tile_iso_coords.x += (BASE_TILE_SIZE.width / 2) - (draw_size.width / 2);
    tile_iso_coords.y -= draw_size.height - (BASE_TILE_SIZE.height / 2);

    tile_iso_coords.to_vec2()
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

impl<'tile_sets> BlockerTile<'tile_sets> {
    fn new(blocker_cell: Cell,
           owner_cell: Cell,
           layer: &TileMapLayer<'tile_sets>) -> Self {
        Self {
            layer: UnsafeWeakRef::new(layer),
            cell: blocker_cell,
            owner_cell,
        }
    }

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
    #[inline]
    fn set_flags(&mut self, _current_flags: &mut TileFlags, new_flags: TileFlags, value: bool) {
        // Propagate back to owner tile:
        self.owner_mut().set_flags(new_flags, value);
    }

    #[inline] fn set_base_cell(&mut self, _cell: Cell) { panic!("Not implemented for BlockerTile!"); }
    #[inline] fn set_iso_coords_f32(&mut self, _iso_coords: Vec2) { panic!("Not implemented for BlockerTile!"); }

    #[inline] fn game_state_handle(&self) -> GameStateHandle { self.owner().game_state_handle() }
    #[inline] fn set_game_state_handle(&mut self, handle: GameStateHandle) { self.owner_mut().set_game_state_handle(handle); }

    #[inline] fn z_sort_key(&self) -> i32 { self.owner().z_sort_key() }
    #[inline] fn iso_coords_f32(&self) -> Vec2 { self.owner().iso_coords_f32() }

    #[inline] fn actual_base_cell(&self) -> Cell { self.cell }
    #[inline] fn cell_range(&self) -> CellRange { self.owner().cell_range() }

    #[inline] fn tile_def(&self) -> &'tile_sets TileDef { self.owner().tile_def() }
    #[inline] fn is_valid(&self) -> bool { self.cell.is_valid() && self.owner_cell.is_valid() && self.owner().is_valid() }

    // Animations:
    #[inline] fn anim_state_ref(&self) -> &TileAnimState { self.owner().anim_state_ref() }
    #[inline] fn anim_state_mut_ref(&mut self) -> &mut TileAnimState { self.owner_mut().anim_state_mut_ref() }
}

// ----------------------------------------------
// Tile impl
// ----------------------------------------------

impl<'tile_sets> Tile<'tile_sets> {
    fn new(cell: Cell,
           tile_def: &'tile_sets TileDef,
           layer: &TileMapLayer<'tile_sets>) -> Self {

        let (z_sort_key, archetype) = match layer.kind() {
            TileMapLayerKind::Terrain => {
                debug_assert!(tile_def.kind() == TileKind::Terrain); // Only Terrain.
                let terrain = TerrainTile::new(cell, tile_def);
                (terrain.z_sort_key(), TileArchetype::new_terrain(terrain))
            },
            TileMapLayerKind::Objects => {
                debug_assert!(tile_def.kind().intersects(TileKind::Object)); // Object | Building, Prop, etc...
                let object = ObjectTile::new(cell, tile_def, layer);
                (object.z_sort_key(), TileArchetype::new_object(object))
            }
        };

        Self {
            kind: tile_def.kind(),
            flags: tile_def.flags(),
            variation_index: 0,
            z_sort_key,
            archetype
        }
    }

    fn new_blocker(blocker_cell: Cell,
                   owner_cell: Cell,
                   owner_kind: TileKind,
                   owner_flags: TileFlags,
                   layer: &TileMapLayer<'tile_sets>) -> Self {
        debug_assert!(owner_kind == TileKind::Object | TileKind::Building);
        Self {
            kind: TileKind::Object | TileKind::Blocker,
            flags: owner_flags,
            variation_index: 0, // unused
            z_sort_key:      0, // unused
            archetype: TileArchetype::new_blocker(BlockerTile::new(blocker_cell, owner_cell, layer))
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
        !self.kind.is_empty() && delegate_to_archetype!(self, is_valid)
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
    pub fn occupies_multiple_cells(&self) -> bool {
        self.tile_def().occupies_multiple_cells()
    }

    #[inline]
    pub fn z_sort_key(&self) -> i32 {
        self.z_sort_key
    }

    #[inline]
    pub fn set_z_sort_key(&mut self, z_sort_key: i32) {
        self.z_sort_key = z_sort_key;
    }

    #[inline]
    pub fn iso_coords(&self) -> IsoPoint {
        let coords_f32 = delegate_to_archetype!(self, iso_coords_f32);
        IsoPoint::new(coords_f32.x as i32, coords_f32.y as i32)
    }

    #[inline]
    pub fn iso_coords_f32(&self) -> Vec2 {
        delegate_to_archetype!(self, iso_coords_f32)
    }

    #[inline]
    pub fn set_iso_coords(&mut self, iso_coords: IsoPoint) {
        let coords_f32 = iso_coords.to_vec2();
        self.set_iso_coords_f32(coords_f32);
    }

    #[inline]
    pub fn set_iso_coords_f32(&mut self, iso_coords: Vec2) {
        // Native internal format is f32.
        delegate_to_archetype!(self, set_iso_coords_f32, iso_coords);

        // Terrain z-sort is derived from iso coords. For Objects it
        // is derived from the cell, so no need to update it here.
        if self.is(TileKind::Terrain) {
            let new_z_sort_key = delegate_to_archetype!(self, z_sort_key);
            self.set_z_sort_key(new_z_sort_key);
        }
    }

    #[inline]
    pub fn screen_rect(&self, transform: &WorldToScreenTransform) -> Rect {
        let draw_size = self.draw_size();
        let iso_position = self.iso_coords_f32();
        coords::iso_to_screen_rect_f32(iso_position, draw_size, transform)
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
        self.variation_index.into()
    }

    #[inline]
    pub fn set_variation_index(&mut self, index: usize) {
        self.variation_index =
            index.min(self.variation_count() - 1).try_into()
                .expect("Value cannot fit into a u16!"); 
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
        let anim_state = self.anim_state_mut_ref();
        let new_anim_set_index: u16 = index.min(max_index).try_into().expect("Anim Set index must be <= u16::MAX!");
        if new_anim_set_index != anim_state.anim_set_index {
            anim_state.anim_set_index = new_anim_set_index;
            anim_state.frame_index = 0;
            anim_state.frame_play_time_secs = 0.0;
        }
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
    fn update_anim(&mut self, delta_time_secs: Seconds) {
        if !self.is_animated_archetype() {
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

            let anim_state = self.anim_state_mut_ref();
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

    #[inline]
    fn is_animated_archetype(&self) -> bool {
        !self.is(TileKind::Terrain | TileKind::Blocker)
    }

    #[inline]
    fn anim_state_ref(&self) -> &TileAnimState {
        delegate_to_archetype!(self, anim_state_ref)
    }

    #[inline]
    fn anim_state_mut_ref(&mut self) -> &mut TileAnimState {
        delegate_to_archetype!(self, anim_state_mut_ref)
    }

    #[inline]
    fn set_base_cell(&mut self, cell: Cell) {
        // We would have to update all blocker cells here and point its owner cell back to the new cell.
        assert!(!self.occupies_multiple_cells(), "This does not support multi-cell tiles yet!");

        // This will also update the cached iso coords in the archetype.
        delegate_to_archetype!(self, set_base_cell, cell);

        // Z-sort key is derived from cell and iso coords so it needs to be recomputed.
        let new_z_sort_key = delegate_to_archetype!(self, z_sort_key);
        self.set_z_sort_key(new_z_sort_key);
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
            layer_kind,
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

        // Keeping it within one CPU cache line for best runtime performance.
        debug_assert!(std::mem::size_of::<Tile>() == 64);

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
        } else {
            // Else layer is left empty. Pre-reserve some memory for future tile placements.
            layer.pool.slab.reserve(256);
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
        where F: FnMut(&Tile<'tile_sets>)
    {
        for (_, tile) in &self.pool.slab {
            if tile.is(tile_kinds) {
                visitor_fn(tile);
            }
        }
    }

    #[inline]
    pub fn for_each_tile_mut<F>(&mut self, tile_kinds: TileKind, mut visitor_fn: F)
        where F: FnMut(&mut Tile<'tile_sets>)
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
    fn update_anims(&mut self, visible_range: CellRange, delta_time_secs: Seconds) {
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
            size_in_cells,
            layers: ArrayVec::new(),
        };
        tile_map.reset(fill_with_def);
        tile_map
    }

    pub fn with_terrain_tile(size_in_cells: Size,
                             tile_sets: &'tile_sets TileSets,
                             category_name: StrHashPair,
                             tile_name: StrHashPair) -> Self {

        let fill_with_def = tile_sets.find_tile_def_by_hash(
            TileMapLayerKind::Terrain,
            category_name.hash,
            tile_name.hash);

        Self::new(size_in_cells, fill_with_def)
    }

    pub fn reset(&mut self, fill_with_def: Option<&'tile_sets TileDef>) {
        self.layers.clear();
        invoke_map_reset_callback(self);

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
        where F: FnMut(&Tile<'tile_sets>)
    {
        let layer = self.layer(layer_kind);
        layer.for_each_tile(tile_kinds, visitor_fn);
    }

    #[inline]
    pub fn for_each_tile_mut<F>(&mut self, layer_kind: TileMapLayerKind, tile_kinds: TileKind, visitor_fn: F)
        where F: FnMut(&mut Tile<'tile_sets>)
    {
        let layer = self.layer_mut(layer_kind);
        layer.for_each_tile_mut(tile_kinds, visitor_fn);
    }

    #[inline]
    pub fn update_anims(&mut self, visible_range: CellRange, delta_time_secs: Seconds) {
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
                          tile_def_to_place: &'tile_sets TileDef) -> Result<&mut Tile<'tile_sets>, String> {
        self.try_place_tile_in_layer(
            target_cell,
            tile_def_to_place.layer_kind(), // Guess layer from TileDef.
            tile_def_to_place)
    }

    #[inline]
    pub fn try_place_tile_in_layer(&mut self,
                                   target_cell: Cell,
                                   layer_kind: TileMapLayerKind,
                                   tile_def_to_place: &'tile_sets TileDef) -> Result<&mut Tile<'tile_sets>, String> {

        let layer = self.layer_mut(layer_kind);
        let prev_pool_capacity = layer.pool_capacity();

        placement::try_place_tile_in_layer(layer, target_cell, tile_def_to_place)
            .map(|(tile, new_pool_capacity)| {
                let did_reallocate = new_pool_capacity != prev_pool_capacity;
                invoke_tile_placed_callback(tile, did_reallocate);
                tile
            })
    }

    #[inline]
    pub fn try_place_tile_at_cursor(&mut self,
                                    cursor_screen_pos: Vec2,
                                    transform: &WorldToScreenTransform,
                                    tile_def_to_place: &'tile_sets TileDef) -> Result<&mut Tile<'tile_sets>, String> {

        let prev_pool_capacity = {
            let layer = self.layer(tile_def_to_place.layer_kind());
            layer.pool_capacity()
        };

        placement::try_place_tile_at_cursor(self, cursor_screen_pos, transform, tile_def_to_place)
            .map(|(tile, new_pool_capacity)| {
                let did_reallocate = new_pool_capacity != prev_pool_capacity;
                invoke_tile_placed_callback(tile, did_reallocate);
                tile
            })
    }

    #[inline]
    pub fn try_clear_tile_from_layer(&mut self,
                                     target_cell: Cell,
                                     layer_kind: TileMapLayerKind) -> Result<(), String> {

        if has_removing_tile_callback() {
            if let Some(tile) = self.try_tile_from_layer_mut(target_cell, layer_kind) {
                invoke_removing_tile_callback(tile);
            }
        }

        placement::try_clear_tile_from_layer(self.layer_mut(layer_kind), target_cell)
    }

    #[inline]
    pub fn try_clear_tile_at_cursor(&mut self,
                                    cursor_screen_pos: Vec2,
                                    transform: &WorldToScreenTransform) -> Result<(), String> {

        if has_removing_tile_callback() {
            for layer_kind in TileMapLayerKind::iter().rev() {
                let target_cell = self.find_exact_cell_for_point(layer_kind, cursor_screen_pos, transform);
                if let Some(tile) = self.try_tile_from_layer_mut(target_cell, layer_kind) {
                    invoke_removing_tile_callback(tile);
                    break;
                }
            }
        }

        placement::try_clear_tile_at_cursor(self, cursor_screen_pos, transform)
    }

    // Move tile from one cell to another if destination is free.
    pub fn try_move_tile(&mut self, from: Cell, to: Cell, layer_kind: TileMapLayerKind) -> bool {
        if from == to {
            return false;
        }

        if !self.is_cell_within_bounds(from) ||
           !self.is_cell_within_bounds(to) {
            return false;
        }

        let layer = self.layer_mut(layer_kind);
        if layer.try_tile(from).is_none() {
            return false; // No tile at 'from' cell!
        }
        if layer.try_tile(to).is_some() {
            return false; // 'to' tile is occupied!
        }

        let from_cell_index = layer.pool.map_cell_to_index(from);
        let from_slab_index = layer.pool.cell_to_slab_idx[from_cell_index];
        debug_assert!(from_slab_index != INVALID_TILE_INDEX); // Can't be empty, we have the 'from' tile.

        let to_cell_index = layer.pool.map_cell_to_index(to);
        let to_slab_index = layer.pool.cell_to_slab_idx[to_cell_index];
        debug_assert!(to_slab_index == INVALID_TILE_INDEX); // Should be empty, we've checked the destination is free.

        // Swap indices.
        layer.pool.cell_to_slab_idx[from_cell_index] = to_slab_index;
        layer.pool.cell_to_slab_idx[to_cell_index]   = from_slab_index;

        // Update cached tile states:
        let tile = &mut layer.pool.slab[from_slab_index];
        tile.set_base_cell(to);

        true
    }

    // ----------------------
    // Tile selection:
    // ----------------------

    #[inline]
    pub fn update_selection(&mut self,
                            selection: &mut TileSelection,
                            cursor_screen_pos: Vec2,
                            transform: &WorldToScreenTransform,
                            placement_op: PlacementOp) {
        let map_size_in_cells = self.size_in_cells();
        selection.update(
            self.layers_mut(),
            map_size_in_cells,
            cursor_screen_pos,
            transform, 
            placement_op);
    }

    #[inline]
    pub fn clear_selection(&mut self, selection: &mut TileSelection) {
        selection.clear(self.layers_mut());
    }

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

    pub fn topmost_tile_at_cursor(&self,
                                  cursor_screen_pos: Vec2,
                                  transform: &WorldToScreenTransform) -> Option<&Tile<'tile_sets>> {

        debug_assert!(transform.is_valid());

        // Find topmost layer tile under the target cell.
        for layer_kind in TileMapLayerKind::iter().rev() {
            let layer = self.layer(layer_kind);

            let target_cell = layer.find_exact_cell_for_point(
                cursor_screen_pos,
                transform);

            let tile = layer.try_tile(target_cell);
            if tile.is_some() {
                return tile;
            }
        }
        None
    }
}

// ----------------------------------------------
// Optional callbacks used for editing and dev
// ----------------------------------------------

type TilePlacedCallback   = std::cell::OnceCell<Box<dyn Fn(&mut Tile, bool) + 'static>>;
type RemovingTileCallback = std::cell::OnceCell<Box<dyn Fn(&mut Tile) + 'static>>;
type MapResetCallback     = std::cell::OnceCell<Box<dyn Fn(&mut TileMap) + 'static>>;

std::thread_local! {
    // Called *after* a tile is placed with the new tile instance.
    static ON_TILE_PLACED_CALLBACK: TilePlacedCallback = const { TilePlacedCallback::new() };

    // Called *before* the tile is removed with the instance about to be removed.
    static ON_REMOVING_TILE_CALLBACK: RemovingTileCallback = const { RemovingTileCallback::new() };

    // Called *after* the TileMap has been reset. Any existing tile references/cells are invalidated.
    static ON_MAP_RESET_CALLBACK: MapResetCallback = const { MapResetCallback::new() };
}

// The callbacks can only be set once and will stay set globally (per-thread).

pub fn set_tile_placed_callback(callback: impl Fn(&mut Tile, bool) + 'static) {
    ON_TILE_PLACED_CALLBACK.with(|cb| {
        cb.set(Box::new(callback)).unwrap_or_else(|_| panic!("ON_TILE_PLACED_CALLBACK was already set!"));
    });
}

pub fn set_removing_tile_callback(callback: impl Fn(&mut Tile) + 'static) {
    ON_REMOVING_TILE_CALLBACK.with(|cb| {
        cb.set(Box::new(callback)).unwrap_or_else(|_| panic!("ON_REMOVING_TILE_CALLBACK was already set!"));
    });
}

pub fn set_map_reset_callback(callback: impl Fn(&mut TileMap) + 'static) {
    ON_MAP_RESET_CALLBACK.with(|cb| {
        cb.set(Box::new(callback)).unwrap_or_else(|_| panic!("ON_MAP_RESET_CALLBACK was already set!"));
    });
}

#[inline]
fn invoke_tile_placed_callback(tile: &mut Tile, did_reallocate: bool) {
    ON_TILE_PLACED_CALLBACK.with(|cb| {
        if let Some(callback) = cb.get() {
            callback(tile, did_reallocate);
        }
    });
}

#[inline]
fn invoke_removing_tile_callback(tile: &mut Tile) {
    ON_REMOVING_TILE_CALLBACK.with(|cb| {
        if let Some(callback) = cb.get() {
            callback(tile);
        }
    });
}

#[inline]
fn invoke_map_reset_callback(tile_map: &mut TileMap) {
    ON_MAP_RESET_CALLBACK.with(|cb| {
        if let Some(callback) = cb.get() {
            callback(tile_map);
        }
    });
}

#[inline]
fn has_removing_tile_callback() -> bool {
    ON_REMOVING_TILE_CALLBACK.with(|cb| -> bool {
        cb.get().is_some()
    })
}
