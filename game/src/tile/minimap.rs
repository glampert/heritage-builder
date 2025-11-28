use rand::Rng;

use super::{
    TileKind, TileMap, TileMapLayerKind, sets::TileDef,
    camera::Camera, water, road, BASE_TILE_SIZE,
};

use crate::{
    save::{PreLoadContext, PostLoadContext},
    utils::{Size, Rect, Vec2, coords::{self, Cell, IsoPoint}},
    imgui_ui::{self, UiSystem, UiTextureHandle, UiStaticVar},
    render::{RenderSystem, TextureCache, TextureHandle, TextureSettings, TextureFilter},
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
    const LIGHT_BROWN:   Self = Self { r: 143, g: 90,  b: 53,  a: 255 };
    const DARK_BROWN:    Self = Self { r: 75,  g: 35,  b: 10,  a: 255 };
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
            road::RoadKind::Paved => Self::LIGHT_GRAY,
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

    fn reset(&mut self, size: Size, fill_color: MinimapTileColor) {
        self.need_update = true;

        if size == self.size {
            self.pixels.fill(fill_color);
            return; // No change in size.
        }

        self.pixels.clear();

        let pixel_count = (size.width * size.height) as usize;
        self.pixels.resize(pixel_count, fill_color);

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
                gen_mipmaps: false,
                ..Default::default()
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
        self.reset(tile_map.size_in_cells(), MinimapTileColor::default());

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
// Minimap
// ----------------------------------------------

#[derive(Default)]
pub struct Minimap {
    texture: MinimapTexture,
}

impl Minimap {
    pub fn new(size_in_cells: Size) -> Self {
        Self {
            // One pixel per tile map cell.
            texture: MinimapTexture::new(size_in_cells),
        }
    }

    #[inline]
    pub fn update(&mut self, tex_cache: &mut dyn TextureCache) {
        self.texture.update(tex_cache);
    }

    #[inline]
    pub fn pre_load(&mut self, context: &PreLoadContext) {
        self.texture.pre_load(context.tex_cache_mut());
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

    pub fn reset(&mut self, fill_with_def: Option<&'static TileDef>, new_map_size: Option<Size>) {
        let fill_color = {
            if let Some(tile_def) = fill_with_def {
                MinimapTileColor::from_tile_def(tile_def).unwrap_or_default()
            } else {
                MinimapTileColor::default()
            }
        };

        let size = new_map_size.unwrap_or(self.texture.size);
        self.texture.reset(size, fill_color);
    }

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

    const ENABLE_DEBUG_CONTROLS: bool = false;

    // Draw the minimap using ImGui, nestled inside a window.
    pub fn draw(&self,
                camera: &mut Camera,
                render_sys: &mut impl RenderSystem,
                ui_sys: &UiSystem,
                cursor_screen_pos: Vec2) {
        let ui = ui_sys.builder();

        static SHOW_MINIMAP:    UiStaticVar<bool> = UiStaticVar::new(true);
        static ROTATED_MINIMAP: UiStaticVar<bool> = UiStaticVar::new(true);
        static DEBUG_MINIMAP:   UiStaticVar<bool> = UiStaticVar::new(Minimap::ENABLE_DEBUG_CONTROLS);

        if *SHOW_MINIMAP {
            let minimap = MinimapWidget {
                screen_pos:  Vec2::new(35.0,  55.0),
                screen_size: Vec2::new(140.0, 140.0),
                map_size:    self.texture.size, // Same as tile map size in cells.
                rotated:     *ROTATED_MINIMAP,
                debug:       *DEBUG_MINIMAP,
            };

            let window_size = [210.0, 230.0];
            let window_position = [5.0, ui.io().display_size[1] - window_size[1] - 5.0];

            let window_flags =
                imgui::WindowFlags::NO_RESIZE
                | imgui::WindowFlags::NO_SCROLLBAR
                | imgui::WindowFlags::NO_MOVE
                | imgui::WindowFlags::NO_COLLAPSE;

            ui.window("Minimap")
                .opened(SHOW_MINIMAP.as_mut())
                .flags(window_flags)
                .position(window_position, imgui::Condition::Always)
                .size(window_size, imgui::Condition::Always)
                .build(|| {
                    let tex_cache = render_sys.texture_cache();
                    let texture = ui_sys.to_ui_texture(tex_cache, self.texture.handle);

                    let hovered_tile_iso =
                        draw_minimap_widget(&minimap, texture, camera, cursor_screen_pos, ui);

                    if let Some(destination_iso) = hovered_tile_iso {
                        if ui.is_mouse_down(imgui::MouseButton::Left) {
                            camera.teleport_iso(destination_iso);
                        }
                    }

                    if Minimap::ENABLE_DEBUG_CONTROLS {
                        // Debug controls at the widget's bottom:
                        ui.dummy([0.0, window_size[1] - 60.0]);
                        ui.checkbox("Rotated", ROTATED_MINIMAP.as_mut());
                        ui.same_line();
                        ui.dummy([50.0, 0.0]);
                        ui.same_line();
                        ui.checkbox("Debug", DEBUG_MINIMAP.as_mut());
                    }
                });
        } else {
            // Minimap open/close button:
            let window_position = [5.0, ui.io().display_size[1] - 35.0];

            let window_flags =
                imgui::WindowFlags::NO_DECORATION
                | imgui::WindowFlags::NO_BACKGROUND
                | imgui::WindowFlags::NO_RESIZE
                | imgui::WindowFlags::NO_SCROLLBAR
                | imgui::WindowFlags::NO_MOVE
                | imgui::WindowFlags::NO_COLLAPSE;

            ui.window("Minimap Button")
                .flags(window_flags)
                .position(window_position, imgui::Condition::Always)
                .always_auto_resize(true)
                .bg_alpha(0.0)
                .build(|| {
                    if imgui_ui::icon_button(ui_sys, imgui_ui::icons::ICON_MAP, Some("Open Minimap")) {
                        SHOW_MINIMAP.set(true);
                    }
                });
        }
    }
}

// ----------------------------------------------
// Coordinate space conversion helpers
// ----------------------------------------------

struct MinimapWidget {
    screen_pos: Vec2,  // Top-left corner in screen space.
    screen_size: Vec2, // Display size in pixels.
    map_size: Size,    // Tile map size in cells.
    rotated: bool,     // Apply the 45 degrees isometric rotation when drawing the minimap?
    debug: bool,       // Enable debug drawing.
}

// Rotate the minimap -45 degrees to match our isometric world projection.
const MINIMAP_ROTATION_ANGLE: f32 = -45.0 * (std::f32::consts::PI / 180.0);

// Map fractional cell coords to minimap UV [0,1].
fn cell_to_minimap_uv(cell: Cell, map_size: Size) -> Vec2 {
    Vec2::new(
        cell.x as f32 / map_size.width as f32,
        // Flip V for ImGui (because OpenGL textures have V=0 at bottom).
        1.0 - (cell.y as f32 / map_size.height as f32)
    )
}

fn minimap_uv_to_cell(uv: Vec2, map_size: Size) -> Cell {
    Cell::new(
        (uv.x * map_size.width as f32).floor() as i32,
        // Flip V for ImGui (because OpenGL textures have V=0 at bottom).
        ((1.0 - uv.y) * map_size.height as f32).floor() as i32,
    )
}

fn minimap_uv_to_minimap_px(minimap: &MinimapWidget, origin: Vec2, uv: Vec2) -> Vec2 {
    let minimap_abs_pos = origin + minimap.screen_pos;
    minimap_abs_pos + (uv * minimap.screen_size)
}

fn minimap_px_to_minimap_uv(minimap: &MinimapWidget, origin: Vec2, minimap_px: Vec2) -> Vec2 {
    let minimap_abs_pos = origin + minimap.screen_pos;
    (minimap_px - minimap_abs_pos) / minimap.screen_size
}

// ----------------------------------------------
// TileMap cell -> minimap screen pixel
// ----------------------------------------------

fn cell_to_minimap_px(minimap: &MinimapWidget, origin: Vec2, cell: Cell) -> Vec2 {
    let uv = cell_to_minimap_uv(cell, minimap.map_size);
    minimap_uv_to_minimap_px(minimap, origin, uv)
}

fn minimap_px_to_cell(minimap: &MinimapWidget, origin: Vec2, minimap_px: Vec2) -> Option<Cell> {
    let uv = minimap_px_to_minimap_uv(minimap, origin, minimap_px);

    if uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 {
        return None; // outside minimap.
    }

    Some(minimap_uv_to_cell(uv, minimap.map_size))
}

// ----------------------------------------------
// Iso coord -> minimap screen pixel
// ----------------------------------------------

fn iso_to_minimap_px(minimap: &MinimapWidget, origin: Vec2, iso_point: Vec2) -> Vec2 {
    let cell_frac = coords::iso_to_cell_f32(iso_point, BASE_TILE_SIZE);
    let cell = Cell::new(cell_frac.x.floor() as i32, cell_frac.y.floor() as i32);
    cell_to_minimap_px(minimap, origin, cell)
}

fn minimap_px_to_iso(minimap: &MinimapWidget, origin: Vec2, minimap_px: Vec2) -> Option<IsoPoint> {
    minimap_px_to_cell(minimap, origin, minimap_px).map(|cell| coords::cell_to_iso(cell, BASE_TILE_SIZE))
}

// ----------------------------------------------
// Viewport pixel -> minimap screen pixel
// ----------------------------------------------

fn viewport_px_to_minimap_px(minimap: &MinimapWidget, origin: Vec2, vp_size: Vec2, vp_px: Vec2) -> Vec2 {
    let uv = vp_px / vp_size;               // normalized point in full screen
    minimap_uv_to_minimap_px(minimap, origin, uv) // map into minimap rect
}

fn minimap_px_to_viewport_px(minimap: &MinimapWidget, origin: Vec2, vp_size: Vec2, minimap_px: Vec2) -> Vec2 {
    let uv = minimap_px_to_minimap_uv(minimap, origin, minimap_px);
    uv * vp_size
}

// ----------------------------------------------
// Camera rect minimap overlay
// ----------------------------------------------

fn minimap_camera_rect(minimap: &MinimapWidget, origin: Vec2, camera: &Camera) -> Rect {
    let transform = camera.transform();
    let viewport_size = camera.viewport_size().to_vec2();
    let viewport_center = viewport_size * 0.5; // Camera center in world/iso coords.

    let half_iso = viewport_center / transform.scaling;
    let center_iso = (viewport_center - transform.offset) / transform.scaling;

    let (uv_min, uv_max) = {
        if minimap.rotated {
            // Convert iso rect corners to fractional cell coordinates (continuous):
            let cell_min_frac = coords::iso_to_cell_f32(center_iso - half_iso, BASE_TILE_SIZE);
            let cell_max_frac = coords::iso_to_cell_f32(center_iso + half_iso, BASE_TILE_SIZE);

            // Important: ensure correct ordering (min <= max) after transform.
            let cell_x_min = cell_min_frac.x.min(cell_max_frac.x);
            let cell_x_max = cell_min_frac.x.max(cell_max_frac.x);
            let cell_y_min = cell_min_frac.y.min(cell_max_frac.y);
            let cell_y_max = cell_min_frac.y.max(cell_max_frac.y);

            let map_width  = minimap.map_size.width  as f32;
            let map_height = minimap.map_size.height as f32;

            // Flip V for ImGui (because OpenGL textures have V=0 at bottom).
            let uv_min = Vec2::new(cell_x_min / map_width, 1.0 - (cell_y_min / map_height));
            let uv_max = Vec2::new(cell_x_max / map_width, 1.0 - (cell_y_max / map_height));

            (uv_min, uv_max)
        } else {
            // Convert iso -> UV directly using world iso bounds:
            fn point_to_uv(rect: &Rect, p: Vec2) -> Vec2 {
                Vec2::new(
                    (p.x - rect.min.x) / rect.width(),
                    (p.y - rect.min.y) / rect.height()
                )
            }

            let iso_min = center_iso - half_iso;
            let iso_max = center_iso + half_iso;

            let bounds = calc_map_bounds_iso(minimap.map_size);
            let uv_min = point_to_uv(&bounds, iso_min);
            let uv_max = point_to_uv(&bounds, iso_max);

            (uv_min, uv_max)
        }
    };

    let rect_min = minimap_uv_to_minimap_px(minimap, origin, uv_min);
    let rect_max = minimap_uv_to_minimap_px(minimap, origin, uv_max);

    Rect { min: rect_min, max: rect_max } // top-left & bottom-right corners.
}

fn calc_map_bounds_iso(map_size_in_cells: Size) -> Rect {
    let map_width  = map_size_in_cells.width  as f32;
    let map_height = map_size_in_cells.height as f32;

    let points = [
        coords::cell_to_iso_f32(Vec2::new(0.0, 0.0), BASE_TILE_SIZE),
        coords::cell_to_iso_f32(Vec2::new(0.0, map_height), BASE_TILE_SIZE),
        coords::cell_to_iso_f32(Vec2::new(map_width, 0.0), BASE_TILE_SIZE),
        coords::cell_to_iso_f32(Vec2::new(map_width, map_height), BASE_TILE_SIZE),
    ];

    Rect::aabb(&points)
}

// ----------------------------------------------
// Cursor -> minimap cell picking
// ----------------------------------------------

fn unrotate_cursor_pos(minimap: &MinimapWidget, origin: Vec2, cursor_screen_pos: Vec2) -> Vec2 {
    if minimap.rotated {
        let minimap_abs_pos = origin + minimap.screen_pos;
        let center = minimap_abs_pos + minimap.screen_size * 0.5;
        cursor_screen_pos.rotate_around_point(center, -MINIMAP_ROTATION_ANGLE)
    } else {
        cursor_screen_pos
    }
}

// Returns floating-point isometric coords without rounding to cell space.
fn pick_minimap_iso(minimap: &MinimapWidget, origin: Vec2, cursor_screen_pos: Vec2) -> Option<Vec2> {
    let minimap_px = unrotate_cursor_pos(minimap, origin, cursor_screen_pos);

    // Convert to local minimap uv coordinates, [0..1] range:
    let uv = minimap_px_to_minimap_uv(minimap, origin, minimap_px);
    if uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 {
        return None; // outside minimap.
    }

    // Rotated: sample continuous cell coordinates.
    if minimap.rotated {
        // Compute corresponding *continuous* cell coordinates.
        // (0..map_width, 0..map_height), no rounding.
        let cell_frac = Vec2::new(
            uv.x * minimap.map_size.width as f32,
            // Flip V for ImGui (because OpenGL textures have V=0 at bottom).
            (1.0 - uv.y) * minimap.map_size.height as f32
        );

        Some(coords::cell_to_iso_f32(cell_frac, BASE_TILE_SIZE))
    } else {
        // Unrotated: sample continuous iso coords directly.
        let bounds = calc_map_bounds_iso(minimap.map_size);

        let iso_x = bounds.min.x + (uv.x * bounds.width());
        let iso_y = bounds.min.y + (uv.y * bounds.height());

        Some(Vec2::new(iso_x, iso_y))
    }
}

// ----------------------------------------------
// ImGui draw helpers
// ----------------------------------------------

fn draw_minimap_widget(minimap: &MinimapWidget,
                       texture: UiTextureHandle,
                       camera: &Camera,
                       cursor_screen_pos: Vec2,
                       ui: &imgui::Ui) -> Option<Vec2> {
    let draw_list = ui.get_window_draw_list();
    let origin = Vec2::from_array(ui.window_pos());

    let rect = Rect::new(origin + minimap.screen_pos, minimap.screen_size);
    let center = rect.center();

    let corners = corners(&rect, center, minimap.rotated);
    let aabb = Rect::aabb(&corners);

    draw_texture_rect(&draw_list, texture, &corners);
    draw_outline_rect(&draw_list, minimap, &corners, &aabb, cursor_screen_pos);
    draw_camera_rect(&draw_list, minimap, camera, &aabb, origin, center);

    let cursor_inside_minimap = coords::is_screen_point_inside_diamond(cursor_screen_pos, &corners);
    if cursor_inside_minimap {
        return pick_minimap_iso(minimap, origin, cursor_screen_pos);
    }

    None
}

fn corners(rect: &Rect, center: Vec2, rotated: bool) -> [Vec2; 4] {
    let mut corners = rect.corners_ccw();
    if rotated {
        for corner in &mut corners {
            *corner = corner.rotate_around_point(center, MINIMAP_ROTATION_ANGLE);
        }
    }
    corners
}

fn draw_texture_rect(draw_list: &imgui::DrawListMut<'_>,
                     texture: UiTextureHandle,
                     corners: &[Vec2; 4]) {
    draw_list
        .add_image_quad(texture,
                        corners[0].to_array(),
                        corners[1].to_array(),
                        corners[2].to_array(),
                        corners[3].to_array())
        .uv([0.0, 1.0],
            [1.0, 1.0],
            [1.0, 0.0],
            [0.0, 0.0])
        .build();
}

fn draw_camera_rect(draw_list: &imgui::DrawListMut<'_>,
                    minimap: &MinimapWidget,
                    camera: &Camera,
                    aabb: &Rect,
                    origin: Vec2,
                    center: Vec2) {
    let mut rect = minimap_camera_rect(minimap, origin, camera);

    // Clamp camera rect to minimap aabb minus some margin, and rotate if required.
    const CORNER_MARGIN: f32 = 4.0;
    if minimap.rotated {
        rect.min = rect.min.rotate_around_point(center, MINIMAP_ROTATION_ANGLE);
        rect.max = rect.max.rotate_around_point(center, MINIMAP_ROTATION_ANGLE);
        // NOTE: These have to be flipped due to the rotation.
        rect.max.x = rect.max.x.max(aabb.min.x + CORNER_MARGIN);
        rect.max.y = rect.max.y.max(aabb.min.y + CORNER_MARGIN);
        rect.min.x = rect.min.x.min(aabb.max.x - CORNER_MARGIN);
        rect.min.y = rect.min.y.min(aabb.max.y - CORNER_MARGIN);
    } else {
        rect.max.x = rect.max.x.min(aabb.max.x - CORNER_MARGIN);
        rect.max.y = rect.max.y.min(aabb.max.y - CORNER_MARGIN);
        rect.min.x = rect.min.x.max(aabb.min.x + CORNER_MARGIN);
        rect.min.y = rect.min.y.max(aabb.min.y + CORNER_MARGIN);
    }

    draw_list.add_rect(rect.min.to_array(),
                       rect.max.to_array(),
                       imgui::ImColor32::WHITE)
                       .build();
}

fn draw_outline_rect(draw_list: &imgui::DrawListMut<'_>,
                     minimap: &MinimapWidget,
                     corners: &[Vec2; 4],
                     aabb: &Rect,
                     cursor_screen_pos: Vec2) {
    let (rect_color, cursor_inside_minimap) = {
        if minimap.debug && coords::is_screen_point_inside_diamond(cursor_screen_pos, corners) {
            (imgui::ImColor32::from_rgb(255, 0, 0), true) // Red when cursor inside.
        } else {
            (imgui::ImColor32::BLACK, false)
        }
    };

    if cursor_inside_minimap {
        draw_list.add_circle(cursor_screen_pos.to_array(),
                             2.0,
                             rect_color)
                             .build();
    }

    draw_list.add_rect(aabb.min.to_array(),
                       aabb.max.to_array(),
                       rect_color)
                       .build();
}
