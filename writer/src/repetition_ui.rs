//! Overused words in the window (see repetition.rs): echoes tinted and
//! watch-list words underlined in the open document (⌥⌘R), and the
//! manuscript's most used words in Statistics.

use std::hash::{DefaultHasher, Hash, Hasher};

use eframe::egui::{self, Color32, RichText};
use sloth_core::editor::Mark;

use super::WriterApp;
use crate::repetition;

/// The watch list's underline.
const WATCHED: Color32 = Color32::from_rgb(0xd0, 0x7a, 0x45);
/// Most used words are gathered again this often while Statistics is open.
const REFRESH_SECONDS: f64 = 5.0;

/// Marks for the open document, as last worked out: for which text (and
/// list), and the marks.
#[derive(Default)]
pub(super) struct RepetitionCache {
    key: u64,
    marks: Vec<Mark>,
    most_used: Option<(f64, Vec<(String, usize)>)>,
}

/// The watch list as typed in Settings: words or phrases, commas between.
pub(super) fn watch_list(typed: &str) -> Vec<String> {
    typed.split(',').map(str::trim).filter(|w| !w.is_empty()).map(str::to_owned).collect()
}

impl WriterApp {
    pub(super) fn toggle_repetition(&mut self) {
        self.settings.show_repetition = !self.settings.show_repetition;
    }

    /// The open document's marks: echoes tinted in the theme's accent,
    /// watched words underlined. None unless they're switched on.
    pub(super) fn repetition_marks(&mut self, visuals: &egui::Visuals) -> Vec<Mark> {
        if !self.settings.show_repetition {
            return Vec::new();
        }
        let watch = self.settings.watch_words.clone();
        let tint = visuals.hyperlink_color.gamma_multiply(if visuals.dark_mode { 0.28 } else { 0.18 });
        let Some(text) = self.current().map(|t| t.doc.text.clone()) else { return Vec::new() };
        let mut hasher = DefaultHasher::new();
        (&text, &watch, tint).hash(&mut hasher);
        let key = hasher.finish();
        if self.repetition.key == key {
            return self.repetition.marks.clone();
        }
        let found = repetition::find(&text, &watch_list(&watch));
        // A watched phrase wins over echoes inside it; one mark each.
        let mut marks: Vec<Mark> = found
            .watched
            .iter()
            .map(|r| Mark { range: r.clone(), background: Color32::TRANSPARENT, underline: WATCHED })
            .collect();
        for r in found.echoes {
            if let Some(i) = marks.iter().position(|m| m.range == r) {
                marks[i].background = tint;
            } else if !marks.iter().any(|m| m.range.start < r.end && r.start < m.range.end) {
                marks.push(Mark { range: r, background: tint, underline: Color32::TRANSPARENT });
            }
        }
        marks.sort_by_key(|m| m.range.start);
        self.repetition = RepetitionCache { key, marks: marks.clone(), most_used: self.repetition.most_used.take() };
        marks
    }

    /// The manuscript's most used words, gathered now and then.
    pub(super) fn most_used_words(&mut self, now: f64) -> Vec<(String, usize)> {
        if let Some((at, list)) = &self.repetition.most_used
            && now - at < REFRESH_SECONDS
        {
            return list.clone();
        }
        let Some(project) = &self.project else { return Vec::new() };
        let ids: Vec<String> = project
            .manifest
            .binder
            .iter()
            .filter(|n| n.role == Some(crate::project::Role::Manuscript))
            .flat_map(|top| project.documents_under(&top.id))
            .map(|n| n.id.clone())
            .collect();
        let texts: Vec<String> = ids.iter().filter_map(|id| self.document_text(id)).collect();
        let list = repetition::most_used(texts.iter().map(String::as_str), 16);
        self.repetition.most_used = Some((now, list.clone()));
        list
    }
}

/// The most used words, as small buttons with their counts. Returns the one
/// clicked (to find it in the project).
pub(super) fn most_used_section(ui: &mut egui::Ui, words: &[(String, usize)]) -> Option<String> {
    if words.is_empty() {
        return None;
    }
    let mut picked = None;
    ui.add_space(8.0);
    ui.separator();
    ui.strong("Most used words");
    ui.weak("In the manuscript, leaving out the common little ones. Click one to find it.");
    ui.horizontal_wrapped(|ui| {
        for (word, count) in words {
            let text = RichText::new(format!("{word}  {count}"));
            if ui.button(text).on_hover_text(format!("Find \u{201c}{word}\u{201d} in the project")).clicked() {
                picked = Some(word.clone());
            }
        }
    });
    picked
}
