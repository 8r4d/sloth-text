//! The glossary in the app: Add to Glossary (from the editor's right-click
//! menu) and its box for the definition, terms marked in a document while
//! asked to (the inspector's Glossary Terms), their definitions on hover,
//! and Sort A–Z. The glossary itself is a document (see glossary.rs).

use std::ops::Range;

use eframe::egui::{self, Color32, RichText};
use sloth_core::editor::{CursorInfo, Mark};

use super::WriterApp;
use crate::glossary::Glossary;

/// A term being added or changed in the box.
pub(super) struct GlossaryEdit {
    pub term: String,
    pub definition: String,
    /// It's in the glossary already (the box changes it).
    pub existed: bool,
    /// The definition field has had the cursor put in it.
    pub focused: bool,
}

/// Terms found in the document being marked, kept until its text changes.
#[derive(Default)]
pub(super) struct GlossaryMarks {
    key: u64,
    /// Each term's place in the text, with its term and definition.
    found: Vec<(Range<usize>, String, String)>,
}

/// The underline under glossary terms: a soft teal.
const TERM_LINE: Color32 = Color32::from_rgb(60, 160, 150);

impl WriterApp {
    /// The glossary as it stands (open in the editor, or as saved).
    pub(super) fn read_glossary(&self) -> Option<(String, Glossary)> {
        let id = self.project.as_ref()?.glossary()?.id.clone();
        let text = self.document_text(&id)?;
        Some((id, Glossary::parse(&text)))
    }

    /// Add to Glossary… was chosen on these words: open the box, with the
    /// definition there already if the term is.
    pub(super) fn start_glossary_add(&mut self, words: String) {
        let term = words.trim().trim_matches(|c: char| c.is_ascii_punctuation() && c != '\'').to_owned();
        if term.is_empty() {
            return;
        }
        let existing = self.read_glossary().and_then(|(_, g)| g.find(&term).cloned());
        self.glossary_edit = Some(match existing {
            Some(entry) => GlossaryEdit { term: entry.term, definition: entry.definition, existed: true, focused: false },
            None => GlossaryEdit { term, definition: String::new(), existed: false, focused: false },
        });
    }

    /// The box for a term's definition.
    pub(super) fn glossary_window(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        let Some(edit) = &mut self.glossary_edit else { return };
        let (mut save, mut cancel) = (false, false);
        let mut open = true;
        egui::Window::new(if edit.existed { "Glossary" } else { "Add to Glossary" })
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(360.0)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .show(ctx, |ui| {
                ui.add(egui::TextEdit::singleline(&mut edit.term).font(egui::FontId::proportional(16.0)).desired_width(f32::INFINITY));
                ui.add_space(4.0);
                let field = ui.add(
                    egui::TextEdit::multiline(&mut edit.definition)
                        .hint_text("What it means")
                        .desired_rows(4)
                        .desired_width(f32::INFINITY),
                );
                // Straight to the definition (the term's usually right).
                if !edit.focused {
                    field.request_focus();
                    edit.focused = true;
                }
                if edit.existed {
                    ui.weak("It's in the glossary already: this changes it.");
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let ready = !edit.term.trim().is_empty();
                    let command_enter = ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter));
                    if ui.add_enabled(ready, egui::Button::new(if edit.existed { "Save" } else { "Add" })).clicked() || (ready && command_enter) {
                        save = true;
                    }
                    if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        cancel = true;
                    }
                });
            });
        if save {
            let edit = self.glossary_edit.take().expect("checked above");
            self.save_glossary_entry(&edit.term, &edit.definition, now);
        } else if cancel || !open {
            self.glossary_edit = None;
        }
    }

    /// Put a term and its definition in the glossary (made if need be),
    /// in its A–Z place.
    fn save_glossary_entry(&mut self, term: &str, definition: &str, now: f64) {
        let Some(project) = self.project.as_mut() else { return };
        let id = match project.make_glossary() {
            Ok(id) => id,
            Err(e) => {
                self.error = Some(format!("Couldn't make the glossary: {e}"));
                return;
            }
        };
        let mut glossary = Glossary::parse(&self.document_text(&id).unwrap_or_default());
        glossary.set(term, definition);
        self.apply_rewrites(vec![(id, glossary.to_text())]);
        self.glossary_marks = GlossaryMarks::default();
        self.ignore_glossary_spellings();
        self.set_status(format!("Added \u{201c}{}\u{201d} to the glossary", term.trim()), now);
    }

    /// The glossary's terms are words you mean: the spell checker leaves
    /// them be (for as long as Writer's open).
    pub(super) fn ignore_glossary_spellings(&self) {
        let Some((_, glossary)) = self.read_glossary() else { return };
        for entry in &glossary.entries {
            for word in entry.term.split_whitespace() {
                let word = word.trim_matches(|c: char| !c.is_alphanumeric());
                if !word.is_empty() && !sloth_core::spell::misspelled(word).is_empty() {
                    sloth_core::spell::ignore(word);
                }
            }
        }
    }

    /// Sort A–Z: the glossary's terms in order.
    pub(super) fn sort_glossary(&mut self) {
        let Some((id, mut glossary)) = self.read_glossary() else { return };
        glossary.sort();
        self.apply_rewrites(vec![(id, glossary.to_text())]);
    }

    /// The glossary's terms in the open document, marked while Glossary
    /// Terms is on for it.
    pub(super) fn glossary_term_marks(&mut self) -> Vec<Mark> {
        if self.glossary_terms_in.is_none() || self.glossary_terms_in != self.selected {
            return Vec::new();
        }
        let Some(text) = self.current().map(|t| t.doc.text.clone()) else { return Vec::new() };
        let glossary = self.read_glossary().map(|(_, g)| g).unwrap_or_default();
        let key = {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            (&text, glossary.to_text()).hash(&mut hasher);
            hasher.finish()
        };
        if self.glossary_marks.key != key {
            let found = crate::glossary::occurrences(&text, &glossary)
                .into_iter()
                .map(|(r, i)| (r, glossary.entries[i].term.clone(), glossary.entries[i].definition.clone()))
                .collect();
            self.glossary_marks = GlossaryMarks { key, found };
        }
        self.glossary_marks.found.iter().map(|(r, ..)| Mark { range: r.clone(), background: Color32::TRANSPARENT, underline: TERM_LINE }).collect()
    }

    /// A marked term's definition, beside the pointer resting on it.
    pub(super) fn glossary_tooltip(&self, ui: &egui::Ui, info: &CursorInfo) {
        if self.glossary_terms_in.is_none() || self.glossary_terms_in != self.selected {
            return;
        }
        let Some(at) = info.hovered_byte else { return };
        let Some((_, term, definition)) = self.glossary_marks.found.iter().find(|(r, ..)| r.contains(&at)) else { return };
        egui::Tooltip::always_open(ui.ctx().clone(), ui.layer_id(), egui::Id::unique("glossary-tooltip"), egui::PopupAnchor::Pointer).show(|ui| {
            ui.set_max_width(320.0);
            ui.label(RichText::new(term).strong());
            if definition.is_empty() {
                ui.weak("No definition yet.");
            } else {
                ui.label(definition.as_str());
            }
        });
    }

    #[cfg(test)]
    pub(super) fn glossary_marks_found(&self) -> Vec<Range<usize>> {
        self.glossary_marks.found.iter().map(|(r, ..)| r.clone()).collect()
    }
}
