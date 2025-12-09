use rand::Rng;
use std::path::PathBuf;
use smallvec::SmallVec;
use strum::{EnumCount, IntoEnumIterator, EnumProperty};
use strum_macros::{EnumCount, EnumIter, EnumProperty};

use super::{
    TileKind, TileMap, TileMapLayerKind, sets::TileDef,
    camera::Camera, water, road, BASE_TILE_SIZE,
};

use crate::{
    singleton,
    engine::time::Seconds,
    save::{PreLoadContext, PostLoadContext},
    imgui_ui::{self, UiSystem, UiTextureHandle},
    app::input::{InputSystem, InputAction, MouseButton},
    utils::{Color, Rect, RectCorners, Size, Vec2, coords::{self, Cell, CellF32, IsoPointF32}, platform::paths},
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
    pub fn draw(&mut self, render_sys: &impl RenderSystem, camera: &Camera, ui_sys: &UiSystem) {
        self.widget.draw(render_sys, camera, ui_sys, self.texture.handle, &self.icons);
    }

    #[inline]
    pub fn draw_debug_ui(&mut self, camera: &mut Camera, ui_sys: &UiSystem, enable_debug_controls: bool) {
        self.widget.draw_debug_ui(camera, ui_sys, enable_debug_controls);
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
            camera: &Camera,
            ui_sys: &UiSystem,
            tex_handle: TextureHandle,
            icons: &[MinimapIconInstance]);

    fn draw_debug_ui(&mut self,
                     camera: &mut Camera,
                     ui_sys: &UiSystem,
                     enable_debug_controls: bool);
}

// Rotate the minimap -45 degrees to match our isometric world projection.
const MINIMAP_ROTATION_ANGLE: f32 = -45.0 * (std::f32::consts::PI / 180.0);

struct MinimapTransform {
    offsets: Vec2, // Minimap texture offset/panning in cells (pixels), from minimap origin (0,0).
    scale: f32,    // Zoom amount: 1=draw full minimap, >1 zooms-in, <1 zooms-out. Must not be zero.
    rotated: bool, // Apply 45 degrees isometric rotation when drawing the minimap?
}

impl MinimapTransform {
    const ZOOM_STEP: f32 = 0.1;

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
        Self { offsets: Vec2::default(), scale: 1.0, rotated: true }
    }
}

#[derive(Default)]
struct MinimapDrawInfo {
    rect: Rect,
    aabb: Rect,         // Axis-aligned bounding box of `rect` if rotated, same as `rect` if not.
    corners: [Vec2; 4], // CCW corners of `rect`.
}

struct MinimapWidgetImGui {
    is_open: bool,
    cursor_pos: Vec2,                    // Cursor screen space position, cached on update().
    widget_rect: Rect,                   // Minimap widget screen space rect, relative to window rect.
    window_rect: Rect,                   // Widget window screen rect in absolute screen space.

    minimap_transform: MinimapTransform, // Zoom (scale) & UV offsets (translation).
    minimap_draw_info: MinimapDrawInfo,  // Absolute screen rect where we render the minimap texture.
    minimap_size_in_cells: Vec2,         // Minimap/TileMap size in cells (1 TileMap cell = 1 minimap pixel).
    minimap_auto_zoom: bool,             // Automatically adjust zoom to best match desired number of visible tiles/cells.
    minimap_auto_scroll: bool,           // Scroll minimap when camera rect touches the minimap edges?
    scroll_speed_px_per_sec: f32,        // Scroll speed in pixels per second when `minimap_auto_scroll=true`.
    desired_visible_cells: Size,         // Desired number of visible cells we want to display for when `minimap_auto_zoom=true`.

    camera_screen_rect: Rect,            // Camera rect in absolute screen space, ready for overlay rendering.
    camera_corners_near_minimap_edge: RectCorners,

    // Debug switches:
    enable_debug_draw: bool,
    enable_debug_controls: bool,
    show_debug_controls: bool,
    show_origin_markers: bool,
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
            minimap_transform: MinimapTransform::default(),
            minimap_draw_info: MinimapDrawInfo::default(),
            minimap_size_in_cells: Vec2::default(),
            minimap_auto_zoom: true,
            minimap_auto_scroll: true,
            scroll_speed_px_per_sec: 30.0,
            desired_visible_cells: Size::new(85, 85),
            camera_screen_rect: Rect::default(),
            camera_corners_near_minimap_edge: RectCorners::default(),
            enable_debug_draw: false,
            enable_debug_controls: false,
            show_debug_controls: false,
            show_origin_markers: false,
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
        if !size_in_cells.is_valid() {
            return;
        }

        debug_assert!(self.widget_rect.is_valid());
        debug_assert!(self.minimap_transform.is_valid());

        // Must update these every frame:
        self.cursor_pos = input_sys.cursor_pos();
        self.minimap_size_in_cells = size_in_cells.to_vec2();
        self.window_rect = self.calc_window_rect(ui_sys);
        self.camera_screen_rect = self.calc_camera_screen_rect(camera);
        self.minimap_draw_info = self.calc_minimap_draw_info();
        self.camera_corners_near_minimap_edge = self.rect_corners_near_minimap_edge(&self.camera_screen_rect);

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

    fn draw(&mut self,
            render_sys: &impl RenderSystem,
            camera: &Camera,
            ui_sys: &UiSystem,
            tex_handle: TextureHandle,
            icons: &[MinimapIconInstance]) {
        if self.is_open {
            self.draw_widget_window(render_sys, camera, ui_sys, tex_handle, icons);
        } else {
            self.draw_open_button(ui_sys);
        }
    }

    fn draw_debug_ui(&mut self,
                     camera: &mut Camera,
                     ui_sys: &UiSystem,
                     enable_debug_controls: bool) {
        self.enable_debug_controls = enable_debug_controls;

        if !self.is_open || !self.enable_debug_controls || !self.show_debug_controls {
            return;
        }

        self.draw_debug_controls(camera, ui_sys);
    }
}

impl MinimapWidgetImGui {
    fn draw_widget_window(&mut self,
                          render_sys: &impl RenderSystem,
                          camera: &Camera,
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

                self.draw_minimap(camera, ui_sys, ui_texture);
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
            parent_window_pos[1] - 95.0,
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
                    self.minimap_transform.reset();
                    camera.center();
                }

                ui.same_line();
                ui.checkbox("Debug Origin", &mut self.show_origin_markers);
                ui.same_line();
                ui.checkbox("Debug Draw", &mut self.enable_debug_draw);

                // newline
                ui.checkbox("Rotated", &mut self.minimap_transform.rotated);
                ui.same_line();
                ui.checkbox("Auto Scroll", &mut self.minimap_auto_scroll);
                ui.same_line();
                ui.checkbox("Auto Zoom", &mut self.minimap_auto_zoom);

                ui.input_float("Scroll Speed", &mut self.scroll_speed_px_per_sec)
                    .display_format("%.2f")
                    .step(1.0)
                    .build();

                ui.input_float("Scroll X", &mut self.minimap_transform.offsets.x)
                    .display_format("%.2f")
                    .step(1.0)
                    .build();

                ui.input_float("Scroll Y", &mut self.minimap_transform.offsets.y)
                    .display_format("%.2f")
                    .step(1.0)
                    .build();

                if ui.input_float("Zoom", &mut self.minimap_transform.scale)
                    .display_format("%.2f")
                    .step(MinimapTransform::ZOOM_STEP)
                    .build()
                {
                    // Clamp to 10x max zoom.
                    self.minimap_transform.scale = self.minimap_transform.scale.clamp(1.0, 10.0);
                }

                ui.separator();

                let (uv_min, uv_max) = self.current_minimap_uv_window();

                let camera_center_iso = camera.iso_world_position();
                let camera_center_cell = coords::iso_to_cell_f32(camera_center_iso, BASE_TILE_SIZE);

                let minimap_visible_cells = Self::calc_minimap_visible_cells(self.minimap_size_in_cells,
                                                                             self.minimap_transform.zoom());

                ui.text(format!("UV Window          : {}", uv_max - uv_min));
                ui.text(format!("UV Window Min/Max  : {} / {}", uv_min, uv_max));
                ui.text(format!("Visible Cells      : {}", minimap_visible_cells));
                ui.text(format!("Camera Center Iso  : {}", camera_center_iso.0));
                ui.text(format!("Camera Center Cell : {}", camera_center_cell.0));
                ui.text(format!("Camera Screen Rect : {}", self.camera_screen_rect));

                if self.camera_corners_near_minimap_edge.is_empty() {
                    ui.text("Camera Corners Near Edge : None");
                } else {
                    ui.text("Camera Corners Near Edge :");
                    ui.same_line();
                    ui.text_colored(Color::red().to_array(), self.camera_corners_near_minimap_edge.to_string());
                }
            });

        self.show_debug_controls = show_debug_controls;
    }

    fn draw_minimap(&self, camera: &Camera, ui_sys: &UiSystem, ui_texture: UiTextureHandle) {
        debug_assert!(self.minimap_size_in_cells != Vec2::zero());
        debug_assert!(self.minimap_draw_info.rect.is_valid());
        debug_assert!(self.minimap_draw_info.aabb.is_valid());

        let draw_list = ui_sys.ui().get_window_draw_list();
        self.draw_texture_rect(&draw_list, ui_texture);
        self.draw_outline_rect(&draw_list);
        self.draw_camera_rect(&draw_list, camera);

        // Show minimap widget screen origin (local [0,0] coord) and UV/Map origins:
        if self.enable_debug_draw && self.show_origin_markers {
            self.draw_origin_debug_markers(&draw_list);
        }
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
        let minimap_aabb = self.minimap_draw_info.aabb.shrunk(Self::MINIMAP_AABB_MARGIN);

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

            // Clip icon if outside of the minimap aabb.
            if !minimap_aabb.contains_rect(&icon_rect) {
                continue;
            }

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
        let minimap_corners  = &self.minimap_draw_info.corners;
        let (uv_min, uv_max) = self.current_minimap_uv_window();

        // NOTE: Flip V for ImGui.
        let uv1 = [uv_min.x, 1.0 - uv_min.y];
        let uv2 = [uv_max.x, 1.0 - uv_min.y];
        let uv3 = [uv_max.x, 1.0 - uv_max.y];
        let uv4 = [uv_min.x, 1.0 - uv_max.y];

        draw_list
            .add_image_quad(
                ui_texture,
                minimap_corners[0].to_array(),
                minimap_corners[1].to_array(),
                minimap_corners[2].to_array(),
                minimap_corners[3].to_array())
            .uv(uv1, uv2, uv3, uv4)
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

    fn draw_camera_rect(&self, draw_list: &imgui::DrawListMut<'_>, camera: &Camera) {
        let outline_color = {
            if self.enable_debug_draw && !self.camera_corners_near_minimap_edge.is_empty() {
                // Color it red if any corner of the camera rect is falling outside the minimap.
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
                    // We want to visualize the derived camera center cell:
                    let camera_center_iso = camera.iso_world_position();
                    let camera_center_cell = coords::iso_to_cell_f32(camera_center_iso, BASE_TILE_SIZE);
                    let camera_center_screen = self.cell_to_scaled_minimap_widget_px(camera_center_cell);
                    let minimap_center_screen = self.minimap_draw_info.rect.center();
                    camera_center_screen.rotate_around_point(minimap_center_screen, MINIMAP_ROTATION_ANGLE)
                } else {
                    self.camera_screen_rect.center()
                }
            };

            // Clip to AABB:
            const POINT_RADIUS: f32 = 4.0;
            let point_rect = Rect::from_extents(
                Vec2::new(camera_center_point.x - POINT_RADIUS, camera_center_point.y - POINT_RADIUS),
                Vec2::new(camera_center_point.x + POINT_RADIUS, camera_center_point.y + POINT_RADIUS)
            );

            if self.minimap_draw_info.aabb.contains_rect(&point_rect) {
                draw_list.add_circle(camera_center_point.to_array(),
                                     POINT_RADIUS,
                                     imgui::ImColor32::from_rgb(0, 255, 0))
                                     .build();
            }

            // Camera rect min (blue) / max (yellow):
            draw_list.add_circle(self.camera_screen_rect.min.to_array(),
                                 2.0,
                                 imgui::ImColor32::from_rgb(0, 0, 255))
                                 .filled(true)
                                 .build();

            draw_list.add_circle(self.camera_screen_rect.max.to_array(),
                                 2.0,
                                 imgui::ImColor32::from_rgb(255, 255, 0))
                                 .filled(true)
                                 .build();
        }
    }

    fn draw_origin_debug_markers(&self, draw_list: &imgui::DrawListMut<'_>) {
        // Widget screen space: Top-left origin (CYAN circle).
        let widget_origin = self.minimap_draw_info.rect.position();
        draw_list.add_circle(widget_origin.to_array(),
                             10.0,
                             imgui::ImColor32::from_rgb(0, 255, 255))
                             .build();

        // Minimap texture rect UVs: Top-left origin (RED circle).
        let uv_origin = self.minimap_uv_to_minimap_px(Vec2::new(0.0, 0.0));
        draw_list.add_circle(uv_origin.to_array(),
                             8.0,
                             imgui::ImColor32::from_rgb(255, 0, 0))
                             .build();

        // Full-map texture rect UVs: Top-left origin (GREEN circle).
        let fullmap_uv_origin = self.minimap_uv_to_minimap_px(
            self.fullmap_uv_to_window_uv(Vec2::new(0.0, 0.0))
        );
        draw_list.add_circle(fullmap_uv_origin.to_array(),
                             6.0,
                             imgui::ImColor32::from_rgb(0, 255, 0))
                             .build();

        // Tile map cells: Bottom-left origin (YELLOW circle).
        let cell_origin = self.cell_to_scaled_minimap_widget_px(CellF32(Vec2::new(0.0, 0.0)));
        draw_list.add_circle(cell_origin.to_array(),
                             4.0,
                             imgui::ImColor32::from_rgb(255, 255, 0))
                             .build();

        // Tile map iso coords: Bottom-left origin (BLUE circle).
        let iso_origin = self.cell_to_scaled_minimap_widget_px(
            coords::iso_to_cell_f32(IsoPointF32(Vec2::new(0.0, 0.0)), BASE_TILE_SIZE)
        );
        draw_list.add_circle(iso_origin.to_array(),
                             2.0,
                             imgui::ImColor32::from_rgb(0, 0, 255))
                             .filled(true)
                             .build();
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
        if cell.0.x < 0.0 || cell.0.x >= self.minimap_size_in_cells.x ||
           cell.0.y < 0.0 || cell.0.y >= self.minimap_size_in_cells.y
        {
            return None;
        }

        Some(coords::cell_to_iso_f32(cell, BASE_TILE_SIZE))
    }

    fn update_minimap_zoom(&mut self) {
        if !self.minimap_auto_zoom {
            return;
        }

        loop {
            let visible_cells = Self::calc_minimap_visible_cells(self.minimap_size_in_cells,
                                                                 self.minimap_transform.zoom());

            if visible_cells.x as i32 <= self.desired_visible_cells.width ||
               visible_cells.y as i32 <= self.desired_visible_cells.height
            {
                break;
            }

            self.minimap_transform.scale += MinimapTransform::ZOOM_STEP;
        }
    }

    fn update_minimap_scrolling(&mut self, delta_time_secs: Seconds) {
        if !self.minimap_auto_scroll ||
            self.minimap_transform.zoom() <= 1.0 ||
            self.camera_corners_near_minimap_edge.is_empty()
        {
            return;
        }

        let (uv_min, uv_max) = self.current_minimap_uv_window();
        let mut scrollable_corners = RectCorners::all();

        // Corners already at their limits will not scroll further.
        if uv_min.x <= 0.0 {
            scrollable_corners.remove(RectCorners::BottomLeft);
        }
        if uv_max.y >= 1.0 {
            scrollable_corners.remove(RectCorners::BottomRight);
        }
        if uv_max.x >= 1.0 {
            scrollable_corners.remove(RectCorners::TopRight);
        }
        if uv_min.y <= 0.0 {
            scrollable_corners.remove(RectCorners::TopLeft);
        }
        if scrollable_corners.is_empty() {
            return;
        }

        // Minimap scrolling:
        if self.camera_corners_near_minimap_edge.intersects(RectCorners::TopLeft)
            && scrollable_corners.intersects(RectCorners::TopLeft)
        {
            self.minimap_transform.offsets.y -= self.scroll_speed_px_per_sec * delta_time_secs;
        }
        if self.camera_corners_near_minimap_edge.intersects(RectCorners::BottomRight)
            && scrollable_corners.intersects(RectCorners::BottomRight)
        {
            self.minimap_transform.offsets.y += self.scroll_speed_px_per_sec * delta_time_secs;
        }
        if self.camera_corners_near_minimap_edge.intersects(RectCorners::TopRight)
            && scrollable_corners.intersects(RectCorners::TopRight)
        {
            self.minimap_transform.offsets.x += self.scroll_speed_px_per_sec * delta_time_secs;
        }
        if self.camera_corners_near_minimap_edge.intersects(RectCorners::BottomLeft)
            && scrollable_corners.intersects(RectCorners::BottomLeft)
        {
            self.minimap_transform.offsets.x -= self.scroll_speed_px_per_sec * delta_time_secs;
        }
    }

    const MINIMAP_AABB_MARGIN: Vec2 = Vec2::new(4.0, 4.0); // Margin in pixels.

    fn rect_corners_near_minimap_edge(&self, screen_rect: &Rect) -> RectCorners {
        let mut corners_outside = RectCorners::empty();
        let rect = screen_rect.expanded(Self::MINIMAP_AABB_MARGIN);

        if self.is_minimap_rotated() {
            let minimap_corners = &self.minimap_draw_info.corners;

            // NOTE: RectCorners top/bottom are inverted here due to the minimap rotation.
            if !coords::is_screen_point_inside_diamond(rect.top_left(), minimap_corners) {
                corners_outside |= RectCorners::BottomLeft;
            }
            if !coords::is_screen_point_inside_diamond(rect.top_right(), minimap_corners) {
                corners_outside |= RectCorners::BottomRight;
            }
            if !coords::is_screen_point_inside_diamond(rect.bottom_left(), minimap_corners) {
                corners_outside |= RectCorners::TopLeft;
            }
            if !coords::is_screen_point_inside_diamond(rect.bottom_right(), minimap_corners) {
                corners_outside |= RectCorners::TopRight;
            }
        } else {
            let minimap_aabb = &self.minimap_draw_info.aabb;

            if rect.max.x >= minimap_aabb.max.x {
                corners_outside |= RectCorners::TopRight;
            }
            if rect.max.y >= minimap_aabb.max.y {
                corners_outside |= RectCorners::BottomRight;
            }
            if rect.min.x <= minimap_aabb.min.x {
                corners_outside |= RectCorners::BottomLeft;
            }
            if rect.min.y <= minimap_aabb.min.y {
                corners_outside |= RectCorners::TopLeft;
            }
        }

        corners_outside
    }

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

    // Rect in screen space, ready to be drawn with ImGui.
    fn calc_camera_screen_rect(&self, camera: &Camera) -> Rect {
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
        *camera_rect.clamp(&self.minimap_draw_info.aabb.shrunk(Self::MINIMAP_AABB_MARGIN))
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
    fn calc_zoom_offsets_from_center(size_in_cells: Vec2, center_cell: CellF32, zoom: f32) -> Vec2 {
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
    fn calc_minimap_draw_rect_corners(&self, minimap_rect: &Rect) -> [Vec2; 4] {
        let mut corners = minimap_rect.corners_ccw();
        if self.is_minimap_rotated() {
            let center = minimap_rect.center();
            for corner in &mut corners {
                *corner = corner.rotate_around_point(center, MINIMAP_ROTATION_ANGLE);
            }
        }
        corners
    }

    #[inline]
    fn calc_minimap_draw_info(&self) -> MinimapDrawInfo {
        debug_assert!(self.widget_rect.is_valid() && self.window_rect.is_valid());
        let rect = Rect::new(self.widget_rect.position() + self.window_rect.position(), self.widget_rect.size_as_vec2());
        let corners = self.calc_minimap_draw_rect_corners(&rect);
        let aabb = Rect::aabb(&corners);
        MinimapDrawInfo { rect, aabb, corners }
    }

    #[inline]
    fn calc_window_rect(&self, ui_sys: &UiSystem) -> Rect {
        debug_assert!(self.widget_rect.is_valid());
        let size = Vec2::new(self.widget_rect.width() + 70.0, self.widget_rect.height() + 90.0);
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
        let map_center_cell = CellF32(self.minimap_size_in_cells * 0.5);
        let zoom = self.minimap_transform.zoom();
        let zoom_offsets = Self::calc_zoom_offsets_from_center(self.minimap_size_in_cells, map_center_cell, zoom);
        let combined_offsets = zoom_offsets + self.minimap_transform.offsets;
        Self::calc_minimap_rect_uvs(self.minimap_size_in_cells, combined_offsets, zoom)
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
            (1.0 - uv.y) * self.minimap_size_in_cells.y
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
