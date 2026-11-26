// The river's floods (docs/plans/seasons-floods.md, §3): the wet season's notice, the year's flood
// at its month, what it does to what it reaches, its months under water and the water going; and
// what the World Debug window and automation ask for. The state is sim::floods::CityFloods.
//
// The flood comes at the start of its month (on_new_month, after the finance's month-end and before
// the farmsteads' fields and faith's rolls): out from its sources' shores over the land, to a reach
// drawn from the profile's range each year (farther by a severe flood's severity). Each building it
// reaches takes collapse risk, a house disease risk too (M6's risks, scaled by the severity); a
// farmstead under water loses the harvest it holds and a field its crop; each cell takes silt. The
// cells are flagged TileFlags::Flooded, which the renderer draws as silty water and placement
// refuses, until the water goes at the start of the month after its last.

use common::{
    Color,
    coords::{Cell, CellRange},
    mem::SingleThreadStatic,
};
use engine::{
    log,
    ui::{
        sound::UiSoundKey,
        text::{self, UiTextCategory},
    },
};

use crate::{
    building::{BuildingKind, BuildingKindAndId, risks::RiskKind},
    config::GameConfigs,
    farms,
    messages::{Message, MessageKind, MessageTarget},
    sim::{
        SimContext,
        SimContextMut,
        calendar::{CalendarDate, Season},
        floods::{FloodProfile, FloodRecord, MAX_WEAR, is_dyke, reach_meeting},
    },
    tile::{
        TileFlags,
        TileKind,
        TileMapLayerKind,
        minimap::{MINIMAP_ICON_DEFAULT_LIFETIME, MinimapIcon},
        road,
        rubble,
        sets::{TERRAIN_LAND_CATEGORY, TileSets},
    },
    world::object::{Spawner, SpawnerResult},
};

// The minimap's ping over a building the flood damaged.
pub const FLOOD_ALERT_TINT: Color = Color::new(0.35, 0.6, 0.9, 1.0);

// ----------------------------------------------
// Notices
// ----------------------------------------------

// What the river did, for the message log (GameLoop::notify_floods).
#[derive(Clone, Debug)]
pub enum FloodNotice {
    // The wet season's first month: the river is rising.
    RiverRising,
    // The flood came: the cells under water, the buildings it damaged and the fields it covered;
    // `at`, the first building it damaged, else a cell under water, for the message's place.
    Flood { cells: u32, buildings_damaged: u32, fields_flooded: u32, at: Option<Cell> },
    // The water broke through dykes at that many cells, the first `at`.
    Breached { breaches: u32, at: Option<Cell> },
    // The sluice gates let the flood's silt onto that many fields behind the dykes, the first `at`.
    Sluiced { fields: u32, at: Option<Cell> },
    // The water went: the fields its silt reached.
    Receded { fields: u32 },
}

static NOTICES: SingleThreadStatic<Vec<FloodNotice>> = SingleThreadStatic::new(Vec::new());

pub fn take_notices() -> Vec<FloodNotice> {
    std::mem::take(NOTICES.as_mut())
}

fn notify(notice: FloodNotice) {
    NOTICES.as_mut().push(notice);
}

// The river the messages name: the profile's Realm text key ("the Cinnabar River"), else "the river".
pub fn river_name(profile: &FloodProfile) -> String {
    if profile.river.is_empty() {
        text::get(UiTextCategory::Messages, "floods.the_river").to_string()
    } else {
        text::get(UiTextCategory::Realm, &profile.river).to_string()
    }
}

// A notice's message: its kind, its sound and its text, naming `river`.
pub fn notice_message(notice: &FloodNotice, river: &str, sources: &[Cell]) -> (Message, UiSoundKey) {
    let river = sentence_start(river);
    let at = |cell: Option<Cell>| cell.map(MessageTarget::Cell);
    match *notice {
        FloodNotice::RiverRising => (
            Message::new(
                MessageKind::Season,
                text::format(UiTextCategory::Messages, "floods.river_rising", &[("river", &river)]),
            )
            .with_target(at(sources.first().copied())),
            UiSoundKey::Notification,
        ),
        FloodNotice::Flood { buildings_damaged, fields_flooded, at: cell, .. } => {
            let buildings =
                text::format_count(UiTextCategory::Messages, "floods.buildings_damaged", buildings_damaged as i64, &[]);
            let fields = text::format_count(UiTextCategory::Messages, "floods.fields", fields_flooded as i64, &[]);
            let text = text::format(UiTextCategory::Messages, "floods.flood", &[
                ("river", &river),
                ("buildings", &buildings),
                ("fields", &fields),
            ]);
            (Message::new(MessageKind::Warning, text).with_target(at(cell)), UiSoundKey::Alert)
        }
        FloodNotice::Breached { breaches, at: cell } => {
            let text =
                text::format_count(UiTextCategory::Messages, "floods.breached", breaches as i64, &[("river", &river)]);
            (Message::new(MessageKind::Warning, text).with_target(at(cell)), UiSoundKey::Alert)
        }
        FloodNotice::Sluiced { fields, at: cell } => {
            let text = text::format_count(UiTextCategory::Messages, "floods.sluiced", fields as i64, &[]);
            (Message::new(MessageKind::Season, text).with_target(at(cell)), UiSoundKey::Notification)
        }
        FloodNotice::Receded { fields } => {
            let text = if fields == 0 {
                text::get(UiTextCategory::Messages, "floods.receded_no_fields").to_string()
            } else {
                let fields = text::format_count(UiTextCategory::Messages, "floods.fields", fields as i64, &[]);
                text::format(UiTextCategory::Messages, "floods.receded", &[("fields", &fields)])
            };
            (Message::new(MessageKind::Season, text), UiSoundKey::Notification)
        }
    }
}

// "the river" as a sentence's first words: "The river".
fn sentence_start(words: &str) -> String {
    let mut chars = words.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

// ----------------------------------------------
// The month
// ----------------------------------------------

// The start of a month: the water under way goes once its months are over; the dykes wear; the wet
// season's first month says the river is rising; the flood's month brings the year's flood. Nothing
// while floods are off.
pub fn on_new_month(context: &SimContextMut, date: CalendarDate) {
    if !context.floods().is_enabled() {
        return;
    }
    if context.floods_mut().month_passed() {
        recede(context);
    }

    let rules = &GameConfigs::get().sim.floods;
    let floods = context.floods_mut();
    floods.sync_dykes(context.tile_map());
    floods.wear_dykes(rules.wear_per_month, rules.wear_jitter);

    let now = date.ordinal_month();
    let settings = context.calendar().settings();
    if settings.season_of(date.month) == Season::Wet && farms::season_start(settings, now) == now {
        notify(FloodNotice::RiverRising);
    }

    if date.month == context.floods().profile().month && context.floods().flood().is_none() {
        come(context, None, now);
    }
}

// A flood now, out to `reach` cells if given, else to a reach drawn from the profile's range
// (automation, the World Debug window). False while floods are off or a flood is under way.
pub fn flood_now(context: &SimContextMut, reach: Option<u32>) -> bool {
    let floods = context.floods();
    if !floods.is_enabled() || floods.flood().is_some() {
        return false;
    }
    come(context, reach, farms::current_month(context));
    true
}

// The water goes now, with its notice. False without a flood under way.
pub fn recede(context: &SimContextMut) -> bool {
    let Some(fields) = drain(context) else {
        return false;
    };
    log::info!(log::channel!("floods"), "The water goes down: the silt reached {fields} fields.");
    notify(FloodNotice::Receded { fields });
    true
}

// Turns floods on or off for the game (automation, the World Debug window): turning them off takes
// the water under way away first, quietly.
pub fn set_enabled(context: &SimContextMut, enabled: bool) {
    if !enabled {
        drain(context);
    }
    context.floods_mut().set_enabled(enabled, &GameConfigs::get().sim.floods, context.tile_map());
}

// Whether a cell is under water now: nothing new is placed there until the water goes.
#[inline]
pub fn is_flooded(context: &SimContext, cell: Cell) -> bool {
    context.floods().is_flooded(cell)
}

// The ordinal month the water under way goes in, if there is any.
pub fn recedes_in(context: &SimContext) -> Option<u32> {
    let flood = context.floods().flood()?;
    Some(farms::current_month(context) + flood.months_left)
}

// The flood comes: its reach, the damage, the fields, the silt, the flags and its record.
fn come(context: &SimContextMut, reach: Option<u32>, ordinal_month: u32) {
    let rules = &GameConfigs::get().sim.floods;
    let severity = context.floods_mut().take_next_severity();
    let [min, max] = context.floods().profile().reach;
    let drawn = reach.unwrap_or_else(|| context.random_range(min..=max));
    let reach = if severity > 1.0 { (drawn as f32 * severity).round() as u32 } else { drawn };

    // Out over the land, stopped by the dykes; the dykes it reaches may breach, and the water goes
    // on through them.
    context.floods_mut().sync_dykes(context.tile_map());
    let sources = context.floods().profile().sources.clone();
    let level = context.floods().profile().flood_level;
    let breached = breach_dykes(context, &sources, reach, level, severity);
    let tile_map = context.tile_map();
    let (reached, met) = reach_meeting(tile_map, &sources, reach, level, |cell| is_dyke(tile_map, cell));
    let cells: Vec<Cell> = reached.into_iter().map(|(cell, _)| cell).collect();

    let size = context.tile_map().size_in_cells();
    let (width, height) = (size.width.max(0), size.height.max(0));
    let mut under_water = vec![false; (width * height) as usize];
    for cell in &cells {
        under_water[(cell.y * width + cell.x) as usize] = true;
    }
    let is_under_water = |cell: Cell| {
        cell.x >= 0 && cell.y >= 0 && cell.x < width && cell.y < height && under_water[(cell.y * width + cell.x) as usize]
    };

    // The buildings it reaches, in the world's order.
    let mut reached: Vec<BuildingKindAndId> = Vec::new();
    context.world().for_each_building_of_any_kind(|building| {
        if building.cell_range().iter().any(is_under_water) {
            reached.push(building.kind_and_id());
        }
        true
    });

    let (risks_on, disease_on) = (context.risks().is_active(), context.risks().is_disease_active());
    let mut buildings_damaged = 0;
    let mut first_damaged = None;
    for id in reached {
        let Some(building) = context.find_building_mut(id.kind, id.id) else {
            continue;
        };
        // A farmstead under water loses the harvest it holds (a runner already carrying a load
        // delivers it).
        if farms::crop_of(building).is_some() {
            building.as_producer_mut().lose_harvest();
        }
        let mut damaged = false;
        if risks_on {
            damaged |= building.add_risk(RiskKind::Collapse, rules.collapse_damage * severity);
        }
        if disease_on && building.is(BuildingKind::House) {
            damaged |= building.add_risk(RiskKind::Disease, rules.disease_damage * severity);
        }
        if damaged {
            buildings_damaged += 1;
            let cell = building.base_cell();
            first_damaged.get_or_insert(cell);
            context.minimap_mut().push_icon(MinimapIcon::Alert, cell, FLOOD_ALERT_TINT, MINIMAP_ICON_DEFAULT_LIFETIME);
        }
    }

    let fields_flooded = farms::flood_fields(context, is_under_water);

    // The sluice gates the water stands against let its silt onto the fields they water, without
    // the water.
    let sluiced = farms::sluiced_fields(context, &met, is_under_water);
    context.floods_mut().add_silt(&sluiced, rules.silt);

    // The land under water, and the river's own shore cells beside it, so their banks are drawn
    // under the flood too.
    let tile_map = context.tile_map_mut();
    for &cell in &cells {
        tile_map.set_tile_flags(cell, TileKind::Terrain, TileFlags::Flooded, true);
        for next in neighbours(cell) {
            if !is_under_water(next) && is_water(tile_map, next) {
                tile_map.set_tile_flags(next, TileKind::Terrain, TileFlags::Flooded, true);
            }
        }
    }

    let record = FloodRecord {
        ordinal_month,
        reach,
        severity,
        cells: cells.len() as u32,
        buildings_damaged,
        fields_flooded,
        breaches: breached.len() as u32,
    };
    let cell_count = record.cells;
    let at = first_damaged.or_else(|| cells.get(cells.len() / 2).copied());
    context.floods_mut().begin_flood(cells, severity, rules.silt, record);

    log::info!(
        log::channel!("floods"),
        "The river floods: {cell_count} cells to a reach of {reach} (severity {severity:.2}), {buildings_damaged} \
         buildings damaged, {fields_flooded} fields under water."
    );
    notify(FloodNotice::Flood { cells: cell_count, buildings_damaged, fields_flooded, at });
    if !breached.is_empty() {
        notify(FloodNotice::Breached { breaches: breached.len() as u32, at: breached.first().copied() });
    }
    if !sluiced.is_empty() {
        log::info!(log::channel!("floods"), "The sluice gates let the silt onto {} fields.", sluiced.len());
        notify(FloodNotice::Sluiced { fields: sluiced.len() as u32, at: sluiced.first().copied() });
    }
}

// The dykes the water reaches, each rolled once, in row order: one breaches with chance (wear /
// 100)² × the flood's severity, from the simulation's RNG (a sound dyke draws nothing). Water
// through a breach may reach more dykes, rolled in turn. Each breach turns its cell to dirt under a
// pile of rubble (the flood goes on through it); the cells breached.
fn breach_dykes(context: &SimContextMut, sources: &[Cell], reach: u32, level: u8, severity: f32) -> Vec<Cell> {
    let mut rolled: Vec<Cell> = Vec::new();
    let mut breached: Vec<Cell> = Vec::new();
    loop {
        let tile_map = context.tile_map();
        let (_, mut met) =
            reach_meeting(tile_map, sources, reach, level, |cell| is_dyke(tile_map, cell) && !breached.contains(&cell));
        met.retain(|cell| !rolled.contains(cell));
        if met.is_empty() {
            break;
        }
        met.sort_by_key(|cell| (cell.y, cell.x));
        let mut any = false;
        for cell in met {
            rolled.push(cell);
            let chance = (context.floods().wear_at(cell) / MAX_WEAR).powi(2) * severity;
            if chance > 0.0 && context.random_range(0.0..1.0_f32) < chance {
                breached.push(cell);
                any = true;
            }
        }
        if !any {
            break;
        }
    }

    for &cell in &breached {
        breach(context, cell);
    }
    breached
}

// A dyke cell gives way: dirt under a pile of rubble, its record gone, its neighbours' joins
// redrawn, and a ping on the minimap. The player clears the rubble and draws the dyke again.
fn breach(context: &SimContextMut, cell: Cell) {
    context.floods_mut().remove_dyke(cell);
    if let Some(dirt) =
        TileSets::get().find_tile_def_by_name(TileMapLayerKind::Terrain, TERRAIN_LAND_CATEGORY.string, "dirt")
    {
        // As the clear tool takes a road away, then the dirt in its place.
        let spawner = Spawner::new(context);
        if let Some(tile) = context.tile_map().try_tile_from_layer(cell, TileMapLayerKind::Terrain) {
            spawner.despawn_tile(tile);
        }
        if let SpawnerResult::Err(err) = spawner.try_spawn_tile_with_def(cell, dirt) {
            log::warning!(log::channel!("floods"), "No dirt for the breach at {cell}: {}", err.message);
        }
        road::update_junctions(context.tile_map_mut(), cell);
    }
    rubble::place_pile(context, CellRange::new(cell, cell));
    context.minimap_mut().push_icon(MinimapIcon::Alert, cell, FLOOD_ALERT_TINT, MINIMAP_ICON_DEFAULT_LIFETIME);
    log::info!(log::channel!("floods"), "The river broke through the dyke at {cell}.");
}

// The water goes without a notice: the cells are dry land again. The fields under it, or None
// without a flood under way.
fn drain(context: &SimContextMut) -> Option<u32> {
    let flood = context.floods_mut().end_flood()?;
    let tile_map = context.tile_map_mut();
    let mut fields = 0;
    for &cell in &flood.cells {
        tile_map.set_tile_flags(cell, TileKind::Terrain, TileFlags::Flooded, false);
        for next in neighbours(cell) {
            tile_map.set_tile_flags(next, TileKind::Terrain, TileFlags::Flooded, false);
        }
        if tile_map.try_tile_from_layer(cell, TileMapLayerKind::Objects).is_some_and(|tile| tile.is(TileKind::Field)) {
            fields += 1;
        }
    }
    Some(fields)
}

#[inline]
fn neighbours(cell: Cell) -> [Cell; 4] {
    [
        Cell::new(cell.x + 1, cell.y),
        Cell::new(cell.x - 1, cell.y),
        Cell::new(cell.x, cell.y + 1),
        Cell::new(cell.x, cell.y - 1),
    ]
}

#[inline]
fn is_water(tile_map: &crate::tile::TileMap, cell: Cell) -> bool {
    tile_map.try_tile_from_layer(cell, TileMapLayerKind::Terrain).is_some_and(|tile| tile.path_kind().is_water())
}

// ----------------------------------------------
// Requests
// ----------------------------------------------

// What the World Debug window's Floods tab asks for, run at the start of the next frame
// (Simulation::update): the window draws while the world is borrowed.
#[derive(Copy, Clone)]
pub enum Request {
    SetEnabled(bool),
    FloodNow,
    Recede,
}

static REQUESTS: SingleThreadStatic<Vec<Request>> = SingleThreadStatic::new(Vec::new());

pub fn request(request: Request) {
    REQUESTS.as_mut().push(request);
}

#[inline]
pub fn has_requests() -> bool {
    !REQUESTS.as_ref().is_empty()
}

pub fn run_requests(context: &SimContextMut) {
    for request in std::mem::take(REQUESTS.as_mut()) {
        match request {
            Request::SetEnabled(enabled) => set_enabled(context, enabled),
            Request::FloodNow => {
                flood_now(context, None);
            }
            Request::Recede => {
                recede(context);
            }
        }
    }
}
