// Decorated panels: a 9-slice panel (UiNineSlice) and ornaments hung on it, sprites anchored to
// the panel rect's edges that may reach past it: a crest over the top edge, blocks on the corners,
// stupas on piers, a naga along the bottom. The HUD's bars and panels are drawn this way.
//
// This file is the geometry only, with no egui: where each decoration goes for a panel rect, a
// pure function of the two. UiPainter::decorated_panel (immediate.rs) draws one, and so do the
// retained widgets that take a panel: UiMenu::with_panel, UiMessageBox::with_panel,
// UiTooltipText::with_panel and UiFrame::with_panel, which take anything that converts into a
// UiDecoratedPanel (a plain UiNineSlice does, with no decorations).
//
// Decorations are drawn only: they never change a widget's or window's size, its content rect,
// what it lays out, what takes the pointer or what the widget registry records (ui.layout_issues
// doesn't see them). The widgets draw them past their own clip rect (see each widget), since they
// reach past the panel by design.

use common::{Rect, Vec2};

use super::UiNineSlice;
use crate::render::texture::TextureHandle;

// ----------------------------------------------
// UiDecorationAnchor
// ----------------------------------------------

// The point on a rect's edge a decoration (or a badge) hangs from: a corner or an edge's middle.
#[derive(Copy, Clone, Default, PartialEq, Eq, Debug)]
pub enum UiDecorationAnchor {
    TopLeft,
    #[default]
    TopCenter,
    TopRight,
    LeftCenter,
    RightCenter,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

impl UiDecorationAnchor {
    // That point on `rect`.
    pub fn point(self, rect: Rect) -> Vec2 {
        let center = rect.center();
        match self {
            Self::TopLeft => rect.min,
            Self::TopCenter => Vec2::new(center.x, rect.min.y),
            Self::TopRight => Vec2::new(rect.max.x, rect.min.y),
            Self::LeftCenter => Vec2::new(rect.min.x, center.y),
            Self::RightCenter => Vec2::new(rect.max.x, center.y),
            Self::BottomLeft => Vec2::new(rect.min.x, rect.max.y),
            Self::BottomCenter => Vec2::new(center.x, rect.max.y),
            Self::BottomRight => rect.max,
        }
    }
}

// ----------------------------------------------
// UiPanelDecoration
// ----------------------------------------------

// A sprite hung on a panel: the whole texture stretched over `size` points, its centre `offset`
// points from the `anchor` point on the panel rect's edge (e.g. TopCenter with an offset of
// (0, -10) puts a crest's centre 10 points above the top edge). Drawn only on a panel rect at
// least `min_panel_size` large (a corner ornament that would crowd a small panel), after the
// panel, or before it when `behind` (a pier the panel stands on).
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct UiPanelDecoration {
    pub texture: TextureHandle,
    pub size: Vec2,
    pub anchor: UiDecorationAnchor,
    pub offset: Vec2,
    pub min_panel_size: Vec2,
    pub behind: bool,
}

impl UiPanelDecoration {
    // `texture` `size` points large, centred on the `anchor` point, always shown, over the panel.
    pub fn new(texture: TextureHandle, size: Vec2, anchor: UiDecorationAnchor) -> Self {
        Self { texture, size, anchor, offset: Vec2::zero(), min_panel_size: Vec2::zero(), behind: false }
    }

    // From the anchor point to the sprite's centre, in points.
    pub fn with_offset(mut self, offset: Vec2) -> Self {
        self.offset = offset;
        self
    }

    // Drawn only on a panel rect at least this large.
    pub fn with_min_panel_size(mut self, min_panel_size: Vec2) -> Self {
        self.min_panel_size = min_panel_size;
        self
    }

    // Drawn before the panel instead of after it.
    pub fn with_behind(mut self, behind: bool) -> Self {
        self.behind = behind;
        self
    }

    // Whether it shows on a panel over `panel_rect`.
    pub fn is_shown(&self, panel_rect: Rect) -> bool {
        self.size.x > 0.0
            && self.size.y > 0.0
            && panel_rect.width() >= self.min_panel_size.x
            && panel_rect.height() >= self.min_panel_size.y
    }

    // The rect it covers on a panel over `panel_rect`, or None where it doesn't show.
    pub fn rect(&self, panel_rect: Rect) -> Option<Rect> {
        if !self.is_shown(panel_rect) {
            return None;
        }
        let center = self.anchor.point(panel_rect) + self.offset;
        Some(Rect::from_pos_and_size(center - self.size * 0.5, self.size))
    }
}

// ----------------------------------------------
// UiDecoratedPanel
// ----------------------------------------------

// A 9-slice panel and the decorations hung on it, drawn behind ones first, then the panel, then
// the others, each in the order given.
#[derive(Clone, PartialEq, Debug)]
pub struct UiDecoratedPanel {
    pub panel: UiNineSlice,
    pub decorations: Vec<UiPanelDecoration>,
}

impl From<UiNineSlice> for UiDecoratedPanel {
    fn from(panel: UiNineSlice) -> Self {
        Self::new(panel)
    }
}

impl UiDecoratedPanel {
    // The panel alone, with no decorations yet.
    pub fn new(panel: UiNineSlice) -> Self {
        Self { panel, decorations: Vec::new() }
    }

    pub fn with_decoration(mut self, decoration: UiPanelDecoration) -> Self {
        self.decorations.push(decoration);
        self
    }

    pub fn with_decorations(mut self, decorations: impl IntoIterator<Item = UiPanelDecoration>) -> Self {
        self.decorations.extend(decorations);
        self
    }

    #[inline]
    pub fn has_decorations(&self) -> bool {
        !self.decorations.is_empty()
    }

    // The decorations drawn before (`behind`) or after the panel over `panel_rect`, with their
    // rects, in order; those that don't show are left out.
    pub fn decoration_rects(&self, panel_rect: Rect, behind: bool) -> impl Iterator<Item = (&UiPanelDecoration, Rect)> {
        self.decorations
            .iter()
            .filter(move |decoration| decoration.behind == behind)
            .filter_map(move |decoration| decoration.rect(panel_rect).map(|rect| (decoration, rect)))
    }

    // The rect the panel over `panel_rect` and its decorations cover together.
    pub fn bounds(&self, panel_rect: Rect) -> Rect {
        let mut bounds = panel_rect;
        for rect in self.decorations.iter().filter_map(|decoration| decoration.rect(panel_rect)) {
            bounds = union(bounds, rect);
        }
        bounds
    }

    // How far the decorations reach past `panel_rect` on each side, in points: left, top, right,
    // bottom (0 where they don't), e.g. to grow a clip rect by.
    pub fn overhang(&self, panel_rect: Rect) -> [f32; 4] {
        overhang(panel_rect, self.bounds(panel_rect))
    }
}

// The smallest rect holding both.
pub(crate) fn union(a: Rect, b: Rect) -> Rect {
    Rect {
        min: Vec2::new(a.min.x.min(b.min.x), a.min.y.min(b.min.y)),
        max: Vec2::new(a.max.x.max(b.max.x), a.max.y.max(b.max.y)),
    }
}

// How far `outer` reaches past `inner` on each side: left, top, right, bottom, never negative.
pub(crate) fn overhang(inner: Rect, outer: Rect) -> [f32; 4] {
    [
        (inner.min.x - outer.min.x).max(0.0),
        (inner.min.y - outer.min.y).max(0.0),
        (outer.max.x - inner.max.x).max(0.0),
        (outer.max.y - inner.max.y).max(0.0),
    ]
}

// ----------------------------------------------
// Unit tests
// ----------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
        Rect { min: Vec2::new(x0, y0), max: Vec2::new(x1, y1) }
    }

    fn decoration(anchor: UiDecorationAnchor) -> UiPanelDecoration {
        UiPanelDecoration::new(TextureHandle::White, Vec2::new(20.0, 10.0), anchor)
    }

    fn panel() -> UiNineSlice {
        UiNineSlice::new(TextureHandle::White, Vec2::new(32.0, 32.0), [8.0; 4])
    }

    #[test]
    fn anchors_are_the_corners_and_edge_middles() {
        let r = rect(10.0, 20.0, 110.0, 70.0);
        let expected = [
            (UiDecorationAnchor::TopLeft, Vec2::new(10.0, 20.0)),
            (UiDecorationAnchor::TopCenter, Vec2::new(60.0, 20.0)),
            (UiDecorationAnchor::TopRight, Vec2::new(110.0, 20.0)),
            (UiDecorationAnchor::LeftCenter, Vec2::new(10.0, 45.0)),
            (UiDecorationAnchor::RightCenter, Vec2::new(110.0, 45.0)),
            (UiDecorationAnchor::BottomLeft, Vec2::new(10.0, 70.0)),
            (UiDecorationAnchor::BottomCenter, Vec2::new(60.0, 70.0)),
            (UiDecorationAnchor::BottomRight, Vec2::new(110.0, 70.0)),
        ];
        for (anchor, point) in expected {
            assert_eq!(anchor.point(r), point, "{anchor:?}");
            // Centred on the point with no offset.
            assert_eq!(decoration(anchor).rect(r), Some(Rect::from_pos_and_size(point - Vec2::new(10.0, 5.0), Vec2::new(20.0, 10.0))), "{anchor:?}");
        }
    }

    #[test]
    fn offset_moves_the_centre_from_the_anchor() {
        let r = rect(0.0, 0.0, 200.0, 40.0);
        // A crest 30 × 24 whose centre is 8 points above the top edge's middle.
        let crest = UiPanelDecoration::new(TextureHandle::White, Vec2::new(30.0, 24.0), UiDecorationAnchor::TopCenter).with_offset(Vec2::new(0.0, -8.0));
        assert_eq!(crest.rect(r), Some(rect(85.0, -20.0, 115.0, 4.0)));

        // A corner block pulled out to the bottom right.
        let block = decoration(UiDecorationAnchor::BottomRight).with_offset(Vec2::new(4.0, 2.0));
        assert_eq!(block.rect(r), Some(rect(194.0, 37.0, 214.0, 47.0)));
    }

    #[test]
    fn min_panel_size_hides_it_on_a_small_panel() {
        let corner = decoration(UiDecorationAnchor::TopLeft).with_min_panel_size(Vec2::new(100.0, 50.0));
        assert!(corner.rect(rect(0.0, 0.0, 100.0, 50.0)).is_some()); // Exactly as large.
        assert!(corner.rect(rect(0.0, 0.0, 99.0, 80.0)).is_none()); // Too narrow.
        assert!(corner.rect(rect(0.0, 0.0, 180.0, 49.0)).is_none()); // Too short.

        let empty = UiPanelDecoration::new(TextureHandle::White, Vec2::new(0.0, 10.0), UiDecorationAnchor::TopLeft);
        assert!(empty.rect(rect(0.0, 0.0, 100.0, 100.0)).is_none());
    }

    #[test]
    fn decorations_split_behind_and_front_in_order() {
        let decorated = UiDecoratedPanel::new(panel())
            .with_decoration(decoration(UiDecorationAnchor::TopLeft))
            .with_decoration(decoration(UiDecorationAnchor::BottomCenter).with_behind(true))
            .with_decoration(decoration(UiDecorationAnchor::TopRight).with_min_panel_size(Vec2::new(500.0, 0.0)))
            .with_decoration(decoration(UiDecorationAnchor::BottomRight));
        let r = rect(0.0, 0.0, 100.0, 50.0);

        let front: Vec<_> = decorated.decoration_rects(r, false).map(|(decoration, _)| decoration.anchor).collect();
        assert_eq!(front, [UiDecorationAnchor::TopLeft, UiDecorationAnchor::BottomRight]); // TopRight hidden.
        let behind: Vec<_> = decorated.decoration_rects(r, true).map(|(decoration, _)| decoration.anchor).collect();
        assert_eq!(behind, [UiDecorationAnchor::BottomCenter]);
    }

    #[test]
    fn bounds_and_overhang_cover_the_shown_decorations() {
        let r = rect(0.0, 0.0, 100.0, 50.0);
        assert_eq!(UiDecoratedPanel::from(panel()).bounds(r), r);
        assert_eq!(UiDecoratedPanel::from(panel()).overhang(r), [0.0; 4]);

        let decorated = UiDecoratedPanel::new(panel())
            .with_decoration(UiPanelDecoration::new(TextureHandle::White, Vec2::new(30.0, 24.0), UiDecorationAnchor::TopCenter).with_offset(Vec2::new(0.0, -8.0)))
            .with_decoration(decoration(UiDecorationAnchor::BottomLeft))
            .with_decoration(decoration(UiDecorationAnchor::RightCenter).with_min_panel_size(Vec2::new(1000.0, 0.0)));
        // The crest reaches 20 above, the bottom left block 10 left and 5 below; the hidden one
        // counts for nothing.
        assert_eq!(decorated.bounds(r), rect(-10.0, -20.0, 100.0, 55.0));
        assert_eq!(decorated.overhang(r), [10.0, 20.0, 0.0, 5.0]);
    }
}
