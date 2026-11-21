//! Find & replace: the search logic (pure functions, unit-tested) and the
//! find bar UI.
//!
//! Match positions are byte ranges into the document text.

use std::ops::Range;

use eframe::egui::{self, Key, TextEdit};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FindOptions {
    pub match_case: bool,
    pub whole_word: bool,
}

/// All non-overlapping matches of `query` in `text`, as byte ranges.
pub fn find_all(text: &str, query: &str, opts: FindOptions) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    if query.is_empty() {
        return out;
    }
    let mut start = 0;
    while start < text.len() {
        if let Some(end) = match_at(text, start, query, opts.match_case)
            && (!opts.whole_word || is_word_boundary(text, start, end))
        {
            out.push(start..end);
            start = end;
            continue;
        }
        // Advance one character.
        start += text[start..].chars().next().map_or(1, char::len_utf8);
    }
    out
}

/// If `query` matches at byte offset `start`, return the end of the match.
fn match_at(text: &str, start: usize, query: &str, match_case: bool) -> Option<usize> {
    let mut chars = text[start..].char_indices();
    for q in query.chars() {
        let (_, c) = chars.next()?;
        let equal = c == q || (!match_case && c.to_lowercase().eq(q.to_lowercase()));
        if !equal {
            return None;
        }
    }
    Some(chars.next().map_or(text.len(), |(i, _)| start + i))
}

fn is_word_boundary(text: &str, start: usize, end: usize) -> bool {
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let before = text[..start].chars().next_back();
    let after = text[end..].chars().next();
    !before.is_some_and(is_word) && !after.is_some_and(is_word)
}

/// Replace every match; returns the new text and the number of replacements.
pub fn replace_all(text: &str, query: &str, replacement: &str, opts: FindOptions) -> (String, usize) {
    let matches = find_all(text, query, opts);
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for m in &matches {
        out.push_str(&text[last..m.start]);
        out.push_str(replacement);
        last = m.end;
    }
    out.push_str(&text[last..]);
    (out, matches.len())
}

/// Convert a character index (as used by the text cursor) to a byte offset.
pub fn char_to_byte(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .nth(char_index)
        .map_or(text.len(), |(b, _)| b)
}

/// Convert a byte offset to a character index.
pub fn byte_to_char(text: &str, byte: usize) -> usize {
    text[..byte.min(text.len())].chars().count()
}

// ----------------------------------------------------------------------------
// Find bar

#[derive(Default)]
pub struct FindState {
    pub visible: bool,
    pub show_replace: bool,
    pub query: String,
    pub replacement: String,
    pub options: FindOptions,
    /// The match the user is currently on (byte range), if any.
    pub current: Option<Range<usize>>,
    /// Move keyboard focus to the search field on the next frame.
    focus_query: bool,
    /// Move keyboard focus to the replace field on the next frame.
    focus_replace: bool,
}

impl FindState {
    pub fn open(&mut self, with_replace: bool, prefill: Option<String>) {
        self.visible = true;
        self.show_replace |= with_replace;
        if let Some(text) = prefill {
            self.query = text;
        }
        self.focus_query = true;
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.show_replace = false;
        self.current = None;
    }

    pub fn is_searching(&self) -> bool {
        self.visible && !self.query.is_empty()
    }
}

pub enum FindAction {
    /// The query or options changed: jump to the first match from the cursor.
    Search,
    Next,
    Previous,
    Replace,
    ReplaceAll,
    Close,
}

pub fn find_bar(ui: &mut egui::Ui, state: &mut FindState, match_count: usize, current_index: Option<usize>) -> Option<FindAction> {
    let mut action = None;
    let enter_pressed = |ui: &egui::Ui| ui.input(|i| i.key_pressed(Key::Enter));
    let shift_held = |ui: &egui::Ui| ui.input(|i| i.modifiers.shift);

    ui.add_space(2.0);
    ui.horizontal(|ui| {
        let field_width = 260.0;
        let query = ui.add(
            TextEdit::singleline(&mut state.query)
                .hint_text("Find")
                .desired_width(field_width)
                // Keep Tab from moving focus to the buttons; we send it to
                // the replace field instead (below).
                .lock_focus(state.show_replace),
        );
        if state.focus_query {
            query.request_focus();
            // Select the existing text so typing replaces it.
            if let Some(mut s) = egui::text_edit::TextEditState::load(ui.ctx(), query.id) {
                s.cursor.set_char_range(Some(egui::text::CCursorRange::select_all_str(&state.query)));
                s.store(ui.ctx(), query.id);
            }
            state.focus_query = false;
        }
        if query.changed() {
            action = Some(FindAction::Search);
        }
        // Tab goes from the search field straight to the replace field.
        if state.show_replace
            && query.has_focus()
            && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Tab))
        {
            state.focus_replace = true;
        }
        if query.lost_focus() && enter_pressed(ui) {
            action = Some(if shift_held(ui) { FindAction::Previous } else { FindAction::Next });
            query.request_focus(); // keep typing / pressing Enter
        }

        if ui.button("⏶").on_hover_text("Previous match (⇧⌘G)").clicked() {
            action = Some(FindAction::Previous);
        }
        if ui.button("⏷").on_hover_text("Next match (⌘G)").clicked() {
            action = Some(FindAction::Next);
        }

        let changed_case = ui
            .toggle_value(&mut state.options.match_case, "Aa")
            .on_hover_text("Match case")
            .changed();
        let changed_word = ui
            .toggle_value(&mut state.options.whole_word, "Word")
            .on_hover_text("Whole words only")
            .changed();
        if changed_case || changed_word {
            action = Some(FindAction::Search);
        }

        let summary = match (state.query.is_empty(), match_count, current_index) {
            (true, _, _) => String::new(),
            (false, 0, _) => "No matches".to_owned(),
            (false, n, Some(i)) => format!("{} of {n}", i + 1),
            (false, 1, None) => "1 match".to_owned(),
            (false, n, None) => format!("{n} matches"),
        };
        ui.weak(summary);

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("×").on_hover_text("Close (Esc)").clicked() {
                action = Some(FindAction::Close);
            }
            let label = if state.show_replace { "Hide Replace" } else { "Replace…" };
            if ui.button(label).clicked() {
                state.show_replace = !state.show_replace;
            }
        });
    });

    if state.show_replace {
        ui.horizontal(|ui| {
            let replace = ui.add(
                TextEdit::singleline(&mut state.replacement)
                    .hint_text("Replace with")
                    .desired_width(260.0),
            );
            if std::mem::take(&mut state.focus_replace) {
                replace.request_focus();
            }
            if replace.lost_focus() && enter_pressed(ui) {
                action = Some(FindAction::Replace);
                replace.request_focus();
            }
            let enabled = match_count > 0;
            if ui.add_enabled(enabled, egui::Button::new("Replace")).clicked() {
                action = Some(FindAction::Replace);
            }
            if ui.add_enabled(enabled, egui::Button::new("Replace All")).clicked() {
                action = Some(FindAction::ReplaceAll);
            }
        });
    }
    ui.add_space(2.0);

    action
}

#[cfg(test)]
mod tests {
    use super::*;

    const ANY: FindOptions = FindOptions { match_case: false, whole_word: false };

    #[test]
    fn finds_case_insensitively_by_default() {
        let text = "Cat cat CAT scatter";
        assert_eq!(find_all(text, "cat", ANY), vec![0..3, 4..7, 8..11, 13..16]);
    }

    #[test]
    fn match_case_and_whole_word() {
        let text = "Cat cat CAT scatter";
        let case = FindOptions { match_case: true, whole_word: false };
        assert_eq!(find_all(text, "cat", case), vec![4..7, 13..16]);
        let word = FindOptions { match_case: false, whole_word: true };
        assert_eq!(find_all(text, "cat", word), vec![0..3, 4..7, 8..11]);
    }

    #[test]
    fn handles_multibyte_text() {
        let text = "café Café CAFÉ";
        let m = find_all(text, "café", ANY);
        assert_eq!(m.len(), 3);
        for r in &m {
            assert!(text[r.clone()].to_lowercase() == "café");
        }
        assert_eq!(byte_to_char(text, m[1].start), 5);
        assert_eq!(char_to_byte(text, 5), m[1].start);
    }

    #[test]
    fn replace_all_counts() {
        let (out, n) = replace_all("a-b-a-B", "b", "x", ANY);
        assert_eq!((out.as_str(), n), ("a-x-a-x", 2));
        let (out, n) = replace_all("aaa", "a", "aa", ANY);
        assert_eq!((out.as_str(), n), ("aaaaaa", 3));
    }

    #[test]
    fn empty_query_matches_nothing() {
        assert!(find_all("abc", "", ANY).is_empty());
    }
}
