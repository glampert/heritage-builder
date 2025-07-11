use crate::{
    utils::hash::{self},
    tile::map::Tile,
    game::sim::resources::{
        ConsumerGoodKind,
        ConsumerGoodsList,
        RawMaterialsList,
        ServicesList
    }
};

use super::{
    Building,
    BuildingKind,
    BuildingArchetype,
    house::{
        HouseLevel,
        HouseLevelConfig,
        HouseBuilding
    },
    producer::{
        ProducerConfig,
        ProducerOutputKind,
        ProducerBuilding
    },
    service::{
        ServiceBuilding,
        ServiceConfig
    },
    storage::{
        StorageConfig
    }
};

// ----------------------------------------------
// BuildingConfigs
// ----------------------------------------------

pub struct BuildingConfigs {
    // TODO: Temporary
    house0: HouseLevelConfig,
    house1: HouseLevelConfig,
    house2: HouseLevelConfig,
    service_well_small: ServiceConfig,
    service_well_big: ServiceConfig,
    service_market: ServiceConfig,
    producer_farm: ProducerConfig,
    dummy_storage: StorageConfig,
}

impl BuildingConfigs {
    // TODO: Load from config file.
    pub fn load() -> Self {
        Self {
            house0: HouseLevelConfig {
                tile_def_name: "house0".to_string(),
                tile_def_name_hash: hash::fnv1a_from_str("house0"),
                max_residents: 2,
                tax_generated: 0,
                services_required: ServicesList::empty(),
                goods_required: ConsumerGoodsList::empty(),        
            },
            house1: HouseLevelConfig {
                tile_def_name: "house1".to_string(),
                tile_def_name_hash: hash::fnv1a_from_str("house1"),
                max_residents: 4,
                tax_generated: 1,
                services_required: ServicesList::new(&[BuildingKind::WellSmall | BuildingKind::WellBig, BuildingKind::Market]),
                // Any 1 kind of food.
                goods_required: ConsumerGoodsList::new(&[ConsumerGoodKind::any_food()]),
            },
            house2: HouseLevelConfig {
                tile_def_name: "house2".to_string(),
                tile_def_name_hash: hash::fnv1a_from_str("house2"),
                max_residents: 6,
                tax_generated: 2,
                services_required: ServicesList::new(&[BuildingKind::WellBig, BuildingKind::Market]),
                // 2 kinds of food required: Rice + meat or fish.
                goods_required: ConsumerGoodsList::new(&[ConsumerGoodKind::Rice, ConsumerGoodKind::Meat | ConsumerGoodKind::Fish]),
            },
            service_well_small: ServiceConfig {
                tile_def_name: "well_small".to_string(),
                tile_def_name_hash: hash::fnv1a_from_str("well_small"),
                min_workers: 0,
                max_workers: 1,
                effect_radius: 3,
                goods_required: ConsumerGoodsList::empty(),
            },
            service_well_big: ServiceConfig {
                tile_def_name: "well_big".to_string(),
                tile_def_name_hash: hash::fnv1a_from_str("well_big"),
                min_workers: 0,
                max_workers: 1,
                effect_radius: 5,
                goods_required: ConsumerGoodsList::empty(),
            },
            service_market: ServiceConfig {
                tile_def_name: "market".to_string(),
                tile_def_name_hash: hash::fnv1a_from_str("market"),
                min_workers: 0,
                max_workers: 1,
                effect_radius: 5,
                goods_required: ConsumerGoodsList::all(),
            },
            producer_farm: ProducerConfig {
                tile_def_name: "rice_farm".to_string(),
                tile_def_name_hash: hash::fnv1a_from_str("rice_farm"),
                min_workers: 0,
                max_workers: 1,
                production_output: ProducerOutputKind::ConsumerGood(ConsumerGoodKind::Rice),
                production_capacity: 5,
                raw_materials_required: RawMaterialsList::empty(),
                raw_materials_capacity: 5,
            },
            dummy_storage: StorageConfig {
                tile_def_name: "storage".to_string(),
                tile_def_name_hash: hash::fnv1a_from_str("storage"),
                min_workers: 0,
                max_workers: 1,
                goods_accepted: ConsumerGoodsList::empty(),
                raw_materials_accepted: RawMaterialsList::empty()
            }
        }
    }

    pub fn find_house_level(&self, level: HouseLevel) -> &HouseLevelConfig {
        match level {
            HouseLevel::Level0 => &self.house0,
            HouseLevel::Level1 => &self.house1,
            HouseLevel::Level2 => &self.house2,
        }
    }

    pub fn find<T: BuildingConfigLookup>(&self, kind: BuildingKind) -> &T {
        T::find(self, kind)
    }
}

// Trait to specialize lookup for each config type.
pub trait BuildingConfigLookup {
    fn find<'config>(configs: &'config BuildingConfigs, kind: BuildingKind) -> &'config Self;
}

impl BuildingConfigLookup for ProducerConfig {
    fn find<'config>(configs: &'config BuildingConfigs, kind: BuildingKind) -> &'config Self {
        if kind == BuildingKind::Farm {
            &configs.producer_farm
        } else { panic!("No producer!") }
    }
}

impl BuildingConfigLookup for ServiceConfig {
    fn find<'config>(configs: &'config BuildingConfigs, kind: BuildingKind) -> &'config Self {
        if kind == BuildingKind::WellSmall {
            &configs.service_well_small
        } else if kind == BuildingKind::WellBig {
            &configs.service_well_big
        } else if kind == BuildingKind::Market {
            &configs.service_market
        } else { panic!("No service!") }
    }
}

impl BuildingConfigLookup for StorageConfig {
    fn find<'config>(configs: &'config BuildingConfigs, _kind: BuildingKind) -> &'config Self {
        &configs.dummy_storage
    }
}

// ----------------------------------------------
// Helper functions
// ----------------------------------------------

pub fn instantiate<'config>(tile: &Tile, configs: &'config BuildingConfigs) -> Option<Building<'config>> {
    // TODO: Temporary
    if tile.name() == "well_small" {
        Some(Building::new(
            "Well Small",
            BuildingKind::WellSmall,
            tile.cell_range(),
            configs,
            BuildingArchetype::new_service(ServiceBuilding::new(BuildingKind::WellSmall, configs))
        ))
    } else if tile.name() == "well_big" {
        Some(Building::new(
            "Well Big",
            BuildingKind::WellBig,
            tile.cell_range(),
            configs,
            BuildingArchetype::new_service(ServiceBuilding::new(BuildingKind::WellSmall, configs))
        ))
    } else if tile.name() == "market" {
        Some(Building::new(
            "Market",
            BuildingKind::Market,
            tile.cell_range(),
            configs,
            BuildingArchetype::new_service(ServiceBuilding::new(BuildingKind::Market, configs))
        ))
    } else if tile.name() == "house0" {
        Some(Building::new(
            "House",
            BuildingKind::House,
            tile.cell_range(),
            configs,
            BuildingArchetype::new_house(HouseBuilding::new(HouseLevel::Level0, configs))
        ))
    } else if tile.name() == "rice_farm" {
        Some(Building::new(
            "Rice Farm",
            BuildingKind::Farm,
            tile.cell_range(),
            configs,
            BuildingArchetype::new_producer(ProducerBuilding::new(BuildingKind::Farm, configs))
        ))
    } else {
        eprintln!("Unknown building tile!");
        None
    }
}
