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
use crate::project::{Meta, Role};

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
        let is_profile = project.profile_of(&id).is_some();
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

        // The title, and its words (against a target, if it has one).
        if role.is_some() {
            ui.label(RichText::new(&title).size(16.0).strong());
        } else {
            let buffers = self.inspector_buffers.as_mut().expect("set above");
            let field = ui.add(
                egui::TextEdit::singleline(&mut buffers.title)
                    .font(egui::FontId::proportional(16.0))
                    .desired_width(f32::INFINITY),
            );
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
        let words = self.words_under(&id);
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let what = if role == Some(Role::Manuscript) { "in the manuscript" } else { "" };
            let count = format!("{} {}", crate::stats::thousands(words as u32), if words == 1 { "word" } else { "words" });
            ui.label(if what.is_empty() { count } else { format!("{count} {what}") });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(target) = &mut meta.target_words {
                    ui.add(egui::DragValue::new(target).speed(50).range(1..=10_000_000));
                }
                let mut has_target = meta.target_words.is_some();
                if ui.checkbox(&mut has_target, "Target").changed() {
                    meta.target_words = has_target.then_some(if role == Some(Role::Manuscript) { 80_000 } else { 2_000 });
                }
            });
        });
        if let Some(target) = meta.target_words.filter(|&t| t > 0) {
            let fraction = (words as f32 / target as f32).min(1.0);
            ui.add(egui::ProgressBar::new(fraction).desired_height(6.0));
        }

        if role.is_none() {
            section(ui, "Synopsis", "synopsis", true, |ui| {
                // Two lines to start; it grows with the text, then scrolls.
                egui::ScrollArea::vertical().id_salt("synopsis-scroll").max_height(260.0).show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut meta.synopsis)
                            .hint_text("What happens here, in a sentence or two")
                            .desired_rows(2)
                            .desired_width(f32::INFINITY),
                    );
                });
            });

            let editing_labels = &mut self.editing_labels;
            let editing_statuses = &mut self.editing_statuses;
            let buffers = self.inspector_buffers.as_mut().expect("set above");
            section(ui, "Details", "details", true, |ui| {
                egui::Grid::new("inspector-grid").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                    ui.label("Status");
                    let shown = meta.status.and_then(|i| statuses.get(i)).map_or("None", String::as_str);
                    egui::ComboBox::from_id_salt("status").selected_text(shown).width(150.0).show_ui(ui, |ui| {
                        ui.selectable_value(&mut meta.status, None, "None");
                        for (i, name) in statuses.iter().enumerate() {
                            ui.selectable_value(&mut meta.status, Some(i), name);
                        }
                        ui.separator();
                        if ui.button("Edit Statuses\u{2026}").clicked() {
                            *editing_statuses = true;
                        }
                    });
                    ui.end_row();

                    ui.label("Label");
                    let shown = match meta.label.and_then(|i| labels.get(i)) {
                        Some(label) => RichText::new(format!("\u{25cf} {}", label.name)).color(label_color(label.color)),
                        None => RichText::new("None"),
                    };
                    egui::ComboBox::from_id_salt("label").selected_text(shown).width(150.0).show_ui(ui, |ui| {
                        ui.selectable_value(&mut meta.label, None, "None");
                        for (i, label) in labels.iter().enumerate() {
                            let text = RichText::new(format!("\u{25cf} {}", label.name)).color(label_color(label.color));
                            ui.selectable_value(&mut meta.label, Some(i), text);
                        }
                        ui.separator();
                        if ui.button("Edit Labels\u{2026}").clicked() {
                            *editing_labels = true;
                        }
                    });
                    ui.end_row();

                    ui.label("Keywords");
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
                    ui.end_row();
                });
                ui.add_space(4.0);
                if in_manuscript {
                    let mut include = !meta.exclude_from_compile;
                    if ui.checkbox(&mut include, "Include when compiling").changed() {
                        meta.exclude_from_compile = !include;
                    }
                }
            });
            // (Outside the closure above: it needs the whole app.)
            ui.indent("ipad", |ui| self.ipad_checkbox(ui, &id, &mut meta));

            // A document's notes are above its text; anything else's are here.
            if !is_document || is_profile {
                section(ui, "Notes", "notes", true, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut meta.notes)
                            .hint_text("Anything to remember about it")
                            .desired_rows(4)
                            .desired_width(f32::INFINITY),
                    );
                });
            }

            // Who and what appears here (not for the profiles themselves).
            if !is_profile && self.project.as_ref().is_some_and(|p| !p.manifest.profiles.is_empty()) {
                ui.add_space(4.0);
                egui::CollapsingHeader::new(RichText::new("In This Scene").strong())
                    .id_salt("in-this-scene")
                    .default_open(true)
                    .show(ui, |ui| self.scene_profiles(ui, &mut meta));
            }
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

    /// The Statuses window: rename, reorder, add and remove statuses.
    /// Every document keeps its status through a reorder; removing one
    /// leaves the documents that had it with none.
    pub(super) fn statuses_window(&mut self, ctx: &egui::Context) {
        if !self.editing_statuses {
            return;
        }
        let Some(project) = self.project.as_mut() else { return };
        let mut open = true;
        let mut changed = false;
        let (mut remove, mut swap) = (None, None);
        let count = project.manifest.statuses.len();
        let uses: Vec<usize> = (0..count).map(|i| project.status_uses(i)).collect();
        egui::Window::new("Statuses")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(340.0)
            .show(ctx, |ui| {
                ui.weak("In the order the Status menu offers them.");
                ui.add_space(4.0);
                for (i, status) in project.manifest.statuses.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        if ui.add_enabled(i > 0, egui::Button::new("\u{2191}").small()).on_hover_text("Move up").clicked() {
                            swap = Some(i - 1);
                        }
                        if ui.add_enabled(i + 1 < count, egui::Button::new("\u{2193}").small()).on_hover_text("Move down").clicked() {
                            swap = Some(i);
                        }
                        changed |= ui.add(egui::TextEdit::singleline(status).desired_width(170.0)).changed();
                        let hint = match uses[i] {
                            0 => "Nothing has this status".to_owned(),
                            1 => "1 item has this status: it will have none".to_owned(),
                            n => format!("{n} items have this status: they will have none"),
                        };
                        if ui.button("Remove").on_hover_text(hint).clicked() {
                            remove = Some(i);
                        }
                    });
                }
                ui.add_space(6.0);
                if ui.button("Add Status").clicked() {
                    project.add_status("New status");
                    changed = true;
                }
            });
        if let Some(i) = swap {
            project.swap_statuses(i);
            changed = true;
        }
        if let Some(i) = remove {
            project.remove_status(i);
            changed = true;
        }
        if changed {
            self.manifest_dirty = true;
        }
        self.editing_statuses = open;
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

impl WriterApp {
    /// A document's notes (never compiled), folded above its text in the
    /// text's column; open when there are any.
    pub(super) fn notes_panel(&mut self, ui: &mut egui::Ui, id: &str, column: Option<f32>) {
        let Some(project) = &self.project else { return };
        let Some(node) = project.node(id) else { return };
        let mut notes = node.meta.notes.clone();
        let width = column.map_or(ui.available_width(), |w| w.min(ui.available_width()));
        let first_line = notes.lines().find(|l| !l.trim().is_empty()).unwrap_or("").to_owned();
        let mut changed = false;
        let mut height = self.settings.notes_height;
        ui.horizontal(|ui| {
            ui.add_space(((ui.available_width() - width) / 2.0).max(0.0));
            ui.vertical(|ui| {
                ui.set_width(width);
                let header = egui::CollapsingHeader::new(RichText::new("Notes").strong())
                    .id_salt(("doc-notes", id))
                    .default_open(!notes.trim().is_empty());
                let shown = header.show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt(("doc-notes-scroll", id))
                        .min_scrolled_height(height)
                        .max_height(height)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            changed = ui
                                .add(
                                    egui::TextEdit::multiline(&mut notes)
                                        .hint_text("Notes about this document: research, reminders, what to fix. Not part of the compiled book.")
                                        .desired_rows(2)
                                        .desired_width(f32::INFINITY),
                                )
                                .changed();
                        });
                    // Drag the bottom edge for more or less of them.
                    let (handle, drag) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 8.0), egui::Sense::drag());
                    let hovered = drag.hovered() || drag.dragged();
                    if hovered {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
                    }
                    let color = if hovered { ui.visuals().strong_text_color() } else { ui.visuals().weak_text_color() };
                    let mid = handle.center();
                    ui.painter().line_segment([mid - egui::vec2(18.0, 0.0), mid + egui::vec2(18.0, 0.0)], egui::Stroke::new(2.0, color));
                    if drag.dragged() {
                        height = (height + drag.drag_delta().y).clamp(48.0, 900.0);
                    }
                    let _ = drag.on_hover_text("Drag to make the notes taller or shorter");
                });
                // Folded: a glimpse of what's in them.
                if shown.body_returned.is_none() && !first_line.is_empty() {
                    let glimpse = egui::Label::new(RichText::new(&first_line).small().weak()).truncate();
                    ui.add(glimpse);
                }
            });
        });
        ui.separator();
        self.settings.notes_height = height;
        if changed
            && let Some(project) = self.project.as_mut()
        {
            let meta = Meta { notes, ..project.node(id).map(|n| n.meta.clone()).unwrap_or_default() };
            if project.set_meta(id, meta) {
                self.manifest_dirty = true;
            }
        }
    }
}

/// A part of the inspector that can be folded away (remembered by `id`).
fn section(ui: &mut egui::Ui, title: &str, id: &str, open: bool, add: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(4.0);
    egui::CollapsingHeader::new(RichText::new(title).strong()).id_salt(id).default_open(open).show(ui, add);
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
