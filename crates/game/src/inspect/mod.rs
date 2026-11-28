// What the inspector shows of the thing clicked (docs/plans/realm-ui.md, Phase 8): a report read
// from the simulation when the panel is built, as `inspector-<kind>-<realm>.jpg` lays it out. A
// header (an icon, a name, its kind and size), a status line, then sections drawn top to bottom,
// and the panel's actions. The panel (menu/in_game/inspector) paints it; automation's
// `inspect.at` reports it, so scenarios check the numbers the player reads.
//
// building.rs reads buildings (a panel per archetype, and construction sites), unit.rs walkers,
// tile.rs props and terrain.

mod building;
mod tile;
mod unit;

pub use building::{building_name, dock_partner};
use common::coords::Cell;
use engine::ui::text::{self, UiTextCategory};
use serde::Serialize;
pub use unit::home_cell as unit_home_cell;

use crate::{
    building::{BuildingKind, BuildingKindAndId},
    config::GameConfigs,
    realm::Realm,
    sim::{SimContext, resources::ResourceKind},
    tile::{Tile, TileKind},
    unit::UnitId,
};

// ----------------------------------------------
// The report
// ----------------------------------------------

// What was clicked, to read it again while the panel is open (a walker moves off its cell).
#[derive(Copy, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InspectTarget {
    Building {
        #[serde(serialize_with = "serialize_building_kind")]
        building: BuildingKind,
        #[serde(skip)]
        handle: BuildingKindAndId,
        cell: Cell,
    },
    Unit {
        #[serde(skip)]
        id: UnitId,
        cell: Cell,
    },
    Prop {
        cell: Cell,
    },
    Terrain {
        cell: Cell,
    },
}

impl InspectTarget {
    pub fn cell(&self) -> Cell {
        match *self {
            Self::Building { cell, .. } | Self::Unit { cell, .. } | Self::Prop { cell } | Self::Terrain { cell } => cell,
        }
    }
}

fn serialize_building_kind<S: serde::Serializer>(kind: &BuildingKind, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&kind.to_string())
}

fn serialize_resource_kind<S: serde::Serializer>(kind: &ResourceKind, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(kind.name())
}

// Which panel's layout: the mockups' kinds.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PanelKind {
    Workshop,
    House,
    Storage,
    Service,
    Market,
    Site,
    Unit,
    Prop,
    Terrain,
}

// The header's icon: a building's palette glyph, a sector's (a house: housing), or the tile's
// own sprite (walkers, props, terrain).
#[derive(Clone, PartialEq, Debug, Serialize)]
#[serde(tag = "kind", content = "name", rename_all = "snake_case")]
pub enum InspectIcon {
    Building(String),
    Sector(String),
    Sprite,
}

// The status line's dot.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Tone {
    Good,
    Pending,
    Bad,
}

#[derive(Clone, PartialEq, Debug, Serialize)]
pub struct Status {
    pub word: String,
    pub tone: Tone,
    pub summary: String,
}

// A good's order in a store: refused, accepted, fetched (Get) or carried out (Empty).
#[derive(Copy, Clone, PartialEq, Eq, Default, Debug, Serialize, serde::Deserialize, strum::EnumIter)]
#[serde(rename_all = "snake_case")]
pub enum StorageOrder {
    Refuse,
    #[default]
    Accept,
    Get,
    Empty,
}

impl StorageOrder {
    pub const ALL: [Self; 4] = [Self::Refuse, Self::Accept, Self::Get, Self::Empty];

    pub fn key(self) -> &'static str {
        match self {
            Self::Refuse => "refuse",
            Self::Accept => "accept",
            Self::Get => "get",
            Self::Empty => "empty",
        }
    }

    // Its word in the store's rows ("Accept").
    pub fn label(self) -> String {
        t(&format!("order.{}", self.key()))
    }

    // Whether the store takes in more of the good. Empty takes in none, and its runner carries out
    // what it holds (building::storage).
    pub fn takes_in(self) -> bool {
        matches!(self, Self::Accept | Self::Get)
    }
}

#[derive(Clone, PartialEq, Debug, Serialize)]
pub struct ProductionRow {
    pub label: String, // "Needs", "Makes".
    #[serde(serialize_with = "serialize_resource_kind")]
    pub kind: ResourceKind,
    pub good: String,
    pub have: u32,
    pub capacity: u32,
}

#[derive(Clone, PartialEq, Debug, Serialize)]
pub struct StockRow {
    #[serde(serialize_with = "serialize_resource_kind")]
    pub kind: ResourceKind,
    pub good: String,
    pub have: u32,
    pub capacity: u32,
}

#[derive(Clone, PartialEq, Debug, Serialize)]
pub struct StorageRow {
    #[serde(serialize_with = "serialize_resource_kind")]
    pub kind: ResourceKind,
    pub good: String,
    pub have: u32,
    pub room: u32,
    pub order: StorageOrder,
}

#[derive(Clone, PartialEq, Debug, Serialize)]
pub struct TickRow {
    pub name: String,
    pub met: bool,
}

#[derive(Clone, PartialEq, Debug, Serialize)]
pub struct ValueRow {
    pub label: String,
    pub value: String,
    pub warn: bool, // In the realm's red.
}

#[derive(Clone, PartialEq, Debug, Serialize)]
pub struct RiskRow {
    pub label: String,
    pub value: f32, // 0 to 100.
    pub level: String,
    pub high: bool,
}

// What the errand line's icon shows.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrandIcon {
    Cart,
    Walker,
}

// A part of the panel, drawn top to bottom as the mockups' sections are
// (docs/design/mockups/generator/dialogs_content.py, INSPECTORS).
#[derive(Clone, PartialEq, Debug, Serialize)]
#[serde(tag = "section", rename_all = "snake_case")]
pub enum Section {
    // Figures per post, empty ones faded, and a note; `compact` on one line, without figures.
    Workers { have: u32, need: u32, note: String, short: bool, compact: bool },
    // The same for a house's people.
    Residents { have: u32, room: u32, note: String },
    // Inputs and the output against their room, and the next batch.
    Production { note: String, rows: Vec<ProductionRow>, progress: f32 },
    // Goods held as meters; `warn_empty` marks a good at 0 in red (a house's, a market's).
    Stock { title: String, note: Option<String>, rows: Vec<StockRow>, footer: Option<String>, warn_empty: bool },
    // A store's goods with their orders, and Accept all and Accept none under them.
    Storage { title: String, note: String, rows: Vec<StorageRow>, footer: String },
    // Needs met or not, in two columns; the footer a missing one, with a cross.
    Ticks { title: String, note: Option<String>, rows: Vec<TickRow>, footer: Option<String> },
    // Labels and values.
    Values { title: String, note: Option<String>, rows: Vec<ValueRow> },
    // One meter and its percentage.
    Progress { title: String, note: String, value: f32 },
    // What a walker is doing, under a rule.
    Errand { icon: ErrandIcon, text: String },
    // Fire, collapse and disease risk: a meter and a level each.
    Risks { rows: Vec<RiskRow> },
    // Costs and risks side by side (the wide storage yard's panel).
    CostsRisks { costs: Vec<ValueRow>, risks: Vec<RiskRow> },
    // A short paragraph.
    Text { text: String },
}

// What the panel's buttons do.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    PauseWork,
    Demolish,
    CancelSite,
    ShowHome,
    Clear,
    TradeLimits, // A dock's: the trade limits dialog for its routes.
}

#[derive(Clone, PartialEq, Debug, Serialize)]
pub struct Action {
    pub kind: ActionKind,
    pub label: String,
    pub enabled: bool,
    pub tooltip: Option<String>,
}

impl Action {
    fn new(kind: ActionKind, key: &str) -> Self {
        Self { kind, label: t(key), enabled: true, tooltip: None }
    }

    fn disabled(mut self, tooltip_key: &str) -> Self {
        self.enabled = false;
        self.tooltip = Some(t(tooltip_key));
        self
    }
}

#[derive(Clone, PartialEq, Serialize)]
pub struct InspectReport {
    pub target: InspectTarget,
    pub panel: PanelKind,
    pub icon: InspectIcon,
    pub name: String,
    pub subtitle: String,
    pub status: Option<Status>,
    pub sections: Vec<Section>,
    pub actions: Vec<Action>,
    pub wide: bool, // The wide panel (a store with many goods).
}

// ----------------------------------------------
// Reading
// ----------------------------------------------

// The report for what's on top at `tile` (as the inspector picks it: a walker, a building, a
// harvestable prop, else the terrain), in `realm`'s words.
pub fn inspect_tile(context: &SimContext, tile: &Tile, realm: Realm) -> Option<InspectReport> {
    let world = context.world();
    // A building's or prop's other cells hold blockers: inspect what owns them.
    if tile.is(TileKind::Blocker) {
        let owner = context.tile_map().find_tile(
            tile.base_cell(),
            TileKind::Building | TileKind::Prop | TileKind::Vegetation | TileKind::Rocks | TileKind::Field,
        )?;
        return inspect_tile(context, owner, realm);
    }
    if tile.is(TileKind::Unit) {
        let unit = world.find_unit_for_tile(tile)?;
        return unit::read(context, unit, realm);
    }
    if tile.is(TileKind::Building) {
        // Any of its cells: a tile off its base cell isn't the building's own.
        let building = world.find_building_for_tile(tile)?;
        return Some(building::read(context, building, realm));
    }
    if tile.is_harvestable_prop() {
        let prop = world.find_prop_for_tile(tile)?;
        return Some(tile::read_prop(context, prop, tile));
    }
    if tile.is(TileKind::Field) {
        return tile::read_field(context, tile);
    }
    if tile.is(TileKind::Wall) {
        return Some(tile::read_wall(context, tile));
    }
    if tile.is(TileKind::Terrain | TileKind::Rocks | TileKind::Vegetation) {
        return Some(tile::read_terrain(context, tile));
    }
    None
}

// The report for `target` as it is now, or None once it's gone (a building burnt down, a walker
// home and despawned).
pub fn inspect_target(context: &SimContext, target: &InspectTarget, realm: Realm) -> Option<InspectReport> {
    let world = context.world();
    match *target {
        InspectTarget::Building { handle, .. } => {
            world.find_building(handle.kind, handle.id).map(|building| building::read(context, building, realm))
        }
        InspectTarget::Unit { id, .. } => world.find_unit(id).and_then(|unit| unit::read(context, unit, realm)),
        InspectTarget::Prop { .. } | InspectTarget::Terrain { .. } => {
            inspect_tile(context, target_tile(context, target)?, realm)
        }
    }
}

// A prop's or terrain target's tile, not a walker passing over it (for a building or a walker,
// what's on top at its cell).
pub fn target_tile<'a>(context: &'a SimContext, target: &InspectTarget) -> Option<&'a Tile> {
    let tiles = context.tile_map();
    match *target {
        InspectTarget::Prop { cell } => tiles.find_tile(cell, TileKind::Prop | TileKind::Vegetation | TileKind::Rocks),
        InspectTarget::Terrain { cell } => tiles
            .find_tile(cell, TileKind::Vegetation | TileKind::Rocks | TileKind::Field | TileKind::Wall)
            .or_else(|| tiles.find_tile(cell, TileKind::Terrain)),
        InspectTarget::Building { cell, .. } | InspectTarget::Unit { cell, .. } => tiles.topmost_tile_at_cell(cell),
    }
}

// ----------------------------------------------
// Words
// ----------------------------------------------

// The inspector's Hud strings: `inspect.<key>`.
pub(crate) fn t(key: &str) -> String {
    text::get(UiTextCategory::Hud, &format!("inspect.{key}")).to_string()
}

pub(crate) fn tf(key: &str, args: &[(&str, &str)]) -> String {
    text::format(UiTextCategory::Hud, &format!("inspect.{key}"), args)
}

pub(crate) fn tc(key: &str, count: i64, args: &[(&str, &str)]) -> String {
    text::format_count(UiTextCategory::Hud, &format!("inspect.{key}"), count, args)
}

// A risk's level as the player reads it: Low, Medium (from 40), High (from 75).
pub fn risk_level(value: f32) -> String {
    t(&format!("risks.level.{}", crate::building::risks::risk_level_name(value).to_lowercase()))
}

// How far a cell takes a walker, as players read distances.
const METRES_PER_CELL: f32 = 10.0;

// A walker's distance in rough metres from its cells: "Close by", "About 100 metres",
// "About 1.5 km".
pub fn distance_text(cells: usize) -> String {
    let metres = cells as f32 * METRES_PER_CELL;
    if metres < 30.0 {
        return t("distance.close");
    }
    let step = if metres < 200.0 { 10.0 } else { 50.0 };
    let rounded = (metres / step).round() * step;
    if rounded < 1000.0 {
        return tf("distance.metres", &[("count", &(rounded as i64).to_string())]);
    }
    let km = (metres / 100.0).round() / 10.0;
    let km = if km.fract() == 0.0 { format!("{km:.0}") } else { format!("{km:.1}") };
    tf("distance.km", &[("count", &km)])
}

// Game seconds as calendar days (30 a month: SimConfigs::days_per_month).
pub fn secs_to_days(secs: f32) -> f32 {
    let configs = &GameConfigs::get().sim;
    secs / (configs.seconds_per_month / configs.days_per_month.max(1) as f32)
}

// "about 3 days", "about 2 months": a span of game seconds as the player reads it.
pub fn duration_text(secs: f32) -> String {
    let days = secs_to_days(secs).ceil().max(1.0);
    let days_per_month = GameConfigs::get().sim.days_per_month.max(1) as f32;
    if days >= days_per_month * 1.5 {
        let months = (days / days_per_month).round() as i64;
        return tc("duration.months", months, &[]);
    }
    tc("duration.days", days as i64, &[])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orders_take_in_only_when_accepting_or_getting() {
        assert!(StorageOrder::Accept.takes_in());
        assert!(StorageOrder::Get.takes_in());
        assert!(!StorageOrder::Refuse.takes_in());
        assert!(!StorageOrder::Empty.takes_in());
    }
}
