// A building that burns down or collapses (docs/plans/city-risks.md): it's despawned as when the
// player demolishes it, with nothing refunded, and leaves rubble on every cell it stood on.
//
// Building::update queues it with SimCmds::destroy_building, which runs after the step's other
// commands: a walker's visit queued in the same step still finds the building, and the next
// step finds rubble, as after a demolition between steps.

use arrayvec::ArrayVec;
use strum::Display;

use common::Color;
use engine::log;

use super::{BuildingKindAndId, risks};
use crate::{
    sim::{
        SimContext,
        resources::{RESOURCE_KIND_COUNT, StockItem},
    },
    tile::{
        minimap::{MINIMAP_ICON_DEFAULT_LIFETIME, MinimapIcon},
        rubble,
    },
    world::object::Spawner,
};

// The minimap's pings for a fire and for a collapse or rubble.
pub const FIRE_ALERT_TINT: Color = Color::new(1.0, 0.45, 0.15, 1.0);
pub const RUBBLE_ALERT_TINT: Color = Color::new(0.75, 0.68, 0.6, 1.0);

#[derive(Copy, Clone, Debug, PartialEq, Eq, Display)]
pub enum Destruction {
    BurntDown,
    Collapsed,
}

// Destroys the building: despawns it, covers its footprint with rubble, recounts the world
// stats so its people and goods stop counting at once, and raises the notice and a minimap
// alert. Whatever it held is lost. Returns false, doing nothing, if the building is gone, or if
// it was to burn down and a fire watcher put the fire out first.
pub fn destroy_building(context: &SimContext, kind_and_id: BuildingKindAndId, cause: Destruction) -> bool {
    let Some(building) = context.find_building_mut(kind_and_id.kind, kind_and_id.id) else {
        log::verbose!(log::channel!("risks"), "{cause}: {} {} is gone already.", kind_and_id.kind, kind_and_id.id);
        return false;
    };

    if cause == Destruction::BurntDown && !building.risks().is_burning() {
        log::verbose!(log::channel!("risks"), "{} at {} was put out in time.", building.name(), building.base_cell());
        return false;
    }

    let name = building.notice_name();
    let footprint = building.cell_range();
    let base_cell = building.base_cell();
    let lost: ArrayVec<StockItem, RESOURCE_KIND_COUNT> =
        building.stock().into_iter().filter(|item| item.count != 0).collect();

    Spawner::new(context).despawn_building(building);
    let rubble_cells = rubble::place_pile(context, footprint);

    // The stats are recounted on every step; recount them now so the HUD, world.stats and the
    // construction checks don't count the building's people and goods until then.
    context.world_mut().refresh_stats(context.finance());

    context.minimap_mut().push_icon(MinimapIcon::Alert, base_cell, RUBBLE_ALERT_TINT, MINIMAP_ICON_DEFAULT_LIFETIME);

    let what = match cause {
        Destruction::BurntDown => "burnt down",
        Destruction::Collapsed => "collapsed",
    };
    log::info!(
        log::channel!("risks"),
        "{name} at {base_cell} {what}, leaving {rubble_cells} cells of rubble. Lost: {}.",
        lost_goods_text(&lost).unwrap_or_else(|| "nothing".into())
    );
    risks::notify(destruction_notice(name, what, &lost));
    true
}

// "The storage yard burnt down. Lost: 40 rice, 12 wood."
fn destruction_notice(name: &str, what: &str, lost: &[StockItem]) -> String {
    let mut notice = format!("The {} {what}.", name.to_lowercase());
    if let Some(lost) = lost_goods_text(lost) {
        notice += &format!(" Lost: {lost}.");
    }
    notice
}

// "40 rice, 12 wood", or None if nothing was held.
fn lost_goods_text(lost: &[StockItem]) -> Option<String> {
    if lost.is_empty() {
        return None;
    }
    let items: Vec<String> =
        lost.iter().map(|item| format!("{} {}", item.count, item.kind.display_name().to_lowercase())).collect();
    Some(items.join(", "))
}
