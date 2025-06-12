use crate::{
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
        HouseState,
        DEFAULT_HOUSE_UPGRADE_FREQUENCY_SECS
    },
    producer::{
        ProducerConfig,
        ProducerOutputKind
    },
    service::{
        ServiceState,
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
    // Temporary
    house_0: HouseLevelConfig,
    house_1: HouseLevelConfig,
    service_well: ServiceConfig,
    service_market: ServiceConfig,
    dummy_producer: ProducerConfig,
    dummy_storage: StorageConfig,
}

impl BuildingConfigs {
    // TODO: Load from config file.
    pub fn load() -> Self {
        Self {
            house_0: HouseLevelConfig {
                tile_def_name: "house_0".to_string(),
                upgrade_frequency_secs: DEFAULT_HOUSE_UPGRADE_FREQUENCY_SECS,
                max_residents: 2,
                tax_generated: 0,
                services_required: ServicesList::from_slice(&[BuildingKind::Well]),
                goods_required: ConsumerGoodsList::new(),        
            },
            house_1: HouseLevelConfig {
                tile_def_name: "house_1".to_string(),
                upgrade_frequency_secs: DEFAULT_HOUSE_UPGRADE_FREQUENCY_SECS,
                max_residents: 4,
                tax_generated: 1,
                services_required: ServicesList::from_slice(&[BuildingKind::Well, BuildingKind::Market]),
                goods_required: ConsumerGoodsList::new(),        
            },
            service_well: ServiceConfig {
                tile_def_name: "well".to_string(),
                min_workers: 0,
                max_workers: 1,
                effect_radius: 3,
                goods_required: ConsumerGoodsList::new(),
            },
            service_market: ServiceConfig {
                tile_def_name: "market".to_string(),
                min_workers: 0,
                max_workers: 1,
                effect_radius: 5,
                goods_required: ConsumerGoodsList::new(),
            },
            dummy_producer: ProducerConfig {
                tile_def_name: "Producer".to_string(),
                min_workers: 0,
                max_workers: 1,
                production_output: ProducerOutputKind::ConsumerGood(ConsumerGoodKind::Rice),
                raw_materials_required: RawMaterialsList::new(),
            },
            dummy_storage: StorageConfig {
                tile_def_name: "Storage".to_string(),
                min_workers: 0,
                max_workers: 1,
                goods_accepted: ConsumerGoodsList::new(),
                raw_materials_accepted: RawMaterialsList::new()
            }
        }
    }

    pub fn find_house_level(&self, level: HouseLevel) -> &HouseLevelConfig {
        match level {
            HouseLevel::Level0 => &self.house_0,
            HouseLevel::Level1 => &self.house_1,
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
    fn find<'config>(configs: &'config BuildingConfigs, _kind: BuildingKind) -> &'config Self {
        &configs.dummy_producer
    }
}

impl BuildingConfigLookup for ServiceConfig {
    fn find<'config>(configs: &'config BuildingConfigs, kind: BuildingKind) -> &'config Self {
        if kind == BuildingKind::Well {
            &configs.service_well
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

pub fn instantiate<'config>(tile: &Tile, configs: &'config BuildingConfigs) -> Building<'config> {
    // TODO: Temporary
    if tile.name() == "well" {
        Building::new(
            "Well",
            BuildingKind::Well,
            tile.cell,
            configs,
            BuildingArchetype::new_service(ServiceState::new(BuildingKind::Well, configs))
        )
    } else if tile.name() == "market" {
        Building::new(
            "Market",
            BuildingKind::Market,
            tile.cell,
            configs,
            BuildingArchetype::new_service(ServiceState::new(BuildingKind::Market, configs))
        )
    } else if tile.name() == "house_0" {
        Building::new(
            "House",
            BuildingKind::House,
            tile.cell,
            configs,
            BuildingArchetype::new_house(HouseState::new(HouseLevel::Level0, configs))
        )
    } else {
        panic!("Unknown building tile!")
    }
}
