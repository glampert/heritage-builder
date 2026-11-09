// The minimap's menus and drawing: lens buttons across the top, the map, the tools down the
// right, as in the HUD mockups.

use strum::{EnumCount, EnumIter, IntoEnumIterator};

use common::{Color, Rect, Vec2, coords::Cell, format_fixed_string};
use engine::{
    file_sys::paths::PathRef,
    render::texture::TextureHandle,
    ui::{self, UiCondition, UiFontScale, UiLayer, UiNumberFormat, UiPainter, UiStaticVar, UiSystem, UiTooltipBackground, UiWindow, sound::UiButtonSoundsEnabled, widgets::*},
};

use super::{
    MinimapLens,
    lens::LensData,
    model::MinimapFeature,
    shapes::{self, CellBox, MeshParams},
    style::{self, MinimapStyle},
    widget::{BUTTON_SIZE, GAP, LEGEND_ROW_HEIGHT, MinimapLayout},
};
use crate::{realm::{self, Realm}, ui_context::GameUiContext};

// ----------------------------------------------
// MinimapTool
// ----------------------------------------------

#[derive(Copy, Clone, PartialEq, Eq, Debug, EnumCount, EnumIter)]
pub enum MinimapTool {
    ZoomIn,
    ZoomOut,
    Alerts,
    Home,
    WorldMap,
    Collapse,
}

pub const TOOL_COUNT: usize = MinimapTool::COUNT;

impl MinimapTool {
    pub fn sprite(self) -> &'static str {
        match self {
            Self::ZoomIn => "zoom_in",
            Self::ZoomOut => "zoom_out",
            Self::Alerts => "alerts",
            Self::Home => "home",
            Self::WorldMap => "world_map",
            Self::Collapse => "collapse",
        }
    }

    pub fn tooltip(self) -> &'static str {
        match self {
            Self::ZoomIn => "Zoom in",
            Self::ZoomOut => "Zoom out",
            Self::Alerts => "Next alert",
            Self::Home => "Centre on the palace",
            Self::WorldMap => "World map: coming with trade routes",
            Self::Collapse => "Hide map",
        }
    }
}

// ----------------------------------------------
// MinimapRenderer
// ----------------------------------------------

pub trait MinimapRenderer {
    // Draw the minimap, nestled inside its own UI window.
    fn draw(&mut self, context: &mut GameUiContext);
}

struct MinimapButtons {
    lenses: Vec<(MinimapLens, UiSpriteButton)>,
    tools: Vec<(MinimapTool, UiSpriteButton)>,
    // The building icons toggle: a button per state, the one showing the current state drawn.
    landmarks_on: UiSpriteButton,
    landmarks_off: UiSpriteButton,
    debug: Option<UiSpriteButton>,
}

// ----------------------------------------------
// BaseMinimapRenderer
// ----------------------------------------------

struct BaseMinimapRenderer {
    minimap_menu: Option<UiMenuRcMut>,
    open_button_menu: Option<UiMenuRcMut>,
    buttons: Option<MinimapButtons>,
    tooltip_background: Option<TextureHandle>, // The style's frame, behind the map's hover tooltip.
    style_version: u32,
    button_sounds_enabled: UiButtonSoundsEnabled,
    font_scale: UiFontScale,
    with_frame: bool, // The style's frame sprite; the developer UI uses a plain window.
    with_debug_button: bool,
    apply_widget_clip_rect: bool,
}

impl BaseMinimapRenderer {
    fn new(button_sounds_enabled: UiButtonSoundsEnabled, font_scale: UiFontScale, with_frame: bool, with_debug_button: bool) -> Self {
        Self {
            minimap_menu: None,
            open_button_menu: None,
            buttons: None,
            tooltip_background: None,
            style_version: 0,
            button_sounds_enabled,
            font_scale,
            with_frame,
            with_debug_button,
            apply_widget_clip_rect: true,
        }
    }

    fn create_menus(&mut self, context: &mut GameUiContext) {
        let style = style::current();
        let font_scale = self.font_scale;
        let sounds = self.button_sounds_enabled;
        let frame = self.with_frame.then(|| style.frame.clone());
        let tooltip_frame = self.with_frame.then(|| style.tooltip_frame().to_string());

        let make_menu = |context: &mut GameUiContext, label: &str| match &frame {
            Some(sprite) => UiMenu::new(label).with_background(context, PathRef::from_str(sprite)),
            None => UiMenu::new(label),
        };
        // The buttons' tooltips look like the rest of the HUD's (the developer UI's are plain);
        // the style's tooltip settings are for the map's hover tooltip.
        let with_frame = self.with_frame;
        let make_tooltip = |context: &mut GameUiContext, text: &str| {
            if with_frame { crate::menu::make_tooltip(context, text) } else { UiTooltipText::new(text).with_font_scale(font_scale) }
        };
        let make_button = |context: &mut GameUiContext, sprite: &str, tooltip: &str| {
            let tooltip = make_tooltip(context, tooltip);
            UiSpriteButton::new(context, sprite, Vec2::new(BUTTON_SIZE, BUTTON_SIZE)).with_tooltip(tooltip).with_sounds(sounds)
        };

        // Its size follows the layout every frame (the style's map size, the lens legend).
        let layout_size = context.tile_map.minimap().layout().window.size();
        let minimap_menu = make_menu(context, "Minimap")
            .with_flags(UiMenuFlags::IsOpen | UiMenuFlags::NoTitleBar)
            .with_size(layout_size.max(Vec2::one()))
            .with_content_insets(UiMenuInsets::same(super::widget::PADDING))
            .with_position(UiMenuPosition::callback(|_menu, context| {
                ui::widgets::context_as_mut::<GameUiContext>(context).tile_map.minimap().layout().window.min
            }))
            .into_rc();

        // Unlabeled: its button is named "open" in widget paths. In the bottom left corner, where
        // the minimap's tools were.
        const OPEN_MENU_PADDING: f32 = 6.0;
        let open_menu_size = Vec2::new(BUTTON_SIZE + OPEN_MENU_PADDING * 2.0, BUTTON_SIZE + OPEN_MENU_PADDING * 2.0);
        let mut open_button_menu = make_menu(context, "")
            .with_size(open_menu_size)
            .with_content_insets(UiMenuInsets::same(OPEN_MENU_PADDING))
            .with_position(UiMenuPosition::callback(move |_menu, context| {
                Vec2::new(0.0, ui::screen_size(context.ui_sys()).y - open_menu_size.y)
            }))
            .into_rc();
        let open_button = make_button(context, &format!("{}/open", style.buttons), "Open map").on_state_changed(|button, context, _| {
            if button.is_pressed() {
                ui::widgets::context_as_mut::<GameUiContext>(context).tile_map.minimap_mut().set_open(true);
                button.press(false); // Single click button.
            }
        });
        open_button_menu.add_widget(open_button);

        let lenses = MinimapLens::available()
            .iter()
            .map(|lens| (*lens, make_button(context, &format!("{}/{}", style.buttons, lens.sprite()), lens.label())))
            .collect();

        let tools = MinimapTool::iter()
            .map(|tool| {
                let mut button = make_button(context, &format!("{}/{}", style.buttons, tool.sprite()), tool.tooltip());
                if tool == MinimapTool::WorldMap {
                    button.enable(false); // Until M10 brings the world map.
                }
                (tool, button)
            })
            .collect();

        let debug = self.with_debug_button.then(|| make_button(context, &format!("{}/debug", style.buttons), "Debug"));
        let landmarks_on = make_button(context, &format!("{}/landmarks_on", style.buttons), "Hide building icons");
        let landmarks_off = make_button(context, &format!("{}/landmarks_off", style.buttons), "Show building icons");
        self.tooltip_background = tooltip_frame.as_ref().map(|sprite| context.load_texture(PathRef::from_str(sprite)));

        self.minimap_menu = Some(minimap_menu);
        self.open_button_menu = Some(open_button_menu);
        self.buttons = Some(MinimapButtons { lenses, tools, landmarks_on, landmarks_off, debug });
    }

    fn draw<F>(&mut self, context: &mut GameUiContext, custom_draw_fn: F)
    where
        F: FnOnce(&mut GameUiContext),
    {
        let style_version = style::version();
        if self.minimap_menu.is_none() || self.style_version != style_version {
            self.create_menus(context);
            self.style_version = style_version;
        }

        // The minimap's open state is the truth; the menus follow it.
        let is_open = context.tile_map.minimap().is_open();
        let mut minimap_menu = self.minimap_menu.as_ref().unwrap().clone();
        let mut open_button_menu = self.open_button_menu.as_ref().unwrap().clone();
        if minimap_menu.is_open() != is_open {
            if is_open {
                minimap_menu.open(context);
                open_button_menu.close(context);
            } else {
                minimap_menu.close(context);
                open_button_menu.open(context);
            }
        }

        if !is_open {
            open_button_menu.draw(context);
            return;
        }

        // Lenses: recompute while one is open.
        let delta_time_secs = context.delta_time_secs;
        if context.tile_map.minimap_mut().lens_refresh_due(delta_time_secs) {
            let sim_context = context.sim.new_sim_context(0.0, context.tile_map, context.world);
            context.tile_map.minimap_mut().refresh_lens(&sim_context);
        }

        // The active lens's button shows pressed, and the alerts tool is off without alerts.
        let current_lens = context.tile_map.minimap().lens();
        let has_alerts = context.tile_map.minimap().alert_count() > 0;
        let buttons = self.buttons.as_mut().unwrap();
        for (lens, button) in &mut buttons.lenses {
            button.press(*lens == current_lens);
        }
        for (tool, button) in &mut buttons.tools {
            if *tool == MinimapTool::Alerts {
                button.enable(has_alerts);
            }
        }

        minimap_menu.set_size(context.tile_map.minimap().layout().window.size());

        let flags = minimap_menu.flags();
        minimap_menu.draw_custom(context, flags, UiMenu::close, UiMenu::message_box, |_menu, context| {
            let game_context = ui::widgets::context_as_mut::<GameUiContext>(context);
            self.draw_contents(game_context);
            custom_draw_fn(game_context);
        });

        self.handle_buttons(context);
    }

    fn draw_contents(&mut self, context: &mut GameUiContext) {
        let ui_sys = context.ui_sys;
        let layout = *context.tile_map.minimap().layout();
        let buttons = self.buttons.as_mut().unwrap();

        // The top row, right-aligned against the tool column. From right to left: the lenses (City
        // first), the building icons toggle, then the debug toggle in the developer UI.
        let right_slot = |slot: usize| {
            let x = layout.lens_row.max.x - BUTTON_SIZE - slot as f32 * (BUTTON_SIZE + GAP);
            Rect::from_pos_and_size(Vec2::new(x, layout.lens_row.min.y), Vec2::new(BUTTON_SIZE, BUTTON_SIZE))
        };

        let mut slot = 0;
        for (_, button) in &mut buttons.lenses {
            ui::place_at(ui_sys, right_slot(slot), true, GAP, |_| button.draw(context));
            slot += 1;
        }

        let toggle = if context.tile_map.minimap().shows_landmarks() { &mut buttons.landmarks_on } else { &mut buttons.landmarks_off };
        ui::place_at(ui_sys, right_slot(slot), true, GAP, |_| toggle.draw(context));
        slot += 1;

        if let Some(debug) = &mut buttons.debug {
            ui::place_at(ui_sys, right_slot(slot), true, GAP, |_| debug.draw(context));
        }

        ui::place_at(ui_sys, layout.tools, false, layout.tool_spacing, |_| {
            for (_, button) in &mut buttons.tools {
                button.draw(context);
            }
        });

        self.paint_map(context, &layout);
        self.paint_badge(context);
        self.hover_tooltip(context, &layout);
    }

    fn handle_buttons(&mut self, context: &mut GameUiContext) {
        let buttons = self.buttons.as_mut().unwrap();
        let minimap = context.tile_map.minimap_mut();

        // A newly pressed lens: the current one's button shows pressed too.
        let current = minimap.lens();
        if let Some((lens, _)) = buttons.lenses.iter().find(|(lens, button)| button.is_pressed() && *lens != current) {
            minimap.set_lens(*lens);
        }

        for (tool, button) in &mut buttons.tools {
            if !button.is_pressed() {
                continue;
            }
            button.press(false); // Single click buttons.

            match tool {
                MinimapTool::ZoomIn => minimap.view_mut().zoom_by(1),
                MinimapTool::ZoomOut => minimap.view_mut().zoom_by(-1),
                MinimapTool::Alerts => {
                    if let Some(cell) = minimap.next_alert() {
                        context.camera.teleport(cell);
                    }
                }
                MinimapTool::Home => match minimap.seat_cell() {
                    Some(cell) => {
                        context.camera.teleport(cell);
                    }
                    None => context.camera.center(),
                },
                MinimapTool::WorldMap => {}
                MinimapTool::Collapse => {
                    minimap.set_open(false);
                    minimap.view_mut().auto_zoom = true; // Reopening resets the zoom.
                }
            }
        }

        for (button, show) in [(&mut buttons.landmarks_on, false), (&mut buttons.landmarks_off, true)] {
            if button.is_pressed() {
                button.press(false);
                minimap.set_show_landmarks(show);
            }
        }

        if let Some(debug) = &mut buttons.debug
            && debug.is_pressed()
        {
            debug.press(false);
            SHOW_MINIMAP_DEBUG_CONTROLS.set(!*SHOW_MINIMAP_DEBUG_CONTROLS);
        }
    }

    // ----------------------
    // Painting:
    // ----------------------

    fn paint_map(&self, context: &mut GameUiContext, layout: &MinimapLayout) {
        let ui_sys = context.ui_sys;
        let minimap = context.tile_map.minimap_mut();
        let style = style::current();
        minimap.shapes.refresh(&minimap.features, &minimap.grid, style, style::version());

        let view = minimap.view();
        if !view.is_valid() || !minimap.base.handle.is_valid() {
            return;
        }

        let transform = view.cell_to_minimap();
        let size = minimap.size_in_cells();
        let (w, h) = (size.width as f32, size.height as f32);
        let corners = [transform.apply(-0.5, -0.5), transform.apply(w - 0.5, -0.5), transform.apply(w - 0.5, h - 0.5), transform.apply(-0.5, h - 0.5)];
        let uvs = [Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0), Vec2::new(1.0, 1.0), Vec2::new(0.0, 1.0)];

        // The cells on the map area, for culling.
        let visible = {
            let points = layout.map.corners_ccw().map(|p| transform.inverse(p));
            let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
            for p in points {
                x0 = x0.min(p.x);
                y0 = y0.min(p.y);
                x1 = x1.max(p.x);
                y1 = y1.max(p.y);
            }
            CellBox { x0: x0 - 1.0, y0: y0 - 1.0, x1: x1 + 1.0, y1: y1 + 1.0 }
        };

        let lens_on = minimap.lens() != MinimapLens::City;
        let lens_data: &LensData = &minimap.lens_data;
        let lens_fn = |feature: &MinimapFeature| lens_data.building_color(feature.base_cell, feature.sector, style);

        let meshes = shapes::build_meshes(&minimap.shapes, &MeshParams {
            transform,
            visible,
            pixels_per_point: ui::pixels_per_point(ui_sys),
            style,
            field_texture: minimap.textures.field,
            lens: if lens_on { Some(&lens_fn) } else { None },
        });

        let painter = ui::painter(ui_sys, UiLayer::Window);
        let clip = if self.apply_widget_clip_rect { layout.map } else { Rect::from_pos_and_size(Vec2::zero(), ui::screen_size(ui_sys)) };

        painter.clipped(clip, |painter| {
            painter.rect_filled(layout.map, style.palette.background.color(), 0.0);
            painter.image_quad(minimap.base.handle, corners, uvs, Color::white());

            if lens_on {
                painter.rect_filled(layout.map, style.lenses.veil.color(), 0.0);
                if lens_data.has_heat_field() && minimap.lens_texture.handle.is_valid() {
                    painter.image_quad(minimap.lens_texture.handle, corners, uvs, Color::white());
                }
            }

            painter.mesh(meshes.shadows);
            painter.mesh(meshes.fields);
            painter.mesh(meshes.top);

            if !lens_on && minimap.shows_landmarks() {
                Self::paint_landmarks(painter, minimap, style, &transform);
            }
            Self::paint_camera(painter, view.camera_rect, style);
            Self::paint_pings(painter, minimap, style, &transform, layout.map);
        });

        painter.rect_stroke(layout.map, 0.0, 1.0, style.palette.outline.color());

        if lens_on {
            // On the developer UI's dark window, the legend is white.
            let text_color = if self.with_frame { style.legend.text.color() } else { Color::white() };
            Self::paint_legend(&painter, &lens_data.legend, layout, style, text_color);
        }
    }

    fn paint_landmarks(painter: &UiPainter, minimap: &super::Minimap, style: &MinimapStyle, transform: &super::widget::CellToMinimap) {
        if transform.cell_side() < style.landmark.min_cell_px {
            return;
        }
        let radius = style.landmark.size * 0.5;
        let opacity = style.landmark.opacity.clamp(0.0, 1.0);
        let faded = |color: Color| Color::new(color.r, color.g, color.b, color.a * opacity);
        let (fill, ink) = (faded(style.landmark.fill.color()), faded(style.landmark.ink.color()));
        for (center, kind, _) in &minimap.shapes.landmarks {
            let p = transform.apply(center.x, center.y);
            painter.circle_filled(p, radius, fill);
            painter.circle_stroke(p, radius, 1.5, ink);
            let icon = minimap.textures.landmarks[*kind as usize];
            if icon.is_valid() {
                let half = Vec2::new(radius * 0.72, radius * 0.72);
                painter.image(icon, Rect::from_extents(p - half, p + half), ink);
            }
        }
    }

    fn paint_camera(painter: &UiPainter, rect: Rect, style: &MinimapStyle) {
        if !rect.is_valid() {
            return;
        }
        let camera = &style.camera;
        painter.rect_filled(rect, camera.fill.color(), 2.0);
        painter.rect_stroke(rect, 2.0, camera.width, camera.outline.color());

        // Corner ticks.
        let tick = camera.ticks.min(rect.width() * 0.5).min(rect.height() * 0.5);
        let width = camera.width * 1.6;
        let color = camera.outline.color();
        for (corner, dx, dy) in [(rect.min, 1.0, 1.0), (Vec2::new(rect.max.x, rect.min.y), -1.0, 1.0), (rect.max, -1.0, -1.0), (Vec2::new(rect.min.x, rect.max.y), 1.0, -1.0)] {
            painter.polyline(&[corner + Vec2::new(0.0, dy * tick), corner, corner + Vec2::new(dx * tick, 0.0)], width, color, false);
        }
    }

    fn paint_pings(painter: &UiPainter, minimap: &super::Minimap, style: &MinimapStyle, transform: &super::widget::CellToMinimap, map: Rect) {
        for ping in &minimap.pings {
            if ping.lifetime <= 0.0 {
                continue;
            }
            let p = transform.apply(ping.target_cell.x as f32, ping.target_cell.y as f32);
            if !map.expanded(Vec2::new(24.0, 24.0)).contains_point(p) {
                continue;
            }
            let age = 1.0 - (ping.time_left / ping.lifetime).clamp(0.0, 1.0);
            let fade = (ping.time_left / ping.lifetime).clamp(0.0, 1.0) * style.ping_opacity.clamp(0.0, 1.0);

            // Two rings growing out of the point, half a period apart.
            for k in 0..2 {
                let phase = (age * 2.5 + k as f32 * 0.5).fract();
                let mut color = style.ping.color();
                color.a *= (1.0 - phase) * fade;
                painter.circle_stroke(p, 3.0 + 13.0 * phase, 2.0, color);
            }

            let icon = minimap.textures.icon(ping.icon);
            if icon.is_valid() {
                let tint = Color::new(ping.tint.r, ping.tint.g, ping.tint.b, fade);
                let half = Vec2::new(8.0, 8.0);
                let center = p - Vec2::new(0.0, 12.0);
                painter.image(icon, Rect::from_extents(center - half, center + half), tint);
            }
        }
    }

    // The legend in its rows under the map, on the widget's frame.
    fn paint_legend(painter: &UiPainter, legend: &[(Color, &'static str)], layout: &MinimapLayout, style: &MinimapStyle, text_color: Color) {
        if legend.is_empty() || !layout.legend.is_valid() {
            return;
        }
        let k = layout.legend_scale;
        let columns = layout.legend_columns;
        let column_width = layout.legend.width() / columns as f32;
        let row_height = LEGEND_ROW_HEIGHT * k;
        let swatch = Rect::from_pos_and_size(Vec2::new(0.0, 3.0 * k), Vec2::new(8.0 * k, 8.0 * k));
        for (index, (color, label)) in legend.iter().enumerate() {
            let (row, col) = (index / columns, index % columns);
            let origin = Vec2::new(layout.legend.min.x + col as f32 * column_width, layout.legend.min.y + row as f32 * row_height);
            let swatch = Rect::from_extents(origin + swatch.min, origin + swatch.max);
            painter.rect_filled(swatch, *color, 1.5 * k);
            painter.rect_stroke(swatch, 1.5 * k, 0.8, style.palette.outline.color());
            painter.text_scaled(origin + Vec2::new(11.0 * k, 0.0), text_color, style.legend.text_scale, *label);
        }
    }

    fn paint_badge(&self, context: &mut GameUiContext) {
        let count = context.tile_map.minimap().alert_count();
        if count == 0 {
            return;
        }
        let Some((_, button)) = self.buttons.as_ref().unwrap().tools.iter().find(|(tool, _)| *tool == MinimapTool::Alerts) else {
            return;
        };
        let style = style::current();
        let center = button.position() + Vec2::new(BUTTON_SIZE - 3.0, 3.0);
        let painter = ui::painter(context.ui_sys, UiLayer::Window);
        painter.circle_filled(center, 7.0, style.badge.color());
        painter.circle_stroke(center, 7.0, 1.2, style.badge_text.color());
        let text = if count > 9 { format_fixed_string!(8, "9+") } else { format_fixed_string!(8, "{count}") };
        painter.text_centered(center, style.badge_text.color(), 0.55, &text);
    }

    // Names what's under the cursor on the map.
    fn hover_tooltip(&self, context: &mut GameUiContext, layout: &MinimapLayout) {
        let minimap = context.tile_map.minimap();
        let cursor = minimap.view().cursor_pos;
        if !layout.map.contains_point(cursor) {
            return;
        }
        let Some(cell) = minimap.view().pick_cell(cursor) else {
            return;
        };
        let cell = Cell::new(cell.0.x.round() as i32, cell.0.y.round() as i32);
        if let Some(name) = minimap.describe_cell(cell) {
            // The style's tooltip settings: the realm's frame (or none), at its opacity. The
            // developer UI's is plain.
            let style = style::current();
            let background = match self.tooltip_background {
                _ if !style.tooltip_background => UiTooltipBackground::None,
                Some(sprite) => UiTooltipBackground::Sprite(sprite),
                None => UiTooltipBackground::Style,
            };
            ui::pointer_tooltip(context.ui_sys, self.font_scale, background, style.tooltip_opacity, |ui_sys| ui::text(ui_sys, &name));
        }
    }
}

// ----------------------------------------------
// InGameUiMinimapRenderer
// ----------------------------------------------

pub struct InGameUiMinimapRenderer {
    base_renderer: BaseMinimapRenderer,
}

impl MinimapRenderer for InGameUiMinimapRenderer {
    fn draw(&mut self, context: &mut GameUiContext) {
        self.base_renderer.draw(context, |_| {});
    }
}

impl InGameUiMinimapRenderer {
    const WIDGET_FONT_SCALE: UiFontScale = UiFontScale(0.8);

    pub fn new(_context: &mut GameUiContext) -> Self {
        Self { base_renderer: BaseMinimapRenderer::new(UiButtonSoundsEnabled::Pressed, Self::WIDGET_FONT_SCALE, true, false) }
    }
}

// ----------------------------------------------
// DevUiMinimapRenderer
// ----------------------------------------------

static SHOW_MINIMAP_DEBUG_CONTROLS: UiStaticVar<bool> = UiStaticVar::new(false);

// The minimap with debug controls and debug drawing.
pub struct DevUiMinimapRenderer {
    base_renderer: BaseMinimapRenderer,
    enable_debug_draw: bool,
}

impl MinimapRenderer for DevUiMinimapRenderer {
    fn draw(&mut self, context: &mut GameUiContext) {
        let debug_draw = self.enable_debug_draw;
        self.base_renderer.draw(context, move |context| {
            if debug_draw {
                Self::draw_debug(context);
            }
        });
        self.draw_debug_controls(context);
    }
}

impl DevUiMinimapRenderer {
    const WIDGET_FONT_SCALE: UiFontScale = UiFontScale(1.0);

    pub fn new(_context: &mut GameUiContext) -> Self {
        Self {
            base_renderer: BaseMinimapRenderer::new(UiButtonSoundsEnabled::empty(), Self::WIDGET_FONT_SCALE, false, true),
            enable_debug_draw: false,
        }
    }

    // The cursor's cell and the camera's centre.
    fn draw_debug(context: &mut GameUiContext) {
        let painter = ui::painter(context.ui_sys, UiLayer::Window);
        let minimap = context.tile_map.minimap();
        let view = minimap.view();
        let transform = view.cell_to_minimap();

        if let Some(cell) = view.pick_cell(view.cursor_pos) {
            let (x, y) = (cell.0.x.round(), cell.0.y.round());
            let corners = [(x - 0.5, y - 0.5), (x + 0.5, y - 0.5), (x + 0.5, y + 0.5), (x - 0.5, y + 0.5)].map(|(x, y)| transform.apply(x, y));
            painter.polyline(&corners, 1.0, Color::red(), true);
        }

        let camera_center = super::widget::world_to_cell(context.camera.iso_world_position().0 + Vec2::new(
            common::constants::HALF_BASE_TILE_WIDTH_F32,
            common::constants::HALF_BASE_TILE_HEIGHT_F32,
        ));
        painter.circle(transform.apply(camera_center.x, camera_center.y), 4.0, Color::green());
        painter.circle(view.camera_rect.center(), 2.0, Color::red());
    }

    fn draw_debug_controls(&mut self, context: &mut GameUiContext) {
        if !*SHOW_MINIMAP_DEBUG_CONTROLS || !context.tile_map.minimap().is_open() {
            return;
        }

        let layout = *context.tile_map.minimap().layout();
        let window_pos = Vec2::new(layout.window.max.x + 10.0, layout.window.min.y - 120.0);
        let mut show_debug_controls = *SHOW_MINIMAP_DEBUG_CONTROLS;
        let map_size = context.tile_map.minimap().size_in_cells();
        let window_name = format_fixed_string!(128, "Minimap Debug | {}x{}", map_size.width, map_size.height);

        let window = UiWindow::new(&window_name)
            .name("Minimap Debug")
            .open(&mut show_debug_controls)
            .no_resize()
            .no_scrollbar()
            .no_collapse()
            .auto_resize()
            .position(window_pos, UiCondition::FirstUse);

        let ui_sys: &UiSystem = context.ui_sys;
        ui::window(ui_sys, window, |ui_sys| {
            let minimap = context.tile_map.minimap_mut();

            ui::horizontal(ui_sys, |ui_sys| {
                if ui::small_button(ui_sys, "Reset") {
                    context.camera.center();
                    minimap.view_mut().reset();
                }
                ui::checkbox(ui_sys, "Debug Draw", &mut self.enable_debug_draw);
                ui::checkbox(ui_sys, "Clipped", &mut self.base_renderer.apply_widget_clip_rect);
            });

            let view = minimap.view_mut();
            ui::horizontal(ui_sys, |ui_sys| {
                ui::checkbox(ui_sys, "Auto Scroll", &mut view.auto_scroll);
                ui::checkbox(ui_sys, "Auto Zoom", &mut view.auto_zoom);
            });

            let decimals = UiNumberFormat::Decimals(2);
            ui::input_float(ui_sys, "Scroll Speed", &mut view.scroll_speed, 0.5, decimals, false);
            let mut zoom = view.zoom;
            if ui::input_float(ui_sys, "Zoom", &mut zoom, 0.25, decimals, false) {
                view.auto_zoom = false;
                view.set_zoom(zoom);
            }

            // The style follows the UI realm: picking one here sets the UI realm override, as
            // ui.realm does, and "auto" clears it.
            let realms: Vec<String> = std::iter::once("auto".to_string()).chain(Realm::names()).collect();
            let mut choice = realm::override_realm().and_then(|chosen| Realm::iter().position(|realm| realm == chosen)).map_or(0, |index| index + 1);
            if ui::combo(ui_sys, "Style", &mut choice, &realms) {
                realm::set_override(choice.checked_sub(1).and_then(|index| Realm::iter().nth(index)));
            }
            if ui::small_button(ui_sys, "Reload Style") {
                if let Err(err) = style::reload(realm::ui_realm()) {
                    engine::log::error!(engine::log::channel!("minimap"), "{err}");
                }
            }

            ui::separator(ui_sys);

            let view = minimap.view();
            ui::text(ui_sys, format_fixed_string!(128, "Visible Cells : {:.1}", view.visible_cells()));
            ui::text(ui_sys, format_fixed_string!(128, "View Center   : {}", view.center));
            ui::text(ui_sys, format_fixed_string!(128, "Texture       : {}x{} at {} px/cell", minimap.texture_size().width, minimap.texture_size().height, minimap.px_per_cell()));
            ui::text(ui_sys, format_fixed_string!(128, "Last Bake     : {:.2} ms", minimap.last_bake_secs() * 1000.0));
            ui::text(ui_sys, format_fixed_string!(128, "Features      : {}  Shapes: {}", minimap.features().len(), minimap.shapes.shape_count()));
            ui::text(ui_sys, format_fixed_string!(128, "Lens          : {}  Alerts: {}", minimap.lens(), minimap.alert_count()));
        });

        SHOW_MINIMAP_DEBUG_CONTROLS.set(show_debug_controls);
    }
}
