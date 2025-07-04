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
    index: i32,
    kind:  i32,
}

impl GameStateHandle {
    #[inline]
    pub fn new(index: usize, kind: i32) -> Self {
        debug_assert!(kind >= 0);
        Self {
            index: index.try_into().expect("Value cannot fit into an i32"),
            kind:  kind
        }
    }

    #[inline]
    pub const fn invalid() -> Self {
        Self { index: -1, kind: -1 }
    }

    #[inline]
    pub fn is_valid(&self) -> bool {
        self.index >= 0 && self.kind >= 0
    }

    #[inline]
    pub fn index(&self) -> usize {
        debug_assert!(self.is_valid());
        self.index as usize
    }

    #[inline]
    pub fn kind(&self) -> i32 {
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

#[derive(Default)]
struct TileAnimState {
    anim_set_index: u16,
    frame_index: u16,
    frame_play_time_secs: f32,
}

// ----------------------------------------------
// Tile
// ----------------------------------------------

// Tile is tied to the lifetime of the TileSets that owns the underlying TileDef.
// We also may keep a reference to the owning TileMapLayer for building blockers and objects.
pub struct Tile<'tile_sets> {
    kind: TileKind,
    flags: TileFlags,
    archetype: TileArchetype<'tile_sets>,
}

enum TileArchetype<'tile_sets> {
    Terrain {
        // Terrain tiles always occupy a single cell (of BASE_TILE_SIZE).
        cell: Cell,
        def: &'tile_sets TileDef,
    },
    Object {
        // Buildings can occupy multiple cells. `cell_range.start` is the start or "base" cell.
        cell_range: CellRange,
        def: &'tile_sets TileDef,
        variation_index: u32,
        anim_state: TileAnimState,
        game_state: GameStateHandle,

        // Owning layer so we can propagate flags from a building to all of its blocker tiles.
        // SAFETY: This ref will always be valid as long as the Tile instance is, since the Tile
        // belongs to its parent layer.
        layer: UnsafeWeakRef<TileMapLayer<'tile_sets>>,
    },
    Blocker {
        // Building blocker tiles occupy a single cell and have a backreference to the owner start cell.
        // `owner_cell` must be always valid.
        cell: Cell,
        owner_cell: Cell,

        // Weak reference to owning map layer so we can seamlessly resolve blockers into buildings.
        layer: UnsafeWeakRef<TileMapLayer<'tile_sets>>,
    }
}

impl<'tile_sets> Tile<'tile_sets> {
    fn new(cell: Cell,
           tile_def: &'tile_sets TileDef,
           layer: &TileMapLayer<'tile_sets>) -> Self {

        let archetype = match layer.kind() {
            TileMapLayerKind::Terrain => {
                TileArchetype::Terrain {
                    cell: cell,
                    def: tile_def,
                }
            },
            TileMapLayerKind::Objects => {
                TileArchetype::Object {
                    cell_range: tile_def.calc_footprint_cells(cell),
                    def: tile_def,
                    variation_index: 0,
                    anim_state: TileAnimState::default(),
                    game_state: GameStateHandle::default(),
                    layer: UnsafeWeakRef::new(layer),
                }
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
        Self {
            kind: TileKind::Blocker | owner_kind,
            flags: owner_flags,
            archetype: TileArchetype::Blocker {
                cell: blocker_cell,
                owner_cell: owner_cell,
                layer: UnsafeWeakRef::new(layer),
            },
        }
    }

    #[inline]
    pub fn set_flags(&mut self, flags: TileFlags, value: bool) {
        match &mut self.archetype {
            TileArchetype::Terrain { .. } => {
                self.flags.set(flags, value);
            },
            TileArchetype::Object { cell_range, layer, .. } => {
                // Propagate flags to any child blockers in its cell range:
                for cell in cell_range.iter() {
                    let tile = layer.tile_mut(cell);
                    tile.flags.set(flags, value);
                }
            },
            TileArchetype::Blocker { owner_cell, layer, .. } => {
                // Propagate back to owner tile:
                layer.find_blocker_owner_mut(*owner_cell).set_flags(flags, value);
            }
        }

        debug_assert!(self.has_flags(flags) == value);
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
        match self.archetype {
            TileArchetype::Terrain { cell, def } => {
                !self.kind.is_empty() && cell.is_valid() && def.is_valid()
            },
            TileArchetype::Object  { cell_range, def, .. } => {
                !self.kind.is_empty() && cell_range.is_valid() && def.is_valid()
            },
            TileArchetype::Blocker { cell, owner_cell, layer } => {
                if !cell.is_valid() || !owner_cell.is_valid() {
                    return false;
                }
                layer.find_blocker_owner(owner_cell).is_valid()
            }
        }
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
        match &self.archetype {
            TileArchetype::Terrain { .. } => GameStateHandle::invalid(), // Terrain tiles cannot store game state.
            TileArchetype::Object  { game_state, .. } => *game_state,
            TileArchetype::Blocker { owner_cell, layer, .. } => {
                layer.find_blocker_owner(*owner_cell).game_state_handle()
            }
        }
    }

    #[inline]
    pub fn set_game_state_handle(&mut self, handle: GameStateHandle) {
        match &mut self.archetype {
            TileArchetype::Terrain { .. } => {}, // Terrain tiles cannot store game state.
            TileArchetype::Object  { game_state, .. } => *game_state = handle,
            TileArchetype::Blocker { owner_cell, layer, .. } => {
                layer.find_blocker_owner_mut(*owner_cell).set_game_state_handle(handle)
            }
        }
    }

    #[inline]
    pub fn name(&self) -> &'tile_sets str {
        match self.archetype {
            TileArchetype::Terrain { def, .. } => &def.name,
            TileArchetype::Object  { def, .. } => &def.name,
            TileArchetype::Blocker { owner_cell, layer, .. } => {
                layer.find_blocker_owner(owner_cell).name()
            }
        }
    }

    #[inline]
    pub fn logical_size(&self) -> Size {
        match self.archetype {
            TileArchetype::Terrain { .. } => BASE_TILE_SIZE, // Terrain tile logical size is fixed.
            TileArchetype::Object  { def, .. } => def.logical_size,
            TileArchetype::Blocker { owner_cell, layer, .. } => {
                layer.find_blocker_owner(owner_cell).logical_size()             
            }
        }
    }

    #[inline]
    pub fn draw_size(&self) -> Size {
        match self.archetype {
            TileArchetype::Terrain { def, .. } => def.draw_size, // Terrain tile draw size can be customized.
            TileArchetype::Object  { def, .. } => def.draw_size,
            TileArchetype::Blocker { owner_cell, layer, .. } => {
                layer.find_blocker_owner(owner_cell).draw_size()             
            }
        }
    }

    #[inline]
    pub fn size_in_cells(&self) -> Size {
        match self.archetype {
            TileArchetype::Terrain { .. } => Size::new(1, 1), // Always 1x1
            TileArchetype::Object  { def, .. } => def.size_in_cells(),
            TileArchetype::Blocker { owner_cell, layer, .. } => {
                layer.find_blocker_owner(owner_cell).size_in_cells()             
            }
        }
    }

    #[inline]
    pub fn tint_color(&self) -> Color {
        match self.archetype {
            TileArchetype::Terrain { def, .. } => def.color,
            TileArchetype::Object  { def, .. } => def.color,
            TileArchetype::Blocker { owner_cell, layer, .. } => {
                layer.find_blocker_owner(owner_cell).tint_color()             
            }
        }
    }

    #[inline]
    pub fn tile_def(&self) -> &'tile_sets TileDef {
        match self.archetype {
            TileArchetype::Terrain { def, .. } => def,
            TileArchetype::Object  { def, .. } => def,
            TileArchetype::Blocker { owner_cell, layer, .. } => {
                layer.find_blocker_owner(owner_cell).tile_def()
            }
        }
    }

    #[inline]
    pub fn has_multi_cell_footprint(&self) -> bool {
        match self.archetype {
            TileArchetype::Terrain { .. } => false, // Always 1x1
            TileArchetype::Object  { def, .. } => def.has_multi_cell_footprint(),
            TileArchetype::Blocker { owner_cell, layer, .. } => {
                layer.find_blocker_owner(owner_cell).has_multi_cell_footprint()             
            }
        }
    }

    #[inline]
    pub fn calc_z_sort(&self) -> i32 {
        match self.archetype {
            TileArchetype::Terrain { cell, .. } => {
                coords::cell_to_iso(cell, BASE_TILE_SIZE).y
            },
            TileArchetype::Object { cell_range, def, .. } => {
                coords::cell_to_iso(cell_range.start, BASE_TILE_SIZE).y - def.logical_size.height
            },
            TileArchetype::Blocker { owner_cell, layer, .. } => {
                layer.find_blocker_owner(owner_cell).calc_z_sort()    
            }
        }
    }

    #[inline]
    pub fn calc_adjusted_iso_coords(&self) -> IsoPoint {
        match self.archetype {
            TileArchetype::Terrain { cell, .. } => {
                // No position adjustments needed for terrain.
                coords::cell_to_iso(cell, BASE_TILE_SIZE)
            },
            TileArchetype::Object { cell_range, def, .. } => {
                // Convert the anchor (bottom tile for buildings) to isometric coordinates:
                let mut tile_iso_coords = coords::cell_to_iso(cell_range.start, BASE_TILE_SIZE);

                if self.is(TileKind::Building | TileKind::Prop | TileKind::Vegetation) {
                    // Center the sprite horizontally:
                    tile_iso_coords.x += (BASE_TILE_SIZE.width / 2) - (def.logical_size.width / 2);

                    // Vertical offset: move up the full sprite height *minus* 1 tile's height.
                    // Since the anchor is the bottom tile, and cell_to_iso gives us the *bottom*,
                    // we must offset up by (image_height - one_tile_height).
                    tile_iso_coords.y -= def.draw_size.height - BASE_TILE_SIZE.height;
                } else if self.is(TileKind::Unit) {
                    // Adjust to center the unit sprite:
                    tile_iso_coords.x += (BASE_TILE_SIZE.width / 2) - (def.draw_size.width / 2);
                    tile_iso_coords.y -= def.draw_size.height - (BASE_TILE_SIZE.height / 2);
                }

                tile_iso_coords
            },
            TileArchetype::Blocker { owner_cell, layer, .. } => {
                layer.find_blocker_owner(owner_cell).calc_adjusted_iso_coords()             
            }
        }
    }

    #[inline]
    pub fn calc_screen_rect(&self, transform: &WorldToScreenTransform) -> Rect {
        let draw_size = self.draw_size();
        let iso_position = self.calc_adjusted_iso_coords();
        coords::iso_to_screen_rect(iso_position, draw_size, transform)
    }

    #[inline]
    pub fn base_cell(&self) -> Cell {
        self.cell_range().start
    }

    // Base cell without resolving blocker tiles into their owner cell.
    #[inline]
    pub fn actual_base_cell(&self) -> Cell {
        match self.archetype {
            TileArchetype::Terrain { cell, .. } => cell,
            TileArchetype::Object  { cell_range, .. } => cell_range.start,
            TileArchetype::Blocker { cell, .. } => cell
        }
    }

    #[inline]
    pub fn cell_range(&self) -> CellRange {
        match self.archetype {
            TileArchetype::Terrain { cell, .. } => CellRange::new(cell, cell), // Always 1x1
            TileArchetype::Object  { cell_range, .. } => cell_range,
            TileArchetype::Blocker { owner_cell, layer, .. } => {
                /*
                Buildings have an origin tile and zero or more associated blockers
                if they occupy multiple tiles, so here we might need to back-track
                to the origin of the building tile from a blocker tile.

                For instance, a 2x2 house tile `H` will have the house at its origin
                cell, and 3 other blocker tiles `B` that backreference the house tile.
                +---+---+
                | B | B |
                +---+---+
                | B | H | <-- origin tile, AKA base tile
                +---+---+ 
                */
                layer.find_blocker_owner(owner_cell).cell_range()             
            }
        }
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
        let (cell, tile_size) = match self.archetype {
            TileArchetype::Terrain { cell, .. } => {
                (cell, BASE_TILE_SIZE) // Terrain tiles are fixed size.
            },
            TileArchetype::Blocker { cell, .. } => {
                (cell, BASE_TILE_SIZE) // Check against actual blocker cell rather than owner's.
            },
            TileArchetype::Object { cell_range, def, .. } => {
                (cell_range.start, def.logical_size)
            }
        };

        if coords::is_screen_point_inside_cell(screen_point,
                                               cell,
                                               tile_size,
                                               BASE_TILE_SIZE,
                                               transform) {
            return true;
        }
        false
    }

    pub fn category_name(&self, tile_sets: &'tile_sets TileSets) -> &'tile_sets str {
        match self.archetype {
            TileArchetype::Terrain { def, .. } => {
                tile_sets.find_category_for_tile_def(def).map_or("<none>", |cat| &cat.name)
            },
            TileArchetype::Object { def, .. } => {
                tile_sets.find_category_for_tile_def(def).map_or("<none>", |cat| &cat.name)
            },
            TileArchetype::Blocker { owner_cell, layer, .. } => {
                layer.find_blocker_owner(owner_cell).category_name(tile_sets)
            }
        }
    }

    pub fn try_get_editable_tile_def(&self, tile_sets: &'tile_sets TileSets) -> Option<&'tile_sets mut TileDef> {
        match self.archetype {
            TileArchetype::Terrain { def, .. } => tile_sets.try_get_editable_tile_def(def),
            TileArchetype::Object  { def, .. } => tile_sets.try_get_editable_tile_def(def),
            TileArchetype::Blocker { owner_cell, layer, .. } => {
                layer.find_blocker_owner(owner_cell).try_get_editable_tile_def(tile_sets)
            }
        }
    }

    // ----------------------
    // Variations:
    // ----------------------

    #[inline]
    pub fn has_variations(&self) -> bool {
        match self.archetype {
            TileArchetype::Terrain { .. } => false,
            TileArchetype::Object  { def, .. } => def.variations.len() > 1,
            TileArchetype::Blocker { owner_cell, layer, .. } => {
                layer.find_blocker_owner(owner_cell).has_variations()
            }
        }
    }

    #[inline]
    pub fn variation_count(&self) -> usize {
        match self.archetype {
            TileArchetype::Terrain { .. } => 0,
            TileArchetype::Object  { def, .. } => def.variations.len(),
            TileArchetype::Blocker { owner_cell, layer, .. } => {
                layer.find_blocker_owner(owner_cell).variation_count()
            }
        }
    }

    #[inline]
    pub fn variation_name(&self) -> &'tile_sets str {
        match self.archetype {
            TileArchetype::Terrain { .. } => "",
            TileArchetype::Object  { def, variation_index, .. } => def.variation_name(variation_index as usize),
            TileArchetype::Blocker { owner_cell, layer, .. } => {
                layer.find_blocker_owner(owner_cell).variation_name()
            }
        }
    }

    #[inline]
    pub fn variation_index(&self) -> usize {
        match self.archetype {
            TileArchetype::Terrain { .. } => 0,
            TileArchetype::Object  { variation_index, .. } => variation_index as usize,
            TileArchetype::Blocker { owner_cell, layer, .. } => {
                layer.find_blocker_owner(owner_cell).variation_index()
            }
        }
    }

    #[inline]
    pub fn set_variation_index(&mut self, index: usize) {
        match &mut self.archetype {
            TileArchetype::Terrain { .. } => {},
            TileArchetype::Object { def, variation_index, .. } => {
                *variation_index = index.min(def.variations.len() - 1) as u32;
            }
            TileArchetype::Blocker { owner_cell, layer, .. } => {
                layer.find_blocker_owner_mut(*owner_cell).set_variation_index(index);
            }
        }
    }

    // ----------------------
    // Animations:
    // ----------------------

    #[inline]
    pub fn anim_sets_count(&self) -> usize {
        match self.archetype {
            TileArchetype::Terrain { .. } => 0,
            TileArchetype::Object  { def, variation_index, .. } => def.anim_sets_count(variation_index as usize),
            TileArchetype::Blocker { owner_cell, layer, .. } => {
                layer.find_blocker_owner(owner_cell).anim_sets_count()
            }
        }
    }

    #[inline]
    pub fn anim_set_name(&self) -> &'tile_sets str {
        match &self.archetype {
            TileArchetype::Terrain { .. } => "",
            TileArchetype::Object { def, variation_index, anim_state, .. } => {
                def.anim_set_name(*variation_index as usize, anim_state.anim_set_index as usize)
            },
            TileArchetype::Blocker { owner_cell, layer, .. } => {
                layer.find_blocker_owner(*owner_cell).anim_set_name()
            }
        }
    }

    #[inline]
    pub fn anim_frames_count(&self) -> usize {
        match self.archetype {
            TileArchetype::Terrain { .. } => 0,
            TileArchetype::Object { def, variation_index, .. } => {
                def.anim_frames_count(variation_index as usize)
            },
            TileArchetype::Blocker { owner_cell, layer, .. } => {
                layer.find_blocker_owner(owner_cell).anim_frames_count()
            }
        }
    }

    #[inline]
    pub fn anim_set_index(&self) -> usize {
        match &self.archetype {
            TileArchetype::Terrain { .. } => 0,
            TileArchetype::Object  { anim_state, .. } => anim_state.anim_set_index as usize,
            TileArchetype::Blocker { owner_cell, layer, .. } => {
                layer.find_blocker_owner(*owner_cell).anim_set_index()
            }
        }
    }

    #[inline]
    pub fn anim_frame_index(&self) -> usize {
        match &self.archetype {
            TileArchetype::Terrain { .. } => 0,
            TileArchetype::Object  { anim_state, .. } => anim_state.frame_index as usize,
            TileArchetype::Blocker { owner_cell, layer, .. } => {
                layer.find_blocker_owner(*owner_cell).anim_frame_index()
            }
        }
    }

    #[inline]
    pub fn anim_frame_play_time_secs(&self) -> f32 {
        match &self.archetype {
            TileArchetype::Terrain { .. } => 0.0,
            TileArchetype::Object  { anim_state, .. } => anim_state.frame_play_time_secs,
            TileArchetype::Blocker { owner_cell, layer, .. } => {
                layer.find_blocker_owner(*owner_cell).anim_frame_play_time_secs()
            }
        }
    }

    #[inline]
    pub fn anim_frame_tex_info(&self) -> Option<&'tile_sets TileTexInfo> {
        match &self.archetype {
            TileArchetype::Terrain { def, .. } => {
                if let Some(anim_set) = def.anim_set_by_index(0, 0) {
                    return Some(&anim_set.frames[0].tex_info);
                }
                None          
            },
            TileArchetype::Object { def, variation_index, anim_state, .. } => {
                if let Some(anim_set) = def.anim_set_by_index(*variation_index as usize, anim_state.anim_set_index as usize) {
                    let anim_frame_index = anim_state.frame_index as usize;
                    if anim_frame_index < anim_set.frames.len() {
                        return Some(&anim_set.frames[anim_frame_index].tex_info);
                    }
                }
                None
            },
            TileArchetype::Blocker { owner_cell, layer, .. } => {
                layer.find_blocker_owner(*owner_cell).anim_frame_tex_info()
            }
        }
    }

    #[inline]
    pub fn has_animations(&self) -> bool {
        match &self.archetype {
            TileArchetype::Terrain { .. } => false,
            TileArchetype::Object { def, variation_index, anim_state, .. } => {
                if let Some(anim_set) = def.anim_set_by_index(*variation_index as usize, anim_state.anim_set_index as usize) {
                    if anim_set.frames.len() > 1 {
                        return true;
                    }
                }
                false
            },
            TileArchetype::Blocker { owner_cell, layer, .. } => {
                layer.find_blocker_owner(*owner_cell).has_animations()
            }
        }
    }

    fn update_anim(&mut self, delta_time_secs: f32) {
        if self.is(TileKind::Terrain | TileKind::Blocker) {
            return; // Not animated.
        }

        if let TileArchetype::Object {
            def,
            variation_index,
            anim_state,
            ..
        } = &mut self.archetype {
            if let Some(anim_set) = 
                def.anim_set_by_index(*variation_index as usize, anim_state.anim_set_index as usize) {

                if anim_set.frames.len() <= 1 {
                    // Single frame sprite, nothing to update.
                    return;
                }

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
        // NOTE: No need to backtrack from blocker to owner tile here.
        // Owner tile will have its animation state updated if it is visible. 
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
