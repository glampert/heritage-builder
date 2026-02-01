use rand::Rng;
use std::path::PathBuf;
use smallvec::SmallVec;
use strum::{EnumCount, IntoEnumIterator, EnumProperty};
use strum_macros::{EnumCount, EnumIter, EnumProperty};

use super::{
    TileKind, TileMap, TileMapLayerKind,
    sets::{TileDef, TileSector},
    camera::Camera, water, road,
};

use crate::{
    singleton,
    engine::time::Seconds,
    save::{PreLoadContext, PostLoadContext},
    app::input::{InputSystem, InputAction, MouseButton},
    ui::{self, UiSystem, UiTextureHandle, UiFontScale, widgets::UiWidgetContext},
    render::{RenderSystem, TextureCache, TextureFilter, TextureWrapMode, TextureHandle, TextureSettings},
    utils::{
        platform::paths,
        Color, Rect, RectEdges, Size, Vec2,
        coords::{self, Cell, CellF32, IsoPointF32, IsoDiamond, WorldToScreenTransform},
    },
};

// ----------------------------------------------
// Constants
// ----------------------------------------------

// Minimap only displays terrain tiles and these object kinds.
const MINIMAP_OBJECT_TILE_KINDS: TileKind =
    TileKind::from_bits_retain(
        TileKind::Building.bits()
        | TileKind::Blocker.bits()
        | TileKind::Rocks.bits()
        | TileKind::Vegetation.bits()
    );

// ----------------------------------------------
// MinimapTileColor
// ----------------------------------------------

#[repr(C)]
#[derive(Copy, Clone)]
struct MinimapTileColor {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

impl MinimapTileColor {
    // Default:
    const BLACK:                  Self = Self { r: 0,   g: 0,   b: 0,   a: 255 };

    // Terrain:
    const WATER:                  Self = Self { r: 30,  g: 100, b: 115, a: 255 }; // dark blue
    const EMPTY_LAND_1:           Self = Self { r: 112, g: 125, b: 55,  a: 255 }; // light green
    const EMPTY_LAND_2:           Self = Self { r: 100, g: 120, b: 50,  a: 255 }; // light green
    const VEGETATION_1:           Self = Self { r: 10,  g: 115, b: 25,  a: 255 }; // dark green
    const VEGETATION_2:           Self = Self { r: 25,  g: 125, b: 40,  a: 255 }; // dark green
    const ROCKS_1:                Self = Self { r: 90,  g: 85,  b: 75,  a: 255 }; // dark gray
    const ROCKS_2:                Self = Self { r: 80,  g: 75,  b: 65,  a: 255 }; // dark gray

    // Roads:
    const DIRT_ROAD:              Self = Self { r: 165, g: 122, b: 81,  a: 255 }; // light brown
    const PAVED_ROAD:             Self = Self { r: 138, g: 92,  b: 68,  a: 255 }; // dark brown

    // Building Sectors:
    const VACANT_LOT:             Self = Self { r: 210, g: 225, b: 20,  a: 255 }; // bright yellow
    const HOUSING:                Self = Self { r: 225, g: 195, b: 120, a: 255 }; // light yellow
    const FOOD_AND_FARMING:       Self = Self { r: 155, g: 170, b: 40,  a: 255 }; // olive
    const INDUSTRY_AND_RESOURCES: Self = Self { r: 170, g: 35,  b: 35,  a: 255 }; // dark red
    const SERVICES:               Self = Self { r: 80,  g: 140, b: 255, a: 255 }; // light blue
    const INFRASTRUCTURE:         Self = Self { r: 100, g: 100, b: 100, a: 255 }; // gray
    const CULTURE_AND_RELIGION:   Self = Self { r: 170, g: 60,  b: 190, a: 255 }; // purple
    const TRADE_AND_ECONOMY:      Self = Self { r: 230, g: 185, b: 40,  a: 255 }; // gold
    const BEAUTIFICATION:         Self = Self { r: 60,  g: 200, b: 110, a: 255 }; // light green

    #[inline]
    fn vacant_lot() -> Self {
        Self::VACANT_LOT
    }

    #[inline]
    fn water() -> Self {
        Self::WATER
    }

    #[inline]
    fn road(tile_def: &'static TileDef) -> Self {
        match road::kind(tile_def) {
            road::RoadKind::Dirt  => Self::DIRT_ROAD,
            road::RoadKind::Paved => Self::PAVED_ROAD,
        }
    }

    #[inline]
    fn empty_land() -> Self {
        // Alternate randomly between two similar colors
        // to give the minimap a more pleasant texture.
        if rand::rng().random_bool(0.5) {
            Self::EMPTY_LAND_1
        } else {
            Self::EMPTY_LAND_2
        }
    }

    #[inline]
    fn vegetation() -> Self {
        if rand::rng().random_bool(0.5) {
            Self::VEGETATION_1
        } else {
            Self::VEGETATION_2
        }
    }

    #[inline]
    fn rocks() -> Self {
        if rand::rng().random_bool(0.5) {
            Self::ROCKS_1
        } else {
            Self::ROCKS_2
        }
    }

    fn building(tile_def: &'static TileDef) -> Self {
        match tile_def.sector {
            TileSector::None                 => Self::BLACK,
            TileSector::Housing              => Self::HOUSING,
            TileSector::Roads                => Self::DIRT_ROAD, // NOTE: Handled elsewhere, listed here to cover all enum cases.
            TileSector::FoodAndFarming       => Self::FOOD_AND_FARMING,
            TileSector::IndustryAndResources => Self::INDUSTRY_AND_RESOURCES,
            TileSector::Services             => Self::SERVICES,
            TileSector::Infrastructure       => Self::INFRASTRUCTURE,
            TileSector::CultureAndReligion   => Self::CULTURE_AND_RELIGION,
            TileSector::TradeAndEconomy      => Self::TRADE_AND_ECONOMY,
            TileSector::Beautification       => Self::BEAUTIFICATION,
        }
    }

    fn from_tile_def(tile_def: &'static TileDef) -> Option<Self> {
        Some({
            if tile_def.path_kind.is_empty_land() {
                Self::empty_land()
            } else if tile_def.path_kind.is_vacant_lot() {
                Self::vacant_lot()
            } else if tile_def.path_kind.is_water() {
                Self::water()
            } else if tile_def.path_kind.is_road() {
                Self::road(tile_def)
            } else if tile_def.path_kind.is_rocks() {
                Self::rocks()
            } else if tile_def.path_kind.is_vegetation() {
                Self::vegetation()
            } else if tile_def.is(TileKind::Building) {
                Self::building(tile_def)
            } else {
                // Units or anything else we don't display on the minimap.
                return None;
            }
        })
    }
}

impl Default for MinimapTileColor {
    #[inline]
    fn default() -> Self {
        Self::BLACK
    }
}

// ----------------------------------------------
// MinimapTexture
// ----------------------------------------------

#[derive(Default)]
struct MinimapTexture {
    size: Size,
    pixels: Vec<MinimapTileColor>,
    handle: TextureHandle,
    need_update: bool,
    size_changed: bool,
}

impl MinimapTexture {
    fn new(size: Size) -> Self {
        let pixel_count = (size.width * size.height) as usize;
        Self {
            size,
            pixels: vec![MinimapTileColor::default(); pixel_count],
            handle: TextureHandle::invalid(),
            need_update: true,
            size_changed: false,
        }
    }

    fn memory_usage_estimate(&self) -> usize {
        self.pixels.capacity() * std::mem::size_of::<MinimapTileColor>()
    }

    fn reset<F>(&mut self, size: Size, fill_fn: F)
        where F: Fn() -> MinimapTileColor
    {
        self.need_update = true;

        if size == self.size {
            self.pixels.fill_with(fill_fn);
            return; // No change in size.
        }

        self.pixels.clear();

        let pixel_count = (size.width * size.height) as usize;
        self.pixels.resize_with(pixel_count, fill_fn);

        self.size = size;
        self.size_changed = true;
    }

    fn update(&mut self, tex_cache: &mut dyn TextureCache) {
        if !self.need_update || !self.size.is_valid() {
            return;
        }

        if self.size_changed {
            tex_cache.release_texture(&mut self.handle);
            self.size_changed = false;
        }

        if !self.handle.is_valid() {
            const TEXTURE_NAME: &str = "minimap";

            // Make sure we remove it from the cache first before attempting to recreate it.
            if let Some(mut existing_texture) = tex_cache.find_loaded_texture(TEXTURE_NAME) {
                tex_cache.release_texture(&mut existing_texture);
            }

            let minimap_texture_settings = TextureSettings {
                filter: TextureFilter::Nearest,
                wrap_mode: TextureWrapMode::ClampToBorder,
                gen_mipmaps: false,
            };
            self.handle = tex_cache.new_uninitialized_texture(TEXTURE_NAME,
                                                              self.size,
                                                              Some(minimap_texture_settings));
        }

        let len_in_bytes  = self.pixels.len() * std::mem::size_of::<MinimapTileColor>();
        let bytes_ptr = self.pixels.as_ptr() as *const u8;
        let pixels = unsafe { std::slice::from_raw_parts(bytes_ptr, len_in_bytes) };

        tex_cache.update_texture(self.handle, 0, 0, self.size, 0, pixels);
        self.need_update = false;
    }

    fn pre_load(&mut self, tex_cache: &mut dyn TextureCache) {
        // Release the current minimap texture. It will be recreated
        // with the correct dimensions on next update().
        tex_cache.release_texture(&mut self.handle);
    }

    fn post_load(&mut self, tile_map: &TileMap) {
        self.reset(tile_map.size_in_cells(), MinimapTileColor::default);

        tile_map.for_each_tile(TileMapLayerKind::Terrain, TileKind::Terrain,
            |terrain| {
                if let Some(color) = MinimapTileColor::from_tile_def(terrain.tile_def()) {
                    self.set_pixel(terrain.base_cell(), color);
                }
            });

        tile_map.for_each_tile(TileMapLayerKind::Objects, MINIMAP_OBJECT_TILE_KINDS,
            |object| {
                if let Some(color) = MinimapTileColor::from_tile_def(object.tile_def()) {
                    for cell in &object.cell_range() {
                        self.set_pixel(cell, color);
                    }
                }
            });
    }

    #[inline]
    fn set_pixel(&mut self, cell: Cell, color: MinimapTileColor) {
        let index = self.cell_to_index(cell);
        self.pixels[index] = color;
        self.need_update = true;
    }

    #[inline]
    fn cell_to_index(&self, cell: Cell) -> usize {
        let cell_index = cell.x + (cell.y * self.size.width);
        cell_index as usize
    }

    #[inline]
    fn is_cell_within_bounds(&self, cell: Cell) -> bool {
        if (cell.x < 0 || cell.x >= self.size.width) ||
           (cell.y < 0 || cell.y >= self.size.height)
        {
            return false;
        }
        true
    }
}

// ----------------------------------------------
// MinimapIcon / MinimapIconInstance
// ----------------------------------------------

pub const MINIMAP_ICON_DEFAULT_LIFETIME: Seconds = 5.0;

const MINIMAP_ICON_SIZE: f32 = 20.0; // W & H in pixels.
const MINIMAP_ICON_COUNT: usize = MinimapIcon::COUNT;

#[repr(u8)]
#[derive(Copy, Clone, EnumCount, EnumIter, EnumProperty)]
pub enum MinimapIcon {
    #[strum(props(AssetPath = "ui/icons/alert_icon.png"))]
    Alert
}

impl MinimapIcon {
    #[inline]
    fn asset_path(self) -> PathBuf {
        let path = self.get_str("AssetPath").unwrap();
        paths::asset_path(path)
    }
}

#[derive(Copy, Clone)]
struct MinimapIconInstance {
    icon: MinimapIcon,

    target_cell: Cell,
    texture: TextureHandle,
    tint: Color,

    // One time_left reaches zero the icon expires and is removed.
    lifetime: Seconds,
    time_left: Seconds,
}

// Takes care of loading the icon textures exactly once.
struct MinimapIconTexCache {
    textures: [TextureHandle; MINIMAP_ICON_COUNT],
}

impl MinimapIconTexCache {
    const fn new() -> Self {
        Self { textures: [TextureHandle::invalid(); MINIMAP_ICON_COUNT] }
    }

    #[inline]
    fn icon_texture(&self, icon: MinimapIcon) -> TextureHandle {
        let texture = self.textures[icon as usize];
        debug_assert!(texture.is_valid());
        texture
    }

    #[inline]
    fn are_icon_textures_loaded(&self) -> bool {
        self.textures[0].is_valid()
    }

    fn load_icon_textures(&mut self, tex_cache: &mut dyn TextureCache) {
        for icon in MinimapIcon::iter() {
            let texture = &mut self.textures[icon as usize];
            debug_assert!(!texture.is_valid(), "Minimap icon texture is already loaded!");

            *texture = tex_cache.load_texture_with_settings(
                icon.asset_path().to_str().unwrap(),
                Some(ui::texture_settings())
            );
        }
    }
}

singleton! { MINIMAP_ICON_TEX_CACHE_SINGLETON, MinimapIconTexCache }

// ----------------------------------------------
// Minimap
// ----------------------------------------------

#[derive(Default)]
pub struct Minimap {
    texture: MinimapTexture,
    icons: Vec<MinimapIconInstance>,
    widget: MinimapWidget,
}

impl Minimap {
    pub fn new(map_size_in_cells: Size) -> Self {
        Self {
            // One pixel per tile map cell.
            texture: MinimapTexture::new(map_size_in_cells),
            ..Default::default()
        }
    }

    pub fn update(&mut self,
                  camera: &mut Camera,
                  tex_cache: &mut dyn TextureCache,
                  input_sys: &dyn InputSystem,
                  ui_sys: &UiSystem,
                  delta_time_secs: Seconds) {
        // Load icon textures once:
        if !MinimapIconTexCache::get().are_icon_textures_loaded() {
            MinimapIconTexCache::get_mut().load_icon_textures(tex_cache);
        }

        self.texture.update(tex_cache);
        self.update_icons(delta_time_secs);
        self.widget.update(camera, input_sys, ui_sys, self.size_in_cells(), delta_time_secs);
    }

    #[inline]
    pub fn pre_load(&mut self, context: &PreLoadContext) {
        self.texture.pre_load(context.engine().texture_cache());
        self.icons.clear();
    }

    #[inline]
    pub fn post_load(&mut self, context: &PostLoadContext) {
        self.texture.post_load(context.tile_map());
    }

    #[inline]
    pub fn memory_usage_estimate(&self) -> usize {
        self.texture.memory_usage_estimate()
    }

    #[inline]
    pub fn tile_count(&self) -> usize {
        // 1 pixel = 1 tile
        (self.texture.size.width * self.texture.size.height) as usize
    }

    #[inline]
    pub fn size_in_cells(&self) -> Size {
        // 1 pixel = 1 tile
        self.texture.size
    }

    pub fn reset(&mut self, fill_with_def: Option<&'static TileDef>, new_map_size: Option<Size>) {
        let size = new_map_size.unwrap_or(self.texture.size);
        self.texture.reset(size, || {
            if let Some(tile_def) = fill_with_def {
                MinimapTileColor::from_tile_def(tile_def).unwrap_or_default()
            } else {
                MinimapTileColor::default()
            }
        });
        self.widget.reset();
    }

    // ----------------------
    // Tile placement:
    // ----------------------

    pub fn place_tile(&mut self, target_cell: Cell, tile_def: &'static TileDef) {
        if !self.texture.is_cell_within_bounds(target_cell) {
            return;
        }

        if let Some(color) = MinimapTileColor::from_tile_def(tile_def) {
            for cell in &tile_def.cell_range(target_cell) {
                self.texture.set_pixel(cell, color);
            }
        }
    }

    pub fn clear_tile(&mut self, target_cell: Cell, tile_def: &'static TileDef) {
        if !self.texture.is_cell_within_bounds(target_cell) {
            return;
        }

        if tile_def.is(TileKind::Terrain) {
            self.texture.set_pixel(target_cell, MinimapTileColor::default());
        } else if water::is_port_or_wharf(tile_def) {
            for cell in &tile_def.cell_range(target_cell) {
                self.texture.set_pixel(cell, MinimapTileColor::water());
            }
        } else if tile_def.is(MINIMAP_OBJECT_TILE_KINDS) {
            for cell in &tile_def.cell_range(target_cell) {
                self.texture.set_pixel(cell, MinimapTileColor::empty_land());
            }
        }
    }

    // ----------------------
    // Minimap icons:
    // ----------------------

    pub fn push_icon(&mut self, icon: MinimapIcon, target_cell: Cell, tint: Color, lifetime_secs: Seconds) {
        self.icons.push(MinimapIconInstance {
            icon,
            target_cell,
            texture: MinimapIconTexCache::get().icon_texture(icon),
            tint,
            lifetime: lifetime_secs,
            time_left: lifetime_secs,
        });
    }

    fn update_icons(&mut self, delta_time_secs: Seconds) {
        if self.icons.is_empty() {
            return;
        }

        let mut expired_indices = SmallVec::<[usize; 16]>::new();

        // Update time left and expire icons:
        for (index, icon) in self.icons.iter_mut().enumerate() {
            icon.time_left -= delta_time_secs;
            if icon.time_left <= 0.0 {
                expired_indices.push(index);
            }
        }

        // Remove in reverse order so any vector shuffles will not invalidate the
        // remaining indices.
        for expired_index in expired_indices.iter().rev() {
            self.icons.swap_remove(*expired_index);
        }
    }

    // ----------------------
    // Minimap rendering:
    // ----------------------

    // Draw the minimap using ImGui, nestled inside its own window.
    pub fn draw(&mut self,
                renderer: &mut impl MinimapRenderer,
                context: &mut UiWidgetContext,
                camera: &mut Camera) {
        let mut render_ctx = MinimapRenderContext {
            camera,
            ui_sys: context.ui_sys,
            render_sys: context.render_sys,
            minimap: self,
        };
        renderer.draw(&mut render_ctx);
    }
}

// ----------------------------------------------
// Minimap rendering constants
// ----------------------------------------------

// Margins in pixels.
const MINIMAP_EDGE_MARGINS: Vec2 = Vec2::new(4.0, 4.0);

// Rotate the minimap -45 degrees to match our isometric world projection.
const MINIMAP_ROTATION_ANGLE: f32 = -45.0 * (std::f32::consts::PI / 180.0);

// ----------------------------------------------
// MinimapTransform
// ----------------------------------------------

struct MinimapTransform {
    offsets: Vec2, // Minimap texture offset/panning in cells (pixels), from minimap origin (0,0).
    scale: f32,    // Zoom amount: 1=draw full minimap, >1 zooms-in, <1 zooms-out. Must not be zero.
}

impl MinimapTransform {
    const ZOOM_STEP: f32 = 0.1;
    const ZOOM_MIN:  f32 = 0.1;
    const ZOOM_MAX:  f32 = 10.0;

    #[inline]
    fn zoom(&self) -> f32 {
        debug_assert!(self.is_valid());
        self.scale
    }

    #[inline]
    fn is_valid(&self) -> bool {
        self.scale > 0.0
    }

    fn reset(&mut self) {
        self.offsets = Vec2::default();
        self.scale   = 1.0;
    }
}

impl Default for MinimapTransform {
    fn default() -> Self {
        Self { offsets: Vec2::default(), scale: 1.0 }
    }
}

// ----------------------------------------------
// ScreenToMinimap
// ----------------------------------------------

struct ScreenToMinimap {
    screen_rect: Rect,  // Visible world area in screen space.
    minimap_rect: Rect, // Widget rect (aabb).
}

impl ScreenToMinimap {
    fn transform_point(&self, p: Vec2) -> Vec2 {
        let t = (p - self.screen_rect.min) / self.screen_rect.size();
        self.minimap_rect.min + (t * self.minimap_rect.size())
    }
}

// ----------------------------------------------
// MinimapDrawData
// ----------------------------------------------

#[derive(Default)]
struct MinimapDrawData {
    axis_aligned_minimap_rect: Rect, // Unrotated minimap widget rect.
    inner_playable_area_rect: Rect,  // Inner playable map area, also the clip rect to trim unreachable diamond margins.
    diamond: IsoDiamond,             // CCW rotated corners of `axis_aligned_minimap_rect`, AKA the minimap diamond.
    diamond_bounding_rect: Rect,     // Axis-aligned bounding rect of `diamond`.
}

impl MinimapDrawData {
    fn is_valid(&self) -> bool {
        self.axis_aligned_minimap_rect.is_valid() &&
        self.inner_playable_area_rect.is_valid() &&
        self.diamond_bounding_rect.is_valid()
    }

    fn center(&self) -> Vec2 {
        self.axis_aligned_minimap_rect.center()
    }

    fn bounding_rect(&self) -> Rect {
        self.diamond_bounding_rect
    }

    // Scaled inner playable area rect, also the widget's clip rect. Nothing outside this rect will render.
    fn clip_rect(&self) -> Rect {
        let scaling = self.scaling_factor();
        self.inner_playable_area_rect.scaled(scaling)
    }

    // Compute scaling factor needed so that the inner playable area rect fills the whole widget.
    fn scaling_factor(&self) -> f32 {
        let inner_size  = self.inner_playable_area_rect.size();
        let widget_size = self.diamond_bounding_rect.size();
        (widget_size.x / inner_size.x).min(widget_size.y / inner_size.y)
    }

    // Minimap diamond corners scaled and translated so that the inner playable rect is centered and fills the whole widget.
    fn corners(&self) -> [Vec2; 4] {
        self.diamond.map_points(|p| self.transform_point(p))
    }

    // Apply scaling and translation to `p` so that it is ready to render inside the minimap widget clip rect.
    fn transform_point(&self, p: Vec2) -> Vec2 {
        let scaling    = self.scaling_factor();
        let inner_min  = self.inner_playable_area_rect.min;
        let widget_min = self.diamond_bounding_rect.min;
        widget_min + (p - inner_min) * scaling
    }

    // Undo transform_point().
    fn untransform_point(&self, p: Vec2) -> Vec2 {
        let scaling    = self.scaling_factor();
        let inner_min  = self.inner_playable_area_rect.min;
        let widget_min = self.diamond_bounding_rect.min;
        inner_min + (p - widget_min) / scaling
    }
}

// ----------------------------------------------
// MinimapWidget
// ----------------------------------------------

struct MinimapWidget {
    is_open: bool,
    cursor_pos: Vec2,             // Cursor screen space position, cached on update().
    widget_rect: Rect,            // Minimap widget screen space rect, relative to window rect.
    window_rect: Rect,            // Widget window screen rect in absolute screen space.
    transform: MinimapTransform,  // Zoom (scale) & UV offsets (translation).
    draw_data: MinimapDrawData,   // Scaled and translated screen rects where we render the minimap texture to.
    map_size_in_cells: Vec2,      // Minimap/TileMap size in cells (1 TileMap cell = 1 minimap pixel).
    auto_zoom: bool,              // Automatically adjust zoom to best match desired number of visible tiles/cells.
    auto_scroll: bool,            // Scroll minimap when camera rect nears the minimap edges?
    scroll_speed_px_per_sec: f32, // Scroll speed in pixels per second when `auto_scroll=true`.
    desired_visible_cells: Size,  // Desired number of visible cells we want to display for when `auto_zoom=true`.
    camera_rect: Rect,            // Camera overlay rect in screen space, recomputed every update().
}

impl Default for MinimapWidget {
    fn default() -> Self {
        Self {
            is_open: true,
            cursor_pos: Vec2::default(),
            widget_rect: Rect::from_pos_and_size(
                Vec2::new(35.0, 55.0),
                Vec2::new(100.0, 100.0)
            ),
            window_rect: Rect::default(),
            transform: MinimapTransform::default(),
            draw_data: MinimapDrawData::default(),
            map_size_in_cells: Vec2::default(),
            auto_zoom: true,
            auto_scroll: true,
            scroll_speed_px_per_sec: 30.0,
            desired_visible_cells: Size::new(95, 95),
            camera_rect: Rect::default(),
        }
    }
}

impl MinimapWidget {
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn update(&mut self,
              camera: &mut Camera,
              input_sys: &dyn InputSystem,
              ui_sys: &UiSystem,
              map_size_in_cells: Size,
              delta_time_secs: Seconds) {
        if !self.is_open || !map_size_in_cells.is_valid() {
            return;
        }

        debug_assert!(self.widget_rect.is_valid());
        debug_assert!(self.transform.is_valid());

        // Must update these every frame:
        self.cursor_pos        = input_sys.cursor_pos();
        self.map_size_in_cells = map_size_in_cells.to_vec2();
        self.window_rect       = self.calc_window_rect(ui_sys);
        self.draw_data         = self.calc_minimap_draw_data();
        self.camera_rect       = self.calc_camera_minimap_rect(camera);

        // Auto zoom for large maps:
        self.update_minimap_zoom();

        // Auto scrolling when camera rect is touching the map edges (if zoomed in):
        self.update_minimap_scrolling(delta_time_secs);

        // Cursor -> minimap cell picking:
        if input_sys.mouse_button_state(MouseButton::Left) == InputAction::Press {
            if let Some(teleport_destination_iso) = self.pick_cursor_pos() {
                camera.teleport_iso(teleport_destination_iso);
            }
        }
    }

    fn update_minimap_zoom(&mut self) {
        if !self.auto_zoom {
            return;
        }

        loop {
            let visible_cells = Self::calc_minimap_visible_cells(self.map_size_in_cells,
                                                                 self.transform.zoom());

            if visible_cells.x as i32 <= self.desired_visible_cells.width ||
               visible_cells.y as i32 <= self.desired_visible_cells.height
            {
                break;
            }

            self.transform.scale += MinimapTransform::ZOOM_STEP;
        }

        self.transform.scale =
            self.transform.scale.clamp(
                MinimapTransform::ZOOM_MIN,
                MinimapTransform::ZOOM_MAX);
    }

    fn update_minimap_scrolling(&mut self, delta_time_secs: Seconds) {
        // Converts distance from edge into a [0, 1] push factor.
        // Negative distance means we're still inside the playable area.
        fn edge_push(distance: f32, margin: f32) -> f32 {
            if distance > -margin {
                ((margin + distance) / margin).clamp(0.0, 1.0)
            } else {
                0.0
            }
        }

        if !self.auto_scroll || self.transform.zoom() <= 1.0 {
            return;
        }

        let camera_rect   = self.camera_rect;
        let playable_area = self.draw_data.clip_rect();

        // Signed distances: negative = inside, positive = violating / pushing.
        let dx_left   = playable_area.min.x - camera_rect.min.x;
        let dx_right  = camera_rect.max.x   - playable_area.max.x;
        let dy_top    = playable_area.min.y - camera_rect.min.y;
        let dy_bottom = camera_rect.max.y   - playable_area.max.y;

        let push_left   = edge_push(dx_left,   MINIMAP_EDGE_MARGINS.x);
        let push_right  = edge_push(dx_right,  MINIMAP_EDGE_MARGINS.x);
        let push_top    = edge_push(dy_top,    MINIMAP_EDGE_MARGINS.y);
        let push_bottom = edge_push(dy_bottom, MINIMAP_EDGE_MARGINS.y);

        let mut scroll_dir = Vec2::zero();

        // Diamond-space directions:
        scroll_dir += Vec2::new( 1.0, -1.0) * push_top;
        scroll_dir += Vec2::new( 1.0,  1.0) * push_right;
        scroll_dir += Vec2::new(-1.0,  1.0) * push_bottom;
        scroll_dir += Vec2::new(-1.0, -1.0) * push_left;

        if scroll_dir == Vec2::zero() {
            return;
        }

        let scroll = scroll_dir * self.scroll_speed_px_per_sec * delta_time_secs;
        self.transform.offsets += scroll;

        // Ensure we never go out of bounds on our min/max minimap texture UVs.
        self.clamp_minimap_uv_window();
    }

    // Returns floating-point isometric coords without rounding to integer cell space.
    fn pick_cursor_pos(&self) -> Option<IsoPointF32> {
        debug_assert!(self.draw_data.is_valid());

        if !self.draw_data.clip_rect().contains_point(self.cursor_pos) {
            return None; // Cursor outside minimap playable area.
        }

        // Undo minimap rotation first:
        let minimap_center = self.draw_data.center();
        let minimap_px = self.cursor_pos.rotate_around_point(minimap_center, -MINIMAP_ROTATION_ANGLE);

        // Convert widget px -> fractional cell (using UV window inverse).
        let cell = self.scaled_minimap_widget_px_to_cell(minimap_px);

        // UV may be outside window -> return None if outside full map.
        if cell.0.x < 0.0 || cell.0.x >= self.map_size_in_cells.x ||
           cell.0.y < 0.0 || cell.0.y >= self.map_size_in_cells.y
        {
            return None;
        }

        Some(coords::cell_to_iso_f32(cell))
    }

    // Edges of camera rect near the playable area limits, with MINIMAP_EDGE_MARGINS.
    fn camera_rect_edges_near_playable_map_area_limits(&self) -> RectEdges {
        debug_assert!(self.camera_rect.is_valid());
        debug_assert!(self.draw_data.is_valid());

        // Perform overlap test with margin.
        let camera_rect = self.camera_rect.expanded(MINIMAP_EDGE_MARGINS);
        self.draw_data.clip_rect().edges_outside(&camera_rect)
    }

    // Rect in minimap widget screen space, ready to be rendered.
    fn calc_camera_minimap_rect(&self, camera: &Camera) -> Rect {
        debug_assert!(self.draw_data.is_valid());

        // Camera viewport extents in screen space:
        let camera_screen_rect   = camera.camera_screen_rect();
        let camera_screen_center = camera_screen_rect.center();
        let camera_screen_half   = camera_screen_rect.size() * 0.5;

        let camera_screen_corners = [
            camera_screen_center + Vec2::new(-camera_screen_half.x, 0.0), // left
            camera_screen_center + Vec2::new( camera_screen_half.x, 0.0), // right
            camera_screen_center + Vec2::new(0.0, -camera_screen_half.y), // top
            camera_screen_center + Vec2::new(0.0,  camera_screen_half.y), // bottom
        ];

        // Convert screen -> fractional cell space:
        let mut cell_min = Vec2::new(f32::MAX, f32::MAX);
        let mut cell_max = Vec2::new(f32::MIN, f32::MIN);
        for corner in camera_screen_corners {
            let cell = coords::screen_point_to_cell_f32(corner, camera.transform()).0;
            cell_min = cell_min.min(cell);
            cell_max = cell_max.max(cell);
        }

        // Cell -> minimap widget window (in diamond space):
        let widget_camera_rect_corners = [
            self.cell_to_scaled_minimap_widget_px(CellF32(Vec2::new(cell_min.x, cell_min.y))),
            self.cell_to_scaled_minimap_widget_px(CellF32(Vec2::new(cell_max.x, cell_min.y))),
            self.cell_to_scaled_minimap_widget_px(CellF32(Vec2::new(cell_max.x, cell_max.y))),
            self.cell_to_scaled_minimap_widget_px(CellF32(Vec2::new(cell_min.x, cell_max.y))),
        ];

        // Apply rotation so we move from diamond to final widget screen space.
        let minimap_center = self.draw_data.center();
        let rotated_camera_rect_corners = widget_camera_rect_corners.map(|corner| {
            corner.rotate_around_point(minimap_center, MINIMAP_ROTATION_ANGLE)
        });

        // Finally, make sure we stay within the inner playable area, always.
        *Rect::from_points(&rotated_camera_rect_corners)
            .clamp(&self.draw_data.clip_rect().shrunk(Vec2::one())) // 1px margin so we never overlap the clip rect.
    }

    fn calc_playable_map_area_rect(map_size_in_cells: Vec2, diamond_bounding_rect: Rect) -> Rect {
        debug_assert!(map_size_in_cells != Vec2::zero());
        debug_assert!(diamond_bounding_rect.is_valid());

        let map_diamond = IsoDiamond::from_tile_map(
            Size::from_vec2(map_size_in_cells),
            WorldToScreenTransform::default()
        );

        let screen_to_minimap = ScreenToMinimap {
            screen_rect: map_diamond.bounding_rect(),
            minimap_rect: diamond_bounding_rect,
        };

        map_diamond.map_inner_rect(|p| screen_to_minimap.transform_point(p))
    }

    #[inline]
    fn calc_minimap_visible_cells(map_size_in_cells: Vec2, zoom: f32) -> Vec2 {
        (map_size_in_cells - Vec2::one()) / zoom
    }

    // `offsets` are in minimap cells/pixels (same units as map_size_in_cells).
    #[inline]
    fn calc_minimap_rect_uvs(map_size_in_cells: Vec2, offsets: Vec2, zoom: f32) -> (Vec2, Vec2) {
        let visible_cells = Self::calc_minimap_visible_cells(map_size_in_cells, zoom);
        let max_cells = map_size_in_cells - Vec2::one();
        let uv_min = offsets / max_cells;
        let uv_max = (offsets + visible_cells) / max_cells;
        (uv_min, uv_max)
    }

    #[inline]
    fn calc_zoom_offsets_from_center(map_size_in_cells: Vec2, center_cell: CellF32, zoom: f32) -> Vec2 {
        if zoom <= 1.0 {
            // Full minimap already visible.
            return Vec2::zero();
        }

        let visible_cells = Self::calc_minimap_visible_cells(map_size_in_cells, zoom);
        let max_offsets = map_size_in_cells - Vec2::one() - visible_cells;

        // Offset so center cell stays fixed regardless of zoom.
        let mut offsets = center_cell.0 - (visible_cells * 0.5);

        // Clamp to texture bounds.
        offsets.x = offsets.x.clamp(0.0, max_offsets.x);
        offsets.y = offsets.y.clamp(0.0, max_offsets.y);

        offsets
    }

    fn calc_minimap_draw_data(&self) -> MinimapDrawData {
        debug_assert!(self.map_size_in_cells != Vec2::zero());
        debug_assert!(self.widget_rect.is_valid() && self.window_rect.is_valid());

        let axis_aligned_minimap_rect = Rect::from_pos_and_size(
            self.widget_rect.position() + self.window_rect.position(),
            self.widget_rect.size()
        );

        let minimap_center = axis_aligned_minimap_rect.center();
        let diamond_corners = axis_aligned_minimap_rect.corners_ccw().map(|corner| {
            corner.rotate_around_point(minimap_center, MINIMAP_ROTATION_ANGLE)
        });

        let diamond = IsoDiamond::from_screen_points(diamond_corners);
        let diamond_bounding_rect = diamond.bounding_rect();

        let inner_playable_area_rect = Self::calc_playable_map_area_rect(
            self.map_size_in_cells,
            diamond_bounding_rect
        );

        MinimapDrawData {
            axis_aligned_minimap_rect,
            inner_playable_area_rect,
            diamond,
            diamond_bounding_rect
        }
    }

    #[inline]
    fn calc_window_rect(&self, ui_sys: &UiSystem) -> Rect {
        debug_assert!(self.widget_rect.is_valid());
        let size = Vec2::new(self.widget_rect.width() + 70.0, self.widget_rect.height() + 90.0);
        let pos  = Vec2::new(0.0, ui_sys.ui().io().display_size[1] - size.y);
        Rect::from_pos_and_size(pos, size)
    }

    // Convenience to return the current UV window used by drawing code.
    #[inline]
    fn current_minimap_uv_window(&self) -> (Vec2, Vec2) {
        let map_center_cell = CellF32((self.map_size_in_cells - Vec2::one()) * 0.5);
        let zoom = self.transform.zoom();
        let zoom_offsets = Self::calc_zoom_offsets_from_center(self.map_size_in_cells, map_center_cell, zoom);
        let combined_offsets = zoom_offsets + self.transform.offsets;
        Self::calc_minimap_rect_uvs(self.map_size_in_cells, combined_offsets, zoom)
    }

    fn clamp_minimap_uv_window(&mut self) {
        let (uv_min, uv_max) = self.current_minimap_uv_window();

        let clamped_min = uv_min.clamp(Vec2::zero(), Vec2::one());
        let clamped_max = uv_max.clamp(Vec2::zero(), Vec2::one());

        let delta_min = clamped_min - uv_min;
        let delta_max = clamped_max - uv_max;

        // If either side was clamped, shift offsets accordingly.
        let uv_correction = delta_min + delta_max;
        if uv_correction != Vec2::zero() {
            // Convert UV delta back into offset space.
            let minimap_size_px = self.draw_data.axis_aligned_minimap_rect.size();
            let offset_delta = uv_correction * minimap_size_px;
            self.transform.offsets += offset_delta;
        }
    }

    // ----------------------------------------------
    // Coordinate space conversion helpers
    // ----------------------------------------------

    // Maps fractional cell coords to full minimap UVs [0,1] and vice-versa.
    #[inline]
    fn cell_to_minimap_uv(&self, cell: CellF32) -> Vec2 {
        let max_cells = self.map_size_in_cells - Vec2::one();
        Vec2::new(
            cell.0.x / max_cells.x,
            // NOTE: Flip V for ImGui (because OpenGL textures have V=0 at bottom).
            1.0 - (cell.0.y / max_cells.y)
        )
    }

    #[inline]
    fn minimap_uv_to_cell(&self, uv: Vec2) -> CellF32 {
        let max_cells = self.map_size_in_cells - Vec2::one();
        CellF32(Vec2::new(
            uv.x * max_cells.x,
            // NOTE: Flip V for ImGui (because OpenGL textures have V=0 at bottom).
            (1.0 - uv.y) * max_cells.y
        ))
    }

    // Maps minimap UVs in [0,1] range into minimap screen pixels and vice-versa.
    #[inline]
    fn minimap_uv_to_minimap_px(&self, uv: Vec2) -> Vec2 {
        let p =
            self.draw_data.axis_aligned_minimap_rect.position()
                + (uv * self.draw_data.axis_aligned_minimap_rect.size());

        self.draw_data.transform_point(p)
    }

    #[inline]
    fn minimap_px_to_minimap_uv(&self, minimap_px: Vec2) -> Vec2 {
        let p = self.draw_data.untransform_point(minimap_px);

        (p - self.draw_data.axis_aligned_minimap_rect.position())
            / self.draw_data.axis_aligned_minimap_rect.size()
    }

    // Map fractional cell coords (CellF32) -> widget (screen) pixels in the axis-aligned
    // minimap rectangle (self.minimap_draw_info.rect). This respects the current
    // minimap UV visible window (zoom + offsets).
    #[inline]
    fn cell_to_scaled_minimap_widget_px(&self, cell: CellF32) -> Vec2 {
        // Base UV in full-map space to zoomed minimap window:
        let fullmap_uv = self.cell_to_minimap_uv(cell);
        let window_uv  = self.fullmap_uv_to_window_uv(fullmap_uv);

        // Map normalized window coords -> widget pixels.
        self.minimap_uv_to_minimap_px(window_uv)
    }

    // Inverse: widget pixel -> fractional cell coords (CellF32).
    // `widget_px` is in absolute screen coordinates.
    #[inline]
    fn scaled_minimap_widget_px_to_cell(&self, widget_px: Vec2) -> CellF32 {
        // Normalized [0,1] within visible window.
        let window_uv  = self.minimap_px_to_minimap_uv(widget_px);

        let (uv_min, uv_max) = self.current_minimap_uv_window();
        let fullmap_uv = uv_min + window_uv * (uv_max - uv_min); // Convert to full-map UV.

        self.minimap_uv_to_cell(fullmap_uv)
    }

    // Convert UV in full-map space into minimap UV in the current visible window.
    // Input: fullmap_uv [0,1] relative to the whole minimap texture.
    // Return: window_uv [0,1] relative to the zoomed/translated minimap window.
    #[inline]
    fn fullmap_uv_to_window_uv(&self, fullmap_uv: Vec2) -> Vec2 {
        // Current visible uv window (uv_min, uv_max) where both in full-map UV space:
        let (uv_min, uv_max) = self.current_minimap_uv_window();

        // Map fullmap_uv into visible window-local normalized [0,1]:
        let win_size = uv_max - uv_min;
        debug_assert!(win_size.x > 0.0 && win_size.y > 0.0);

        let window_uv = (fullmap_uv - uv_min) / win_size;
        debug_assert!(window_uv.x.is_finite() && window_uv.y.is_finite());

        window_uv
    }
}

// ----------------------------------------------
// MinimapRenderer / MinimapRenderContext
// ----------------------------------------------

pub trait MinimapRenderer {
    fn draw(&mut self, context: &mut MinimapRenderContext);
}

pub struct MinimapRenderContext<'game> {
    // Game systems:
    pub camera: &'game mut Camera,
    pub ui_sys: &'game UiSystem,
    pub render_sys: &'game mut dyn RenderSystem,

    // Minimap:
    pub minimap: &'game mut Minimap,
}

impl MinimapRenderContext<'_> {
    #[inline]
    fn widget_mut(&mut self) -> &mut MinimapWidget {
        &mut self.minimap.widget
    }

    #[inline]
    fn widget(&self) -> &MinimapWidget {
        &self.minimap.widget
    }

    #[inline]
    fn icons(&self) -> &[MinimapIconInstance] {
        &self.minimap.icons
    }

    #[inline]
    fn texture(&self) -> TextureHandle {
        self.minimap.texture.handle
    }
}

// ----------------------------------------------
// BaseMinimapRenderer
// ----------------------------------------------

struct BaseMinimapRenderer {
    widget_font_scale: UiFontScale,
    widget_custom_background: Option<TextureHandle>,
    apply_widget_clip_rect: bool,
}

impl MinimapRenderer for BaseMinimapRenderer {
    fn draw(&mut self, context: &mut MinimapRenderContext) {
        debug_assert!(context.widget().window_rect.is_valid());
        debug_assert!(context.widget().draw_data.is_valid());
        debug_assert!(self.widget_font_scale.is_valid());

        if context.widget().is_open {
            self.draw_widget_window(context);
        } else {
            self.draw_open_button(context);
        }
    }
}

impl BaseMinimapRenderer {
    fn draw_widget_window(&mut self, context: &mut MinimapRenderContext) {
        let widget = context.widget();

        let window_pos  = widget.window_rect.position().to_array();
        let window_size = widget.window_rect.size().to_array();

        context.ui_sys.ui().window("Minimap")
            .flags(self.window_flags())
            .position(window_pos, imgui::Condition::Always)
            .size(window_size, imgui::Condition::Always)
            .build(|| {
                context.ui_sys.set_window_font_scale(self.widget_font_scale);

                // Minimap texture and overlay icons:
                self.draw_minimap(context);
                self.draw_icons(context);

                // Header / close button:
                self.draw_header_buttons(context);

                context.ui_sys.set_window_font_scale(UiFontScale::default());
            });
    }

    fn draw_open_button(&mut self, context: &mut MinimapRenderContext) {
        let ui = context.ui_sys.ui();
        let window_pos = [5.0, ui.io().display_size[1] - 35.0];

        ui.window("Minimap Button")
            .flags(self.window_flags() | imgui::WindowFlags::NO_BACKGROUND)
            .position(window_pos, imgui::Condition::Always)
            .build(|| {
                context.minimap.widget.is_open = ui::icon_button_custom_tooltip(
                    context.ui_sys,
                    ui::icons::ICON_MAP,
                    || {
                        ui::custom_tooltip(
                            context.ui_sys,
                            self.widget_font_scale,
                            self.custom_background(context),
                            || ui.text("Open Map"));
                    });
            });
    }

    fn draw_header_buttons(&mut self, context: &mut MinimapRenderContext) {
        let ui = context.ui_sys.ui();

        // No border, no background.
        let _btn_border_size = ui.push_style_var(imgui::StyleVar::FrameBorderSize(0.0));
        let _btn_bg_color = ui.push_style_color(imgui::StyleColor::Button, [0.0; 4]);

        // Make hover / active effects semi-transparent.
        let mut btn_hovered_color = ui.style_color(imgui::StyleColor::ButtonHovered);
        btn_hovered_color[3] = 0.5;

        let mut btn_active_color = ui.style_color(imgui::StyleColor::ButtonActive);
        btn_active_color[3] = 0.5;

        let _btn_hovered = ui.push_style_color(imgui::StyleColor::ButtonHovered, btn_hovered_color);
        let _btn_active = ui.push_style_color(imgui::StyleColor::ButtonActive, btn_active_color);

        ui.set_cursor_pos([10.0, 5.0]);
        ui.text("Map");

        // Close widget button:
        ui.set_cursor_pos([context.widget().window_rect.size().x - 30.0, 5.0]);
        if ui.button("X") {
            context.minimap.widget.is_open = false;
        }

        if ui.is_item_hovered() {
            ui::custom_tooltip(
                context.ui_sys,
                self.widget_font_scale,
                self.custom_background(context),
                || ui.text("Close"));
        }
    }

    fn window_flags(&self) -> imgui::WindowFlags {
        let mut flags =
            imgui::WindowFlags::ALWAYS_AUTO_RESIZE
            | imgui::WindowFlags::NO_RESIZE
            | imgui::WindowFlags::NO_DECORATION
            | imgui::WindowFlags::NO_SCROLLBAR
            | imgui::WindowFlags::NO_MOVE
            | imgui::WindowFlags::NO_COLLAPSE;
    
        if self.widget_custom_background.is_some() {
            flags |= imgui::WindowFlags::NO_BACKGROUND;
        }

        flags
    }

    fn custom_background(&self, context: &mut MinimapRenderContext) -> Option<UiTextureHandle> {
        self.widget_custom_background.map(|tex_handle| {
            context.ui_sys.to_ui_texture(context.render_sys.texture_cache(), tex_handle)
        })
    }

    fn draw_minimap(&mut self, context: &mut MinimapRenderContext) {
        self.draw_custom_background(context);
        self.draw_minimap_texture_rect(context);
        self.draw_camera_overlay_rect(context);
    }

    fn draw_custom_background(&mut self, context: &mut MinimapRenderContext) {
        if let Some(background_texture_handle) = self.custom_background(context) {
            let ui = context.ui_sys.ui();
            let draw_list = ui.get_window_draw_list();

            let window_rect = Rect::from_pos_and_size(
                Vec2::from_array(ui.window_pos()),
                Vec2::from_array(ui.window_size())
            );

            draw_list.add_image(background_texture_handle,
                                window_rect.min.to_array(),
                                window_rect.max.to_array())
                                .build();
        }
    }

    fn draw_minimap_texture_rect(&mut self, context: &mut MinimapRenderContext) {
        let draw_list = context.ui_sys.ui().get_window_draw_list();
        let widget = context.widget();

        let draw_minimap_texture = || {
            let minimap_texture_handle =
                context.ui_sys.to_ui_texture(context.render_sys.texture_cache(), context.texture());

            let (uv_min, uv_max) = widget.current_minimap_uv_window();
            let minimap_corners = widget.draw_data.corners();

            // NOTE: Flip V for ImGui.
            let uv1 = [uv_min.x, 1.0 - uv_min.y];
            let uv2 = [uv_max.x, 1.0 - uv_min.y];
            let uv3 = [uv_max.x, 1.0 - uv_max.y];
            let uv4 = [uv_min.x, 1.0 - uv_max.y];

            draw_list
                .add_image_quad(
                    minimap_texture_handle,
                    minimap_corners[0].to_array(),
                    minimap_corners[1].to_array(),
                    minimap_corners[2].to_array(),
                    minimap_corners[3].to_array())
                .uv(uv1, uv2, uv3, uv4)
                .build();
        };

        if self.apply_widget_clip_rect {
            // Draw inner playable rectangle of the minimap diamond only.
            let clip_rect = widget.draw_data.clip_rect();
            draw_list.with_clip_rect(clip_rect.min.to_array(),
                                     clip_rect.max.to_array(),
                                     draw_minimap_texture);
        } else {
            // Draw whole minimap unclipped.
            draw_minimap_texture();
        }
    }

    fn draw_camera_overlay_rect(&mut self, context: &mut MinimapRenderContext) {
        let draw_list = context.ui_sys.ui().get_window_draw_list();
        let widget = context.widget();

        let clip_rect = widget.draw_data.clip_rect();
        let camera_rect = widget.camera_rect;

        draw_list.add_rect(camera_rect.min.to_array(),
                           camera_rect.max.to_array(),
                           imgui::ImColor32::WHITE)
                           .build();

        // Whole map outline rect, after camera overlay rect so it draws on top.
        draw_list.add_rect(clip_rect.min.to_array(),
                           clip_rect.max.to_array(),
                           imgui::ImColor32::BLACK)
                           .build();
    }

    fn draw_icons(&mut self, context: &mut MinimapRenderContext) {
        let icons = context.icons();
        if icons.is_empty() {
            return;
        }

        let tex_cache = context.render_sys.texture_cache();
        let draw_list = context.ui_sys.ui().get_window_draw_list();
        let widget    = context.widget();
        let clip_rect = widget.draw_data.clip_rect();

        let draw_all_icons = || {
            const ICON_SIZE: Vec2 = Vec2::new(MINIMAP_ICON_SIZE, MINIMAP_ICON_SIZE);
            const ICON_HALF_SIZE: Vec2 = Vec2::new(ICON_SIZE.x * 0.5, ICON_SIZE.y * 0.5);

            let minimap_center = widget.draw_data.center();
            let minimap_bounding_rect = clip_rect.expanded(ICON_SIZE);

            for icon in icons {
                if icon.lifetime <= 0.0 || icon.time_left <= 0.0 {
                    continue;
                }

                let icon_center = widget
                    .cell_to_scaled_minimap_widget_px(CellF32::from_integer_cell(icon.target_cell))
                    .rotate_around_point(minimap_center, MINIMAP_ROTATION_ANGLE);

                let icon_rect =
                    Rect::from_extents(icon_center - ICON_HALF_SIZE, icon_center + ICON_HALF_SIZE);

                // Discard icon if fully outside of the minimap bounds.
                if !minimap_bounding_rect.contains_rect(&icon_rect) {
                    continue;
                }

                // Fade-out based on remaining lifetime seconds.
                let icon_tint_alpha = (icon.time_left / icon.lifetime).clamp(0.0, 1.0);
                let icon_tint = Color::new(icon.tint.r, icon.tint.g, icon.tint.b, icon_tint_alpha);

                let icon_ui_texture = context.ui_sys.to_ui_texture(tex_cache, icon.texture);
        
                draw_list
                    .add_image(icon_ui_texture, icon_rect.min.to_array(), icon_rect.max.to_array())
                    .col(imgui::ImColor32::from_rgba_f32s(icon_tint.r, icon_tint.g, icon_tint.b, icon_tint_alpha))
                    .build();
            }
        };

        if self.apply_widget_clip_rect {
            draw_list.with_clip_rect(clip_rect.min.to_array(),
                                     clip_rect.max.to_array(),
                                     draw_all_icons);
        } else {
            draw_all_icons();
        }
    }
}

// ----------------------------------------------
// InGameUiMinimapRenderer
// ----------------------------------------------

pub struct InGameUiMinimapRenderer {
    base_renderer: BaseMinimapRenderer,
}

impl MinimapRenderer for InGameUiMinimapRenderer {
    fn draw(&mut self, context: &mut MinimapRenderContext) {
        self.base_renderer.draw(context);
    }
}

impl InGameUiMinimapRenderer {
    const WIDGET_FONT_SCALE: UiFontScale = UiFontScale(0.8);

    pub fn new(context: &mut UiWidgetContext) -> Self {
        let background_texture = context.load_texture("misc/square_page_bg.png");

        Self {
            base_renderer: BaseMinimapRenderer {
                widget_font_scale: Self::WIDGET_FONT_SCALE,
                widget_custom_background: Some(background_texture),
                apply_widget_clip_rect: true,
            }
        }
    }
}

// ----------------------------------------------
// DevUiMinimapRenderer
// ----------------------------------------------

// Render minimap with debug controls.
pub struct DevUiMinimapRenderer {
    base_renderer: BaseMinimapRenderer,
    enable_debug_draw: bool,
    show_debug_controls: bool,
}

impl MinimapRenderer for DevUiMinimapRenderer {
    fn draw(&mut self, context: &mut MinimapRenderContext) {
        // Draw base widget:
        self.base_renderer.draw(context);

        if context.widget().is_open {
            // Extend minimap widget window:
            context.ui_sys.ui()
                .window("Minimap")
                .build(|| {
                    self.draw_debug_header_buttons(context);
                    self.draw_debug_outline_rect(context);
                    self.draw_debug_camera_rect(context);
                });

            // Debug controls panel:
            self.draw_debug_controls(context);
        }
    }
}

impl DevUiMinimapRenderer {
    const WIDGET_FONT_SCALE: UiFontScale = UiFontScale::identity();

    pub fn new() -> Self {
        Self {
            base_renderer: BaseMinimapRenderer {
                widget_font_scale: Self::WIDGET_FONT_SCALE,
                widget_custom_background: None,
                apply_widget_clip_rect: true,
            },
            enable_debug_draw: false,
            show_debug_controls: false,
        }
    }

    fn draw_debug_header_buttons(&mut self, context: &mut MinimapRenderContext) {
        let ui = context.ui_sys.ui();

        // No border, no background.
        let _btn_border_size = ui.push_style_var(imgui::StyleVar::FrameBorderSize(0.0));
        let _btn_bg_color = ui.push_style_color(imgui::StyleColor::Button, [0.0; 4]);

        // Make hover / active effects semi-transparent.
        let mut btn_hovered_color = ui.style_color(imgui::StyleColor::ButtonHovered);
        btn_hovered_color[3] = 0.5;

        let mut btn_active_color = ui.style_color(imgui::StyleColor::ButtonActive);
        btn_active_color[3] = 0.5;

        let _btn_hovered = ui.push_style_color(imgui::StyleColor::ButtonHovered, btn_hovered_color);
        let _btn_active = ui.push_style_color(imgui::StyleColor::ButtonActive, btn_active_color);

        let prev_cursor = ui.cursor_pos();

        // Open/close Debug Controls:
        ui.set_cursor_pos([context.widget().window_rect.size().x - 60.0, 5.0]);
        if ui.button("D") {
            self.show_debug_controls = !self.show_debug_controls;
        }

        if ui.is_item_hovered() {
            ui::custom_tooltip(
                context.ui_sys,
                self.base_renderer.widget_font_scale,
                None,
                || ui.text("Debug"));
        }

        ui.set_cursor_pos(prev_cursor);
    }

    fn draw_debug_outline_rect(&self, context: &mut MinimapRenderContext) {
        if !self.enable_debug_draw {
            return;
        }

        let draw_list  = context.ui_sys.ui().get_window_draw_list();
        let widget     = context.widget();
        let cursor_pos = widget.cursor_pos;
        let clip_rect  = widget.draw_data.clip_rect();

        if clip_rect.contains_point(cursor_pos) {
            // Red when cursor inside minimap.
            let color = imgui::ImColor32::from_rgb(255, 0, 0);

            draw_list.add_rect(clip_rect.min.to_array(),
                               clip_rect.max.to_array(),
                               color)
                               .build();

            draw_list.add_circle(cursor_pos.to_array(),
                                 4.0,
                                 color)
                                 .build();
        }

        // Minimap diamond corners:
        let corner_colors = [
            imgui::ImColor32::from_rgb(255, 0, 0),     // 0, red
            imgui::ImColor32::from_rgb(0, 255, 0),     // 1, green
            imgui::ImColor32::from_rgb(0, 0, 255),     // 2, blue
            imgui::ImColor32::from_rgb(255, 255, 255), // 3, white
        ];

        for (corner, color) in widget.draw_data.diamond.screen_points().iter().zip(corner_colors) {
            draw_list.add_circle(corner.to_array(), 2.0, color).build();
        }
    }

    fn draw_debug_camera_rect(&self, context: &mut MinimapRenderContext) {
        if !self.enable_debug_draw {
            return;
        }

        let draw_list = context.ui_sys.ui().get_window_draw_list();
        let widget = context.widget();

        let camera_rect = widget.camera_rect;
        let camera_near_playable_area_limits = !widget.camera_rect_edges_near_playable_map_area_limits().is_empty();

        if camera_near_playable_area_limits {
            // Color camera rect red if any corner of the camera is nearing the playable area limits.
            draw_list.add_rect(camera_rect.min.to_array(),
                               camera_rect.max.to_array(),
                               imgui::ImColor32::from_rgb(255, 0, 0))
                               .build();
        }

        // Camera rect min (BLUE) / max (YELLOW):
        draw_list.add_circle(camera_rect.min.to_array(),
                             2.0,
                             imgui::ImColor32::from_rgb(0, 0, 255))
                             .filled(true)
                             .build();

        draw_list.add_circle(camera_rect.max.to_array(),
                             2.0,
                             imgui::ImColor32::from_rgb(255, 255, 0))
                             .filled(true)
                             .build();

        // Draw a green/red debug circle at the camera's center:
        let camera_center_point = {
            // We want to visualize the derived camera center cell:
            let camera_center_iso = context.camera.iso_world_position();
            let camera_center_cell = coords::iso_to_cell_f32(camera_center_iso);
            let camera_center_screen = widget.cell_to_scaled_minimap_widget_px(camera_center_cell);
            camera_center_screen.rotate_around_point(widget.draw_data.center(), MINIMAP_ROTATION_ANGLE)
        };

        // Clip to AABB:
        const POINT_RADIUS: f32 = 4.0;
        let point_rect = Rect::from_extents(
            Vec2::new(camera_center_point.x - POINT_RADIUS, camera_center_point.y - POINT_RADIUS),
            Vec2::new(camera_center_point.x + POINT_RADIUS, camera_center_point.y + POINT_RADIUS)
        );

        if widget.draw_data.clip_rect().contains_rect(&point_rect) {
            // Center derived from iso coords (GREEN):
            draw_list.add_circle(camera_center_point.to_array(),
                                 POINT_RADIUS,
                                 imgui::ImColor32::from_rgb(0, 255, 0))
                                 .build();

            // Center derived from screen-space rect (RED):
            draw_list.add_circle(camera_rect.center().to_array(),
                                 POINT_RADIUS * 0.5,
                                 imgui::ImColor32::from_rgb(255, 0, 0))
                                 .build();
        }
    }

    fn draw_debug_controls(&mut self, context: &mut MinimapRenderContext) {
        if !self.show_debug_controls {
            return;
        }

        let parent_window_size = context.widget().window_rect.size().to_array();
        let parent_window_pos  = context.widget().window_rect.position().to_array();

        let window_pos = [
            parent_window_pos[0] + parent_window_size[0] + 10.0,
            parent_window_pos[1] - 115.0,
        ];

        let window_flags =
            imgui::WindowFlags::NO_RESIZE
            | imgui::WindowFlags::NO_SCROLLBAR
            | imgui::WindowFlags::NO_COLLAPSE;

        let ui = context.ui_sys.ui();
        let mut show_debug_controls = self.show_debug_controls;

        let window_name =
            format!("Minimap Debug | {}x{}",
                    context.widget().map_size_in_cells.x as i32,
                    context.widget().map_size_in_cells.y as i32);

        ui.window(window_name)
            .opened(&mut show_debug_controls)
            .flags(window_flags)
            .position(window_pos, imgui::Condition::FirstUseEver)
            .always_auto_resize(true)
            .build(|| {
                let camera_center_iso = context.camera.iso_world_position();
                let camera_center_cell = coords::iso_to_cell_f32(camera_center_iso);

                let camera_edges_near_playable_area_limits =
                    context.widget().camera_rect_edges_near_playable_map_area_limits();

                let visible_cells = MinimapWidget::calc_minimap_visible_cells(
                    context.widget().map_size_in_cells,
                    context.widget().transform.zoom());

                let (uv_min, uv_max) = context.widget().current_minimap_uv_window();

                if ui.small_button("Reset") {
                    context.camera.center();
                    context.widget_mut().transform.reset();
                }

                let widget = context.widget_mut();

                ui.same_line();
                ui.checkbox("Debug Draw", &mut self.enable_debug_draw);
                ui.same_line();
                ui.checkbox("Clipped", &mut self.base_renderer.apply_widget_clip_rect);

                // newline
                ui.checkbox("Auto Scroll", &mut widget.auto_scroll);
                ui.same_line();
                ui.checkbox("Auto Zoom", &mut widget.auto_zoom);

                ui.input_float("Scroll Speed", &mut widget.scroll_speed_px_per_sec)
                    .display_format("%.2f")
                    .step(1.0)
                    .build();

                ui.input_float("Scroll X", &mut widget.transform.offsets.x)
                    .display_format("%.2f")
                    .step(1.0)
                    .build();

                ui.input_float("Scroll Y", &mut widget.transform.offsets.y)
                    .display_format("%.2f")
                    .step(1.0)
                    .build();

                if ui.input_float("Zoom", &mut widget.transform.scale)
                    .display_format("%.2f")
                    .step(MinimapTransform::ZOOM_STEP)
                    .build()
                {
                    widget.transform.scale =
                        widget.transform.scale.clamp(
                            MinimapTransform::ZOOM_MIN,
                            MinimapTransform::ZOOM_MAX);
                }

                ui.separator();

                ui.text(format!("UV Window          : {}", uv_max - uv_min));
                ui.text(format!("UV Window Min/Max  : {} / {}", uv_min, uv_max));
                ui.text(format!("Visible Cells      : {}", visible_cells));
                ui.text(format!("Camera Center Iso  : {}", camera_center_iso.0));
                ui.text(format!("Camera Center Cell : {}", camera_center_cell.0));

                if camera_edges_near_playable_area_limits.is_empty() {
                    ui.text("Camera Edges Near Limit : None");
                } else {
                    ui.text("Camera Edges Near Limit :");
                    ui.same_line();
                    ui.text_colored(Color::red().to_array(), camera_edges_near_playable_area_limits.to_string());
                }
            });

        self.show_debug_controls = show_debug_controls;
    }
}
