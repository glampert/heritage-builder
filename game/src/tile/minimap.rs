use rand::Rng;
use std::path::PathBuf;
use smallvec::SmallVec;
use bitflags::bitflags;
use strum::{EnumCount, IntoEnumIterator, EnumProperty};
use strum_macros::{EnumCount, EnumIter, EnumProperty};

use super::{
    bitflags_with_display,
    TileKind, TileMap, TileMapLayerKind, sets::TileDef,
    camera::Camera, water, road, BASE_TILE_SIZE,
};

use crate::{
    singleton,
    engine::time::Seconds,
    save::{PreLoadContext, PostLoadContext},
    app::input::{InputSystem, InputAction, MouseButton},
    imgui_ui::{self, UiSystem, UiTextureHandle},
    utils::{Color, Rect, Size, Vec2, coords::{self, Cell, CellF32, IsoPointF32}, platform::paths},
    render::{RenderSystem, TextureCache, TextureFilter, TextureWrapMode, TextureHandle, TextureSettings},
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
    const BLACK:         Self = Self { r: 0,   g: 0,   b: 0,   a: 255 };
    const WHITE:         Self = Self { r: 255, g: 255, b: 255, a: 255 };
    const CYAN:          Self = Self { r: 0,   g: 255, b: 255, a: 255 };
    const MAGENTA:       Self = Self { r: 255, g: 0,   b: 255, a: 255 };
    const LIGHT_RED:     Self = Self { r: 250, g: 35,  b: 35,  a: 255 };
    const DARK_RED:      Self = Self { r: 195, g: 15,  b: 15,  a: 255 };
    const LIGHT_PINK:    Self = Self { r: 220, g: 20,  b: 195, a: 255 };
    const DARK_PINK:     Self = Self { r: 140, g: 5,   b: 120, a: 255 };
    const LIGHT_PURPLE:  Self = Self { r: 165, g: 70,  b: 185, a: 255 };
    const DARK_PURPLE:   Self = Self { r: 80,  g: 25,  b: 90,  a: 255 };
    const LIGHT_GREEN_1: Self = Self { r: 112, g: 125, b: 55,  a: 255 };
    const LIGHT_GREEN_2: Self = Self { r: 100, g: 120, b: 50,  a: 255 };
    const DARK_GREEN_1:  Self = Self { r: 10,  g: 115, b: 25,  a: 255 };
    const DARK_GREEN_2:  Self = Self { r: 25,  g: 125, b: 40,  a: 255 };
    const LIGHT_YELLOW:  Self = Self { r: 210, g: 225, b: 20,  a: 255 };
    const DARK_YELLOW:   Self = Self { r: 225, g: 200, b: 20,  a: 255 };
    const LIGHT_BLUE:    Self = Self { r: 15,  g: 100, b: 230, a: 255 };
    const DARK_BLUE:     Self = Self { r: 30,  g: 100, b: 115, a: 255 };
    const LIGHT_BROWN:   Self = Self { r: 165, g: 122, b: 81,  a: 255 };
    const DARK_BROWN:    Self = Self { r: 138, g: 92,  b: 68,  a: 255 };
    const LIGHT_GRAY:    Self = Self { r: 100, g: 100, b: 100, a: 255 };
    const DARK_GRAY_1:   Self = Self { r: 90,  g: 85,  b: 75,  a: 255 };
    const DARK_GRAY_2:   Self = Self { r: 80,  g: 75,  b: 65,  a: 255 };

    #[inline]
    fn vacant_lot() -> Self {
        Self::LIGHT_YELLOW
    }

    #[inline]
    fn water() -> Self {
        Self::DARK_BLUE
    }

    #[inline]
    fn road(tile_def: &'static TileDef) -> Self {
        match road::kind(tile_def) {
            road::RoadKind::Dirt  => Self::LIGHT_BROWN,
            road::RoadKind::Paved => Self::DARK_BROWN,
        }
    }

    #[inline]
    fn empty_land() -> Self {
        // Alternate randomly between two similar colors
        // to give the minimap a more pleasant texture.
        if rand::rng().random_bool(0.5) {
            Self::LIGHT_GREEN_1
        } else {
            Self::LIGHT_GREEN_2
        }
    }

    #[inline]
    fn vegetation() -> Self {
        if rand::rng().random_bool(0.5) {
            Self::DARK_GREEN_1
        } else {
            Self::DARK_GREEN_2
        }
    }

    #[inline]
    fn rocks() -> Self {
        if rand::rng().random_bool(0.5) {
            Self::DARK_GRAY_1
        } else {
            Self::DARK_GRAY_2
        }
    }

    fn building(tile_def: &'static TileDef) -> Self {
        if tile_def.is_house() {
            return Self::DARK_YELLOW;
        }
        // TEMP: Give different building sizes a unique color for now.
        // TODO: Color should come from building category (e.g.: government, services, industry, etc).
        let size = tile_def.size_in_cells();
        if size.width == 1 {
            Self::WHITE
        } else if size.width == 2 {
            Self::CYAN
        } else if size.width == 3 {
            Self::MAGENTA
        } else if size.width == 4 {
            Self::LIGHT_PURPLE
        } else if size.width == 5 {
            Self::LIGHT_RED
        } else {
            Self::DARK_PINK
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
            let settings = TextureSettings {
                filter: TextureFilter::Nearest,
                wrap_mode: TextureWrapMode::ClampToBorder,
                gen_mipmaps: false,
            };
            self.handle = tex_cache.new_uninitialized_texture("minimap",
                                                              self.size,
                                                              Some(settings));
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
        if (cell.x < 0 || cell.x >= self.size.width)
            || (cell.y < 0 || cell.y >= self.size.height)
        {
            return false;
        }
        true
    }
}

// ----------------------------------------------
// MinimapIcon / MinimapIconInstance
// ----------------------------------------------

pub const MINIMAP_ICON_SIZE: f32 = 20.0; // W & H in pixels.
pub const MINIMAP_ICON_COUNT: usize = MinimapIcon::COUNT;
pub const MINIMAP_ICON_DEFAULT_LIFETIME: Seconds = 5.0;

#[repr(u8)]
#[derive(Copy, Clone, EnumCount, EnumIter, EnumProperty)]
pub enum MinimapIcon {
    #[strum(props(AssetPath = "ui/alert_icon.png"))]
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
        let settings = TextureSettings {
            filter: TextureFilter::Linear,
            gen_mipmaps: false,
            ..Default::default()
        };

        for icon in MinimapIcon::iter() {
            let texture = &mut self.textures[icon as usize];
            debug_assert!(!texture.is_valid(), "Minimap icon texture is already loaded!");

            *texture = tex_cache.load_texture_with_settings(
                icon.asset_path().to_str().unwrap(),
                Some(settings)
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
    widget: MinimapWidgetImGui,
}

impl Minimap {
    pub fn new(size_in_cells: Size) -> Self {
        Self {
            // One pixel per tile map cell.
            texture: MinimapTexture::new(size_in_cells),
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
        self.texture.pre_load(context.tex_cache_mut());
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

    // Draw the minimap using ImGui, nestled inside a window.
    #[inline]
    pub fn draw(&mut self, render_sys: &impl RenderSystem, ui_sys: &UiSystem) {
        self.widget.draw(render_sys, ui_sys, self.texture.handle, &self.icons);
    }

    #[inline]
    pub fn draw_debug_ui(&mut self, camera: &mut Camera, ui_sys: &UiSystem) {
        self.widget.draw_debug_ui(camera, ui_sys);
    }
}

// ----------------------------------------------
// MinimapWidget
// ----------------------------------------------

trait MinimapWidget {
    fn update(&mut self,
              camera: &mut Camera,
              input_sys: &dyn InputSystem,
              ui_sys: &UiSystem,
              size_in_cells: Size,
              delta_time_secs: Seconds);

    fn draw(&mut self,
            render_sys: &impl RenderSystem,
            ui_sys: &UiSystem,
            tex_handle: TextureHandle,
            icons: &[MinimapIconInstance]);

    fn draw_debug_ui(&mut self,
                     camera: &mut Camera,
                     ui_sys: &UiSystem);
}

const MINIMAP_WITH_DEBUG_CONTROLS: bool = true;

// Rotate the minimap -45 degrees to match our isometric world projection.
const MINIMAP_ROTATION_ANGLE: f32 = -45.0 * (std::f32::consts::PI / 180.0);

bitflags_with_display! {
    #[derive(Copy, Clone, Default)]
    struct RectCorners: u32 {
        const TopLeft     = 1 << 0;
        const TopRight    = 1 << 1;
        const BottomLeft  = 1 << 2;
        const BottomRight = 1 << 3;
    }
}

struct MinimapTransform {
    offsets: Vec2, // Minimap texture offset/panning in cells (pixels), from minimap origin (0,0).
    scale: f32,    // Zoom amount [1..N] range. 1=draw full minimap, >1 zooms in.
    rotated: bool, // Apply 45 degrees isometric rotation when drawing the minimap?
}

impl MinimapTransform {
    #[inline]
    fn zoom(&self) -> f32 {
        self.scale
    }

    #[inline]
    fn is_valid(&self) -> bool {
        self.scale > 0.0
    }
}

impl Default for MinimapTransform {
    fn default() -> Self {
        Self { offsets: Vec2::default(), scale: 1.0, rotated: true }
    }
}

struct MinimapScroll {
    auto_scroll: bool, // Scroll minimap when camera rect touches the minimap edges?
    speed_px: f32,     // Scroll speed in pixels per second when `auto_scroll=true`.
    offsets: Vec2,     // Scroll offsets (translation) in cells/pixels.
}

impl Default for MinimapScroll {
    fn default() -> Self {
        Self { auto_scroll: true, speed_px: 30.0, offsets: Vec2::default() }
    }
}

#[derive(Default)]
struct MinimapDrawInfo {
    rect: Rect,
    aabb: Rect, // Axis-aligned bounding box of `rect` if rotated, same as `rect` if not.
    corners: [Vec2; 4],
}

struct MinimapWidgetImGui {
    is_open: bool,
    cursor_pos: Vec2,  // Cursor screen space position, cached on update().
    widget_rect: Rect, // Minimap widget screen space rect, relative to window rect.
    window_rect: Rect, // Widget window screen rect in absolute screen space.

    minimap_scroll: MinimapScroll,
    minimap_transform: MinimapTransform, // Zoom (scale) & XY offsets (translation).
    minimap_draw_info: MinimapDrawInfo,  // Absolute screen rect where we render the minimap texture.
    minimap_size_in_cells: Vec2,         // Minimap/TileMap size in cells (1 TileMap cell = 1 minimap pixel).
    minimap_uvs: (Vec2, Vec2),           // Min/Max minimap texture UVs for rendering.

    camera_center: CellF32,   // Camera center cell / minimap pixel.
    camera_screen_rect: Rect, // Camera rect in absolute screen space, ready for overlay rendering.
    camera_corners_out_of_bounds: RectCorners,

    // Debug switches:
    enable_debug_draw: bool,
    enable_debug_controls: bool,
    show_debug_controls: bool,
}

impl Default for MinimapWidgetImGui {
    fn default() -> Self {
        Self {
            is_open: true,
            cursor_pos: Vec2::default(),
            widget_rect: Rect::new(
                Vec2::new(35.0, 55.0),
                Vec2::new(128.0, 128.0)
            ),
            window_rect: Rect::default(),
            minimap_scroll: MinimapScroll::default(),
            minimap_transform: MinimapTransform::default(),
            minimap_draw_info: MinimapDrawInfo::default(),
            minimap_size_in_cells: Vec2::default(),
            minimap_uvs: (Vec2::default(), Vec2::default()),
            camera_center: CellF32::default(),
            camera_screen_rect: Rect::default(),
            camera_corners_out_of_bounds: RectCorners::default(),
            enable_debug_draw: MINIMAP_WITH_DEBUG_CONTROLS,
            enable_debug_controls: MINIMAP_WITH_DEBUG_CONTROLS,
            show_debug_controls: MINIMAP_WITH_DEBUG_CONTROLS,
        }
    }
}

impl MinimapWidget for MinimapWidgetImGui {
    fn update(&mut self,
              camera: &mut Camera,
              input_sys: &dyn InputSystem,
              ui_sys: &UiSystem,
              size_in_cells: Size,
              delta_time_secs: Seconds) {
        debug_assert!(size_in_cells.is_valid());
        debug_assert!(self.widget_rect.is_valid());
        debug_assert!(self.minimap_transform.is_valid());

        // Must update these every frame:
        self.cursor_pos = input_sys.cursor_pos();
        self.window_rect = Self::calc_window_rect(&self.widget_rect, ui_sys);
        self.minimap_size_in_cells = size_in_cells.to_vec2();
        self.camera_center = Self::calc_camera_center_cell(camera);
        self.camera_screen_rect = self.calc_camera_rect_in_screen_px(camera);

        self.minimap_draw_info =
            Self::calc_minimap_draw_info(&self.widget_rect,
                                         &self.window_rect,
                                         self.is_minimap_rotated());

        self.minimap_transform.offsets =
            Self::calc_minimap_offsets_from_center(self.minimap_size_in_cells,
                                                   CellF32(self.minimap_size_in_cells * 0.5), // TileMap center.
                                                   self.minimap_transform.zoom());

        self.minimap_uvs =
            Self::calc_minimap_rect_uvs(self.minimap_size_in_cells,
                                        self.minimap_transform.offsets,
                                        self.minimap_transform.zoom());

        self.camera_corners_out_of_bounds =
            Self::calc_camera_corners_out_of_bounds(&self.camera_screen_rect,
                                                    &self.minimap_draw_info,
                                                    self.is_minimap_rotated());

        // Cursor -> minimap cell picking:
        if input_sys.mouse_button_state(MouseButton::Left) == InputAction::Press {
            if let Some(teleport_destination_iso) = self.pick_cursor_pos() {
                camera.teleport_iso(teleport_destination_iso);
            }
        }

        if self.minimap_scroll.auto_scroll && self.minimap_transform.zoom() > 1.0 {
            self.update_minimap_scrolling(delta_time_secs);
        }
    }

    fn draw(&mut self,
            render_sys: &impl RenderSystem,
            ui_sys: &UiSystem,
            tex_handle: TextureHandle,
            icons: &[MinimapIconInstance]) {
        if self.is_open {
            self.draw_widget_window(render_sys, ui_sys, tex_handle, icons);
        } else {
            self.draw_open_button(ui_sys);
        }
    }

    fn draw_debug_ui(&mut self,
                     camera: &mut Camera,
                     ui_sys: &UiSystem) {
        if !self.is_open || !self.enable_debug_controls || !self.show_debug_controls {
            return;
        }
        self.draw_debug_controls(camera, ui_sys);
    }
}

impl MinimapWidgetImGui {
    fn draw_widget_window(&mut self,
                          render_sys: &impl RenderSystem,
                          ui_sys: &UiSystem,
                          tex_handle: TextureHandle,
                          icons: &[MinimapIconInstance]) {
        debug_assert!(self.window_rect.is_valid());

        let window_size = self.window_rect.size_as_vec2().to_array();
        let window_pos  = self.window_rect.position().to_array();

        let window_flags =
            imgui::WindowFlags::NO_RESIZE
            | imgui::WindowFlags::NO_SCROLLBAR
            | imgui::WindowFlags::NO_MOVE
            | imgui::WindowFlags::NO_COLLAPSE;

        let ui = ui_sys.ui();
        let mut is_open = self.is_open;

        ui.window("Minimap")
            .opened(&mut is_open)
            .flags(window_flags)
            .position(window_pos, imgui::Condition::Always)
            .size(window_size, imgui::Condition::Always)
            .build(|| {
                let tex_cache = render_sys.texture_cache();
                let ui_texture = ui_sys.to_ui_texture(tex_cache, tex_handle);

                self.draw_minimap(ui_sys, ui_texture);
                self.draw_icons(render_sys, ui_sys, icons);

                if self.enable_debug_controls {
                    // Debug controls checkbox at the minimap widget's bottom:
                    ui.dummy([0.0, window_size[1] - 65.0]);
                    ui.dummy([2.0, 0.0]); ui.same_line();
                    ui.checkbox("Debug", &mut self.show_debug_controls);
                }
            });

        self.is_open = is_open;
    }

    fn draw_open_button(&mut self, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();

        let window_pos = [5.0, ui.io().display_size[1] - 35.0];

        let window_flags =
            imgui::WindowFlags::NO_DECORATION
            | imgui::WindowFlags::NO_BACKGROUND
            | imgui::WindowFlags::NO_RESIZE
            | imgui::WindowFlags::NO_SCROLLBAR
            | imgui::WindowFlags::NO_MOVE
            | imgui::WindowFlags::NO_COLLAPSE;

        ui.window("Minimap Button")
            .flags(window_flags)
            .position(window_pos, imgui::Condition::Always)
            .always_auto_resize(true)
            .bg_alpha(0.0)
            .build(|| {
                if imgui_ui::icon_button(ui_sys, imgui_ui::icons::ICON_MAP, Some("Open Minimap")) {
                    self.is_open = true;
                }
            });
    }

    fn draw_debug_controls(&mut self, camera: &mut Camera, ui_sys: &UiSystem) {
        debug_assert!(self.window_rect.is_valid());

        let parent_window_size = self.window_rect.size_as_vec2().to_array();
        let parent_window_pos  = self.window_rect.position().to_array();

        let window_pos = [
            parent_window_pos[0] + parent_window_size[0] + 10.0,
            parent_window_pos[1] - 55.0,
        ];

        let window_flags =
            imgui::WindowFlags::NO_RESIZE
            | imgui::WindowFlags::NO_SCROLLBAR
            | imgui::WindowFlags::NO_COLLAPSE;

        let ui = ui_sys.ui();
        let mut show_debug_controls = self.show_debug_controls;

        ui.window(format!("Minimap Debug | {}x{}", self.minimap_size_in_cells.x as i32, self.minimap_size_in_cells.y as i32))
            .opened(&mut show_debug_controls)
            .flags(window_flags)
            .position(window_pos, imgui::Condition::FirstUseEver)
            .always_auto_resize(true)
            .build(|| {
                if ui.small_button("Reset") {
                    self.minimap_transform.scale   = 1.0;
                    self.minimap_transform.offsets = Vec2::default();
                    self.minimap_scroll.offsets    = Vec2::default();
                    camera.center();
                }

                ui.same_line();
                ui.checkbox("Rotated", &mut self.minimap_transform.rotated);
                ui.same_line();
                ui.checkbox("Auto Scroll", &mut self.minimap_scroll.auto_scroll);
                ui.same_line();
                ui.checkbox("Debug Draw", &mut self.enable_debug_draw);

                ui.input_float("Scroll Speed", &mut self.minimap_scroll.speed_px)
                    .display_format("%.2f")
                    .step(1.0)
                    .build();

                ui.input_float("Scroll X", &mut self.minimap_scroll.offsets.x)
                    .display_format("%.2f")
                    .step(1.0)
                    .build();

                ui.input_float("Scroll Y", &mut self.minimap_scroll.offsets.y)
                    .display_format("%.2f")
                    .step(1.0)
                    .build();

                if ui.input_float("Zoom", &mut self.minimap_transform.scale)
                    .display_format("%.2f")
                    .step(0.1)
                    .build()
                {
                    // Clamp to 10x max zoom.
                    self.minimap_transform.scale = self.minimap_transform.scale.clamp(1.0, 10.0);
                }

                let minimap_visible_cells =
                    Self::calc_minimap_visible_cells(self.minimap_size_in_cells,
                                                     self.minimap_transform.zoom());

                ui.separator();

                ui.text(format!("XY Offsets: {}", self.minimap_transform.offsets));
                ui.text(format!("UVs: (Min:{}, Max:{})", self.minimap_uvs.0, self.minimap_uvs.1));
                ui.text(format!("Visible Cells: {}", minimap_visible_cells));
                ui.text(format!("Camera Center: {}", self.camera_center.0));
                ui.text(format!("Camera Rect: {}", self.camera_screen_rect));

                if self.camera_corners_out_of_bounds.is_empty() {
                    ui.text("Camera Corners Out: None");
                } else {
                    ui.text("Camera Corners Out:");
                    ui.same_line();
                    ui.text_colored(Color::red().to_array(), self.camera_corners_out_of_bounds.to_string());
                }
            });

        self.show_debug_controls = show_debug_controls;
    }

    fn draw_minimap(&self, ui_sys: &UiSystem, ui_texture: UiTextureHandle) {
        debug_assert!(self.minimap_size_in_cells != Vec2::zero());
        debug_assert!(self.minimap_draw_info.rect.is_valid());
        debug_assert!(self.minimap_draw_info.aabb.is_valid());

        let draw_list = ui_sys.ui().get_window_draw_list();
        self.draw_texture_rect(&draw_list, ui_texture);
        self.draw_outline_rect(&draw_list);
        self.draw_camera_rect(&draw_list);
    }

    fn draw_icons(&self,
                  render_sys: &impl RenderSystem,
                  ui_sys: &UiSystem,
                  icons: &[MinimapIconInstance]) {
        debug_assert!(self.minimap_size_in_cells != Vec2::zero());
        debug_assert!(self.minimap_draw_info.rect.is_valid());
        debug_assert!(self.minimap_draw_info.aabb.is_valid());

        if icons.is_empty() {
            return;
        }

        let draw_list = ui_sys.ui().get_window_draw_list();
        let tex_cache = render_sys.texture_cache();
        let minimap_center = self.minimap_draw_info.rect.center(); // Minimap center.

        for icon in icons {
            if icon.lifetime <= 0.0 || icon.time_left <= 0.0 {
                continue;
            }

            let mut icon_center =
                self.cell_to_scaled_minimap_widget_px(CellF32::from_integer_cell(icon.target_cell));

            if self.is_minimap_rotated() {
                icon_center = icon_center.rotate_around_point(minimap_center, MINIMAP_ROTATION_ANGLE);
            }

            const ICON_HALF_SIZE: f32 = MINIMAP_ICON_SIZE / 2.0;
            let icon_rect = Rect::from_extents(
                Vec2::new(icon_center.x - ICON_HALF_SIZE, icon_center.y - ICON_HALF_SIZE),
                Vec2::new(icon_center.x + ICON_HALF_SIZE, icon_center.y + ICON_HALF_SIZE)
            );

            // Fade-out based on remaining lifetime seconds.
            let icon_tint_alpha = (icon.time_left / icon.lifetime).clamp(0.0, 1.0);
            let icon_tint = Color::new(icon.tint.r, icon.tint.g, icon.tint.b, icon_tint_alpha);

            let icon_ui_texture = ui_sys.to_ui_texture(tex_cache, icon.texture);

            draw_list
                .add_image(icon_ui_texture, icon_rect.min.to_array(), icon_rect.max.to_array())
                .col(imgui::ImColor32::from_rgba_f32s(icon_tint.r, icon_tint.g, icon_tint.b, icon_tint_alpha))
                .build();
        }
    }

    fn draw_texture_rect(&self, draw_list: &imgui::DrawListMut<'_>, ui_texture: UiTextureHandle) {
        let minimap_corners = &self.minimap_draw_info.corners;
        let mut uv_min = self.minimap_uvs.0;
        let mut uv_max = self.minimap_uvs.1;

        let scroll = self.minimap_scroll.offsets / self.minimap_size_in_cells;

        uv_min += scroll;
        uv_max += scroll;

        draw_list
            .add_image_quad(
                ui_texture,
                minimap_corners[0].to_array(),
                minimap_corners[1].to_array(),
                minimap_corners[2].to_array(),
                minimap_corners[3].to_array())
            .uv(
                [uv_min.x, uv_max.y],
                uv_max.to_array(),
                [uv_max.x, uv_min.y],
                uv_min.to_array())
            .build();
    }

    fn draw_outline_rect(&self, draw_list: &imgui::DrawListMut<'_>) {
        let (rect_color, cursor_inside_minimap) = {
            if self.enable_debug_draw
                && coords::is_screen_point_inside_diamond(self.cursor_pos, &self.minimap_draw_info.corners)
            {
                (imgui::ImColor32::from_rgb(255, 0, 0), true) // Red when cursor inside.
            } else {
                (imgui::ImColor32::BLACK, false)
            }
        };

        if cursor_inside_minimap {
            draw_list.add_circle(self.cursor_pos.to_array(),
                                 4.0,
                                 rect_color)
                                 .build();
        }

        draw_list.add_rect(self.minimap_draw_info.aabb.min.to_array(),
                           self.minimap_draw_info.aabb.max.to_array(),
                           rect_color)
                           .build();
    }

    fn draw_camera_rect(&self, draw_list: &imgui::DrawListMut<'_>) {
        let outline_color = {
            if self.enable_debug_draw && !self.camera_corners_out_of_bounds.is_empty() {
                // Color it red if any corner of the camera rect falls outside the minimap.
                imgui::ImColor32::from_rgb(255, 0, 0)
            } else {
                imgui::ImColor32::WHITE
            }
        };

        draw_list.add_rect(self.camera_screen_rect.min.to_array(),
                           self.camera_screen_rect.max.to_array(),
                           outline_color)
                           .build();

        // Draw a green debug circle at the camera's center:
        if self.enable_debug_draw {
            let camera_center_point = {
                if self.is_minimap_rotated() {
                    // We want to visualize the computed `camera_center` cell.
                    let camera_center_screen  = self.cell_to_scaled_minimap_widget_px(self.camera_center);
                    let minimap_center_screen = self.minimap_draw_info.rect.center();
                    camera_center_screen.rotate_around_point(minimap_center_screen, MINIMAP_ROTATION_ANGLE)
                } else {
                    self.camera_screen_rect.center()
                }
            };

            // Clip to AABB:
            const POINT_RADIUS: f32 = 4.0;
            let point_rect = Rect::from_extents(
                camera_center_point - Vec2::new(POINT_RADIUS, POINT_RADIUS),
                camera_center_point + Vec2::new(POINT_RADIUS, POINT_RADIUS)
            );

            if self.minimap_draw_info.aabb.contains_rect(&point_rect) {
                draw_list.add_circle(camera_center_point.to_array(),
                                     POINT_RADIUS,
                                     imgui::ImColor32::from_rgb(0, 255, 0))
                                     .build();
            }
        }
    }

    // Returns floating-point isometric coords without rounding to integer cell space.
    fn pick_cursor_pos(&self) -> Option<IsoPointF32> {
        if !coords::is_screen_point_inside_diamond(self.cursor_pos, &self.minimap_draw_info.corners) {
            return None; // Cursor outside minimap.
        }

        // Undo minimap rotation first if needed:
        let minimap_px = {
            if self.is_minimap_rotated() {
                let minimap_center_screen = self.minimap_draw_info.rect.center();
                self.cursor_pos.rotate_around_point(minimap_center_screen, -MINIMAP_ROTATION_ANGLE)
            } else {
                self.cursor_pos
            }
        };

        // Convert widget px -> fractional cell (using UV window inverse).
        let cell = self.scaled_minimap_widget_px_to_cell(minimap_px);

        // UV may be outside window -> return None if outside full map.
        if cell.0.x < 0.0 || cell.0.x > self.minimap_size_in_cells.x ||
           cell.0.y < 0.0 || cell.0.y > self.minimap_size_in_cells.y
        {
            return None;
        }

        Some(coords::cell_to_iso_f32(cell, BASE_TILE_SIZE))
    }

    fn update_minimap_scrolling(&mut self, delta_time_secs: Seconds) {
        let (uv_min, uv_max) = self.minimap_uvs;
        let mut scrollable_corners = RectCorners::all();

        // Corners already at their limits will not scroll further.
        if uv_min.x <= 0.0 {
            scrollable_corners.remove(RectCorners::BottomLeft);
        }
        if uv_min.y <= 0.0 {
            scrollable_corners.remove(RectCorners::BottomRight);
        }
        if uv_max.x >= 1.0 {
            scrollable_corners.remove(RectCorners::TopRight);
        }
        if uv_max.y >= 1.0 {
            scrollable_corners.remove(RectCorners::TopLeft);
        }

        if scrollable_corners.is_empty() || self.camera_corners_out_of_bounds.is_empty() {
            return;
        }

        // Minimap scrolling:
        if self.camera_corners_out_of_bounds.intersects(RectCorners::TopLeft)
            && scrollable_corners.intersects(RectCorners::TopLeft)
        {
            self.minimap_scroll.offsets.y += self.minimap_scroll.speed_px * delta_time_secs;
        }
        if self.camera_corners_out_of_bounds.intersects(RectCorners::BottomRight)
            && scrollable_corners.intersects(RectCorners::BottomRight)
        {
            self.minimap_scroll.offsets.y -= self.minimap_scroll.speed_px * delta_time_secs;
        }
        if self.camera_corners_out_of_bounds.intersects(RectCorners::TopRight)
            && scrollable_corners.intersects(RectCorners::TopRight)
        {
            self.minimap_scroll.offsets.x += self.minimap_scroll.speed_px * delta_time_secs;
        }
        if self.camera_corners_out_of_bounds.intersects(RectCorners::BottomLeft)
            && scrollable_corners.intersects(RectCorners::BottomLeft)
        {
            self.minimap_scroll.offsets.x -= self.minimap_scroll.speed_px * delta_time_secs;
        }

        // Clamp to minimap scrollable size:
        let minimap_visible_cells =
            Self::calc_minimap_visible_cells(self.minimap_size_in_cells,
                                             self.minimap_transform.zoom());

        // We can scroll this many tiles on each side, so divide by 2.
        let scrollable_cells = (self.minimap_size_in_cells - minimap_visible_cells) * 0.5;

        self.minimap_scroll.offsets.x = self.minimap_scroll.offsets.x.min(scrollable_cells.x);
        self.minimap_scroll.offsets.y = self.minimap_scroll.offsets.y.min(scrollable_cells.y);
        self.minimap_scroll.offsets.x = self.minimap_scroll.offsets.x.max(-scrollable_cells.x);
        self.minimap_scroll.offsets.y = self.minimap_scroll.offsets.y.max(-scrollable_cells.y);
    }

    const CAMERA_CORNER_MARGIN_PX: f32 = 4.0;

    fn calc_map_iso_bounds(size_in_cells: Vec2) -> Rect {
        let w = size_in_cells.x;
        let h = size_in_cells.y;
        let points = [
            coords::cell_to_iso_f32(CellF32(Vec2::new(0.0, 0.0)), BASE_TILE_SIZE).0,
            coords::cell_to_iso_f32(CellF32(Vec2::new(0.0, h  )), BASE_TILE_SIZE).0,
            coords::cell_to_iso_f32(CellF32(Vec2::new(w,   0.0)), BASE_TILE_SIZE).0,
            coords::cell_to_iso_f32(CellF32(Vec2::new(w,   h  )), BASE_TILE_SIZE).0,
        ];
        Rect::aabb(&points)
    }

    fn calc_camera_corners_out_of_bounds(camera_screen_rect: &Rect,
                                         minimap_draw_info: &MinimapDrawInfo,
                                         is_rotated: bool) -> RectCorners {
        let mut corners_outside = RectCorners::empty();

        if is_rotated {
            let minimap_corners = &minimap_draw_info.corners;

            // NOTE: RectCorners top/bottom are inverted here due to the minimap rotation.
            if !coords::is_screen_point_inside_diamond(camera_screen_rect.top_left(), minimap_corners) {
                corners_outside |= RectCorners::BottomLeft;
            }
            if !coords::is_screen_point_inside_diamond(camera_screen_rect.top_right(), minimap_corners) {
                corners_outside |= RectCorners::BottomRight;
            }
            if !coords::is_screen_point_inside_diamond(camera_screen_rect.bottom_left(), minimap_corners) {
                corners_outside |= RectCorners::TopLeft;
            }
            if !coords::is_screen_point_inside_diamond(camera_screen_rect.bottom_right(), minimap_corners) {
                corners_outside |= RectCorners::TopRight;
            }
        } else {
            let minimap_aabb = &minimap_draw_info.aabb;

            if camera_screen_rect.max.x >= minimap_aabb.max.x - Self::CAMERA_CORNER_MARGIN_PX {
                corners_outside |= RectCorners::TopRight;
            }
            if camera_screen_rect.max.y >= minimap_aabb.max.y - Self::CAMERA_CORNER_MARGIN_PX {
                corners_outside |= RectCorners::BottomRight;
            }
            if camera_screen_rect.min.x <= minimap_aabb.min.x + Self::CAMERA_CORNER_MARGIN_PX {
                corners_outside |= RectCorners::BottomLeft;
            }
            if camera_screen_rect.min.y <= minimap_aabb.min.y + Self::CAMERA_CORNER_MARGIN_PX {
                corners_outside |= RectCorners::TopLeft;
            }
        }

        corners_outside
    }

    // Rect in screen space, ready to be drawn with ImGui.
    fn calc_camera_rect_in_screen_px(&self, camera: &Camera) -> Rect {
        debug_assert!(self.minimap_size_in_cells != Vec2::zero());

        let center_iso = camera.iso_world_position();
        let half_iso   = camera.iso_viewport_center();

        let iso_min = IsoPointF32(center_iso.0 - half_iso.0);
        let iso_max = IsoPointF32(center_iso.0 + half_iso.0);

        let mut camera_rect = if self.is_minimap_rotated() {
            // Convert iso rect corners to fractional cell coordinates (continuous):
            let cell_min = coords::iso_to_cell_f32(iso_min, BASE_TILE_SIZE);
            let cell_max = coords::iso_to_cell_f32(iso_max, BASE_TILE_SIZE);

            // Ensure correct ordering (min <= max).
            let cell_x_min = cell_min.0.x.min(cell_max.0.x);
            let cell_x_max = cell_min.0.x.max(cell_max.0.x);
            let cell_y_min = cell_min.0.y.min(cell_max.0.y);
            let cell_y_max = cell_min.0.y.max(cell_max.0.y);

            // Build cell rect corners in fractional cell coords (CellF32):
            let top_left     = CellF32(Vec2::new(cell_x_min, cell_y_min));
            let bottom_right = CellF32(Vec2::new(cell_x_max, cell_y_max));

            // Map these cell coords into widget pixels using the zoom-aware mapper:
            let px_min = self.cell_to_scaled_minimap_widget_px(top_left);
            let px_max = self.cell_to_scaled_minimap_widget_px(bottom_right);

            Rect::from_extents(px_min, px_max)
        } else {
            // Convert iso -> UV directly using world iso bounds:
            fn point_to_uv(rect: &Rect, iso: IsoPointF32) -> Vec2 {
                Vec2::new((iso.0.x - rect.min.x) / rect.width(),
                          (iso.0.y - rect.min.y) / rect.height())
            }

            let bounds = Self::calc_map_iso_bounds(self.minimap_size_in_cells);
            let uv_min_full = point_to_uv(&bounds, iso_min);
            let uv_max_full = point_to_uv(&bounds, iso_max);

            // Convert full-map UV -> zoomed-window UV:
            let uv_min_win = self.fullmap_uv_to_window_uv(uv_min_full);
            let uv_max_win = self.fullmap_uv_to_window_uv(uv_max_full);

            // Convert zoomed-window-UV -> widget pixels:
            let px_min = self.minimap_uv_to_minimap_px(uv_min_win);
            let px_max = self.minimap_uv_to_minimap_px(uv_max_win);

            Rect::from_extents(px_min, px_max)
        };

        // Rotate if necessary:
        if self.is_minimap_rotated() {
            let minimap_center_screen = self.minimap_draw_info.rect.center();
            camera_rect.min = camera_rect.min.rotate_around_point(minimap_center_screen, MINIMAP_ROTATION_ANGLE);
            camera_rect.max = camera_rect.max.rotate_around_point(minimap_center_screen, MINIMAP_ROTATION_ANGLE);
            camera_rect.canonicalize();
        }

        // Clamp camera rect to minimap aabb minus margin:
        camera_rect.max.x = camera_rect.max.x.min(self.minimap_draw_info.aabb.max.x - Self::CAMERA_CORNER_MARGIN_PX);
        camera_rect.max.y = camera_rect.max.y.min(self.minimap_draw_info.aabb.max.y - Self::CAMERA_CORNER_MARGIN_PX);
        camera_rect.min.x = camera_rect.min.x.max(self.minimap_draw_info.aabb.min.x + Self::CAMERA_CORNER_MARGIN_PX);
        camera_rect.min.y = camera_rect.min.y.max(self.minimap_draw_info.aabb.min.y + Self::CAMERA_CORNER_MARGIN_PX);
        camera_rect.canonicalize();

        camera_rect
    }

    #[inline]
    fn calc_camera_center_cell(camera: &Camera) -> CellF32 {
        let center_iso = camera.iso_world_position();
        coords::iso_to_cell_f32(center_iso, BASE_TILE_SIZE)
    }

    #[inline]
    fn calc_minimap_visible_cells(size_in_cells: Vec2, zoom: f32) -> Vec2 {
        size_in_cells / zoom
    }

    // `offsets` are in minimap cells/pixels (same units as minimap_size_in_cells).
    #[inline]
    fn calc_minimap_rect_uvs(size_in_cells: Vec2, offsets: Vec2, zoom: f32) -> (Vec2, Vec2) {
        let visible_cells = Self::calc_minimap_visible_cells(size_in_cells, zoom);
        let uv_min = offsets / size_in_cells;
        let uv_max = (offsets + visible_cells) / size_in_cells;
        (uv_min, uv_max)
    }

    #[inline]
    fn calc_minimap_offsets_from_center(size_in_cells: Vec2, center_cell: CellF32, zoom: f32) -> Vec2 {
        if zoom <= 1.0 {
            // Full minimap already visible.
            return Vec2::zero();
        }

        let visible_cells = Self::calc_minimap_visible_cells(size_in_cells, zoom);

        // Offset so center cell stays fixed regardless of zoom.
        let mut offsets = center_cell.0 - (visible_cells * 0.5);

        // Clamp to texture bounds.
        offsets.x = offsets.x.clamp(0.0, size_in_cells.x - visible_cells.x);
        offsets.y = offsets.y.clamp(0.0, size_in_cells.y - visible_cells.y);

        offsets
    }

    #[inline]
    fn calc_minimap_draw_rect_corners(minimap_rect: &Rect, is_rotated: bool) -> [Vec2; 4] {
        let mut corners = minimap_rect.corners_ccw();
        if is_rotated {
            let center = minimap_rect.center();
            for corner in &mut corners {
                *corner = corner.rotate_around_point(center, MINIMAP_ROTATION_ANGLE);
            }
        }
        corners
    }

    #[inline]
    fn calc_minimap_draw_info(widget_rect: &Rect, window_rect: &Rect, is_rotated: bool) -> MinimapDrawInfo {
        debug_assert!(widget_rect.is_valid() && window_rect.is_valid());
        let rect = Rect::new(widget_rect.position() + window_rect.position(), widget_rect.size_as_vec2());
        let corners = Self::calc_minimap_draw_rect_corners(&rect, is_rotated);
        let aabb = Rect::aabb(&corners);
        MinimapDrawInfo { rect, aabb, corners }
    }

    #[inline]
    fn calc_window_rect(widget_rect: &Rect, ui_sys: &UiSystem) -> Rect {
        debug_assert!(widget_rect.is_valid());
        let size = Vec2::new(widget_rect.width() + 70.0, widget_rect.height() + 90.0);
        let pos: Vec2  = Vec2::new(5.0, ui_sys.ui().io().display_size[1] - size.y - 5.0);
        Rect::new(pos, size)
    }

    #[inline]
    fn is_minimap_rotated(&self) -> bool {
        self.minimap_transform.rotated
    }

    // Convenience to return the current UV window used by drawing code.
    #[inline]
    fn current_minimap_uv_window(&self) -> (Vec2, Vec2) {
        Self::calc_minimap_rect_uvs(self.minimap_size_in_cells,
                                    self.minimap_transform.offsets,
                                    self.minimap_transform.zoom())
    }

    // ----------------------------------------------
    // Coordinate space conversion helpers
    // ----------------------------------------------

    // Maps fractional cell coords to full minimap UVs [0,1] and vice-versa.
    #[inline]
    fn cell_to_minimap_uv(&self, cell: CellF32) -> Vec2 {
        Vec2::new(
            cell.0.x / self.minimap_size_in_cells.x,
            // NOTE: Flip V for ImGui (because OpenGL textures have V=0 at bottom).
            1.0 - (cell.0.y / self.minimap_size_in_cells.y)
        )
    }

    #[inline]
    fn minimap_uv_to_cell(&self, uv: Vec2) -> CellF32 {
        CellF32(Vec2::new(
            uv.x * self.minimap_size_in_cells.x,
            // NOTE: Flip V for ImGui (because OpenGL textures have V=0 at bottom).
            (1.0 - uv.y) * self.minimap_size_in_cells.y,
        ))
    }

    // Maps minimap UVs in [0,1] range into minimap screen pixels and vice-versa.
    #[inline]
    fn minimap_uv_to_minimap_px(&self, uv: Vec2) -> Vec2 {
        self.minimap_draw_info.rect.position() + (uv * self.minimap_draw_info.rect.size_as_vec2())
    }

    #[inline]
    fn minimap_px_to_minimap_uv(&self, minimap_px: Vec2) -> Vec2 {
        (minimap_px - self.minimap_draw_info.rect.position()) / self.minimap_draw_info.rect.size_as_vec2()
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
    // `widget_px` is in absolute screen coordinates. If the minimap is rotated,
    // call this only after un-rotating the point into the widget AABB (see cursor cell picking).
    #[inline]
    fn scaled_minimap_widget_px_to_cell(&self, widget_px: Vec2) -> CellF32 {
        let (uv_min, uv_max) = self.current_minimap_uv_window();

        let widget_origin = self.minimap_draw_info.rect.position();
        let widget_size   = self.minimap_draw_info.rect.size_as_vec2();

        let window_uv  = (widget_px - widget_origin) / widget_size; // Normalized [0,1] within visible window.
        let fullmap_uv = uv_min + window_uv * (uv_max - uv_min);    // Convert to full-map UV.

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
