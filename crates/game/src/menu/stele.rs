// Draws a campaign's stele (crate::campaign::stele) on the skin's art (skin::intermission): the
// niche behind it, the slab, and its text carved live in the realm's font, as the mockup
// generator's stele_lines sets it (docs/plans/realm-ui.md, Phase 3: Stele). The debrief and the
// campaign-complete screen draw it, and the Skin Gallery shows a sample.
//
// Positions are the skin's, in points on the debrief's 1920 × 1080 screen; a caller draws the
// composition at a scale (screen points per such point) from an origin (where its top left lands).
// The text is laid out to fit above the slab's `bottom`: when there are too many lines, they're set
// smaller (to half their size at most) until they do.

use common::{Color, Rect, Vec2};
use engine::{
    file_sys::paths::PathRef,
    render::texture::TextureHandle,
    ui::{UiLabelLook, UiPainter, UiTextAnchor, widgets::UiWidgetContext},
};

use super::skin::{self, SkinMapTextDef, SkinSteleDef};
use crate::campaign::stele::Stele;

// The smallest the lines are set to fit, as a share of their size.
const MIN_FIT: f32 = 0.5;
// How much smaller each try sets them.
const FIT_STEP: f32 = 0.94;
// The title shrinks slower than the lines (the generator's min(1, scale × 1.15)).
const TITLE_FIT: f32 = 1.15;

// The current skin's stele art, loaded.
#[derive(Clone)]
pub struct SteleArt {
    def: SkinSteleDef,
    niche_rect: [f32; 4],
    stele: Option<TextureHandle>,
    niche: Option<TextureHandle>,
    mark: Option<TextureHandle>,
}

impl SteleArt {
    pub fn load(context: &mut dyn UiWidgetContext) -> Self {
        let art = skin::intermission();
        let mut load = |path: &str| (!path.is_empty()).then(|| context.load_texture(PathRef::from_str(path)));
        Self {
            stele: load(&art.stele.sprite),
            niche: load(&art.niche.sprite),
            mark: load(&art.stele.mark),
            niche_rect: art.niche.sprite_rect(),
            def: art.stele.clone(),
        }
    }

    // The same art placed as the campaign-complete screen places it (skin::SkinCampaignCompleteDef):
    // the stele's sprite stretched to `rect` ([x, y, width, height]), its text as far in from its
    // left and top as before but as much wider and lower as the rect grows (the generator's
    // stele() takes its text's width and bottom from its rect's), and the niche stretched sideways
    // by `niche_stretch` ([factor, about x]). The lines start `rule_clearance` points lower, the
    // rule under the title staying where it was.
    pub fn stretched(mut self, rect: [f32; 4], niche_stretch: [f32; 2], rule_clearance: f32) -> Self {
        let [_, _, width, height] = self.def.rect;
        self.def.text[2] += rect[2] - width;
        self.def.bottom += rect[3] - height;
        self.def.rect = rect;
        self.def.title_space += rule_clearance;
        self.def.rule[1] += rule_clearance;
        let [factor, about] = niche_stretch;
        let [x, y, niche_width, niche_height] = self.niche_rect;
        self.niche_rect = [about + (x - about) * factor, y, niche_width * factor, niche_height];
        self
    }

    // The niche's and the stele's sprites together, [x, y, width, height] in the composition's
    // points: what draw_niche and draw_stele cover.
    pub fn bounds(&self) -> Rect {
        let rect = |[x, y, width, height]: [f32; 4]| Rect::from_pos_and_size(Vec2::new(x, y), Vec2::new(width, height));
        let (a, b) = (rect(self.niche_rect), rect(self.def.sprite_rect()));
        Rect {
            min: Vec2::new(a.min.x.min(b.min.x), a.min.y.min(b.min.y)),
            max: Vec2::new(a.max.x.max(b.max.x), a.max.y.max(b.max.y)),
        }
    }

    // The niche, the composition's (0, 0) at `origin`, `scale` screen points per point.
    pub fn draw_niche(&self, painter: &UiPainter, origin: Vec2, scale: f32) {
        if let Some(texture) = self.niche {
            painter.image(texture, place(self.niche_rect, origin, scale), Color::white());
        }
    }

    // The stele's slab with nothing carved on it, as draw_niche places things.
    pub fn draw_slab(&self, painter: &UiPainter, origin: Vec2, scale: f32) {
        if let Some(texture) = self.stele {
            painter.image(texture, place(self.def.sprite_rect(), origin, scale), Color::white());
        }
    }

    // The stele and its carved text, as draw_niche places things.
    pub fn draw_stele(&self, painter: &UiPainter, stele: &Stele, origin: Vec2, scale: f32) {
        self.draw_slab(painter, origin, scale);
        let layout = self.layout(painter, stele, origin, scale);
        for (text, at, look) in &layout.texts {
            painter.label(*at, UiTextAnchor::Start, text, look);
        }
        for (text, at, look) in &layout.titles {
            painter.label(*at, UiTextAnchor::Middle, text, look);
        }
        let color = |name: &str| self.def.colors.get(name).and_then(|name| skin::color(name)).unwrap_or(Color::black());
        if let Some((from, to)) = layout.rule {
            painter.line(from, to, (self.def.rule[2] * scale).max(1.0), color("rule"));
        }
        let [dash, gap] = self.def.row_dash;
        for (from, to) in &layout.rows {
            painter.dashed_polyline(&[*from, *to], scale.max(1.0), color("row"), (dash * scale).max(1.0), gap * scale);
        }
        if let (Some(texture), Some(centre)) = (self.mark, layout.mark) {
            let size = Vec2::new(self.def.mark_size[0], self.def.mark_size[1]) * layout.fit.max(0.75) * scale;
            painter.image(texture, Rect::from_pos_and_size(centre - size * 0.5, size), Color::white());
        }
    }

    // Where everything goes, the lines set at the largest fit (MIN_FIT to 1) that keeps them above
    // the slab's bottom.
    fn layout(&self, painter: &UiPainter, stele: &Stele, origin: Vec2, scale: f32) -> SteleLayout {
        let mut fit = 1.0;
        loop {
            let layout = self.layout_at(painter, stele, origin, scale, fit);
            if layout.fits || fit <= MIN_FIT {
                return layout;
            }
            fit = (fit * FIT_STEP).max(MIN_FIT);
        }
    }

    fn layout_at(&self, painter: &UiPainter, stele: &Stele, origin: Vec2, scale: f32, fit: f32) -> SteleLayout {
        let def = &self.def;
        let look = |role: &str, fit: f32| {
            let fallback = SkinMapTextDef::default();
            let text = def.looks.get(role).or_else(|| def.looks.get("line")).unwrap_or(&fallback);
            skin::label_look(text, text.size * fit, scale)
        };
        // In the stele rect's points, then on screen.
        let to_screen = |x: f32, y: f32| origin + (Vec2::new(def.rect[0] + x, def.rect[1] + y)) * scale;
        let [text_x, text_y, width] = def.text;
        let title_fit = (fit * TITLE_FIT).min(1.0);

        let mut layout = SteleLayout { fit, ..SteleLayout::default() };
        let title_look = look("title", title_fit);
        let title_size = def.looks.get("title").map_or(30.0, |title| title.size) * title_fit;
        let title_lines = painter.wrap_label(&stele.title, &title_look, width * scale);
        for (index, line) in title_lines.iter().enumerate() {
            let y = text_y + index as f32 * (title_size + def.title_gap * title_fit);
            layout.titles.push((line.clone(), to_screen(text_x + width * 0.5, y), title_look));
        }
        let mut y = text_y + title_lines.len() as f32 * (title_size + def.title_gap * title_fit) + def.title_space * fit;
        let [inset, above, _] = def.rule;
        layout.rule = Some((to_screen(text_x + inset, y - above * fit), to_screen(text_x + width - inset, y - above * fit)));

        let line_look = look("line", fit);
        let newest_look = look("newest", fit);
        let line_height = def.line_height * fit;
        let mut last_baseline = y;
        for line in &stele.lines {
            let look = if line.newest { newest_look } else { line_look };
            let wrapped = painter.wrap_label(&line.text, &look, width * scale);
            if line.newest && layout.mark.is_none() {
                let [dx, dy] = def.mark_offset;
                layout.mark = Some(to_screen(text_x + dx * fit, y + dy * fit));
            }
            for (index, text) in wrapped.iter().enumerate() {
                last_baseline = y + index as f32 * line_height;
                layout.texts.push((text.clone(), to_screen(text_x, last_baseline), look));
            }
            y += wrapped.len().max(1) as f32 * line_height + def.entry_gap * fit;
        }
        layout.fits = last_baseline <= def.bottom;

        // The empty rows for the missions to come, as many as fit.
        for index in 0..stele.rows_to_come.min(def.rows) {
            let row_y = y + index as f32 * def.row_gap * fit;
            if row_y > def.bottom {
                break;
            }
            layout.rows.push((to_screen(text_x, row_y), to_screen(text_x + width, row_y)));
        }
        layout
    }
}

// A rect in the composition's points on screen.
fn place([x, y, width, height]: [f32; 4], origin: Vec2, scale: f32) -> Rect {
    Rect::from_pos_and_size(origin + Vec2::new(x, y) * scale, Vec2::new(width, height) * scale)
}

// Where the stele's text goes on screen.
#[derive(Default)]
struct SteleLayout {
    fit: f32,
    fits: bool,
    titles: Vec<(String, Vec2, UiLabelLook)>, // Centred on the point.
    texts: Vec<(String, Vec2, UiLabelLook)>,  // From the point.
    rule: Option<(Vec2, Vec2)>,
    rows: Vec<(Vec2, Vec2)>,
    mark: Option<Vec2>,
}
