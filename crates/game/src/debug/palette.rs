#![allow(clippy::too_many_arguments)]

use std::collections::HashMap;

use common::{
    self,
    Color,
    Rect,
    RectTexCoords,
    Vec2,
    constants::*,
    coords::{self, WorldToScreenTransform},
};
use engine::{
    app::input::{InputAction, MouseButton},
    file_sys::paths::PathRef,
    render::{debug::DebugDraw, texture::TextureHandle},
    ui::{self, UiCondition, UiInputEvent, UiSystem, UiWindow, widgets::UiWidgetContext},
};
use smallvec::SmallVec;

use crate::{
    building::{BuildingArchetypeKind, config::BuildingConfigs},
    menu::{self, HeightBrush, TilePalette, TilePaletteSelection},
    tile::{
        TileKind,
        TileMapLayerKind,
        elevation::{self, Dir, HeightTool},
        rendering::INVALID_TILE_COLOR,
        sets::{TileCategory, TileDef, TileDefHandle, TileSet, TileSets},
    },
    ui_context::GameUiContext,
    undo_redo,
};

// ----------------------------------------------
// TilePaletteDevMenu
// ----------------------------------------------

#[derive(Default)]
pub struct TilePaletteDevMenu {
    start_open: bool,
    left_mouse_button_pressed: bool,
    current_selection: TilePaletteSelection,
    selected_index: HashMap<TileKind, usize>, // For highlighting the selected button.
    clear_button_image: TextureHandle,
    realm: crate::realm::Realm, // The game's, as of the last draw: the variations it offers.
    variations: Option<VariationPopup>, // The tile whose variations are open beside the palette.
    palette_left: f32,          // The palette window's left edge, as of the last draw.
    height_brush_size: usize,   // The Height section's brush, as an index into HEIGHT_BRUSH_SIZES.
    terrace_level: i32,         // The Height section's Terrace level.
}

// The Height section's tools, in the order their buttons show (docs/plans/terrain-height.md, §8).
const HEIGHT_TOOLS: [HeightTool; 9] = [
    HeightTool::Raise,
    HeightTool::Lower,
    HeightTool::Terrace(1),
    HeightTool::Smooth,
    HeightTool::Flatten,
    HeightTool::Ramp(Dir::N),
    HeightTool::Ramp(Dir::E),
    HeightTool::Ramp(Dir::S),
    HeightTool::Ramp(Dir::W),
];
const HEIGHT_BRUSH_SIZES: [i32; 3] = [1, 3, 5];
const HEIGHT_BRUSH_LABELS: [&str; 3] = ["1 × 1", "3 × 3", "5 × 5"];

// The brush's outline under the cursor.
const HEIGHT_BRUSH_COLOR: Color = Color::new(1.0, 0.85, 0.3, 1.0);

// A tile whose variations the player picks (a shrine's or a temple's patron), its variations
// offered in the game's realm open in a popup window beside the palette, as the in-game build
// palette's variation submenu: which tile, its button, and where its button's top is.
#[derive(Copy, Clone)]
struct VariationPopup {
    handle: TileDefHandle,
    tile_kind: TileKind,
    tile_index: usize,
    button_top: f32, // In window points: the popup's top.
}

impl TilePalette for TilePaletteDevMenu {
    fn on_mouse_button(&mut self, button: MouseButton, action: InputAction) -> UiInputEvent {
        if button == MouseButton::Left {
            if action == InputAction::Press {
                self.left_mouse_button_pressed = true;
            } else if action == InputAction::Release {
                self.left_mouse_button_pressed = false;
                // A height brush's stroke is one undo step.
                menu::end_height_stroke();
            }
            UiInputEvent::Handled
        } else {
            UiInputEvent::NotHandled
        }
    }

    fn on_tile_placement_canceled(&mut self, _context: &mut GameUiContext) {
        // Escape or a right click with a tile's variations open closes them, and only them.
        if self.variations.take().is_some() {
            return;
        }
        self.reset_selection_internal();
    }

    fn clear_current_selection(&mut self, _context: &mut GameUiContext) {
        self.reset_selection_internal();
    }

    fn current_selection(&self) -> TilePaletteSelection {
        self.current_selection
    }

    fn wants_to_place_or_clear_tile(&self) -> bool {
        self.left_mouse_button_pressed && self.has_selection()
    }
}

// The palette's tile buttons, frame included: 4:3, near the buildings' sprites (about 1.3:1), so
// they fill most of it; each sprite keeps its own aspect inside.
const TILE_BUTTON_SIZE: Vec2 = Vec2::new(BASE_TILE_WIDTH_F32, BASE_TILE_WIDTH_F32 * 0.75);

// The clear tool's icon (icons/red_x_icon.png, 819 × 620): the aspect it's drawn at.
const CLEAR_ICON_ASPECT: Vec2 = Vec2::new(819.0, 620.0);

// Between the palette window and its variation popup.
const VARIATION_POPUP_GAP: f32 = 4.0;

impl TilePaletteDevMenu {
    pub fn new(context: &mut GameUiContext) -> Self {
        Self {
            start_open: true,
            clear_button_image: context.load_texture(PathRef::from_str("icons/red_x_icon.png")),
            ..Default::default()
        }
    }

    pub fn draw(&mut self, context: &mut GameUiContext, debug_draw: &mut DebugDraw, show_selection_bounds: bool) {
        let ui_sys = context.ui_sys;
        self.realm = context.sim.realm();

        let tiles_per_row = 2;
        let spacing_between_tiles = 4.0;

        // Its top-right corner against the screen's right edge, under the menu bar.
        let window_position = Vec2::new(ui::screen_size(ui_sys).x, 20.0);

        // UI automation: ui.set on the palette selects a tile by its button label or its
        // tile name ("Small Well" or "small_well"), "Clear", or nothing ("").
        let selection = ui_sys.is_widget_registry_enabled().then(|| self.selection_json());

        let window = UiWindow::new("Tile Selection")
            .record_as("tile_palette", selection)
            .auto_resize()
            .no_resize()
            .collapsed(!self.start_open, UiCondition::FirstUse)
            .position(window_position, UiCondition::FirstUse)
            .right_aligned()
            .item_spacing(Vec2::new(spacing_between_tiles, spacing_between_tiles));

        let shown = ui::window(ui_sys, window, |ui_sys| {
            self.palette_left = ui::window_rect(ui_sys).min.x;

            let set_selection = ui::take_window_set_value(ui_sys);
            let set_selection = set_selection.as_ref().and_then(|value| value.as_str());

            match set_selection {
                Some("") => self.reset_selection_internal(),
                Some(name) if name.eq_ignore_ascii_case("Clear") => {
                    self.reset_selection_internal();
                    self.current_selection = TilePaletteSelection::Clear;
                }
                _ => {}
            }

            ui::text(ui_sys, "Tools");
            {
                let _path = ui::path_scope(ui_sys, "Tools");

                ui::horizontal(ui_sys, |ui_sys| {
                    if ui::icon_button(ui_sys, ui::icons::ICON_UNDO, Some("Undo")) {
                        undo_redo::undo(&context.new_sim_context());
                    }
                    if ui::icon_button(ui_sys, ui::icons::ICON_REDO, Some("Redo")) {
                        undo_redo::redo(&context.new_sim_context());
                    }
                });

                let btn_params = ui::UiImageButtonParams {
                    id: "Clear",
                    size: TILE_BUTTON_SIZE,
                    texture: self.clear_button_image,
                    tooltip: Some("Clear Land"),
                    image_size: Some(CLEAR_ICON_ASPECT),
                    selected: self.current_selection().is_clear(),
                    ..Default::default()
                };

                if ui::image_button(ui_sys, &btn_params) {
                    self.reset_selection_internal();
                    self.current_selection = TilePaletteSelection::Clear;
                }
            }

            self.draw_height_tools(ui_sys);

            let sections = [
                ("Terrain", TileKind::Terrain),
                ("", TileKind::Building | TileKind::Field),
                ("Props", TileKind::Rocks | TileKind::Vegetation),
                ("Units", TileKind::Unit),
            ];

            for (label, tile_kind) in sections {
                let _path = ui::path_scope(ui_sys, if label.is_empty() { "Buildings" } else { label });
                self.draw_tile_list(label, tile_kind, ui_sys, tiles_per_row, spacing_between_tiles, set_selection);
            }
        })
        .is_some();

        // A collapsed palette shows no tile buttons: a tile's variations close with it.
        if !shown {
            self.variations = None;
        }
        self.draw_variations(ui_sys);
        self.draw_selected_tile(context, debug_draw, show_selection_bounds);
    }

    // The Height section (docs/plans/terrain-height.md, §8): a button per tool, the brush's width and
    // the Terrace tool's level. A tool picked is the palette's selection, painted with the left button
    // (TilePlacement::paint_height), one undo step a stroke. While one is picked the map shows its
    // levels (TileMapRenderFlags::DrawElevation).
    fn draw_height_tools(&mut self, ui_sys: &UiSystem) {
        let Some(_path) = ui::collapsing_header_scope(ui_sys, "Height", false) else {
            return;
        };
        let picked = self.current_selection.height_brush().map(|brush| brush.tool);
        let mut new_tool = None;

        for row in HEIGHT_TOOLS.chunks(3) {
            ui::horizontal(ui_sys, |ui_sys| {
                for tool in row {
                    let selected = picked.is_some_and(|picked| {
                        std::mem::discriminant(&picked) == std::mem::discriminant(tool) && Self::same_ramp(picked, *tool)
                    });
                    if ui::selectable_button(ui_sys, tool.name(), selected) {
                        new_tool = Some(*tool);
                    }
                }
            });
        }

        let mut size_changed = ui::combo(ui_sys, "Brush", &mut self.height_brush_size, &HEIGHT_BRUSH_LABELS);
        size_changed |= ui::slider(
            ui_sys,
            "Terrace level",
            0,
            i32::from(elevation::MAX_LEVEL),
            ui::UiNumberFormat::Default,
            &mut self.terrace_level,
        );

        let tool = new_tool.or(if size_changed { picked } else { None });
        if let Some(tool) = tool {
            let tool = match tool {
                HeightTool::Terrace(_) => {
                    HeightTool::Terrace(self.terrace_level.clamp(0, i32::from(elevation::MAX_LEVEL)) as u8)
                }
                other => other,
            };
            if new_tool.is_some() {
                self.reset_selection_internal();
            }
            let size = HEIGHT_BRUSH_SIZES[self.height_brush_size.min(HEIGHT_BRUSH_SIZES.len() - 1)];
            self.current_selection = TilePaletteSelection::Height(HeightBrush { tool, size });
        }
    }

    // Whether two tools are the same ramp direction (or not ramps at all).
    fn same_ramp(a: HeightTool, b: HeightTool) -> bool {
        match (a, b) {
            (HeightTool::Ramp(a), HeightTool::Ramp(b)) => a == b,
            _ => true,
        }
    }

    // The open tile's variations offered in the game's realm, a button each, in a popup window
    // (`Tile Variations`, titled by the tile) left of the palette, level with the tile's button.
    // A button picks the tile and that variation and closes the popup; its close button, Escape or
    // a right click close it without picking (on_tile_placement_canceled).
    fn draw_variations(&mut self, ui_sys: &UiSystem) {
        let Some(popup) = self.variations else {
            return;
        };
        let Some(tile_def) = TileSets::get().handle_to_tile_def(popup.handle) else {
            self.variations = None;
            return;
        };

        let title = format!("{}###tile_variations", common::fixed_string::snake_case_to_title::<64>(&tile_def.name));
        let mut open = true;
        let window = UiWindow::new(&title)
            .name("Tile Variations")
            .open(&mut open)
            .auto_resize()
            .no_resize()
            .no_collapse()
            .fit_on_screen()
            .position(Vec2::new(self.palette_left - VARIATION_POPUP_GAP, popup.button_top), UiCondition::Always)
            .right_aligned();

        let mut picked = None;
        ui::window(ui_sys, window, |ui_sys| {
            for index in tile_def.offered_variations(self.realm) {
                let variation = &tile_def.variations[index];
                if ui::button(ui_sys, &variation.label_text()) {
                    picked = Some(index);
                }
                if let Some(tooltip) = variation.tooltip_text() {
                    ui::item_tooltip_text(ui_sys, tooltip.as_str());
                }
            }
        });

        if let Some(index) = picked {
            self.reset_selection_internal();
            self.current_selection = TilePaletteSelection::Tile(popup.handle, Some(index));
            self.selected_index.insert(popup.tile_kind, popup.tile_index);
        } else if !open {
            self.variations = None;
        }
    }

    fn draw_selected_tile(&self, context: &mut GameUiContext, debug_draw: &mut DebugDraw, show_selection_bounds: bool) {
        if !self.has_selection() {
            return;
        }

        // A height brush: its cells' outlines around the cell under the cursor.
        if let Some(brush) = self.current_selection().height_brush() {
            let transform = context.camera.transform();
            let centre =
                context.tile_map.find_exact_cell_for_point(TileMapLayerKind::Terrain, context.cursor_screen_pos, transform);
            if centre.is_valid() {
                for cell in elevation::brush_cells(context.tile_map.elevation(), brush.tool, centre, brush.size) {
                    let points = coords::cell_to_screen_diamond_points(cell, BASE_TILE_SIZE_I32, transform);
                    for i in 0..points.len() {
                        let (from, to) = (points[i], points[(i + 1) % points.len()]);
                        debug_draw.line_with_thickness(from, to, HEIGHT_BRUSH_COLOR, 2.0);
                    }
                }
            }
            return;
        }

        // Draw clear tile icon under the cursor:
        if self.current_selection().is_clear() {
            const CLEAR_ICON_SIZE: Vec2 = BASE_TILE_SIZE_F32;

            let rect = Rect::from_pos_and_size(context.cursor_screen_pos - (CLEAR_ICON_SIZE * 0.5), CLEAR_ICON_SIZE);

            debug_draw.textured_colored_rect(rect, &RectTexCoords::DEFAULT, self.clear_button_image, Color::white());
        } else {
            let selected_tile = self.current_selection().as_tile_def().unwrap();
            let rect = Rect::from_pos_and_size(context.cursor_screen_pos, selected_tile.draw_size.to_vec2());

            let offset = if selected_tile.is(TileKind::Building | TileKind::Rocks | TileKind::Vegetation) {
                Vec2::new(-(selected_tile.draw_size.width as f32 / 2.0), -(selected_tile.draw_size.height as f32))
            } else {
                Vec2::new(-(selected_tile.draw_size.width as f32 / 2.0), -(selected_tile.draw_size.height as f32 / 2.0))
            };

            let has_valid_placement = context.tile_selection.has_valid_placement();
            let transform = context.camera.transform();

            let cursor_transform = WorldToScreenTransform::new(transform.scaling, offset);
            let highlight_color = if has_valid_placement { Color::white() } else { INVALID_TILE_COLOR };

            let variation = self.current_selection().variation().unwrap_or(0);
            if let Some(sprite_frame) = selected_tile.anim_frame_by_index(variation, 0, 0) {
                let tile_color = Color::new(
                    selected_tile.color.r,
                    selected_tile.color.g,
                    selected_tile.color.b,
                    0.7, // Semi-transparent
                );

                debug_draw.textured_colored_rect(
                    cursor_transform.scale_and_offset_rect(rect),
                    &sprite_frame.tex_info.coords,
                    sprite_frame.tex_info.texture,
                    tile_color * highlight_color,
                );
            }

            if show_selection_bounds {
                debug_draw.wireframe_rect(cursor_transform.scale_and_offset_rect(rect), Color::red());
            }
        }
    }

    // `select`: a tile to select as if its button was clicked, by button label or tile name.
    fn draw_tile_list(
        &mut self,
        label: &str,
        tile_kind: TileKind,
        ui_sys: &UiSystem,
        tiles_per_row: usize,
        padding_between_tiles: f32,
        select: Option<&str>,
    ) {
        if !label.is_empty() {
            ui::text(ui_sys, label);
        }

        let mut tile_defs = SmallVec::<[(&TileSet, &TileCategory, &TileDef, Option<BuildingArchetypeKind>); 32]>::new();

        // Gather relevant tiles:
        TileSets::get().for_each_tile_def(|tile_set, tile_category, tile_def| {
            if tile_def.is(tile_kind) {
                // A farmstead's field goes with the producers, beside the farms.
                let building_archetype = if tile_def.is(TileKind::Field) {
                    Some(BuildingArchetypeKind::ProducerBuilding)
                } else {
                    BuildingConfigs::get().find_building_archetype_kind_for_tile_def(tile_def)
                };

                tile_defs.push((tile_set, tile_category, tile_def, building_archetype));
            }
            true
        });

        // Group by building archetype kind (if any):
        tile_defs.sort_by_key(|entry| {
            if let Some(archetype) = entry.3 {
                match archetype {
                    // Custom sort order.
                    BuildingArchetypeKind::ServiceBuilding => 0,
                    BuildingArchetypeKind::ProducerBuilding => 1,
                    BuildingArchetypeKind::StorageBuilding => 2,
                    BuildingArchetypeKind::HouseBuilding => 3,
                }
            } else {
                4
            }
        });

        // Rows of up to `tiles_per_row` buttons. Each named building group starts on a new row,
        // under its name.
        let mut tile_index = 0;
        let mut row = SmallVec::<[(&TileSet, &TileCategory, &TileDef); 4]>::new();
        let mut prev_building_archetype: Option<BuildingArchetypeKind> = None;

        for (tile_set, tile_category, tile_def, building_archetype) in tile_defs {
            if row.len() == tiles_per_row {
                self.draw_tile_row(ui_sys, tile_kind, &row, &mut tile_index, padding_between_tiles, select);
                row.clear();
            }

            // New named building group?
            if building_archetype != prev_building_archetype {
                if let Some(archetype_kind) = building_archetype {
                    if !row.is_empty() {
                        self.draw_tile_row(ui_sys, tile_kind, &row, &mut tile_index, padding_between_tiles, select);
                        row.clear();
                    }
                    ui::text(ui_sys, Self::building_archetype_kind_label(archetype_kind));
                }
                prev_building_archetype = building_archetype;
            }

            row.push((tile_set, tile_category, tile_def));
        }

        if !row.is_empty() {
            self.draw_tile_row(ui_sys, tile_kind, &row, &mut tile_index, padding_between_tiles, select);
        }
    }

    fn draw_tile_row(
        &mut self,
        ui_sys: &UiSystem,
        tile_kind: TileKind,
        row: &[(&TileSet, &TileCategory, &TileDef)],
        tile_index: &mut usize,
        padding_between_tiles: f32,
        select: Option<&str>,
    ) {
        ui::horizontal_spaced(ui_sys, padding_between_tiles, |ui_sys| {
            for &(tile_set, tile_category, tile_def) in row {
                self.draw_tile_button(ui_sys, tile_kind, *tile_index, tile_set, tile_category, tile_def, select);
                *tile_index += 1;
            }
        });
    }

    fn draw_tile_button(
        &mut self,
        ui_sys: &UiSystem,
        tile_kind: TileKind,
        tile_index: usize,
        tile_set: &TileSet,
        tile_category: &TileCategory,
        tile_def: &TileDef,
        select: Option<&str>,
    ) {
        // Selected, or its variations open.
        let selected = self.selected_index.get(&tile_kind) == Some(&tile_index)
            || self.variations.is_some_and(|popup| popup.tile_kind == tile_kind && popup.tile_index == tile_index);
        let button_top = ui::available_rect(ui_sys).min.y;

        let tile_sprite = tile_def.texture_by_index(0, 0, 0);

        let btn_id = common::fixed_string::snake_case_to_title::<64>(&tile_def.name);
        let btn_tooltip_buf;
        let btn_tooltip = if tile_def.has_cost() {
            btn_tooltip_buf = format!("{}\nCost: {}", btn_id, crate::world::object::tile_cost_text(tile_def));
            btn_tooltip_buf.as_str()
        } else {
            btn_id.as_str()
        };

        // The sprite at the aspect it's drawn at on the map.
        let draw_size = if tile_def.draw_size.is_valid() { tile_def.draw_size } else { tile_def.logical_size };

        let btn_params = ui::UiImageButtonParams {
            id: &btn_id,
            size: TILE_BUTTON_SIZE,
            texture: tile_sprite.texture,
            tooltip: Some(btn_tooltip),
            tint_color: Some(tile_def.color),
            top_left_uvs: Some(tile_sprite.coords.top_left()),
            bottom_right_uvs: Some(tile_sprite.coords.bottom_right()),
            image_size: Some(draw_size.to_vec2()),
            selected,
        };

        let clicked = ui::image_button(ui_sys, &btn_params);
        let selected_by_name =
            select.is_some_and(|name| name.eq_ignore_ascii_case(&btn_id) || name.eq_ignore_ascii_case(&tile_def.name));

        if clicked || selected_by_name {
            let handle = TileDefHandle::new(tile_set, tile_category, tile_def);
            // A tile whose variations the player picks opens them beside the palette, if there's a
            // choice (clicked again, it closes them); others are selected at once, the variation
            // random (or the only one offered).
            let offered = if tile_def.pick_variation { tile_def.offered_variations(self.realm) } else { Default::default() };
            if offered.len() > 1 {
                let reopened = self.variations.is_some_and(|popup| popup.handle == handle);
                self.reset_selection_internal();
                if !reopened {
                    self.variations = Some(VariationPopup { handle, tile_kind, tile_index, button_top });
                }
                return;
            }
            self.reset_selection_internal();
            self.current_selection = TilePaletteSelection::Tile(handle, offered.first().copied());
            self.selected_index.insert(tile_kind, tile_index);
        }
    }

    fn building_archetype_kind_label(archetype_kind: BuildingArchetypeKind) -> &'static str {
        match archetype_kind {
            BuildingArchetypeKind::ProducerBuilding => "Production",
            BuildingArchetypeKind::StorageBuilding => "Storage",
            BuildingArchetypeKind::ServiceBuilding => "Services",
            BuildingArchetypeKind::HouseBuilding => "Houses",
        }
    }

    // The selection as the palette records it for UI automation: the button label, "Clear" or null.
    fn selection_json(&self) -> serde_json::Value {
        if self.current_selection.is_clear() {
            return serde_json::Value::from("Clear");
        }
        if let Some(brush) = self.current_selection.height_brush() {
            return serde_json::Value::from(format!("Height: {}", brush.tool.name()));
        }
        match self.current_selection.as_tile_def() {
            Some(tile_def) => {
                serde_json::Value::from(common::fixed_string::snake_case_to_title::<64>(&tile_def.name).as_str())
            }
            None => serde_json::Value::Null,
        }
    }

    fn reset_selection_internal(&mut self) {
        menu::end_height_stroke();
        self.left_mouse_button_pressed = false;
        self.current_selection = TilePaletteSelection::None;
        self.selected_index.clear();
        self.variations = None;
    }
}
