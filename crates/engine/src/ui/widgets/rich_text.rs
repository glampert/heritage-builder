use std::any::Any;
use egui::{Sense, text::LayoutJob, vec2};

use common::{Color, Vec2};

use super::{UiWidget, UiWidgetContext};
use crate::ui::{
    UiFontScale, UiString, UiSystem,
    immediate::{self, color32},
    internal,
};

// ----------------------------------------------
// UiTextSpan
// ----------------------------------------------

// A run of text in one look: emphasized (italic, in the widget's emphasis colour unless it has a
// colour of its own) and coloured by a name the widget knows (with_color), "#RRGGBB(AA)", or the
// widget's text colour if None.
#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub struct UiTextSpan {
    pub text: String,
    pub emphasis: bool,
    pub color: Option<String>,
}

impl UiTextSpan {
    pub fn plain(text: impl Into<String>) -> Self {
        Self { text: text.into(), emphasis: false, color: None }
    }

    pub fn emphasized(text: impl Into<String>) -> Self {
        Self { text: text.into(), emphasis: true, color: None }
    }

    pub fn colored(text: impl Into<String>, color: impl Into<String>) -> Self {
        Self { text: text.into(), emphasis: false, color: Some(color.into()) }
    }
}

// Parses the rich text markup: `*emphasis*`, `[name]coloured text[/]` (a colour the widget names,
// or `[#a8261c]`), nestable one in the other; `\*`, `\[` and `\\` are the characters themselves.
// Text keeps its line breaks. An unclosed `*` or `[name]` runs to the end; a `[/]` with nothing
// open, or a `[` that doesn't make a tag, is text. Adjacent runs in the same look are merged.
pub fn parse_rich_text(markup: &str) -> Vec<UiTextSpan> {
    let mut spans: Vec<UiTextSpan> = Vec::new();
    let mut text = String::new();
    let mut emphasis = false;
    let mut colors: Vec<String> = Vec::new();

    let flush = |spans: &mut Vec<UiTextSpan>, text: &mut String, emphasis: bool, color: Option<&String>| {
        if text.is_empty() {
            return;
        }
        match spans.last_mut() {
            Some(last) if last.emphasis == emphasis && last.color.as_ref() == color => last.text.push_str(text),
            _ => spans.push(UiTextSpan { text: text.clone(), emphasis, color: color.cloned() }),
        }
        text.clear();
    };

    let mut chars = markup.char_indices().peekable();
    while let Some((index, ch)) = chars.next() {
        match ch {
            '\\' => match chars.peek() {
                Some(&(_, next @ ('*' | '[' | '\\'))) => {
                    text.push(next);
                    chars.next();
                }
                _ => text.push('\\'),
            },
            '*' => {
                flush(&mut spans, &mut text, emphasis, colors.last());
                emphasis = !emphasis;
            }
            '[' => {
                let rest = &markup[index + 1..];
                let tag = rest.find(']').map(|end| &rest[..end]).filter(|tag| is_tag(tag));
                match tag {
                    Some(tag) => {
                        flush(&mut spans, &mut text, emphasis, colors.last());
                        if tag == "/" {
                            if colors.pop().is_none() {
                                text.push_str("[/]"); // Nothing open: it's text.
                            }
                        } else {
                            colors.push(tag.to_string());
                        }
                        // Skip the tag and its closing bracket.
                        for _ in 0..tag.chars().count() + 1 {
                            chars.next();
                        }
                    }
                    None => text.push('['),
                }
            }
            _ => text.push(ch),
        }
    }
    flush(&mut spans, &mut text, emphasis, colors.last());
    spans
}

// `[/]`, `[name]` (letters, digits, '_' and '-') or `[#hex]`.
fn is_tag(tag: &str) -> bool {
    if tag == "/" {
        return true;
    }
    let name = tag.strip_prefix('#').unwrap_or(tag);
    !name.is_empty() && name.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
}

// "#RRGGBB" or "#RRGGBBAA". None otherwise.
pub fn parse_hex_color(text: &str) -> Option<Color> {
    let hex = text.strip_prefix('#')?;
    if !(hex.len() == 6 || hex.len() == 8) || !hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16).ok().map(|value| value as f32 / 255.0);
    let alpha = if hex.len() == 8 { channel(6)? } else { 1.0 };
    Some(Color::new(channel(0)?, channel(2)?, channel(4)?, alpha))
}

// The text of `spans`, without their looks.
pub fn plain_text(spans: &[UiTextSpan]) -> String {
    spans.iter().map(|span| span.text.as_str()).collect()
}

// ----------------------------------------------
// UiRichText
// ----------------------------------------------

enum UiRichTextSource {
    Markup(UiString), // Parsed each time it's drawn, so a key follows a language switch.
    Spans(Vec<UiTextSpan>),
}

#[derive(Copy, Clone, Default, PartialEq, Eq, Debug)]
pub enum UiTextAlign {
    #[default]
    Left,
    Center,
    Right,
}

// Wrapped text whose runs differ in emphasis and colour: captions, a briefing, the stele's lines
// with the newest one highlighted. From markup (parse_rich_text) or a span list. Recorded as kind
// "rich_text", labeled by its own label or else its text, its value the text.
pub struct UiRichText {
    source: UiRichTextSource,
    label: UiString,
    font_scale: UiFontScale,
    width: Option<f32>, // The menu's width if None.
    align: UiTextAlign,
    text_color: Option<Color>,     // The style's if None.
    emphasis_color: Option<Color>, // The text colour if None.
    colors: Vec<(String, Color)>,  // The names spans give.
    line_spacing: f32,
}

impl UiWidget for UiRichText {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn draw(&mut self, context: &mut dyn UiWidgetContext) {
        debug_assert!(context.is_inside_widget_window());

        let ui_sys = context.ui_sys();
        let spans = self.spans();

        ui_sys.with_ui(|ui| {
            let width = self.width.unwrap_or_else(|| immediate::fill_size(ui).x);
            let default_color = self.text_color.map_or_else(|| ui.visuals().text_color(), color32);
            let job = self.layout_job(ui_sys, &spans, width, default_color);
            let galley = ui.ctx().fonts_mut(|fonts| fonts.layout_job(job));

            let size = vec2(if width > 0.0 { width } else { galley.size().x }, galley.size().y);
            let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
            // egui lines rows up on x = 0: their left, centre or right.
            let x = match self.align {
                UiTextAlign::Left => rect.left(),
                UiTextAlign::Center => rect.center().x,
                UiTextAlign::Right => rect.right(),
            };
            ui.painter().galley(egui::pos2(x, rect.top()), galley, default_color);

            let text = plain_text(&spans);
            let label = if self.label.is_empty() { text.as_str() } else { self.label.resolve() };
            immediate::record(ui_sys, ui, &response, "rich_text", label, None, true, Some(serde_json::Value::String(text.clone())));
        });
    }

    fn measure(&self, context: &dyn UiWidgetContext) -> Vec2 {
        let ui_sys = context.ui_sys();
        let width = self.width.unwrap_or_else(|| internal::available_size(ui_sys).x);
        let job = self.layout_job(ui_sys, &self.spans(), width, egui::Color32::WHITE);
        let size = ui_sys.ctx().fonts_mut(|fonts| fonts.layout_job(job).size());
        Vec2::new(if width > 0.0 { width } else { size.x }, size.y)
    }

    fn label(&self) -> &str {
        self.label.resolve()
    }

    fn font_scale(&self) -> UiFontScale {
        self.font_scale
    }
}

impl UiRichText {
    // Text in the markup parse_rich_text reads, e.g. "The *flood* came [accent]early[/] this year."
    pub fn new(markup: impl Into<UiString>) -> Self {
        Self::with_source(UiRichTextSource::Markup(markup.into()))
    }

    // Text made of `spans`, e.g. lines the game puts together.
    pub fn from_spans(spans: impl IntoIterator<Item = UiTextSpan>) -> Self {
        Self::with_source(UiRichTextSource::Spans(spans.into_iter().collect()))
    }

    fn with_source(source: UiRichTextSource) -> Self {
        Self {
            source,
            label: UiString::default(),
            font_scale: UiFontScale::default(),
            width: None,
            align: UiTextAlign::Left,
            text_color: None,
            emphasis_color: None,
            colors: Vec::new(),
            line_spacing: 0.0,
        }
    }

    // Names it in widget paths, in place of its text.
    pub fn with_label(mut self, label: impl Into<UiString>) -> Self {
        self.label = label.into();
        self
    }

    pub fn with_font_scale(mut self, font_scale: UiFontScale) -> Self {
        debug_assert!(font_scale.is_valid());
        self.font_scale = font_scale;
        self
    }

    // Wrapped at this width; at the menu's width by default.
    pub fn with_width(mut self, width: f32) -> Self {
        debug_assert!(width > 0.0);
        self.width = Some(width);
        self
    }

    pub fn with_align(mut self, align: UiTextAlign) -> Self {
        self.align = align;
        self
    }

    pub fn with_text_color(mut self, color: Color) -> Self {
        self.text_color = Some(color);
        self
    }

    // The colour of emphasized runs without a colour of their own.
    pub fn with_emphasis_color(mut self, color: Color) -> Self {
        self.emphasis_color = Some(color);
        self
    }

    // A colour runs can name, e.g. ("accent", the skin's accent).
    pub fn with_color(mut self, name: impl Into<String>, color: Color) -> Self {
        self.colors.push((name.into(), color));
        self
    }

    // Extra room between lines, in points.
    pub fn with_line_spacing(mut self, spacing: f32) -> Self {
        self.line_spacing = spacing;
        self
    }

    pub fn set_markup(&mut self, markup: impl Into<UiString>) {
        self.source = UiRichTextSource::Markup(markup.into());
    }

    pub fn set_spans(&mut self, spans: impl IntoIterator<Item = UiTextSpan>) {
        self.source = UiRichTextSource::Spans(spans.into_iter().collect());
    }

    pub fn spans(&self) -> Vec<UiTextSpan> {
        match &self.source {
            UiRichTextSource::Markup(markup) => parse_rich_text(markup.resolve()),
            UiRichTextSource::Spans(spans) => spans.clone(),
        }
    }

    fn named_color(&self, name: &str) -> Option<Color> {
        parse_hex_color(name).or_else(|| self.colors.iter().find(|(known, _)| known == name).map(|(_, color)| *color))
    }

    fn layout_job(&self, ui_sys: &UiSystem, spans: &[UiTextSpan], width: f32, default_color: egui::Color32) -> LayoutJob {
        let font = internal::font(ui_sys, self.font_scale);
        let mut job = LayoutJob::default();
        job.wrap.max_width = if width > 0.0 { width } else { f32::INFINITY };
        job.halign = match self.align {
            UiTextAlign::Left => egui::Align::LEFT,
            UiTextAlign::Center => egui::Align::Center,
            UiTextAlign::Right => egui::Align::RIGHT,
        };

        // egui fakes italics by shearing the glyphs, whose tops then lean into what follows:
        // the text after an emphasis run starts a little further on, so its space stays visible.
        let italic_overhang = font.size * 0.2;
        let mut after_emphasis = false;

        for span in spans {
            let color = match &span.color {
                Some(name) => self.named_color(name).map(color32),
                None if span.emphasis => self.emphasis_color.map(color32),
                None => None,
            };
            let format = egui::TextFormat {
                font_id: font.clone(),
                color: color.unwrap_or(default_color),
                italics: span.emphasis,
                line_height: (self.line_spacing != 0.0).then(|| ui_sys.ctx().fonts_mut(|fonts| fonts.row_height(&font)) + self.line_spacing),
                ..Default::default()
            };
            let leading_space = if after_emphasis && !span.emphasis { italic_overhang } else { 0.0 };
            job.append(&span.text, leading_space, format);
            after_emphasis = span.emphasis;
        }
        job
    }
}

// ----------------------------------------------
// Unit tests
// ----------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn span(text: &str, emphasis: bool, color: Option<&str>) -> UiTextSpan {
        UiTextSpan { text: text.into(), emphasis, color: color.map(str::to_string) }
    }

    #[test]
    fn plain_text_is_one_span() {
        assert_eq!(parse_rich_text("The river rose."), [span("The river rose.", false, None)]);
        assert!(parse_rich_text("").is_empty());
    }

    #[test]
    fn emphasis_and_colours_nest() {
        assert_eq!(
            parse_rich_text("The *flood* came [accent]early, *very* early[/] this year."),
            [
                span("The ", false, None),
                span("flood", true, None),
                span(" came ", false, None),
                span("early, ", false, Some("accent")),
                span("very", true, Some("accent")),
                span(" early", false, Some("accent")),
                span(" this year.", false, None),
            ]
        );
        assert_eq!(parse_rich_text("[#a8261c]red[/]"), [span("red", false, Some("#a8261c"))]);
    }

    #[test]
    fn escapes_and_stray_brackets_are_text() {
        assert_eq!(parse_rich_text(r"2 \* 3 \[x] \\"), [span(r"2 * 3 [x] \", false, None)]);
        assert_eq!(parse_rich_text("a [not a tag] b"), [span("a [not a tag] b", false, None)]);
        assert_eq!(parse_rich_text("a [/] b"), [span("a [/] b", false, None)]);
        assert_eq!(parse_rich_text("a [b"), [span("a [b", false, None)]);
    }

    #[test]
    fn unclosed_runs_go_to_the_end_and_empty_runs_vanish() {
        assert_eq!(parse_rich_text("a *b"), [span("a ", false, None), span("b", true, None)]);
        assert_eq!(parse_rich_text("[sub]a"), [span("a", false, Some("sub"))]);
        assert_eq!(parse_rich_text("**a"), [span("a", false, None)]);
        assert_eq!(parse_rich_text("Đại *Long*\nline two"), [span("Đại ", false, None), span("Long", true, None), span("\nline two", false, None)]);
    }

    #[test]
    fn hex_colours() {
        assert_eq!(parse_hex_color("#ff0000"), Some(Color::new(1.0, 0.0, 0.0, 1.0)));
        assert_eq!(parse_hex_color("#00000000"), Some(Color::new(0.0, 0.0, 0.0, 0.0)));
        assert_eq!(parse_hex_color("ff0000"), None);
        assert_eq!(parse_hex_color("#ff00"), None);
        assert_eq!(parse_hex_color("#gg0000"), None);
    }
}
