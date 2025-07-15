use crate::{
    utils::hash::{self},
    tile::map::Tile,
    game::sim::resources::{
        ResourceKind,
        ResourceKinds,
        ServiceKinds
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
        ProducerBuilding
    },
    service::{
        ServiceConfig,
        ServiceBuilding
    },
    storage::{
        StorageConfig,
        StorageBuilding
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
    storage_yard: StorageConfig,
    storage_granary: StorageConfig,
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
                services_required: ServiceKinds::none(),
                resources_required: ResourceKinds::none(),        
            },
            house1: HouseLevelConfig {
                tile_def_name: "house1".to_string(),
                tile_def_name_hash: hash::fnv1a_from_str("house1"),
                max_residents: 4,
                tax_generated: 1,
                // Any water source (small well OR big well) AND a market.
                services_required: ServiceKinds::with_slice(&[BuildingKind::WellSmall | BuildingKind::WellBig, BuildingKind::Market]),
                // Any 1 kind of food.
                resources_required: ResourceKinds::with_kinds(ResourceKind::foods()),
            },
            house2: HouseLevelConfig {
                tile_def_name: "house2".to_string(),
                tile_def_name_hash: hash::fnv1a_from_str("house2"),
                max_residents: 6,
                tax_generated: 2,
                services_required: ServiceKinds::with_slice(&[BuildingKind::WellBig, BuildingKind::Market]),
                // 2 kinds of food required: Rice AND Meat OR Fish.
                resources_required: ResourceKinds::with_slice(&[ResourceKind::Rice, ResourceKind::Meat | ResourceKind::Fish]),
            },
            service_well_small: ServiceConfig {
                tile_def_name: "well_small".to_string(),
                tile_def_name_hash: hash::fnv1a_from_str("well_small"),
                min_workers: 0,
                max_workers: 1,
                effect_radius: 3,
                resources_required: ResourceKinds::none(),
            },
            service_well_big: ServiceConfig {
                tile_def_name: "well_big".to_string(),
                tile_def_name_hash: hash::fnv1a_from_str("well_big"),
                min_workers: 0,
                max_workers: 1,
                effect_radius: 5,
                resources_required: ResourceKinds::none(),
            },
            service_market: ServiceConfig {
                tile_def_name: "market".to_string(),
                tile_def_name_hash: hash::fnv1a_from_str("market"),
                min_workers: 0,
                max_workers: 1,
                effect_radius: 5,
                resources_required: ResourceKinds::with_kinds(ResourceKind::foods()),
            },
            producer_farm: ProducerConfig {
                tile_def_name: "rice_farm".to_string(),
                tile_def_name_hash: hash::fnv1a_from_str("rice_farm"),
                min_workers: 0,
                max_workers: 1,
                production_output: ResourceKind::Rice,
                production_capacity: 5,
                resources_required: ResourceKinds::none(),
                resources_capacity: 0,
            },
            storage_yard: StorageConfig {
                tile_def_name: "storage_yard".to_string(),
                tile_def_name_hash: hash::fnv1a_from_str("storage_yard"),
                min_workers: 0,
                max_workers: 1,
                resources_accepted: ResourceKinds::all(),
                num_slots: 8,
                slot_capacity: 4,
            },
            storage_granary: StorageConfig {
                tile_def_name: "granary".to_string(),
                tile_def_name_hash: hash::fnv1a_from_str("granary"),
                min_workers: 0,
                max_workers: 1,
                resources_accepted: ResourceKinds::with_kinds(ResourceKind::foods()),
                num_slots: 8,
                slot_capacity: 4,
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
    fn find<'config>(configs: &'config BuildingConfigs, kind: BuildingKind) -> &'config Self {
        if kind == BuildingKind::Granary {
            &configs.storage_granary
        } else if kind == BuildingKind::StorageYard {
            &configs.storage_yard
        } else { panic!("No storage!") }
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
    } else if tile.name() == "granary" {
        Some(Building::new(
            "Granary",
            BuildingKind::Granary,
            tile.cell_range(),
            configs,
            BuildingArchetype::new_storage(StorageBuilding::new(BuildingKind::Granary, configs))
        ))
    } else if tile.name() == "storage_yard" {
        Some(Building::new(
            "Storage Yard",
            BuildingKind::StorageYard,
            tile.cell_range(),
            configs,
            BuildingArchetype::new_storage(StorageBuilding::new(BuildingKind::StorageYard, configs))
        ))
    } else {
        eprintln!("Unknown building tile!");
        None
    }
}
