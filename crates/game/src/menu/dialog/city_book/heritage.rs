// The Heritage page (city-heritage-<realm>.jpg): faith and monuments (GDD 5.11 and 5.15) in three
// tabs, Monument, Patrons and Festivals. The tabs exist from Phase 6 with what the game has, which
// is nothing yet: a mission's monument comes with game Phase 2, the realm's patrons and its
// festivals with M8 (faith and festivals), each filling its tab.

use common::Vec2;
use engine::ui::{self, UiTextAnchor, text::UiTextCategory, widgets::*};
use serde_json::json;

use super::{BODY, CityPage, PageContext, Paint, STATE, body_canvas};
use crate::{menu::skin, ui_context::GameUiContext};

pub(super) struct Heritage;

const TABS: [&str; 3] = ["monument", "patrons", "festivals"];

impl CityPage for Heritage {
    fn id(&self) -> &'static str {
        "heritage"
    }

    fn icon(&self) -> &'static str {
        "monument"
    }

    fn note(&self, _context: &GameUiContext) -> String {
        ui::text::get(UiTextCategory::City, "book.note.heritage").to_string()
    }

    fn body(&self, context: &mut GameUiContext, page: &PageContext) -> UiWidgetGroup {
        let size = Vec2::new(page.pt(200.0), page.pt(50.0));
        let mut tabs = skin::tab_bar_sized(context, size, 1.0)
            .with_label(ui::text::get(UiTextCategory::City, "book.heritage.tabs").to_string())
            .with_selected(STATE.as_ref().heritage_tab.min(TABS.len() - 1))
            .on_tab_changed(|bar, _, _| STATE.as_mut().heritage_tab = bar.selected_index());
        // The tabs' row and the gap under it, as a bar with one tab measures it.
        let tabs_height = skin::tab_bar_sized(context, size, 1.0).with_tab("Tab").measure(context).y;
        let height = (BODY.y - tabs_height / page.scales.size).max(100.0);
        for tab in TABS {
            let title = ui::text::get(UiTextCategory::City, &format!("book.heritage.{tab}")).to_string();
            let empty = ui::text::get(UiTextCategory::City, &format!("book.heritage.{tab}.none")).to_string();
            let later = ui::text::get(UiTextCategory::City, &format!("book.heritage.{tab}.later")).to_string();
            let card_title = title.clone();
            let canvas = body_canvas(page, BODY.x, height, tab, json!({ "empty": true, "text": empty }), move |p: &Paint| {
                p.card(0.0, 0.0, BODY.x, height, Some(&card_title), None);
                p.text(BODY.x * 0.5, height * 0.45, &empty, 24.0, p.c.text, UiTextAnchor::Middle, true);
                p.text(BODY.x * 0.5, height * 0.45 + 40.0, &later, 18.0, p.c.sub, UiTextAnchor::Middle, false);
            });
            tabs = tabs.with_tab_contents(title, super::left_group().with_widget(canvas));
        }
        super::left_group().with_widget_spacing(Vec2::zero()).with_widget(tabs)
    }
}
