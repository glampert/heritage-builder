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
    imgui_ui::{self, UiStaticVar, UiSystem, UiTextureHandle},
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
}

impl Minimap {
    pub fn new(size_in_cells: Size) -> Self {
        Self {
            // One pixel per tile map cell.
            texture: MinimapTexture::new(size_in_cells),
            icons: Vec::new(),
        }
    }

    #[inline]
    pub fn update(&mut self, tex_cache: &mut dyn TextureCache, delta_time_secs: Seconds) {
        // Preload icon textures once:
        if !MinimapIconTexCache::get().are_icon_textures_loaded() {
            MinimapIconTexCache::get_mut().load_icon_textures(tex_cache);
        }

        self.texture.update(tex_cache);
        self.update_icons(delta_time_secs);
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

    fn draw_icons(&self, render_sys: &mut impl RenderSystem, ui_sys: &UiSystem, minimap: &MinimapWidget) {
        if self.icons.is_empty() {
            return;
        }

        let ui = ui_sys.ui();
        let tex_cache = render_sys.texture_cache();

        let draw_list = ui.get_window_draw_list();
        let origin = Vec2::from_array(ui.window_pos());

        // Minimap center.
        let center = Rect::new(origin + minimap.screen_pos, minimap.screen_size).center();

        for icon in &self.icons {
            if icon.lifetime <= 0.0 || icon.time_left <= 0.0 {
                continue;
            }

            let mut icon_center = cell_to_minimap_px(minimap, origin, icon.target_cell);
            if minimap.rotated {
                icon_center = icon_center.rotate_around_point(center, MINIMAP_ROTATION_ANGLE);
            }

            let icon_half_size = MINIMAP_ICON_SIZE / 2.0;
            let icon_rect = Rect::from_extents(
                Vec2::new(icon_center.x - icon_half_size, icon_center.y - icon_half_size),
                Vec2::new(icon_center.x + icon_half_size, icon_center.y + icon_half_size)
            );

            // Fade-out based on remaining lifetime seconds.
            let icon_tint_alpha = (icon.time_left / icon.lifetime).clamp(0.0, 1.0);
            let icon_tint = Color::new(icon.tint.r, icon.tint.g, icon.tint.b, icon_tint_alpha);

            let icon_texture = ui_sys.to_ui_texture(tex_cache, icon.texture);

            draw_list
                .add_image(icon_texture, icon_rect.min.to_array(), icon_rect.max.to_array())
                .col(imgui::ImColor32::from_rgba_f32s(icon_tint.r, icon_tint.g, icon_tint.b, icon_tint_alpha))
                .build();
        }
    }

    // ----------------------
    // Minimap rendering:
    // ----------------------

    // Draw the minimap using ImGui, nestled inside a window.
    pub fn draw(&self,
                camera: &mut Camera,
                render_sys: &mut impl RenderSystem,
                ui_sys: &UiSystem,
                cursor_screen_pos: Vec2) {

        const ENABLE_DEBUG_CONTROLS: bool = true;

        static MINIMAP: UiStaticVar<MinimapWidget> = UiStaticVar::new(
            MinimapWidget {
                screen_pos: Vec2::new(35.0, 55.0),
                screen_size: Vec2::new(128.0, 128.0),
                size: Size::zero(), // Set each time before drawing.
                rotated: true,
                opened: true,

                // Minimap Scrolling:
                offsets: Vec2::zero(),
                zoom: 1.0,
                scroll_speed_px: 30.0,
                enable_auto_scroll: true,

                // Debug:
                enable_debug_draw: ENABLE_DEBUG_CONTROLS,
                enable_debug_controls: ENABLE_DEBUG_CONTROLS,
                show_debug_controls: ENABLE_DEBUG_CONTROLS,
            });

        if MINIMAP.opened {
            // Minimap widget window:
            self.draw_minimap_widget_window(MINIMAP.as_mut(), camera, render_sys, ui_sys, cursor_screen_pos);
        } else {
            // Minimap open/close button:
            Self::draw_open_minimap_button(MINIMAP.as_mut(), ui_sys);
        }
    }

    fn draw_minimap_widget_window(&self,
                                  minimap: &mut MinimapWidget,
                                  camera: &mut Camera,
                                  render_sys: &mut impl RenderSystem,
                                  ui_sys: &UiSystem,
                                  cursor_screen_pos: Vec2) {
        let ui = ui_sys.ui();

        // 1 minimap texture pixel = 1 tile map cell.
        minimap.size = self.texture.size;

        let window_size = [minimap.screen_size.x + 70.0, minimap.screen_size.y + 90.0];
        let window_position = [5.0, ui.io().display_size[1] - window_size[1] - 5.0];

        let window_flags =
            imgui::WindowFlags::NO_RESIZE
            | imgui::WindowFlags::NO_SCROLLBAR
            | imgui::WindowFlags::NO_MOVE
            | imgui::WindowFlags::NO_COLLAPSE;

        let mut opened = minimap.opened;

        ui.window("Minimap")
            .opened(&mut opened)
            .flags(window_flags)
            .position(window_position, imgui::Condition::Always)
            .size(window_size, imgui::Condition::Always)
            .build(|| {
                let tex_cache = render_sys.texture_cache();
                let texture = ui_sys.to_ui_texture(tex_cache, self.texture.handle);

                let (hovered_tile_iso, camera_corners_outside_minimap) =
                    draw_minimap_widget(minimap, texture, camera, cursor_screen_pos, ui);

                self.draw_icons(render_sys, ui_sys, minimap);

                if let Some(teleport_destination_iso) = hovered_tile_iso {
                    if ui.is_mouse_down(imgui::MouseButton::Left) {
                        camera.teleport_iso(teleport_destination_iso);
                    }
                }

                if minimap.enable_auto_scroll {
                    Self::scroll(
                        minimap,
                        camera_corners_outside_minimap,
                        ui.io().delta_time);
                }

                if minimap.enable_debug_controls {
                    Self::draw_debug_controls(
                        minimap,
                        camera,
                        camera_corners_outside_minimap,
                        window_position,
                        window_size,
                        ui_sys);
                }
            });

        minimap.opened = opened;
    }

    fn draw_debug_controls(minimap: &mut MinimapWidget,
                           camera: &mut Camera,
                           camera_corners_outside_minimap: CameraRectCorners,
                           parent_window_position: [f32; 2],
                           parent_window_size: [f32; 2],
                           ui_sys: &UiSystem) {
        let ui = ui_sys.ui();

        // Debug controls checkbox at the minimap widget's bottom:
        ui.dummy([0.0, parent_window_size[1] - 65.0]);
        ui.dummy([2.0, 0.0]); ui.same_line();
        ui.checkbox("Debug", &mut minimap.show_debug_controls);

        if !minimap.show_debug_controls {
            return;
        }

        let window_position = [
            parent_window_position[0] + parent_window_size[0] + 10.0,
            parent_window_position[1] - 50.0,
        ];

        let window_flags =
            imgui::WindowFlags::NO_RESIZE
            | imgui::WindowFlags::NO_SCROLLBAR
            | imgui::WindowFlags::NO_MOVE
            | imgui::WindowFlags::NO_COLLAPSE;

        ui.window(format!("Minimap Debug {}", minimap.size))
            .opened(&mut minimap.show_debug_controls)
            .flags(window_flags)
            .position(window_position, imgui::Condition::Always)
            .always_auto_resize(true)
            .build(|| {
                if ui.small_button("Reset") {
                    camera.center();
                    minimap.offsets = Vec2::zero();
                    minimap.zoom = 1.0;
                }

                ui.same_line();
                ui.checkbox("Rotated", &mut minimap.rotated);
                ui.same_line();
                ui.checkbox("Scrolling", &mut minimap.enable_auto_scroll);
                ui.same_line();
                ui.checkbox("Debug Draw", &mut minimap.enable_debug_draw);

                if imgui_ui::input_f32(
                    ui,
                    "Scroll Speed:",
                    &mut minimap.scroll_speed_px,
                    false,
                    Some(1.0))
                {
                    minimap.scroll_speed_px = minimap.scroll_speed_px.max(1.0);
                }

                let camera_center = minimap_camera_center_cell(camera);

                if imgui_ui::input_f32(ui,
                    "Zoom:",
                    &mut minimap.zoom,
                    false,
                    Some(0.1))
                {
                    minimap.zoom = minimap.zoom.clamp(1.0, 5.0);

                    minimap.offsets = calc_minimap_offsets_from_center(
                        camera_center,
                        minimap.size.to_vec2(),
                        minimap.zoom);
                }

                imgui_ui::input_f32_xy(
                    ui,
                    "Offsets:",
                    &mut minimap.offsets,
                    false,
                    None,
                    None);

                let (uv_min, uv_max) =
                    calc_minimap_rect_uvs(minimap.size.to_vec2(), minimap.offsets, minimap.zoom);

                ui.text(format!("UVs min:{uv_min} max:{uv_max}"));
                ui.text(format!("Cam Center Cell: {}", camera_center.0));

                if camera_corners_outside_minimap.is_empty() {
                    ui.text("Camera Corners Out: None");
                } else {
                    ui.text("Camera Corners Out:");
                    ui.same_line();
                    ui.text_colored(Color::red().to_array(),
                                    camera_corners_outside_minimap.to_string());
                }
            });
    }

    fn draw_open_minimap_button(minimap: &mut MinimapWidget, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();

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
                    minimap.opened = true;
                }
            });
    }

    fn scroll(minimap: &mut MinimapWidget,
              camera_corners_outside_minimap: CameraRectCorners,
              delta_time_secs: Seconds) {
        let (uv_min, uv_max) = calc_minimap_rect_uvs(minimap.size.to_vec2(), minimap.offsets, minimap.zoom);
        let mut scrollable_corners = CameraRectCorners::all();

        // Corners already at their limits will not scroll further.
        if uv_min.x <= 0.0 {
            scrollable_corners.remove(CameraRectCorners::BottomLeft);
        }
        if uv_min.y <= 0.0 {
            scrollable_corners.remove(CameraRectCorners::BottomRight);
        }
        if uv_max.x >= 1.0 {
            scrollable_corners.remove(CameraRectCorners::TopRight);
        }
        if uv_max.y >= 1.0 {
            scrollable_corners.remove(CameraRectCorners::TopLeft);
        }

        // Minimap scrolling:
        if camera_corners_outside_minimap.intersects(CameraRectCorners::TopLeft)
            && scrollable_corners.intersects(CameraRectCorners::TopLeft)
        {
            minimap.offsets.y += minimap.scroll_speed_px * delta_time_secs;
        }
        if camera_corners_outside_minimap.intersects(CameraRectCorners::BottomRight)
            && scrollable_corners.intersects(CameraRectCorners::BottomRight)
        {
            minimap.offsets.y -= minimap.scroll_speed_px * delta_time_secs;
        }
        if camera_corners_outside_minimap.intersects(CameraRectCorners::TopRight)
            && scrollable_corners.intersects(CameraRectCorners::TopRight)
        {
            minimap.offsets.x += minimap.scroll_speed_px * delta_time_secs;
        }
        if camera_corners_outside_minimap.intersects(CameraRectCorners::BottomLeft)
            && scrollable_corners.intersects(CameraRectCorners::BottomLeft)
        {
            minimap.offsets.x -= minimap.scroll_speed_px * delta_time_secs;
        }
    }
}

// ----------------------------------------------
// MinimapWidget
// ----------------------------------------------

struct MinimapWidget {
    screen_pos: Vec2,         // Top-left corner in screen space, relative to `origin`.
    screen_size: Vec2,        // Display size in pixels.
    size: Size,               // Minimap / TileMap size in cells (1 cell = 1 minimap pixel).
    rotated: bool,            // Apply the 45 degrees isometric rotation when drawing the minimap?
    opened: bool,

    offsets: Vec2,            // Minimap texture offset/panning in pixels.
    zoom: f32,                // Zoom amount, [0,1] range; 1=draw full texture, >1, zooms in.
    scroll_speed_px: f32,     // Scroll speed in pixels per second when `enable_auto_scroll=true`.
    enable_auto_scroll: bool, // Scroll minimap when camera rect touches the minimap edges?

    // Debug switches:
    enable_debug_draw: bool,
    enable_debug_controls: bool,
    show_debug_controls: bool,
}

// ----------------------------------------------
// Coordinate space conversion helpers
// ----------------------------------------------

// Rotate the minimap -45 degrees to match our isometric world projection.
const MINIMAP_ROTATION_ANGLE: f32 = -45.0 * (std::f32::consts::PI / 180.0);

// Map fractional cell coords to minimap UV [0,1].
fn cell_to_minimap_uv_f32(x: f32, y: f32, size: Size) -> Vec2 {
    Vec2::new(
        x / size.width as f32,
        // NOTE: Flip V for ImGui (because OpenGL textures have V=0 at bottom).
        1.0 - (y / size.height as f32)
    )
}

fn minimap_uv_to_cell_f32(uv: Vec2, size: Size) -> CellF32 {
    CellF32(Vec2::new(
        uv.x * size.width as f32,
        // NOTE: Flip V for ImGui (because OpenGL textures have V=0 at bottom).
        (1.0 - uv.y) * size.height as f32,
    ))
}

// Map minimap UVs in [0,1] range into minimap screen pixels.
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
    let uv = cell_to_minimap_uv_f32(cell.x as f32, cell.y as f32, minimap.size);
    minimap_uv_to_minimap_px(minimap, origin, uv)
}

fn cell_to_minimap_px_f32(minimap: &MinimapWidget, origin: Vec2, cell: CellF32) -> Vec2 {
    let uv = cell_to_minimap_uv_f32(cell.0.x, cell.0.y, minimap.size);
    minimap_uv_to_minimap_px(minimap, origin, uv)
}

// ----------------------------------------------
// Cursor -> minimap cell picking
// ----------------------------------------------

// Returns floating-point isometric coords without rounding to cell space.
fn pick_minimap_iso(minimap: &MinimapWidget, origin: Vec2, cursor_screen_pos: Vec2) -> Option<IsoPointF32> {
    // Undo minimap rotation first if needed:
    let minimap_px = {
        if minimap.rotated {
            let minimap_abs_pos = origin + minimap.screen_pos;
            let center = minimap_abs_pos + minimap.screen_size * 0.5;
            cursor_screen_pos.rotate_around_point(center, -MINIMAP_ROTATION_ANGLE)
        } else {
            cursor_screen_pos
        }
    };

    // Convert to local minimap uv coordinates, [0,1] range:
    let uv = minimap_px_to_minimap_uv(minimap, origin, minimap_px);
    if uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 {
        return None; // outside minimap.
    }

    // Rotated: sample continuous cell coordinates.
    if minimap.rotated {
        // Compute corresponding *continuous* cell coordinates.
        // (0..map_width, 0..map_height), no rounding.
        let cell = minimap_uv_to_cell_f32(uv, minimap.size);

        Some(coords::cell_to_iso_f32(cell, BASE_TILE_SIZE))
    } else {
        // Unrotated: sample continuous iso coords directly.
        let bounds = calc_map_bounds_iso(minimap.size);

        let iso_x = bounds.min.x + (uv.x * bounds.width());
        let iso_y = bounds.min.y + (uv.y * bounds.height());

        Some(IsoPointF32(Vec2::new(iso_x, iso_y)))
    }
}

// ----------------------------------------------
// Minimap camera rect overlay helpers
// ----------------------------------------------

bitflags_with_display! {
    #[derive(Copy, Clone)]
    struct CameraRectCorners: u32 {
        const TopLeft     = 1 << 0;
        const TopRight    = 1 << 1;
        const BottomLeft  = 1 << 2;
        const BottomRight = 1 << 3;
    }
}

// Rect in screen space, ready to be drawn with ImGui.
fn minimap_camera_rect_in_screen_px(minimap: &MinimapWidget, origin: Vec2, camera: &Camera) -> Rect {
    let center_iso = camera.iso_world_position();
    let half_iso   = camera.iso_viewport_center();

    let (uv_min, uv_max) = {
        if minimap.rotated {
            // Convert iso rect corners to fractional cell coordinates (continuous):
            let cell_min_frac = coords::iso_to_cell_f32(IsoPointF32(center_iso.0 - half_iso.0), BASE_TILE_SIZE);
            let cell_max_frac = coords::iso_to_cell_f32(IsoPointF32(center_iso.0 + half_iso.0), BASE_TILE_SIZE);

            // Important: ensure correct ordering (min <= max) after transform.
            let cell_x_min = cell_min_frac.0.x.min(cell_max_frac.0.x);
            let cell_x_max = cell_min_frac.0.x.max(cell_max_frac.0.x);
            let cell_y_min = cell_min_frac.0.y.min(cell_max_frac.0.y);
            let cell_y_max = cell_min_frac.0.y.max(cell_max_frac.0.y);

            let uv_min = cell_to_minimap_uv_f32(cell_x_min, cell_y_min, minimap.size);
            let uv_max = cell_to_minimap_uv_f32(cell_x_max, cell_y_max, minimap.size);

            (uv_min, uv_max)
        } else {
            // Convert iso -> UV directly using world iso bounds:
            fn point_to_uv(rect: &Rect, p: Vec2) -> Vec2 {
                Vec2::new(
                    (p.x - rect.min.x) / rect.width(),
                    (p.y - rect.min.y) / rect.height()
                )
            }

            let iso_min = center_iso.0 - half_iso.0;
            let iso_max = center_iso.0 + half_iso.0;

            let bounds = calc_map_bounds_iso(minimap.size);
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
        coords::cell_to_iso_f32(CellF32(Vec2::new(0.0, 0.0)), BASE_TILE_SIZE).0,
        coords::cell_to_iso_f32(CellF32(Vec2::new(0.0, map_height)), BASE_TILE_SIZE).0,
        coords::cell_to_iso_f32(CellF32(Vec2::new(map_width, 0.0)), BASE_TILE_SIZE).0,
        coords::cell_to_iso_f32(CellF32(Vec2::new(map_width, map_height)), BASE_TILE_SIZE).0,
    ];

    Rect::aabb(&points)
}

fn minimap_camera_center_cell(camera: &Camera) -> CellF32 {
    let center_iso = camera.iso_world_position();
    coords::iso_to_cell_f32(center_iso, BASE_TILE_SIZE)
}

// Compute minimap scrolling offset so that we zoom *around the camera center*.
// `camera_center` is the world camera center in minimap pixels (cell coords).
fn calc_minimap_offsets_from_center(camera_center: CellF32, minimap_size_px: Vec2, zoom: f32) -> Vec2 {
    // Size of the visible window in minimap pixels.
    let visible_size = minimap_size_px / zoom.max(1.0);

    // Offset so camera center stays fixed regardless of zoom.
    let mut offset = camera_center.0 - (visible_size * 0.5);

    // Clamp to texture bounds.
    offset.x = offset.x.clamp(0.0, minimap_size_px.x - visible_size.x);
    offset.y = offset.y.clamp(0.0, minimap_size_px.y - visible_size.y);

    offset
}

fn calc_minimap_rect_uvs(minimap_size_px: Vec2, offsets: Vec2, zoom: f32) -> (Vec2, Vec2) {
    let visible_size = minimap_size_px / zoom.max(1.0);

    let uv_min = offsets / minimap_size_px;
    let uv_max = (offsets + visible_size) / minimap_size_px;

    (uv_min, uv_max)
}

// ----------------------------------------------
// ImGui draw helpers
// ----------------------------------------------

fn draw_minimap_widget(minimap: &MinimapWidget,
                       texture: UiTextureHandle,
                       camera: &Camera,
                       cursor_screen_pos: Vec2,
                       ui: &imgui::Ui) -> (Option<IsoPointF32>, CameraRectCorners) {
    let draw_list = ui.get_window_draw_list();
    let origin = Vec2::from_array(ui.window_pos());

    let rect = Rect::new(origin + minimap.screen_pos, minimap.screen_size);
    let center = rect.center();

    let corners = rect_corners(&rect, center, minimap.rotated);
    let aabb = Rect::aabb(&corners);

    let minimap_size_px = minimap.size.to_vec2();

    draw_minimap_rect(&draw_list, texture, minimap_size_px, &corners, minimap.offsets, minimap.zoom);
    draw_outline_rect(&draw_list, minimap, &corners, &aabb, cursor_screen_pos);

    let camera_corners_outside_minimap =
        draw_camera_rect(&draw_list, minimap, camera, &corners, &aabb, origin, center);

    let hovered_tile_iso = {
        let cursor_inside_minimap =
            coords::is_screen_point_inside_diamond(cursor_screen_pos, &corners);

        if cursor_inside_minimap {
            pick_minimap_iso(minimap, origin, cursor_screen_pos)
        } else {
            None
        }
    };

    (hovered_tile_iso, camera_corners_outside_minimap)
}

fn draw_minimap_rect(draw_list: &imgui::DrawListMut<'_>,
                     texture: UiTextureHandle,
                     minimap_size_px: Vec2,
                     corners: &[Vec2; 4],
                     offsets: Vec2,
                     zoom: f32) {
    let (uv_min, uv_max) = calc_minimap_rect_uvs(minimap_size_px, offsets, zoom);

    draw_list
        .add_image_quad(
            texture,
            corners[0].to_array(),
            corners[1].to_array(),
            corners[2].to_array(),
            corners[3].to_array())
        .uv(
            [uv_min.x, uv_max.y],
            uv_max.to_array(),
            [uv_max.x, uv_min.y],
            uv_min.to_array())
        .build();
}

fn draw_camera_rect(draw_list: &imgui::DrawListMut<'_>,
                    minimap: &MinimapWidget,
                    camera: &Camera,
                    corners: &[Vec2; 4],
                    aabb: &Rect,
                    origin: Vec2,
                    center: Vec2) -> CameraRectCorners {
    let mut rect = minimap_camera_rect_in_screen_px(minimap, origin, camera).scaled(minimap.zoom);
    let mut corners_outside = CameraRectCorners::empty();

    // Clamp camera rect to minimap aabb minus some margin, and rotate if required.
    const CORNER_MARGIN: f32 = 4.0;
    if minimap.rotated {
        rect.min = rect.min.rotate_around_point(center, MINIMAP_ROTATION_ANGLE);
        rect.max = rect.max.rotate_around_point(center, MINIMAP_ROTATION_ANGLE);

        // NOTE: These have to be flipped due to the minimap rotation.
        rect.max.x = rect.max.x.max(aabb.min.x + CORNER_MARGIN);
        rect.max.y = rect.max.y.max(aabb.min.y + CORNER_MARGIN);
        rect.min.x = rect.min.x.min(aabb.max.x - CORNER_MARGIN);
        rect.min.y = rect.min.y.min(aabb.max.y - CORNER_MARGIN);

        // NOTE: CameraRectCorners left/right are inverted due to the minimap rotation.
        if !coords::is_screen_point_inside_diamond(rect.top_left(), corners) {
            corners_outside |= CameraRectCorners::TopRight;
        }
        if !coords::is_screen_point_inside_diamond(rect.top_right(), corners) {
            corners_outside |= CameraRectCorners::TopLeft;
        }
        if !coords::is_screen_point_inside_diamond(rect.bottom_left(), corners) {
            corners_outside |= CameraRectCorners::BottomRight;
        }
        if !coords::is_screen_point_inside_diamond(rect.bottom_right(), corners) {
            corners_outside |= CameraRectCorners::BottomLeft;
        }
    } else {
        rect.max.x = rect.max.x.min(aabb.max.x - CORNER_MARGIN);
        rect.max.y = rect.max.y.min(aabb.max.y - CORNER_MARGIN);
        rect.min.x = rect.min.x.max(aabb.min.x + CORNER_MARGIN);
        rect.min.y = rect.min.y.max(aabb.min.y + CORNER_MARGIN);

        if rect.max.x >= aabb.max.x - CORNER_MARGIN {
            corners_outside |= CameraRectCorners::TopRight;
        }
        if rect.max.y >= aabb.max.y - CORNER_MARGIN {
            corners_outside |= CameraRectCorners::BottomRight;
        }
        if rect.min.x <= aabb.min.x + CORNER_MARGIN {
            corners_outside |= CameraRectCorners::BottomLeft;
        }
        if rect.min.y <= aabb.min.y + CORNER_MARGIN {
            corners_outside |= CameraRectCorners::TopLeft;
        }
    }

    let outline_color = if minimap.enable_debug_draw && !corners_outside.is_empty() {
        // Color it red if any corner of the camera rect falls outside the minimap.
        imgui::ImColor32::from_rgb(255, 0, 0)
    } else {
        imgui::ImColor32::WHITE
    };

    draw_list.add_rect(rect.min.to_array(),
                       rect.max.to_array(),
                       outline_color)
                       .build();

    // Draw a circle at the camera's center point:
    if minimap.enable_debug_draw {
        let screen_point = {
            if minimap.rotated {
                // We want to visualize the result of minimap_camera_center_in_cells:
                let camera_center = minimap_camera_center_cell(camera);
                let screen_point  = cell_to_minimap_px_f32(minimap, origin, camera_center);
                screen_point.rotate_around_point(center, MINIMAP_ROTATION_ANGLE)
            } else {
                rect.center()
            }
        };

        draw_list.add_circle(screen_point.to_array(),
                             4.0,
                             imgui::ImColor32::from_rgb(0, 255, 0))
                             .build();
    }

    corners_outside
}

fn draw_outline_rect(draw_list: &imgui::DrawListMut<'_>,
                     minimap: &MinimapWidget,
                     corners: &[Vec2; 4],
                     aabb: &Rect,
                     cursor_screen_pos: Vec2) {
    let (rect_color, cursor_inside_minimap) = {
        if minimap.enable_debug_draw && coords::is_screen_point_inside_diamond(cursor_screen_pos, corners) {
            (imgui::ImColor32::from_rgb(255, 0, 0), true) // Red when cursor inside.
        } else {
            (imgui::ImColor32::BLACK, false)
        }
    };

    if cursor_inside_minimap {
        draw_list.add_circle(cursor_screen_pos.to_array(),
                             4.0,
                             rect_color)
                             .build();
    }

    draw_list.add_rect(aabb.min.to_array(),
                       aabb.max.to_array(),
                       rect_color)
                       .build();
}

fn rect_corners(rect: &Rect, center: Vec2, rotated: bool) -> [Vec2; 4] {
    let mut corners = rect.corners_ccw();
    if rotated {
        for corner in &mut corners {
            *corner = corner.rotate_around_point(center, MINIMAP_ROTATION_ANGLE);
        }
    }
    corners
}
