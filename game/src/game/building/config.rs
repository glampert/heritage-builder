use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::{
    house::{HouseBuilding, HouseConfig, HouseLevel, HouseLevelConfig},
    producer::{ProducerBuilding, ProducerConfig},
    service::{ServiceBuilding, ServiceConfig},
    storage::{StorageBuilding, StorageConfig},
    BuildingArchetype, BuildingArchetypeKind, BuildingKind,
};
use crate::{
    configurations,
    game::sim::resources::ServiceKind,
    imgui_ui::UiSystem,
    log,
    tile::sets::TileDef,
    utils::hash::{self, PreHashedKeyMap, StringHash},
};

// ----------------------------------------------
// BuildingConfig
// ----------------------------------------------

pub trait BuildingConfig {
    fn building_kind(&self) -> BuildingKind;
    fn archetype_kind(&self) -> BuildingArchetypeKind;
    fn post_load(&mut self, index: usize) -> bool;
    fn draw_debug_ui(&self, ui_sys: &UiSystem);
}

#[macro_export]
macro_rules! building_config {
    ($config_struct:ident) => {
        impl $crate::game::building::config::BuildingConfig for $config_struct {
            #[inline]
            fn building_kind(&self) -> $crate::game::building::BuildingKind {
                self.kind
            }

            #[inline]
            fn archetype_kind(&self) -> $crate::game::building::BuildingArchetypeKind {
                self.kind.archetype_kind()
            }

            fn post_load(&mut self, index: usize) -> bool {
                use $crate::{log, utils::hash};

                // Must have a building name.
                if self.name.is_empty() {
                    log::error!(log::channel!("config"),
                                "{} [{index}]: Invalid empty name!",
                                stringify!($config_struct));
                    return false;
                }

                // Must have a tile def name.
                if self.tile_def_name.is_empty() {
                    log::error!(log::channel!("config"),
                                "{} '{}': Invalid empty TileDef name! Index: [{index}]",
                                stringify!($config_struct),
                                self.name);
                    return false;
                }

                self.tile_def_name_hash = hash::fnv1a_from_str(&self.tile_def_name);
                debug_assert!(self.tile_def_name_hash != hash::NULL_HASH);
                true
            }

            // This requires that the config struct derives from DrawDebugUi
            // or that it provides a draw_debug_ui_with_header() function.
            fn draw_debug_ui(&self, ui_sys: &$crate::imgui_ui::UiSystem) {
                self.draw_debug_ui_with_header("Config", ui_sys);
            }
        }
    };
}

// ----------------------------------------------
// BuildingConfigEntry
// ----------------------------------------------

struct BuildingConfigEntry {
    archetype_kind: BuildingArchetypeKind,
    index: usize,
}

impl BuildingConfigEntry {
    fn instantiate_archetype(&'static self,
                             configs: &'static BuildingConfigs)
                             -> (BuildingKind, BuildingArchetype) {
        match self.archetype_kind {
            BuildingArchetypeKind::ProducerBuilding => {
                let producer_config = &configs.producer_configs[self.index];
                debug_assert!(producer_config.kind.intersects(BuildingKind::producers()));
                (producer_config.kind,
                 BuildingArchetype::from(ProducerBuilding::new(producer_config)))
            }
            BuildingArchetypeKind::StorageBuilding => {
                let storage_config = &configs.storage_configs[self.index];
                debug_assert!(storage_config.kind.intersects(BuildingKind::storage()));
                (storage_config.kind, BuildingArchetype::from(StorageBuilding::new(storage_config)))
            }
            BuildingArchetypeKind::ServiceBuilding => {
                let service_config = &configs.service_configs[self.index];
                debug_assert!(service_config.kind.intersects(BuildingKind::services()));
                (service_config.kind, BuildingArchetype::from(ServiceBuilding::new(service_config)))
            }
            BuildingArchetypeKind::HouseBuilding => {
                let house_config = &configs.house_config;
                let house_level_config = &configs.house_levels[self.index];
                (BuildingKind::House,
                 BuildingArchetype::from(HouseBuilding::new(house_level_config.level,
                                                            house_config,
                                                            configs)))
            }
        }
    }
}

// ----------------------------------------------
// BuildingConfigs
// ----------------------------------------------

#[derive(Default, Serialize, Deserialize)]
pub struct BuildingConfigs {
    // Serialized data:
    house_config: HouseConfig,
    house_levels: Vec<HouseLevelConfig>, // [HouseLevel::COUNT]

    producer_configs: Vec<ProducerConfig>,
    service_configs: Vec<ServiceConfig>,
    storage_configs: Vec<StorageConfig>,

    // Runtime lookup:
    #[serde(skip)]
    tile_def_mapping: PreHashedKeyMap<StringHash, BuildingConfigEntry>, // tile_def.name => (kind, index)

    #[serde(skip)]
    service_mapping: HashMap<ServiceKind, usize>, // ServiceKind => service_configs[index]

    #[serde(skip)]
    storage_mapping: HashMap<BuildingKind, usize>, // BuildingKind => storage_configs[index]

    // Default fallback configs:
    #[serde(skip)]
    default_house_level_config: HouseLevelConfig,

    #[serde(skip)]
    default_producer_config: ProducerConfig,

    #[serde(skip)]
    default_service_config: ServiceConfig,

    #[serde(skip)]
    default_storage_config: StorageConfig,
}

impl BuildingConfigs {
    pub fn find_house_config(&'static self) -> &'static HouseConfig {
        &self.house_config
    }

    pub fn find_house_level_config(&'static self, level: HouseLevel) -> &'static HouseLevelConfig {
        let index = level as usize;

        if index < self.house_levels.len() {
            let config = &self.house_levels[index];
            debug_assert!(config.kind == BuildingKind::House);
            return config;
        }

        log::error!(log::channel!("config"), "Can't find HouseLevelConfig for level {level}!");
        &self.default_house_level_config
    }

    pub fn find_producer_config(&'static self,
                                kind: BuildingKind,
                                tile_def_name_hash: StringHash,
                                tile_def_name: &str)
                                -> &'static ProducerConfig {
        debug_assert!(kind.is_single_building());
        debug_assert!(tile_def_name_hash != hash::NULL_HASH);

        match self.tile_def_mapping.get(&tile_def_name_hash) {
            Some(entry) => {
                debug_assert!(entry.archetype_kind == BuildingArchetypeKind::ProducerBuilding);
                if entry.archetype_kind == kind.archetype_kind() {
                    let config = &self.producer_configs[entry.index];
                    debug_assert!(config.kind.is_single_building()
                                  && config.kind.intersects(BuildingKind::producers()));
                    if config.kind == kind {
                        return config;
                    }
                }
                log::error!(log::channel!("config"),
                            "Invalid ProducerConfig kind ({kind}) for '{tile_def_name}'.");
                &self.default_producer_config
            }
            None => {
                log::error!(log::channel!("config"),
                            "Can't find ProducerConfig for {kind} | '{tile_def_name}'.");
                &self.default_producer_config
            }
        }
    }

    pub fn find_service_config(&'static self, kind: ServiceKind) -> &'static ServiceConfig {
        debug_assert!(kind.is_single_building());

        match self.service_mapping.get(&kind) {
            Some(index) => {
                let config = &self.service_configs[*index];
                debug_assert!(config.kind.is_single_building() && config.kind == kind);
                config
            }
            None => {
                log::error!(log::channel!("config"), "Can't find ServiceConfig for {kind}!");
                &self.default_service_config
            }
        }
    }

    pub fn find_storage_config(&'static self, kind: BuildingKind) -> &'static StorageConfig {
        debug_assert!(kind.is_single_building());

        match self.storage_mapping.get(&kind) {
            Some(index) => {
                let config = &self.storage_configs[*index];
                debug_assert!(config.kind.is_single_building() && config.kind == kind);
                config
            }
            None => {
                log::error!(log::channel!("config"), "Can't find StorageConfig for {kind}!");
                &self.default_storage_config
            }
        }
    }

    pub fn new_building_archetype_for_tile_def(
        &'static self,
        tile_def: &TileDef)
        -> Result<(BuildingKind, BuildingArchetype), String> {
        debug_assert!(tile_def.hash != hash::NULL_HASH);

        match self.tile_def_mapping.get(&tile_def.hash) {
            Some(entry) => Ok(entry.instantiate_archetype(self)),
            None => Err(format!("Can't find Building config for TileDef '{}'", tile_def.name)),
        }
    }

    fn post_load(&'static mut self) {
        self.house_config.kind = BuildingKind::House;
        self.house_config.post_load(0);

        if self.house_levels.len() != HouseLevel::count() {
            log::error!(log::channel!("config"),
                        "BuildingConfigs: Unexpected House Level count: {} vs {}",
                        self.house_levels.len(),
                        HouseLevel::count());
        }

        // HOUSE LEVELS:
        for (index, config) in &mut self.house_levels.iter_mut().enumerate() {
            config.kind = BuildingKind::House;

            if !config.post_load(index) {
                // Entries that fail to load will not be visible in the lookup table.
                continue;
            }

            let entry =
                BuildingConfigEntry { archetype_kind: BuildingArchetypeKind::HouseBuilding, index };

            if self.tile_def_mapping.insert(config.tile_def_name_hash, entry).is_some() {
                log::error!(log::channel!("config"), "HouseLevelConfig '{}': An entry for key '{}' ({:#X}) already exists at [{index}]!",
                            config.name,
                            config.tile_def_name,
                            config.tile_def_name_hash);
            }
        }

        // PRODUCERS:
        for (index, config) in &mut self.producer_configs.iter_mut().enumerate() {
            if !config.kind.intersects(BuildingKind::producers())
               || !config.kind.is_single_building()
            {
                log::error!(log::channel!("config"),
                            "ProducerConfig '{}': Invalid BuildingKind: {}.",
                            config.name,
                            config.kind);
                continue;
            }

            if !config.post_load(index) {
                // Entries that fail to load will not be visible in the lookup table.
                continue;
            }

            let entry =
                BuildingConfigEntry { archetype_kind: BuildingArchetypeKind::ProducerBuilding, index };

            if self.tile_def_mapping.insert(config.tile_def_name_hash, entry).is_some() {
                log::error!(log::channel!("config"), "ProducerConfig '{}': An entry for key '{}' ({:#X}) already exists at [{index}]!",
                            config.name,
                            config.tile_def_name,
                            config.tile_def_name_hash);
            }
        }

        // SERVICES:
        for (index, config) in &mut self.service_configs.iter_mut().enumerate() {
            if !config.kind.intersects(BuildingKind::services())
               || !config.kind.is_single_building()
            {
                log::error!(log::channel!("config"),
                            "ServiceConfig '{}': Invalid BuildingKind: {}.",
                            config.name,
                            config.kind);
                continue;
            }

            if !config.post_load(index) {
                // Entries that fail to load will not be visible in the lookup table.
                continue;
            }

            let entry =
                BuildingConfigEntry { archetype_kind: BuildingArchetypeKind::ServiceBuilding, index };

            if self.tile_def_mapping.insert(config.tile_def_name_hash, entry).is_some() {
                log::error!(log::channel!("config"), "ServiceConfig '{}': An entry for key '{}' ({:#X}) already exists at [{index}]!",
                            config.name,
                            config.tile_def_name,
                            config.tile_def_name_hash);
            }

            if self.service_mapping.insert(config.kind, index).is_some() {
                log::error!(log::channel!("config"), "ServiceConfig '{}': An entry for kind {} already exists at [{index}]!",
                            config.name,
                            config.kind);
            }
        }

        // STORAGE:
        for (index, config) in &mut self.storage_configs.iter_mut().enumerate() {
            if !config.kind.intersects(BuildingKind::storage()) || !config.kind.is_single_building()
            {
                log::error!(log::channel!("config"),
                            "StorageConfig '{}': Invalid BuildingKind: {}.",
                            config.name,
                            config.kind);
                continue;
            }

            if !config.post_load(index) {
                // Entries that fail to load will not be visible in the lookup table.
                continue;
            }

            let entry =
                BuildingConfigEntry { archetype_kind: BuildingArchetypeKind::StorageBuilding, index };

            if self.tile_def_mapping.insert(config.tile_def_name_hash, entry).is_some() {
                log::error!(log::channel!("config"), "StorageConfig '{}': An entry for key '{}' ({:#X}) already exists at [{index}]!",
                            config.name,
                            config.tile_def_name,
                            config.tile_def_name_hash);
            }

            if self.storage_mapping.insert(config.kind, index).is_some() {
                log::error!(log::channel!("config"), "StorageConfig '{}': An entry for kind {} already exists at [{index}]!",
                            config.name,
                            config.kind);
            }
        }
    }

    fn draw_debug_ui_with_header(&'static self, _header: &str, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();

        self.house_config.draw_debug_ui_with_header("House", ui_sys);

        if ui.collapsing_header("House Levels", imgui::TreeNodeFlags::empty()) {
            ui.indent_by(10.0);
            for config in &self.house_levels {
                config.draw_debug_ui_with_header(&config.name, ui_sys);
            }
            ui.unindent_by(10.0);
        }

        if ui.collapsing_header("Producers", imgui::TreeNodeFlags::empty()) {
            ui.indent_by(10.0);
            for config in &self.producer_configs {
                config.draw_debug_ui_with_header(&config.name, ui_sys);
            }
            ui.unindent_by(10.0);
        }

        if ui.collapsing_header("Services", imgui::TreeNodeFlags::empty()) {
            ui.indent_by(10.0);
            for config in &self.service_configs {
                config.draw_debug_ui_with_header(&config.name, ui_sys);
            }
            ui.unindent_by(10.0);
        }

        if ui.collapsing_header("Storage", imgui::TreeNodeFlags::empty()) {
            ui.indent_by(10.0);
            for config in &self.storage_configs {
                config.draw_debug_ui_with_header(&config.name, ui_sys);
            }
            ui.unindent_by(10.0);
        }
    }
}

// ----------------------------------------------
// BuildingConfigs Global Singleton
// ----------------------------------------------

configurations! { BUILDING_CONFIGS_SINGLETON, BuildingConfigs, "buildings/configs" }
