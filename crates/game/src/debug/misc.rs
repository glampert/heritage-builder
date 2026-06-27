use common::coords::Cell;
use engine::ui::{self, DrawDebugUi, UiStaticVar, UiSystem};

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
        let ui = ui_sys.ui();
        let configs = &mut GameConfigs::get_mut().camera;

        let mut key_shortcut_zoom = !configs.disable_key_shortcut_zoom;
        if ui.checkbox("Keyboard Zoom", &mut key_shortcut_zoom) {
            configs.disable_key_shortcut_zoom = !key_shortcut_zoom;
        }

        let mut mouse_scroll_zoom = !configs.disable_mouse_scroll_zoom;
        if ui.checkbox("Mouse Scroll Zoom", &mut mouse_scroll_zoom) {
            configs.disable_mouse_scroll_zoom = !mouse_scroll_zoom;
        }

        let mut smooth_mouse_scroll_zoom = !configs.disable_smooth_mouse_scroll_zoom;
        if ui.checkbox("Smooth Mouse Scroll Zoom", &mut smooth_mouse_scroll_zoom) {
            configs.disable_smooth_mouse_scroll_zoom = !smooth_mouse_scroll_zoom;
        }

        ui.checkbox("Constrain To Playable Map Area", &mut configs.constrain_to_playable_map_area);
        ui.checkbox("Clamp To Map AABB Bounds", &mut configs.clamp_to_map_bounds);
        ui.checkbox("Enable Debug Draw", &mut configs.enable_debug_draw);

        ui.separator();

        let (zoom_min, zoom_max) = self.zoom_limits();
        let mut zoom = self.current_zoom();

        if ui.slider("Zoom", zoom_min, zoom_max, &mut zoom) {
            self.set_zoom(zoom);
        }

        let mut step_zoom = configs.fixed_step_zoom_amount;
        if ui.input_float("Step Zoom", &mut step_zoom).display_format("%.1f").step(0.5).build() {
            configs.fixed_step_zoom_amount = step_zoom.clamp(zoom_min, zoom_max);
        }

        ui.separator();

        let scroll_limits = self.scroll_limits();
        let mut scroll = self.current_scroll();

        if ui.slider_config("Scroll X", scroll_limits.0.x, scroll_limits.1.x).display_format("%.1f").build(&mut scroll.x) {
            self.set_scroll(scroll);
        }

        if ui.slider_config("Scroll Y", scroll_limits.0.y, scroll_limits.1.y).display_format("%.1f").build(&mut scroll.y) {
            self.set_scroll(scroll);
        }

        ui.separator();

        static TELEPORT_CELL: UiStaticVar<Cell> = UiStaticVar::new(Cell::invalid());
        ui::input_i32_xy(ui, "Teleport To Cell:", TELEPORT_CELL.as_mut(), false, None, None);

        if ui.button("Teleport") {
            self.teleport(*TELEPORT_CELL);
        }

        ui.same_line();

        if ui.button("Re-center") {
            self.center();
        }
    }
}

// ----------------------------------------------
// NodeKind Debug UI (pathfinding)
// ----------------------------------------------

impl NodeKind {
    pub(crate) fn draw_debug_ui(&mut self, ui_sys: &UiSystem) {
        macro_rules! node_kind_ui_checkbox {
            ($ui:ident, $node_kind:ident, $flag_name:ident) => {
                let mut value = $node_kind.intersects(NodeKind::$flag_name);
                $ui.checkbox(stringify!($flag_name), &mut value);
                $node_kind.set(NodeKind::$flag_name, value);
            };
        }

        let ui = ui_sys.ui();
        node_kind_ui_checkbox!(ui, self, EmptyLand);
        node_kind_ui_checkbox!(ui, self, Road);
        node_kind_ui_checkbox!(ui, self, Water);
        node_kind_ui_checkbox!(ui, self, Building);
        node_kind_ui_checkbox!(ui, self, BuildingRoadLink);
        node_kind_ui_checkbox!(ui, self, BuildingAccess);
        node_kind_ui_checkbox!(ui, self, VacantLot);
        node_kind_ui_checkbox!(ui, self, SettlersSpawnPoint);
        node_kind_ui_checkbox!(ui, self, Rocks);
        node_kind_ui_checkbox!(ui, self, Vegetation);
        node_kind_ui_checkbox!(ui, self, HarvestableTree);
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
        let ui = ui_sys.ui();

        self.house_config_mut().draw_debug_ui_with_header("House", ui_sys);

        if ui.collapsing_header("House Levels", imgui::TreeNodeFlags::empty()) {
            ui.indent_by(10.0);
            for config in self.house_levels_mut() {
                let name = config.name.clone();
                config.draw_debug_ui_with_header(&name, ui_sys);
            }
            ui.unindent_by(10.0);
        }

        if ui.collapsing_header("Producers", imgui::TreeNodeFlags::empty()) {
            ui.indent_by(10.0);
            for config in self.producer_configs_mut() {
                let name = config.name.clone();
                config.draw_debug_ui_with_header(&name, ui_sys);
            }
            ui.unindent_by(10.0);
        }

        if ui.collapsing_header("Services", imgui::TreeNodeFlags::empty()) {
            ui.indent_by(10.0);
            for config in self.service_configs_mut() {
                let name = config.name.clone();
                config.draw_debug_ui_with_header(&name, ui_sys);
            }
            ui.unindent_by(10.0);
        }

        if ui.collapsing_header("Storage", imgui::TreeNodeFlags::empty()) {
            ui.indent_by(10.0);
            for config in self.storage_configs_mut() {
                let name = config.name.clone();
                config.draw_debug_ui_with_header(&name, ui_sys);
            }
            ui.unindent_by(10.0);
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
        let ui = ui_sys.ui();
        if ui.collapsing_header(header, imgui::TreeNodeFlags::empty()) {
            self.draw_debug_ui(ui_sys);
        }
    }

    fn draw_debug_ui(&self, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();

        if self.campaigns.is_empty() {
            ui.text("No campaigns loaded.");
            return;
        }

        for (campaign_id, campaign) in self.campaigns.iter().enumerate() {
            let campaign_header = format!("[{campaign_id}] {} ({} missions)", campaign.name, campaign.missions.len());
            if ui.collapsing_header(&campaign_header, imgui::TreeNodeFlags::empty()) {
                ui.indent_by(10.0);

                for (mission_index, mission) in campaign.missions.iter().enumerate() {
                    // Indices keep the collapsing_header id unique across missions
                    // (even when two missions share the same name).
                    let mission_header = format!(
                        "[{campaign_id}.{mission_index}] {} ({} goals)",
                        mission.name,
                        mission.requirements.goals.len()
                    );

                    if ui.collapsing_header(&mission_header, imgui::TreeNodeFlags::empty()) {
                        ui.indent_by(10.0);
                        Self::draw_mission_def_debug_ui(ui_sys, mission);
                        ui.unindent_by(10.0);
                    }
                }

                ui.unindent_by(10.0);
            }
        }
    }

    // Render all members of a single MissionDef.
    fn draw_mission_def_debug_ui(ui_sys: &UiSystem, mission: &MissionDef) {
        let ui = ui_sys.ui();

        ui.text(format!("Name: {}", mission.name));
        ui.text(format!("Description: {}", mission.description));

        match &mission.map {
            MissionMap::SaveGame { save_file }     => ui.text(format!("Map: save game '{save_file}'")),
            MissionMap::Preset   { preset_number } => ui.text(format!("Map: preset #{preset_number}")),
        }

        if mission.requirements.goals.is_empty() {
            ui.text("Requirements: none");
        } else {
            ui.text("Requirements:");
            for goal in &mission.requirements.goals {
                match goal {
                    MissionGoal::Population { min }          => ui.bullet_text(format!("Population >= {min}")),
                    MissionGoal::Employment { min_employed } => ui.bullet_text(format!("Employment >= {min_employed}")),
                    MissionGoal::Treasury   { min_gold }     => ui.bullet_text(format!("Treasury >= {min_gold} gold")),
                    MissionGoal::Resource   { kind, min }    => ui.bullet_text(format!("Resource {kind} >= {min}")),
                }
            }
        }
    }
}
