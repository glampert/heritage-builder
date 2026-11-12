// What the world map screen draws live over its base art (docs/plans/realm-ui.md, Phase 2: Markers
// and labels), after the mockup generator's map layer: region labels, uncharted areas' words,
// threats (their storm clouds a sprite at the map's scale, their words and arrow), routes by kind
// and state, foreign cities' rings, the player's marks (seals with their initial, prangs, stupas),
// the next mission's A/B sites and the compass, and the next mission's choice box (in the view's
// corner where the world file puts it at the fit, whatever the zoom), its dotted lines to the sites
// and their names in it. Positions follow the map view's transform; sizes (marks, text, line
// widths) stay at the screen's scale whatever the zoom.
//
// Looks come from the skin's world_map block (skin::SkinWorldMapDef): mark sprites, text looks,
// label places, route styles and colours.

use std::collections::BTreeMap;

use common::{Color, Rect, Vec2};
use engine::{
    file_sys::paths::PathRef,
    render::texture::TextureHandle,
    ui::{
        UiLabelLook, UiPainter, UiSystem, UiTextAnchor,
        text::{self, UiTextCategory},
        path_scope,
        widgets::{UiMapViewPointer, UiMapViewState, UiMapViewTransform, UiWidgetContext, map_view_transform, record_map_item, record_map_marker},
    },
};

use super::LegendEntry;
use crate::{
    campaign::config::ChoiceSite,
    menu::skin::{self, SkinChoiceBoxDef, SkinMapPlaceDef, SkinMapRouteDef, SkinMapTextDef},
    world_map::{
        WorldMapProvider,
        config::{CityKind, LabelSide, MapLabelAlign, MapLabelDef, MapLabelStyle, RouteKind, RouteState, ThreatDef, UnchartedAreaDef},
        provider::{ChoiceSiteInfo, MapCity, MapRoute, NextMissionChoice},
    },
};

// Points along a route's Catmull-Rom curve per segment.
const CURVE_STEPS: usize = 12;
// Generator sizes, in points on a 1920 × 1080 screen.
const FOREIGN_RADIUS: f32 = 10.0;
const SELECTED_RADIUS: f32 = 26.0;
const SELECTED_OUTER_RADIUS: f32 = 33.0;
const CAPITAL_SELECTED_RADIUS: f32 = 36.0;
const CHOICE_RADIUS: f32 = 13.0;
const HIT_HALF_SIZE: f32 = 14.0; // The least a marker answers the pointer around its point.
const COMPASS_N_RISE: f32 = 46.0;
const UNCHARTED_NOTE_DROP: f32 = 30.0;
const THREAT_TEXT_OFFSET: [f32; 2] = [30.0, 10.0];
const THREAT_NOTE_DROP: f32 = 26.0;
// The choice box's seal (Đại Long's): its size and centre, in from the box's lower right; the room
// it keeps right of the text.
const CHOICE_SEAL_SIZE: f32 = 26.0;
const CHOICE_SEAL_INSET: [f32; 2] = [30.0, 26.0];
const CHOICE_SEAL_ROOM: f32 = 48.0;
// Where a dotted line from the box meets a site: this far below its point.
const CHOICE_LINE_DROP: f32 = 14.0;
const CHOICE_LINE_STEPS: usize = 16;
// The least room the choice box keeps from the map's edges.
const CHOICE_BOX_MARGIN: f32 = 14.0;

// The id a next-mission site answers the pointer by, on the map and in the choice box.
pub fn site_id(site: ChoiceSite) -> String {
    format!("site:{site}")
}

// ----------------------------------------------
// MapScene
// ----------------------------------------------

// What the map shows, read once from the provider when the screen is built.
pub struct MapScene {
    cities: Vec<MapCity>,
    routes: Vec<MapRoute>,
    labels: Vec<MapLabelDef>,
    uncharted: Vec<UnchartedAreaDef>,
    threats: Vec<ThreatDef>,
    compass: [f32; 2],
    extent: Vec2,
    choice_box: [f32; 2],
    choice: Option<NextMissionChoice>,
}

impl MapScene {
    pub fn new(provider: &impl WorldMapProvider) -> Self {
        let map = provider.map();
        Self {
            cities: provider.cities(),
            routes: provider.routes(),
            labels: map.labels.clone(),
            uncharted: map.uncharted.clone(),
            threats: map.threats.clone(),
            compass: map.compass,
            extent: Vec2::new(map.extent[0], map.extent[1]),
            choice_box: map.choice_box,
            choice: provider.next_mission_choice(),
        }
    }

    pub fn city(&self, id: &str) -> Option<&MapCity> {
        self.cities.iter().find(|city| city.id == id)
    }

    // The next mission's site a pointer id names (site_id), and its choice.
    pub fn site(&self, id: &str) -> Option<(&NextMissionChoice, &ChoiceSiteInfo)> {
        let choice = self.choice.as_ref()?;
        choice.sites.iter().find(|site| site_id(site.site) == id).map(|site| (choice, site))
    }

    // Draws the scene, records its markers, and returns the id of the city or site under the
    // pointer (the last drawn, so the topmost; a site's name in the choice box answers as the site,
    // and the box covers what's under it).
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &self,
        ui_sys: &UiSystem,
        painter: &UiPainter,
        transform: &UiMapViewTransform,
        pointer: &UiMapViewPointer,
        look: &MapLook,
        selected: Option<&str>,
        hovered: Option<&str>,
    ) -> Option<String> {
        let k = look.scale;
        let at = |position: [f32; 2]| transform.to_screen(Vec2::new(position[0], position[1]));

        for label in &self.labels {
            let role = match label.style {
                MapLabelStyle::Note => "note",
                _ if label.own => "region_own",
                _ => "region",
            };
            let text = text::get(UiTextCategory::Realm, &label.text);
            let mut text_look = look.text(role, label.size);
            if label.style != MapLabelStyle::Note {
                text_look.letter_spacing = label.letter_spacing * k;
            }
            let anchor = match label.align {
                MapLabelAlign::Middle => UiTextAnchor::Middle,
                MapLabelAlign::Start => UiTextAnchor::Start,
            };
            painter.label(at(label.position), anchor, &look.case(role, text), &text_look);
        }

        for area in &self.uncharted {
            let point = at(area.label);
            let title = text::get(UiTextCategory::Realm, &area.title);
            painter.label(point, UiTextAnchor::Middle, &look.case("uncharted", title), &look.text("uncharted", 0.0));
            let note = text::get(UiTextCategory::Realm, &area.subtitle);
            painter.label(point + Vec2::new(0.0, UNCHARTED_NOTE_DROP * k), UiTextAnchor::Middle, note, &look.text("uncharted_note", 0.0));
        }

        for threat in &self.threats {
            self.draw_threat(painter, transform, look, threat);
        }

        for route in &self.routes {
            let points: Vec<Vec2> = route.points.iter().map(|point| at(*point)).collect();
            let style = match route.state {
                RouteState::Closed => "closed",
                RouteState::Open => match route.kind {
                    RouteKind::Land => "land",
                    RouteKind::Sea => "sea",
                    RouteKind::River => "river",
                },
            };
            look.draw_route(painter, style, &catmull_rom(&points, CURVE_STEPS));
        }

        // Foreign cities under the player's, as the generator draws them; markers recorded as drawn.
        let mut hit = None;
        let pointer_at = pointer.position;
        // Recorded for automation only while the widget registry is on.
        let recording = ui_sys.is_widget_registry_enabled();
        let mut markers: Vec<(String, Rect, String, serde_json::Value)> = Vec::new();
        for city in self.cities.iter().filter(|city| !city.player_owned).chain(self.cities.iter().filter(|city| city.player_owned)) {
            let point = at(city.position);
            let is_selected = selected == Some(city.id.as_str());
            let is_hovered = hovered == Some(city.id.as_str());
            let rect = look.draw_city(painter, city, point, is_selected, is_hovered);
            if pointer_at.is_some_and(|cursor| rect.contains_point(cursor)) && transform.rect.contains_point(point) {
                hit = Some(city.id.clone());
            }
            if recording {
                let value = serde_json::json!({
                    "id": city.id, "kind": city.kind.to_string(), "owner": city.owner, "player": city.player_owned,
                    "selected": is_selected, "current": city.current,
                });
                markers.push((city.id.clone(), rect, city.name().to_string(), value));
            }
        }

        if let Some(choice) = &self.choice {
            for site in &choice.sites {
                let point = at(site.position);
                let id = site_id(site.site);
                let lit = hovered == Some(id.as_str());
                let rect = look.draw_choice_site(painter, &site.site.to_string(), &site.name, site.label, point, lit);
                if pointer_at.is_some_and(|cursor| rect.contains_point(cursor)) && transform.rect.contains_point(point) {
                    hit = Some(id.clone());
                }
                if recording {
                    let value = serde_json::json!({ "site": site.site.to_string(), "mission": site.mission_index, "kind": "choice_site" });
                    markers.push((id, rect, site.name.clone(), value));
                }
            }
        }

        look.draw_compass(painter, at(self.compass));

        // The choice box, over everything: what's under it doesn't answer the pointer.
        let choice_box = self.choice.as_ref().map(|choice| {
            // Where the world file's corner is at the fit (centred, as the view is at zoom 1).
            let fitted = map_view_transform(
                UiMapViewState { zoom: 1.0, centre: Vec2::new(self.extent.x * 0.5, self.extent.y * 0.5) },
                transform.rect,
                self.extent,
            );
            let corner = fitted.to_screen(Vec2::new(self.choice_box[0], self.choice_box[1]));
            let sites: Vec<Vec2> = choice.sites.iter().map(|site| at(site.position)).collect();
            let (box_rect, names) = look.draw_choice_box(painter, choice, corner, &sites, transform.rect, hovered);
            let pointed = pointer_at.is_some_and(|cursor| box_rect.contains_point(cursor));
            if pointed {
                hit = names.iter().find(|(_, rect)| pointer_at.is_some_and(|cursor| rect.contains_point(cursor))).map(|(site, _)| site_id(site.site));
            }
            (choice, box_rect, names, pointed)
        });

        for (id, rect, name, value) in markers {
            record_map_marker(ui_sys, &name, rect, transform.rect, hit.as_deref() == Some(id.as_str()), Some(value));
        }
        if let Some((choice, box_rect, names, pointed)) = choice_box.filter(|_| recording) {
            let value = serde_json::json!({ "title": choice.title, "subtitle": choice.subtitle, "mission": choice.mission_index, "open": choice.open });
            record_map_item(ui_sys, "choice_box", "choice", box_rect, transform.rect, pointed, Some(value));
            let _scope = path_scope(ui_sys, "choice");
            for (site, rect) in &names {
                let id = site_id(site.site);
                let value = serde_json::json!({ "site": site.site.to_string(), "mission": site.mission_index });
                record_map_item(ui_sys, "choice_option", &site.phrase, *rect, transform.rect, hit.as_deref() == Some(id.as_str()), Some(value));
            }
        }
        hit
    }

    fn draw_threat(&self, painter: &UiPainter, transform: &UiMapViewTransform, look: &MapLook, threat: &ThreatDef) {
        let k = look.scale;
        let point = transform.to_screen(Vec2::new(threat.position[0], threat.position[1]));
        if let Some((texture, size_px)) = look.threat {
            // At the map's scale: the clouds are scenery.
            let art = skin::world_map();
            let scale = transform.points_per_unit / art.pixels_per_unit.max(f32::EPSILON);
            let min = Vec2::new(point.x - art.threat_anchor[0] * scale, point.y - art.threat_anchor[1] * scale);
            painter.image(texture, Rect::from_pos_and_size(min, size_px * scale), Color::white());
        }
        let text_at = point + Vec2::new(THREAT_TEXT_OFFSET[0] * k, THREAT_TEXT_OFFSET[1] * k);
        let title = text::get(UiTextCategory::Realm, &threat.title);
        painter.label(text_at, UiTextAnchor::Middle, &look.case("threat", title), &look.text("threat", 0.0));
        let note = text::get(UiTextCategory::Realm, &threat.subtitle);
        painter.label(text_at + Vec2::new(0.0, THREAT_NOTE_DROP * k), UiTextAnchor::Middle, note, &look.text("threat_note", 0.0));
        if threat.arrow.len() >= 2 {
            let points: Vec<Vec2> = threat.arrow.iter().map(|point| transform.to_screen(Vec2::new(point[0], point[1]))).collect();
            let color = look.color("threat_arrow", "accent");
            painter.dashed_polyline(&catmull_rom(&points, CURVE_STEPS), 2.5 * k, color, 6.0 * k, 5.0 * k);
            let (tip, before) = (points[points.len() - 1], points[points.len() - 2]);
            let dir = normalized(tip - before);
            let side = Vec2::new(-dir.y, dir.x);
            let head = [tip + dir * (4.0 * k), tip - dir * (9.0 * k) + side * (6.0 * k), tip - dir * (9.0 * k) - side * (6.0 * k)];
            painter.convex_polygon(&head, color, 0.0, color);
        }
    }
}

fn normalized(v: Vec2) -> Vec2 {
    let length = (v.x * v.x + v.y * v.y).sqrt();
    if length > f32::EPSILON { Vec2::new(v.x / length, v.y / length) } else { Vec2::new(0.0, 1.0) }
}

// The Catmull-Rom curve through `points` (tension 0.5, as the generator's smooth()), sampled at
// `steps` points per segment.
pub fn catmull_rom(points: &[Vec2], steps: usize) -> Vec<Vec2> {
    if points.len() < 3 {
        return points.to_vec();
    }
    const TENSION: f32 = 0.5;
    let mut out = vec![points[0]];
    let n = points.len();
    for i in 0..n - 1 {
        let p0 = points[if i > 0 { i - 1 } else { i }];
        let p1 = points[i];
        let p2 = points[i + 1];
        let p3 = points[if i + 2 < n { i + 2 } else { i + 1 }];
        let c1 = p1 + (p2 - p0) * (TENSION / 3.0);
        let c2 = p2 - (p3 - p1) * (TENSION / 3.0);
        for step in 1..=steps {
            let t = step as f32 / steps as f32;
            let u = 1.0 - t;
            let point = p1 * (u * u * u) + c1 * (3.0 * u * u * t) + c2 * (3.0 * u * t * t) + p2 * (t * t * t);
            out.push(point);
        }
    }
    out
}

// ----------------------------------------------
// MapLook
// ----------------------------------------------

// The skin's world map looks, resolved for drawing at the screen's scale `scale`.
pub struct MapLook {
    pub scale: f32,
    marks: BTreeMap<String, (TextureHandle, Vec2)>, // Sprite and size in points (at scale 1).
    threat: Option<(TextureHandle, Vec2)>,          // Sprite and size in pixels.
    seal_letter: Option<SkinMapTextDef>,
    text: BTreeMap<String, MapTextLook>, // By role, resolved once.
    places: BTreeMap<String, SkinMapPlaceDef>,
    routes: BTreeMap<String, SkinMapRouteDef>,
    colors: BTreeMap<String, String>,
    choice_box: SkinChoiceBoxDef,
}

// A text role's look (SkinMapTextDef) resolved at the screen's scale: drawn at its own size, or
// another (`em`, its size in points per point of font size asked).
struct MapTextLook {
    look: UiLabelLook,
    em: f32,
    capitals: bool,
}

impl MapTextLook {
    fn new(def: &SkinMapTextDef, scale: f32) -> Self {
        Self { look: map_label_look(def, def.size, scale), em: scale / skin::font_em(def.heading), capitals: def.capitals }
    }
}

// A piece of the choice box's subtitle: its text, or a site's name.
enum SubtitlePiece<'c> {
    Text(String),
    Site(&'c ChoiceSiteInfo),
}

impl SubtitlePiece<'_> {
    fn text(&self) -> &str {
        match self {
            Self::Text(text) => text,
            Self::Site(site) => &site.phrase,
        }
    }
}

// The choice box's subtitle ("Choose {a} or {b}", or the choice point's own) cut at its sites'
// names, so each can be pointed at. A text without both names in it is one piece, the subtitle as
// the provider gives it.
fn subtitle_pieces(choice: &NextMissionChoice) -> Vec<SubtitlePiece<'_>> {
    split_subtitle(text::get(UiTextCategory::Menu, &choice.subtitle_key), choice)
}

// `template` cut at its `{a}` and `{b}`, its `{{` and `}}` read as braces, as text::format reads
// them; any other placeholder is kept as it is. The provider's subtitle whole if the template
// names a site the choice lacks or leaves one of its sites out.
fn split_subtitle<'c>(template: &str, choice: &'c NextMissionChoice) -> Vec<SubtitlePiece<'c>> {
    let whole = || vec![SubtitlePiece::Text(choice.subtitle.clone())];
    let mut pieces = Vec::new();
    let mut text = String::new();
    let mut rest = template;
    while let Some(c) = rest.chars().next() {
        let mark = [("{a}", ChoiceSite::A), ("{b}", ChoiceSite::B)].into_iter().find(|(mark, _)| rest.starts_with(mark));
        if let Some((mark, which)) = mark {
            let Some(info) = choice.sites.iter().find(|site| site.site == which) else {
                return whole();
            };
            if !text.is_empty() {
                pieces.push(SubtitlePiece::Text(std::mem::take(&mut text)));
            }
            pieces.push(SubtitlePiece::Site(info));
            rest = &rest[mark.len()..];
        } else {
            text.push(c);
            let escaped = rest.starts_with("{{") || rest.starts_with("}}");
            rest = &rest[if escaped { 2 } else { c.len_utf8() }..];
        }
    }
    if !text.is_empty() {
        pieces.push(SubtitlePiece::Text(text));
    }
    if pieces.iter().filter(|piece| matches!(piece, SubtitlePiece::Site(_))).count() < choice.sites.len() {
        return whole();
    }
    pieces
}

impl MapLook {
    pub fn load(context: &mut dyn UiWidgetContext, scale: f32) -> Self {
        let art = skin::world_map();
        let marks = art.marks.iter().map(|(name, path)| (name.clone(), skin::sprite_and_size(context, path))).collect();
        let threat = (!art.threat.is_empty()).then(|| {
            let texture = context.load_texture(PathRef::from_str(&art.threat));
            let size = context.render_sys().texture_cache().texture_size(texture).map_or(Vec2::new(1.0, 1.0), |size| Vec2::new(size.width as f32, size.height as f32));
            (texture, size)
        });
        Self {
            scale,
            marks,
            threat,
            seal_letter: art.seal_letter.clone(),
            text: art.text.iter().map(|(role, def)| (role.clone(), MapTextLook::new(def, scale))).collect(),
            places: art.places.clone(),
            routes: art.routes.clone(),
            colors: art.colors.clone(),
            choice_box: art.choice_box.clone(),
        }
    }

    fn color(&self, name: &str, fallback: &str) -> Color {
        self.colors.get(name).and_then(|value| skin::color(value)).or_else(|| skin::color(fallback)).unwrap_or(Color::black())
    }

    fn opt_color(&self, name: &str) -> Option<Color> {
        self.colors.get(name).and_then(|value| skin::color(value))
    }

    // The look of text of `role`, at `size` points (0: the role's own size).
    fn text(&self, role: &str, size: f32) -> UiLabelLook {
        let Some(text) = self.text.get(role) else {
            let def = SkinMapTextDef::default();
            return map_label_look(&def, if size > 0.0 { size } else { def.size }, self.scale);
        };
        let mut look = text.look;
        if size > 0.0 {
            look.size = size * text.em;
        }
        look
    }

    // `text` in capitals if `role`'s look says so.
    fn case(&self, role: &str, text: &str) -> String {
        if self.text.get(role).is_some_and(|text| text.capitals) { text.to_uppercase() } else { text.to_string() }
    }

    fn place(&self, role: &str) -> SkinMapPlaceDef {
        self.places.get(role).copied().unwrap_or_default()
    }

    fn mark(&self, name: &str) -> Option<(TextureHandle, Vec2)> {
        self.marks.get(name).copied()
    }

    // A sprite mark centred on `point`, at the screen's scale. Its rect.
    fn draw_mark(&self, painter: &UiPainter, name: &str, point: Vec2) -> Option<Rect> {
        let (texture, size) = self.mark(name)?;
        let size = size * self.scale;
        let rect = Rect::from_pos_and_size(point - size * 0.5, size);
        painter.image(texture, rect, Color::white());
        Some(rect)
    }

    fn draw_route(&self, painter: &UiPainter, style: &str, points: &[Vec2]) {
        let Some(def) = self.routes.get(style) else {
            return;
        };
        let k = self.scale;
        if let Some(shadow) = def.shadow.as_deref().and_then(skin::color) {
            let offset = Vec2::new(def.shadow_offset[0] * k, def.shadow_offset[1] * k);
            let shifted: Vec<Vec2> = points.iter().map(|point| *point + offset).collect();
            painter.polyline(&shifted, def.shadow_width * k, shadow, false);
        }
        let color = skin::color(&def.color).unwrap_or(Color::black());
        if def.dots > 0.0 {
            painter.dotted_polyline(points, def.width * 0.5 * k, color, def.dots * k);
        } else {
            painter.dashed_polyline(points, def.width * k, color, def.dash[0] * k, def.dash[1] * k);
        }
    }

    // The selection ring round a city (and its hover glow, fainter).
    fn draw_ring(&self, painter: &UiPainter, point: Vec2, radius: f32, faint: bool) {
        let k = self.scale;
        let alpha = if faint { 0.45 } else { 1.0 };
        let fade = |color: Color| Color::new(color.r, color.g, color.b, color.a * alpha);
        let selected = fade(self.color("selected", "accent"));
        if let Some(shadow) = self.opt_color("selected_shadow") {
            painter.circle_stroke(point + Vec2::new(1.0 * k, 2.0 * k), radius * k, 4.0 * k, fade(shadow));
        }
        painter.circle_stroke(point, radius * k, 3.0 * k, selected);
        if !faint {
            let outer = self.color("selected_outer", "accent");
            painter.dashed_circle(point, (radius + SELECTED_OUTER_RADIUS - SELECTED_RADIUS) * k, 1.0 * k, outer, 3.0 * k, 4.0 * k);
        }
    }

    // A city: its mark, its ring if selected or hovered, its name beside it. The rect the pointer
    // finds it in.
    fn draw_city(&self, painter: &UiPainter, city: &MapCity, point: Vec2, selected: bool, hovered: bool) -> Rect {
        let k = self.scale;
        let hit_half = HIT_HALF_SIZE * k;
        let mut rect = Rect::from_pos_and_size(point - Vec2::new(hit_half, hit_half), Vec2::new(hit_half * 2.0, hit_half * 2.0));
        let name = city.name();

        if !city.player_owned {
            let ring_radius = SELECTED_RADIUS;
            if selected {
                self.draw_ring(painter, point, ring_radius, false);
            } else if hovered {
                self.draw_ring(painter, point, ring_radius, true);
            }
            let radius = FOREIGN_RADIUS * k;
            painter.circle_filled(point, radius, self.color("foreign_fill", "light"));
            painter.circle_stroke(point, radius, 2.0 * k, self.color("foreign_stroke", "ink"));
            painter.circle_filled(point, radius * 0.4, self.color("foreign_dot", "ink"));
            self.draw_name(painter, "foreign", "foreign", name, point, city.label);
            return rect;
        }

        let (mark, role) = match city.kind {
            CityKind::Capital => ("capital", "capital"),
            CityKind::Monument => ("monument", "monument"),
            _ => ("city", "city"),
        };
        if selected || hovered {
            let radius = if city.kind == CityKind::Capital { CAPITAL_SELECTED_RADIUS } else { SELECTED_RADIUS };
            self.draw_ring(painter, point, radius, !selected);
        }
        if let Some(mark_rect) = self.draw_mark(painter, mark, point) {
            // The sprite's room around the mark is blank: the pointer finds the city near its point.
            let half = (mark_rect.width().min(mark_rect.height()) * 0.3).max(hit_half);
            rect = Rect::from_pos_and_size(point - Vec2::new(half, half), Vec2::new(half * 2.0, half * 2.0));
        }
        let place = self.place(role);
        if let Some(letter_def) = &self.seal_letter
            && place.seal > 0.0
            && let Some(initial) = name.chars().next()
        {
            let seal = place.seal * k;
            let look = map_label_look(letter_def, place.seal * letter_def.size, k);
            let letter: String = initial.to_uppercase().collect();
            painter.label(point + Vec2::new(0.0, seal * 0.2), UiTextAnchor::Middle, &letter, &look);
        }
        self.draw_name(painter, role, role, name, point, city.label);
        rect
    }

    // A name beside a marker at `point`, as the generator's place_label puts it.
    fn draw_name(&self, painter: &UiPainter, text_role: &str, place_role: &str, name: &str, point: Vec2, side: LabelSide) -> Rect {
        let k = self.scale;
        let look = self.text(text_role, 0.0);
        let place = self.place(place_role);
        let size = look.size;
        let base = Vec2::new(point.x, point.y + place.y * k);
        let gap = place.gap * k;
        let (at, anchor) = match side {
            LabelSide::Below => (Vec2::new(base.x, base.y + gap + size * 0.8), UiTextAnchor::Middle),
            LabelSide::Above => (Vec2::new(base.x, base.y - gap - 4.0 * k), UiTextAnchor::Middle),
            LabelSide::Right => (Vec2::new(base.x + gap + 4.0 * k, base.y + size * 0.35), UiTextAnchor::Start),
            LabelSide::Left => (Vec2::new(base.x - gap - 4.0 * k, base.y + size * 0.35), UiTextAnchor::End),
        };
        painter.label(at, anchor, name, &look)
    }

    // A next-mission site: a dashed ring with a dot, "A · name" beside it, a glow under it while
    // `lit` (the pointer on it or on its name in the choice box). Its rect.
    fn draw_choice_site(&self, painter: &UiPainter, tag: &str, name: &str, side: LabelSide, point: Vec2, lit: bool) -> Rect {
        let k = self.scale;
        if lit {
            let glow = self.color("choice", "accent");
            painter.circle_filled(point, (CHOICE_RADIUS + 5.0) * k, Color::new(glow.r, glow.g, glow.b, glow.a * 0.25));
        }
        self.draw_site_glyph(painter, point, CHOICE_RADIUS * k);
        let text = text::format(UiTextCategory::Menu, "world_map.site", &[("site", tag), ("name", name)]);
        self.draw_name(painter, "choice", "choice", &text, point, side);
        let half = (CHOICE_RADIUS + 2.0) * k;
        Rect::from_pos_and_size(point - Vec2::new(half, half), Vec2::new(half * 2.0, half * 2.0))
    }

    // The next mission's choice box, after the generator's choice_box: its top left at `corner`
    // (kept in `view`), dotted lines from it to the sites at `sites` (window points), "Next:
    // <mission>" and "Choose <A> or <B>", the sites' names in it lit while `hovered` names their
    // site. Its rect and each site's name's rect in it.
    fn draw_choice_box<'c>(
        &self,
        painter: &UiPainter,
        choice: &'c NextMissionChoice,
        corner: Vec2,
        sites: &[Vec2],
        view: Rect,
        hovered: Option<&str>,
    ) -> (Rect, Vec<(&'c ChoiceSiteInfo, Rect)>) {
        let k = self.scale;
        let def = &self.choice_box;
        let title_look = self.text("choice_title", 0.0);
        let sub_look = self.text("choice_subtitle", 0.0);
        let pieces = subtitle_pieces(choice);

        // As large as the mockup's, or more for longer text.
        let measure = |text: &str, look: &UiLabelLook| painter.label_rect(Vec2::zero(), UiTextAnchor::Start, text, look).width();
        let title_width = measure(&choice.title, &title_look);
        let sub_width: f32 = pieces.iter().map(|piece| measure(piece.text(), &sub_look)).sum();
        let seal_room = if def.seal { CHOICE_SEAL_ROOM * k } else { def.text_x * k };
        let width = (def.size[0] * k).max(def.text_x * k + title_width.max(sub_width) + seal_room).ceil();
        let height = (def.size[1] * k).ceil();
        // A wider box grows to the left, keeping the mockup's right edge, and stays a margin
        // inside the map.
        let inside = view.shrunk(Vec2::new(CHOICE_BOX_MARGIN * k, CHOICE_BOX_MARGIN * k));
        let left = corner.x - (width - def.size[0] * k).max(0.0);
        let min = Vec2::new(
            left.min(inside.max.x - width).max(inside.min.x),
            corner.y.min(inside.max.y - height).max(inside.min.y),
        );
        let rect = Rect::from_pos_and_size(min, Vec2::new(width, height));

        // The dotted lines from its top edge to each site, under it.
        let line = self.color("choice_line", "accent");
        for &site in sites {
            // From the end of its top edge nearer the site, as the generator draws them.
            let (left, right) = (rect.min.x + 30.0 * k, rect.max.x - 30.0 * k);
            let start = Vec2::new(if (site.x - left).abs() <= (site.x - right).abs() { left } else { right }, rect.min.y);
            let end = site + Vec2::new(0.0, CHOICE_LINE_DROP * k);
            let control = (start + end) * 0.5 + Vec2::new(20.0 * k, 0.0);
            let curve: Vec<Vec2> = (0..=CHOICE_LINE_STEPS)
                .map(|step| {
                    let t = step as f32 / CHOICE_LINE_STEPS as f32;
                    let u = 1.0 - t;
                    start * (u * u) + control * (2.0 * u * t) + end * (t * t)
                })
                .collect();
            painter.dashed_polyline(&curve, def.line_width * k, line, 2.0 * k, 4.0 * k);
        }

        if let Some(shadow) = self.opt_color("choice_box_shadow") {
            let offset = Vec2::new(def.shadow_offset[0] * k, def.shadow_offset[1] * k);
            painter.rect_filled(Rect::from_pos_and_size(min + offset, rect.size()), shadow, 0.0);
        }
        painter.rect_filled(rect, self.color("choice_box_fill", "light"), 0.0);
        if def.border_width > 0.0 {
            painter.rect_stroke(rect, 0.0, def.border_width * k, self.color("choice_box_border", "ink"));
        }
        let inset = def.inner_inset * k;
        let inner = Rect::from_pos_and_size(min + Vec2::new(inset, inset), rect.size() - Vec2::new(inset * 2.0, inset * 2.0));
        if let Some(fill) = self.opt_color("choice_box_inner_fill") {
            painter.rect_filled(inner, fill, 0.0);
        }
        if def.inner_width > 0.0 {
            painter.rect_stroke(inner, 0.0, def.inner_width * k, self.color("choice_box_inner", "accent"));
        }

        let text_x = min.x + def.text_x * k;
        painter.label(Vec2::new(text_x, min.y + def.title_baseline * k), UiTextAnchor::Start, &choice.title, &title_look);

        // "Choose <A> or <B>": the sites' names lit and underlined while their site is hovered.
        let hover = self.opt_color("choice_hover").unwrap_or(sub_look.color);
        let baseline = min.y + def.subtitle_baseline * k;
        let mut x = text_x;
        let mut names = Vec::new();
        for piece in &pieces {
            let text = piece.text();
            match piece {
                SubtitlePiece::Text(_) => {
                    x += painter.label(Vec2::new(x, baseline), UiTextAnchor::Start, text, &sub_look).width();
                }
                SubtitlePiece::Site(site) => {
                    let lit = hovered == Some(site_id(site.site).as_str());
                    let look = if lit { UiLabelLook { color: hover, ..sub_look.clone() } } else { sub_look.clone() };
                    let drawn = painter.label(Vec2::new(x, baseline), UiTextAnchor::Start, text, &look);
                    let underline = baseline + 3.0 * k;
                    let color = if lit { hover } else { Color::new(sub_look.color.r, sub_look.color.g, sub_look.color.b, sub_look.color.a * 0.45) };
                    painter.line(Vec2::new(drawn.min.x, underline), Vec2::new(drawn.max.x, underline), (if lit { 1.5 } else { 1.0 }) * k, color);
                    names.push((*site, drawn));
                    x += drawn.width();
                }
            }
        }

        if def.seal {
            self.draw_choice_seal(painter, rect.max - Vec2::new(CHOICE_SEAL_INSET[0] * k, CHOICE_SEAL_INSET[1] * k));
        }
        (rect, names)
    }

    // Đại Long's seal on the choice box: the city seal with two arrows, one each way (the
    // generator's "⇄", which the seal font lacks).
    fn draw_choice_seal(&self, painter: &UiPainter, centre: Vec2) {
        let k = self.scale;
        let Some((texture, size)) = self.mark("city") else {
            return;
        };
        let seal_points = self.place("city").seal.max(1.0);
        let draw_size = size * (CHOICE_SEAL_SIZE / seal_points * k);
        painter.image(texture, Rect::from_pos_and_size(centre - draw_size * 0.5, draw_size), Color::white());
        let color = self.seal_letter.as_ref().and_then(|def| skin::color(&def.color)).unwrap_or(Color::white());
        let (half, rise, head) = (6.0 * k, 3.0 * k, 3.0 * k);
        for (y, dir) in [(-rise, 1.0), (rise, -1.0)] {
            let from = centre + Vec2::new(-half * dir, y);
            let to = centre + Vec2::new(half * dir, y);
            painter.line(from, to, 1.6 * k, color);
            painter.line(to, to + Vec2::new(-head * dir, -head * 0.8), 1.6 * k, color);
            painter.line(to, to + Vec2::new(-head * dir, head * 0.8), 1.6 * k, color);
        }
    }

    fn draw_site_glyph(&self, painter: &UiPainter, point: Vec2, radius: f32) {
        let k = self.scale;
        let ring = self.color("choice", "accent");
        painter.dashed_circle(point, radius, 2.3 * k, ring, 4.0 * k, 3.0 * k);
        painter.circle_filled(point, radius * 0.35, self.color("choice_dot", "accent"));
    }

    fn draw_compass(&self, painter: &UiPainter, point: Vec2) {
        if self.draw_mark(painter, "compass", point).is_some() {
            let look = self.text("compass", 0.0);
            painter.label(point - Vec2::new(0.0, COMPASS_N_RISE * self.scale), UiTextAnchor::Middle, text::get(UiTextCategory::Menu, "world_map.compass_north"), &look);
        }
    }

    // A legend entry's glyph with its middle-left at `at`, as the generator's legend_glyph draws it.
    pub(super) fn draw_legend_glyph(&self, painter: &UiPainter, entry: LegendEntry, at: Vec2) {
        let k = self.scale;
        let center = at + Vec2::new(8.0 * k, 0.0);
        match entry {
            LegendEntry::Player => {
                self.draw_mark(painter, "legend", center);
            }
            LegendEntry::Foreign => {
                let radius = 7.0 * k;
                painter.circle_filled(center, radius, self.color("foreign_fill", "light"));
                painter.circle_stroke(center, radius, 2.0 * k, self.color("foreign_stroke", "ink"));
                painter.circle_filled(center, radius * 0.4, self.color("foreign_dot", "ink"));
            }
            LegendEntry::Choice => self.draw_site_glyph(painter, center, 8.0 * k),
            LegendEntry::River | LegendEntry::Sea | LegendEntry::Land | LegendEntry::Closed => {
                let style = match entry {
                    LegendEntry::River => "river",
                    LegendEntry::Sea => "sea",
                    LegendEntry::Land => "land",
                    _ => "closed_legend",
                };
                let line = [at + Vec2::new(-4.0 * k, 0.0), at + Vec2::new(22.0 * k, 0.0)];
                self.draw_route(painter, style, &line);
            }
        }
    }
}

// A UiLabelLook of a skin's map text look, `size` points large on a 1920 × 1080 screen (an em, as
// the generator's font sizes: skin::font_em), at the screen's scale `scale`.
pub fn map_label_look(def: &SkinMapTextDef, size: f32, scale: f32) -> UiLabelLook {
    skin::label_look(def, size, scale)
}

#[cfg(test)]
mod tests {
    use super::*;

    // The curve passes through every point, and a straight line stays straight.
    #[test]
    fn routes_pass_through_their_points() {
        let points = [Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(20.0, 0.0), Vec2::new(30.0, 5.0)];
        let curve = catmull_rom(&points, 4);
        assert_eq!(curve.len(), 1 + 3 * 4);
        for (index, point) in points.iter().enumerate() {
            let on = curve[index * 4];
            assert!((on.x - point.x).abs() < 1e-4 && (on.y - point.y).abs() < 1e-4, "{index}: {on:?}");
        }
        assert!(curve[..5].iter().all(|point| point.y.abs() < 1e-4));
        assert_eq!(catmull_rom(&points[..2], 4), points[..2].to_vec());
    }

    fn choice(sites: &[ChoiceSite]) -> NextMissionChoice {
        let site = |site: ChoiceSite| ChoiceSiteInfo {
            site,
            name_key: String::new(),
            name: site.to_string(),
            phrase: site.to_string(),
            phrased: false,
            position: [0.0, 0.0],
            label: LabelSide::Below,
            mission_index: 0,
        };
        NextMissionChoice {
            mission_index: 0,
            title: String::new(),
            subtitle: "the subtitle".into(),
            subtitle_key: String::new(),
            sites: sites.iter().map(|&which| site(which)).collect(),
            open: true,
        }
    }

    // The pieces as text, a site as <A> or <B>.
    fn split(template: &str, sites: &[ChoiceSite]) -> Vec<String> {
        let choice = choice(sites);
        split_subtitle(template, &choice)
            .iter()
            .map(|piece| match piece {
                SubtitlePiece::Text(text) => text.clone(),
                SubtitlePiece::Site(site) => format!("<{}>", site.site),
            })
            .collect()
    }

    // The subtitle is cut at both sites' marks, in either order; a mark for a site the choice
    // lacks, a site left out or no marks at all give the provider's subtitle whole; braces escape.
    #[test]
    fn subtitle_pieces_cut_at_the_sites() {
        let both = [ChoiceSite::A, ChoiceSite::B];
        assert_eq!(split("Choose {a} or {b}", &both), ["Choose ", "<A>", " or ", "<B>"]);
        assert_eq!(split("{b}, then {a}.", &both), ["<B>", ", then ", "<A>", "."]);
        assert_eq!(split("{a}{b}", &both), ["<A>", "<B>"]);
        assert_eq!(split("Choose {a} only", &both), ["the subtitle"]);
        assert_eq!(split("No sites named", &both), ["the subtitle"]);
        assert_eq!(split("Choose {a} or {b}", &[ChoiceSite::A]), ["the subtitle"]);
        assert_eq!(split("{{Choose}} {a} or {b} {count}", &both), ["{Choose} ", "<A>", " or ", "<B>", " {count}"]);
        assert_eq!(split("{{a}} {a} {b}", &both), ["{a} ", "<A>", " ", "<B>"]);
    }
}
