use common::Color;
use engine::ui::{DrawDebugUi, UiFontScale, UiStaticVar, UiSystem};
use proc_macros::DrawDebugUi;

use crate::{
    debug::DebugUiMode,
    prop::{Prop, PropId},
    sim::{SimCmds, SimContext, resources::ResourceKind},
    world::object::GameObject,
};

// All ImGui debug-UI drawing for `Prop`, relocated here from `prop/mod.rs`.
// The `GameObject::draw_debug_ui` method on `Prop` is a thin forward into this.
impl Prop {
    pub(crate) fn draw_debug_ui_dispatch(
        &mut self,
        _cmds: &mut SimCmds,
        context: &SimContext,
        ui_sys: &UiSystem,
        mode: DebugUiMode,
    ) {
        debug_assert!(self.is_spawned());

        match mode {
            DebugUiMode::Overview => {
                self.draw_debug_ui_overview(context, ui_sys);
            }
            DebugUiMode::Detailed => {
                let ui = ui_sys.ui();
                if ui.collapsing_header("Prop", imgui::TreeNodeFlags::empty()) {
                    ui.indent_by(10.0);
                    self.draw_debug_ui_detailed(context, ui_sys);
                    ui.unindent_by(10.0);
                }
            }
        }
    }

    fn draw_debug_ui_overview(&mut self, _context: &SimContext, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();

        ui_sys.set_window_font_scale(UiFontScale(1.2));
        ui.text(format!("{} | ID{} @{}", self.name(), self.id(), self.cell()));
        ui_sys.set_window_font_scale(UiFontScale::default());

        let color_bullet_text = |label: &str, value: u32| {
            ui.bullet_text(format!("{label}:"));
            ui.same_line();
            if value == 0 {
                ui.text_colored(Color::red().to_array(), format!("{value}"));
            } else {
                ui.text(format!("{value}"));
            }
        };

        color_bullet_text(&format!("Harvestable {}", self.harvestable_resource()), self.harvestable_amount());
    }

    fn draw_debug_ui_detailed(&mut self, context: &SimContext, ui_sys: &UiSystem) {
        let ui = ui_sys.ui();

        // Configs are static & read-only; clone to a local for the &mut display path.
        let mut config = self.config().clone();
        config.draw_debug_ui_with_header("Config", ui_sys);

        // NOTE: Use the special ##id here so we don't collide with Tile/Properties.
        if !ui.collapsing_header("Properties##_prop_properties", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        #[derive(DrawDebugUi)]
        struct DrawDebugUiVariables<'a> {
            name: &'a str,
            cell: common::coords::Cell,
            id: PropId,
            harvestable_resource: ResourceKind,
            harvestable_amount: u32,
            is_being_harvested: bool,
        }
        let mut debug_vars = DrawDebugUiVariables {
            name: self.name(),
            cell: self.cell(),
            id: self.id(),
            harvestable_resource: self.harvestable_resource(),
            harvestable_amount: self.harvestable_amount(),
            is_being_harvested: self.is_being_harvested(),
        };
        debug_vars.draw_debug_ui(ui_sys);

        if self.is_harvestable() {
            if self.harvestable_amount() == 0 {
                ui.text(format!("Time Until Respawn   : {:.2}", self.harvestable_respawn_remaining_secs()));
            }

            static HARVEST_AMOUNT: UiStaticVar<u32> = UiStaticVar::new(1);
            ui.input_scalar("Harvest Amount", HARVEST_AMOUNT.as_mut()).step(1).build();

            if ui.button("Harvest") {
                self.harvest(context, *HARVEST_AMOUNT);
            }

            if ui.button("Respawn Now") {
                self.respawn_harvestable(context);
            }
        }
    }
}
