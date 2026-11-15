// The in-game inspector (docs/plans/realm-ui.md, Phase 8): a panel for what's clicked, as
// inspector-<kind>-<realm>.jpg lays it out, but centred on the screen under the top bar, clear
// of the build column at either side, on the dialogs' panel. crate::inspect reads the report;
// paint.rs paints its sections; this module builds the panel's widgets around them: the header (icon tile, name, close tile),
// the painted runs, a paragraph's text, a store's goods with their orders, and the buttons.
//
// The panel pauses the simulation while it's open, as it always has. It reads its report again
// every few frames and is built again when the report changes (an order set, a site finished),
// and closes once what it shows is gone. The buttons' actions run after it draws (`Command`s),
// never from inside its own drawing.

mod paint;

use std::{cell::RefCell, rc::Rc};

use common::{
    Vec2,
    mem::{RcMut, WeakMut, WeakRef},
};
use engine::ui::{
    UiFontScale, UiString,
    text::UiTextCategory,
    widgets::*,
};

use self::paint::{Icons, Item};
use super::layout;
use crate::{
    building::BuildingKind,
    inspect::{self, ActionKind, InspectIcon, InspectReport, InspectTarget, PanelKind, Section, StorageOrder},
    menu::{
        TileInspector, TilePlacement,
        dialog::city_book::{BookColors, Paint},
        skin::{self, ButtonRole, HudGlyphGroup, HudTileKind},
    },
    realm,
    sim::resources::ResourceKind,
    ui_context::GameUiContext,
};

// ----------------------------------------------
// Layout
// ----------------------------------------------

// The dialogs' frame: the HUD's large panel, its insets clear of each realm's foot ornaments.
const PANEL: &str = "dialog";
const TOP: f32 = 130.0; // Under the top bar, room for a large panel's crest.
const BOTTOM: f32 = 24.0; // Room under it, for the panel's foot ornaments.
const MIN_SCALE: f32 = 0.45;

// The mockup's content widths, px: the panel (500 or 620) less 28 a side.
const CONTENT: f32 = 444.0;
const CONTENT_WIDE: f32 = 564.0;

const ICON: f32 = 76.0;
const CLOSE: f32 = 36.0;
const HEADER: f32 = 86.0; // The header's height.
const TEXT_GAP: f32 = 12.0; // Under a paragraph.
const STORE_ROW: f32 = 32.0;
const ORDER_WIDTH: f32 = 102.0;
const ACCEPT_BUTTON: Vec2 = Vec2::new(150.0, 40.0);
const BUTTON: Vec2 = Vec2::new(210.0, 56.0);
const BUTTON_WIDE: f32 = 240.0;
const BUTTONS_GAP: f32 = 18.0; // Above the buttons.
const CHECK_EVERY_FRAMES: u32 = 15;
const HEIGHT_SLACK: f32 = 16.0;

// How often the panel reads its report again while open.

// ----------------------------------------------
// TileInspectorMenu
// ----------------------------------------------

pub struct TileInspectorMenu {
    panel: Option<Panel>,
    commands: Commands,
}

pub type TileInspectorMenuRcMut = RcMut<TileInspectorMenu>;
pub type TileInspectorMenuWeakMut = WeakMut<TileInspectorMenu>;
pub type TileInspectorMenuWeakRef = WeakRef<TileInspectorMenu>;

// What the panel's widgets ask for, run after it draws.
#[derive(Copy, Clone, Debug)]
enum Command {
    Close,
    Action(ActionKind),
    ConfirmDemolish,
    CancelPopup,
    SetOrder(ResourceKind, StorageOrder),
    SetAllOrders(StorageOrder),
}

type Commands = Rc<RefCell<Vec<Command>>>;

struct Panel {
    window: Vec2, // The window it was laid out for: built again when it changes.
    target: InspectTarget,
    report: InspectReport,
    menu: UiMenuRcMut,
    frames: u32,
}

impl TileInspector for TileInspectorMenu {
    fn open(&mut self, context: &mut GameUiContext) {
        self.close(context);
        let realm = realm::ui_realm_for(false).realm;
        let cell = context.tile_selection.last_cell();
        let report = {
            let sim = context.new_sim_context();
            let Some(tile) = sim.tile_map().topmost_tile_at_cell(cell) else {
                return;
            };
            inspect::inspect_tile(&sim, tile, realm)
        };
        if let Some(report) = report {
            self.show(context, report);
        }
    }

    fn close(&mut self, context: &mut GameUiContext) {
        if let Some(mut panel) = self.panel.take() {
            panel.menu.close(context);
        }
        self.commands.borrow_mut().clear();
    }

    fn on_escape(&mut self, context: &mut GameUiContext) {
        match &mut self.panel {
            Some(panel) if panel.menu.is_message_box_open() => panel.menu.close_message_box(context),
            _ => self.close(context),
        }
    }
}

fn window_size(context: &GameUiContext) -> Vec2 {
    Vec2::new(context.viewport_size.width as f32, context.viewport_size.height as f32)
}

impl TileInspectorMenu {
    pub fn new(_context: &mut GameUiContext) -> TileInspectorMenuRcMut {
        TileInspectorMenuRcMut::new(Self { panel: None, commands: Rc::new(RefCell::new(Vec::new())) })
    }

    // The report the panel shows, if it's open (automation).
    pub fn report(&self) -> Option<&InspectReport> {
        self.panel.as_ref().map(|panel| &panel.report)
    }

    pub fn draw(&mut self, context: &mut GameUiContext) {
        let Some(panel) = &mut self.panel else {
            return;
        };

        // Laid out again for a new window size.
        if panel.window != window_size(context) && !panel.menu.is_message_box_open() {
            let report = panel.report.clone();
            if let Some(mut panel) = self.panel.take() {
                panel.menu.close(context);
            }
            self.show(context, report);
        }
        let Some(panel) = &mut self.panel else {
            return;
        };

        // Read again now and then: an order set, a site finished, a building gone.
        panel.frames += 1;
        if panel.frames >= CHECK_EVERY_FRAMES {
            panel.frames = 0;
            self.refresh(context);
        }

        if let Some(panel) = &mut self.panel {
            panel.menu.draw(context);
            if !panel.menu.is_open() {
                self.close(context);
            }
        }

        let commands: Vec<Command> = self.commands.borrow_mut().drain(..).collect();
        for command in commands {
            self.run(context, command);
        }
    }

    fn show(&mut self, context: &mut GameUiContext, report: InspectReport) {
        let mut menu = build(context, &report, &self.commands);
        menu.open(context);
        let window = window_size(context);
        self.panel = Some(Panel { window, target: report.target, report, menu, frames: 0 });
    }

    // Reads the report again; builds the panel again if it changed, closes it if what it shows is
    // gone.
    fn refresh(&mut self, context: &mut GameUiContext) {
        let Some(panel) = &self.panel else {
            return;
        };
        let target = panel.target;
        let realm = realm::ui_realm_for(false).realm;
        let report = {
            let sim = context.new_sim_context();
            inspect::inspect_target(&sim, &target, realm)
        };
        match report {
            None => self.close(context),
            Some(report) if Some(&report) != self.report() => {
                let popup_open = self.panel.as_ref().is_some_and(|panel| panel.menu.is_message_box_open());
                if popup_open {
                    return; // Not under an open popup: after it closes.
                }
                if let Some(mut panel) = self.panel.take() {
                    panel.menu.close(context);
                }
                self.show(context, report);
            }
            Some(_) => {}
        }
    }

    fn run(&mut self, context: &mut GameUiContext, command: Command) {
        let Some(panel) = &mut self.panel else {
            return;
        };
        let target = panel.target;
        match command {
            Command::Close => self.close(context),
            Command::Action(ActionKind::Demolish) => {
                let name = panel.report.name.clone();
                let commands = self.commands.clone();
                panel.menu.reset_message_box();
                panel.menu.open_message_box(context, move |context| demolish_popup(context, &name, &commands));
            }
            Command::CancelPopup => panel.menu.close_message_box(context),
            Command::ConfirmDemolish | Command::Action(ActionKind::CancelSite) | Command::Action(ActionKind::Clear) => {
                panel.menu.close_message_box(context);
                clear(context, target);
                self.close(context);
            }
            Command::Action(ActionKind::ShowHome) => {
                let home = {
                    let sim = context.new_sim_context();
                    match target {
                        InspectTarget::Unit { id, .. } => sim.find_unit(id).and_then(|unit| inspect::unit_home_cell(&sim, unit, realm::ui_realm_for(false).realm)),
                        _ => None,
                    }
                };
                if let Some(cell) = home {
                    context.camera.teleport(cell);
                }
                self.close(context);
            }
            Command::Action(ActionKind::PauseWork) => {} // Disabled until M11's labour.
            Command::SetOrder(kind, order) => {
                set_orders(context, target, kind, order);
                self.refresh(context);
            }
            Command::SetAllOrders(order) => {
                set_orders(context, target, ResourceKind::all(), order);
                self.refresh(context);
            }
        }
    }
}

// Clears what the panel shows through the player's own path: a building demolished (nothing
// given back), a site cancelled (all it took given back), a prop or a road cleared.
fn clear(context: &mut GameUiContext, target: InspectTarget) {
    let sim = context.new_sim_context();
    let tile = match target {
        InspectTarget::Building { handle, .. } => match sim.find_building(handle.kind, handle.id) {
            Some(building) => sim.tile_map().find_tile(building.base_cell(), crate::tile::TileKind::Building),
            None => return,
        },
        // Not a walker passing over it.
        other => inspect::target_tile(&sim, &other),
    };
    if let Some(tile) = tile {
        let _ = TilePlacement::clear(&sim, tile, false, true);
    }
}

// Sets a store's order for each good of `kinds` it takes.
fn set_orders(context: &mut GameUiContext, target: InspectTarget, kinds: ResourceKind, order: StorageOrder) {
    let InspectTarget::Building { handle, .. } = target else {
        return;
    };
    if !handle.kind.intersects(BuildingKind::storage()) {
        return;
    }
    if let Some(building) = context.world.find_building_mut(handle.kind, handle.id) {
        building.as_storage_mut().storage_slots.set_orders(kinds, order);
    }
}

fn demolish_popup(context: &mut dyn UiWidgetContext, name: &str, commands: &Commands) -> UiMessageBox {
    let title = inspect::tf("demolish.title", &[("building", &name.to_lowercase())]);
    let body = [UiString::key(UiTextCategory::Hud, "inspect.demolish.body")];
    let confirm = commands.clone();
    let cancel = commands.clone();
    skin::popup_with_body(context, "Demolish Popup", [UiString::from(title)], body).with_buttons([
        skin::button(context, ButtonRole::Primary, UiString::key(UiTextCategory::Hud, "inspect.action.demolish"), move |_, _| {
            confirm.borrow_mut().push(Command::ConfirmDemolish);
        }),
        skin::button(context, ButtonRole::Secondary, UiString::key(UiTextCategory::Menu, "button.cancel"), move |_, _| {
            cancel.borrow_mut().push(Command::CancelPopup);
        }),
    ])
}

// ----------------------------------------------
// Building the panel
// ----------------------------------------------

// The menu's label: its widgets' paths start with it (`BuildingInspector/Close`).
fn menu_label(panel: PanelKind) -> &'static str {
    match panel {
        PanelKind::Unit => "UnitInspector",
        PanelKind::Prop => "PropInspector",
        PanelKind::Terrain => "TerrainInspector",
        _ => "BuildingInspector",
    }
}

// The parts of the panel under its header, in order.
enum Part {
    Run(Vec<Item>),
    Text(String),
    Store(Section),
}

fn parts(report: &InspectReport) -> Vec<Part> {
    let mut parts = Vec::new();
    let mut run = vec![Item::Status(report.status.clone())];
    for section in &report.sections {
        match section {
            Section::Text { text } => {
                parts.push(Part::Run(std::mem::take(&mut run)));
                parts.push(Part::Text(text.clone()));
            }
            Section::Storage { .. } => {
                parts.push(Part::Run(std::mem::take(&mut run)));
                parts.push(Part::Store(section.clone()));
            }
            _ => run.push(Item::Section(section.clone())),
        }
    }
    parts.push(Part::Run(run));
    parts.retain(|part| !matches!(part, Part::Run(items) if items.is_empty()));
    parts
}

fn insets(context: &mut GameUiContext) -> (f32, f32) {
    match skin::panel(context, PANEL) {
        skin::SkinPanel::NineSlice { content, .. } => (content.left + content.right, content.top + content.bottom),
        skin::SkinPanel::Sprite { .. } => (40.0, 40.0),
    }
}

// Lines a paragraph takes, about: its characters over what a line holds at 18 px.
fn text_lines(text: &str, width: f32) -> f32 {
    let per_line = (width / (18.0 * 0.46)).max(10.0);
    text.split('\n').map(|line| (line.chars().count() as f32 / per_line).ceil().max(1.0)).sum()
}

// The panel's height in mockup px at scale 1, without its insets.
fn content_height(parts: &[Part], width: f32, actions: bool) -> f32 {
    let icons = Icons { population: None };
    let mut height = HEADER;
    for part in parts {
        height += match part {
            Part::Run(items) => paint::flow(None, icons, items, width),
            Part::Text(text) => text_lines(text, width) * 27.0 + TEXT_GAP,
            Part::Store(Section::Storage { rows, .. }) => paint::TOP + 14.0 + rows.len() as f32 * STORE_ROW + ACCEPT_BUTTON.y + 30.0,
            Part::Store(_) => 0.0,
        };
    }
    if actions {
        height += BUTTONS_GAP + BUTTON.y;
    }
    height
}

fn build(context: &mut GameUiContext, report: &InspectReport, commands: &Commands) -> UiMenuRcMut {
    let window = window_size(context);
    let width = if report.wide { CONTENT_WIDE } else { CONTENT };
    let (inset_x, inset_y) = insets(context);
    let column = layout::column_width(context);

    // Its scale: the mockup's size where the window has room, smaller where it hasn't. The
    // estimate is checked against what the widgets measure, and the panel built once more,
    // smaller, if they don't fit.
    let height = content_height(&parts(report), width, !report.actions.is_empty());
    // Centred, so the build column's width is kept clear at both sides.
    let room = Vec2::new(window.x - 2.0 * (column + layout::GAP + layout::MARGIN) - inset_x, window.y - TOP - BOTTOM - inset_y);
    let k = (room.y / height).min(room.x / width).clamp(MIN_SCALE, 1.0);
    let (menu, measured) = build_at(context, report, commands, k);
    if measured <= room.y + 0.5 || k <= MIN_SCALE {
        return menu;
    }
    let k = (k * room.y / measured).clamp(MIN_SCALE, 1.0);
    build_at(context, report, commands, k).0
}

// The panel at scale `k`, and its widgets' height.
fn build_at(context: &mut GameUiContext, report: &InspectReport, commands: &Commands, k: f32) -> (UiMenuRcMut, f32) {
    let window = window_size(context);
    let width = if report.wide { CONTENT_WIDE } else { CONTENT };
    let parts = parts(report);
    let (inset_x, inset_y) = insets(context);
    let height = content_height(&parts, width, !report.actions.is_empty());

    let colors = BookColors::read();
    let icons = Icons { population: Some(skin::hud_icon(context, "population").0) };
    let size = Vec2::new(width * k + inset_x, height * k + inset_y + 4.0);
    // Centred on the screen, between the top bar and the screen's foot.
    let room_height = window.y - TOP - BOTTOM;
    let centred = |height: f32| Vec2::new(((window.x - size.x) * 0.5).round(), (TOP + ((room_height - height) * 0.5).max(0.0)).round());
    let position = centred(size.y);

    let menu = UiMenu::new(menu_label(report.panel))
        // Escape goes through the game's key handler (on_escape), which closes a popup first.
        .with_flags(UiMenuFlags::PauseSimIfOpen | UiMenuFlags::Modal)
        .with_position(position)
        .with_size(size)
        .with_widget_spacing(Vec2::zero());
    let mut menu = skin::panel(context, PANEL).apply_to_menu(menu);

    // Its widgets, measured as they're added: the menu is as tall as they are, up to the room
    // there is (then it scrolls).
    let mut widgets = Added { menu: &mut menu, height: 0.0 };
    let header = header(context, report, width, k, commands);
    widgets.add(context, header);

    for part in parts {
        match part {
            Part::Run(items) => {
                let run_height = paint::flow(None, icons, &items, width);
                let canvas = UiCanvas::new(Vec2::new(width * k, run_height * k), move |_, painter, rect| {
                    let p = Paint::new(painter, rect, k, colors);
                    paint::flow(Some(&p), icons, &items, width);
                });
                widgets.add(context, canvas);
            }
            Part::Text(text) => {
                // The body font at the panel's scale, the player's UI scale taken back out as the
                // painted runs don't grow with it.
                let line = UiText::new(text, text_scale(k)).with_color(colors.text).with_wrap_width(width * k);
                widgets.add(context, UiMenuHeading::new([line]).with_center_horizontally(false));
                widgets.add(context, spacer(width * k, TEXT_GAP * k));
            }
            Part::Store(section) => add_store(context, &mut widgets, &section, width, k, colors, commands),
        }
    }

    if !report.actions.is_empty() {
        widgets.add(context, spacer(width * k, BUTTONS_GAP * k));
        let actions = actions(context, report, width, k, commands);
        widgets.add(context, actions);
    }

    // A little room for the spacing the menu adds round its widgets.
    let measured = widgets.height + HEIGHT_SLACK;
    let height = (measured + inset_y).min(room_height);
    menu.set_size(Vec2::new(size.x, height));
    menu.set_position(centred(height));
    (menu.into_rc(), measured)
}

// The panel's widgets as they're added, and the height they take.
struct Added<'a> {
    menu: &'a mut UiMenu,
    height: f32,
}

impl Added<'_> {
    fn add(&mut self, context: &GameUiContext, widget: impl Into<UiWidgetImpl> + UiWidget) {
        self.height += widget.measure(context).y;
        self.menu.add_widget(widget);
    }
}

// Text in the body font, 18 mockup px at the panel's scale, not grown by the player's UI scale.
fn text_scale(k: f32) -> UiFontScale {
    UiFontScale(0.9 * k / skin::ui_scale().max(0.1))
}

// A button's text at the panel's scale, not grown by the player's UI scale.
fn button_text_scale(k: f32) -> UiFontScale {
    UiFontScale(k / skin::ui_scale().max(0.1))
}

fn spacer(width: f32, height: f32) -> UiCanvas {
    UiCanvas::new(Vec2::new(width, height), |_, _, _| {})
}

// The icon tile, the name and the kind, and the close tile.
fn header(context: &mut GameUiContext, report: &InspectReport, width: f32, k: f32, commands: &Commands) -> UiWidgetGroup {
    let gap = 20.0;
    let mut group = UiWidgetGroup::new().with_stack_vertically(false).with_widget_spacing(Vec2::new(gap * k, 0.0));

    let icon_size = ICON * k;
    match &report.icon {
        InspectIcon::Building(name) | InspectIcon::Sector(name) => {
            let group_kind = if matches!(report.icon, InspectIcon::Building(_)) { HudGlyphGroup::Building } else { HudGlyphGroup::Sector };
            let scale = icon_size / skin::hud_tile_size(HudTileKind::Large).x.max(1.0);
            group = group.with_widget(skin::hud_tile_scaled(context, HudTileKind::Large, scale, "icon", group_kind, name));
        }
        InspectIcon::Sprite => group = group.with_widget(sprite_icon(context, report.target, icon_size)),
    }

    let colors = BookColors::read();
    let name = report.name.clone();
    let subtitle = report.subtitle.clone();
    let title_width = width * k - icon_size - CLOSE * k - 2.0 * gap * k;
    let value = serde_json::json!({ "name": name, "subtitle": subtitle });
    let title = UiCanvas::new(Vec2::new(title_width, HEADER * k), move |_, painter, rect| {
        let p = Paint::new(painter, rect, k, colors);
        p.text(0.0, 36.0, &name, 30.0, p.c.text, engine::ui::UiTextAnchor::Start, true);
        p.text(0.0, 66.0, &subtitle, 18.0, p.c.sub, engine::ui::UiTextAnchor::Start, false);
    })
    .with_label("title", Some(value));
    group = group.with_widget(title);

    let close_commands = commands.clone();
    let close = skin::hud_tile_scaled(context, HudTileKind::Small, k, "Close", HudGlyphGroup::Hud, "close")
        .with_tooltip(skin::tooltip(context, UiString::key(UiTextCategory::Hud, "button.close")))
        .on_pressed(move |_, _| close_commands.borrow_mut().push(Command::Close));
    group.with_widget(close)
}

// A walker's, prop's or terrain's own sprite, fitted in the icon's square.
fn sprite_icon(context: &mut GameUiContext, target: InspectTarget, size: f32) -> UiSpriteIcon {
    let mut icon = UiSpriteIcon::new(Vec2::new(size, size));
    let sprite = {
        let sim = context.new_sim_context();
        match target {
            InspectTarget::Unit { id, .. } => sim.find_unit(id).map(|unit| unit.find_tile(&sim).icon_sprite()),
            other => inspect::target_tile(&sim, &other).map(|tile| tile.icon_sprite()),
        }
    };
    if let Some(sprite) = sprite {
        icon.set_sprite(sprite.tex_info.texture);
        icon.set_tex_coords(sprite.tex_info.coords);
        let natural = sprite.size.to_vec2();
        let fit = (size / natural.x.max(1.0)).min(size / natural.y.max(1.0));
        icon.set_size(natural * fit);
    }
    icon
}

// A store's goods: the title, a row per good with its order, and Accept all and Accept none.
fn add_store(context: &mut GameUiContext, menu: &mut Added, section: &Section, width: f32, k: f32, colors: BookColors, commands: &Commands) {
    let Section::Storage { title, note, rows, footer } = section else {
        return;
    };
    let title = title.clone();
    let note = note.clone();
    menu.add(context, UiCanvas::new(Vec2::new(width * k, (paint::TOP + 14.0) * k), move |_, painter, rect| {
        let p = Paint::new(painter, rect, k, colors);
        p.section(0.0, paint::TOP, &title);
        p.text(width, paint::TOP, &note, 17.0, p.c.sub, engine::ui::UiTextAnchor::End, false);
    }));

    let steps: Vec<UiString> = StorageOrder::ALL.iter().map(|order| UiString::from(order.label())).collect();
    for row in rows {
        let row = row.clone();
        let order_commands = commands.clone();
        let kind = row.kind;
        let current = StorageOrder::ALL.iter().position(|order| *order == row.order).unwrap_or(1);
        let order = skin::step_selector(row.good.clone(), steps.clone())
            .with_style(UiStepSelectorStyle::Arrows)
            .with_wrap(true)
            .with_label_hidden(true)
            .with_width(ORDER_WIDTH * k)
            .with_font_scale(text_scale(k))
            .with_current(current)
            // The callback's index is the step it left: the selector holds the new one.
            .on_changed(move |selector, _, _previous| {
                let order = StorageOrder::ALL[selector.current().min(StorageOrder::ALL.len() - 1)];
                order_commands.borrow_mut().push(Command::SetOrder(kind, order));
            });
        // The row's meter and count take what the selector leaves (it may need more than its
        // column for its arrows and words).
        let order_width = order.measure(context).x.max(ORDER_WIDTH * k);
        let line_width = (width * k - order_width) / k;
        let refused = !row.order.takes_in();
        let (good, have, room) = (row.good.clone(), row.have, row.room);
        let canvas = UiCanvas::new(Vec2::new(line_width * k, STORE_ROW * k), move |_, painter, rect| {
            let p = Paint::new(painter, rect, k, colors);
            let y = 22.0;
            p.painter.rect_filled(p.rect(3.0, y - 11.0, 9.0, 9.0), p.c.accent, 1.0 * k);
            let name_color = if refused { p.c.sub } else { p.c.text };
            p.text(20.0, y, &good, 18.0, name_color, engine::ui::UiTextAnchor::Start, false);
            let ratio = if room > 0 { have as f32 / room as f32 } else { 0.0 };
            p.bar(150.0, y - 11.0, (line_width - 280.0).max(20.0), 9.0, ratio);
            p.text(line_width - 16.0, y, &format!("{have} / {room}"), 17.0, p.c.text, engine::ui::UiTextAnchor::End, false);
        });
        menu.add(context, UiWidgetGroup::new().with_stack_vertically(false).with_widget_spacing(Vec2::zero()).with_widget(canvas).with_widget(order));
    }

    menu.add(context, spacer(width * k, 10.0 * k));
    let all = commands.clone();
    let none = commands.clone();
    let accept_all = skin::button(context, ButtonRole::Secondary, UiString::key(UiTextCategory::Hud, "inspect.action.accept_all"), move |_, _| {
        all.borrow_mut().push(Command::SetAllOrders(StorageOrder::Accept));
    })
    .with_width(UiTextButtonWidth::Fixed(ACCEPT_BUTTON.x * k))
    .with_min_height(ACCEPT_BUTTON.y * k)
    .with_font_scale(button_text_scale(k));
    let accept_none = skin::button(context, ButtonRole::Secondary, UiString::key(UiTextCategory::Hud, "inspect.action.accept_none"), move |_, _| {
        none.borrow_mut().push(Command::SetAllOrders(StorageOrder::Refuse));
    })
    .with_width(UiTextButtonWidth::Fixed(ACCEPT_BUTTON.x * k))
    .with_min_height(ACCEPT_BUTTON.y * k)
    .with_font_scale(button_text_scale(k));
    let footer = footer.clone();
    let footer_width = width - 2.0 * ACCEPT_BUTTON.x - 28.0;
    let footer_canvas = UiCanvas::new(Vec2::new(footer_width * k, ACCEPT_BUTTON.y * k), move |_, painter, rect| {
        let p = Paint::new(painter, rect, k, colors);
        p.text(footer_width, 26.0, &footer, 17.0, p.c.sub, engine::ui::UiTextAnchor::End, false);
    });
    menu.add(
        context,
        UiWidgetGroup::new()
            .with_stack_vertically(false)
            .with_widget_spacing(Vec2::new(14.0 * k, 0.0))
            .with_widget(accept_all)
            .with_widget(accept_none)
            .with_widget(footer_canvas),
    );
    menu.add(context, spacer(width * k, 20.0 * k));
}

// The panel's buttons at its foot: two at its edges, or one at the right.
fn actions(context: &mut GameUiContext, report: &InspectReport, width: f32, k: f32, commands: &Commands) -> UiWidgetGroup {
    let single = report.actions.len() == 1;
    let button_width = if single { BUTTON_WIDE } else { BUTTON.x };
    let mut group = UiWidgetGroup::new().with_stack_vertically(false).with_widget_spacing(Vec2::zero());
    let gap = width - button_width * report.actions.len() as f32;
    if single {
        group = group.with_widget(spacer(gap * k, BUTTON.y * k));
    }
    for (i, action) in report.actions.iter().enumerate() {
        if i > 0 {
            group = group.with_widget(spacer(gap.max(0.0) * k, BUTTON.y * k));
        }
        let pushed = commands.clone();
        let kind = action.kind;
        let mut button = skin::button(context, ButtonRole::Secondary, UiString::from(action.label.clone()), move |_, _| {
            pushed.borrow_mut().push(Command::Action(kind));
        })
        .with_width(UiTextButtonWidth::Fixed(button_width * k))
        .with_min_height(BUTTON.y * k)
        .with_font_scale(button_text_scale(k))
        .with_enabled(action.enabled);
        if let Some(tooltip) = &action.tooltip {
            button = button.with_tooltip(skin::tooltip(context, UiString::from(tooltip.clone())));
        }
        group = group.with_widget(button);
    }
    group
}
