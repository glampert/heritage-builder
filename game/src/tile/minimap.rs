use super::{
    TileKind, TileMap, TileMapLayerKind, sets::TileDef, water,
};

use crate::{
    save::PostLoadContext,
    utils::{Size, coords::Cell},
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
    size_in_pixels: Size,
    pixels: Vec<MinimapTileColor>,
    handle: TextureHandle,
    changed: bool,
}

impl MinimapTexture {
    fn new(size_in_pixels: Size) -> Self {
        let pixel_count = (size_in_pixels.width * size_in_pixels.height) as usize;
        Self {
            size_in_pixels,
            pixels: vec![MinimapTileColor::default(); pixel_count],
            handle: TextureHandle::invalid(),
            changed: true,
        }
    }

    fn memory_usage_estimate(&self) -> usize {
        self.pixels.capacity() * std::mem::size_of::<MinimapTileColor>()
    }

    fn reset(&mut self, size_in_pixels: Size, fill_color: MinimapTileColor) {
        self.changed = true;

        if size_in_pixels == self.size_in_pixels {
            self.pixels.fill(fill_color);
            return; // No change in size.
        }

        self.pixels.clear();

        let pixel_count = (size_in_pixels.width * size_in_pixels.height) as usize;
        self.pixels.resize(pixel_count, fill_color);

        self.size_in_pixels = size_in_pixels;
    }

    fn update(&mut self, tex_cache: &mut dyn TextureCache) {
        if !self.changed || !self.size_in_pixels.is_valid() {
            return;
        }

        if !self.handle.is_valid() {
            let settings = TextureSettings {
                filter: TextureFilter::Nearest,
                gen_mipmaps: false,
                ..Default::default()
            };
            self.handle = tex_cache.new_uninitialized_texture("minimap",
                                                              self.size_in_pixels,
                                                              Some(settings));
        }

        let len_in_bytes  = self.pixels.len() * std::mem::size_of::<MinimapTileColor>();
        let bytes_ptr = self.pixels.as_ptr() as *const u8;
        let pixels = unsafe { std::slice::from_raw_parts(bytes_ptr, len_in_bytes) };

        tex_cache.update_texture(self.handle, 0, 0, self.size_in_pixels, 0, pixels);

        self.changed = false;
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
        self.changed = true;
    }

    #[inline]
    fn cell_to_index(&self, cell: Cell) -> usize {
        let cell_index = cell.x + (cell.y * self.size_in_pixels.width);
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
        if (cell.x < 0 || cell.x >= self.texture.size_in_pixels.width)
            || (cell.y < 0 || cell.y >= self.texture.size_in_pixels.height)
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
    pub fn post_load(&mut self, context: &PostLoadContext) {
        self.texture.post_load(context.tile_map());
    }

    #[inline]
    pub fn texture_handle(&self) -> TextureHandle {
        self.texture.handle
    }

    #[inline]
    pub fn memory_usage_estimate(&self) -> usize {
        self.texture.memory_usage_estimate()
    }

    #[inline]
    pub fn tile_count(&self) -> usize {
        // 1 pixel = 1 tile
        (self.texture.size_in_pixels.width * self.texture.size_in_pixels.height) as usize
    }

    pub fn reset(&mut self, fill_with_def: Option<&'static TileDef>, new_map_size: Option<Size>) {
        let fill_color = {
            if let Some(tile_def) = fill_with_def {
                MinimapTileColor::from_tile_def(tile_def).unwrap_or_default()
            } else {
                MinimapTileColor::default()
            }
        };

        let size_in_pixels = new_map_size.unwrap_or(self.texture.size_in_pixels);
        self.texture.reset(size_in_pixels, fill_color);
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

    pub fn draw(&self, render_sys: &mut impl RenderSystem, ui_sys: &UiSystem) {
        static SHOW_MINIMAP: UiStaticVar<bool> = UiStaticVar::new(true);
        let ui = ui_sys.builder();

        if *SHOW_MINIMAP {
            let window_flags =
                imgui::WindowFlags::NO_RESIZE
                | imgui::WindowFlags::NO_SCROLLBAR
                | imgui::WindowFlags::NO_MOVE
                | imgui::WindowFlags::NO_COLLAPSE;

            let window_size = [
                210.0,
                230.0,
            ];

            let window_position = [
                5.0,
                ui.io().display_size[1] - window_size[1] - 5.0,
            ];

            ui.window("Minimap")
                .opened(SHOW_MINIMAP.as_mut())
                .flags(window_flags)
                .position(window_position, imgui::Condition::Always)
                .size(window_size, imgui::Condition::Always)
                .build(|| {
                    let tex_cache = render_sys.texture_cache();
                    let texture_id = ui_sys.to_ui_texture(tex_cache, self.texture.handle);
                    let angle_radians = -45.0 * (std::f32::consts::PI / 180.0);
                    draw_rotated_minimap(ui, texture_id, [35.0, 55.0], [140.0, 140.0], angle_radians);
                });
        } else { // Minimap open/close button:
            let window_flags =
                imgui::WindowFlags::NO_DECORATION
                | imgui::WindowFlags::NO_BACKGROUND
                | imgui::WindowFlags::NO_RESIZE
                | imgui::WindowFlags::NO_SCROLLBAR
                | imgui::WindowFlags::NO_MOVE
                | imgui::WindowFlags::NO_COLLAPSE;

            let window_position = [
                5.0,
                ui.io().display_size[1] - 35.0,
            ];

            ui.window("Minimap Button")
                .flags(window_flags)
                .position(window_position, imgui::Condition::Always)
                .always_auto_resize(true)
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

fn rotate_around_center(p: [f32; 2], center: [f32; 2], angle_radians: f32) -> [f32; 2] {
    let s = angle_radians.sin();
    let c = angle_radians.cos();

    let dx = p[0] - center[0];
    let dy = p[1] - center[1];

    [
        center[0] + dx * c - dy * s,
        center[1] + dx * s + dy * c,
    ]
}

fn draw_rotated_minimap(ui: &imgui::Ui, texture_id: UiTextureHandle, pos: [f32; 2], size: [f32; 2], angle_radians: f32) {
    let draw_list = ui.get_window_draw_list();

    // Screen-space position where the window is located.
    let origin = ui.window_pos();

    // Add window offset to the local position:
    let x = origin[0] + pos[0];
    let y = origin[1] + pos[1];
    let w = size[0];
    let h = size[1];

    // Quad vertices in screen space (unrotated):
    let mut p0 = [x,     y];
    let mut p1 = [x + w, y];
    let mut p2 = [x + w, y + h];
    let mut p3 = [x,     y + h];

    // Rotate around the center of the quad:
    let center = [x + w * 0.5, y + h * 0.5];
    p0 = rotate_around_center(p0, center, angle_radians);
    p1 = rotate_around_center(p1, center, angle_radians);
    p2 = rotate_around_center(p2, center, angle_radians);
    p3 = rotate_around_center(p3, center, angle_radians);

    let uv0 = [0.0, 1.0];
    let uv1 = [1.0, 1.0];
    let uv2 = [1.0, 0.0];
    let uv3 = [0.0, 0.0];

    draw_list
        .add_image_quad(texture_id, p0, p1, p2, p3)
        .uv(uv0, uv1, uv2, uv3)
        .build();
}
