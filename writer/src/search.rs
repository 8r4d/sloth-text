//! Find in Sloth Writer. ⌘F searches the open document: matches are
//! highlighted and ⌘G / ⇧⌘G step through them. ⇧⌘F also lists the matches
//! in every document in the project (outside the Trash); clicking one opens
//! that document at the match. The search itself, the find bar and the
//! results panel come from core, as in Sloth.

use std::fs;
use std::ops::Range;
use std::time::Duration;

use eframe::egui;
use sloth_core::editor;
use sloth_core::find::{self, FindAction, FindBarInfo};
use sloth_core::search_results::{self, ResultRow, ResultsAction, ResultsWording, TabResults};

use super::WriterApp;

/// While the project results are shown, they're rebuilt this often (as
/// well as whenever the search changes), so they keep up with typing.
const REFRESH_SECONDS: f64 = 1.0;

const WORDING: ResultsWording = ResultsWording {
    one: "document",
    many: "documents",
    none: "No matches in the project",
};

/// One document's matches in a project search.
pub(super) struct DocResults {
    id: String,
    title: String,
    rows: Vec<ResultRow>,
}

#[derive(Clone, Copy)]
pub(super) enum Direction {
    /// Typing in the search field: stay on a match at the cursor.
    FromCursor,
    Next,
    Previous,
}

impl WriterApp {
    /// ⌘F, or ⇧⌘F with `project_wide` to show the results for the whole project.
    pub(super) fn open_find(&mut self, project_wide: bool) {
        self.find.open(false, None);
        if project_wide {
            self.show_results = true;
            self.results_key = None;
        }
    }

    pub(super) fn find_action(&mut self, ctx: &egui::Context, action: FindAction) {
        match action {
            FindAction::Search => {
                self.results_key = None;
                if self.find.query.is_empty() {
                    self.find.current = None;
                } else {
                    self.find_step(ctx, Direction::FromCursor);
                }
            }
            FindAction::Next => self.find_step(ctx, Direction::Next),
            FindAction::Previous => self.find_step(ctx, Direction::Previous),
            // The find bar doesn't offer Replace in Writer yet.
            FindAction::Replace | FindAction::ReplaceAll => {}
            FindAction::ToggleResults => {
                self.show_results = !self.show_results;
                self.results_key = None;
            }
            FindAction::Close => {
                self.find.close();
                self.show_results = false;
                self.focus_editor = true;
            }
        }
    }

    /// Matches in the open document, as byte ranges. Empty when there's
    /// nothing to search; an error for an invalid regular expression.
    pub(super) fn current_matches(&mut self) -> Result<Vec<Range<usize>>, String> {
        if !self.find.is_searching() {
            return Ok(Vec::new());
        }
        let (query, options) = (self.find.query.clone(), self.find.options);
        match self.current() {
            Some(tab) => find::find_all(&tab.doc.text, &query, options),
            None => Ok(Vec::new()),
        }
    }

    pub(super) fn find_step(&mut self, ctx: &egui::Context, direction: Direction) {
        if self.find.query.is_empty() {
            self.find.open(false, None);
            return;
        }
        self.find.visible = true;
        let matches = self.current_matches().unwrap_or_default();
        if matches.is_empty() {
            self.find.current = None;
            return;
        }
        // Step from the current match, or else from the cursor.
        let anchor = self
            .find
            .current
            .clone()
            .filter(|c| matches.contains(c))
            .unwrap_or_else(|| self.selection_bytes(ctx));
        let index = match direction {
            Direction::FromCursor => matches.iter().position(|m| m.start >= anchor.start),
            Direction::Next => matches.iter().position(|m| m.start >= anchor.end && *m != anchor),
            Direction::Previous => matches.iter().rposition(|m| m.start < anchor.start),
        };
        // Wrap around the ends of the document.
        let index = index.unwrap_or(match direction {
            Direction::Previous => matches.len() - 1,
            _ => 0,
        });
        self.select_match(ctx, matches[index].clone());
    }

    /// The editor's selection in the open document, as bytes.
    fn selection_bytes(&mut self, ctx: &egui::Context) -> Range<usize> {
        let Some(tab) = self.current() else { return 0..0 };
        let text = &tab.doc.text;
        editor::selection(ctx, tab.id)
            .map(|r| find::char_to_byte(text, r.start)..find::char_to_byte(text, r.end))
            .unwrap_or(0..0)
    }

    /// Select a match in the open document and scroll to it. Skipped if the
    /// text changed since the match was found and it no longer fits.
    fn select_match(&mut self, ctx: &egui::Context, range: Range<usize>) {
        let Some(tab) = self.current() else { return };
        if tab.doc.text.get(range.clone()).is_none() {
            return;
        }
        let start = find::byte_to_char(&tab.doc.text, range.start);
        let end = find::byte_to_char(&tab.doc.text, range.end);
        editor::set_selection(ctx, tab.id, start..end);
        self.reveal = Some(start);
        self.find.current = Some(range);
    }

    /// Search every document outside the Trash, in binder order. Open ones
    /// are searched as they are in the editor, the rest as saved.
    fn search_project(&mut self) {
        self.results.clear();
        self.results_error = None;
        let Some(project) = &self.project else { return };
        for node in project.documents() {
            let open = self.open.get(&node.id).and_then(|&tab| self.docs.get(tab));
            let text = match open {
                Some(tab) => tab.doc.text.clone(),
                None => match project.file_path(&node.id).map(fs::read) {
                    Some(Ok(bytes)) => String::from_utf8_lossy(&bytes).into_owned(),
                    // Missing or unreadable: nothing to find in it.
                    _ => continue,
                },
            };
            match find::find_all(&text, &self.find.query, self.find.options) {
                Ok(matches) if matches.is_empty() => {}
                Ok(matches) => self.results.push(DocResults {
                    id: node.id.clone(),
                    title: node.title.clone(),
                    rows: search_results::build_rows(&text, &matches),
                }),
                Err(e) => {
                    self.results_error = Some(e);
                    break;
                }
            }
        }
    }

    pub(super) fn find_bar(&mut self, ui: &mut egui::Ui) {
        if !self.find.visible {
            return;
        }
        let matches = self.current_matches();
        let current_index = match (&matches, &self.find.current) {
            (Ok(m), Some(current)) => m.iter().position(|r| r == current),
            _ => None,
        };
        let error = matches.as_ref().err().cloned();
        let info = FindBarInfo {
            match_count: matches.as_ref().map_or(0, Vec::len),
            current_index,
            error: error.as_deref(),
            history: &[],
            results_open: self.show_results,
            results_label: "Whole Project",
            results_hint: "List matches in every document in the project (\u{21e7}\u{2318}F)",
            allow_replace: false,
        };
        let action = egui::Panel::top(egui::Id::unique("find"))
            .show(ui, |ui| find::find_bar(ui, &mut self.find, info))
            .inner;
        if let Some(action) = action {
            self.find_action(&ui.ctx().clone(), action);
        }
    }

    pub(super) fn results_panel(&mut self, ui: &mut egui::Ui) {
        if !self.show_results || !self.find.is_searching() {
            return;
        }
        let now = ui.input(|i| i.time);
        let key = (self.find.query.clone(), self.find.options);
        if self.results_key.as_ref() != Some(&key) || now - self.results_at >= REFRESH_SECONDS {
            self.search_project();
            self.results_key = Some(key);
            self.results_at = now;
        }
        ui.ctx().request_repaint_after(Duration::from_secs_f64(REFRESH_SECONDS));

        let results: Vec<TabResults<'_, String>> = self
            .results
            .iter()
            .map(|r| TabResults { key: r.id.clone(), name: r.title.clone(), rows: &r.rows })
            .collect();
        let error = self.results_error.as_deref();
        let action = egui::Panel::bottom(egui::Id::unique("search-results"))
            .resizable(true)
            .default_size(200.0)
            .show(ui, |ui| search_results::results_panel(ui, &results, error, &WORDING))
            .inner;
        match action {
            Some(ResultsAction::Jump(id, range)) => {
                if self.selected.as_deref() == Some(id.as_str()) {
                    self.focus_editor = true;
                } else {
                    self.select(id);
                }
                self.select_match(&ui.ctx().clone(), range);
            }
            Some(ResultsAction::Close) => self.show_results = false,
            None => {}
        }
    }
}
