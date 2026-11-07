// The minimap's crisp layer: buildings, fields and roads as shapes over the baked base.
//
// The shapes are kept in cell coordinates and rebuilt only when the features, the roads or the
// style change. Each frame they're moved to the screen with the view's affine transform, culled
// to the visible cells and batched into three meshes (shadows; fields; roads and buildings), so
// a frame costs a few vertices per building and no tessellation. Quads get a one-pixel feather
// ring for anti-aliasing.

use common::{Color, Vec2};
use engine::{render::texture::TextureHandle, ui::UiMesh};

use super::{
    model::{FeatureState, LandmarkKind, MinimapClass, MinimapFeature, MinimapFeatures, MinimapGrid, MinimapRoad},
    style::MinimapStyle,
    widget::CellToMinimap,
};

// ----------------------------------------------
// ShapeCache
// ----------------------------------------------

// An axis-aligned rectangle in cell coordinates (cell centres at whole numbers).
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct CellBox {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl CellBox {
    #[inline]
    fn inset(&self, amount: f32) -> Self {
        Self { x0: self.x0 + amount, y0: self.y0 + amount, x1: self.x1 - amount, y1: self.y1 - amount }
    }

    #[inline]
    fn offset(&self, dx: f32, dy: f32) -> Self {
        Self { x0: self.x0 + dx, y0: self.y0 + dy, x1: self.x1 + dx, y1: self.y1 + dy }
    }

    #[inline]
    fn overlaps(&self, other: &Self) -> bool {
        self.x0 <= other.x1 && other.x0 <= self.x1 && self.y0 <= other.y1 && other.y0 <= self.y1
    }

    #[inline]
    fn corners(&self) -> [(f32, f32); 4] {
        [(self.x0, self.y0), (self.x1, self.y0), (self.x1, self.y1), (self.x0, self.y1)]
    }
}

#[derive(Copy, Clone)]
pub struct FeatureShape {
    pub feature: MinimapFeature,
    pub footprint: CellBox, // The cells it covers, edge to edge.
    pub base_color: Color,
}

#[derive(Copy, Clone)]
pub struct RoadShape {
    pub from: (f32, f32), // Centres of the first and last cells of a straight run.
    pub to: (f32, f32),
    pub paved: bool,
}

#[derive(Default)]
pub struct ShapeCache {
    versions: Option<(u64, u64, u32)>, // Features, roads, style.
    pub features: Vec<FeatureShape>,
    pub roads: Vec<RoadShape>,
    pub landmarks: Vec<(Vec2, LandmarkKind, f32)>, // Centre in cells, kind, footprint size in cells.
}

impl ShapeCache {
    // Rebuilds the cache if anything it holds changed.
    pub fn refresh(&mut self, features: &MinimapFeatures, grid: &MinimapGrid, style: &MinimapStyle, style_version: u32) {
        let versions = (features.version(), grid.roads_version(), style_version);
        if self.versions == Some(versions) {
            return;
        }
        self.versions = Some(versions);
        self.rebuild(features, grid, style);
    }

    pub fn invalidate(&mut self) {
        self.versions = None;
    }

    fn rebuild(&mut self, features: &MinimapFeatures, grid: &MinimapGrid, style: &MinimapStyle) {
        self.features.clear();
        self.landmarks.clear();
        self.roads.clear();

        for feature in features.iter() {
            if feature.class == MinimapClass::None {
                continue;
            }
            let x0 = feature.base_cell.x as f32 - 0.5;
            let y0 = feature.base_cell.y as f32 - 0.5;
            let footprint = CellBox { x0, y0, x1: x0 + feature.size.width as f32, y1: y0 + feature.size.height as f32 };
            let base_color = match feature.class {
                MinimapClass::Field => feature.color.unwrap_or(style.palette.field.color()),
                _ => feature.color.unwrap_or_else(|| style.sector_colors.color(feature.sector)),
            };
            self.features.push(FeatureShape { feature: *feature, footprint, base_color });

            if let Some(landmark) = feature.landmark {
                let (cx, cy) = feature.center();
                self.landmarks.push((Vec2::new(cx, cy), landmark, feature.size.width.max(feature.size.height) as f32));
            }
        }

        // Roads: straight runs along x, then along y, between cell centres. A road cell with no
        // neighbour along either axis is a run of one.
        let size = grid.size();
        let is_road = |x: i32, y: i32| grid.road(common::coords::Cell::new(x, y)) != MinimapRoad::None;
        let is_paved = |x: i32, y: i32| grid.road(common::coords::Cell::new(x, y)) == MinimapRoad::Paved;

        for y in 0..size.height {
            let mut x = 0;
            while x < size.width {
                if !is_road(x, y) {
                    x += 1;
                    continue;
                }
                let paved = is_paved(x, y);
                let start = x;
                while x + 1 < size.width && is_road(x + 1, y) && is_paved(x + 1, y) == paved {
                    x += 1;
                }
                let alone = start == x && !is_road(x, y - 1) && !is_road(x, y + 1);
                if start < x || alone {
                    self.roads.push(RoadShape { from: (start as f32, y as f32), to: (x as f32, y as f32), paved });
                }
                x += 1;
            }
        }
        for x in 0..size.width {
            let mut y = 0;
            while y < size.height {
                if !is_road(x, y) {
                    y += 1;
                    continue;
                }
                let paved = is_paved(x, y);
                let start = y;
                while y + 1 < size.height && is_road(x, y + 1) && is_paved(x, y + 1) == paved {
                    y += 1;
                }
                if start < y {
                    self.roads.push(RoadShape { from: (x as f32, start as f32), to: (x as f32, y as f32), paved });
                }
                y += 1;
            }
        }
    }

    pub fn shape_count(&self) -> usize {
        self.features.len() + self.roads.len()
    }
}

// ----------------------------------------------
// Meshes
// ----------------------------------------------

pub struct MinimapMeshes {
    pub shadows: UiMesh,
    pub fields: UiMesh,
    pub top: UiMesh, // Road casings, roads, outlines and buildings, in that order.
}

pub struct MeshParams<'a> {
    pub transform: CellToMinimap,
    pub visible: CellBox,             // Cells on the map area, for culling.
    pub pixels_per_point: f32,
    pub style: &'a MinimapStyle,
    pub field_texture: TextureHandle,
    // A lens's colour for a feature, None for neutral. Without a lens, features keep their colour.
    pub lens: Option<&'a dyn Fn(&MinimapFeature) -> Option<Color>>,
}

pub fn build_meshes(cache: &ShapeCache, params: &MeshParams) -> MinimapMeshes {
    let style = params.style;
    let side_px = params.transform.cell_side() * params.pixels_per_point;
    let feather = 1.0 / params.pixels_per_point; // One pixel, in points.
    let detailed = side_px >= 1.5;

    let mut meshes = MinimapMeshes {
        shadows: UiMesh::with_capacity(TextureHandle::White, cache.features.len() * 12, cache.features.len() * 30),
        fields: UiMesh::new(params.field_texture),
        top: UiMesh::with_capacity(TextureHandle::White, (cache.features.len() * 2 + cache.roads.len() * 2) * 12, 0),
    };

    let quad = |mesh: &mut UiMesh, cells: &CellBox, color: Color| {
        let corners = cells.corners().map(|(x, y)| params.transform.apply(x, y));
        add_quad(mesh, corners, color, feather);
    };

    // Shadows: towards the bottom of the screen, which is -x and -y in cells.
    if detailed {
        let (dx, dy) = (-style.shadow_offset_cells[0], -style.shadow_offset_cells[1]);
        let shadow = style.palette.shadow.color();
        for shape in &cache.features {
            let class = shape.feature.class;
            if matches!(class, MinimapClass::Plaza | MinimapClass::Field) && params.lens.is_none() {
                continue;
            }
            if shape.feature.state == FeatureState::UnderConstruction || !shape.footprint.overlaps(&params.visible) {
                continue;
            }
            quad(&mut meshes.shadows, &shape.footprint.inset(style.building_inset_cells).offset(dx, dy), shadow);
        }
    }

    // Fields: striped, with the stripes' UVs in cells so they tile.
    if params.lens.is_none() {
        let stripes = style.field_stripes_per_cell;
        for shape in &cache.features {
            if shape.feature.class != MinimapClass::Field || !shape.footprint.overlaps(&params.visible) {
                continue;
            }
            let cells = shape.footprint.inset(style.building_inset_cells * 0.5);
            let corners = cells.corners().map(|(x, y)| params.transform.apply(x, y));
            let uvs = cells.corners().map(|(x, y)| Vec2::new(x * stripes * 0.25, y * stripes));
            let color = field_color(shape);
            let mut indices = [0u32; 4];
            for k in 0..4 {
                indices[k] = meshes.fields.add_vertex(corners[k], uvs[k], color);
            }
            meshes.fields.add_triangle(indices[0], indices[1], indices[2]);
            meshes.fields.add_triangle(indices[0], indices[2], indices[3]);
        }
    }

    // Roads: a casing, then the road, as quads along each run.
    if side_px >= 0.75 {
        let min_width = 1.0 / params.pixels_per_point / params.transform.cell_side().max(1e-4);
        let road_width = style.road_width_cells.max(min_width);
        let mut road = |width: f32, color_of: &dyn Fn(&RoadShape) -> Color| {
            let half = width * 0.5;
            for run in &cache.roads {
                let cells = CellBox { x0: run.from.0 - half, y0: run.from.1 - half, x1: run.to.0 + half, y1: run.to.1 + half };
                if cells.overlaps(&params.visible) {
                    quad(&mut meshes.top, &cells, color_of(run));
                }
            }
        };
        if detailed {
            let casing = style.palette.road_casing.color();
            road(style.road_casing_cells.max(road_width), &|_| casing);
        }
        let (dirt, paved) = (style.palette.road.color(), style.palette.paved_road.color());
        road(road_width, &|run| if run.paved { paved } else { dirt });
    }

    // Buildings: an outline (a slightly larger quad underneath), then the block.
    let outline = style.palette.outline.color();
    for shape in &cache.features {
        let feature = &shape.feature;
        if !shape.footprint.overlaps(&params.visible) {
            continue;
        }
        if feature.class == MinimapClass::Field && params.lens.is_none() {
            continue;
        }

        let color = match params.lens {
            Some(lens) => lens(feature).unwrap_or(style.lenses.neutral.color()),
            None => shape.base_color,
        };
        let block = shape.footprint.inset(style.building_inset_cells);
        let outlined = detailed && feature.class != MinimapClass::Decor;

        match feature.state {
            FeatureState::UnderConstruction => {
                // A site: its outline and a faint fill.
                if detailed {
                    quad(&mut meshes.top, &block.inset(-style.outline_cells), outline);
                }
                quad(&mut meshes.top, &block, mix(style.palette.land_low.color(), color, 0.35));
            }
            state => {
                if outlined {
                    quad(&mut meshes.top, &block.inset(-style.outline_cells * 0.5), outline);
                }
                let color = if state == FeatureState::Renovating { mix(color, Color::white(), 0.35) } else { color };
                quad(&mut meshes.top, &block.inset(style.outline_cells * 0.5 * outlined as i32 as f32), color);
            }
        }
    }

    meshes
}

fn field_color(shape: &FeatureShape) -> Color {
    match shape.feature.state {
        FeatureState::UnderConstruction => mix(shape.base_color, Color::white(), 0.5),
        _ => shape.base_color,
    }
}

pub fn mix(a: Color, b: Color, t: f32) -> Color {
    Color::new(a.r + (b.r - a.r) * t, a.g + (b.g - a.g) * t, a.b + (b.b - a.b) * t, a.a + (b.a - a.a) * t)
}

// A convex quad with a feather ring `feather` points wide around it, fading to transparent, so
// its edges are anti-aliased like egui's own shapes.
pub fn add_quad(mesh: &mut UiMesh, corners: [Vec2; 4], color: Color, feather: f32) {
    // Outward miter offsets at each corner, whichever way the quad winds.
    let area = (0..4).map(|i| {
        let (a, b) = (corners[i], corners[(i + 1) % 4]);
        a.x * b.y - b.x * a.y
    }).sum::<f32>();
    let sign = if area >= 0.0 { 1.0 } else { -1.0 };

    let normal = |a: Vec2, b: Vec2| {
        let d = b - a;
        let length = d.length().max(1e-6);
        Vec2::new(d.y, -d.x) * (sign / length)
    };

    let half = feather * 0.5;
    let mut inner = [Vec2::zero(); 4];
    let mut outer = [Vec2::zero(); 4];
    for i in 0..4 {
        let prev = corners[(i + 3) % 4];
        let next = corners[(i + 1) % 4];
        let n1 = normal(prev, corners[i]);
        let n2 = normal(corners[i], next);
        let sum = n1 + n2;
        let miter = sum * (1.0 / (1.0 + n1.dot(n2)).max(0.25));
        inner[i] = corners[i] - miter * half;
        outer[i] = corners[i] + miter * half;
    }

    let clear = Color::new(color.r, color.g, color.b, 0.0);
    let uv = Vec2::zero();
    let i: [u32; 4] = inner.map(|p| mesh.add_vertex(p, uv, color));
    let o: [u32; 4] = outer.map(|p| mesh.add_vertex(p, uv, clear));

    mesh.add_triangle(i[0], i[1], i[2]);
    mesh.add_triangle(i[0], i[2], i[3]);
    for k in 0..4 {
        let next = (k + 1) % 4;
        mesh.add_triangle(i[k], o[k], o[next]);
        mesh.add_triangle(i[k], o[next], i[next]);
    }
}
