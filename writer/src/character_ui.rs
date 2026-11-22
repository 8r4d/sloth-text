//! Characters in the window: the profile form shown for a character (name,
//! description, a section per characteristic, + Characteristic, and the
//! scenes they appear in), the window for editing the project's list of
//! characteristics, and the Characters section of a scene's inspector.

use eframe::egui::{self, FontId, RichText};

use super::WriterApp;
use crate::binder::BinderAction;
use crate::character::{Section, Sheet};
use crate::project::{Meta, Role};

/// The profile column's widest.
const COLUMN: f32 = 680.0;

/// The Characteristics window's state: the names as being edited.
pub(super) struct EditingCharacteristics {
    names: Vec<String>,
}

impl WriterApp {
    /// A character is a document in the Characters folder.
    pub(super) fn is_character(&self, id: &str) -> bool {
        self.project
            .as_ref()
            .is_some_and(|p| p.role_of(id) == Some(Role::Characters) && p.node(id).is_some_and(|n| n.is_document()))
    }

    /// The selected character's profile, as a form. Edits go straight into
    /// the document's text (so it autosaves like any other document).
    pub(super) fn character_form(&mut self, ui: &mut egui::Ui, id: &str) {
        let Some(project) = &self.project else { return };
        let title = project.node(id).map(|n| n.title.clone()).unwrap_or_default();
        let characteristics = project.manifest.characteristics.clone();
        let appearances: Vec<(String, String)> =
            project.appearances(id).into_iter().map(|n| (n.id.clone(), n.title.clone())).collect();
        let Some(text) = self.current().map(|t| t.doc.text.clone()) else { return };
        let original = Sheet::parse(&text);
        let mut sheet = original.clone();
        let prose = FontId::new(16.0, sloth_core::fonts::prose_family());
        let mut rename = None;
        let mut open = None;

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let side = ((ui.available_width() - COLUMN) / 2.0).max(0.0);
            ui.horizontal_top(|ui| {
                ui.add_space(side);
                ui.vertical(|ui| {
                    ui.set_max_width(COLUMN.min(ui.available_width()));

                    // The name: the item's title.
                    let buffer = match &mut self.character_name {
                        Some((for_id, name)) if for_id == id => name,
                        slot => &mut slot.insert((id.to_owned(), title.clone())).1,
                    };
                    let name = ui.add(
                        egui::TextEdit::singleline(buffer)
                            .font(FontId::new(26.0, sloth_core::fonts::prose_family()))
                            .hint_text("Name")
                            .frame(egui::Frame::NONE)
                            .desired_width(f32::INFINITY),
                    );
                    if name.lost_focus() && buffer.trim() != title && !buffer.trim().is_empty() {
                        rename = Some(buffer.clone());
                    } else if !name.has_focus() && *buffer != title {
                        *buffer = title.clone();
                    }
                    ui.add_space(8.0);

                    ui.add(
                        egui::TextEdit::multiline(&mut sheet.description)
                            .font(prose.clone())
                            .hint_text("Who are they? A few lines to start.")
                            .desired_rows(4)
                            .desired_width(f32::INFINITY),
                    );

                    // A section per characteristic.
                    let mut remove = None;
                    let mut swap = None;
                    let count = sheet.sections.len();
                    for (i, section) in sheet.sections.iter_mut().enumerate() {
                        ui.add_space(14.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&section.heading).strong().size(15.0));
                            ui.menu_button("\u{2026}", |ui| {
                                if ui.add_enabled(i > 0, egui::Button::new("Move Up")).clicked() {
                                    swap = Some((i - 1, i));
                                    ui.close();
                                }
                                if ui.add_enabled(i + 1 < count, egui::Button::new("Move Down")).clicked() {
                                    swap = Some((i, i + 1));
                                    ui.close();
                                }
                                ui.separator();
                                if ui.button("Remove Section").clicked() {
                                    remove = Some(i);
                                    ui.close();
                                }
                            });
                        });
                        ui.add(
                            egui::TextEdit::multiline(&mut section.text)
                                .id_salt(("characteristic", id, &section.heading))
                                .font(prose.clone())
                                .desired_rows(3)
                                .desired_width(f32::INFINITY),
                        );
                    }
                    if let Some((a, b)) = swap {
                        sheet.sections.swap(a, b);
                    }
                    if let Some(i) = remove {
                        sheet.sections.remove(i);
                    }

                    ui.add_space(14.0);
                    ui.menu_button("+ Characteristic", |ui| {
                        let unused: Vec<&String> = characteristics.iter().filter(|c| !sheet.has(c)).collect();
                        if unused.is_empty() {
                            ui.weak("Every characteristic is here.");
                        }
                        for heading in unused {
                            if ui.button(heading).clicked() {
                                sheet.sections.push(Section { heading: heading.clone(), text: String::new() });
                                ui.close();
                            }
                        }
                        ui.separator();
                        if ui.button("Edit Characteristics\u{2026}").clicked() {
                            self.editing_characteristics = Some(EditingCharacteristics { names: characteristics.clone() });
                            ui.close();
                        }
                    });

                    // Where they appear.
                    ui.add_space(22.0);
                    ui.label(RichText::new("Appears in").small().weak());
                    if appearances.is_empty() {
                        ui.weak("No scenes yet. Add this character to a scene in its inspector.");
                    }
                    for (scene, scene_title) in &appearances {
                        if ui.link(scene_title).clicked() {
                            open = Some(scene.clone());
                        }
                    }
                });
            });
        });

        if sheet != original
            && let Some(tab) = self.current()
        {
            tab.doc.text = sheet.to_text();
        }
        if let Some(name) = rename {
            self.apply(BinderAction::FinishRename { id: id.to_owned(), title: name });
        }
        if let Some(scene) = open {
            self.select(scene);
        }
    }

    /// The Characteristics window: the project's list of sections a profile
    /// can have. Renaming one renames that section in every character.
    pub(super) fn characteristics_window(&mut self, ctx: &egui::Context) {
        let Some(editing) = self.editing_characteristics.as_mut() else { return };
        let Some(project) = &self.project else { return };
        let current = project.manifest.characteristics.clone();
        let mut open = true;
        let mut renamed = None;
        let mut list_changed = None;
        egui::Window::new("Characteristics")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(320.0)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .show(ctx, |ui| {
                ui.weak("The sections a character's profile can have.");
                ui.add_space(6.0);
                let mut remove = None;
                for (i, name) in editing.names.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        let field = ui.add(egui::TextEdit::singleline(name).desired_width(200.0));
                        // Applied on leaving the field: a rename touches every character.
                        if field.lost_focus()
                            && let Some(old) = current.get(i)
                            && name.trim() != old
                            && !name.trim().is_empty()
                        {
                            renamed = Some((old.clone(), name.trim().to_owned()));
                        }
                        if ui.button("Remove").on_hover_text("Profiles keep any section they have").clicked() {
                            remove = Some(i);
                        }
                    });
                }
                if let Some(i) = remove {
                    editing.names.remove(i);
                    list_changed = Some(editing.names.clone());
                }
                ui.add_space(6.0);
                if ui.button("Add Characteristic").clicked() {
                    editing.names.push("New characteristic".to_owned());
                    list_changed = Some(editing.names.clone());
                }
            });
        if !open {
            self.editing_characteristics = None;
        }
        if let Some((old, new)) = renamed {
            self.rename_characteristic(&old, &new);
        } else if let Some(list) = list_changed
            && let Some(project) = self.project.as_mut()
        {
            project.manifest.characteristics = list.into_iter().map(|n| n.trim().to_owned()).filter(|n| !n.is_empty()).collect();
            self.manifest_dirty = true;
        }
    }

    /// Rename a characteristic in the project's list and in every profile
    /// that has it (open ones in the editor, the rest on disk).
    pub(super) fn rename_characteristic(&mut self, old: &str, new: &str) {
        let Some(project) = self.project.as_mut() else { return };
        for name in project.manifest.characteristics.iter_mut().filter(|n| *n == old) {
            *name = new.to_owned();
        }
        self.manifest_dirty = true;
        let characters: Vec<String> = project.characters().into_iter().map(|n| n.id.clone()).collect();
        let mut failed = Vec::new();
        for id in characters {
            if let Some(tab) = self.open.get(&id).and_then(|&t| self.docs.get_mut(t)) {
                let mut sheet = Sheet::parse(&tab.doc.text);
                if sheet.rename(old, new) {
                    tab.doc.text = sheet.to_text();
                }
                continue;
            }
            let Some(path) = self.project.as_ref().and_then(|p| p.file_path(&id)) else { continue };
            let Ok(text) = std::fs::read_to_string(&path) else { continue };
            let mut sheet = Sheet::parse(&text);
            if sheet.rename(old, new)
                && let Err(e) = sloth_core::document::write_atomically(&path, sheet.to_text().as_bytes())
            {
                failed.push(format!("{} ({e})", path.display()));
            }
        }
        if !failed.is_empty() {
            self.error = Some(format!("Couldn't rename the section in {}", failed.join(", ")));
        }
    }

    /// The Characters section of a scene's inspector: who appears in it.
    pub(super) fn scene_characters(&self, ui: &mut egui::Ui, meta: &mut Meta) {
        let Some(project) = &self.project else { return };
        let all: Vec<(String, String)> = project.characters().into_iter().map(|n| (n.id.clone(), n.title.clone())).collect();
        ui.add_space(10.0);
        ui.label(RichText::new("Characters").small().weak());
        let mut remove = None;
        ui.horizontal_wrapped(|ui| {
            for (id, title) in all.iter().filter(|(id, _)| meta.characters.contains(id)) {
                if ui.button(format!("{title}  \u{d7}")).on_hover_text("Remove from this scene").clicked() {
                    remove = Some(id.clone());
                }
            }
            ui.menu_button("+ Character", |ui| {
                let unlinked: Vec<&(String, String)> = all.iter().filter(|(id, _)| !meta.characters.contains(id)).collect();
                if all.is_empty() {
                    ui.weak("Add characters in the Characters folder first.");
                } else if unlinked.is_empty() {
                    ui.weak("Everyone's here.");
                }
                for (id, title) in unlinked {
                    if ui.button(title).clicked() {
                        meta.characters.push(id.clone());
                        ui.close();
                    }
                }
            });
        });
        if let Some(id) = remove {
            meta.characters.retain(|c| *c != id);
        }
    }
}
