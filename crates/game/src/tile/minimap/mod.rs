// The minimap (docs/plans/minimap.md): a painted relief base baked on the CPU, buildings and
// roads as shapes over it, lenses, a toolbar, and realm styles from JSON.
//
// - model.rs: what the minimap knows (terrain and cover per cell, features), kept up to date by
//   place_tile and clear_tile.
// - crates/minimap_bake: the base texture, a pure function of the grid.
// - shapes.rs: buildings, fields and roads as batched meshes.
// - lens.rs: the lenses and their providers.
// - widget.rs: the layout and the view maths (zoom, scroll, picking).
// - render.rs: the menus, the toolbar and the drawing.
// - style.rs: the realm style files.

use std::collections::VecDeque;

use strum::{EnumCount, EnumIter};

use common::{
    Color,
    Size,
    coords::Cell,
    time::{Instant, Seconds},
};
use engine::{
    app::input::{InputAction, InputSystem, MouseButton},
    file_sys::paths::PathRef,
    render::texture::{TextureCache, TextureFilter, TextureHandle, TextureSettings, TextureWrapMode},
    ui::{self, UiSystem},
};
use minimap_bake::{CellRect, Cover, Terrain};

use super::{TileFlags, TileKind, TileMap, sets::TileDef};
use crate::{
    camera::Camera,
    save_context::{PostLoadContext, PreLoadContext},
};

pub mod lens;
pub mod model;
pub mod shapes;
pub mod style;
pub mod widget;

mod render;

pub use lens::MinimapLens;
pub use render::{DevUiMinimapRenderer, InGameUiMinimapRenderer, MinimapRenderer, MinimapTool};

use lens::LensData;
use model::{FeatureState, LandmarkKind, MinimapFeature, MinimapFeatures, MinimapGrid};
use shapes::ShapeCache;
use style::MinimapStyle;
pub use widget::MinimapFrame;
use widget::{MinimapLayout, MinimapView};

// ----------------------------------------------
// MinimapIcon
// ----------------------------------------------

pub const MINIMAP_ICON_DEFAULT_LIFETIME: Seconds = 5.0;

// Alerts are kept for the alerts tool this long, or until they're resolved.
pub const MINIMAP_ALERT_MAX_AGE: Seconds = 60.0;

#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Eq, Debug, EnumCount, EnumIter)]
pub enum MinimapIcon {
    Alert,
}

// A ping: an icon with rings growing out of it, fading over its lifetime.
#[derive(Copy, Clone)]
struct MinimapPing {
    icon: MinimapIcon,
    target_cell: Cell,
    tint: Color,
    lifetime: Seconds,
    time_left: Seconds,
}

#[derive(Copy, Clone)]
struct MinimapAlert {
    cell: Cell,
    icon: MinimapIcon,
    age: Seconds,
}

// ----------------------------------------------
// BaseTexture
// ----------------------------------------------

// The baked base: CPU pixels and the texture they're uploaded to.
#[derive(Default)]
struct BaseTexture {
    handle: TextureHandle,
    handle_size: Size, // The size the texture was made with.
    size: Size,        // In pixels.
    px_per_cell: u32,
    pixels: Vec<u8>,
    mips: Vec<Vec<u8>>, // Levels 1 and down, built from `pixels` when uploading.
    upload: Vec<u8>,
    scratch: minimap_bake::Scratch,
    style_version: u32,
    needs_full_bake: bool,
    pending_upload: CellRect, // Baked on the CPU, not yet uploaded.
    full_upload: bool,
    last_bake_secs: f64,
}

impl BaseTexture {
    const TEXTURE_NAME: &'static str = "minimap";

    // Bakes on the CPU what changed: everything after a load, a new map or a style change, else
    // the dirty cells and the margin around them, up to `budget_cells` a call.
    fn bake(&mut self, grid: &mut MinimapGrid, style: &MinimapStyle, style_version: u32, budget_cells: Option<i64>) {
        let size = grid.size();
        if !size.is_valid() {
            return;
        }

        let px = minimap_bake::px_per_cell(size.width, size.height, style.max_texture_side);
        let texture_size = Size::new(size.width * px as i32, size.height * px as i32);
        if texture_size != self.size || px != self.px_per_cell || self.style_version != style_version {
            self.size = texture_size;
            self.px_per_cell = px;
            self.style_version = style_version;
            self.pixels.clear();
            self.pixels.resize((texture_size.width * texture_size.height * 4) as usize, 0);
            self.needs_full_bake = true;
        }

        let bake_style = style.bake_style();
        let bake_grid = grid.bake_grid();
        let start = Instant::now();

        if self.needs_full_bake {
            minimap_bake::bake_all(&bake_grid, &bake_style, px, &mut self.scratch, &mut self.pixels);
            self.needs_full_bake = false;
            self.full_upload = true;
            grid.set_dirty(CellRect::default());
        } else {
            let dirty = grid.dirty();
            if dirty.is_empty() {
                return;
            }
            let margin = minimap_bake::margin_cells(&bake_style);
            let mut region = dirty.grown(margin).clipped(size.width, size.height);
            let mut remaining = CellRect::default();

            // Over budget: bake the top rows that fit and leave the rest dirty.
            if let Some(budget) = budget_cells
                && region.area() > budget
            {
                let rows = (budget / region.width().max(1) as i64).max(1) as i32;
                remaining = CellRect::new(dirty.x0, region.y0 + rows, dirty.x1, dirty.y1.max(region.y0 + rows + 1));
                region.y1 = region.y0 + rows;
            }

            minimap_bake::bake_region(&bake_grid, &bake_style, region, px, &mut self.scratch, &mut self.pixels);
            self.pending_upload = self.pending_upload.union(&region);
            grid.set_dirty(remaining);
        }

        self.last_bake_secs = Instant::now().duration_since(start).as_secs_f64();
    }

    // Uploads what was baked, with its mip levels: the map is drawn smaller than it's baked, and
    // without mips thin lines such as the foam would shimmer. The mips are built here, on the CPU,
    // since neither backend rebuilds them after an update.
    fn upload(&mut self, tex_cache: &mut TextureCache) {
        if !self.size.is_valid() {
            return;
        }

        if self.handle.is_valid() && self.handle_size != self.size {
            tex_cache.release_texture(&mut self.handle);
        }

        let mut full = self.full_upload;
        let mut level0_uploaded = false;
        if !self.handle.is_valid() {
            if let Some(mut existing) = tex_cache.find_loaded_texture(Self::TEXTURE_NAME) {
                tex_cache.release_texture(&mut existing);
            }
            let settings = TextureSettings { filter: TextureFilter::LinearMipmapLinear, wrap_mode: TextureWrapMode::ClampToEdge, mipmaps: true };
            self.handle = tex_cache.new_initialized_texture(Self::TEXTURE_NAME, self.size, &self.pixels, Some(settings));
            self.handle_size = self.size;
            full = true;
            level0_uploaded = true;
        }

        let px = self.px_per_cell as i32;
        let region = if full {
            CellRect::map(self.size.width / px, self.size.height / px)
        } else {
            self.pending_upload
        };
        self.full_upload = false;
        self.pending_upload = CellRect::default();
        if region.is_empty() {
            return;
        }

        // Level 0, then each mip level over the same area.
        let (x0, y0) = (region.x0 * px, region.y0 * px);
        let (x1, y1) = (region.x1 * px, region.y1 * px);
        self.build_mips(x0, y0, x1, y1);

        for level in 0..=self.mips.len() {
            if level == 0 && level0_uploaded {
                continue;
            }
            let level_size = mip_size(self.size, level);
            let (lx0, ly0) = (x0 >> level, y0 >> level);
            let lx1 = ((x1 + (1 << level) - 1) >> level).min(level_size.width);
            let ly1 = ((y1 + (1 << level) - 1) >> level).min(level_size.height);
            if lx1 <= lx0 || ly1 <= ly0 {
                continue;
            }

            let source = if level == 0 { &self.pixels } else { &self.mips[level - 1] };
            let stride = level_size.width as usize * 4;
            let width = (lx1 - lx0) as usize;
            self.upload.clear();
            for row in ly0 as usize..ly1 as usize {
                let start = row * stride + lx0 as usize * 4;
                self.upload.extend_from_slice(&source[start..start + width * 4]);
            }
            tex_cache.update_texture(self.handle, lx0 as u32, ly0 as u32, Size::new(width as i32, ly1 - ly0), level as u32, &self.upload);
        }
    }

    // Box-filters the mip levels over the level 0 pixels from (x0, y0) up to (x1, y1).
    fn build_mips(&mut self, x0: i32, y0: i32, x1: i32, y1: i32) {
        let count = mip_count(self.size);
        self.mips.resize_with(count - 1, Vec::new);
        for level in 1..count {
            let size = mip_size(self.size, level);
            let bytes = (size.width * size.height * 4) as usize;
            if self.mips[level - 1].len() != bytes {
                self.mips[level - 1] = vec![0; bytes];
            }
        }

        for level in 1..count {
            let size = mip_size(self.size, level);
            let parent_size = mip_size(self.size, level - 1);
            let (lx0, ly0) = (x0 >> level, y0 >> level);
            let lx1 = ((x1 + (1 << level) - 1) >> level).min(size.width);
            let ly1 = ((y1 + (1 << level) - 1) >> level).min(size.height);

            let (before, after) = self.mips.split_at_mut(level - 1);
            let parent: &[u8] = if level == 1 { &self.pixels } else { &before[level - 2] };
            let target = &mut after[0];

            minimap_bake::downsample(parent, (parent_size.width, parent_size.height), target, (size.width, size.height), (lx0, ly0, lx1, ly1));
        }
    }

    fn release(&mut self, tex_cache: &mut TextureCache) {
        tex_cache.release_texture(&mut self.handle);
        self.size = Size::zero();
        self.needs_full_bake = true;
    }

    fn memory_usage_estimate(&self) -> usize {
        self.pixels.capacity() + self.upload.capacity() + self.mips.iter().map(Vec::capacity).sum::<usize>()
    }
}

// Mip levels down to 1 × 1, as both backends count them.
fn mip_count(size: Size) -> usize {
    (size.width.max(size.height).max(1) as f32).log2().floor() as usize + 1
}

fn mip_size(size: Size, level: usize) -> Size {
    Size::new((size.width >> level).max(1), (size.height >> level).max(1))
}

// ----------------------------------------------
// LensTexture
// ----------------------------------------------

#[derive(Default)]
struct LensTexture {
    handle: TextureHandle,
    size: Size,
    uploaded_version: Option<u64>,
    texels: Vec<u8>,
}

impl LensTexture {
    const TEXTURE_NAME: &'static str = "minimap_lens";

    fn update(&mut self, tex_cache: &mut TextureCache, data: &LensData, map_size: Size) {
        if !data.has_heat_field() || self.uploaded_version == Some(data.version) || !map_size.is_valid() {
            return;
        }
        data.texels(&mut self.texels);

        if self.handle.is_valid() && self.size != map_size {
            tex_cache.release_texture(&mut self.handle);
        }
        if !self.handle.is_valid() {
            if let Some(mut existing) = tex_cache.find_loaded_texture(Self::TEXTURE_NAME) {
                tex_cache.release_texture(&mut existing);
            }
            let settings = TextureSettings { filter: TextureFilter::Linear, wrap_mode: TextureWrapMode::ClampToEdge, mipmaps: false };
            self.handle = tex_cache.new_initialized_texture(Self::TEXTURE_NAME, map_size, &self.texels, Some(settings));
            self.size = map_size;
        } else {
            tex_cache.update_texture(self.handle, 0, 0, map_size, 0, &self.texels);
        }
        self.uploaded_version = Some(data.version);
    }

    fn release(&mut self, tex_cache: &mut TextureCache) {
        tex_cache.release_texture(&mut self.handle);
        self.uploaded_version = None;
    }
}

// ----------------------------------------------
// MinimapTextures: sprites from the style
// ----------------------------------------------

#[derive(Default)]
struct MinimapTextures {
    style_version: Option<u32>,
    field: TextureHandle,
    alert: TextureHandle,
    landmarks: [TextureHandle; LandmarkKind::COUNT],
}

impl MinimapTextures {
    fn load(&mut self, tex_cache: &mut TextureCache, style: &MinimapStyle, style_version: u32) {
        if self.style_version == Some(style_version) {
            return;
        }
        self.style_version = Some(style_version);

        let ui_path = |path: &str| ui::assets_path().join(path);
        let load = |tex_cache: &mut TextureCache, path: &str, settings: TextureSettings| {
            let path = ui_path(path);
            tex_cache.load_texture_with_settings(PathRef::from_str(path.as_str()), Some(settings))
        };

        let repeat = TextureSettings { filter: TextureFilter::Linear, wrap_mode: TextureWrapMode::Repeat, mipmaps: false };
        self.field = load(tex_cache, &style.field_texture, repeat);
        self.alert = load(tex_cache, &style.alert_icon, ui::texture_settings());
        for (kind, path) in &style.landmark_icons {
            self.landmarks[*kind as usize] = load(tex_cache, path, ui::texture_settings());
        }
    }

    fn icon(&self, icon: MinimapIcon) -> TextureHandle {
        match icon {
            MinimapIcon::Alert => self.alert,
        }
    }
}

// ----------------------------------------------
// Minimap
// ----------------------------------------------

pub struct Minimap {
    grid: MinimapGrid,
    features: MinimapFeatures,
    base: BaseTexture,
    textures: MinimapTextures,
    shapes: ShapeCache,

    lens: MinimapLens,
    lens_data: LensData,
    lens_texture: LensTexture,
    lens_refresh_in: Seconds,

    pings: Vec<MinimapPing>,
    alerts: VecDeque<MinimapAlert>,
    next_alert: usize,
    raised_alerts: u32, // New alerts since take_raised_alerts, for the GameLoop's Alert sound.

    view: MinimapView,
    layout: MinimapLayout,
    frame: MinimapFrame, // The room its window's frame takes, set by the renderer drawing it.
    is_open: bool,
    show_landmarks: bool, // The building icons in the City view; alerts show either way.
    pointer_blocked: bool, // A dialog covers the HUD: no hover tooltip, no click to move the camera.
}

impl Default for Minimap {
    fn default() -> Self {
        Self {
            grid: MinimapGrid::default(),
            features: MinimapFeatures::default(),
            base: BaseTexture { needs_full_bake: true, ..Default::default() },
            textures: MinimapTextures::default(),
            shapes: ShapeCache::default(),
            lens: MinimapLens::City,
            lens_data: LensData::default(),
            lens_texture: LensTexture::default(),
            lens_refresh_in: 0.0,
            pings: Vec::new(),
            alerts: VecDeque::new(),
            next_alert: 0,
            raised_alerts: 0,
            view: MinimapView::default(),
            layout: MinimapLayout::default(),
            frame: MinimapFrame::default(),
            is_open: true,
            show_landmarks: true,
            pointer_blocked: false,
        }
    }
}

impl Minimap {
    pub fn new(map_size_in_cells: Size) -> Self {
        let mut minimap = Self::default();
        minimap.grid.reset(map_size_in_cells, None);
        minimap
    }

    pub fn update(
        &mut self,
        camera: &mut Camera,
        tex_cache: &mut TextureCache,
        input_sys: &InputSystem,
        ui_sys: &UiSystem,
        pointer_blocked: bool,
        delta_time_secs: Seconds,
    ) {
        self.pointer_blocked = pointer_blocked;
        let style = style::current();
        let style_version = style::version();

        self.textures.load(tex_cache, style, style_version);
        self.bake(Some(style.bake_budget_cells as i64));
        self.base.upload(tex_cache);
        self.lens_texture.update(tex_cache, &self.lens_data, self.grid.size());

        self.update_pings(delta_time_secs);
        self.update_alerts(delta_time_secs);

        let legend_entries = if self.lens == MinimapLens::City { 0 } else { self.lens_data.legend.len() };
        self.layout = MinimapLayout::new(ui::screen_size(ui_sys), render::TOOL_COUNT, style.map_size(), legend_entries, style.legend.scale(), self.frame);
        if !self.is_open || !self.grid.size().is_valid() {
            return;
        }

        self.view.update(camera, self.layout.map, self.grid.size(), input_sys.cursor_pos(), delta_time_secs);

        // Click or drag on the map to move the camera there, unless a dialog covers the minimap.
        if !self.pointer_blocked
            && input_sys.mouse_button_state(MouseButton::Left) == InputAction::Press
            && let Some(cell) = self.view.pick_cell(self.view.cursor_pos)
        {
            camera.teleport_iso(widget::world_to_iso(widget::cell_to_world(cell.0)));
        }
    }

    // Bakes what changed on the CPU, without uploading it: `budget_cells` limits a patch bake,
    // None bakes all that's dirty.
    pub fn bake(&mut self, budget_cells: Option<i64>) {
        let style = style::current();
        self.base.bake(&mut self.grid, style, style::version(), budget_cells);
    }

    #[inline]
    pub fn pre_load(&mut self, context: &mut PreLoadContext) {
        let tex_cache = context.engine_mut().texture_cache_mut();
        self.base.release(tex_cache);
        self.lens_texture.release(tex_cache);
        self.pings.clear();
        self.alerts.clear();
    }

    #[inline]
    pub fn post_load(&mut self, context: &PostLoadContext) {
        self.rebuild_from(context.tile_map());
    }

    // Builds the whole minimap again from the tile map's tiles.
    pub fn rebuild_from(&mut self, tile_map: &TileMap) {
        self.grid.reset(tile_map.size_in_cells(), None);
        self.features.clear();
        self.base.needs_full_bake = true;
        self.view.reset();

        tile_map.for_each_tile(TileKind::Terrain, |_tile_map, terrain| {
            self.place_tile(terrain.base_cell(), terrain.tile_def());
        });
        tile_map.for_each_tile(MINIMAP_OBJECT_TILE_KINDS, |_tile_map, object| {
            self.place_tile(object.base_cell(), object.tile_def());
            self.on_tile_flags_changed(object.base_cell(), object.flags());
        });
    }

    #[inline]
    pub fn memory_usage_estimate(&self) -> usize {
        self.base.memory_usage_estimate() + self.grid.memory_usage_estimate() + self.lens_texture.texels.capacity()
    }

    #[inline]
    pub fn tile_count(&self) -> usize {
        let size = self.grid.size();
        (size.width * size.height) as usize
    }

    #[inline]
    pub fn size_in_cells(&self) -> Size {
        self.grid.size()
    }

    pub fn reset(&mut self, fill_with_def: Option<&'static TileDef>, new_map_size: Option<Size>) {
        let size = new_map_size.unwrap_or(self.grid.size());
        self.grid.reset(size, fill_with_def);
        self.features.clear();
        self.base.needs_full_bake = true;
        self.view.reset();
        self.pings.clear();
        self.alerts.clear();
        self.raised_alerts = 0;
        self.lens_data.clear(self.lens);
    }

    // ----------------------
    // Tile placement:
    // ----------------------

    pub fn place_tile(&mut self, target_cell: Cell, tile_def: &'static TileDef) {
        if !self.grid.is_cell_within_bounds(target_cell) {
            return;
        }

        if tile_def.is(TileKind::Terrain) {
            let (terrain, road) = model::terrain_of(tile_def);
            self.grid.set_terrain(target_cell, terrain, road);
            return;
        }

        let Some(cover) = model::cover_of(tile_def) else {
            return; // Units and anything else the minimap doesn't show.
        };
        for cell in &tile_def.cell_range(target_cell) {
            self.grid.set_cover(cell, cover);
        }
        if cover == Cover::Building {
            self.features.insert(MinimapFeature::new(target_cell, tile_def));
        }
    }

    pub fn clear_tile(&mut self, target_cell: Cell, tile_def: &'static TileDef) {
        if !self.grid.is_cell_within_bounds(target_cell) {
            return;
        }

        if tile_def.is(TileKind::Terrain) {
            self.grid.set_terrain(target_cell, Terrain::None, model::MinimapRoad::None);
            return;
        }

        if model::cover_of(tile_def).is_none() {
            return;
        }
        for cell in &tile_def.cell_range(target_cell) {
            self.grid.set_cover(cell, Cover::None);
        }
        self.features.remove(target_cell);
    }

    // Construction sites and renovations, from the building's tile flags.
    pub fn on_tile_flags_changed(&mut self, base_cell: Cell, flags: TileFlags) {
        self.features.set_state(base_cell, FeatureState::from_tile_flags(flags));
    }

    // ----------------------
    // Icons and alerts:
    // ----------------------

    // Shows a ping at `target_cell` for `lifetime_secs`, and keeps an alert there for the alerts
    // tool. A new alert plays the Alert sound (GameLoop::notify_minimap_alerts).
    // TODO(realm-ui): only debug tools and minimap.alert raise alerts so far; the game's warnings
    // (M11's messages: fires, collapses, unrest) will push them here.
    pub fn push_icon(&mut self, icon: MinimapIcon, target_cell: Cell, tint: Color, lifetime_secs: Seconds) {
        self.pings.push(MinimapPing { icon, target_cell, tint, lifetime: lifetime_secs, time_left: lifetime_secs });

        // One alert per cell: a repeat restarts its age, quietly.
        if let Some(alert) = self.alerts.iter_mut().find(|alert| alert.cell == target_cell && alert.icon == icon) {
            alert.age = 0.0;
        } else {
            self.alerts.push_back(MinimapAlert { cell: target_cell, icon, age: 0.0 });
            self.raised_alerts += 1;
        }
    }

    // How many new alerts were raised since the last call.
    pub fn take_raised_alerts(&mut self) -> u32 {
        std::mem::take(&mut self.raised_alerts)
    }

    // The alert after the last one visited, cycling, or None without alerts.
    pub fn next_alert(&mut self) -> Option<Cell> {
        if self.alerts.is_empty() {
            return None;
        }
        let index = self.next_alert % self.alerts.len();
        self.next_alert = index + 1;
        Some(self.alerts[index].cell)
    }

    // Drops the alerts at `cell`, e.g. once what raised them is resolved.
    pub fn resolve_alerts(&mut self, cell: Cell) {
        self.alerts.retain(|alert| alert.cell != cell);
    }

    #[inline]
    pub fn alert_count(&self) -> usize {
        self.alerts.len()
    }

    fn update_pings(&mut self, delta_time_secs: Seconds) {
        for ping in &mut self.pings {
            ping.time_left -= delta_time_secs;
        }
        self.pings.retain(|ping| ping.time_left > 0.0);
    }

    fn update_alerts(&mut self, delta_time_secs: Seconds) {
        for alert in &mut self.alerts {
            alert.age += delta_time_secs;
        }
        let before = self.alerts.len();
        self.alerts.retain(|alert| alert.age < MINIMAP_ALERT_MAX_AGE);
        if self.alerts.len() != before {
            self.next_alert = 0;
        }
    }

    // ----------------------
    // Lenses:
    // ----------------------

    #[inline]
    pub fn lens(&self) -> MinimapLens {
        self.lens
    }

    pub fn set_lens(&mut self, lens: MinimapLens) {
        if lens != self.lens {
            self.lens = lens;
            self.lens_data.clear(lens);
            self.lens_refresh_in = 0.0; // Refresh at the next draw.
        }
    }

    // True when the lens should be computed again, every refresh_secs while one is open.
    pub fn lens_refresh_due(&mut self, delta_time_secs: Seconds) -> bool {
        if self.lens == MinimapLens::City {
            return false;
        }
        self.lens_refresh_in -= delta_time_secs;
        if self.lens_refresh_in <= 0.0 {
            self.lens_refresh_in = style::current().lenses.refresh_secs.max(0.1);
            return true;
        }
        false
    }

    // Computes the open lens again. The minimap lives in the tile map that the lens providers
    // read through `sim`, so its lens data is taken out while they compute it and put back after.
    pub fn refresh_lens(sim: &crate::sim::SimContextMut) {
        let mut lens_data = std::mem::take(&mut sim.minimap_mut().lens_data);

        let minimap = sim.tile_map().minimap();
        let lens = minimap.lens;
        lens_data.clear(lens);
        if let Some(provider) = lens::provider(lens) {
            let context = lens::LensContext { sim, features: &minimap.features, map_size: minimap.grid.size(), style: style::current() };
            provider.compute(&context, &mut lens_data);
        }

        sim.minimap_mut().lens_data = lens_data;
    }

    // ----------------------
    // Queries:
    // ----------------------

    // Whether a dialog covers the HUD this frame (GameMenusSystem::covers_hud).
    #[inline]
    pub fn is_pointer_blocked(&self) -> bool {
        self.pointer_blocked
    }

    #[inline]
    pub fn is_open(&self) -> bool {
        self.is_open
    }

    #[inline]
    pub fn set_open(&mut self, open: bool) {
        self.is_open = open;
    }

    #[inline]
    pub fn shows_landmarks(&self) -> bool {
        self.show_landmarks
    }

    #[inline]
    pub fn set_show_landmarks(&mut self, show: bool) {
        self.show_landmarks = show;
    }

    #[inline]
    pub fn view(&self) -> &MinimapView {
        &self.view
    }

    #[inline]
    pub fn view_mut(&mut self) -> &mut MinimapView {
        &mut self.view
    }

    #[inline]
    // The room its window's frame takes (the retail HUD's panel), from the next layout on.
    pub fn set_frame(&mut self, frame: MinimapFrame) {
        self.frame = frame;
    }

    pub fn layout(&self) -> &MinimapLayout {
        &self.layout
    }

    #[inline]
    pub fn grid(&self) -> &MinimapGrid {
        &self.grid
    }

    #[inline]
    pub fn features(&self) -> &MinimapFeatures {
        &self.features
    }

    #[inline]
    pub fn lens_data(&self) -> &LensData {
        &self.lens_data
    }

    #[inline]
    pub fn texture_size(&self) -> Size {
        self.base.size
    }

    #[inline]
    pub fn px_per_cell(&self) -> u32 {
        self.base.px_per_cell
    }

    #[inline]
    pub fn last_bake_secs(&self) -> f64 {
        self.base.last_bake_secs
    }

    // The baked base, RGBA8, rows from cell y = 0 down. Bakes first if anything is pending.
    pub fn baked_pixels(&mut self) -> (&[u8], Size) {
        self.bake(None);
        (&self.base.pixels, self.base.size)
    }

    // The seat (palace or governor's house) for the Home tool: its centre cell.
    pub fn seat_cell(&self) -> Option<Cell> {
        self.features.find_landmark(LandmarkKind::Seat).map(|feature| {
            let (x, y) = feature.center();
            Cell::new(x.round() as i32, y.round() as i32)
        })
    }

    // What's at a cell, as the hover tooltip names it.
    pub fn describe_cell(&self, cell: Cell) -> Option<String> {
        if !self.grid.is_cell_within_bounds(cell) {
            return None;
        }
        if let Some(feature) = self.features.at(cell) {
            return Some(display_name(&feature.tile_def.name));
        }
        let name = match (self.grid.road(cell), self.grid.cover(cell), self.grid.terrain(cell)) {
            (model::MinimapRoad::Dirt, ..) => "Dirt road",
            (model::MinimapRoad::Paved, ..) => "Paved road",
            (_, Cover::Vegetation, _) => "Trees",
            (_, Cover::Rocks, _) => "Rocks",
            (_, _, Terrain::Water) => "Water",
            (_, _, Terrain::VacantLot) => "Vacant lot",
            (_, _, Terrain::Land) => "Land",
            _ => return None,
        };
        Some(name.to_string())
    }
}

// "rice_farm" as "Rice farm".
fn display_name(name: &str) -> String {
    let mut text = name.replace('_', " ");
    if let Some(first) = text.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    text
}

// Minimap only displays terrain tiles and these object kinds.
const MINIMAP_OBJECT_TILE_KINDS: TileKind =
    TileKind::from_bits_retain(TileKind::Building.bits() | TileKind::Rocks.bits() | TileKind::Vegetation.bits());

