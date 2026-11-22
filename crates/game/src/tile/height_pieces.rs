// The pieces the renderer draws for raised terrain (docs/plans/terrain-height.md, §5), from the
// tile generator (crates/tools/blender-tile-generator/catalog/heights.json): sprites in the objects
// tile set's overlays category, never tiles on the map, looked up by name. Each is anchored as a
// 1 × 1 building on its cell and lifted to its level.
//
// - A cliff course: one level of rock face on a cell's south (left on screen) or east (right) edge,
//   stacked from the lower ground up, the top one capped by a lip of the cell's terrain.
// - A rim: the inked lip along one side of a raised cell's top where the ground drops.
// - Half a ramp: a ramp climbs one level over two cells (tile::elevation), its top the terrain's.

use common::{
    coords::Cell,
    hash::{fnv1a_from_str, splitmix64},
};

use super::{
    TileMapLayerKind,
    elevation::{Dir, Half},
    sets::{OBJECTS_OVERLAYS_CATEGORY, TileDef, TileSets},
};

// A cliff face's edge: the cell's two front edges.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum FaceEdge {
    Left,  // The south side (-x), the lower-left edge on screen.
    Right, // The east side (-y), the lower-right edge.
}

impl FaceEdge {
    pub const ALL: [FaceEdge; 2] = [FaceEdge::Left, FaceEdge::Right];

    const fn tile_name(self) -> &'static str {
        match self {
            FaceEdge::Left => "cliff_left",
            FaceEdge::Right => "cliff_right",
        }
    }

    // The side of the cell it's on.
    pub const fn side(self) -> Dir {
        match self {
            FaceEdge::Left => Dir::S,
            FaceEdge::Right => Dir::E,
        }
    }
}

// Which course of a cliff: the levels under the top one are `Mid`; the top one carries the lip,
// dirt where the cell's terrain is bare ground.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Course {
    Mid,
    Top { dirt_lip: bool },
}

// What a ramp's top is made of: the terrain of its cell.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum RampSurface {
    Grass,
    Dirt,
    DirtRoad,
    PavedRoad,
}

impl RampSurface {
    pub const ALL: [RampSurface; 4] = [RampSurface::Grass, RampSurface::Dirt, RampSurface::DirtRoad, RampSurface::PavedRoad];

    const fn tile_name(self) -> &'static str {
        match self {
            RampSurface::Grass => "ramp_grass",
            RampSurface::Dirt => "ramp_dirt",
            RampSurface::DirtRoad => "ramp_dirt_road",
            RampSurface::PavedRoad => "ramp_paved_road",
        }
    }

    // A terrain tile's ramp surface: its roads' and bare dirt's own, grass for everything else.
    pub fn of_terrain(terrain: &TileDef) -> RampSurface {
        match terrain.name.as_str() {
            "dirt_road" => RampSurface::DirtRoad,
            "paved_road" => RampSurface::PavedRoad,
            "dirt" => RampSurface::Dirt,
            _ => RampSurface::Grass,
        }
    }
}

// Whether a terrain tile's cliffs get a dirt lip rather than grass.
pub fn has_dirt_lip(terrain: &TileDef) -> bool {
    matches!(terrain.name.as_str(), "dirt" | "dirt_road" | "paved_road")
}

// A course's variation names: a top and a mid course in two looks, picked by a hash of the cell and
// level so the same cell always draws the same; one dirt-lipped top.
const TOP_COURSES: [&str; 2] = ["top_a", "top_b"];
const MID_COURSES: [&str; 2] = ["mid_a", "mid_b"];
const DIRT_TOP_COURSE: &str = "top_dirt";

fn overlay_def(name: &str) -> Option<&'static TileDef> {
    TileSets::get().try_find_tile_def_by_hash(
        TileMapLayerKind::Objects,
        OBJECTS_OVERLAYS_CATEGORY.hash,
        fnv1a_from_str(name),
    )
}

fn piece(tile: &str, variation: &str) -> Option<(&'static TileDef, usize)> {
    let def = overlay_def(tile)?;
    def.find_variation(variation).map(|index| (def, index))
}

// The course of a cliff on `edge` of `cell` whose foot is at `level`: its tile def and variation.
pub fn cliff_course(edge: FaceEdge, course: Course, cell: Cell, level: u8) -> Option<(&'static TileDef, usize)> {
    let pick = (splitmix64(((cell.x as u32 as u64) << 32) ^ (cell.y as u32 as u64) ^ ((level as u64) << 56)) & 1) as usize;
    let variation = match course {
        Course::Mid => MID_COURSES[pick],
        Course::Top { dirt_lip: false } => TOP_COURSES[pick],
        Course::Top { dirt_lip: true } => DIRT_TOP_COURSE,
    };
    piece(edge.tile_name(), variation)
}

// The rim along `side` of a raised cell's top.
pub fn rim(side: Dir) -> Option<(&'static TileDef, usize)> {
    piece("cliff_rim", side.letter())
}

// Half of a ramp climbing towards `dir`, its top `surface`.
pub fn ramp(surface: RampSurface, dir: Dir, half: Half) -> Option<(&'static TileDef, usize)> {
    let half = match half {
        Half::Low => "low",
        Half::High => "high",
    };
    piece(surface.tile_name(), &format!("{}_{half}", dir.letter()))
}

// Every piece's name, as (tile, variation): what the tile sets must hold for raised terrain to draw.
pub fn all_piece_names() -> Vec<(&'static str, String)> {
    let mut names = Vec::new();
    for edge in FaceEdge::ALL {
        for variation in TOP_COURSES.iter().chain(&MID_COURSES).chain(std::iter::once(&DIRT_TOP_COURSE)) {
            names.push((edge.tile_name(), variation.to_string()));
        }
    }
    for side in Dir::ALL {
        names.push(("cliff_rim", side.letter().to_string()));
    }
    for surface in RampSurface::ALL {
        for dir in Dir::ALL {
            for half in ["low", "high"] {
                names.push((surface.tile_name(), format!("{}_{half}", dir.letter())));
            }
        }
    }
    names
}

// Whether the tile sets hold a piece by name.
pub fn has_piece(tile: &str, variation: &str) -> bool {
    piece(tile, variation).is_some()
}
