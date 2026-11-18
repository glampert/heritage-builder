// The inspector's props and terrain (inspector-terrain-<realm>.jpg): a description, what a prop
// holds, and whether it can be cleared (clearing is free; a road or a vacant lot leaves grass).

use engine::ui::text::{self, UiTextCategory};

use super::{
    Action,
    ActionKind,
    InspectIcon,
    InspectReport,
    InspectTarget,
    PanelKind,
    Section,
    ValueRow,
    building,
    t,
    tc,
    tf,
};
use crate::{
    menu::TilePlacement,
    prop::Prop,
    sim::SimContext,
    tile::{Tile, TileKind, rubble},
};

fn description(tile: &Tile) -> Option<Section> {
    let text = text::find_str(UiTextCategory::TileDescription, tile.tile_def().hash).unwrap_or("");
    (!text.is_empty()).then(|| Section::Text { text: text.to_string() })
}

fn size_text(tile: &Tile) -> String {
    let size = tile.tile_def().size_in_cells();
    format!("{}×{}", size.width, size.height)
}

fn clearing(tile: &Tile, leaves_grass: bool) -> Section {
    let mut rows = vec![ValueRow { label: t("clear.cost"), value: t("clear.free"), warn: false }];
    if leaves_grass {
        rows.push(ValueRow { label: t("clear.leaves"), value: t("clear.grass"), warn: false });
    }
    if !tile.is(TileKind::Terrain) || tile.tile_def().is_road() {
        let blocks = if tile.tile_def().is_road() { t("clear.blocks_building") } else { t("clear.blocks_both") };
        rows.push(ValueRow { label: t("clear.blocks"), value: blocks, warn: false });
    }
    Section::Values { title: t("clear.title"), note: None, rows }
}

pub(super) fn read_prop(_context: &SimContext, prop: &Prop, tile: &Tile) -> InspectReport {
    let mut sections = Vec::new();
    sections.extend(description(tile));
    if prop.is_harvestable() && !prop.harvestable_resource().is_empty() {
        let kind = prop.harvestable_resource();
        sections.push(Section::Values {
            title: t("prop.holds"),
            note: None,
            rows: vec![ValueRow {
                label: building::good_name(kind),
                value: tc("production.units", i64::from(prop.harvestable_amount()), &[(
                    "good",
                    &building::good_name(kind).to_lowercase(),
                )]),
                warn: false,
            }],
        });
    }
    sections.push(clearing(tile, false));
    InspectReport {
        target: InspectTarget::Prop { cell: tile.base_cell() },
        panel: PanelKind::Prop,
        icon: InspectIcon::Sprite,
        name: building::sentence_case(&prop.name().to_lowercase()),
        subtitle: format!("{} · {}", t("kind.woodland"), size_text(tile)),
        status: None,
        sections,
        actions: vec![Action::new(ActionKind::Clear, "action.clear")],
        wide: false,
    }
}

// A rice field (crate::farms): its farmstead, its crop and its land.
pub(super) fn read_field(context: &SimContext, tile: &Tile) -> Option<InspectReport> {
    let field = crate::farms::field_report(context, tile.base_cell())?;
    let month_of = |ordinal: u32| (ordinal % 12 + 1).to_string();
    let crop = match (field.sown, field.reaped_in) {
        _ if field.flooded => t("field.crop.flooded"),
        (Some(sown), Some(reaped)) => tf("field.crop.growing", &[("sown", &month_of(sown)), ("reaped", &month_of(reaped))]),
        _ if field.farmstead.is_none() => t("field.crop.no_farmstead"),
        _ if !field.tended => t("field.crop.untended"),
        _ => t("field.crop.fallow"),
    };
    let farmstead = match field.farmstead_cell {
        Some(cell) => tf("field.farmstead.at", &[("cell", &format!("{}, {}", cell.x, cell.y))]),
        None => t("field.farmstead.none"),
    };
    let fertility = context.floods().fertility_at(field.cell);
    let rows = vec![
        ValueRow { label: t("field.farmstead"), value: farmstead, warn: field.farmstead.is_none() },
        ValueRow { label: t("field.stage"), value: t(&format!("field.stage.{}", field.stage.name())), warn: false },
        ValueRow { label: t("field.crop"), value: crop, warn: field.flooded },
        ValueRow { label: t("field.land"), value: tf("field.fertility", &[("value", &fertility.to_string())]), warn: false },
    ];
    let mut sections = vec![Section::Values { title: t("field.title"), note: None, rows }];
    sections.extend(description(tile));
    Some(InspectReport {
        target: InspectTarget::Terrain { cell: tile.base_cell() },
        panel: PanelKind::Terrain,
        icon: InspectIcon::Sprite,
        name: building::sentence_case(tile.name()),
        subtitle: format!("{} · {}", t("kind.field"), size_text(tile)),
        status: None,
        sections,
        actions: vec![Action::new(ActionKind::Clear, "action.clear")],
        wide: false,
    })
}

pub(super) fn read_terrain(tile: &Tile) -> InspectReport {
    let def = tile.tile_def();
    let kind_key = if rubble::is_rubble_def(def) {
        "kind.rubble"
    } else if def.is_road() {
        "kind.road"
    } else if def.is_vacant_lot() {
        "kind.vacant_lot"
    } else if tile.is(TileKind::Vegetation) {
        "kind.woodland"
    } else if tile.is(TileKind::Rocks) {
        "kind.rocks"
    } else if tile.path_kind().intersects(crate::pathfind::NodeKind::Water) {
        "kind.water"
    } else {
        "kind.land"
    };
    let can_clear = TilePlacement::can_clear(tile);
    let mut sections = Vec::new();
    sections.extend(description(tile));
    if can_clear {
        sections.push(clearing(tile, def.is_road() || def.is_vacant_lot()));
    }
    InspectReport {
        target: InspectTarget::Terrain { cell: tile.base_cell() },
        panel: PanelKind::Terrain,
        icon: InspectIcon::Sprite,
        name: building::sentence_case(tile.name()),
        subtitle: format!("{} · {}", t(kind_key), size_text(tile)),
        status: None,
        sections,
        actions: if can_clear { vec![Action::new(ActionKind::Clear, "action.clear")] } else { Vec::new() },
        wide: false,
    }
}
