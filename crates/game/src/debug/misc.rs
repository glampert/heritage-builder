use common::{coords::Cell, format_small};
use engine::ui::{self, DrawDebugUi, UiNumberFormat, UiStaticVar, UiSystem};

use crate::{
    building::config::BuildingConfigs,
    camera::Camera,
    campaign::config::{CampaignConfigs, MissionDef, MissionGoal, MissionMap},
    config::GameConfigs,
    pathfind::NodeKind,
    prop::config::PropConfigs,
    unit::config::UnitConfigs,
};

// ----------------------------------------------
// Camera Debug UI
// ----------------------------------------------

impl Camera {
    pub(crate) fn draw_debug_ui(&mut self, ui_sys: &UiSystem) {
        let configs = &mut GameConfigs::get_mut().camera;

        let mut key_shortcut_zoom = !configs.disable_key_shortcut_zoom;
        if ui::checkbox(ui_sys, "Keyboard Zoom", &mut key_shortcut_zoom) {
            configs.disable_key_shortcut_zoom = !key_shortcut_zoom;
        }

        let mut mouse_scroll_zoom = !configs.disable_mouse_scroll_zoom;
        if ui::checkbox(ui_sys, "Mouse Scroll Zoom", &mut mouse_scroll_zoom) {
            configs.disable_mouse_scroll_zoom = !mouse_scroll_zoom;
        }

        let mut smooth_mouse_scroll_zoom = !configs.disable_smooth_mouse_scroll_zoom;
        if ui::checkbox(ui_sys, "Smooth Mouse Scroll Zoom", &mut smooth_mouse_scroll_zoom) {
            configs.disable_smooth_mouse_scroll_zoom = !smooth_mouse_scroll_zoom;
        }

        ui::checkbox(ui_sys, "Constrain To Playable Map Area", &mut configs.constrain_to_playable_map_area);
        ui::checkbox(ui_sys, "Clamp To Map AABB Bounds", &mut configs.clamp_to_map_bounds);
        ui::checkbox(ui_sys, "Enable Debug Draw", &mut configs.enable_debug_draw);

        ui::separator(ui_sys);

        let (zoom_min, zoom_max) = self.zoom_limits();
        let mut zoom = self.current_zoom();

        if ui::slider(ui_sys, "Zoom", zoom_min, zoom_max, UiNumberFormat::Default, &mut zoom) {
            self.set_zoom(zoom);
        }

        let mut step_zoom = configs.fixed_step_zoom_amount;
        if ui::input_float(ui_sys, "Step Zoom", &mut step_zoom, 0.5, UiNumberFormat::Decimals(1), false) {
            configs.fixed_step_zoom_amount = step_zoom.clamp(zoom_min, zoom_max);
        }

        ui::separator(ui_sys);

        let scroll_limits = self.scroll_limits();
        let mut scroll = self.current_scroll();

        if ui::slider(ui_sys, "Scroll X", scroll_limits.0.x, scroll_limits.1.x, UiNumberFormat::Decimals(1), &mut scroll.x) {
            self.set_scroll(scroll);
        }

        if ui::slider(ui_sys, "Scroll Y", scroll_limits.0.y, scroll_limits.1.y, UiNumberFormat::Decimals(1), &mut scroll.y) {
            self.set_scroll(scroll);
        }

        ui::separator(ui_sys);

        static TELEPORT_CELL: UiStaticVar<Cell> = UiStaticVar::new(Cell::invalid());
        ui::input_i32_xy(ui_sys, "Teleport To Cell:", TELEPORT_CELL.as_mut(), false, None, None);

        ui::horizontal(ui_sys, |ui_sys| {
            if ui::button(ui_sys, "Teleport") {
                self.teleport(*TELEPORT_CELL);
            }

            if ui::button(ui_sys, "Re-center") {
                self.center();
            }
        });
    }
}

// ----------------------------------------------
// NodeKind Debug UI (pathfinding)
// ----------------------------------------------

impl NodeKind {
    pub(crate) fn draw_debug_ui(&mut self, ui_sys: &UiSystem) {
        macro_rules! node_kind_ui_checkbox {
            ($ui_sys:ident, $node_kind:ident, $flag_name:ident) => {
                let mut value = $node_kind.intersects(NodeKind::$flag_name);
                ui::checkbox($ui_sys, stringify!($flag_name), &mut value);
                $node_kind.set(NodeKind::$flag_name, value);
            };
        }

        node_kind_ui_checkbox!(ui_sys, self, EmptyLand);
        node_kind_ui_checkbox!(ui_sys, self, Road);
        node_kind_ui_checkbox!(ui_sys, self, Water);
        node_kind_ui_checkbox!(ui_sys, self, Building);
        node_kind_ui_checkbox!(ui_sys, self, BuildingRoadLink);
        node_kind_ui_checkbox!(ui_sys, self, BuildingAccess);
        node_kind_ui_checkbox!(ui_sys, self, VacantLot);
        node_kind_ui_checkbox!(ui_sys, self, SettlersSpawnPoint);
        node_kind_ui_checkbox!(ui_sys, self, Rocks);
        node_kind_ui_checkbox!(ui_sys, self, Vegetation);
        node_kind_ui_checkbox!(ui_sys, self, HarvestableTree);
    }
}

// ----------------------------------------------
// Config Container Debug UI
// ----------------------------------------------

// These per-container panels just iterate the contained config structs (each of
// which derives `DrawDebugUi`) and render them. Backed by pub(crate) accessors on
// the config containers so this debug code stays out of the config modules.

impl UnitConfigs {
    pub(crate) fn draw_debug_ui_with_header(&mut self, _header: &str, ui_sys: &UiSystem) {
        for config in self.configs_mut() {
            let name = config.name.clone();
            config.draw_debug_ui_with_header(&name, ui_sys);
        }
    }
}

impl PropConfigs {
    pub(crate) fn draw_debug_ui_with_header(&mut self, _header: &str, ui_sys: &UiSystem) {
        for config in self.configs_mut() {
            let name = config.name.clone();
            config.draw_debug_ui_with_header(&name, ui_sys);
        }
    }
}

impl BuildingConfigs {
    pub(crate) fn draw_debug_ui_with_header(&mut self, _header: &str, ui_sys: &UiSystem) {
        self.house_config_mut().draw_debug_ui_with_header("House", ui_sys);

        if let Some(_path) = ui::collapsing_header_scope(ui_sys, "House Levels", false) {
            ui::indent(ui_sys, 10.0, |ui_sys| {
                for config in self.house_levels_mut() {
                    let name = config.name.clone();
                    config.draw_debug_ui_with_header(&name, ui_sys);
                }
            });
        }

        if let Some(_path) = ui::collapsing_header_scope(ui_sys, "Producers", false) {
            ui::indent(ui_sys, 10.0, |ui_sys| {
                for config in self.producer_configs_mut() {
                    let name = config.name.clone();
                    config.draw_debug_ui_with_header(&name, ui_sys);
                }
            });
        }

        if let Some(_path) = ui::collapsing_header_scope(ui_sys, "Services", false) {
            ui::indent(ui_sys, 10.0, |ui_sys| {
                for config in self.service_configs_mut() {
                    let name = config.name.clone();
                    config.draw_debug_ui_with_header(&name, ui_sys);
                }
            });
        }

        if let Some(_path) = ui::collapsing_header_scope(ui_sys, "Storage", false) {
            ui::indent(ui_sys, 10.0, |ui_sys| {
                for config in self.storage_configs_mut() {
                    let name = config.name.clone();
                    config.draw_debug_ui_with_header(&name, ui_sys);
                }
            });
        }
    }
}

// ----------------------------------------------
// Campaign Config Debug UI
// ----------------------------------------------

// Hand-written to mirror the shape of `#[derive(DrawDebugUi)]`, which can't
// render the nested `Vec<CampaignDef>` / `Vec<MissionDef>` structure.
impl CampaignConfigs {
    pub(crate) fn draw_debug_ui_with_header(&self, header: &str, ui_sys: &UiSystem) {
        if let Some(_path) = ui::collapsing_header_scope(ui_sys, header, false) {
            self.draw_debug_ui(ui_sys);
        }
    }

    fn draw_debug_ui(&self, ui_sys: &UiSystem) {
        if self.campaigns.is_empty() {
            ui::text(ui_sys, "No campaigns loaded.");
            return;
        }

        for (campaign_id, campaign) in self.campaigns.iter().enumerate() {
            let campaign_header = format_small!("[{campaign_id}] {} ({} missions)", campaign.name, campaign.missions.len());
            if let Some(_path) = ui::collapsing_header_scope(ui_sys, &campaign_header, false) {
                ui::indent(ui_sys, 10.0, |ui_sys| {
                    for (mission_index, mission) in campaign.missions.iter().enumerate() {
                        // Indices keep the collapsing_header id unique across missions
                        // (even when two missions share the same name).
                        let mission_header = format_small!(
                            "[{campaign_id}.{mission_index}] {} ({} goals)",
                            mission.name,
                            mission.requirements.goals.len()
                        );

                        if let Some(_path) =
                            ui::collapsing_header_scope(ui_sys, &mission_header, false)
                        {
                            ui::indent(ui_sys, 10.0, |ui_sys| {
                                Self::draw_mission_def_debug_ui(ui_sys, mission);
                            });
                        }
                    }
                });
            }
        }
    }

    // Render all members of a single MissionDef.
    fn draw_mission_def_debug_ui(ui_sys: &UiSystem, mission: &MissionDef) {
        ui::text(ui_sys, format_small!("Name: {}", mission.name));
        ui::text(ui_sys, format_small!("Description: {}", mission.description));

        match &mission.map {
            MissionMap::SaveGame { save_file }     => ui::text(ui_sys, format_small!("Map: save game '{save_file}'")),
            MissionMap::Preset   { preset_number } => ui::text(ui_sys, format_small!("Map: preset #{preset_number}")),
        }

        if mission.requirements.goals.is_empty() {
            ui::text(ui_sys, "Requirements: none");
        } else {
            ui::text(ui_sys, "Requirements:");
            for goal in &mission.requirements.goals {
                match goal {
                    MissionGoal::Population { min }          => ui::bullet_text(ui_sys, format_small!("Population >= {min}")),
                    MissionGoal::Employment { min_employed } => ui::bullet_text(ui_sys, format_small!("Employment >= {min_employed}")),
                    MissionGoal::Treasury   { min_gold }     => ui::bullet_text(ui_sys, format_small!("Treasury >= {min_gold} gold")),
                    MissionGoal::Resource   { kind, min }    => ui::bullet_text(ui_sys, format_small!("Resource {kind} >= {min}")),
                }
            }
        }
    }
}
