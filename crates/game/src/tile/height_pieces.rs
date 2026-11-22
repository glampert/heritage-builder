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
    mem::SingleThreadStatic,
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

type Piece = Option<(&'static TileDef, usize)>;

// Every piece, looked up by name once (the renderer asks for them every frame): by edge, the grass
// tops, the mid courses and the dirt top; the rims by side; the ramps by surface, direction and half.
struct Pieces {
    tops: [[Piece; 2]; 2],
    mids: [[Piece; 2]; 2],
    dirt_tops: [Piece; 2],
    rims: [Piece; 4],
    ramps: [[[Piece; 2]; 4]; 4],
}

impl Pieces {
    fn look_up() -> Self {
        let edge_pieces = |names: [&str; 2]| FaceEdge::ALL.map(|edge| names.map(|name| piece(edge.tile_name(), name)));
        Self {
            tops: edge_pieces(TOP_COURSES),
            mids: edge_pieces(MID_COURSES),
            dirt_tops: FaceEdge::ALL.map(|edge| piece(edge.tile_name(), DIRT_TOP_COURSE)),
            rims: Dir::ALL.map(|side| piece("cliff_rim", side.letter())),
            ramps: RampSurface::ALL.map(|surface| {
                Dir::ALL
                    .map(|dir| ["low", "high"].map(|half| piece(surface.tile_name(), &format!("{}_{half}", dir.letter()))))
            }),
        }
    }
}

static PIECES: SingleThreadStatic<Option<Pieces>> = SingleThreadStatic::new(None);

fn pieces() -> &'static Pieces {
    if PIECES.as_ref().is_none() {
        PIECES.set(Some(Pieces::look_up()));
    }
    PIECES.as_ref().as_ref().expect("looked up above")
}

const fn edge_index(edge: FaceEdge) -> usize {
    match edge {
        FaceEdge::Left => 0,
        FaceEdge::Right => 1,
    }
}

const fn dir_index(dir: Dir) -> usize {
    match dir {
        Dir::N => 0,
        Dir::E => 1,
        Dir::S => 2,
        Dir::W => 3,
    }
}

const fn surface_index(surface: RampSurface) -> usize {
    match surface {
        RampSurface::Grass => 0,
        RampSurface::Dirt => 1,
        RampSurface::DirtRoad => 2,
        RampSurface::PavedRoad => 3,
    }
}

// The course of a cliff on `edge` of `cell` whose foot is at `level`: its tile def and variation.
pub fn cliff_course(edge: FaceEdge, course: Course, cell: Cell, level: u8) -> Piece {
    let pick = (splitmix64(((cell.x as u32 as u64) << 32) ^ (cell.y as u32 as u64) ^ ((level as u64) << 56)) & 1) as usize;
    let pieces = pieces();
    let edge = edge_index(edge);
    match course {
        Course::Mid => pieces.mids[edge][pick],
        Course::Top { dirt_lip: false } => pieces.tops[edge][pick],
        Course::Top { dirt_lip: true } => pieces.dirt_tops[edge],
    }
}

// The rim along `side` of a raised cell's top.
pub fn rim(side: Dir) -> Piece {
    pieces().rims[dir_index(side)]
}

// Half of a ramp climbing towards `dir`, its top `surface`.
pub fn ramp(surface: RampSurface, dir: Dir, half: Half) -> Piece {
    pieces().ramps[surface_index(surface)][dir_index(dir)][usize::from(half == Half::High)]
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
