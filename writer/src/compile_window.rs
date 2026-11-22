//! The Compile window (⇧⌘E): choose the author name, format and chapter
//! headings, then save the manuscript. The choices are kept in the project.

use std::fs;
use std::path::Path;

use eframe::egui;

use super::WriterApp;
use crate::compile::{self, ChapterHeadings, Format, Manuscript};

impl WriterApp {
    /// The manuscript as it stands: open documents as they are in the
    /// editor (unsaved edits included), the rest from their files.
    pub(super) fn gather_manuscript(&self) -> Option<Manuscript> {
        let project = self.project.as_ref()?;
        let settings = &project.manifest.compile;
        Some(compile::gather(project, settings, |node| {
            match self.open.get(&node.id).and_then(|&tab| self.docs.get(tab)) {
                Some(tab) => tab.doc.text.clone(),
                None => project.file_path(&node.id).and_then(|p| fs::read_to_string(p).ok()).unwrap_or_default(),
            }
        }))
    }

    pub(super) fn compile_window(&mut self, ctx: &egui::Context) {
        if !self.compile_open {
            return;
        }
        let Some(manuscript) = self.gather_manuscript() else { return };
        let Some(project) = self.project.as_mut() else { return };
        let before = project.manifest.compile.clone();
        let settings = &mut project.manifest.compile;
        let mut open = true;
        let mut go = false;
        egui::Window::new("Compile")
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(360.0)
            .show(ctx, |ui| {
                egui::Grid::new("compile-grid").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
                    ui.label("Author");
                    ui.add(egui::TextEdit::singleline(&mut settings.author).hint_text("Your name as it should appear"));
                    ui.end_row();

                    ui.label("Format");
                    ui.vertical(|ui| {
                        ui.radio_value(&mut settings.format, Format::Manuscript, "Word manuscript (.docx)");
                        ui.radio_value(&mut settings.format, Format::Markdown, "Markdown (.md)");
                    });
                    ui.end_row();

                    ui.label("Chapters");
                    let shown = |h: ChapterHeadings| match h {
                        ChapterHeadings::Titles => "Their titles",
                        ChapterHeadings::Numbers => "Chapter 1, Chapter 2\u{2026}",
                        ChapterHeadings::NumbersAndTitles => "Chapter 1: title\u{2026}",
                    };
                    egui::ComboBox::from_id_salt("chapter-headings")
                        .selected_text(shown(settings.chapter_headings))
                        .show_ui(ui, |ui| {
                            for h in [ChapterHeadings::Titles, ChapterHeadings::Numbers, ChapterHeadings::NumbersAndTitles] {
                                ui.selectable_value(&mut settings.chapter_headings, h, shown(h));
                            }
                        });
                    ui.end_row();
                });

                ui.add_space(8.0);
                let chapters = manuscript.chapters.len();
                ui.weak(format!(
                    "{chapters} {}, {} words. Items marked not to be included, the Trash, and notes in <!-- --> are left out.",
                    if chapters == 1 { "chapter" } else { "chapters" },
                    manuscript.words(),
                ));
                if settings.format == Format::Manuscript {
                    ui.weak("Standard manuscript format: Times New Roman 12 pt, double spaced, a title page, each chapter on a new page.");
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.add_enabled(chapters > 0, egui::Button::new("Compile\u{2026}")).clicked() {
                        go = true;
                    }
                    if ui.button("Cancel").clicked() {
                        self.compile_open = false;
                    }
                });
            });
        if project.manifest.compile != before {
            self.manifest_dirty = true;
        }
        if !open {
            self.compile_open = false;
        }
        if go {
            // Gathered again: the choices above may have changed the headings.
            if let Some(manuscript) = self.gather_manuscript() {
                self.save_compiled(&manuscript, ctx.input(|i| i.time));
            }
        }
    }

    /// Ask where to save, then write the file.
    fn save_compiled(&mut self, manuscript: &Manuscript, now: f64) {
        let Some(project) = &self.project else { return };
        let format = project.manifest.compile.format;
        let mut dialog = rfd::FileDialog::new()
            .set_title("Save Compiled Manuscript")
            .set_file_name(format!("{}.{}", manuscript.title, format.extension()));
        if let Some(parent) = project.root.parent() {
            dialog = dialog.set_directory(parent);
        }
        let Some(path) = dialog.save_file() else { return };
        match write_compiled(manuscript, format, &path) {
            Ok(()) => {
                self.compile_open = false;
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                self.set_status(format!("Compiled {} words to {name}", manuscript.words()), now);
            }
            Err(e) => self.error = Some(format!("Couldn't compile: {e}")),
        }
    }
}

pub(super) fn write_compiled(manuscript: &Manuscript, format: Format, path: &Path) -> Result<(), String> {
    let bytes = match format {
        Format::Manuscript => compile::to_docx(manuscript)?,
        Format::Markdown => compile::to_markdown(manuscript).into_bytes(),
    };
    sloth_core::document::write_atomically(path, &bytes).map_err(|e| e.to_string())
}
