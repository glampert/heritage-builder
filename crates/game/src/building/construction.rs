// Building costs beyond gold: the construction materials a building takes from the city's storage
// yards when placed, and how long it takes to build (docs/plans/construction.md).

use std::fmt;

use arrayvec::ArrayVec;
use serde::{Deserialize, Deserializer, Serialize, de};
use strum::{Display, EnumString};

use crate::sim::resources::{ResourceKind, StockItem};

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
