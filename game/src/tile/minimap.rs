use super::{
    TileKind, TileMap, TileMapLayerKind, sets::TileDef,
    water, camera::Camera, BASE_TILE_SIZE,
};

use crate::{
    save::{PreLoadContext, PostLoadContext},
    imgui_ui::{self, UiSystem, UiTextureHandle, UiStaticVar},
    utils::{Size, Vec2, coords::{self, Cell, IsoPoint, WorldToScreenTransform}},
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
    const BLACK:        Self = Self { r: 0,   g: 0,   b: 0,   a: 255 };
    const WHITE:        Self = Self { r: 255, g: 255, b: 255, a: 255 };
    const CYAN:         Self = Self { r: 0,   g: 255, b: 255, a: 255 };
    const MAGENTA:      Self = Self { r: 255, g: 0,   b: 255, a: 255 };
    const LIGHT_RED:    Self = Self { r: 250, g: 35,  b: 35,  a: 255 };
    const DARK_RED:     Self = Self { r: 195, g: 15,  b: 15,  a: 255 };
    const LIGHT_PINK:   Self = Self { r: 220, g: 20,  b: 195, a: 255 };
    const DARK_PINK:    Self = Self { r: 140, g: 5,   b: 120, a: 255 };
    const LIGHT_PURPLE: Self = Self { r: 165, g: 70,  b: 185, a: 255 };
    const DARK_PURPLE:  Self = Self { r: 80,  g: 25,  b: 90,  a: 255 };
    const LIGHT_GREEN:  Self = Self { r: 112, g: 125, b: 55,  a: 255 };
    const DARK_GREEN:   Self = Self { r: 10,  g: 115, b: 25,  a: 255 };
    const LIGHT_YELLOW: Self = Self { r: 210, g: 225, b: 20,  a: 255 };
    const DARK_YELLOW:  Self = Self { r: 225, g: 200, b: 20,  a: 255 };
    const LIGHT_BLUE:   Self = Self { r: 15,  g: 100, b: 230, a: 255 };
    const DARK_BLUE:    Self = Self { r: 30,  g: 100, b: 115, a: 255 };
    const LIGHT_BROWN:  Self = Self { r: 110, g: 65,  b: 35,  a: 255 };
    const DARK_BROWN:   Self = Self { r: 75,  g: 35,  b: 10,  a: 255 };
    const LIGHT_GRAY:   Self = Self { r: 115, g: 110, b: 105, a: 255 };
    const DARK_GRAY:    Self = Self { r: 70,  g: 65,  b: 60,  a: 255 };

    const EMPTY_LAND:   Self = Self::LIGHT_GREEN;
    const VACANT_LOT:   Self = Self::LIGHT_YELLOW;
    const WATER:        Self = Self::DARK_BLUE;
    const ROAD:         Self = Self::LIGHT_BROWN;
    const ROCKS:        Self = Self::DARK_GRAY;
    const VEGETATION:   Self = Self::DARK_GREEN;

    fn from_tile_def(tile_def: &TileDef) -> Option<Self> {
        Some({
            if tile_def.path_kind.is_empty_land() {
                Self::EMPTY_LAND
            } else if tile_def.path_kind.is_vacant_lot() {
                Self::VACANT_LOT
            } else if tile_def.path_kind.is_water() {
                Self::WATER
            } else if tile_def.path_kind.is_road() {
                Self::ROAD
            } else if tile_def.path_kind.is_rocks() {
                Self::ROCKS
            } else if tile_def.path_kind.is_vegetation() {
                Self::VEGETATION
            } else if tile_def.is(TileKind::Building) {
                Self::for_building_tile(tile_def)
            } else {
                // Units or anything else we don't display on the minimap.
                return None;
            }
        })
    }

    fn for_building_tile(tile_def: &TileDef) -> Self {
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
            Self::DARK_GRAY
        }
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

    fn pre_load(&mut self, context: &PreLoadContext) {
        // Release the current minimap texture. It will be recreated
        // with the correct dimensions on next update().
        context.tex_cache_mut().release_texture(&mut self.handle);
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
    fn is_cell_within_bounds(&self, cell: Cell) -> bool {
        if (cell.x < 0 || cell.x >= self.texture.size.width)
            || (cell.y < 0 || cell.y >= self.texture.size.height)
        {
            return false;
        }
        true
    }

    #[inline]
    pub fn update(&mut self, tex_cache: &mut dyn TextureCache) {
        self.texture.update(tex_cache);
    }

    #[inline]
    pub fn pre_load(&mut self, context: &PreLoadContext) {
        self.texture.pre_load(context);
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
        if !self.is_cell_within_bounds(target_cell) {
            return;
        }

        if let Some(color) = MinimapTileColor::from_tile_def(tile_def) {
            for cell in &tile_def.cell_range(target_cell) {
                self.texture.set_pixel(cell, color);
            }
        }
    }

    pub fn clear_tile(&mut self, target_cell: Cell, tile_def: &'static TileDef) {
        if !self.is_cell_within_bounds(target_cell) {
            return;
        }

        if tile_def.is(TileKind::Terrain) {
            self.texture.set_pixel(target_cell, MinimapTileColor::default());
        } else if water::is_port_or_wharf(tile_def) {
            for cell in &tile_def.cell_range(target_cell) {
                self.texture.set_pixel(cell, MinimapTileColor::WATER);
            }
        } else if tile_def.is(MINIMAP_OBJECT_TILE_KINDS) {
            for cell in &tile_def.cell_range(target_cell) {
                self.texture.set_pixel(cell, MinimapTileColor::EMPTY_LAND);
            }
        }
    }

    // Draw the minimap using ImGui, nestled inside a window.
    pub fn draw(&self, render_sys: &mut impl RenderSystem, ui_sys: &UiSystem, camera: &Camera) {
        static SHOW_MINIMAP: UiStaticVar<bool> = UiStaticVar::new(true);
        let ui = ui_sys.builder();

        if *SHOW_MINIMAP {
            let minimap = MinimapDrawParams {
                screen_pos: Vec2::new(35.0, 55.0),
                screen_size: Vec2::new(140.0, 140.0),
                map_size: self.texture.size, // Same as tile map size in cells.
                rotated: true,
            };

            let window_size = [212.0, 230.0];
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
                    draw_minimap(ui, camera, &minimap, texture);
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
// ImGui draw helpers
// ----------------------------------------------

// Rotate the minimap -45 degrees to match our world isometric projection.
const MINIMAP_ROTATION_ANGLE: f32 = -45.0 * (std::f32::consts::PI / 180.0);

fn draw_minimap(ui: &imgui::Ui,
                camera: &Camera,
                minimap: &MinimapDrawParams,
                texture: UiTextureHandle) {
    let draw_list = ui.get_window_draw_list();

    let origin = ui.window_pos();
    let rect_x = origin[0] + minimap.screen_pos.x;
    let rect_y = origin[1] + minimap.screen_pos.y;
    let rect_w = minimap.screen_size.x;
    let rect_h = minimap.screen_size.y;
    let center = [rect_x + rect_w * 0.5, rect_y + rect_h * 0.5];

    // Draw the minimap rect:
    {
        // Rect vertices in screen space (unrotated):
        let mut points = [
            [rect_x, rect_y],
            [rect_x + rect_w, rect_y],
            [rect_x + rect_w, rect_y + rect_h],
            [rect_x, rect_y + rect_h],
        ];

        // Rotate around the center of the rect:
        if minimap.rotated {
            for point in &mut points {
                *point = rotate_around_center(*point, center, MINIMAP_ROTATION_ANGLE);
            }
        }

        draw_list
            .add_image_quad(texture, points[0], points[1], points[2], points[3])
            .uv([0.0, 1.0],
                [1.0, 1.0],
                [1.0, 0.0],
                [0.0, 0.0])
            .build();
    }

    // Draw camera rectangle overlay:
    {
        let vp_size = camera.viewport_size().to_vec2();
        let transform = camera.transform();

        let (top_left, bottom_right) = minimap_camera_rect(minimap, vp_size, transform);

        let mut tl = top_left.to_array();
        tl[0] += origin[0];
        tl[1] += origin[1];

        let mut br = bottom_right.to_array();
        br[0] += origin[0];
        br[1] += origin[1];

        if minimap.rotated {
            tl = rotate_around_center(tl, center, MINIMAP_ROTATION_ANGLE);
            br = rotate_around_center(br, center, MINIMAP_ROTATION_ANGLE);
        }

        draw_list.add_rect(tl, br, imgui::ImColor32::WHITE).build();
    }
}

fn rotate_around_center(p: [f32; 2], center: [f32; 2], angle_radians: f32) -> [f32; 2] {
    let (s, c) = angle_radians.sin_cos();
    let dx = p[0] - center[0];
    let dy = p[1] - center[1];
    [
        center[0] + dx * c - dy * s,
        center[1] + dx * s + dy * c,
    ]
}

// ----------------------------------------------
// Coordinate space conversion helpers
// ----------------------------------------------

struct MinimapDrawParams {
    screen_pos: Vec2,  // Top-left corner in screen space.
    screen_size: Vec2, // Display size in pixels.
    map_size: Size,    // Tile map size in cells.
    rotated: bool,     // Apply the 45 degrees isometric rotation when drawing the minimap?
}

fn cell_to_minimap_uv(cell: Cell, map_size: Size) -> Vec2 {
    Vec2::new(
        cell.x as f32 / map_size.width  as f32,
        // Flip V for ImGui (because OpenGL textures have V=0 at bottom).
        1.0 - (cell.y as f32 / map_size.height as f32)
    )
}

fn minimap_uv_to_cell(uv: Vec2, map_size: Size) -> Cell {
    Cell::new(
        (uv.x * map_size.width  as f32).round() as i32,
        (uv.y * map_size.height as f32).round() as i32
    )
}

fn minimap_uv_to_screen_rect(minimap: &MinimapDrawParams, uv: Vec2) -> Vec2 {
    minimap.screen_pos + (uv * minimap.screen_size)
}

fn minimap_screen_rect_to_uv(minimap: &MinimapDrawParams, minimap_point: Vec2) -> Vec2 {
    (minimap_point - minimap.screen_pos) / minimap.screen_size
}

// ----------------------------------------------
// TileMap cell -> minimap screen rect
// ----------------------------------------------

fn cell_to_minimap_rect(minimap: &MinimapDrawParams, cell: Cell) -> Vec2 {
    let uv = cell_to_minimap_uv(cell, minimap.map_size);
    minimap_uv_to_screen_rect(minimap, uv)
}

fn minimap_rect_to_cell(minimap: &MinimapDrawParams, minimap_point: Vec2) -> Cell {
    let uv = minimap_screen_rect_to_uv(minimap, minimap_point);

    if uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 {
        return Cell::invalid(); // outside minimap.
    }

    minimap_uv_to_cell(uv, minimap.map_size)
}

// ----------------------------------------------
// Iso coords -> minimap screen rect
// ----------------------------------------------

fn iso_to_minimap_rect(minimap: &MinimapDrawParams, iso_point: Vec2) -> Vec2 {
    // NOTE: This is the same as coords::iso_to_cell() but using f32.
    let half_tile_width  = (BASE_TILE_SIZE.width  / 2) as f32;
    let half_tile_height = (BASE_TILE_SIZE.height / 2) as f32;

    // Invert Y axis to match top-left origin.
    let cell_x = ((iso_point.x / half_tile_width) + (-iso_point.y / half_tile_height)) / 2.0;
    let cell_y = ((-iso_point.y / half_tile_height) - (iso_point.x / half_tile_width)) / 2.0;
    let cell = Cell::new(cell_x.round() as i32, cell_y.round() as i32);

    cell_to_minimap_rect(minimap, cell)
}

fn minimap_rect_to_iso(minimap: &MinimapDrawParams, minimap_point: Vec2) -> IsoPoint {
    let cell = minimap_rect_to_cell(minimap, minimap_point);
    coords::cell_to_iso(cell, BASE_TILE_SIZE)
}

// ----------------------------------------------
// Viewport -> minimap screen rect
// ----------------------------------------------

fn viewport_to_minimap_rect(minimap: &MinimapDrawParams, vp_size: Vec2, vp_point: Vec2) -> Vec2 {
    let uv = vp_point / vp_size;     // normalized point in full screen
    minimap_uv_to_screen_rect(minimap, uv) // map into minimap rect
}

fn minimap_rect_to_viewport(minimap: &MinimapDrawParams, vp_size: Vec2, minimap_point: Vec2) -> Vec2 {
    let uv = minimap_screen_rect_to_uv(minimap, minimap_point);
    uv * vp_size
}

// ----------------------------------------------
// Camera rect minimap overlay
// ----------------------------------------------

fn minimap_camera_rect(minimap: &MinimapDrawParams, vp_size: Vec2, transform: WorldToScreenTransform) -> (Vec2, Vec2) {
    // Camera center in world/iso coords:
    let viewport_center_screen = vp_size * 0.5;
    let center_iso = (viewport_center_screen - transform.offset) / transform.scaling;

    // Half-extent of viewport in world units:
    let half_iso = vp_size * 0.5 / transform.scaling;

    // World TL and BR corners:
    let iso_tl = center_iso + Vec2::new(-half_iso.x, -half_iso.y);
    let iso_br = center_iso + Vec2::new( half_iso.x,  half_iso.y);

    let mm_tl = iso_to_minimap_rect(minimap, iso_tl);
    let mm_br = iso_to_minimap_rect(minimap, iso_br);

    // Return axis-aligned rectangle:
    (mm_tl, mm_br)
}
