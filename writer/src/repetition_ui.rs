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
    most_used: Option<MostUsed>,
}

/// The most used words as last gathered: when, for which common words.
struct MostUsed {
    at: f64,
    common: String,
    list: Vec<(String, usize)>,
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
        let common = self.settings.common_words.clone();
        let tint = visuals.hyperlink_color.gamma_multiply(if visuals.dark_mode { 0.28 } else { 0.18 });
        let Some(text) = self.current().map(|t| t.doc.text.clone()) else { return Vec::new() };
        let mut hasher = DefaultHasher::new();
        (&text, &watch, &common, tint).hash(&mut hasher);
        let key = hasher.finish();
        if self.repetition.key == key {
            return self.repetition.marks.clone();
        }
        let found = repetition::find(&text, &watch_list(&watch), &repetition::Common::new(&watch_list(&common)));
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
        // Gathered again now and then, and as soon as the common words change.
        if let Some(m) = &self.repetition.most_used
            && now - m.at < REFRESH_SECONDS
            && m.common == self.settings.common_words
        {
            return m.list.clone();
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
        let common = repetition::Common::new(&watch_list(&self.settings.common_words));
        let list = repetition::most_used(texts.iter().map(String::as_str), 16, &common);
        self.repetition.most_used = Some(MostUsed { at: now, common: self.settings.common_words.clone(), list: list.clone() });
        list
    }

    /// Add a word to your common words, or take it off.
    pub(super) fn set_common(&mut self, word: &str, common: bool) {
        let mut words = watch_list(&self.settings.common_words);
        words.retain(|w| !w.eq_ignore_ascii_case(word));
        if common {
            words.push(word.to_owned());
        }
        self.settings.common_words = words.join(", ");
    }
}

/// What was done in the most used words.
pub(super) enum MostUsedAction {
    /// Find it in the project.
    Find(String),
    /// Make it one of your common words (true), or not (false).
    Common(String, bool),
}

/// The most used words, as small buttons with their counts (right-click
/// one to leave it out), and your own common words, to bring one back.
pub(super) fn most_used_section(ui: &mut egui::Ui, words: &[(String, usize)], common: &[String]) -> Option<MostUsedAction> {
    if words.is_empty() && common.is_empty() {
        return None;
    }
    let mut action = None;
    ui.add_space(8.0);
    ui.separator();
    ui.strong("Most used words");
    ui.weak("In the manuscript, leaving out the common little ones. Click one to find it; right-click to leave it out.");
    ui.horizontal_wrapped(|ui| {
        for (word, count) in words {
            let text = RichText::new(format!("{word}  {count}"));
            let button = ui.button(text).on_hover_text(format!("Find \u{201c}{word}\u{201d} in the project (right-click to leave it out)"));
            if button.clicked() {
                action = Some(MostUsedAction::Find(word.clone()));
            }
            button.context_menu(|ui| {
                if ui.button("Leave Out").on_hover_text("Add it to your common words (Settings \u{2192} Writing)").clicked() {
                    action = Some(MostUsedAction::Common(word.clone(), true));
                    ui.close();
                }
            });
        }
    });
    if !common.is_empty() {
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.weak("Left out:");
            for word in common {
                let text = RichText::new(format!("{word} \u{d7}")).color(ui.visuals().weak_text_color());
                if ui.add(egui::Button::new(text)).on_hover_text("Count it again").clicked() {
                    action = Some(MostUsedAction::Common(word.clone(), false));
                }
            }
        });
    }
    action
}
