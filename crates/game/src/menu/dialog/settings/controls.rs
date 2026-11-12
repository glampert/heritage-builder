// The Controls tab: the key bindings, read-only.
//
// The game's keys are handled where they act, not from a bindings table: Escape in the menus
// (menu::home, menu::in_game, menu::TilePlacement's selection), undo and redo in menu/mod.rs, the
// zoom keys in Camera::on_key_input, the screenshot keys in engine::screenshot, the console and
// the developer editor in GameLoop::handle_app_event. key_bindings() lists them as they are there:
// keep the two in step.
// TODO(realm-ui): rebinding needs the handlers to read a bindings table from the configs; this
// list would then be built from it, and its rows would take a key.

use super::*;

// Each binding: what it does, and its keys.
fn key_bindings() -> [(&'static str, &'static str); 9] {
    [
        (ui::text::get(UiTextCategory::Menu, "settings.binding.close"), ui::text::get(UiTextCategory::Menu, "settings.keys.close")),
        (ui::text::get(UiTextCategory::Menu, "settings.binding.undo"), ui::text::get(UiTextCategory::Menu, "settings.keys.undo")),
        (ui::text::get(UiTextCategory::Menu, "settings.binding.redo"), ui::text::get(UiTextCategory::Menu, "settings.keys.redo")),
        (ui::text::get(UiTextCategory::Menu, "settings.binding.zoom_in"), ui::text::get(UiTextCategory::Menu, "settings.keys.zoom_in")),
        (ui::text::get(UiTextCategory::Menu, "settings.binding.zoom_out"), ui::text::get(UiTextCategory::Menu, "settings.keys.zoom_out")),
        (ui::text::get(UiTextCategory::Menu, "settings.binding.screenshot"), ui::text::get(UiTextCategory::Menu, "settings.keys.screenshot")),
        (ui::text::get(UiTextCategory::Menu, "settings.binding.screenshot_no_ui"), ui::text::get(UiTextCategory::Menu, "settings.keys.screenshot_no_ui")),
        (ui::text::get(UiTextCategory::Menu, "settings.binding.console"), ui::text::get(UiTextCategory::Menu, "settings.keys.console")),
        (ui::text::get(UiTextCategory::Menu, "settings.binding.dev_editor"), ui::text::get(UiTextCategory::Menu, "settings.keys.dev_editor")),
    ]
}

// The bindings' table (MainSettings/Controls/Key bindings), and a note that they can't be changed,
// `width` points wide (so the note is measured as it wraps when drawn); its text smaller than
// `text_scale` if need be to fit `max_height` points.
pub(super) fn contents(context: &GameUiContext, text_scale: f32, width: f32, max_height: f32) -> UiWidgetGroup {
    // The text's height drives the rows' (with their padding, which stays): shrink it by as much
    // as the whole is too tall, a few times over.
    let mut scale = text_scale;
    let mut group = build(scale, width, None);
    if group.measure(context).y <= max_height {
        return group;
    }
    // Tighter rows first, then smaller text.
    let padding = Some(TIGHT_ROW_PADDING);
    group = build(scale, width, padding);
    for _ in 0..4 {
        let height = group.measure(context).y;
        if height <= max_height || height <= 0.0 || scale <= text_scale * MIN_TEXT_SHARE {
            break;
        }
        scale = (scale * max_height / height).max(text_scale * MIN_TEXT_SHARE);
        group = build(scale, width, padding);
    }
    group
}

// Above and below the text of a table row that must be tight.
const TIGHT_ROW_PADDING: f32 = 1.0;

// The least the table's text shrinks to, as a share of the rows' text.
const MIN_TEXT_SHARE: f32 = 0.6;

fn build(text_scale: f32, width: f32, row_padding: Option<f32>) -> UiWidgetGroup {
    let mut look = skin::row_look();
    if let Some(padding) = row_padding {
        look.padding = padding;
    }
    let mut table = skin::table()
        .with_look(look)
        .with_label(UiString::key(UiTextCategory::Menu, "settings.key_bindings"))
        .with_width(width)
        .with_font_scale(UiFontScale(text_scale))
        .with_column(UiTableColumn::new(UiString::key(UiTextCategory::Menu, "settings.column.action"), UiColumnWidth::Fraction(1.0), UiColumnAlign::Left))
        .with_column(UiTableColumn::new(UiString::key(UiTextCategory::Menu, "settings.column.keys"), UiColumnWidth::Fraction(1.0), UiColumnAlign::Left));
    for (action, keys) in key_bindings() {
        table = table.with_row(UiTableRow::new([action, keys]));
    }

    let mut note = skin::rich_text(UiString::key(UiTextCategory::Menu, "settings.bindings_note")).with_font_scale(UiFontScale(text_scale * NOTE_FONT_SCALE)).with_width(width);
    if let Some(color) = skin::color("sub") {
        note = note.with_text_color(color);
    }

    UiWidgetGroup::new().with_center_horizontally(false).with_widget_spacing(Vec2::new(0.0, NOTE_SPACING)).with_widget(table).with_widget(note)
}

const NOTE_SPACING: f32 = 12.0;
