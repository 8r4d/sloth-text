//! The Compile window (⇧⌘E): choose the author name, format and chapter
//! headings, then save the manuscript. The choices are kept in the project.

use std::fs;
use std::path::Path;

use eframe::egui;

use super::WriterApp;
use crate::compile::{self, ChapterHeadings, Citations, CompileSettings, Format, Manuscript, Paper, SceneBreak, Typeface};

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
        let naming = &mut self.compile_preset_name;
        let mut open = true;
        let mut go = false;
        egui::Window::new("Compile")
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                preset_row(ui, settings, naming);
                ui.separator();
                egui::Grid::new("compile-grid").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
                    ui.label("Author");
                    ui.add(egui::TextEdit::singleline(&mut settings.author).hint_text("Your name as it should appear"));
                    ui.end_row();

                    ui.label("Contact");
                    ui.add(
                        egui::TextEdit::multiline(&mut settings.contact)
                            .hint_text("Address, email, phone, agent\u{2026} (one per line)")
                            .desired_rows(3),
                    );
                    ui.end_row();

                    ui.label("Format");
                    ui.vertical(|ui| {
                        ui.radio_value(&mut settings.format, Format::Manuscript, "Word manuscript (.docx)");
                        ui.radio_value(&mut settings.format, Format::Epub, "E-book (.epub)");
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

                    ui.label("Scene breaks");
                    ui.horizontal(|ui| {
                        // Named, not shown: the interface font has no asterism (⁂),
                        // though the manuscript's and e-book's fonts do.
                        ui.radio_value(&mut settings.scene_break, SceneBreak::Hash, "#");
                        ui.radio_value(&mut settings.scene_break, SceneBreak::Stars, "* * *");
                        ui.radio_value(&mut settings.scene_break, SceneBreak::Asterism, "Asterism");
                    });
                    ui.end_row();

                    // [[References]] to profiles with a Citation section.
                    ui.label("Sources");
                    ui.vertical(|ui| {
                        let as_notes = match settings.format {
                            Format::Manuscript => "As footnotes",
                            Format::Epub => "As notes at the end of each chapter",
                            Format::Markdown => "As footnotes (pandoc)",
                        };
                        ui.radio_value(&mut settings.citations, Citations::LeaveOut, "Leave out");
                        ui.radio_value(&mut settings.citations, Citations::Notes, as_notes);
                        ui.checkbox(&mut settings.works_cited, "List the works cited at the end");
                    })
                    .response
                    .on_hover_text("From [[references]] to profiles with a Citation section, such as Sources. The note is that section, with any page given after | in the reference.");
                    ui.end_row();

                    if settings.format == Format::Manuscript {
                        ui.label("Paper");
                        ui.horizontal(|ui| {
                            ui.radio_value(&mut settings.paper, Paper::Letter, "US Letter");
                            ui.radio_value(&mut settings.paper, Paper::A4, "A4");
                        });
                        ui.end_row();

                        ui.label("Font");
                        ui.horizontal(|ui| {
                            ui.radio_value(&mut settings.typeface, Typeface::Times, "Times New Roman");
                            ui.radio_value(&mut settings.typeface, Typeface::Courier, "Courier New");
                        });
                        ui.end_row();
                    }
                });

                ui.add_space(8.0);
                let chapters = manuscript.chapters.len();
                let sources = match (manuscript.notes.len(), manuscript.works_cited.len()) {
                    (0, 0) => String::new(),
                    (0, w) => format!(" {w} {} cited.", if w == 1 { "work" } else { "works" }),
                    (n, 0) => format!(" {n} {}.", if n == 1 { "note" } else { "notes" }),
                    (n, w) => format!(" {n} {}, {w} {} cited.", if n == 1 { "note" } else { "notes" }, if w == 1 { "work" } else { "works" }),
                };
                ui.weak(format!(
                    "{chapters} {}, {} words.{sources} Items marked not to be included, the Trash, notes in <!-- --> and [[references]] are left out.",
                    if chapters == 1 { "chapter" } else { "chapters" },
                    manuscript.words(),
                ));
                ui.weak(match settings.format {
                    Format::Manuscript => "Standard manuscript format: 12 pt, double spaced, a title page, each chapter on a new page.",
                    Format::Epub => "An e-book for Apple Books, Kindle (through Send to Kindle) and other readers.",
                    Format::Markdown => "The text as Markdown, ready for pandoc or any other tool.",
                });
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
        let settings = project.manifest.compile.clone();
        match write_compiled(manuscript, &settings, &path) {
            Ok(()) => {
                self.compile_open = false;
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                self.set_status(format!("Compiled {} words to {name}", manuscript.words()), now);
            }
            Err(e) => self.error = Some(format!("Couldn't compile: {e}")),
        }
    }
}

pub(super) fn write_compiled(manuscript: &Manuscript, settings: &CompileSettings, path: &Path) -> Result<(), String> {
    let bytes = match settings.format {
        Format::Manuscript => compile::to_docx(manuscript, settings)?,
        Format::Epub => compile::to_epub(manuscript, settings)?,
        Format::Markdown => compile::to_markdown(manuscript, settings).into_bytes(),
    };
    sloth_core::document::write_atomically(path, &bytes).map_err(|e| e.to_string())
}

/// The presets: which one the choices match (or Custom), picking one,
/// saving the choices as a new one, and deleting one.
fn preset_row(ui: &mut egui::Ui, settings: &mut CompileSettings, naming: &mut Option<String>) {
    let current = settings.presets.iter().position(|p| p.matches(settings));
    ui.horizontal(|ui| {
        ui.label("Preset");
        let shown = current.map_or("Custom", |i| settings.presets[i].name.as_str()).to_owned();
        let mut pick = None;
        egui::ComboBox::from_id_salt("compile-preset").selected_text(shown).show_ui(ui, |ui| {
            if settings.presets.is_empty() {
                ui.weak("None saved yet.");
            }
            for (i, preset) in settings.presets.iter().enumerate() {
                if ui.selectable_label(current == Some(i), &preset.name).clicked() {
                    pick = Some(i);
                }
            }
        });
        if let Some(i) = pick {
            let preset = settings.presets[i].clone();
            preset.apply(settings);
        }
        if let Some(i) = current
            && ui.small_button("Delete").on_hover_text("Delete this preset (your choices stay)").clicked()
        {
            settings.presets.remove(i);
        }
        if naming.is_none() && ui.small_button("Save as Preset\u{2026}").clicked() {
            *naming = Some(String::new());
        }
    });
    if let Some(name) = naming {
        let mut done = false;
        ui.horizontal(|ui| {
            let field = ui.add(egui::TextEdit::singleline(name).hint_text("Agent submission").desired_width(180.0));
            field.request_focus();
            let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (ui.button("Save").clicked() || enter) && !name.trim().is_empty() {
                let preset = compile::Preset::from_settings(name.trim(), settings);
                match settings.presets.iter_mut().find(|p| p.name == preset.name) {
                    Some(existing) => *existing = preset,
                    None => settings.presets.push(preset),
                }
                done = true;
            }
            if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                done = true;
            }
        });
        if done {
            *naming = None;
        }
    }
}
