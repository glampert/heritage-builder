// The Skin Gallery: a developer window (DevEditor, Debug/Show Skin Gallery) that shows everything
// the realm skins draw, built with the skin helpers in the realm's own theme: the heading levels,
// the text colours, every panel, every button role and its four states, the controls (toggle,
// checkbox, slider, dropdown, step selectors, a scrolling list and template rows with icons), the
// 9-slice centres, the separators, the emblems, the data widgets (images in each fit, columns), a
// popup, tooltips and a toast. Its tabs pick the realm (they set the ui.realm override); it follows
// ui.realm too, and rebuilds whenever the skin changes (skin::version()).
//
// Its widgets' paths start with "Skin Gallery/". ui_skins.jsonl opens it in each realm, checks
// ui.layout_issues and drives its widgets. It took over from the engine's sample menus.

use common::Vec2;
use engine::ui::{self, UiFontScale, UiNineSlice, UiNineSliceCenter, UiStaticVar, UiString, UiThemeId, widgets::*};
use strum::IntoEnumIterator;

use super::skin::{self, ButtonRole, EmblemState, HeadingLevel, SkinDialogSize, SkinPanel};
use crate::{
    realm::{self, Realm},
    ui_context::GameUiContext,
};

pub const SKIN_GALLERY_LABEL: &str = "Skin Gallery";

// Its size, as dialogs give theirs (points on a 1920 × 1080 window, scaled to the window).
const GALLERY_SIZE: SkinDialogSize = SkinDialogSize::Points([1500.0, -120.0]);

// A panel swatch's size, and how many go in a row.
const SWATCH_SIZE: Vec2 = Vec2::new(118.0, 84.0);
const SWATCHES_PER_ROW: usize = 6;
const STATE_SWATCH_SIZE: Vec2 = Vec2::new(84.0, 34.0);
const CENTRE_SWATCH_SIZE: Vec2 = Vec2::new(200.0, 110.0);

// The buttons of a set width (UiTextButtonWidth), which ui_skins.jsonl measures.
const FIXED_BUTTON_WIDTH: f32 = 150.0;
const MIN_BUTTON_WIDTH: f32 = 120.0;

const SECTION_SPACING: Vec2 = Vec2::new(10.0, 10.0);
const CAPTION_FONT_SCALE: UiFontScale = UiFontScale(0.7);

// A line of text in every realm's names, to see the fonts' Vietnamese letters.
const SAMPLE_TEXT: &str = "Đại Long · Thái Ninh · Hội Thủy";

// What the gallery's controls set.
struct GalleryState {
    toggle: bool,
    checkbox: bool,
    slider: u32,
    dropdown: usize,
    steps: usize,
}

static GALLERY_STATE: UiStaticVar<GalleryState> =
    UiStaticVar::new(GalleryState { toggle: true, checkbox: true, slider: 60, dropdown: 1, steps: 2 });

// ----------------------------------------------
// SkinGallery
// ----------------------------------------------

#[derive(Default)]
pub struct SkinGallery {
    menu: Option<UiMenuRcMut>,
    version: u32, // The skin version it was built with.
}

impl SkinGallery {
    // Draws it in the current skin's theme, built again if the skin changed.
    pub fn draw(&mut self, context: &mut GameUiContext) {
        if !skin::is_loaded() {
            return;
        }

        let previous_theme: UiThemeId = context.ui_sys.swap_ui_theme(skin::theme(context.ui_sys));

        if self.menu.is_none() || self.version != skin::version() {
            self.menu = Some(build(context));
            self.version = skin::version();
        }

        let menu = self.menu.as_mut().unwrap();
        menu.set_size(skin::dialog_size(GALLERY_SIZE, ui::screen_size(context.ui_sys)));
        menu.draw(context);

        context.ui_sys.swap_ui_theme(previous_theme);
    }

    // Drops it, e.g. once hidden: it's built again when next drawn.
    pub fn reset(&mut self) {
        self.menu = None;
    }
}

// ----------------------------------------------
// Building
// ----------------------------------------------

fn build(context: &mut GameUiContext) -> UiMenuRcMut {
    let size = skin::dialog_size(GALLERY_SIZE, ui::screen_size(context.ui_sys));
    let menu = skin::panel(context, "dialog").apply_to_menu(
        UiMenu::new(SKIN_GALLERY_LABEL).with_flags(UiMenuFlags::IsOpen | UiMenuFlags::AlignCenter).with_size(size).with_widget_spacing(SECTION_SPACING),
    );
    let mut menu = menu.into_rc();
    let menu_ref = menu.downgrade();

    let realm = skin::current_realm();
    let title = UiString::from(format!("{SKIN_GALLERY_LABEL}: {}", realm.display_name()));
    menu.add_widget(skin::heading(context, [title], HeadingLevel::Dialog));
    menu.add_widget(realm_tabs(context, realm));
    menu.add_widget(actions(context, menu_ref));

    // One column, the interactive parts first; it scrolls. Each part is the menu's own widget, laid
    // out one after the other as drawn.
    let parts: [(&str, fn(&mut GameUiContext) -> UiWidgetImpl); 9] = [
        ("Controls", |context| controls(context).into()),
        ("Buttons", |context| buttons(context).into()),
        ("Button states", |context| button_states(context).into()),
        ("Headings", |context| headings(context).into()),
        ("Text colours", |_| text_colors().into()),
        ("Panels", |context| panels(context).into()),
        ("Separators", |context| separators(context).into()),
        ("Emblems", |context| emblems(context).into()),
        ("Data", |context| data(context).into()),
    ];
    for (title, part) in parts {
        menu.add_widget(section(context, title));
        menu.add_widget(part(context));
    }
    menu
}

fn section(context: &mut GameUiContext, title: &str) -> UiMenuHeading {
    skin::heading(context, [UiString::from(title)], HeadingLevel::Section)
}

// A line of small text in the skin's `sub` colour.
fn caption(text: &str) -> UiMenuHeading {
    let text = UiText::new(text.to_string(), CAPTION_FONT_SCALE);
    let text = match skin::color("sub") {
        Some(color) => text.with_color(color),
        None => text,
    };
    UiMenuHeading::new([text]).with_margin_top(0.0).with_margin_bottom(0.0)
}

fn row() -> UiWidgetGroup {
    UiWidgetGroup::new().with_stack_vertically(false).with_widget_spacing(Vec2::new(10.0, 6.0))
}

// Rows of `per_row` widgets.
fn rows(widgets: Vec<UiWidgetImpl>, per_row: usize) -> UiWidgetGroup {
    let mut group = UiWidgetGroup::new().with_center_horizontally(false).with_widget_spacing(Vec2::new(10.0, 8.0));
    let mut widgets = widgets.into_iter().peekable();
    while widgets.peek().is_some() {
        let mut line = row();
        for widget in widgets.by_ref().take(per_row) {
            line.add_widget(widget);
        }
        group.add_widget(line);
    }
    group
}

// A widget with a caption under it.
fn captioned(widget: impl Into<UiWidgetImpl>, text: &str) -> UiWidgetImpl {
    UiWidgetImpl::from(UiWidgetGroup::new().with_widget_spacing(Vec2::new(0.0, 2.0)).with_widget(widget).with_widget(caption(text)))
}

// The realm tabs: picking one sets the ui.realm override, which the editor's skin follows.
fn realm_tabs(context: &mut GameUiContext, current: Realm) -> UiTabBar {
    let tabs = Realm::iter().fold(skin::tab_bar(context).with_label("Realm"), |tabs, realm| tabs.with_tab(realm.display_name()));
    tabs.with_selected(current as usize).on_tab_changed(|tabs, _context, _previous| {
        if let Some(realm) = Realm::iter().nth(tabs.selected_index()) {
            realm::set_override(Some(realm));
        }
    })
}

// The popup, toast and tooltip buttons.
fn actions(context: &mut GameUiContext, menu_ref: UiMenuWeakMut) -> UiWidgetGroup {
    let popup = skin::button(context, ButtonRole::Primary, "Open Popup", move |_, context| {
        let Some(mut menu) = menu_ref.upgrade() else {
            return;
        };
        let close_ref = menu.downgrade();
        menu.reset_message_box(); // Built again, in the skin shown now.
        menu.open_message_box(context, |context| {
            let lines = [UiString::from("A popup"), UiString::from(SAMPLE_TEXT)];
            let ok_ref = close_ref.clone();
            skin::popup(context, "Gallery Popup", lines).with_buttons([
                skin::button(context, ButtonRole::Primary, "Ok", move |_, context| {
                    if let Some(mut menu) = ok_ref.upgrade() {
                        menu.close_message_box(context);
                    }
                }),
                skin::button(context, ButtonRole::Secondary, "Cancel", move |_, context| {
                    if let Some(mut menu) = close_ref.upgrade() {
                        menu.close_message_box(context);
                    }
                }),
            ])
        });
    });
    let toast = skin::button(context, ButtonRole::Secondary, "Show Toast", |_, context| {
        context.ui_sys().show_toast(format!("A toast on the caption panel: {SAMPLE_TEXT}"));
    });
    let tooltip = skin::button(context, ButtonRole::Secondary, "Tooltip", |_, _| {}).with_tooltip(skin::tooltip(context, format!("A tooltip: {SAMPLE_TEXT}")));

    row().with_widget(popup).with_widget(toast).with_widget(tooltip)
}

fn headings(context: &mut GameUiContext) -> UiWidgetGroup {
    let mut group = UiWidgetGroup::new().with_widget_spacing(Vec2::new(0.0, 4.0));
    for (level, name) in [(HeadingLevel::Dialog, "Dialog"), (HeadingLevel::Popup, "Popup"), (HeadingLevel::Section, "Section")] {
        group.add_widget(skin::heading(context, [UiString::from(format!("{name} heading · Đại Long"))], level));
    }
    group
}

// Every `text` colour of the skin, each in itself.
fn text_colors() -> UiRichText {
    let names: Vec<String> = skin::current().text.keys().map(|name| format!("[{name}]{name}[/]")).collect();
    skin::rich_text(names.join("  ·  ")).with_label("Text colours")
}

// Every panel of the skin, as a swatch: a 9-slice at the swatch's size, or a stretched sprite.
fn panels(context: &mut GameUiContext) -> UiWidgetGroup {
    let names: Vec<String> = skin::current().panels.keys().cloned().collect();
    let swatches = names
        .iter()
        .map(|name| {
            let image = match skin::panel(context, name) {
                SkinPanel::NineSlice { slice, .. } => UiImage::empty(SWATCH_SIZE).with_frame(slice, 0.0),
                SkinPanel::Sprite { texture, .. } => UiImage::from_texture(texture, SWATCH_SIZE, SWATCH_SIZE).with_fit(UiImageFit::Stretch),
            };
            captioned(image.with_label(format!("panel {name}")), name)
        })
        .collect();
    rows(swatches, SWATCHES_PER_ROW).with_widget(centres(context))
}

// The card panel's 9-slice with each centre: stretched, tiled (the last tile cut) and rounded
// (whole tiles), and on a rect smaller than its borders, where the corners shrink to meet.
fn centres(context: &mut GameUiContext) -> UiWidgetGroup {
    let SkinPanel::NineSlice { slice, .. } = skin::panel(context, "card") else {
        return row();
    };
    let [left, top, right, bottom] = slice.border_points();
    let small = Vec2::new(left + right, top + bottom) * 0.75;
    let swatch = |slice: UiNineSlice, size: Vec2, name: &str| captioned(UiImage::empty(size).with_frame(slice, 0.0).with_label(format!("centre {name}")), name);
    row()
        .with_widget(swatch(slice.with_center(UiNineSliceCenter::Stretch), CENTRE_SWATCH_SIZE, "stretch"))
        .with_widget(swatch(slice.with_center(UiNineSliceCenter::Tile), CENTRE_SWATCH_SIZE, "tile"))
        .with_widget(swatch(slice.with_center(UiNineSliceCenter::Round), CENTRE_SWATCH_SIZE, "round"))
        .with_widget(swatch(slice, small, "shrunk"))
}

fn separators(context: &mut GameUiContext) -> UiWidgetGroup {
    let names: Vec<String> = skin::current().separators.keys().cloned().collect();
    let mut group = UiWidgetGroup::new().with_center_horizontally(false).with_widget_spacing(Vec2::new(0.0, 4.0));
    for name in names {
        group.add_widget(caption(&name));
        group.add_widget(skin::separator(context, &name));
    }
    group
}

// Each realm's emblem in each of its states.
fn emblems(context: &mut GameUiContext) -> UiWidgetGroup {
    let mut widgets = Vec::new();
    for realm in Realm::iter() {
        for (state, name) in [(EmblemState::Idle, "idle"), (EmblemState::Locked, "locked"), (EmblemState::Current, "current")] {
            if let Some((texture, size)) = skin::emblem(context, realm, state) {
                let size = Vec2::new(40.0, 40.0 * size.y / size.x.max(1.0));
                widgets.push(UiWidgetImpl::from(UiImage::from_texture(texture, size, size).with_label(format!("emblem {realm} {name}"))));
            }
        }
    }
    rows(widgets, 9)
}

// Every control the skin draws, wired to the gallery's state; then a list long enough to scroll.
fn controls(context: &mut GameUiContext) -> UiWidgetGroup {
    let toggle = skin::toggle(context, "Toggle")
        .on_read_value(|_, _| GALLERY_STATE.toggle)
        .on_update_value(|_, _, value| GALLERY_STATE.as_mut().toggle = value);
    let checkbox = skin::checkbox(context, "")
        .on_read_value(|_, _| GALLERY_STATE.checkbox)
        .on_update_value(|_, _, value| GALLERY_STATE.as_mut().checkbox = value);
    let slider = skin::slider(context, "", 0u32..=100)
        .on_read_value(|_, _| -> u32 { GALLERY_STATE.slider })
        .on_update_value(|_, _, value: u32| GALLERY_STATE.as_mut().slider = value);
    let dropdown = skin::dropdown(context, "", ["Đại Long", "Kambara", "Tampadesa"])
        .on_get_current_selection(|_, _| GALLERY_STATE.dropdown)
        .on_selection_changed(|dropdown, _| GALLERY_STATE.as_mut().dropdown = dropdown.current_selection_index());
    let steps = skin::step_selector("Steps", ["Very Easy", "Easy", "Normal", "Hard", "Very Hard"].map(UiString::from))
        .with_current(GALLERY_STATE.steps)
        .on_changed(|selector, _, _| GALLERY_STATE.as_mut().steps = selector.current());
    let arrows = skin::step_selector("Arrows", ["Slow", "Normal", "Fast"].map(UiString::from)).with_style(UiStepSelectorStyle::Arrows).with_wrap(true);

    let list_rows = (1..=8).map(|index| UiListRow::new(format!("Row {index}")).with_subtitle("A list row on the row panel").with_right(format!("{index}0"), None)).collect();
    let list = UiItemList::new()
        .with_label("List")
        .with_size(Vec2::new(0.0, 120.0))
        .with_flags(UiItemListFlags::Scrollable)
        .with_row_look(skin::list_row_look(context))
        .with_rows(list_rows)
        .with_current_item(1);

    // Template rows with an icon each, the realms' emblems; the locked ones disabled.
    let realm_rows = Realm::iter()
        .map(|realm| {
            let current = realm == Realm::DaiLong;
            let state = if current { EmblemState::Current } else { EmblemState::Locked };
            let row = UiListRow::new(realm.display_name()).with_subtitle(if current { "Unlocked" } else { "Locked" }).with_enabled(current);
            match skin::emblem(context, realm, state) {
                Some((texture, _)) => row.with_icon(texture),
                None => row,
            }
        })
        .collect();
    let realm_list = UiItemList::new()
        .with_label("Realm List")
        .with_size(Vec2::new(0.0, 150.0))
        .with_flags(UiItemListFlags::Scrollable)
        .with_row_look(UiListRowLook { icon_size: 30.0, ..skin::list_row_look(context) })
        .with_rows(realm_rows)
        .with_current_item(0);

    // Labeled as the settings dialogs label them.
    let labeled = UiLabeledWidgetGroup::new()
        .with_label_spacing(12.0)
        .with_widget_spacing(8.0)
        .with_widget("Checkbox", checkbox)
        .with_widget("Slider", slider)
        .with_widget("Dropdown", dropdown);

    UiWidgetGroup::new()
        .with_center_horizontally(false)
        .with_widget_spacing(Vec2::new(0.0, 8.0))
        .with_widget(toggle)
        .with_widget(labeled)
        .with_widget(steps)
        .with_widget(arrows)
        .with_widget(list)
        .with_widget(realm_list)
}

// A button of every role, and disabled ones; the icon buttons; the HUD's sprite buttons.
fn buttons(context: &mut GameUiContext) -> UiWidgetGroup {
    let framed = row()
        .with_widget(skin::button(context, ButtonRole::Primary, "Primary", |_, _| {}).with_tooltip(skin::tooltip(context, "The primary role")))
        .with_widget(skin::button(context, ButtonRole::Secondary, "Secondary", |_, _| {}))
        .with_widget(skin::button(context, ButtonRole::Primary, "Disabled", |_, _| {}).with_enabled(false))
        .with_widget(skin::button(context, ButtonRole::Secondary, "Fixed Width", |_, _| {}).with_width(UiTextButtonWidth::Fixed(FIXED_BUTTON_WIDTH)))
        .with_widget(skin::button(context, ButtonRole::Primary, "Min", |_, _| {}).with_width(UiTextButtonWidth::Min(MIN_BUTTON_WIDTH)));
    let tabs = row()
        .with_widget(skin::button(context, ButtonRole::Tab, "Tab", |_, _| {}))
        .with_widget(skin::button(context, ButtonRole::TabSelected, "Selected Tab", |_, _| {}))
        .with_widget(skin::button(context, ButtonRole::HudItem, "HUD Item", |_, _| {}));
    let items = UiWidgetGroup::new()
        .with_center_horizontally(false)
        .with_widget(skin::menu_item(context, "Menu Item", Some(UiString::from("With a subline"))))
        .with_widget(skin::menu_item(context, "Disabled Item", None).with_enabled(false));

    let icon_names: Vec<String> = skin::current().icons.keys().cloned().collect();
    let mut icons = row();
    for (index, name) in icon_names.iter().enumerate() {
        let button = skin::icon_button(context, &format!("icon {name}"), name).with_toggle(true).with_selected(index == 0);
        icons.add_widget(button.with_tooltip(skin::tooltip_beside(context, name.clone())));
    }
    icons.add_widget(skin::icon_button(context, "icon disabled", icon_names.first().map_or("", String::as_str)).with_enabled(false));

    let mut sprites = row();
    // A sample button of each set the gallery knows a sprite of.
    const SAMPLE_SPRITES: [(&str, &str); 2] = [("menu_bar", "main_menu"), ("palette", "housing")];
    for (set, name) in SAMPLE_SPRITES.into_iter().filter(|(set, _)| skin::current().icon_sets.contains_key(*set)) {
        sprites.add_widget(skin::sprite_button(context, set, name, Vec2::new(40.0, 40.0)).with_tooltip(skin::tooltip(context, format!("{set}/{name}"))));
    }

    UiWidgetGroup::new()
        .with_center_horizontally(false)
        .with_widget_spacing(Vec2::new(0.0, 8.0))
        .with_widget(framed)
        .with_widget(tabs)
        .with_widget(items)
        .with_widget(icons)
        .with_widget(sprites)
}

// Each framed role's four state sprites, as 9-slices, and its selected frame if it has one (the
// icon button's): every role of the skin's with a `sprite`.
fn button_states(context: &mut GameUiContext) -> UiWidgetGroup {
    let mut group = UiWidgetGroup::new().with_center_horizontally(false).with_widget_spacing(Vec2::new(0.0, 6.0));
    for (role, def) in skin::current().buttons.clone() {
        let Some(stem) = &def.sprite else {
            continue;
        };
        let states = ["idle", "hovered", "pressed", "disabled"].map(|state| (state, format!("{stem}_{state}.png"), def.slice));
        let selected = def.selected.clone().map(|path| ("selected", path, def.selected_slice.unwrap_or(def.slice)));
        let mut line = row();
        for (state, path, slice_px) in states.into_iter().chain(selected) {
            let slice = skin::nine_slice(context, &path, slice_px, def.center);
            line.add_widget(captioned(UiImage::empty(STATE_SWATCH_SIZE).with_frame(slice, 0.0).with_label(format!("{role} {state}")), state));
        }
        group.add_widget(caption(&role));
        group.add_widget(line);
    }
    group
}

// The data widgets in the skin's colours.
fn data(context: &mut GameUiContext) -> UiWidgetGroup {
    let pairs = skin::key_values().with_label("Key Values").with_pair("Population", "1,240").with_pair("Treasury", "2,315 gold").with_pair("Season", "Wet");

    let table = skin::table()
        .with_label("Table")
        .with_status_column(skin::table_status_look())
        .with_column(UiTableColumn::new("Goal", UiColumnWidth::Fraction(2.0), UiColumnAlign::Left))
        .with_column(UiTableColumn::new("Target", UiColumnWidth::Fixed(70.0), UiColumnAlign::Right))
        .with_column(UiTableColumn::new("Achieved", UiColumnWidth::Fixed(80.0), UiColumnAlign::Right))
        .with_row(UiTableRow::new(["Population", "1,200", "1,240"]).with_status(UiTableStatus::Done))
        .with_row(UiTableRow::new(["Culture", "40", "31"]).with_status(UiTableStatus::Pending))
        .with_row(UiTableRow::new(["Debt-free months", "12", "0"]).with_status(UiTableStatus::Failed));

    let meters = row()
        .with_widget(skin::meter(UiMeterKind::Bar, Vec2::new(150.0, 14.0)).with_label("Bar").with_value(0.62, 1.0).with_goal(0.8))
        .with_widget(skin::meter(UiMeterKind::Ring, Vec2::new(48.0, 48.0)).with_label("Ring").with_value(62.0, 100.0))
        .with_widget(skin::meter(UiMeterKind::Steps(5), Vec2::new(110.0, 12.0)).with_label("Step Meter").with_value(3.0, 5.0));

    let tabs = skin::tab_bar(context)
        .with_label("Tabs")
        .with_tab_contents("First", skin::rich_text("The first tab's *contents*.").with_label("First text"))
        .with_tab_contents("Second", skin::rich_text("The [accent]second[/] tab's.").with_label("Second text"));

    // The home backdrop, framed in the well panel as a map preview would be.
    let backdrop = skin::home_backdrop().0.first().cloned();
    let preview = backdrop.as_deref().map(|path| {
        let (texture, texture_size) = skin::sprite_and_size(context, path);
        let image = UiImage::from_texture(texture, texture_size, Vec2::new(0.0, 90.0)).with_label("Framed Image").with_fit(UiImageFit::Cover);
        skin::framed_image(context, image, "well")
    });

    // The key values beside images in each fit, in two columns: a wide image contained (bands
    // above and below), stretched and covering a square, and an emblem contained.
    let mut images = row();
    if let Some(path) = backdrop.as_deref() {
        let (texture, texture_size) = skin::sprite_and_size(context, path);
        for (fit, name) in [(UiImageFit::Contain, "Contain"), (UiImageFit::Stretch, "Stretch"), (UiImageFit::Cover, "Cover")] {
            let image = UiImage::from_texture(texture, texture_size, Vec2::new(60.0, 60.0)).with_fit(fit).with_label(format!("Image {name}"));
            images.add_widget(captioned(image, name));
        }
    }
    if let Some((texture, size)) = skin::emblem(context, Realm::DaiLong, EmblemState::Idle) {
        images.add_widget(captioned(UiImage::from_texture(texture, size, Vec2::new(60.0, 60.0)).with_label("Image Emblem"), "Emblem"));
    }
    let columns = UiColumns::new()
        .with_spacing(18.0)
        .with_column(UiColumnWidth::Fraction(1.0), UiWidgetGroup::new().with_center_horizontally(false).with_widget(pairs))
        .with_column(UiColumnWidth::Fraction(1.0), UiWidgetGroup::new().with_center_horizontally(false).with_widget(images));

    let mut group = UiWidgetGroup::new()
        .with_center_horizontally(false)
        .with_widget_spacing(Vec2::new(0.0, 8.0))
        .with_widget(tabs)
        .with_widget(columns)
        .with_widget(table)
        .with_widget(meters)
        .with_widget(skin::rich_text(format!("Rich text: *emphasis*, [accent]accent[/], [sub]sub[/]. {SAMPLE_TEXT}")).with_label("Rich Text"));
    if let Some(preview) = preview {
        group.add_widget(preview);
    }
    group
}
