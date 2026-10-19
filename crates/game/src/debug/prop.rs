use common::{Color, format_small};
use engine::ui::{self, DrawDebugUi, UiStaticVar, UiSystem};
use proc_macros::DrawDebugUi;

use crate::{
    debug::DebugUiMode,
    prop::{Prop, PropId},
    sim::{SimCmds, SimContext, resources::ResourceKind},
    world::object::GameObject,
};

// All debug-UI drawing for `Prop`, relocated here from `prop/mod.rs`.
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
                if let Some(_path) = ui::collapsing_header_scope(ui_sys, "Prop", false) {
                    ui::indent(ui_sys, 10.0, |ui_sys| {
                        self.draw_debug_ui_detailed(context, ui_sys);
                    });
                }
            }
        }
    }

    fn draw_debug_ui_overview(&mut self, _context: &SimContext, ui_sys: &UiSystem) {
        ui::with_font_scale(ui_sys, 1.2, |ui_sys| ui::text(ui_sys, format_small!("{} | ID{} @{}", self.name(), self.id(), self.cell())));

        let color_bullet_text = |label: &str, value: u32| {
            ui::horizontal(ui_sys, |ui_sys| {
                ui::bullet_text(ui_sys, format_small!("{label}:"));
                if value == 0 {
                    ui::text_colored(ui_sys, Color::red(), format_small!("{value}"));
                } else {
                    ui::text(ui_sys, format_small!("{value}"));
                }
            });
        };

        color_bullet_text(&format_small!("Harvestable {}", self.harvestable_resource()), self.harvestable_amount());
    }

    fn draw_debug_ui_detailed(&mut self, context: &SimContext, ui_sys: &UiSystem) {
        // Configs are static & read-only; clone to a local for the &mut display path.
        let mut config = self.config().clone();
        config.draw_debug_ui_with_header("Config", ui_sys);

        // NOTE: Use the special ##id here so we don't collide with Tile/Properties.
        let Some(_path) =
            ui::collapsing_header_scope(ui_sys, "Properties##_prop_properties", false)
        else {
            return; // collapsed.
        };

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
                ui::text(ui_sys, format_small!("Time Until Respawn   : {:.2}", self.harvestable_respawn_remaining_secs()));
            }

            static HARVEST_AMOUNT: UiStaticVar<u32> = UiStaticVar::new(1);
            ui::input_scalar(ui_sys, "Harvest Amount", HARVEST_AMOUNT.as_mut(), 1);

            if ui::button(ui_sys, "Harvest") {
                self.harvest(context, *HARVEST_AMOUNT);
            }

            if ui::button(ui_sys, "Respawn Now") {
                self.respawn_harvestable(context);
            }
        }
    }
}
