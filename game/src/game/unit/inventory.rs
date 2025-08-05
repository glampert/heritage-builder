use crate::{
    imgui_ui::UiSystem,
    game::sim::resources::{ResourceKind, StockItem}
};

// ----------------------------------------------
// UnitInventory
// ----------------------------------------------

#[derive(Clone, Default)]
pub struct UnitInventory {
    // Unit can carry only one resource kind at a time.
    item: Option<StockItem>,
}

impl UnitInventory {
    #[inline]
    pub fn peek(&self) -> Option<StockItem> {
        self.item
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.item.is_none()
    }

    #[inline]
    pub fn count(&self) -> u32 {
        match &self.item {
            Some(item) => item.count,
            None => 0,
        }
    }

    #[inline]
    pub fn receive_resources(&mut self, kind: ResourceKind, count: u32) -> u32 {
        if let Some(item) = &mut self.item {
            debug_assert!(item.kind == kind && item.count != 0);
            item.count += count;
        } else {
            self.item = Some(StockItem { kind, count });
        }
        count
    }

    // Returns number of items decremented, which can be <= `count`.
    #[inline]
    pub fn give_resources(&mut self, kind: ResourceKind, count: u32) -> u32 {
        if let Some(item) = &mut self.item {
            debug_assert!(item.kind == kind && item.count != 0);

            let given_count = {
                if count <= item.count {
                    item.count -= count;
                    count
                } else {
                    let prev_count = item.count;
                    item.count = 0;
                    prev_count
                }
            };

            if item.count == 0 {
                self.item = None; // Gave away everything.
            }

            given_count
        } else {
            0
        }
    }

    pub fn draw_debug_ui(&mut self, ui_sys: &UiSystem) {
        let ui = ui_sys.builder();

        if !ui.collapsing_header("Inventory", imgui::TreeNodeFlags::empty()) {
            return; // collapsed.
        }

        if let Some(item) = self.item {
            ui.text(format!("Item  : {}", item.kind));
            ui.text(format!("Count : {}", item.count));
        } else {
            ui.text("<empty>");
        }
    }
}
