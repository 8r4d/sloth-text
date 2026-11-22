//! Find & replace: the search logic (pure functions, unit-tested) and the
//! find bar UI.
//!
//! Match positions are byte ranges into the document text.

use std::ops::Range;

use eframe::egui::{self, Key, TextEdit};
use regex::{Regex, RegexBuilder};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FindOptions {
    pub match_case: bool,
    pub whole_word: bool,
    /// Treat the query as a regular expression.
    pub regex: bool,
}

/// Build the search. Plain searches are escaped, so they go through the same
/// (fast, Unicode-aware) regex engine.
fn compile(query: &str, opts: FindOptions) -> Result<Regex, String> {
    let pattern = if opts.regex { query.to_owned() } else { regex::escape(query) };
    RegexBuilder::new(&pattern)
        .case_insensitive(!opts.match_case)
        .multi_line(true)
        .build()
        .map_err(|e| describe_regex_error(&e))
}

/// The regex crate's errors span several lines (pattern, caret, message);
/// keep just the message.
fn describe_regex_error(e: &regex::Error) -> String {
    let text = e.to_string();
    text.lines()
        .rev()
        .find_map(|l| l.trim().strip_prefix("error: "))
        .unwrap_or_else(|| text.lines().next().unwrap_or("invalid pattern"))
        .to_owned()
}

/// Matches worth showing: non-empty (a pattern like `^` matches nothing
/// visible), and on word boundaries when asked.
fn keep(text: &str, m: &regex::Match<'_>, opts: FindOptions) -> bool {
    !m.is_empty() && (!opts.whole_word || is_word_boundary(text, m.start(), m.end()))
}

/// All non-overlapping matches of `query` in `text`, as byte ranges, or a
/// message if the pattern is invalid.
pub fn find_all(text: &str, query: &str, opts: FindOptions) -> Result<Vec<Range<usize>>, String> {
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let re = compile(query, opts)?;
    Ok(re
        .find_iter(text)
        .filter(|m| keep(text, m, opts))
        .map(|m| m.range())
        .collect())
}

fn is_word_boundary(text: &str, start: usize, end: usize) -> bool {
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let before = text[..start].chars().next_back();
    let after = text[end..].chars().next();
    !before.is_some_and(is_word) && !after.is_some_and(is_word)
}

/// The text to put in place of the match at `range`. In regex mode `$1`,
/// `${name}` etc. refer to the match's groups; otherwise it's used as is.
pub fn replacement_for(
    text: &str,
    range: Range<usize>,
    query: &str,
    replacement: &str,
    opts: FindOptions,
) -> Result<String, String> {
    if !opts.regex {
        return Ok(replacement.to_owned());
    }
    let re = compile(query, opts)?;
    let caps = re
        .captures_at(text, range.start)
        .filter(|c| c.get(0).is_some_and(|m| m.range() == range))
        .ok_or_else(|| "the match has changed".to_owned())?;
    let mut out = String::new();
    caps.expand(replacement, &mut out);
    Ok(out)
}

/// Replace every match; returns the new text and the number of replacements.
pub fn replace_all(
    text: &str,
    query: &str,
    replacement: &str,
    opts: FindOptions,
) -> Result<(String, usize), String> {
    if query.is_empty() {
        return Ok((text.to_owned(), 0));
    }
    let re = compile(query, opts)?;
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    let mut count = 0;
    for caps in re.captures_iter(text) {
        let m = caps.get(0).expect("group 0 always matches");
        if !keep(text, &m, opts) {
            continue;
        }
        out.push_str(&text[last..m.start()]);
        if opts.regex {
            caps.expand(replacement, &mut out);
        } else {
            out.push_str(replacement);
        }
        last = m.end();
        count += 1;
    }
    out.push_str(&text[last..]);
    Ok((out, count))
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
    /// Position while stepping through recent searches with ↑/↓.
    history_pos: Option<usize>,
    /// What was typed before stepping into the history.
    draft: String,
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
    /// Show or hide the all-tabs results panel.
    ToggleResults,
}

/// What the find bar shows besides its own state.
pub struct FindBarInfo<'a> {
    pub match_count: usize,
    pub current_index: Option<usize>,
    /// Why the search can't run (an invalid regex), if it can't.
    pub error: Option<&'a str>,
    /// Recent searches, newest first.
    pub history: &'a [String],
    pub results_open: bool,
    /// The button that shows the results panel, and its tooltip.
    pub results_label: &'a str,
    pub results_hint: &'a str,
    /// Offer the Replace… button.
    pub allow_replace: bool,
}

pub fn find_bar(ui: &mut egui::Ui, state: &mut FindState, info: FindBarInfo<'_>) -> Option<FindAction> {
    let FindBarInfo { match_count, current_index, error, history, results_open, results_label, results_hint, allow_replace } =
        info;
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
        let query = if history.is_empty() {
            query
        } else {
            query.on_hover_text("\u{2191} \u{2193} for recent searches")
        };
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
            state.history_pos = None;
            action = Some(FindAction::Search);
        }
        // ↑/↓ step through recent searches.
        if query.has_focus() && !history.is_empty() {
            let (up, down) = ui.input_mut(|i| {
                (
                    i.consume_key(egui::Modifiers::NONE, Key::ArrowUp),
                    i.consume_key(egui::Modifiers::NONE, Key::ArrowDown),
                )
            });
            let new_pos = match (up, down, state.history_pos) {
                (true, _, None) => {
                    state.draft = state.query.clone();
                    Some(Some(0))
                }
                (true, _, Some(p)) => Some(Some((p + 1).min(history.len() - 1))),
                (_, true, Some(0)) => Some(None),
                (_, true, Some(p)) => Some(Some(p - 1)),
                _ => None,
            };
            if let Some(pos) = new_pos {
                state.history_pos = pos;
                state.query = match pos {
                    Some(p) => history[p].clone(),
                    None => std::mem::take(&mut state.draft),
                };
                action = Some(FindAction::Search);
            }
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

        if ui.button("<").on_hover_text("Previous match (⇧⌘G)").clicked() {
            action = Some(FindAction::Previous);
        }
        if ui.button(">").on_hover_text("Next match (⌘G)").clicked() {
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
        let changed_regex = ui
            .toggle_value(&mut state.options.regex, ".*")
            .on_hover_text("Regular expression (use $1, $2 \u{2026} in the replacement)")
            .changed();
        if changed_case || changed_word || changed_regex {
            action = Some(FindAction::Search);
        }

        if let Some(error) = error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        } else {
            let summary = match (state.query.is_empty(), match_count, current_index) {
                (true, _, _) => String::new(),
                (false, 0, _) => "No matches".to_owned(),
                (false, n, Some(i)) => format!("{} of {n}", i + 1),
                (false, 1, None) => "1 match".to_owned(),
                (false, n, None) => format!("{n} matches"),
            };
            ui.weak(summary);
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("×").on_hover_text("Close (Esc)").clicked() {
                action = Some(FindAction::Close);
            }
            if ui.selectable_label(results_open, results_label).on_hover_text(results_hint).clicked() {
                action = Some(FindAction::ToggleResults);
            }
            let label = if state.show_replace { "Hide Replace" } else { "Replace…" };
            if allow_replace && ui.button(label).clicked() {
                state.show_replace = !state.show_replace;
            }
        });
    });

    if allow_replace && state.show_replace {
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

    const ANY: FindOptions = FindOptions { match_case: false, whole_word: false, regex: false };

    fn find(text: &str, query: &str, opts: FindOptions) -> Vec<Range<usize>> {
        find_all(text, query, opts).unwrap()
    }

    #[test]
    fn finds_case_insensitively_by_default() {
        let text = "Cat cat CAT scatter";
        assert_eq!(find(text, "cat", ANY), vec![0..3, 4..7, 8..11, 13..16]);
    }

    #[test]
    fn match_case_and_whole_word() {
        let text = "Cat cat CAT scatter";
        let case = FindOptions { match_case: true, ..ANY };
        assert_eq!(find(text, "cat", case), vec![4..7, 13..16]);
        let word = FindOptions { whole_word: true, ..ANY };
        assert_eq!(find(text, "cat", word), vec![0..3, 4..7, 8..11]);
    }

    #[test]
    fn plain_search_treats_regex_characters_literally() {
        assert_eq!(find("a.b axb (x)", "a.b", ANY), vec![0..3]);
        assert_eq!(find("a.b axb (x)", "(x)", ANY), vec![8..11]);
        // Whole word works even when the query starts with punctuation.
        let word = FindOptions { whole_word: true, ..ANY };
        assert_eq!(find("-x y-x", "-x", word), vec![0..2]);
    }

    #[test]
    fn handles_multibyte_text() {
        let text = "café Café CAFÉ";
        let m = find(text, "café", ANY);
        assert_eq!(m.len(), 3);
        for r in &m {
            assert!(text[r.clone()].to_lowercase() == "café");
        }
        assert_eq!(byte_to_char(text, m[1].start), 5);
        assert_eq!(char_to_byte(text, 5), m[1].start);
    }

    #[test]
    fn regex_search_and_capture_replacement() {
        let re = FindOptions { regex: true, ..ANY };
        let text = "2024-01-15 and 1999-12-31";
        assert_eq!(find(text, r"\d{4}-\d\d", re), vec![0..7, 15..22]);
        let (out, n) = replace_all(text, r"(\d{4})-(\d\d)-(\d\d)", "$3/$2/$1", re).unwrap();
        assert_eq!((out.as_str(), n), ("15/01/2024 and 31/12/1999", 2));
        assert_eq!(
            replacement_for(text, 0..10, r"(\d{4})-(\d\d)-(\d\d)", "$3.$2", re).unwrap(),
            "15.01"
        );
        // Lines can be anchored.
        assert_eq!(find("ab\nab", "^a", re), vec![0..1, 3..4]);
        // Empty matches (like "^" alone) are skipped.
        assert!(find("abc", "^", re).is_empty());
    }

    #[test]
    fn invalid_regex_gives_a_short_message() {
        let re = FindOptions { regex: true, ..ANY };
        let err = find_all("abc", "(unclosed", re).unwrap_err();
        assert!(!err.contains('\n'), "{err}");
        assert!(err.contains("unclosed"), "{err}");
    }

    #[test]
    fn plain_replacement_is_literal() {
        let (out, n) = replace_all("a-b-a-B", "b", "$1", ANY).unwrap();
        assert_eq!((out.as_str(), n), ("a-$1-a-$1", 2));
        let (out, n) = replace_all("aaa", "a", "aa", ANY).unwrap();
        assert_eq!((out.as_str(), n), ("aaaaaa", 3));
    }

    #[test]
    fn empty_query_matches_nothing() {
        assert!(find("abc", "", ANY).is_empty());
    }
}
