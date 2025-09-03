use crate::{
    log,
    tile::Tile,
    imgui_ui::UiSystem,
    utils::hash::{self, StringHash},
    game::sim::resources::{
        ResourceKind,
        ResourceKinds,
        ServiceKind,
        ServiceKinds
    }
};

use super::{
    Building,
    BuildingKind,
    BuildingArchetype,
    house::{
        HouseLevel,
        HouseConfig,
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
// BuildingConfig
// ----------------------------------------------

pub trait BuildingConfig {
    fn draw_debug_ui(&self, ui_sys: &UiSystem);
}

#[macro_export]
macro_rules! building_config_impl {
    ($config_struct:ident) => {
        impl BuildingConfig for $config_struct {
            fn draw_debug_ui(&self, ui_sys: &UiSystem) {
                let ui = ui_sys.builder();
                if ui.collapsing_header("Config", imgui::TreeNodeFlags::empty()) {
                    $config_struct::draw_debug_ui(self, ui_sys);
                }
            }
        }
    };
}

// ----------------------------------------------
// BuildingConfigs
// ----------------------------------------------

pub struct BuildingConfigs {
    // TODO: Temporary
    house_cfg: HouseConfig,
    house0: HouseLevelConfig,
    house1: HouseLevelConfig,
    house2: HouseLevelConfig,
    house3: HouseLevelConfig,
    service_well_small: ServiceConfig,
    service_well_big: ServiceConfig,
    service_market: ServiceConfig,
    producer_rice_farm: ProducerConfig,
    producer_livestock_farm: ProducerConfig,
    producer_distillery: ProducerConfig,
    storage_yard: StorageConfig,
    storage_granary: StorageConfig,
}

impl BuildingConfigs {
    // TODO: Load from config file.
    pub fn load() -> Self {
        Self {
            house_cfg: HouseConfig {
                // General configuration parameters for all house buildings & levels.
                population_update_frequency_secs: 60.0,
                stock_update_frequency_secs: 60.0,
                upgrade_update_frequency_secs: 10.0,
            },
            house0: HouseLevelConfig {
                name: "House Level 0".to_string(),
                tile_def_name: "house0".to_string(),
                tile_def_name_hash: hash::fnv1a_from_str("house0"),
                max_population: 2,
                tax_generated: 0,
                worker_percentage: 100,
                population_increase_chance: 80,
                services_required: ServiceKinds::none(),
                resources_required: ResourceKinds::none(),
                stock_capacity: 5,
            },
            house1: HouseLevelConfig {
                name: "House Level 1".to_string(),
                tile_def_name: "house1".to_string(),
                tile_def_name_hash: hash::fnv1a_from_str("house1"),
                max_population: 8,
                tax_generated: 1,
                worker_percentage: 75,
                population_increase_chance: 70,
                // Any water source (small well OR big well) AND a market.
                services_required: ServiceKinds::with_slice(&[BuildingKind::WellSmall | BuildingKind::WellBig, BuildingKind::Market]),
                // Any 1 kind of food.
                resources_required: ResourceKinds::with_slice(&[ResourceKind::foods()]),
                stock_capacity: 10,
            },
            house2: HouseLevelConfig {
                name: "House Level 2".to_string(),
                tile_def_name: "house2".to_string(),
                tile_def_name_hash: hash::fnv1a_from_str("house2"),
                max_population: 12,
                tax_generated: 2,
                worker_percentage: 50,
                population_increase_chance: 60,
                services_required: ServiceKinds::with_slice(&[BuildingKind::WellBig, BuildingKind::Market]),
                // 2 kinds of food required: Rice AND Meat OR Fish.
                resources_required: ResourceKinds::with_slice(&[ResourceKind::Rice, ResourceKind::Meat | ResourceKind::Fish]),
                stock_capacity: 15,
            },
            house3: HouseLevelConfig {
                name: "House Level 3".to_string(),
                tile_def_name: "house3".to_string(),
                tile_def_name_hash: hash::fnv1a_from_str("house3"),
                max_population: 25,
                tax_generated: 3,
                worker_percentage: 50,
                population_increase_chance: 50,
                services_required: ServiceKinds::with_slice(&[BuildingKind::WellBig, BuildingKind::Market]),
                // 2 kinds of food required: Rice AND Meat OR Fish.
                resources_required: ResourceKinds::with_slice(&[ResourceKind::Rice, ResourceKind::Meat | ResourceKind::Fish, ResourceKind::Wine]),
                stock_capacity: 15,
            },
            service_well_small: ServiceConfig {
                name: "Well Small".to_string(),
                tile_def_name: "well_small".to_string(),
                tile_def_name_hash: hash::fnv1a_from_str("well_small"),
                min_workers: 0,
                max_workers: 0,
                effect_radius: 5,
                requires_road_access: false,
                has_patrol_unit: false,
                patrol_frequency_secs: 0.0,
                stock_update_frequency_secs: 0.0,
                stock_capacity: 0,
                resources_required: ResourceKinds::none(),
            },
            service_well_big: ServiceConfig {
                name: "Well Big".to_string(),
                tile_def_name: "well_big".to_string(),
                tile_def_name_hash: hash::fnv1a_from_str("well_big"),
                min_workers: 1,
                max_workers: 2,
                effect_radius: 10,
                requires_road_access: true,
                has_patrol_unit: false,
                patrol_frequency_secs: 0.0,
                stock_update_frequency_secs: 0.0,
                stock_capacity: 0,
                resources_required: ResourceKinds::none(),
            },
            service_market: ServiceConfig {
                name: "Market".to_string(),
                tile_def_name: "market".to_string(),
                tile_def_name_hash: hash::fnv1a_from_str("market"),
                min_workers: 1,
                max_workers: 2,
                effect_radius: 40,
                requires_road_access: true,
                has_patrol_unit: true,
                patrol_frequency_secs: 10.0,
                stock_update_frequency_secs: 20.0,
                stock_capacity: 10,
                resources_required: ResourceKinds::with_kinds(ResourceKind::foods() | ResourceKind::consumer_goods()),
            },
            producer_rice_farm: ProducerConfig {
                name: "Rice Farm".to_string(),
                tile_def_name: "rice_farm".to_string(),
                tile_def_name_hash: hash::fnv1a_from_str("rice_farm"),
                min_workers: 2,
                max_workers: 4,
                production_output_frequency_secs: 20.0,
                production_output: ResourceKind::Rice,
                production_capacity: 5,
                resources_required: ResourceKinds::none(),
                resources_capacity: 0,
                deliver_to_storage_kinds: BuildingKind::Granary,
                fetch_from_storage_kinds: BuildingKind::Granary,
            },
            producer_livestock_farm: ProducerConfig {
                name: "Livestock Farm".to_string(),
                tile_def_name: "livestock_farm".to_string(),
                tile_def_name_hash: hash::fnv1a_from_str("livestock_farm"),
                min_workers: 2,
                max_workers: 4,
                production_output_frequency_secs: 20.0,
                production_output: ResourceKind::Meat,
                production_capacity: 5,
                resources_required: ResourceKinds::none(),
                resources_capacity: 0,
                deliver_to_storage_kinds: BuildingKind::Granary,
                fetch_from_storage_kinds: BuildingKind::Granary,
            },
            producer_distillery: ProducerConfig {
                name: "Distillery".to_string(),
                tile_def_name: "distillery".to_string(),
                tile_def_name_hash: hash::fnv1a_from_str("distillery"),
                min_workers: 2,
                max_workers: 4,
                production_output_frequency_secs: 20.0,
                production_output: ResourceKind::Wine,
                production_capacity: 5,
                resources_required: ResourceKinds::with_kinds(ResourceKind::Rice),
                resources_capacity: 8,
                deliver_to_storage_kinds: BuildingKind::StorageYard,
                fetch_from_storage_kinds: BuildingKind::Granary,
            },
            storage_yard: StorageConfig {
                name: "Storage Yard".to_string(),
                tile_def_name: "storage_yard".to_string(),
                tile_def_name_hash: hash::fnv1a_from_str("storage_yard"),
                min_workers: 1,
                max_workers: 4,
                resources_accepted: ResourceKinds::all(),
                num_slots: 8,
                slot_capacity: 4,
            },
            storage_granary: StorageConfig {
                name: "Granary".to_string(),
                tile_def_name: "granary".to_string(),
                tile_def_name_hash: hash::fnv1a_from_str("granary"),
                min_workers: 1,
                max_workers: 4,
                resources_accepted: ResourceKinds::with_kinds(ResourceKind::foods()),
                num_slots: 8,
                slot_capacity: 4,
            }
        }
    }

    pub fn find_house_config(&self) -> &HouseConfig {
        &self.house_cfg
    }

    pub fn find_house_level_config(&self, level: HouseLevel) -> &HouseLevelConfig {
        match level {
            HouseLevel::Level0 => &self.house0,
            HouseLevel::Level1 => &self.house1,
            HouseLevel::Level2 => &self.house2,
            HouseLevel::Level3 => &self.house3,
        }
    }

    pub fn find_producer_config(&self, kind: BuildingKind, tile_name: &str, tile_name_hash: StringHash) -> &ProducerConfig {
        if kind == BuildingKind::Farm {
            if tile_name_hash == hash::fnv1a_from_str("rice_farm") {
                &self.producer_rice_farm
            } else if tile_name_hash == hash::fnv1a_from_str("livestock_farm") {
                &self.producer_livestock_farm
            } else { panic!("Unknown farm tile: '{}'", tile_name) }
        } else if kind == BuildingKind::Factory {
            if tile_name_hash == hash::fnv1a_from_str("distillery") {
                &self.producer_distillery
            } else { panic!("Unknown factory tile: '{}'", tile_name) }
        } else { panic!("No producer!") }
    }

    pub fn find_service_config(&self, kind: ServiceKind) -> &ServiceConfig {
        if kind == BuildingKind::WellSmall {
            &self.service_well_small
        } else if kind == BuildingKind::WellBig {
            &self.service_well_big
        } else if kind == BuildingKind::Market {
            &self.service_market
        } else { panic!("No service!") }
    }

    pub fn find_storage_config(&self, kind: BuildingKind) -> &StorageConfig {
        if kind == BuildingKind::Granary {
            &self.storage_granary
        } else if kind == BuildingKind::StorageYard {
            &self.storage_yard
        } else { panic!("No storage!") }
    }
}

// ----------------------------------------------
// Helper functions
// ----------------------------------------------

pub fn instantiate<'config>(tile: &Tile, configs: &'config BuildingConfigs) -> Option<Building<'config>> {
    // TODO: Temporary
    let tile_name_hash = tile.tile_def().hash;
    if tile.name() == "well_small" {
        let config = configs.find_service_config(BuildingKind::WellSmall);
        Some(Building::new(
            BuildingKind::WellSmall,
            tile.cell_range(),
            BuildingArchetype::from(ServiceBuilding::new(config))
        ))
    } else if tile.name() == "well_big" {
        let config = configs.find_service_config(BuildingKind::WellBig);
        Some(Building::new(
            BuildingKind::WellBig,
            tile.cell_range(),
            BuildingArchetype::from(ServiceBuilding::new(config))
        ))
    } else if tile.name() == "market" {
        let config = configs.find_service_config(BuildingKind::Market);
        Some(Building::new(
            BuildingKind::Market,
            tile.cell_range(),
            BuildingArchetype::from(ServiceBuilding::new(config))
        ))
    } else if tile.name() == "house0" {
        let config = configs.find_house_config();
        Some(Building::new(
            BuildingKind::House,
            tile.cell_range(),
            BuildingArchetype::from(HouseBuilding::new(HouseLevel::Level0, config, configs))
        ))
    } else if tile.name() == "rice_farm" || tile.name() == "livestock_farm" {
        let config = configs.find_producer_config(BuildingKind::Farm, tile.name(), tile_name_hash);
        Some(Building::new(
            BuildingKind::Farm,
            tile.cell_range(),
            BuildingArchetype::from(ProducerBuilding::new(config))
        ))
    } else if tile.name() == "distillery" {
        let config = configs.find_producer_config(BuildingKind::Factory, tile.name(), tile_name_hash);
        Some(Building::new(
            BuildingKind::Factory,
            tile.cell_range(),
            BuildingArchetype::from(ProducerBuilding::new(config))
        ))
    } else if tile.name() == "granary" {
        let config = configs.find_storage_config(BuildingKind::Granary);
        Some(Building::new(
            BuildingKind::Granary,
            tile.cell_range(),
            BuildingArchetype::from(StorageBuilding::new(config))
        ))
    } else if tile.name() == "storage_yard" {
        let config = configs.find_storage_config(BuildingKind::StorageYard);
        Some(Building::new(
            BuildingKind::StorageYard,
            tile.cell_range(),
            BuildingArchetype::from(StorageBuilding::new(config))
        ))
    } else {
        log::error!("Unknown building tile!");
        None
    }
}
