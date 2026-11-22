//! The main editing surface: a monospace text area with a line-number gutter.

use std::ops::Range;

use eframe::egui::{
    self, Align, Align2, Color32, FontId, Id, Key, Modifiers, Rect, ScrollArea, Sense, Shape,
    Stroke, TextEdit, Vec2,
    text::{CCursor, CCursorRange, LayoutJob, TextFormat},
    text_edit::TextEditState,
};

use crate::editing::{self, Edit, Indent};
use crate::syntax::{HighlightCache, Span, SyntaxColors};
use crate::tabs::TabId;

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct EditorSettings {
    pub show_line_numbers: bool,
    pub word_wrap: bool,
    pub font_size: f32,
    /// Installed font family for the text, or `None` for the built-in one.
    pub font: Option<String>,
    /// Tab key inserts spaces rather than a tab character.
    pub indent_with_spaces: bool,
    /// Columns per tab stop (and spaces per indent level).
    pub tab_width: usize,
    /// New lines start with the previous line's indentation.
    pub auto_indent: bool,
    pub highlight_current_line: bool,
    pub syntax_highlighting: bool,
    /// Show a rendered preview beside Markdown documents.
    pub markdown_preview: bool,
    /// Typing `--` makes an em dash (Sloth Writer; off in Sloth).
    pub smart_dashes: bool,
    /// Set in the prose font (see `fonts::prose_family`) rather than the
    /// fixed-width editor font (Sloth Writer; off in Sloth).
    pub prose: bool,
    /// Line height as a multiple of the font's usual one.
    pub line_spacing: f32,
    /// With word wrap, keep lines at most this wide (points), centred in the
    /// editor. `None` uses the full width.
    pub max_text_width: Option<f32>,
}

impl EditorSettings {
    pub const DEFAULT_FONT_SIZE: f32 = 14.0;

    pub fn indent(&self) -> Indent {
        Indent {
            spaces: self.indent_with_spaces,
            width: self.tab_width.max(1),
        }
    }
}

impl Default for EditorSettings {
    fn default() -> Self {
        Self {
            show_line_numbers: true,
            word_wrap: false,
            font_size: Self::DEFAULT_FONT_SIZE,
            font: None,
            indent_with_spaces: false,
            tab_width: 4,
            auto_indent: true,
            highlight_current_line: true,
            syntax_highlighting: true,
            markdown_preview: false,
            smart_dashes: false,
            prose: false,
            line_spacing: 1.0,
            max_text_width: None,
        }
    }
}

/// Search matches to highlight (byte ranges into the text).
#[derive(Default)]
pub struct Highlights<'a> {
    pub all: &'a [Range<usize>],
    pub current: Option<Range<usize>>,
}

/// What's needed to colour code in the editor.
pub struct SyntaxInput<'a> {
    pub syntax: &'static syntect::parsing::SyntaxReference,
    pub colors: SyntaxColors,
    pub cache: &'a mut HighlightCache,
}

/// Information the editor reports back to the app each frame.
#[derive(Default, Clone, Copy)]
pub struct CursorInfo {
    /// 1-based line and column of the cursor, if the editor has a cursor.
    pub line_col: Option<(usize, usize)>,
}

/// Each tab has its own text widget, so cursor, undo history and scroll
/// position are kept per tab.
pub fn editor_id(tab: TabId) -> Id {
    // Include the process id so state saved by older versions (which did
    // persist widget state) can never be picked up by a tab in this session.
    Id::unique(("sloth-editor", std::process::id(), tab))
}

/// The selected character range in a tab's editor (sorted), if any.
pub fn selection(ctx: &egui::Context, tab: TabId) -> Option<Range<usize>> {
    let range = TextEditState::load(ctx, editor_id(tab))?.cursor.char_range()?;
    let (a, b) = (range.primary.index.0, range.secondary.index.0);
    Some(a.min(b)..a.max(b))
}

/// Select a character range in a tab's editor.
pub fn set_selection(ctx: &egui::Context, tab: TabId, chars: Range<usize>) {
    let id = editor_id(tab);
    let mut state = TextEditState::load(ctx, id).unwrap_or_default();
    state.cursor.set_char_range(Some(CCursorRange::two(
        CCursor::new(chars.start),
        CCursor::new(chars.end),
    )));
    state.store(ctx, id);
}

/// Apply an editing operation to a tab's text at its current selection.
pub fn apply_edit(
    ctx: &egui::Context,
    tab: TabId,
    text: &mut String,
    op: impl FnOnce(&str, Range<usize>) -> Edit,
) {
    let selection = selection(ctx, tab).unwrap_or(0..0);
    let edit = op(text, selection);
    *text = edit.text;
    set_selection(ctx, tab, edit.selection);
}

/// Enter, Tab and Shift-Tab, handled before the text widget sees them so
/// we can auto-indent and indent with spaces or whole lines.
fn handle_editing_keys(ui: &egui::Ui, tab: TabId, text: &mut String, settings: &EditorSettings) {
    if !ui.memory(|m| m.has_focus(editor_id(tab))) {
        return;
    }
    let indent = settings.indent();
    let (outdents, tabs, newlines) = ui.input_mut(|i| {
        let count = |i: &mut egui::InputState, mods, key| {
            let mut n = 0;
            while i.consume_key(mods, key) {
                n += 1;
            }
            n
        };
        // Shift-Tab first, so plain Tab doesn't also match it.
        let outdents = count(i, Modifiers::SHIFT, Key::Tab);
        let tabs = count(i, Modifiers::NONE, Key::Tab);
        let newlines = if settings.auto_indent { count(i, Modifiers::NONE, Key::Enter) } else { 0 };
        (outdents, tabs, newlines)
    });
    let ctx = ui.ctx();
    for _ in 0..outdents {
        apply_edit(ctx, tab, text, |t, s| editing::outdent_lines(t, s, indent));
    }
    for _ in 0..tabs {
        apply_edit(ctx, tab, text, |t, s| editing::tab(t, s, indent));
    }
    for _ in 0..newlines {
        apply_edit(ctx, tab, text, editing::newline);
    }
}

/// The font the editor's text is set in: the fixed-width editor font, or
/// for prose the prose family (egui's own proportional font if no prose
/// family was installed).
fn editor_font(ui: &egui::Ui, settings: &EditorSettings) -> FontId {
    if !settings.prose {
        return FontId::monospace(settings.font_size);
    }
    let prose = crate::fonts::prose_family();
    if ui.fonts(|f| f.families().contains(&prose)) {
        FontId::new(settings.font_size, prose)
    } else {
        FontId::proportional(settings.font_size)
    }
}

/// Draw the editor. `reveal` scrolls the given character index into view.
pub fn show(
    ui: &mut egui::Ui,
    tab: TabId,
    text: &mut String,
    settings: &EditorSettings,
    highlights: Highlights<'_>,
    reveal: Option<usize>,
    mut syntax: Option<SyntaxInput<'_>>,
) -> CursorInfo {
    let font = editor_font(ui, settings);
    let line_height = (settings.line_spacing != 1.0)
        .then(|| ui.fonts_mut(|f| f.row_height(&font)) * settings.line_spacing);
    let line_count = text.split('\n').count();
    let digits = line_count.to_string().len().max(2);

    let gutter_width = if settings.show_line_numbers {
        let char_w = ui.fonts_mut(|f| f.glyph_width(&font, '0'));
        char_w * digits as f32 + 16.0
    } else {
        0.0
    };

    let scroll = if settings.word_wrap {
        ScrollArea::vertical()
    } else {
        ScrollArea::both()
    };

    let mut info = CursorInfo::default();

    scroll
        .id_salt(("editor-scroll", tab))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                // A centred column, inside the scroll area so the scroll bar
                // stays at the edge and the margins still scroll.
                let column = settings.max_text_width.filter(|_| settings.word_wrap);
                if let Some(max) = column {
                    ui.add_space(((ui.available_width() - max) / 2.0).max(0.0));
                }

                // Reserve the gutter column; numbers are painted after layout,
                // once we know where each text row landed.
                let gutter_rect = if settings.show_line_numbers {
                    let (rect, _) =
                        ui.allocate_exact_size(Vec2::new(gutter_width, 0.0), Sense::hover());
                    Some(rect)
                } else {
                    None
                };
                ui.add_space(6.0); // breathing room between gutter and text

                if settings.smart_dashes {
                    smart_dashes(ui, tab, text);
                }
                handle_editing_keys(ui, tab, text, settings);
                // Placeholder for highlights drawn behind the text; filled in
                // once we know where the cursor's row landed.
                let behind_text = ui.painter().add(Shape::Noop);

                let edit = TextEdit::multiline(text)
                    .id(editor_id(tab))
                    .font(font.clone())
                    .frame(egui::Frame::NONE)
                    .lock_focus(true) // keep Tab inside the editor
                    .desired_rows(1)
                    .desired_width(if settings.word_wrap {
                        column.map_or(ui.available_width(), |max| ui.available_width().min(max))
                    } else {
                        f32::INFINITY
                    });

                // Custom layouter: controls wrapping (infinite width = no wrap)
                // and paints search-match backgrounds.
                let wrap = settings.word_wrap;
                let layout_font = font.clone();
                let edit = if settings.prose { edit } else { edit.code_editor() };
                let mut layouter =
                    move |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap_width: f32| {
                        let text = buf.as_str();
                        let spans = match &mut syntax {
                            Some(s) => s.cache.highlight(text, s.syntax, s.colors),
                            None => Vec::new(),
                        };
                        let mut job = build_layout(
                            text,
                            &layout_font,
                            line_height,
                            ui.visuals().text_color(),
                            &spans,
                            &highlights,
                        );
                        job.wrap.max_width = if wrap { wrap_width } else { f32::INFINITY };
                        ui.fonts_mut(|f| f.layout_job(job))
                    };
                let edit = edit.layouter(&mut layouter);

                // Fill the rest of the viewport so clicking below the text focuses it.
                let min_height = ui.available_height();
                let output = edit.min_size(Vec2::new(0.0, min_height)).show(ui);
                // Name it for VoiceOver (there's no visible label).
                let _ = egui::Response::clone(&output.response).accessible_name("Document text");

                if let Some(cursor) = output.cursor_range {
                    info.line_col = Some(line_col(text, cursor.primary));
                    let shapes = cursor_highlights(ui, &output, text, cursor, settings);
                    ui.painter().set(behind_text, Shape::Vec(shapes));
                }

                if let Some(index) = reveal {
                    let rect = output
                        .galley
                        .pos_from_cursor(CCursor::new(index))
                        .translate(output.galley_pos.to_vec2());
                    ui.scroll_to_rect(rect.expand(8.0), Some(Align::Center));
                }

                if let Some(gutter) = gutter_rect {
                    paint_line_numbers(ui, gutter, &output, &font);
                }
            });
        });

    info
}

/// Smart dashes: before the text widget sees this frame's typing, turn each
/// `-` typed right after another `-` into Backspace + `—`. Rewriting the
/// keystrokes (rather than fixing the text afterwards) works however many
/// arrive in one frame, and undo sees what it would if typed that way.
fn smart_dashes(ui: &egui::Ui, tab: TabId, text: &str) {
    if !ui.memory(|m| m.has_focus(editor_id(tab))) {
        return;
    }
    // What's before the cursor (or the selection typing will replace).
    let start = selection(ui.ctx(), tab).map_or(0, |s| s.start);
    let before = &text[..crate::find::char_to_byte(text, start)];
    ui.ctx().input_mut(|i| {
        let events = std::mem::take(&mut i.events);
        i.events = dash_events(before, events);
    });
}

/// The keystroke rewriting for [`smart_dashes`], given the text before the
/// cursor. Separate so it can be tested without a window.
fn dash_events(before: &str, events: Vec<egui::Event>) -> Vec<egui::Event> {
    use egui::Event;
    // The last few characters before the cursor, and whether that's inside
    // an HTML comment, kept up to date as typing goes on.
    let mut tail: String = before.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect();
    let mut in_comment = editing::in_html_comment(before);
    let mut out = Vec::with_capacity(events.len());
    for event in events {
        match event {
            Event::Text(typed) => {
                let mut chunk = String::new();
                for c in typed.chars() {
                    if c == '-' && editing::makes_em_dash(&tail, in_comment) {
                        if !chunk.is_empty() {
                            out.push(Event::Text(std::mem::take(&mut chunk)));
                        }
                        out.push(Event::Key {
                            key: Key::Backspace,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: Modifiers::NONE,
                        });
                        out.push(Event::Text("\u{2014}".to_owned()));
                        tail.pop();
                        tail.push('\u{2014}');
                    } else {
                        chunk.push(c);
                        tail.push(c);
                    }
                    if tail.chars().count() > 4 {
                        tail.remove(0);
                    }
                    if tail.ends_with("<!--") {
                        in_comment = true;
                    } else if tail.ends_with("-->") {
                        in_comment = false;
                    }
                }
                if !chunk.is_empty() {
                    out.push(Event::Text(chunk));
                }
            }
            // Anything that may move the cursor or change the text other than
            // by typing: stop guessing what's before the cursor this frame.
            Event::Key { key, pressed: true, modifiers, .. }
                if modifiers.command
                    || modifiers.ctrl
                    || matches!(
                        key,
                        Key::ArrowLeft
                            | Key::ArrowRight
                            | Key::ArrowUp
                            | Key::ArrowDown
                            | Key::Home
                            | Key::End
                            | Key::PageUp
                            | Key::PageDown
                            | Key::Backspace
                            | Key::Delete
                            | Key::Enter
                            | Key::Tab
                    ) =>
            {
                tail.clear();
                // Unknown spot: no more conversions until the next frame.
                in_comment = true;
                out.push(event);
            }
            Event::Paste(_) | Event::Cut | Event::PointerButton { pressed: true, .. } => {
                tail.clear();
                // Unknown spot: no more conversions until the next frame.
                in_comment = true;
                out.push(event);
            }
            other => out.push(other),
        }
    }
    out
}

/// The current-line band and matching-bracket boxes, drawn behind the text.
fn cursor_highlights(
    ui: &egui::Ui,
    output: &egui::text_edit::TextEditOutput,
    text: &str,
    cursor: CCursorRange,
    settings: &EditorSettings,
) -> Vec<Shape> {
    let mut shapes = Vec::new();
    if cursor.primary != cursor.secondary {
        return shapes; // a selection is showing; keep it uncluttered
    }
    let offset = output.galley_pos.to_vec2();
    let caret = output.galley.pos_from_cursor(cursor.primary).translate(offset);

    if settings.highlight_current_line {
        let band = Rect::from_x_y_ranges(
            output.response.rect.left()..=ui.clip_rect().right(),
            caret.y_range(),
        );
        shapes.push(Shape::rect_filled(band, 0.0, ui.visuals().text_color().gamma_multiply(0.06)));
    }

    if let Some((a, b)) = editing::matching_bracket(text, cursor.primary.index.0) {
        let accent = ui.visuals().text_cursor.stroke.color;
        for i in [a, b] {
            let left = output.galley.pos_from_cursor(CCursor::new(i));
            let right = output.galley.pos_from_cursor(CCursor::new(i + 1));
            let width = if right.min.y == left.min.y { right.min.x - left.min.x } else { 8.0 };
            let rect = Rect::from_min_size(left.min, Vec2::new(width.max(2.0), left.height()))
                .translate(offset);
            shapes.push(Shape::rect_filled(rect, 2.0, accent.gamma_multiply(0.25)));
            shapes.push(Shape::rect_stroke(
                rect,
                2.0,
                Stroke::new(1.0, accent.gamma_multiply(0.8)),
                egui::StrokeKind::Inside,
            ));
        }
    }
    shapes
}

/// Build the text layout: syntax colours for the text, and highlighted
/// backgrounds for search matches.
fn build_layout(
    text: &str,
    font: &FontId,
    line_height: Option<f32>,
    color: Color32,
    spans: &[Span],
    highlights: &Highlights<'_>,
) -> LayoutJob {
    let plain = TextFormat { line_height, ..TextFormat::simple(font.clone(), color) };
    if spans.is_empty() && highlights.all.is_empty() {
        return LayoutJob::single_section(text.to_owned(), plain);
    }

    let match_bg = Color32::from_rgba_unmultiplied(255, 200, 0, 60);
    let current_bg = Color32::from_rgba_unmultiplied(255, 160, 0, 170);
    // Search matches were computed before this frame's edits; drop any that
    // no longer line up with the text.
    let valid = |r: &Range<usize>| {
        r.start < r.end && r.end <= text.len() && text.is_char_boundary(r.start) && text.is_char_boundary(r.end)
    };
    let backgrounds: Vec<(Range<usize>, Color32)> = highlights
        .all
        .iter()
        .filter(|r| valid(r))
        .map(|r| {
            let bg = if highlights.current.as_ref() == Some(r) { current_bg } else { match_bg };
            (r.clone(), bg)
        })
        .collect();

    // Cut the text wherever a colour or background starts or ends.
    let mut cuts: Vec<usize> = Vec::with_capacity(2 * (spans.len() + backgrounds.len()) + 2);
    cuts.push(0);
    cuts.push(text.len());
    for s in spans {
        cuts.extend([s.range.start, s.range.end]);
    }
    for (r, _) in &backgrounds {
        cuts.extend([r.start, r.end]);
    }
    cuts.retain(|&c| c <= text.len() && text.is_char_boundary(c));
    cuts.sort_unstable();
    cuts.dedup();

    let mut job = LayoutJob::default();
    let (mut si, mut bi) = (0, 0);
    for pair in cuts.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        while si < spans.len() && spans[si].range.end <= a {
            si += 1;
        }
        while bi < backgrounds.len() && backgrounds[bi].0.end <= a {
            bi += 1;
        }
        let mut format = plain.clone();
        if let Some(span) = spans.get(si).filter(|s| s.range.start <= a) {
            format.color = span.color;
            format.italics = span.italic;
        }
        if let Some((_, bg)) = backgrounds.get(bi).filter(|(r, _)| r.start <= a) {
            format.background = *bg;
        }
        job.append(&text[a..b], 0.0, format);
    }
    job
}

fn paint_line_numbers(
    ui: &egui::Ui,
    gutter: Rect,
    output: &egui::text_edit::TextEditOutput,
    font: &FontId,
) {
    let painter = ui.painter();
    let color = ui.visuals().weak_text_color();
    let bg = ui.visuals().faint_bg_color;
    let full = Rect::from_min_max(
        gutter.min,
        egui::pos2(gutter.max.x, output.response.rect.max.y),
    );
    painter.rect_filled(full, 0.0, bg);
    painter.vline(
        full.max.x - 4.0,
        full.y_range(),
        egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
    );

    // Only paint rows that are visible.
    let clip = ui.clip_rect();
    let mut line_no = 1usize;
    let mut at_line_start = true;
    for row in &output.galley.rows {
        let y = output.galley_pos.y + row.pos.y;
        if at_line_start && y + row.height() >= clip.min.y && y <= clip.max.y {
            painter.text(
                egui::pos2(full.max.x - 10.0, y),
                Align2::RIGHT_TOP,
                line_no.to_string(),
                font.clone(),
                color,
            );
        }
        // A wrapped line spans several rows; only number the first.
        at_line_start = row.ends_with_newline;
        if row.ends_with_newline {
            line_no += 1;
        }
    }
}

/// Parse "12" or "12:5" (1-based line and optional column).
pub fn parse_line_col(input: &str) -> Option<(usize, usize)> {
    let mut parts = input.trim().splitn(2, ':');
    let line = parts.next()?.trim().parse().ok().filter(|&l| l > 0)?;
    let col = match parts.next() {
        Some(c) => c.trim().parse().ok().filter(|&c| c > 0)?,
        None => 1,
    };
    Some((line, col))
}

/// Character index of a 1-based line and column, clamped to the text.
pub fn char_index_of(text: &str, line: usize, col: usize) -> usize {
    let mut index = 0;
    for (i, l) in text.split('\n').enumerate() {
        let len = l.chars().count();
        if i + 1 == line {
            return index + (col - 1).min(len);
        }
        index += len + 1; // + the newline
    }
    // Past the last line: go to the end.
    text.chars().count()
}

/// Convert a character cursor into a 1-based (line, column).
fn line_col(text: &str, cursor: CCursor) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    for c in text.chars().take(cursor.index.0) {
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_combines_code_colours_and_search_highlights() {
        let font = FontId::monospace(14.0);
        let text = "let x = 10;";
        let red = Color32::RED;
        let spans = [Span { range: 0..3, color: red, italic: false }]; // "let"
        let found: Vec<Range<usize>> = std::iter::once(2..5).collect(); // "t x": overlaps the end of "let"
        let highlights = Highlights { all: &found, current: None };
        let job = build_layout(text, &font, None, Color32::WHITE, &spans, &highlights);

        let pieces: Vec<(&str, Color32, bool)> = job
            .sections
            .iter()
            .map(|s| {
                let range = s.byte_range.start.0..s.byte_range.end.0;
                (&text[range], s.format.color, s.format.background != Color32::TRANSPARENT)
            })
            .collect();
        assert_eq!(
            pieces,
            vec![
                ("le", red, false),
                ("t", red, true),
                (" x", Color32::WHITE, true),
                (" = 10;", Color32::WHITE, false),
            ]
        );
    }

    #[test]
    fn parses_line_and_column() {
        assert_eq!(parse_line_col("12"), Some((12, 1)));
        assert_eq!(parse_line_col(" 3:7 "), Some((3, 7)));
        assert_eq!(parse_line_col("0"), None);
        assert_eq!(parse_line_col("x"), None);
        assert_eq!(parse_line_col("4:"), None);
    }

    #[test]
    fn finds_char_index_of_line_and_column() {
        let text = "ab\ncdé\nf";
        assert_eq!(char_index_of(text, 1, 1), 0);
        assert_eq!(char_index_of(text, 2, 1), 3);
        assert_eq!(char_index_of(text, 2, 3), 5);
        assert_eq!(char_index_of(text, 2, 99), 6); // clamped to end of line
        assert_eq!(char_index_of(text, 3, 1), 7);
        assert_eq!(char_index_of(text, 99, 1), 8); // past the end
    }

    /// Typing as egui reports it: a key press, then its text.
    fn typed(text: &str) -> Vec<egui::Event> {
        text.chars().map(|c| egui::Event::Text(c.to_string())).collect()
    }

    /// Play events against `before` the way the text widget would, for
    /// the subset dash_events produces: text, and Backspace.
    fn play(before: &str, events: &[egui::Event]) -> String {
        let mut text = before.to_owned();
        for e in events {
            match e {
                egui::Event::Text(t) => text.push_str(t),
                egui::Event::Key { key: Key::Backspace, pressed: true, .. } => {
                    text.pop();
                }
                _ => {}
            }
        }
        text
    }

    #[test]
    fn smart_dashes_rewrite_typing() {
        let cases = [
            ("", "Wait--what", "Wait\u{2014}what"),
            // The first dash was typed in an earlier frame.
            ("Wait-", "-what", "Wait\u{2014}what"),
            // A third dash stays a dash.
            ("", "a---b", "a\u{2014}-b"),
            // HTML comments (inline notes) are left exactly as typed...
            ("", "<!-- a -- note -->", "<!-- a -- note -->"),
            ("<!-- started in an earlier frame ", "-->", "<!-- started in an earlier frame -->"),
            // ...and dashes work again after one closes.
            ("", "<!-- n --> a--b", "<!-- n --> a\u{2014}b"),
            ("", "one-two", "one-two"),
        ];
        for (before, typing, expected) in cases {
            let events = dash_events(before, typed(typing));
            assert_eq!(play(before, &events), expected, "{before:?} + {typing:?}");
        }
    }

    #[test]
    fn smart_dashes_stop_guessing_when_the_cursor_moves() {
        let mut events = typed("-");
        events.push(egui::Event::Key {
            key: Key::ArrowLeft,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        });
        events.extend(typed("-"));
        let out = dash_events("", events);
        assert!(!out.iter().any(|e| matches!(e, egui::Event::Key { key: Key::Backspace, .. })));
    }
}
