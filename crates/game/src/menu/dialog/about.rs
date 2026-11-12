// About (docs/plans/realm-ui.md, Phase 1, About), as the menu-about mockups draw it: the realm's
// crest, the game's title and the realm's campaign under it, the studio line, a rule, the credit
// sections (a role in the accent colour over its names), "Built with", the quote, the version and
// copyright, and Back. The credits that have no names yet say so, as the mockup does; the fonts
// are credited from assets/fonts/ and their licences there. Its contents scale with the panel's
// content area (skin::dialog_content_scale), as Home's do, so they fit down to 1280 × 720.

use super::{
    home::{crest, subtitle},
    *,
};

// ----------------------------------------------
// Layout (points on a 1920 × 1080 window, scaled with the content area: dialog_content_scale)
// ----------------------------------------------

const ABOUT_WIDGET_SPACING: f32 = 6.0;     // Between the panel's parts, and a section's lines.
const ABOUT_CREST_HEIGHT: f32 = 64.0;      // The crest is drawn this tall at most, keeping its shape.
const ABOUT_STUDIO_FONT_SCALE: f32 = 1.1;  // "A City Builder by Core System Games".
const ABOUT_ROLE_FONT_SCALE: f32 = 0.8;    // A credit section's role, in capitals.
const ABOUT_NAMES_FONT_SCALE: f32 = 1.0;   // Its names.
const ABOUT_SECTION_SPACING: f32 = 14.0;   // Between credit sections.
const ABOUT_COLUMN_SPACING: f32 = 24.0;    // Between the credits' two columns.
const ABOUT_QUOTE_FONT_SCALE: f32 = 1.2;
const ABOUT_FOOTER_FONT_SCALE: f32 = 0.85; // The version and copyright.
const ABOUT_BACK_BUTTON_WIDTH: f32 = 240.0;
const ABOUT_BACK_BUTTON_HEIGHT: f32 = 56.0;
const ABOUT_SLACK: f32 = 4.0;              // Room the content area keeps free below Back, so rounding never makes it scroll.

// ----------------------------------------------
// About
// ----------------------------------------------

pub struct About {
    menu: UiMenuRcMut,
}

implement_dialog_menu! { About, ["About"] }

impl About {
    pub fn new(context: &mut GameUiContext) -> Self {
        let kind = Self::KIND.to_string();
        let window = ui::screen_size(context.ui_sys);
        let scale = skin::dialog_content_scale(&kind, window);
        let spacing = (ABOUT_WIDGET_SPACING * scale).round();
        let flags = DialogFrame::from_skin(Self::KIND).menu_flags();
        let mut menu = skin::dialog(context, &kind, flags).with_widget_spacing(Vec2::new(0.0, spacing));

        // The parts, measured as they're added: the quote, the footer and Back go at the bottom of
        // the content area, as the mockups put Back on the panel's bottom edge, a blank filler
        // taking the room the credits leave.
        let mut parts_height = 0.0;
        if let Some(crest) = crest(context, ABOUT_CREST_HEIGHT, scale) {
            parts_height += crest.measure(context).y + spacing;
            menu.add_widget(crest);
        }
        let title = skin::heading_at(context, [UiString::key(UiTextCategory::Menu, "home.title")], HeadingLevel::Title, scale);
        parts_height += title.measure(context).y + spacing;
        menu.add_widget(title);
        let subtitle = skin::heading_at(context, [UiString::from(subtitle())], HeadingLevel::Subtitle, scale);
        parts_height += subtitle.measure(context).y + spacing;
        menu.add_widget(subtitle);
        let studio = text_block([line(UiString::key(UiTextCategory::Menu, "about.studio"), ABOUT_STUDIO_FONT_SCALE * scale, "ink")], 0.0);
        parts_height += studio.measure(context).y + spacing;
        menu.add_widget(studio);
        let rule = skin::separator(context, "rule");
        parts_height += rule.measure(context).y + spacing;
        menu.add_widget(rule);
        let credits = credits(scale);
        parts_height += credits.measure(context).y + spacing;
        menu.add_widget(credits);

        let quote = text_block([line(UiString::key(UiTextCategory::Menu, "about.quote"), ABOUT_QUOTE_FONT_SCALE * scale, "sub")], 0.0);
        let footer = ui::text::format(UiTextCategory::Menu, "about.footer", &[("version", crate::version())]);
        let footer = text_block([line(footer, ABOUT_FOOTER_FONT_SCALE * scale, "sub")], 0.0);
        let back = back_button(context, scale);
        parts_height += quote.measure(context).y + spacing + footer.measure(context).y + spacing + back.measure(context).y;

        let content = skin::dialog_content_size(&kind, window);
        let room = (content.y - parts_height - spacing - ABOUT_SLACK * scale).floor();
        if room > 0.0 {
            menu.add_widget(UiSeparator::new().with_size(Vec2::new(1.0, room)));
        }
        menu.add_widget(quote);
        menu.add_widget(footer);
        menu.add_widget(back);

        Self { menu: menu.into_rc() }
    }
}

// A line of text at `font_scale` in the skin's colour `color`.
fn line(text: impl Into<UiString>, font_scale: f32, color: &str) -> UiText {
    let text = UiText::new(text, UiFontScale(font_scale));
    match skin::color(color) {
        Some(color) => text.with_color(color),
        None => text,
    }
}

// Lines stacked and centered, with `margin_bottom` under them.
fn text_block(lines: impl IntoIterator<Item = UiText>, margin_bottom: f32) -> UiMenuHeading {
    UiMenuHeading::new(lines).with_margin_top(0.0).with_margin_bottom(margin_bottom)
}

// The credit sections, in two columns of three: each a role and its names.
fn credit_sections() -> [[(UiString, Vec<UiString>); 3]; 2] {
    let tba = || vec![UiString::key(UiTextCategory::Menu, "about.credits.tba")];
    [
        [
            (UiString::key(UiTextCategory::Menu, "about.credits.design"), vec![UiString::key(UiTextCategory::Menu, "about.credits.design.names")]),
            (UiString::key(UiTextCategory::Menu, "about.credits.art"), tba()),
            (UiString::key(UiTextCategory::Menu, "about.credits.music"), tba()),
        ],
        [
            (UiString::key(UiTextCategory::Menu, "about.credits.consultants"), tba()),
            (
                UiString::key(UiTextCategory::Menu, "about.credits.fonts"),
                vec![
                    UiString::key(UiTextCategory::Menu, "about.credits.fonts.names"),
                    UiString::key(UiTextCategory::Menu, "about.credits.fonts.names_2"),
                    UiString::key(UiTextCategory::Menu, "about.credits.fonts.licence"),
                ],
            ),
            (
                UiString::key(UiTextCategory::Menu, "about.credits.built_with"),
                vec![
                    UiString::key(UiTextCategory::Menu, "about.credits.built_with.names"),
                    UiString::key(UiTextCategory::Menu, "about.credits.built_with.names_2"),
                ],
            ),
        ],
    ]
}

// The credit sections in two columns, each section a heading: its role in capitals and the accent
// colour, then its names (paths: "About/ART To be announced"). The mockup stacks five in one
// column; the fonts' credits make six, which fit the panel side by side.
fn credits(scale: f32) -> UiColumns {
    let mut columns = UiColumns::new().with_spacing((ABOUT_COLUMN_SPACING * scale).round());
    for sections in credit_sections() {
        // The menu's spacing between lines (a group spaces its headings' lines as its widgets, but
        // measures them at the menu's), and a margin under each section.
        let mut group = UiWidgetGroup::new().with_widget_spacing(Vec2::new(0.0, (ABOUT_WIDGET_SPACING * scale).round()));
        let count = sections.len();
        for (index, (role, names)) in sections.into_iter().enumerate() {
            let role = role.resolve().to_uppercase();
            let lines = std::iter::once(line(role, ABOUT_ROLE_FONT_SCALE * scale, "accent"))
                .chain(names.into_iter().map(|name| line(name, ABOUT_NAMES_FONT_SCALE * scale, "ink")));
            let margin = if index + 1 < count { (ABOUT_SECTION_SPACING * scale).round() } else { 0.0 };
            group.add_widget(text_block(lines, margin));
        }
        columns.add_column(UiColumnWidth::Fraction(1.0), group);
    }
    columns
}

// Back, the skin's primary button, centered: closes About.
fn back_button(context: &mut GameUiContext, scale: f32) -> UiWidgetGroup {
    let back = make_dialog_close_button(context, ButtonRole::Primary, UiString::key(UiTextCategory::Menu, "button.back"))
        .with_width(UiTextButtonWidth::Fixed((ABOUT_BACK_BUTTON_WIDTH * scale).round()))
        .with_min_height((ABOUT_BACK_BUTTON_HEIGHT * scale).round());
    UiWidgetGroup::new().with_widget(back)
}
