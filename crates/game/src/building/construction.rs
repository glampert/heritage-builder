// Building costs beyond gold: the construction materials a building takes from the city's storage
// yards when placed, and how long it takes to build (docs/plans/construction.md).

use std::fmt;

use arrayvec::ArrayVec;
use serde::{Deserialize, Deserializer, Serialize, de};
use strum::{Display, EnumString};

use common::time::Seconds;

use crate::{
    cheats,
    config::GameConfigs,
    sim::resources::{ResourceKind, StockItem},
    tile::sets::TileDef,
    world::city_stock::Withdrawal,
};

// ----------------------------------------------
// ConstructionMaterials
// ----------------------------------------------

pub const MAX_CONSTRUCTION_MATERIALS: usize = 4;

// The materials a tile takes when placed, e.g. 6 Wood and 8 Bricks. Written in `tile_set.json`
// as a map from resource name to count: `"materials": { "Wood": 6, "Bricks": 8 }`.
#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub struct ConstructionMaterials(ArrayVec<StockItem, MAX_CONSTRUCTION_MATERIALS>);

impl ConstructionMaterials {
    // Each kind must be a single resource other than gold, at most once, with a non-zero count.
    pub fn new(items: &[StockItem]) -> Result<Self, String> {
        let mut materials = ArrayVec::new();

        for item in items {
            if !item.kind.is_single_resource() || item.kind == ResourceKind::Gold {
                return Err(format!("'{}' isn't a construction material.", item.kind));
            }
            if item.count == 0 {
                return Err(format!("A count of 0 {} costs nothing; leave it out.", item.kind));
            }
            if materials.iter().any(|existing: &StockItem| existing.kind == item.kind) {
                return Err(format!("{} is listed twice.", item.kind));
            }
            if materials.try_push(*item).is_err() {
                return Err(format!("At most {MAX_CONSTRUCTION_MATERIALS} kinds of materials."));
            }
        }

        Ok(Self(materials))
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = &StockItem> {
        self.0.iter()
    }
}

// "6 Wood, 8 Bricks".
impl fmt::Display for ConstructionMaterials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, item) in self.0.iter().enumerate() {
            if index != 0 {
                f.write_str(", ")?;
            }
            write!(f, "{} {}", item.count, item.kind)?;
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for ConstructionMaterials {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // A map keeps the JSON readable; its order is the order materials are taken in.
        let entries: indexmap_like::OrderedEntries = Deserialize::deserialize(deserializer)?;

        let mut items = ArrayVec::<StockItem, MAX_CONSTRUCTION_MATERIALS>::new();
        for (name, count) in entries.0 {
            let kind = ResourceKind::from_name(&name).ok_or_else(|| de::Error::custom(format!("Unknown resource '{name}'.")))?;
            items.try_push(StockItem { kind, count }).map_err(|_| {
                de::Error::custom(format!("At most {MAX_CONSTRUCTION_MATERIALS} kinds of materials."))
            })?;
        }

        Self::new(&items).map_err(de::Error::custom)
    }
}

// A JSON object read as a list of (name, count) pairs in file order.
mod indexmap_like {
    use std::fmt;

    use serde::{Deserialize, Deserializer, de};

    pub struct OrderedEntries(pub Vec<(String, u32)>);

    impl<'de> Deserialize<'de> for OrderedEntries {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            struct EntriesVisitor;

            impl<'de> de::Visitor<'de> for EntriesVisitor {
                type Value = OrderedEntries;

                fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    f.write_str("a map of resource names to counts")
                }

                fn visit_map<A: de::MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                    let mut entries = Vec::new();
                    while let Some((name, count)) = map.next_entry::<String, u32>()? {
                        entries.push((name, count));
                    }
                    Ok(OrderedEntries(entries))
                }
            }

            deserializer.deserialize_map(EntriesVisitor)
        }
    }
}

// ----------------------------------------------
// ConstructionTier
// ----------------------------------------------

// How long a building takes to build, by kind of building. The durations are in
// `SimConfigs::construction`.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Display, EnumString, Serialize, Deserialize)]
pub enum ConstructionTier {
    // Roads, terrain and anything else placed at once.
    Instant,
    // Gardens and other decorations.
    Decorative,
    // Houses, wells, farms, workshops, storage.
    Basic,
    // Temples, offices, schools.
    Civic,
    // Palaces and fortresses.
    Grand,
}

// ----------------------------------------------
// ConstructionMode
// ----------------------------------------------

// Whether a placed building starts as a construction site (when its tier takes time) or finished.
// The player's placements build sites; presets, world spawns and the developer editor don't.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ConstructionMode {
    Site,
    Finished,
}

// Simulated seconds a building of `tile_def` takes to build, after the time scale: 0 for the
// Instant tier, with the instant_construction cheat, or with a time scale of 0.
pub fn construction_secs(tile_def: &TileDef) -> Seconds {
    let configs = &GameConfigs::get().sim.construction;
    let secs = match tile_def.construction_tier() {
        ConstructionTier::Instant => 0.0,
        ConstructionTier::Decorative => configs.decorative_secs,
        ConstructionTier::Basic => configs.basic_secs,
        ConstructionTier::Civic => configs.civic_secs,
        ConstructionTier::Grand => configs.grand_secs,
    };
    scaled_construction_secs(secs)
}

// `secs` after the time scale and the instant_construction cheat.
pub fn scaled_construction_secs(secs: Seconds) -> Seconds {
    if cheats::get().instant_construction {
        return 0.0;
    }
    (secs * GameConfigs::get().sim.construction.time_scale).max(0.0)
}

// ----------------------------------------------
// ConstructionSite
// ----------------------------------------------

// Number of drawn stages a site goes through (TileFlags::ConstructionStage holds 0 to 3).
pub const CONSTRUCTION_STAGES: u8 = 4;

// A building being built: inert until its timer runs out (docs/plans/construction.md §4).
// Saved with the building.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct ConstructionSite {
    // Simulated seconds it takes, fixed at placement, and how many have passed.
    total_secs: Seconds,
    elapsed_secs: Seconds,

    // What placing it cost, so cancelling refunds exactly that.
    paid_gold: u32,
    withdrawal: Withdrawal,

    // Residents who move in when a house site completes: the settlers who arrived on its lot.
    settlers: u32,
}

impl ConstructionSite {
    pub fn new(total_secs: Seconds, paid_gold: u32, withdrawal: Withdrawal) -> Self {
        debug_assert!(total_secs > 0.0);
        Self { total_secs, elapsed_secs: 0.0, paid_gold, withdrawal, settlers: 0 }
    }

    pub fn with_settlers(mut self, settlers: u32) -> Self {
        self.settlers = settlers;
        self
    }

    // Advances by `delta_time_secs` of simulated time. True once it's finished.
    pub fn advance(&mut self, delta_time_secs: Seconds) -> bool {
        self.elapsed_secs = (self.elapsed_secs + delta_time_secs.max(0.0)).min(self.total_secs);
        self.is_finished()
    }

    #[inline]
    pub fn is_finished(&self) -> bool {
        self.elapsed_secs >= self.total_secs
    }

    // 0 to 1.
    #[inline]
    pub fn progress(&self) -> f32 {
        if self.total_secs > 0.0 { (self.elapsed_secs / self.total_secs).clamp(0.0, 1.0) } else { 1.0 }
    }

    #[inline]
    pub fn remaining_secs(&self) -> Seconds {
        (self.total_secs - self.elapsed_secs).max(0.0)
    }

    #[inline]
    pub fn total_secs(&self) -> Seconds {
        self.total_secs
    }

    // The drawn stage, 0 to CONSTRUCTION_STAGES - 1, a quarter of the progress each.
    pub fn stage(&self) -> u8 {
        ((self.progress() * f32::from(CONSTRUCTION_STAGES)) as u8).min(CONSTRUCTION_STAGES - 1)
    }

    #[inline]
    pub fn paid_gold(&self) -> u32 {
        self.paid_gold
    }

    #[inline]
    pub fn withdrawal(&self) -> &Withdrawal {
        &self.withdrawal
    }

    #[inline]
    pub fn settlers(&self) -> u32 {
        self.settlers
    }
}

// ----------------------------------------------
// Unit Tests
// ----------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_site_progress_and_stages() {
        let mut site = ConstructionSite::new(10.0, 5, Withdrawal::default());
        assert_eq!((site.progress(), site.stage(), site.remaining_secs()), (0.0, 0, 10.0));

        assert!(!site.advance(2.6));
        assert_eq!(site.stage(), 1);

        assert!(!site.advance(5.0));
        assert_eq!(site.stage(), 3);

        assert!(site.advance(5.0));
        assert_eq!((site.progress(), site.stage(), site.remaining_secs()), (1.0, 3, 0.0));
    }

    #[test]
    fn test_materials_parse_and_validate() {
        let materials: ConstructionMaterials = serde_json::from_str(r#"{ "Wood": 6, "Bricks": 8 }"#).unwrap();
        assert_eq!(materials.to_string(), "6 Wood, 8 Bricks");

        assert!(serde_json::from_str::<ConstructionMaterials>(r#"{ "Gold": 1 }"#).is_err());
        assert!(serde_json::from_str::<ConstructionMaterials>(r#"{ "Wood": 0 }"#).is_err());
        assert!(serde_json::from_str::<ConstructionMaterials>(r#"{ "Stone": 1 }"#).is_err());
        assert!(serde_json::from_str::<ConstructionMaterials>(r#"{ "Wood": 1, "Clay": 1, "Metal": 1, "Bricks": 1, "Rice": 1 }"#).is_err());
    }
}
