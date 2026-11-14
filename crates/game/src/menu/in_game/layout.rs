// Where the HUD's parts go (docs/plans/realm-ui.md, Phase 5; hud-<realm>.jpg): the top bar from
// the window's left edge to the build column at the right, the minimap at the bottom left, the
// message log centred at the bottom. Sizes are in points and don't grow with the UI scale.

use engine::ui::widgets::UiWidgetContext;

use crate::menu::skin::{self, HudTileKind, SkinPanel};

// Room between the HUD's panels and the window's edges.
pub const MARGIN: f32 = 2.0;

// Room between two of the HUD's panels side by side (their frames' corner pieces reach past them).
pub const GAP: f32 = 22.0;

// Room on each side of the sector column's tiles, inside its panel's content.
pub const COLUMN_PAD: f32 = 4.0;

// The build column's panel: the HUD panel's art with less room inside its frame (the skin's
// `hud_column`), so the column fits its tiles closely.
pub const COLUMN_PANEL: &str = "hud_column";

// The build column's width: its sector tiles, the room round them and its panel's insets.
pub fn column_width(context: &mut dyn UiWidgetContext) -> f32 {
    let insets = match skin::panel(context, COLUMN_PANEL) {
        SkinPanel::NineSlice { content, .. } => content.left + content.right,
        SkinPanel::Sprite { .. } => 16.0,
    };
    skin::hud_tile_size(HudTileKind::Sector).x + 2.0 * COLUMN_PAD + insets
}
