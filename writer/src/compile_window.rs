//! The Compile window (⇧⌘E): choose the format, chapter headings, front
//! and back matter, then save the manuscript. The choices are kept in the
//! project; who you are comes from your profile in App Settings, unless the
//! project gives its own.

use std::path::Path;

use eframe::egui;

use super::WriterApp;
use crate::compile::{self, ChapterHeadings, Citations, CompileSettings, Format, Manuscript, Paper, SceneBreak, Typeface};
use crate::project::{Node, Role};

impl WriterApp {
    /// The manuscript as it stands: open documents as they are in the
    /// editor (unsaved edits included), the rest from their files.
    pub(super) fn gather_manuscript(&self) -> Option<Manuscript> {
        let project = self.project.as_ref()?;
        let settings = &project.manifest.compile;
        Some(compile::gather(project, settings, &self.settings.profile, |node: &Node| {
            match self.open.get(&node.id).and_then(|&tab| self.docs.get(tab)) {
                Some(tab) => tab.doc.text.clone(),
                None => project.file_path(&node.id).and_then(|p| crate::project::read_text(&p).ok()).unwrap_or_default(),
            }
        }))
    }

    pub(super) fn compile_window(&mut self, ctx: &egui::Context) {
        if !self.compile_open {
            return;
        }
        let Some(manuscript) = self.gather_manuscript() else { return };
        let profile = self.settings.profile.clone();
        let mut edit_profile = false;
        let Some(project) = self.project.as_mut() else { return };
        // Folders that can be front or back matter: any outside the
        // Manuscript and the Trash, by where they are.
        let folders = matter_folders(&project.manifest.binder);
        let before = project.manifest.compile.clone();
        let settings = &mut project.manifest.compile;
        let naming = &mut self.compile_preset_name;
        let mut open = true;
        let mut go = false;
        // Held by its top edge, so changing the format (which shows other
        // choices) doesn't move what's under the pointer.
        let top = ctx.content_rect().center_top() + egui::vec2(0.0, 40.0);
        egui::Window::new("Compile")
            .pivot(egui::Align2::CENTER_TOP)
            .default_pos(top)
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                preset_row(ui, settings, naming);
                ui.separator();
                egui::Grid::new("compile-grid").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
                    ui.label("Author");
                    ui.vertical(|ui| {
                        let from_profile = profile.shown_name();
                        let hint = if from_profile.is_empty() { "Your name as it should appear".to_owned() } else { from_profile.to_owned() };
                        ui.add(egui::TextEdit::singleline(&mut settings.author).hint_text(hint))
                            .on_hover_text("Leave empty to use your profile's name (App Settings \u{2192} You). Fill it in for a project under another name.");
                        ui.horizontal(|ui| {
                            ui.weak(if from_profile.is_empty() { "Not set in your profile." } else { "From your profile unless filled in." });
                            if ui.small_button("Edit Profile\u{2026}").clicked() {
                                edit_profile = true;
                            }
                        });
                    });
                    ui.end_row();

                    if settings.format == Format::Manuscript {
                        ui.label("Contact");
                        ui.add(
                            egui::TextEdit::multiline(&mut settings.contact)
                                .hint_text(if profile.contact.trim().is_empty() {
                                    "Address, email, phone, agent\u{2026} (one per line)".to_owned()
                                } else {
                                    profile.contact.trim().to_owned()
                                })
                                .desired_rows(3),
                        )
                        .on_hover_text("On the title page. Leave empty to use your profile's.");
                        ui.end_row();
                    }

                    ui.label("Format");
                    ui.vertical(|ui| {
                        ui.radio_value(&mut settings.format, Format::Manuscript, "Word manuscript (.docx)");
                        ui.radio_value(&mut settings.format, Format::Pdf, "PDF, set like a book (.pdf)");
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

                    ui.label("Documents");
                    ui.checkbox(&mut settings.document_titles, "Their titles as headings")
                        .on_hover_text("Each document in a chapter under its own title: for essays and posts, rather than scenes. No break mark is needed between them.");
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
                            Format::Epub | Format::Pdf => "As notes at the end of each chapter",
                            Format::Markdown => "As footnotes (pandoc)",
                        };
                        ui.radio_value(&mut settings.citations, Citations::LeaveOut, "Leave out");
                        ui.radio_value(&mut settings.citations, Citations::Notes, as_notes);
                        ui.checkbox(&mut settings.works_cited, "List the works cited at the end");
                    })
                    .response
                    .on_hover_text("From [[references]] to profiles with a Citation section, such as Sources. The note is that section, with any page given after | in the reference.");
                    ui.end_row();

                    if matches!(settings.format, Format::Manuscript | Format::Pdf) {
                        ui.label("Paper");
                        ui.horizontal(|ui| {
                            ui.radio_value(&mut settings.paper, Paper::Letter, "US Letter");
                            ui.radio_value(&mut settings.paper, Paper::A4, "A4");
                            ui.radio_value(&mut settings.paper, Paper::Trade, "6 \u{d7} 9 in").on_hover_text("A trade paperback's page");
                        });
                        ui.end_row();
                    }
                    if settings.format == Format::Manuscript {
                        ui.label("Font");
                        ui.horizontal(|ui| {
                            ui.radio_value(&mut settings.typeface, Typeface::Times, "Times New Roman");
                            ui.radio_value(&mut settings.typeface, Typeface::Courier, "Courier New");
                        });
                        ui.end_row();
                    }
                    if settings.format == Format::Pdf {
                        ui.label("Font");
                        pdf_font_picker(ui, &mut settings.pdf_font);
                        ui.end_row();
                    }
                });

                // Front and back matter, folded away until wanted.
                let in_use = settings.copyright_page
                    || settings.contents
                    || settings.about_author
                    || settings.front_matter.is_some()
                    || settings.back_matter.is_some()
                    || !settings.title_page;
                egui::CollapsingHeader::new("Front and back matter").id_salt("compile-matter").default_open(in_use).show(ui, |ui| {
                    egui::Grid::new("compile-matter-grid").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
                            ui.label("Pages");
                            ui.vertical(|ui| {
                                let word = settings.format == Format::Manuscript;
                                ui.add_enabled_ui(!word, |ui| {
                                    let mut shown = settings.title_page || word;
                                    if ui.checkbox(&mut shown, "Title page").changed() {
                                        settings.title_page = shown;
                                    }
                                })
                                .response
                                .on_disabled_hover_text("A manuscript always has its title page.");
                                ui.checkbox(&mut settings.copyright_page, "Copyright page")
                                    .on_hover_text("\u{a9} this year and the author, All rights reserved, and your website if your profile has one.");
                                if settings.format == Format::Markdown {
                                    ui.checkbox(&mut settings.contents, "Contents").on_hover_text("Asks pandoc for a table of contents.");
                                } else {
                                    ui.add_enabled_ui(!word, |ui| {
                                        ui.checkbox(&mut settings.contents, "Contents");
                                    })
                                    .response
                                    .on_disabled_hover_text("Manuscripts sent to agents and editors don't have one.");
                                }
                                let has_bio = !profile.bio.trim().is_empty();
                                ui.add_enabled_ui(has_bio, |ui| {
                                    ui.checkbox(&mut settings.about_author, "About the Author, at the end");
                                })
                                .response
                                .on_disabled_hover_text("Write a few lines about yourself in your profile (App Settings \u{2192} You) first.");
                            });
                            ui.end_row();

                            ui.label("Front matter");
                            matter_picker(ui, "front-matter", &mut settings.front_matter, &folders)
                                .on_hover_text("A page for each document in the folder, before the first chapter: a dedication, an epigraph, a foreword. Start one with a # heading to give it a title and a place in the contents.");
                            ui.end_row();
                            ui.label("Back matter");
                            matter_picker(ui, "back-matter", &mut settings.back_matter, &folders)
                                .on_hover_text("A page for each document in the folder, after the last chapter: acknowledgements, an afterword. Start one with a # heading to give it a title.");
                            ui.end_row();
                    });
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
                    Format::Pdf => "Pages set like a book: justified, numbered from the first chapter, with bookmarks. Ready to read, share or print.",
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
        if edit_profile {
            self.settings_open = true;
            self.settings_page_you = true;
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
        Format::Pdf => crate::compile_pdf::to_pdf(manuscript, settings)?.0,
    };
    sloth_core::document::write_atomically(path, &bytes).map_err(|e| e.to_string())
}

/// Folders that can be front or back matter, as (id, where it is):
/// any outside the Manuscript and the Trash.
fn matter_folders(binder: &[Node]) -> Vec<(String, String)> {
    fn walk(nodes: &[Node], path: &str, out: &mut Vec<(String, String)>) {
        for node in nodes.iter().filter(|n| !n.is_document()) {
            if matches!(node.role, Some(Role::Manuscript | Role::Trash)) {
                continue;
            }
            let here = if path.is_empty() { node.title.clone() } else { format!("{path} \u{203a} {}", node.title) };
            out.push((node.id.clone(), here.clone()));
            walk(&node.children, &here, out);
        }
    }
    let mut out = Vec::new();
    walk(binder, "", &mut out);
    out
}

/// None, or one of the folders.
fn matter_picker(ui: &mut egui::Ui, id: &str, chosen: &mut Option<String>, folders: &[(String, String)]) -> egui::Response {
    let shown = chosen
        .as_ref()
        .map(|c| folders.iter().find(|(id, _)| id == c).map_or("(a folder no longer there)", |(_, name)| name.as_str()))
        .unwrap_or("None");
    egui::ComboBox::from_id_salt(id)
        .selected_text(shown)
        .width(220.0)
        .show_ui(ui, |ui| {
            ui.selectable_value(chosen, None, "None");
            for (id, name) in folders {
                ui.selectable_value(chosen, Some(id.clone()), name);
            }
        })
        .response
}

/// The PDF's font: the default reading face, or any installed family.
fn pdf_font_picker(ui: &mut egui::Ui, chosen: &mut Option<String>) {
    let default_name = format!("Default ({})", sloth_core::fonts::default_prose_font().unwrap_or("built-in"));
    let shown = chosen.clone().unwrap_or_else(|| default_name.clone());
    egui::ComboBox::from_id_salt("pdf-font").selected_text(shown).height(320.0).width(220.0).show_ui(ui, |ui| {
        ui.selectable_value(chosen, None, default_name);
        let families = sloth_core::fonts::all_families();
        if families.is_empty() {
            ui.weak("Looking for fonts\u{2026}");
        }
        for family in families {
            ui.selectable_value(chosen, Some(family.clone()), family);
        }
    });
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
