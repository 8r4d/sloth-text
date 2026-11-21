//! Syntax highlighting with `syntect`.
//!
//! syntect works out which parts of the text are keywords, strings, comments
//! and so on; the colours come from Sloth's own theme ([`SyntaxColors`]), so
//! highlighting always matches the chosen theme.
//!
//! Highlighting is cached line by line: after an edit, lines are only
//! re-highlighted from the changed one until the parser's state matches what
//! it was before, so typing in a big file stays quick.

use std::ops::Range;
use std::path::Path;
use std::str::FromStr;
use std::sync::OnceLock;

use eframe::egui::Color32;
use syntect::highlighting::{
    Color, FontStyle, HighlightState, Highlighter, RangedHighlightIterator, ScopeSelectors,
    StyleModifier, Theme, ThemeItem, ThemeSettings,
};
use syntect::parsing::{ParseState, ScopeStack, SyntaxReference, SyntaxSet};

/// Larger files aren't highlighted, to keep typing responsive.
pub const MAX_HIGHLIGHT_BYTES: usize = 2_000_000;

/// Colours for each kind of code element.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SyntaxColors {
    pub comment: Color32,
    pub keyword: Color32,
    pub string: Color32,
    pub number: Color32,
    pub function: Color32,
    pub type_: Color32,
}

/// A run of coloured text (byte range into the document).
#[derive(Clone, Debug, PartialEq)]
pub struct Span {
    pub range: Range<usize>,
    pub color: Color32,
    pub italic: bool,
}

fn syntax_set() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

/// The language for a file, if it's one we can highlight. Plain text
/// (`.txt`, unknown extensions) isn't highlighted.
pub fn detect(path: Option<&Path>, text: &str) -> Option<&'static SyntaxReference> {
    let set = syntax_set();
    let by_extension = path.and_then(|p| {
        let name = p.file_name()?.to_str()?;
        // Whole names first (Makefile, Dockerfile…), then the extension.
        set.find_syntax_by_extension(name)
            .or_else(|| set.find_syntax_by_extension(p.extension()?.to_str()?))
    });
    let syntax = by_extension.or_else(|| {
        // Scripts without an extension: look at the #! line.
        let first_line = text.lines().next()?;
        set.find_syntax_by_first_line(first_line)
    })?;
    (syntax.name != "Plain Text").then_some(syntax)
}

/// Is this a Markdown document (for the preview)?
pub fn is_markdown(path: Option<&Path>) -> bool {
    path.and_then(|p| p.extension())
        .and_then(|e| e.to_str())
        .is_some_and(|e| matches!(e.to_lowercase().as_str(), "md" | "markdown" | "mdown" | "mkd"))
}

/// Placeholder foreground meaning "no special colour: use the text colour".
const UNSTYLED: Color = Color { r: 1, g: 2, b: 3, a: 0 };

fn to_syntect(c: Color32) -> Color {
    Color { r: c.r(), g: c.g(), b: c.b(), a: 255 }
}

/// A syntect theme that maps scopes onto our colours.
fn build_theme(colors: SyntaxColors) -> Theme {
    let item = |selector: &str, color: Color32, italic: bool| ThemeItem {
        scope: ScopeSelectors::from_str(selector).expect("valid selector"),
        style: StyleModifier {
            foreground: Some(to_syntect(color)),
            background: None,
            font_style: italic.then_some(FontStyle::ITALIC),
        },
    };
    let c = colors;
    Theme {
        name: Some("Sloth".into()),
        author: None,
        settings: ThemeSettings { foreground: Some(UNSTYLED), ..ThemeSettings::default() },
        scopes: vec![
            item("comment, punctuation.definition.comment", c.comment, true),
            item("string, markup.raw, markup.inline.raw", c.string, false),
            item("constant.numeric, constant.language, constant.character", c.number, false),
            item("keyword - keyword.operator, storage, entity.name.tag", c.keyword, false),
            item(
                "entity.name.function, support.function, variable.function, \
                 entity.other.attribute-name, markup.underline.link, string.other.link",
                c.function,
                false,
            ),
            item("entity.name.type, entity.name.class, support.type, support.class", c.type_, false),
            item("markup.heading, entity.name.section", c.keyword, false),
            item("markup.italic", c.string, true),
        ],
    }
}

/// Per-document highlighting cache.
#[derive(Default)]
pub struct HighlightCache {
    key: Option<(String, SyntaxColors)>,
    theme: Option<Theme>,
    lines: Vec<CachedLine>,
}

struct CachedLine {
    text: String,
    start: (ParseState, HighlightState),
    end: (ParseState, HighlightState),
    /// Spans relative to the start of the line.
    spans: Vec<Span>,
}

impl HighlightCache {
    /// Coloured spans for the whole text.
    pub fn highlight(&mut self, text: &str, syntax: &SyntaxReference, colors: SyntaxColors) -> Vec<Span> {
        if text.len() > MAX_HIGHLIGHT_BYTES {
            return Vec::new();
        }
        let key = (syntax.name.clone(), colors);
        if self.key.as_ref() != Some(&key) {
            *self = Self {
                key: Some(key),
                theme: Some(build_theme(colors)),
                lines: Vec::new(),
            };
        }
        let theme = self.theme.as_ref().expect("set above");
        let highlighter = Highlighter::new(theme);
        let set = syntax_set();

        let mut out = Vec::new();
        let mut offset = 0;
        // `None` while in step with the cache (the state at this line is the
        // cached line's start state).
        let mut state: Option<(ParseState, HighlightState)> = None;
        let mut count = 0;

        for (i, line) in text.split_inclusive('\n').enumerate() {
            count = i + 1;
            let cached = self.lines.get(i);
            let reuse = cached.is_some_and(|c| {
                c.text == line && state.as_ref().is_none_or(|s| *s == c.start)
            });
            if reuse {
                let c = &self.lines[i];
                out.extend(c.spans.iter().map(|s| Span { range: s.range.start + offset..s.range.end + offset, ..s.clone() }));
                state = None;
                offset += line.len();
                continue;
            }

            let start = match state.take() {
                Some(s) => s,
                None if i < self.lines.len() => self.lines[i].start.clone(),
                None if i > 0 => self.lines[i - 1].end.clone(),
                None => (
                    ParseState::new(syntax),
                    HighlightState::new(&highlighter, ScopeStack::new()),
                ),
            };
            let (mut parse, mut highlight) = start.clone();
            let ops = parse.parse_line(line, set).unwrap_or_default();
            let mut spans = Vec::new();
            for (style, _, range) in RangedHighlightIterator::new(&mut highlight, &ops, line, &highlighter) {
                if style.foreground == UNSTYLED || range.is_empty() {
                    continue;
                }
                let color = Color32::from_rgb(style.foreground.r, style.foreground.g, style.foreground.b);
                let italic = style.font_style.contains(FontStyle::ITALIC);
                match spans.last_mut() {
                    // Merge neighbouring runs of the same style.
                    Some(Span { range: last, color: c, italic: it })
                        if last.end == range.start && *c == color && *it == italic =>
                    {
                        last.end = range.end;
                    }
                    _ => spans.push(Span { range, color, italic }),
                }
            }
            out.extend(spans.iter().map(|s| Span { range: s.range.start + offset..s.range.end + offset, ..s.clone() }));
            let end = (parse, highlight);
            let new = CachedLine { text: line.to_owned(), start, end: end.clone(), spans };
            if i < self.lines.len() {
                self.lines[i] = new;
            } else {
                self.lines.push(new);
            }
            state = Some(end);
            offset += line.len();
        }
        self.lines.truncate(count);
        out
    }

    #[cfg(test)]
    fn cached_lines(&self) -> usize {
        self.lines.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const COLORS: SyntaxColors = SyntaxColors {
        comment: Color32::from_rgb(1, 1, 1),
        keyword: Color32::from_rgb(2, 2, 2),
        string: Color32::from_rgb(3, 3, 3),
        number: Color32::from_rgb(4, 4, 4),
        function: Color32::from_rgb(5, 5, 5),
        type_: Color32::from_rgb(6, 6, 6),
    };

    fn color_of<'a>(spans: &'a [Span], text: &str, needle: &str) -> Option<&'a Span> {
        let at = text.find(needle).unwrap();
        spans.iter().find(|s| s.range.contains(&at))
    }

    #[test]
    fn detects_languages_by_name_extension_and_shebang() {
        assert_eq!(detect(Some(Path::new("main.rs")), "").unwrap().name, "Rust");
        assert_eq!(detect(Some(Path::new("Makefile")), "").unwrap().name, "Makefile");
        assert_eq!(detect(None, "#!/usr/bin/env python3\n").unwrap().name, "Python");
        assert!(detect(Some(Path::new("notes.txt")), "hello").is_none());
        assert!(detect(None, "just text").is_none());
        assert!(is_markdown(Some(Path::new("README.MD"))));
    }

    #[test]
    fn colours_keywords_strings_and_comments() {
        let rust = detect(Some(Path::new("a.rs")), "").unwrap();
        let text = "fn main() {\n    let s = \"hi\"; // note\n    let n = 42;\n}\n";
        let spans = HighlightCache::default().highlight(text, rust, COLORS);

        assert_eq!(color_of(&spans, text, "fn").unwrap().color, COLORS.keyword);
        assert_eq!(color_of(&spans, text, "\"hi\"").unwrap().color, COLORS.string);
        let comment = color_of(&spans, text, "// note").unwrap();
        assert_eq!(comment.color, COLORS.comment);
        assert!(comment.italic);
        assert_eq!(color_of(&spans, text, "42").unwrap().color, COLORS.number);
        assert_eq!(color_of(&spans, text, "main").unwrap().color, COLORS.function);
        // Plain identifiers keep the normal text colour (no span).
        assert!(color_of(&spans, text, "s =").is_none());
    }

    #[test]
    fn edits_give_the_same_result_as_highlighting_from_scratch() {
        let rust = detect(Some(Path::new("a.rs")), "").unwrap();
        let before = "let a = 1;\nlet b = 2;\nlet c = 3;\n";
        // Opening a block comment changes how the following lines parse.
        let after = "let a = 1;\n/* let b = 2;\nlet c = 3;\n";

        let mut cache = HighlightCache::default();
        cache.highlight(before, rust, COLORS);
        let incremental = cache.highlight(after, rust, COLORS);
        let fresh = HighlightCache::default().highlight(after, rust, COLORS);
        assert_eq!(incremental, fresh);
        assert_eq!(color_of(&incremental, after, "let c").unwrap().color, COLORS.comment);

        // Removing lines shrinks the cache.
        cache.highlight("let a = 1;\n", rust, COLORS);
        assert_eq!(cache.cached_lines(), 1);
    }
}
