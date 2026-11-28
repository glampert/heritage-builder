// The city's defence (docs/plans/defence.md): its walls and gatehouses now; raids, towers and
// defenders with the plan's later phases. The state is sim::defence::CityDefence.
//
// Walls (Phase 1, tile::wall): pieces drawn with the road tool in three tiers, each with a
// condition up to its tier's strength (DefenceRules), kept in CityDefence in step with the map
// (walls_changed, after every edit, and each month). A piece below `damaged_below` of its strength
// shows its cracks (TileFlags::WallDamaged); at 0 it breaches: the piece goes, rubble takes its
// cell, the pieces beside it close up, and the message log hears of it. A builders' guild walker
// passing beside a damaged piece mends it by `repair_share` of its strength, once a month at most,
// but not while a raid is under way.

use common::{
    Color,
    coords::{Cell, CellRange},
};
use engine::{
    log,
    ui::{
        sound::UiSoundKey,
        text::{self, UiTextCategory},
    },
};

use crate::{
    config::GameConfigs,
    farms,
    messages::{Message, MessageKind, MessageTarget},
    sim::{
        SimContextMut,
        calendar::CalendarDate,
        defence::{DefenceNotice, WallTier},
    },
    tile::{
        TileFlags,
        TileKind,
        TileMap,
        TileMapLayerKind,
        minimap::{MINIMAP_ICON_DEFAULT_LIFETIME, MinimapIcon},
        rubble,
        wall,
    },
    world::object::Spawner,
};

// The minimap's ping over a breach.
pub const BREACH_ALERT_TINT: Color = Color::new(0.85, 0.45, 0.2, 1.0);

// ----------------------------------------------
// Walls
// ----------------------------------------------

// The walls on the map changed (a piece drawn, cleared or breached, a gatehouse placed): their
// records follow, and their cracks are drawn again.
pub fn walls_changed(context: &SimContextMut) {
    let rules = &GameConfigs::get().sim.defence;
    context.defence_mut().sync_walls(wall::wall_cells(context.tile_map()), rules);
    refresh_wall_flags(context);
}

// Sets TileFlags::WallDamaged on each piece below its share of its strength, and clears it on the
// others.
pub fn refresh_wall_flags(context: &SimContextMut) {
    let rules = &GameConfigs::get().sim.defence;
    let walls: Vec<(Cell, bool)> =
        context.defence().walls().iter().map(|wall| (wall.cell, wall.is_damaged(rules))).collect();
    let tile_map = context.tile_map_mut();
    for (cell, damaged) in walls {
        set_damaged_flag(tile_map, cell, damaged);
    }
}

fn set_damaged_flag(tile_map: &mut TileMap, cell: Cell, damaged: bool) {
    let has_flag = tile_map
        .try_tile_from_layer(cell, TileMapLayerKind::Objects)
        .is_some_and(|tile| tile.has_flags(TileFlags::WallDamaged));
    if has_flag != damaged {
        tile_map.set_tile_flags(cell, TileKind::Wall, TileFlags::WallDamaged, damaged);
    }
}

// Each month the records are matched to the map again (a piece placed some other way: the
// developer editor, a preset).
pub fn on_new_month(context: &SimContextMut, _date: CalendarDate) {
    if !context.defence().walls().is_empty() || !wall::wall_cells(context.tile_map()).is_empty() {
        walls_changed(context);
    }
}

// Lowers the piece at `cell` by `amount`: Some(true) if it breached, Some(false) if it stands,
// None where there's no wall.
pub fn damage_wall(context: &SimContextMut, cell: Cell, amount: f32) -> Option<bool> {
    let left = context.defence_mut().damage_wall(cell, amount)?;
    if left <= 0.0 {
        breach(context, cell);
        return Some(true);
    }
    let rules = &GameConfigs::get().sim.defence;
    let damaged = context.defence().wall_at(cell).is_some_and(|wall| wall.is_damaged(rules));
    set_damaged_flag(context.tile_map_mut(), cell, damaged);
    Some(false)
}

// Sets the piece's condition (automation, the developer tools); 0 breaches it. False where there's
// no wall.
pub fn set_wall_condition(context: &SimContextMut, cell: Cell, condition: f32) -> bool {
    let rules = &GameConfigs::get().sim.defence;
    if !context.defence_mut().set_wall_condition(cell, condition, rules) {
        return false;
    }
    if condition <= 0.0 {
        breach(context, cell);
    } else {
        let damaged = context.defence().wall_at(cell).is_some_and(|wall| wall.is_damaged(rules));
        set_damaged_flag(context.tile_map_mut(), cell, damaged);
    }
    true
}

// A piece gives way: it goes (nothing refunded), rubble takes its cell, the pieces beside it close
// up, its record goes with a notice for the log, and the minimap pings. The player clears the
// rubble and draws the wall again.
pub fn breach(context: &SimContextMut, cell: Cell) {
    let Some(tier) = wall::tier_at(context.tile_map(), cell) else {
        return;
    };
    if let Some(tile) = context.tile_map().try_tile_from_layer(cell, TileMapLayerKind::Objects) {
        Spawner::new(context).despawn_tile(tile);
    }
    context.defence_mut().remove_wall(cell);
    wall::update_variations(context.tile_map_mut(), cell);
    rubble::place_pile(context, CellRange::new(cell, cell));
    context.minimap_mut().push_icon(MinimapIcon::Alert, cell, BREACH_ALERT_TINT, MINIMAP_ICON_DEFAULT_LIFETIME);
    log::info!(log::channel!("defence"), "The {} gave way at {cell}.", tier.name());
}

// A builders' guild walker at `cell` mends the damaged pieces beside it (four neighbours), unless a
// raid is under way. Runs as a deferred update (the world step reads the state only).
pub fn builder_passed(context: &SimContextMut, cell: Cell) {
    if !context.defence().under_way().is_empty() {
        return;
    }
    let rules = &GameConfigs::get().sim.defence;
    let now = farms::current_month(context);
    for neighbour in neighbours(cell) {
        let Some(wall) = context.defence().wall_at(neighbour) else {
            continue;
        };
        if !wall.is_damaged(rules) || !context.tile_map().elevation().joins(cell, neighbour) {
            continue;
        }
        if context.defence_mut().repair_wall(neighbour, now, rules) {
            let damaged = context.defence().wall_at(neighbour).is_some_and(|wall| wall.is_damaged(rules));
            set_damaged_flag(context.tile_map_mut(), neighbour, damaged);
        }
    }
}

// Whether a builders' guild walker at `cell` has a damaged piece beside it to mend (read in the
// world step, before queuing builder_passed).
pub fn has_damaged_wall_beside(context: &crate::sim::SimContext, cell: Cell) -> bool {
    let rules = &GameConfigs::get().sim.defence;
    let defence = context.defence();
    if defence.walls().is_empty() || !defence.under_way().is_empty() {
        return false;
    }
    neighbours(cell).into_iter().any(|neighbour| defence.wall_at(neighbour).is_some_and(|wall| wall.is_damaged(rules)))
}

fn neighbours(cell: Cell) -> [Cell; 4] {
    [
        Cell::new(cell.x + 1, cell.y),
        Cell::new(cell.x - 1, cell.y),
        Cell::new(cell.x, cell.y + 1),
        Cell::new(cell.x, cell.y - 1),
    ]
}

// ----------------------------------------------
// Notices
// ----------------------------------------------

// A notice's message and sound (GameLoop::notify_defence).
pub fn notice_message(notice: &DefenceNotice) -> (Message, UiSoundKey) {
    match *notice {
        DefenceNotice::Breach { cell, tier } => {
            let text = match tier {
                WallTier::Palisade => text::get(UiTextCategory::Messages, "defence.breach.palisade"),
                WallTier::Rampart => text::get(UiTextCategory::Messages, "defence.breach.rampart"),
                WallTier::CityWall => text::get(UiTextCategory::Messages, "defence.breach.city_wall"),
            };
            (
                Message::new(MessageKind::Warning, text.to_string()).with_target(Some(MessageTarget::Cell(cell))),
                UiSoundKey::Alert,
            )
        }
    }
}
