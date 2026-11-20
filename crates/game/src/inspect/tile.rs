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
        _ if field.flooded => match crate::floods::recedes_in(context) {
            Some(month) => tf("field.crop.flooded", &[("month", &month_of(month))]),
            None => t("field.crop.flooded_now"),
        },
        (Some(sown), Some(reaped)) => tf("field.crop.growing", &[("sown", &month_of(sown)), ("reaped", &month_of(reaped))]),
        _ if field.farmstead.is_none() => t("field.crop.no_farmstead"),
        _ if !field.tended => t("field.crop.untended"),
        _ => t("field.crop.fallow"),
    };
    let farmstead = match field.farmstead_cell {
        Some(cell) => tf("field.farmstead.at", &[("cell", &format!("{}, {}", cell.x, cell.y))]),
        None => t("field.farmstead.none"),
    };
    let water = match field.gate_cell {
        Some(cell) => tf("field.water.gate", &[("cell", &format!("{}, {}", cell.x, cell.y))]),
        None => t("field.water.rains"),
    };
    let mut rows = vec![
        ValueRow { label: t("field.farmstead"), value: farmstead, warn: field.farmstead.is_none() },
        ValueRow { label: t("field.stage"), value: t(&format!("field.stage.{}", field.stage.name())), warn: false },
        ValueRow { label: t("field.crop"), value: crop, warn: field.flooded },
        ValueRow { label: t("field.water"), value: water, warn: false },
    ];
    // The land's fertility: a meter of its own while the river floods, else one plain line.
    let land = land_sections(context, tile);
    if land.is_empty() {
        let fertility = context.floods().fertility_at(field.cell);
        rows.push(ValueRow {
            label: t("field.land"),
            value: tf("field.fertility", &[("value", &fertility.to_string())]),
            warn: false,
        });
    }
    let mut sections = vec![Section::Values { title: t("field.title"), note: None, rows }];
    sections.extend(land);
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

// The land's fertility (docs/plans/seasons-floods.md, §5), a meter, while the river floods: what
// its harvests are worth, and what takes and gives it (without floods it isn't kept, and every cell
// is the same). For a field, and for land a field could go on.
fn land_sections(context: &SimContext, tile: &Tile) -> Vec<Section> {
    if !context.floods().is_enabled() {
        return Vec::new();
    }
    let cell = tile.base_cell();
    let fertility = context.floods().fertility_at(cell);
    let factor = context.floods().yield_factor_at(cell);
    vec![
        Section::Progress {
            title: t("land.title"),
            note: tf("land.note", &[("factor", &format!("{factor:.1}"))]),
            value: fertility as f32 / crate::sim::floods::MAX_FERTILITY as f32,
        },
        Section::Text { text: t("land.text") },
    ]
}

// The flood's section for a cell under water (crate::floods): when the water goes.
fn flood_section(context: &SimContext, tile: &Tile) -> Option<Section> {
    if !context.floods().is_flooded(tile.base_cell()) {
        return None;
    }
    let month = crate::floods::recedes_in(context)? % 12 + 1;
    Some(Section::Values {
        title: t("flood.title"),
        note: None,
        rows: vec![
            ValueRow {
                label: t("flood.water"),
                value: tf("flood.under_water", &[("month", &month.to_string())]),
                warn: true,
            },
            ValueRow { label: t("flood.refuses"), value: t("flood.refused"), warn: false },
        ],
    })
}

// A dyke's section (crate::floods): its wear and when a builder last walked it, while the river
// floods; without floods a dyke is a road and says nothing more.
fn dyke_section(context: &SimContext, tile: &Tile) -> Option<Section> {
    if !tile.tile_def().blocks_floods || !context.floods().is_enabled() {
        return None;
    }
    let floods = context.floods();
    let cell = tile.base_cell();
    let wear = floods.wear_at(cell).round() as i64;
    let worn = floods.wear_at(cell) >= crate::config::GameConfigs::get().sim.floods.worn_wear;
    let walked = match floods.dyke(cell).and_then(|dyke| dyke.walked) {
        Some(month) => {
            let ago = crate::farms::current_month(context).saturating_sub(month);
            if ago == 0 { t("dyke.walked_this_month") } else { tc("dyke.walked_ago", i64::from(ago), &[]) }
        }
        None => t("dyke.never_walked"),
    };
    Some(Section::Values {
        title: t("dyke.title"),
        note: Some(t("dyke.note")),
        rows: vec![
            ValueRow { label: t("dyke.wear"), value: tf("dyke.wear_value", &[("wear", &wear.to_string())]), warn: worn },
            ValueRow { label: t("dyke.builder"), value: walked, warn: false },
        ],
    })
}

pub(super) fn read_terrain(context: &SimContext, tile: &Tile) -> InspectReport {
    let def = tile.tile_def();
    let kind_key = if rubble::is_rubble_def(def) {
        "kind.rubble"
    } else if def.blocks_floods {
        "kind.dyke"
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
    sections.extend(dyke_section(context, tile));
    sections.extend(flood_section(context, tile));
    if kind_key == "kind.land" || kind_key == "kind.vacant_lot" {
        sections.extend(land_sections(context, tile));
    }
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
