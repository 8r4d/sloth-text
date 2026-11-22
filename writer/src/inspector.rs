//! The inspector: a panel on the right (⌥⌘I) for the selected item's title,
//! synopsis, status, label, keywords, word target and notes, plus a small
//! window for editing the project's labels.
//!
//! Edits change the manifest in memory and mark it unsaved; the app writes
//! it with its regular autosave (see `WriterApp::save_all`), so typing in
//! the synopsis doesn't rewrite project.ron on every keystroke.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

use eframe::egui::{self, Color32, RichText};

use super::WriterApp;
use crate::binder::BinderAction;
use crate::project::Role;

/// Label colours, by the index stored in the manifest. Mid tones, so they
/// read on both light and dark backgrounds.
pub const LABEL_COLORS: [Color32; 8] = [
    Color32::from_rgb(0xE5, 0x48, 0x4D), // red
    Color32::from_rgb(0xF7, 0x6B, 0x15), // orange
    Color32::from_rgb(0xE8, 0xB0, 0x2E), // yellow
    Color32::from_rgb(0x46, 0xA7, 0x58), // green
    Color32::from_rgb(0x12, 0xA5, 0x94), // teal
    Color32::from_rgb(0x00, 0x90, 0xFF), // blue
    Color32::from_rgb(0x8E, 0x4E, 0xC6), // purple
    Color32::from_rgb(0xD6, 0x40, 0x9F), // pink
];

pub fn label_color(index: u8) -> Color32 {
    LABEL_COLORS[index as usize % LABEL_COLORS.len()]
}

/// Text fields whose text isn't stored as typed: the title (renamed when
/// you leave the field) and the keywords (stored as a list).
pub(super) struct Buffers {
    id: String,
    title: String,
    keywords: String,
}

/// Word counts of documents that aren't open, by file, remembered until
/// the file changes.
#[derive(Default)]
pub(super) struct WordCache(HashMap<PathBuf, (Option<SystemTime>, u64, usize)>);

impl WordCache {
    fn count(&mut self, path: PathBuf) -> usize {
        let Ok(stat) = fs::metadata(&path) else { return 0 };
        let stamp = (stat.modified().ok(), stat.len());
        if let Some(&(modified, len, words)) = self.0.get(&path)
            && (modified, len) == stamp
        {
            return words;
        }
        let words = fs::read(&path).map(|b| count_words(&String::from_utf8_lossy(&b))).unwrap_or(0);
        self.0.insert(path, (stamp.0, stamp.1, words));
        words
    }
}

/// Words as targets and compile count them: inline notes (`<!-- … -->`)
/// aren't part of the manuscript, so they don't count.
pub fn count_words(text: &str) -> usize {
    if text.contains("<!--") {
        crate::compile::strip_notes(text).split_whitespace().count()
    } else {
        text.split_whitespace().count()
    }
}

impl WriterApp {
    /// Words in an item: its own text and everything under it. Open
    /// documents are counted as they are in the editor.
    pub(super) fn words_under(&mut self, id: &str) -> usize {
        let Some(project) = &self.project else { return 0 };
        let mut total = 0;
        for doc in project.documents_under(id) {
            let open = self.open.get(&doc.id).and_then(|&tab| self.docs.get(tab));
            total += match open {
                Some(tab) => count_words(&tab.doc.text),
                None => project.file_path(&doc.id).map_or(0, |path| self.word_cache.count(path)),
            };
        }
        total
    }

    pub(super) fn inspector(&mut self, ui: &mut egui::Ui) {
        if !self.show_inspector {
            return;
        }
        egui::Panel::right(egui::Id::unique("inspector"))
            .resizable(true)
            .default_size(280.0)
            .size_range(220.0..=480.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.add_space(6.0);
                    self.inspector_contents(ui);
                });
            });
    }

    fn inspector_contents(&mut self, ui: &mut egui::Ui) {
        let Some(id) = self.selected.clone() else {
            ui.weak("Select an item in the binder.");
            return;
        };
        let Some(project) = &self.project else { return };
        let Some(node) = project.node(&id) else { return };
        let (title, role, mut meta) = (node.title.clone(), node.role, node.meta.clone());
        let is_document = node.is_document();
        let in_manuscript = project.role_of(&id) == Some(Role::Manuscript);
        let statuses = project.manifest.statuses.clone();
        let labels = project.manifest.labels.clone();

        // Start the text buffers afresh for a new selection.
        if self.inspector_buffers.as_ref().is_none_or(|b| b.id != id) {
            self.inspector_buffers = Some(Buffers {
                id: id.clone(),
                title: title.clone(),
                keywords: meta.keywords.join(", "),
            });
        }

        // Title.
        heading(ui, "Title");
        if role.is_some() {
            ui.strong(&title);
        } else {
            let buffers = self.inspector_buffers.as_mut().expect("set above");
            let field = ui.add(egui::TextEdit::singleline(&mut buffers.title).desired_width(f32::INFINITY));
            if field.lost_focus() {
                let typed = buffers.title.clone();
                if typed.trim() != title {
                    self.apply(BinderAction::FinishRename { id: id.clone(), title: typed });
                }
            } else if !field.has_focus() && buffers.title != title {
                // Renamed elsewhere (the binder): show the new title.
                buffers.title = title.clone();
            }
        }

        // Words and target.
        ui.add_space(10.0);
        heading(ui, if role == Some(Role::Manuscript) { "Words in the manuscript" } else { "Words" });
        let words = self.words_under(&id);
        match meta.target_words {
            Some(target) if target > 0 => {
                let fraction = (words as f32 / target as f32).min(1.0);
                ui.add(egui::ProgressBar::new(fraction).text(format!("{words} of {target}")));
            }
            _ => {
                ui.label(format!("{words}"));
            }
        }
        ui.horizontal(|ui| {
            let mut has_target = meta.target_words.is_some();
            if ui.checkbox(&mut has_target, "Target").changed() {
                meta.target_words = has_target.then_some(if role == Some(Role::Manuscript) { 80_000 } else { 2_000 });
            }
            if let Some(target) = &mut meta.target_words {
                ui.add(egui::DragValue::new(target).speed(50).range(1..=10_000_000).suffix(" words"));
            }
        });

        if role.is_none() {
            // Synopsis.
            ui.add_space(10.0);
            heading(ui, "Synopsis");
            ui.add(
                egui::TextEdit::multiline(&mut meta.synopsis)
                    .hint_text("What happens here, in a sentence or two")
                    .desired_rows(4)
                    .desired_width(f32::INFINITY),
            );

            // Status and label.
            ui.add_space(10.0);
            egui::Grid::new("inspector-grid").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                ui.label("Status");
                let shown = meta.status.and_then(|i| statuses.get(i)).map_or("None", String::as_str);
                egui::ComboBox::from_id_salt("status").selected_text(shown).show_ui(ui, |ui| {
                    ui.selectable_value(&mut meta.status, None, "None");
                    for (i, name) in statuses.iter().enumerate() {
                        ui.selectable_value(&mut meta.status, Some(i), name);
                    }
                });
                ui.end_row();

                ui.label("Label");
                let shown = match meta.label.and_then(|i| labels.get(i)) {
                    Some(label) => RichText::new(format!("\u{25cf} {}", label.name)).color(label_color(label.color)),
                    None => RichText::new("None"),
                };
                egui::ComboBox::from_id_salt("label").selected_text(shown).show_ui(ui, |ui| {
                    ui.selectable_value(&mut meta.label, None, "None");
                    for (i, label) in labels.iter().enumerate() {
                        let text = RichText::new(format!("\u{25cf} {}", label.name)).color(label_color(label.color));
                        ui.selectable_value(&mut meta.label, Some(i), text);
                    }
                    ui.separator();
                    if ui.button("Edit Labels\u{2026}").clicked() {
                        self.editing_labels = true;
                    }
                });
                ui.end_row();
            });

            // Keywords.
            ui.add_space(10.0);
            heading(ui, "Keywords");
            let buffers = self.inspector_buffers.as_mut().expect("set above");
            let field = ui.add(
                egui::TextEdit::singleline(&mut buffers.keywords)
                    .hint_text("Separated by commas")
                    .desired_width(f32::INFINITY),
            );
            if field.changed() {
                meta.keywords = buffers
                    .keywords
                    .split(',')
                    .map(str::trim)
                    .filter(|k| !k.is_empty())
                    .map(str::to_owned)
                    .collect();
            }

            if in_manuscript {
                ui.add_space(6.0);
                let mut include = !meta.exclude_from_compile;
                if ui.checkbox(&mut include, "Include when compiling").changed() {
                    meta.exclude_from_compile = !include;
                }
            }

            // Notes.
            ui.add_space(10.0);
            heading(ui, "Notes");
            ui.add(
                egui::TextEdit::multiline(&mut meta.notes)
                    .hint_text("Anything to remember about it")
                    .desired_rows(6)
                    .desired_width(f32::INFINITY),
            );
        }

        if let Some(project) = self.project.as_mut()
            && project.set_meta(&id, meta)
        {
            self.manifest_dirty = true;
        }

        if is_document {
            self.snapshot_section(ui, &id);
        }
    }

    /// The Labels window: rename, recolour, add and remove labels.
    pub(super) fn labels_window(&mut self, ctx: &egui::Context) {
        if !self.editing_labels {
            return;
        }
        let Some(project) = self.project.as_mut() else { return };
        let mut open = true;
        let mut changed = false;
        let mut remove = None;
        egui::Window::new("Labels")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(300.0)
            .show(ctx, |ui| {
                for (i, label) in project.manifest.labels.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        // Click the dot to try the next colour.
                        let dot = RichText::new("\u{25cf}").size(18.0).color(label_color(label.color));
                        if ui.button(dot).on_hover_text("Change colour").clicked() {
                            label.color = (label.color + 1) % LABEL_COLORS.len() as u8;
                            changed = true;
                        }
                        changed |= ui.add(egui::TextEdit::singleline(&mut label.name).desired_width(180.0)).changed();
                        if ui.button("Remove").clicked() {
                            remove = Some(i);
                        }
                    });
                }
                ui.add_space(6.0);
                if ui.button("Add Label").clicked() {
                    let color = (project.manifest.labels.len() % LABEL_COLORS.len()) as u8;
                    project.add_label("New label", color);
                    changed = true;
                }
            });
        if let Some(i) = remove {
            project.remove_label(i);
            changed = true;
        }
        if changed {
            self.manifest_dirty = true;
        }
        self.editing_labels = open;
    }
}

fn heading(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).small().weak());
}

#[cfg(test)]
mod tests {
    use super::count_words;

    #[test]
    fn notes_dont_count_as_words() {
        assert_eq!(count_words("one two three"), 3);
        assert_eq!(count_words("one <!-- not these four words --> two"), 2);
        assert_eq!(count_words("one <!-- an unfinished note"), 1);
    }
}
