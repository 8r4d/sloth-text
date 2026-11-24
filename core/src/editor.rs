//! The main editing surface: a monospace text area with a line-number gutter.

use std::ops::Range;

use eframe::egui::{
    self, Align, Align2, Color32, FontId, Id, Key, Modifiers, Rect, ScrollArea, Sense, Shape,
    Stroke, TextEdit, Vec2,
    text::{CCursor, CCursorRange, LayoutJob, TextFormat},
    text_edit::TextEditState,
};

use crate::editing::{self, Edit, Indent};
use crate::markdown;
use crate::spell;
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
    /// Show Markdown as you type: italics, bold, headings, quotes and notes
    /// styled, their markers faint (Sloth Writer; off in Sloth).
    pub markdown: bool,
    /// Underline misspelled words, with suggestions on right-click (Sloth
    /// Writer; off in Sloth). See `spell`.
    pub spell_check: bool,
    /// Offer Split at Cursor in the right-click menu (the app does the
    /// splitting; see `CursorInfo::split_at`).
    pub split_menu: bool,
    /// Typewriter scrolling: keep the line being typed in the middle of
    /// the editor (with room below the text to let the last line get there).
    pub typewriter: bool,
    /// Extra space after each paragraph (each line of the text), as a
    /// fraction of a line: 0 for none. Blank lines get none.
    pub paragraph_spacing: f32,
    /// First-line indent for each paragraph (each line of the text), as a
    /// multiple of the font size: 0 for none. Only how it looks; the text
    /// doesn't change.
    pub first_line_indent: f32,
    /// With `markdown`, hide the markers (`**`, `*`, `#`, `>`) except in
    /// the paragraph being edited (Sloth Writer; off in Sloth).
    pub hide_markers: bool,
    /// With `markdown`, copy and cut put formatted text on the clipboard
    /// (bold, italics, headings) rather than Markdown; see
    /// `markdown::formatted` (Sloth Writer; off in Sloth).
    pub copy_formatted: bool,
    /// Fade every paragraph but the one being written in (Sloth Writer's
    /// Focus mode). Moving to another paragraph eases the light across.
    pub focus_paragraph: bool,
    /// Offer Add Comment in the right-click menu (Sloth Writer; with
    /// `markdown`, comments' marks are kept out of sight away from the cursor).
    pub comments: bool,
    /// One of several editors on a page (Sloth Writer's Slothenings): no
    /// scroll area of its own, and only as tall as its text.
    pub embedded: bool,
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
            markdown: false,
            spell_check: false,
            split_menu: false,
            typewriter: false,
            paragraph_spacing: 0.0,
            first_line_indent: 0.0,
            hide_markers: false,
            copy_formatted: false,
            focus_paragraph: false,
            comments: false,
            embedded: false,
        }
    }
}

/// Search matches to highlight (byte ranges into the text).
#[derive(Default)]
pub struct Highlights<'a> {
    pub all: &'a [Range<usize>],
    pub current: Option<Range<usize>>,
    /// Other marks over the text (Sloth Writer's repeated words), under
    /// search matches.
    pub marks: &'a [Mark],
    /// Places in the text (bytes) to report where they are on screen, in
    /// `CursorInfo::anchor_tops` (Sloth Writer's comments, beside their
    /// passages).
    pub anchors: &'a [usize],
}

/// A mark over part of the text: a tint behind it, a line under it, or both
/// (`Color32::TRANSPARENT` for neither).
#[derive(Clone, Debug, PartialEq)]
pub struct Mark {
    pub range: Range<usize>,
    pub background: Color32,
    pub underline: Color32,
}

/// What's needed to colour code in the editor.
pub struct SyntaxInput<'a> {
    pub syntax: &'static syntect::parsing::SyntaxReference,
    pub colors: SyntaxColors,
    pub cache: &'a mut HighlightCache,
}

/// Information the editor reports back to the app each frame.
#[derive(Default, Clone)]
pub struct CursorInfo {
    /// 1-based line and column of the cursor, if the editor has a cursor.
    pub line_col: Option<(usize, usize)>,
    /// Split at Cursor was chosen in the right-click menu, at this
    /// character index (see `EditorSettings::split_menu`).
    pub split_at: Option<usize>,
    /// The `[[reference]]` under the pointer (byte range in the text), for
    /// the app to explain in a tooltip (Markdown only).
    pub reference_hovered: Option<(usize, usize)>,
    /// A `[[reference]]` was ⌘-clicked: the app opens what it refers to.
    pub reference_clicked: Option<(usize, usize)>,
    /// Where the cursor is on screen, if the editor has one (for a popup
    /// beside it, such as the app's suggestions).
    pub cursor_rect: Option<Rect>,
    /// The line at the top of the view (0-based), with the fraction of it
    /// scrolled past: for a preview to scroll along.
    pub top_line: f32,
    /// Where each of `Highlights::anchors` is on screen: the top of its row.
    pub anchor_tops: Vec<f32>,
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
    if edit.text != *text {
        mark_undo_point(ctx, tab, text);
    }
    *text = edit.text;
    set_selection(ctx, tab, edit.selection);
}

/// Add a comment: on the selected words (`{==them==}{>><<}`), or at the
/// cursor (`{>><<}`). The cursor goes into its (empty) words, and where
/// they are is kept for the app to find with `take_added_comment`.
pub fn add_comment(ctx: &egui::Context, tab: TabId, text: &mut String) {
    let chars = selection(ctx, tab).unwrap_or(0..0);
    let (start, end) = (crate::find::char_to_byte(text, chars.start), crate::find::char_to_byte(text, chars.end));
    let insert = if start < end { format!("{{=={}==}}{{>><<}}", &text[start..end]) } else { "{>><<}".to_owned() };
    // The cursor between `{>>` and `<<}`.
    let words = start + insert.len() - 3;
    apply_edit(ctx, tab, text, |t, _| {
        let mut edit = editing::replace(t, chars.clone(), &insert);
        let at = t[..start].chars().count() + insert[..insert.len() - 3].chars().count();
        edit.selection = at..at;
        edit
    });
    ctx.data_mut(|d| d.insert_temp(editor_id(tab).with("comment-added"), words));
}

/// Where the comment just added (if any) has its words, in bytes; asked
/// once.
pub fn take_added_comment(ctx: &egui::Context, tab: TabId) -> Option<usize> {
    let id = editor_id(tab).with("comment-added");
    ctx.data_mut(|d| d.remove_temp::<usize>(id))
}

/// Forget a tab's undo history (after a change undo shouldn't reach back
/// past, such as splitting a document in two).
pub fn forget_undo(ctx: &egui::Context, tab: TabId) {
    let id = editor_id(tab);
    if let Some(mut state) = TextEditState::load(ctx, id) {
        state.set_undoer(Default::default());
        state.store(ctx, id);
    }
}

/// Make the text as it is now an undo point, so ⌘Z takes back an edit made
/// here (Bold, Change Case, a spelling fix…) on its own. Otherwise the text
/// widget folds it into whatever was typed in the second before, and undo
/// takes back both.
fn mark_undo_point(ctx: &egui::Context, tab: TabId, text: &str) {
    let id = editor_id(tab);
    let Some(mut state) = TextEditState::load(ctx, id) else { return };
    let cursor = state.cursor.char_range().unwrap_or_default();
    let mut undoer = state.undoer();
    undoer.add_undo(&(cursor, text.to_owned()));
    state.set_undoer(undoer);
    state.store(ctx, id);
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
    if settings.markdown {
        let (bold, italic, blank_lines) = ui.input_mut(|i| {
            (
                i.consume_key(Modifiers::COMMAND, Key::B),
                i.consume_key(Modifiers::COMMAND, Key::I),
                i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::L),
            )
        });
        if bold {
            apply_format(ctx, tab, text, Format::Bold);
        }
        if italic {
            apply_format(ctx, tab, text, Format::Italic);
        }
        if blank_lines {
            apply_edit(ctx, tab, text, editing::remove_blank_lines);
        }
    }
}

/// The Markdown formatting the editor's right-click menu offers.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Format {
    Bold,
    Italic,
    Heading,
    Quote,
    Note,
    SceneBreak,
}

impl Format {
    const ALL: [Format; 6] = [Format::Bold, Format::Italic, Format::Heading, Format::Quote, Format::Note, Format::SceneBreak];

    fn label(self) -> &'static str {
        match self {
            Format::Bold => "Bold",
            Format::Italic => "Italic",
            Format::Heading => "Heading",
            Format::Quote => "Quote",
            Format::Note => "Note (not compiled)",
            Format::SceneBreak => "Scene Break",
        }
    }

    fn shortcut(self) -> Option<&'static str> {
        match self {
            Format::Bold => Some("\u{2318}B"),
            Format::Italic => Some("\u{2318}I"),
            _ => None,
        }
    }
}

fn apply_format(ctx: &egui::Context, tab: TabId, text: &mut String, format: Format) {
    match format {
        Format::Bold => apply_edit(ctx, tab, text, |t, s| editing::toggle_wrap(t, s, "**", "**")),
        Format::Italic => apply_edit(ctx, tab, text, |t, s| editing::toggle_wrap(t, s, "*", "*")),
        Format::Heading => apply_edit(ctx, tab, text, |t, s| editing::toggle_line_prefix(t, s, "# ")),
        Format::Quote => apply_edit(ctx, tab, text, |t, s| editing::toggle_line_prefix(t, s, "> ")),
        Format::Note => apply_edit(ctx, tab, text, |t, s| editing::toggle_wrap(t, s, "<!-- ", " -->")),
        Format::SceneBreak => apply_edit(ctx, tab, text, editing::scene_break),
    }
}

/// Misspelled words as byte ranges, leaving out the one the cursor (a
/// character index) is in or at the end of: it's still being typed.
fn misspellings(text: &str, cursor: Option<usize>) -> Vec<Range<usize>> {
    let mut found = spell::misspelled(text);
    if let Some(cursor) = cursor {
        let at = text.char_indices().nth(cursor).map_or(text.len(), |(b, _)| b);
        found.retain(|r| !(r.start <= at && at <= r.end));
    }
    found
}

/// Paragraph spacing: move each paragraph's rows down by `gap` for every
/// paragraph above it (blank lines don't count). The text widget clicks,
/// selects and draws from these same row positions, so all stays in step.
fn space_paragraphs(galley: std::sync::Arc<egui::Galley>, gap: f32) -> std::sync::Arc<egui::Galley> {
    if gap <= 0.0 || galley.rows.len() < 2 {
        return galley;
    }
    let mut spaced = (*galley).clone();
    let mut offset = 0.0;
    for row in &mut spaced.rows {
        row.pos.y += offset;
        if row.ends_with_newline && !row.row.glyphs.is_empty() {
            offset += gap;
        }
    }
    spaced.rect.max.y += offset;
    spaced.mesh_bounds.max.y += offset;
    std::sync::Arc::new(spaced)
}

/// First-line indent: start each paragraph (each line) `indent` points in.
/// egui only honours a section's leading space where the section starts a
/// paragraph, so sections are split at every line start. Wrapping allows
/// for the indent, and clicks and the cursor follow it.
fn indent_paragraphs(mut job: LayoutJob, indent: f32) -> LayoutJob {
    if indent <= 0.0 {
        return job;
    }
    let starts: Vec<usize> =
        std::iter::once(0).chain(job.text.match_indices('\n').map(|(i, _)| i + 1)).collect();
    let mut sections = Vec::with_capacity(job.sections.len() + starts.len());
    let bytes = |r: Range<usize>| egui::text::ByteIndex(r.start)..egui::text::ByteIndex(r.end);
    for section in job.sections.drain(..) {
        let range = section.byte_range.start.0..section.byte_range.end.0;
        let mut from = range.start;
        for &at in starts.iter().filter(|&&at| range.start <= at && at < range.end) {
            if at > from {
                sections.push(egui::text::LayoutSection { byte_range: bytes(from..at), leading_space: 0.0, ..section.clone() });
            }
            from = at;
        }
        let leading_space = if starts.binary_search(&from).is_ok() { indent } else { section.leading_space };
        sections.push(egui::text::LayoutSection { byte_range: bytes(from..range.end), leading_space, ..section });
    }
    job.sections = sections;
    job
}

/// The lines under marks that have one, just below the text's baseline
/// (egui's own underline sits at the bottom of the row, which with wider
/// line spacing is in the gap, and off the end on the last line).
fn paint_mark_lines(ui: &egui::Ui, output: &egui::text_edit::TextEditOutput, text: &str, marks: &[Mark]) {
    let painter = ui.painter();
    let to_char = |b: usize| text.get(..b).map_or(0, |s| s.chars().count());
    for mark in marks.iter().filter(|m| m.underline != Color32::TRANSPARENT && m.range.end <= text.len()) {
        let at = |i| output.galley.pos_from_cursor(CCursor::new(i)).translate(output.galley_pos.to_vec2());
        let (a, b) = (at(to_char(mark.range.start)), at(to_char(mark.range.end)));
        // A phrase across rows is marked to the end of its first.
        let right = if (a.top() - b.top()).abs() < 1.0 { b.left() } else { output.galley_pos.x + output.galley.rect.right() };
        // Just under the baseline of the row the mark starts on.
        let Some(baseline) = output
            .galley
            .rows
            .iter()
            .find(|r| {
                let top = output.galley_pos.y + r.pos.y;
                a.center().y >= top && a.center().y < top + r.height()
            })
            .and_then(|r| r.glyphs.first().map(|g| output.galley_pos.y + r.pos.y + g.pos.y))
        else {
            continue;
        };
        let y = baseline + 2.5;
        painter.hline(a.left()..=right, y, Stroke::new(1.5, mark.underline));
    }
}

/// A red wavy line under each misspelled word (byte ranges).
fn paint_misspellings(ui: &egui::Ui, output: &egui::text_edit::TextEditOutput, text: &str, words: &[Range<usize>]) {
    if words.is_empty() {
        return;
    }
    let stroke = Stroke::new(1.2, Color32::from_rgb(230, 70, 70));
    let painter = ui.painter();
    let mut chars = 0;
    let mut bytes = 0;
    let mut to_char = |b: usize| {
        // Words come in order, so walk forward from the last one.
        chars += text[bytes..b].chars().count();
        bytes = b;
        chars
    };
    for word in words {
        let (start, end) = (to_char(word.start), to_char(word.end));
        let at = |i| output.galley.pos_from_cursor(CCursor::new(i)).translate(output.galley_pos.to_vec2());
        let (a, b) = (at(start), at(end));
        // A word split across rows (very long ones) is marked on its first.
        let right = if (a.top() - b.top()).abs() < 1.0 { b.left() } else { output.galley_pos.x + output.galley.rect.right() };
        let y = a.bottom() - 2.0;
        let mut points = Vec::new();
        let mut x = a.left();
        let mut up = true;
        while x <= right {
            points.push(egui::pos2(x, if up { y - 1.2 } else { y + 1.2 }));
            up = !up;
            x += 2.0;
        }
        if points.len() >= 2 {
            painter.add(Shape::line(points, stroke));
        }
    }
}

/// The misspelled word a right-click landed on, with its suggestions.
#[derive(Clone)]
struct Misspelling {
    /// Bytes in the text.
    range: Range<usize>,
    word: String,
    guesses: Vec<String>,
}

/// The misspelled word at a point in the editor, if any.
fn misspelling_at(output: &egui::text_edit::TextEditOutput, text: &str, pos: egui::Pos2) -> Option<Misspelling> {
    let index = output.galley.cursor_from_pos(pos - output.galley_pos).index.0;
    let at = text.char_indices().nth(index).map_or(text.len(), |(b, _)| b);
    let range = spell::misspelled(text).into_iter().find(|r| r.start <= at && at < r.end)?;
    let word = text[range.clone()].to_owned();
    let guesses = spell::suggestions(&word).into_iter().take(6).collect();
    Some(Misspelling { range, word, guesses })
}

/// What the right-click menu can do to the text.
enum MenuChoice {
    Format(Format),
    Correct(Misspelling, String),
    Learn(String),
    Ignore(String),
    Case(editing::Case),
    RemoveBlankLines,
    Split,
    Comment,
}

/// Right-click in the editor: spelling suggestions for a misspelled word
/// under the pointer, then Markdown formatting for the selection (or the
/// cursor's line).
fn text_menu(output: &egui::text_edit::TextEditOutput, tab: TabId, text: &mut String, settings: &EditorSettings) -> Option<usize> {
    let response = &output.response;
    // Clicking the menu takes focus from the text, which drops its
    // selection, so remember the selection the menu was opened on, and
    // the misspelled word that was clicked.
    let opened_on = editor_id(tab).with("format-menu-selection");
    let clicked_word = editor_id(tab).with("menu-misspelling");
    if response.secondary_clicked() {
        let picked = selection(&response.ctx, tab).unwrap_or(0..0);
        let word = settings
            .spell_check
            .then(|| response.interact_pointer_pos())
            .flatten()
            .and_then(|pos| misspelling_at(output, text, pos));
        response.ctx.data_mut(|d| {
            d.insert_temp(opened_on, picked);
            d.insert_temp(clicked_word, word);
        });
    }
    let mut split_at = None;
    response.context_menu(|ui| {
        let mut picked = None;
        let word: Option<Misspelling> = ui.ctx().data(|d| d.get_temp(clicked_word)).flatten();
        if let Some(word) = &word {
            if word.guesses.is_empty() {
                ui.add_enabled(false, egui::Button::new("No Guesses Found"));
            }
            for guess in &word.guesses {
                if ui.button(egui::RichText::new(guess).strong()).clicked() {
                    picked = Some(MenuChoice::Correct(word.clone(), guess.clone()));
                }
            }
            ui.separator();
            if ui.button("Learn Spelling").on_hover_text("Add it to your Mac's dictionary").clicked() {
                picked = Some(MenuChoice::Learn(word.word.clone()));
            }
            if ui.button("Ignore Spelling").on_hover_text("Until Sloth Writer quits").clicked() {
                picked = Some(MenuChoice::Ignore(word.word.clone()));
            }
            if settings.markdown {
                ui.separator();
            }
        }
        let formats: &[Format] = if settings.markdown { &Format::ALL } else { &[] };
        for &format in formats {
            if matches!(format, Format::Heading | Format::SceneBreak) {
                ui.separator();
            }
            let mut button = egui::Button::new(format.label());
            if let Some(keys) = format.shortcut() {
                button = button.shortcut_text(keys);
            }
            if ui.add(button).clicked() {
                picked = Some(MenuChoice::Format(format));
            }
        }
        if settings.markdown {
            ui.separator();
            ui.menu_button("Change Case", |ui| {
                for case in editing::Case::ALL {
                    if ui.button(case.label()).clicked() {
                        picked = Some(MenuChoice::Case(case));
                    }
                }
            });
            let removes = egui::Button::new("Remove Blank Lines").shortcut_text("\u{21e7}\u{2318}L");
            if ui.add(removes).on_hover_text("Take out the empty lines in the selection").clicked() {
                picked = Some(MenuChoice::RemoveBlankLines);
            }
        }
        if settings.comments && settings.markdown {
            ui.separator();
            let add = egui::Button::new("Add Comment").shortcut_text("\u{2325}\u{2318}K");
            if ui.add(add).on_hover_text("A comment in the margin, on the selected words").clicked() {
                picked = Some(MenuChoice::Comment);
            }
        }
        if settings.split_menu {
            ui.separator();
            let split = egui::Button::new("Split at Cursor").shortcut_text("\u{2318}K");
            if ui.add(split).on_hover_text("Move the text after the cursor into a new document").clicked() {
                picked = Some(MenuChoice::Split);
            }
        }
        if let Some(choice) = picked {
            let ctx = ui.ctx();
            match choice {
                MenuChoice::Format(format) => {
                    if let Some(range) = ctx.data(|d| d.get_temp::<Range<usize>>(opened_on)) {
                        set_selection(ctx, tab, range);
                    }
                    apply_format(ctx, tab, text, format);
                }
                MenuChoice::Correct(word, guess) => {
                    // Only if the word's still there (nothing changed meanwhile).
                    if text.get(word.range.clone()) == Some(word.word.as_str()) {
                        let start = text[..word.range.start].chars().count();
                        let chars = start..start + word.word.chars().count();
                        apply_edit(ctx, tab, text, |t, _| editing::replace(t, chars, &guess));
                    }
                }
                MenuChoice::Learn(word) => spell::learn(&word),
                MenuChoice::Ignore(word) => spell::ignore(&word),
                MenuChoice::Case(case) => {
                    if let Some(range) = ctx.data(|d| d.get_temp::<Range<usize>>(opened_on)) {
                        set_selection(ctx, tab, range);
                    }
                    apply_edit(ctx, tab, text, |t, s| editing::change_case(t, s, case));
                }
                MenuChoice::RemoveBlankLines => {
                    if let Some(range) = ctx.data(|d| d.get_temp::<Range<usize>>(opened_on)) {
                        set_selection(ctx, tab, range);
                    }
                    apply_edit(ctx, tab, text, editing::remove_blank_lines);
                }
                MenuChoice::Comment => {
                    if let Some(range) = ctx.data(|d| d.get_temp::<Range<usize>>(opened_on)) {
                        set_selection(ctx, tab, range);
                    }
                    add_comment(ctx, tab, text);
                }
                MenuChoice::Split => {
                    let range = ctx.data(|d| d.get_temp::<Range<usize>>(opened_on)).unwrap_or(0..0);
                    split_at = Some(range.start);
                }
            }
            // Back to typing.
            ui.ctx().memory_mut(|m| m.request_focus(editor_id(tab)));
            ui.close();
        }
    });
    split_at
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
    let viewport_height = ui.available_height();

    let body = |ui: &mut egui::Ui| {
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
                let copy_formatted = settings.markdown && settings.copy_formatted;
                if copy_formatted && ui.memory(|m| m.has_focus(editor_id(tab))) {
                    paste_markdown_back(ui.ctx());
                }
                // Hidden markers show again in the paragraphs being edited.
                let hide_markers = (settings.markdown && settings.hide_markers).then(|| {
                    ui.memory(|m| m.has_focus(editor_id(tab)))
                        .then(|| selection(ui.ctx(), tab))
                        .flatten()
                        .map_or(usize::MAX..usize::MAX, |chars| lines_around(text, chars))
                });
                // Comments' marks and words show only in the paragraphs being
                // edited (wherever the cursor isn't, they're out of sight).
                let comments_shown = (settings.markdown && settings.comments).then(|| {
                    ui.memory(|m| m.has_focus(editor_id(tab)))
                        .then(|| selection(ui.ctx(), tab))
                        .flatten()
                        .map_or(usize::MAX..usize::MAX, |chars| lines_around(text, chars))
                });
                // For the cards beside them: where the anchors are.
                let anchors = highlights.anchors;
                // Paragraph focus: which lines are lit, and how far the
                // light has moved from where it was.
                let focus = settings.focus_paragraph.then(|| paragraph_focus(ui, tab, text)).flatten();
                // Placeholder for highlights drawn behind the text; filled in
                // once we know where the cursor's row landed.
                let behind_text = ui.painter().add(Shape::Noop);

                // For the lines under marks, drawn after the text.
                let mark_lines = highlights.marks;
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
                let paragraph_gap = settings.paragraph_spacing.max(0.0)
                    * line_height.unwrap_or_else(|| ui.fonts_mut(|f| f.row_height(&font)));
                let indent = settings.first_line_indent.max(0.0) * settings.font_size;
                let layout_font = font.clone();
                let faces = settings.markdown.then(|| Faces::find(ui, &font));

                let edit = if settings.prose { edit } else { edit.code_editor() };
                let mut layouter =
                    move |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap_width: f32| {
                        let text = buf.as_str();
                        let spans = match &mut syntax {
                            Some(s) => s.cache.highlight(text, s.syntax, s.colors),
                            None => Vec::new(),
                        };
                        let styles = if faces.is_some() { markdown::styles(text) } else { Vec::new() };

                        let mut job = build_layout(
                            text,
                            &layout_font,
                            line_height,
                            ui.visuals().text_color(),
                            &spans,
                            &highlights,
                            &styles,
                            faces.as_ref(),
                            hide_markers.as_ref(),
                            comments_shown.as_ref(),
                        );
                        job.wrap.max_width = if wrap { wrap_width } else { f32::INFINITY };
                        let job = indent_paragraphs(job, indent);
                        let job = match &focus {
                            Some(f) => fade_paragraphs(job, f),
                            None => job,
                        };
                        space_paragraphs(ui.fonts_mut(|f| f.layout_job(job)), paragraph_gap)
                    };
                let edit = edit.layouter(&mut layouter);

                // Fill the rest of the viewport so clicking below the text focuses it.
                // Fill the view (so a click below the text still lands in it),
                // except as one of several on a page.
                let min_height = if settings.embedded { 0.0 } else { ui.available_height() };
                // The text widget moves the cursor on any button's press;
                // a right-click is for the formatting menu, so keep the
                // selection it's meant to format.
                let menu = settings.markdown || settings.spell_check;
                let right_click = menu
                    && ui.input(|i| {
                        i.pointer.button_down(egui::PointerButton::Secondary)
                            || i.pointer.button_released(egui::PointerButton::Secondary)
                    });
                let kept = right_click.then(|| selection(ui.ctx(), tab)).flatten();
                let output = edit.min_size(Vec2::new(0.0, min_height)).show(ui);
                if copy_formatted && output.response.has_focus() {
                    copy_as_formatted(ui.ctx());
                }
                // Name it for VoiceOver (there's no visible label).
                let _ = egui::Response::clone(&output.response).accessible_name("Document text");
                if let Some(kept) = kept {
                    set_selection(ui.ctx(), tab, kept);
                }
                paint_mark_lines(ui, &output, text, mark_lines);
                if settings.spell_check {
                    // The word being typed isn't flagged until the cursor leaves it.
                    let cursor = output.cursor_range.map(|c| c.primary.index.0);
                    paint_misspellings(ui, &output, text, &misspellings(text, cursor));
                }
                if menu {
                    info.split_at = text_menu(&output, tab, text, settings);
                }
                if settings.markdown
                    && let Some(pos) = output.response.hover_pos()
                    && let Some(reference) = reference_at(&output, text, pos)
                {
                    info.reference_hovered = Some(reference);
                    if ui.input(|i| i.modifiers.command) {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        if output.response.clicked() {
                            info.reference_clicked = Some(reference);
                        }
                    }
                }

                if let Some(cursor) = output.cursor_range {
                    info.line_col = Some(line_col(text, cursor.primary));
                    info.cursor_rect = Some(output.galley.pos_from_cursor(cursor.primary).translate(output.galley_pos.to_vec2()));
                    let shapes = cursor_highlights(ui, &output, text, cursor, settings);
                    ui.painter().set(behind_text, Shape::Vec(shapes));
                }

                // Typewriter scrolling: typing or moving with the keys keeps
                // the cursor's line in the middle (not clicking about, which
                // would make the text jump under the pointer).
                if settings.typewriter
                    && output.response.has_focus()
                    && let Some(cursor) = output.cursor_range
                {
                    let typed = output.response.changed()
                        || ui.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Key { pressed: true, .. })));
                    if typed {
                        let rect = output.galley.pos_from_cursor(cursor.primary).translate(output.galley_pos.to_vec2());
                        ui.scroll_to_rect(rect, Some(Align::Center));
                    }
                }

                if let Some(index) = reveal {
                    let rect = output
                        .galley
                        .pos_from_cursor(CCursor::new(index))
                        .translate(output.galley_pos.to_vec2());
                    ui.scroll_to_rect(rect.expand(8.0), Some(Align::Center));
                }

                info.top_line = top_line(&output, ui.clip_rect().min.y);
                info.anchor_tops = anchors
                    .iter()
                    .map(|&b| {
                        let chars = text.get(..b.min(text.len())).map_or(0, |s| s.chars().count());
                        output.galley.pos_from_cursor(CCursor::new(chars)).top() + output.galley_pos.y
                    })
                    .collect();
                if let Some(gutter) = gutter_rect {
                    paint_line_numbers(ui, gutter, &output, &font);
                }
            });
            // Room for the last line to reach the middle.
            if settings.typewriter && !settings.embedded {
                ui.add_space(viewport_height * 0.5);
            }
        };
    if settings.embedded {
        // In a page of several (Slothenings): the page scrolls.
        body(ui);
    } else {
        scroll.id_salt(("editor-scroll", tab)).auto_shrink([false, false]).show(ui, body);
    }

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
#[allow(clippy::too_many_arguments)]
/// References' colour: a mid blue that reads on light and dark alike.
/// A commented passage's tint, and a comment's colour (amber, readable
/// on light and dark).
const COMMENT_TINT: Color32 = Color32::from_rgba_premultiplied(64, 46, 0, 48);
const COMMENT_COLOR: Color32 = Color32::from_rgb(0xc9, 0x8a, 0x22);
const REFERENCE_COLOR: Color32 = Color32::from_rgb(70, 130, 200);

/// The `[[reference]]` under a point in the editor, if the point is on it
/// (not just level with it, past the end of the line).
fn reference_at(output: &egui::text_edit::TextEditOutput, text: &str, pos: egui::Pos2) -> Option<(usize, usize)> {
    let local = pos - output.galley_pos;
    let index = output.galley.cursor_from_pos(local).index.0;
    let at = text.char_indices().nth(index).map_or(text.len(), |(b, _)| b);
    let (range, _) = markdown::references(text).into_iter().find(|(r, _)| r.start <= at && at <= r.end)?;
    let chars = |b: usize| text[..b].chars().count();
    let (a, b) = (output.galley.pos_from_cursor(CCursor::new(chars(range.start))), output.galley.pos_from_cursor(CCursor::new(chars(range.end))));
    let on_it = if (a.top() - b.top()).abs() < 1.0 {
        (a.left()..=b.left()).contains(&local.x) && (a.top()..=a.bottom()).contains(&local.y)
    } else {
        true // wrapped over two rows: near enough
    };
    on_it.then_some((range.start, range.end))
}

/// The font size hidden Markdown markers are set in.
const HIDDEN_SIZE: f32 = 0.01;

/// The byte range of the whole lines a character range touches.
/// How faint the paragraphs not being written in are.
const FADED: f32 = 0.28;
/// How long the light takes to move to another paragraph, in seconds.
const FOCUS_MOVE: f64 = 0.25;

/// Which lines (paragraphs) are lit for paragraph focus, by number: the
/// ones the cursor or selection is on, and the ones that were, as the
/// light moves from those to these (`moved`, 0 to 1).
#[derive(Clone, Debug, PartialEq)]
struct Lit {
    now: Range<usize>,
    before: Option<Range<usize>>,
    moved: f32,
}

/// What's remembered between frames: the lines lit, and since when.
#[derive(Clone, Default)]
struct FocusMemory {
    lines: Option<Range<usize>>,
    before: Option<Range<usize>>,
    since: f64,
}

/// The lit lines this frame, or `None` without a cursor (everything shows).
fn paragraph_focus(ui: &egui::Ui, tab: TabId, text: &str) -> Option<Lit> {
    let chars = selection(ui.ctx(), tab)?;
    let bytes = lines_around(text, chars);
    let line_of = |byte: usize| text[..byte.min(text.len())].matches('\n').count();
    let lines = line_of(bytes.start)..line_of(bytes.end) + 1;
    let id = editor_id(tab).with("paragraph-focus");
    let now = ui.input(|i| i.time);
    let mut memory: FocusMemory = ui.data(|d| d.get_temp(id)).unwrap_or_default();
    if memory.lines.as_ref() != Some(&lines) {
        memory = FocusMemory { before: memory.lines.take(), lines: Some(lines.clone()), since: now };
        ui.data_mut(|d| d.insert_temp(id, memory.clone()));
    }
    let moved = crate::motion::ease(now, memory.since, FOCUS_MOVE);
    if moved < 1.0 {
        ui.ctx().request_repaint();
    }
    Some(Lit { now: lines, before: memory.before.filter(|_| moved < 1.0), moved })
}

/// Paragraph focus: the text's colour faded on the lines not lit. Sections
/// are cut at each line's start, so each piece is on one line.
fn fade_paragraphs(mut job: LayoutJob, lit: &Lit) -> LayoutJob {
    let brightness = |line: usize| {
        let on = |r: &Range<usize>| r.contains(&line);
        let full = 1.0 - FADED;
        match (on(&lit.now), lit.before.as_ref().is_some_and(on)) {
            (true, true) => 1.0,
            (true, false) => FADED + full * lit.moved,
            (false, true) => 1.0 - full * lit.moved,
            (false, false) => FADED,
        }
    };
    let text = job.text.clone();
    let mut sections = Vec::with_capacity(job.sections.len());
    let mut line = 0;
    for section in job.sections.drain(..) {
        let range = section.byte_range.start.0..section.byte_range.end.0;
        let mut from = range.start;
        // Each line's piece of the section, ending after its newline.
        while from < range.end {
            let end = text[from..range.end].find('\n').map_or(range.end, |i| from + i + 1);
            let mut piece = section.clone();
            piece.byte_range = egui::text::ByteIndex(from)..egui::text::ByteIndex(end);
            let alpha = brightness(line);
            if alpha < 1.0 {
                piece.format.color = piece.format.color.gamma_multiply(alpha);
            }
            // Only the first piece keeps the section's leading space (indent).
            if from != range.start {
                piece.leading_space = 0.0;
            }
            sections.push(piece);
            if text[from..end].ends_with('\n') {
                line += 1;
            }
            from = end;
        }
    }
    job.sections = sections;
    job
}

fn lines_around(text: &str, chars: Range<usize>) -> Range<usize> {
    let (start, end) = (crate::find::char_to_byte(text, chars.start), crate::find::char_to_byte(text, chars.end));
    let start = text[..start].rfind('\n').map_or(0, |i| i + 1);
    let end = text[end..].find('\n').map_or(text.len(), |i| end + i);
    start..end
}

/// What was last copied as formatted text: (the plain text put on the
/// clipboard, the Markdown it came from).
fn last_copy_id() -> Id {
    Id::unique("sloth-editor-last-formatted-copy")
}

/// Copy and cut: swap the Markdown egui is about to put on the clipboard
/// for formatted text (HTML, with plain text for apps that don't take it).
fn copy_as_formatted(ctx: &egui::Context) {
    let copied: Vec<String> = ctx.output_mut(|o| {
        let mut copied = Vec::new();
        o.commands.retain(|c| match c {
            egui::OutputCommand::CopyText(t) => {
                copied.push(t.clone());
                false
            }
            _ => true,
        });
        copied
    });
    let Some(markdown) = copied.into_iter().last() else { return };
    let formatted = markdown::formatted(&markdown);
    if copy_html(&formatted.html, &formatted.plain) {
        ctx.data_mut(|d| d.insert_temp(last_copy_id(), (formatted.plain, markdown)));
    } else {
        // Couldn't reach the clipboard ourselves: let egui copy it as it was.
        ctx.copy_text(markdown);
    }
}

/// Put formatted text on the clipboard: `html` for apps that take
/// formatting (a word processor, WordPress), `plain` for the rest.
/// Returns whether it got there.
pub fn copy_html(html: &str, plain: &str) -> bool {
    arboard::Clipboard::new().and_then(|mut c| c.set().html(html, Some(plain))).is_ok()
}

/// Pasting back what was copied from here gives back its Markdown, so
/// bold stays bold when moving text between documents.
fn paste_markdown_back(ctx: &egui::Context) {
    let Some((plain, markdown)) = ctx.data(|d| d.get_temp::<(String, String)>(last_copy_id())) else { return };
    ctx.input_mut(|i| {
        for event in &mut i.events {
            if let egui::Event::Paste(pasted) = event
                && *pasted == plain
            {
                *pasted = markdown.clone();
            }
        }
    });
}

/// `hide_markers`: hide Markdown markers, except within this byte range.
#[allow(clippy::too_many_arguments)]
fn build_layout(
    text: &str,
    font: &FontId,
    line_height: Option<f32>,
    color: Color32,
    spans: &[Span],
    highlights: &Highlights<'_>,
    styles: &[(Range<usize>, markdown::Style)],
    faces: Option<&Faces>,
    hide_markers: Option<&Range<usize>>,
    comments_shown: Option<&Range<usize>>,
) -> LayoutJob {
    let plain = TextFormat { line_height, ..TextFormat::simple(font.clone(), color) };
    if spans.is_empty() && highlights.all.is_empty() && highlights.marks.is_empty() && styles.is_empty() {
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
    for (r, _) in styles {
        cuts.extend([r.start, r.end]);
    }
    let marks: Vec<&Mark> = highlights.marks.iter().filter(|m| valid(&m.range)).collect();
    for m in &marks {
        cuts.extend([m.range.start, m.range.end]);
    }
    cuts.retain(|&c| c <= text.len() && text.is_char_boundary(c));
    cuts.sort_unstable();
    cuts.dedup();

    let mut job = LayoutJob::default();
    let (mut si, mut bi, mut mi, mut ki) = (0, 0, 0, 0);
    for pair in cuts.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        while si < spans.len() && spans[si].range.end <= a {
            si += 1;
        }
        while bi < backgrounds.len() && backgrounds[bi].0.end <= a {
            bi += 1;
        }
        while mi < styles.len() && styles[mi].0.end <= a {
            mi += 1;
        }
        while ki < marks.len() && marks[ki].range.end <= a {
            ki += 1;
        }
        let mut format = plain.clone();
        if let (Some((_, style)), Some(faces)) = (styles.get(mi).filter(|(r, _)| r.start <= a), faces) {
            faces.style(&mut format, *style, color);
            let marker = style & markdown::MARKER != 0 && style & markdown::BREAK == 0;
            let away = |shown: &Range<usize>| !(shown.start <= a && b <= shown.end);
            let hidden_comment = style & markdown::COMMENT != 0 && comments_shown.is_some_and(away);
            if (marker && hide_markers.is_some_and(away)) || hidden_comment {
                // Still there (the cursor steps over it), but too small to see.
                format.font_id.size = HIDDEN_SIZE;
                format.color = Color32::TRANSPARENT;
            }
        }
        if let Some(span) = spans.get(si).filter(|s| s.range.start <= a) {
            format.color = span.color;
            format.italics = span.italic;
        }
        if let Some(mark) = marks.get(ki).filter(|m| m.range.start <= a) {
            format.background = mark.background;
        }
        // Search matches show over marks.
        if let Some((_, bg)) = backgrounds.get(bi).filter(|(r, _)| r.start <= a) {
            format.background = *bg;
        }
        job.append(&text[a..b], 0.0, format);
    }
    job
}

/// The prose font's faces, for Markdown styling: which bold and italic
/// families are installed (see `fonts::prose_variant`).
pub(crate) struct Faces {
    size: f32,
    bold: Option<egui::FontFamily>,
    italic: Option<egui::FontFamily>,
    bold_italic: Option<egui::FontFamily>,
}

impl Faces {
    fn find(ui: &egui::Ui, font: &FontId) -> Self {
        let installed = ui.fonts(|f| f.families());
        let prose = font.family == crate::fonts::prose_family();
        let variant = |bold, italic| {
            crate::fonts::prose_variant(bold, italic).filter(|f| prose && installed.contains(f))
        };
        Self { size: font.size, bold: variant(true, false), italic: variant(false, true), bold_italic: variant(true, true) }
    }

    /// Apply a Markdown style to a piece of text's format.
    fn style(&self, format: &mut TextFormat, style: markdown::Style, color: Color32) {
        use markdown::{BOLD, BREAK, HEADING, ITALIC, MARKER, NOTE, QUOTE, REFERENCE};
        let bold = style & (BOLD | HEADING) != 0;
        let italic = style & (ITALIC | NOTE) != 0;
        let family = match (bold, italic) {
            (true, true) => self.bold_italic.clone().or_else(|| self.bold.clone()),
            (true, false) => self.bold.clone(),
            (false, true) => self.italic.clone(),
            (false, false) => None,
        };
        // Without a true italic face, slant the regular one.
        if italic && (family.is_none() || (bold && self.bold_italic.is_none())) {
            format.italics = true;
        }
        if let Some(family) = family {
            format.font_id.family = family;
        }
        if style & HEADING != 0 {
            format.font_id.size = self.size * 1.3;
            format.line_height = format.line_height.map(|h| h * 1.3);
        }
        if style & markdown::COMMENTED != 0 {
            format.background = COMMENT_TINT;
        }
        if style & markdown::COMMENT != 0 {
            // Small, in the comment colour, like a note in the margin.
            format.font_id.size = self.size * 0.8;
            format.color = if style & MARKER != 0 { COMMENT_COLOR.gamma_multiply(0.5) } else { COMMENT_COLOR };
            return;
        }
        if style & REFERENCE != 0 {
            // Small and blue, like a note in the margin. (No background:
            // it would fill the whole line's height.)
            format.font_id.size = self.size * 0.8;
            format.color = if style & MARKER != 0 { REFERENCE_COLOR.gamma_multiply(0.5) } else { REFERENCE_COLOR };
        } else if style & (MARKER | BREAK) != 0 {
            format.color = color.gamma_multiply(0.35);
        } else if style & NOTE != 0 {
            format.color = color.gamma_multiply(0.5);
        } else if style & QUOTE != 0 {
            format.color = color.gamma_multiply(0.75);
        }
    }
}

/// Which line of the text is at `top` (the top of the view, on screen):
/// 0-based, plus how far through it (a wrapped line has several rows).
fn top_line(output: &egui::text_edit::TextEditOutput, top: f32) -> f32 {
    let rows = &output.galley.rows;
    let mut line = 0usize;
    let mut line_top: Option<f32> = None;
    for (i, row) in rows.iter().enumerate() {
        let y = output.galley_pos.y + row.pos.y;
        let start = *line_top.get_or_insert(y);
        if row.ends_with_newline || i + 1 == rows.len() {
            let bottom = y + row.height();
            if bottom > top {
                return line as f32 + ((top - start) / (bottom - start).max(1.0)).clamp(0.0, 1.0);
            }
            line += 1;
            line_top = None;
        }
    }
    line as f32
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
    fn paragraph_focus_fades_the_other_lines() {
        let text = "One.\nTwo is here.\nThree.";
        let mut job = LayoutJob::default();
        job.append(text, 0.0, TextFormat::simple(FontId::proportional(14.0), Color32::WHITE));
        let brightness = |job: &LayoutJob| -> Vec<(String, u8)> {
            job.sections.iter().map(|s| (job.text[s.byte_range.start.0..s.byte_range.end.0].to_owned(), s.format.color.a())).collect()
        };
        // The light on line 2 (index 1), settled.
        let lit = Lit { now: 1..2, before: None, moved: 1.0 };
        let faded = (255.0 * FADED).round() as u8;
        let got = brightness(&fade_paragraphs(job.clone(), &lit));
        assert_eq!(got.iter().map(|(t, _)| t.as_str()).collect::<Vec<_>>(), ["One.\n", "Two is here.\n", "Three."]);
        assert_eq!(got.iter().map(|(_, a)| *a).collect::<Vec<_>>(), [faded, 255, faded]);

        // Halfway from line 1 to line 3: both between.
        let lit = Lit { now: 2..3, before: Some(0..1), moved: 0.5 };
        let alphas: Vec<u8> = brightness(&fade_paragraphs(job, &lit)).into_iter().map(|(_, a)| a).collect();
        assert!(alphas[0] > faded && alphas[0] < 255 && alphas[2] > faded && alphas[2] < 255);
        assert_eq!(alphas[1], faded);
    }

    #[test]
    fn layout_combines_code_colours_and_search_highlights() {
        let font = FontId::monospace(14.0);
        let text = "let x = 10;";
        let red = Color32::RED;
        let spans = [Span { range: 0..3, color: red, italic: false }]; // "let"
        let found: Vec<Range<usize>> = std::iter::once(2..5).collect(); // "t x": overlaps the end of "let"
        let highlights = Highlights { all: &found, current: None, ..Default::default() };
        let job = build_layout(text, &font, None, Color32::WHITE, &spans, &highlights, &[], None, None, None);

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

    #[cfg(target_os = "macos")]
    #[test]
    fn the_word_being_typed_isnt_flagged_yet() {
        let text = "a wrod and tset";
        let words = |cursor| misspellings(text, cursor).into_iter().map(|r| &text[r]).collect::<Vec<_>>();
        assert_eq!(words(None), vec!["wrod", "tset"]);
        assert_eq!(words(Some(15)), vec!["wrod"], "typing at the end of tset");
        assert_eq!(words(Some(4)), vec!["tset"], "cursor inside wrod");
        assert_eq!(words(Some(8)), vec!["wrod", "tset"], "cursor elsewhere");
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

    #[test]
    fn markdown_styles_the_text_and_dims_its_markers() {
        let text = "a *b* c";
        let styles = markdown::styles(text);
        // No bold or italic faces installed: italics are slanted.
        let faces = Faces { size: 14.0, bold: None, italic: None, bold_italic: None };
        let font = FontId::proportional(14.0);
        let job = build_layout(text, &font, None, Color32::WHITE, &[], &Highlights::default(), &styles, Some(&faces), None, None);
        let pieces: Vec<(&str, bool, bool)> = job
            .sections
            .iter()
            .map(|s| (&text[s.byte_range.start.0..s.byte_range.end.0], s.format.italics, s.format.color != Color32::WHITE))
            .collect();
        assert_eq!(pieces, [("a ", false, false), ("*", false, true), ("b", true, false), ("*", false, true), (" c", false, false)]);
    }

    #[test]
    fn paragraphs_get_space_after_them_but_blank_lines_dont() {
        let ctx = egui::Context::default();
        let mut laid_out = None;
        let mut output = ctx.run_ui(Default::default(), |ui| {
            laid_out = Some(ui.fonts_mut(|f| {
                f.layout_job(LayoutJob::simple("One\nTwo\n\nThree".into(), FontId::proportional(14.0), Color32::WHITE, f32::INFINITY))
            }));
        });
        output.textures_delta.clear();
        let galley = laid_out.unwrap();
        let tops = |g: &egui::Galley| g.rows.iter().map(|r| r.pos.y).collect::<Vec<_>>();
        let before = tops(&galley);
        let height = galley.rect.height();
        let spaced = space_paragraphs(galley, 10.0);
        let after = tops(&spaced);
        // One, Two, (blank), Three: 10 more after One, 10 more after Two,
        // nothing more after the blank line.
        assert_eq!(after[0], before[0]);
        assert_eq!(after[1], before[1] + 10.0);
        assert_eq!(after[2], before[2] + 20.0);
        assert_eq!(after[3], before[3] + 20.0);
        assert_eq!(spaced.rect.height(), height + 20.0);
        // The cursor finds the spaced rows: the start of "Three".
        let pos = spaced.pos_from_cursor(CCursor::new(9));
        assert_eq!(pos.top(), after[3]);
    }

    #[test]
    fn each_paragraph_starts_indented_and_wraps_to_the_margin() {
        let ctx = egui::Context::default();
        let lay = |indent: f32| {
            let mut galley = None;
            let mut output = ctx.run_ui(Default::default(), |ui| {
                let mut job = LayoutJob::simple(
                    "One two three four five six seven eight nine ten\n\nEleven".into(),
                    FontId::proportional(14.0),
                    Color32::WHITE,
                    150.0,
                );
                // Split mid-paragraph, as Markdown styling does.
                job.sections = vec![
                    egui::text::LayoutSection { byte_range: egui::text::ByteIndex(0)..egui::text::ByteIndex(8), ..job.sections[0].clone() },
                    egui::text::LayoutSection { byte_range: egui::text::ByteIndex(8)..egui::text::ByteIndex(job.text.len()), ..job.sections[0].clone() },
                ];
                galley = Some(ui.fonts_mut(|f| f.layout_job(indent_paragraphs(job, indent))));
            });
            output.textures_delta.clear();
            galley.unwrap()
        };
        let (plain, indented) = (lay(0.0), lay(20.0));
        let left = |g: &egui::Galley, row: usize| g.rows[row].pos.x + g.rows[row].row.glyphs.first().map_or(0.0, |g| g.pos.x);
        // The paragraph's first row is indented, its wrapped rows aren't.
        assert!((left(&indented, 0) - left(&plain, 0) - 20.0).abs() < 0.5);
        assert!((left(&indented, 1) - left(&plain, 1)).abs() < 0.5);
        // "Eleven", a paragraph of its own, is indented too.
        let last = |g: &egui::Galley| left(g, g.rows.len() - 1);
        assert!((last(&indented) - last(&plain) - 20.0).abs() < 0.5);
        // Nothing runs past the wrap width.
        assert!(indented.rows.iter().all(|r| r.pos.x + r.row.size.x <= 150.5));
        // The cursor at the start of a paragraph sits at the indent.
        let eleven = indented.text().find("Eleven").unwrap();
        let at = indented.pos_from_cursor(CCursor::new(eleven));
        assert!((at.left() - plain.pos_from_cursor(CCursor::new(eleven)).left() - 20.0).abs() < 0.5);
    }

    #[test]
    fn hidden_markers_take_no_room_except_where_the_cursor_is() {
        let text = "a **b** c\n# Head\n#\na **b** c";
        let styles = markdown::styles(text);
        let faces = Faces { size: 14.0, bold: None, italic: None, bold_italic: None };
        let font = FontId::proportional(14.0);
        let ctx = egui::Context::default();
        let lay = |hide: Option<&Range<usize>>| {
            let job = build_layout(text, &font, None, Color32::WHITE, &[], &Highlights::default(), &styles, Some(&faces), hide, None);
            let mut galley = None;
            let mut output = ctx.run_ui(Default::default(), |ui| galley = Some(ui.fonts_mut(|f| f.layout_job(job.clone()))));
            output.textures_delta.clear();
            galley.unwrap()
        };
        let width = |g: &egui::Galley, row: usize| g.rows[row].row.size.x;
        // The cursor's in the last line: its markers stay, the others go.
        let last_line = text.rfind('\n').unwrap() + 1..text.len();
        let (shown, hidden) = (lay(None), lay(Some(&last_line)));
        let plain = {
            let job = build_layout("a b c", &font, None, Color32::WHITE, &[], &Highlights::default(), &[], None, None, None);
            let mut galley = None;
            let mut output = ctx.run_ui(Default::default(), |ui| galley = Some(ui.fonts_mut(|f| f.layout_job(job.clone()))));
            output.textures_delta.clear();
            galley.unwrap()
        };
        assert!((width(&hidden, 0) - width(&plain, 0)).abs() < 0.5, "\"a **b** c\" as wide as \"a b c\"");
        assert!(width(&hidden, 1) < width(&shown, 1), "the heading's # is hidden");
        assert!((width(&hidden, 2) - width(&shown, 2)).abs() < 0.01, "a scene break is never hidden");
        assert!((width(&hidden, 3) - width(&shown, 3)).abs() < 0.01, "the cursor's line keeps its markers");
        // Rows keep their height.
        assert!((hidden.rows[0].row.size.y - shown.rows[0].row.size.y).abs() < 0.01);
    }
}
