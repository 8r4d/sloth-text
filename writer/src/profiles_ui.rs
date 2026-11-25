//! Profiles in the window. A profile is a document in a profile folder:
//! Characters (every project has it), or Locations, Objects, Factions,
//! Worldbuilding, Themes, or a folder of your own (added from the binder).
//!
//! Here: the form shown for a profile (name, description, a section per
//! characteristic, + Section, and the scenes it appears in), the window for
//! editing a kind's sections, and the part of a scene's inspector that links
//! profiles of each kind to it.

use eframe::egui::{self, FontId, RichText};

use super::WriterApp;
use crate::binder::BinderAction;
use crate::character::{Section, Sheet};
use crate::project::{CHARACTERS, Meta, ProfileKind};

/// The profile column's widest.
const COLUMN: f32 = 680.0;

/// The Sections window's state: a kind, and its names as being edited.
pub(super) struct EditingSections {
    kind: String,
    names: Vec<String>,
}

/// A profile as its form is editing it, and the file text it was last
/// written as (to notice changes from elsewhere).
pub(super) struct ProfileSheet {
    written: String,
    sheet: Sheet,
}

/// "+ Characteristic" for characters, "+ Section" for the rest.
fn add_label(kind: &ProfileKind) -> &'static str {
    if kind.id == CHARACTERS { "+ Characteristic" } else { "+ Section" }
}

impl WriterApp {
    /// If this item is a profile, its kind.
    pub(super) fn profile_kind_of(&self, id: &str) -> Option<ProfileKind> {
        self.project.as_ref()?.profile_of(id).cloned()
    }

    /// The selected profile, as a form. Edits go straight into the
    /// document's text (so it autosaves like any other document).
    pub(super) fn profile_form(&mut self, ui: &mut egui::Ui, id: &str, kind: &ProfileKind) {
        let Some(project) = &self.project else { return };
        let title = project.node(id).map(|n| n.title.clone()).unwrap_or_default();
        let appearances: Vec<(String, String)> =
            project.appearances(&kind.id, id).into_iter().map(|n| (n.id.clone(), n.title.clone())).collect();
        let Some(text) = self.tab_of(id).map(|t| t.doc.text.clone()) else { return };
        let target = crate::references::Target { kind: kind.id.clone(), id: id.to_owned() };
        let links = self.backlinks(ui.input(|i| i.time), &target);
        let mut jump = None;
        // The form edits its own copy, exactly as typed: re-reading the file
        // every frame would trim a space typed at the end before the next
        // word arrives. The file is read again only if it changed from
        // outside the form (a restore, a rename, another program).
        let original = match self.profile_sheet.get(id) {
            Some(c) if c.written == text => c.sheet.clone(),
            _ => Sheet::parse(&text),
        };
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
                    ui.label(RichText::new(&kind.item).small().weak());

                    // The name: the item's title.
                    let buffer = self.profile_name.entry(id.to_owned()).or_insert_with(|| title.clone());
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
                            .hint_text("A few lines to start.")
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
                                .id_salt(("section", id, &section.heading))
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
                    ui.menu_button(add_label(kind), |ui| {
                        let unused: Vec<&String> = kind.sections.iter().filter(|c| !sheet.has(c)).collect();
                        if unused.is_empty() {
                            ui.weak("Every section is here.");
                        }
                        for heading in unused {
                            if ui.button(heading).clicked() {
                                sheet.sections.push(Section { heading: heading.clone(), text: String::new() });
                                ui.close();
                            }
                        }
                        ui.separator();
                        let edit = if kind.id == CHARACTERS { "Edit Characteristics\u{2026}" } else { "Edit Sections\u{2026}" };
                        if ui.button(edit).clicked() {
                            self.editing_sections = Some(EditingSections { kind: kind.id.clone(), names: kind.sections.clone() });
                            ui.close();
                        }
                    });

                    // Where they appear.
                    ui.add_space(22.0);
                    ui.label(RichText::new("Appears in").small().weak());
                    if appearances.is_empty() {
                        ui.weak(format!("No scenes yet. Add this {} to a scene in its inspector.", kind.item.to_lowercase()));
                    }
                    for (scene, scene_title) in &appearances {
                        if ui.link(scene_title).clicked() {
                            open = Some(scene.clone());
                        }
                    }
                    // Where its [[references]] are, by sub-essay.
                    jump = Self::referenced_in(ui, &title, &links);
                });
            });
        });

        let written = if sheet != original { sheet.to_text() } else { text };
        if let Some(tab) = self.tab_of(id)
            && tab.doc.text != written
        {
            tab.doc.text = written.clone();
        }
        self.profile_sheet.insert(id.to_owned(), ProfileSheet { written, sheet });
        if let Some(name) = rename {
            self.apply(BinderAction::FinishRename { id: id.to_owned(), title: name });
        }
        if let Some(scene) = open {
            self.select(scene);
        }
        if let Some(link) = jump {
            self.go_to_backlink(&ui.ctx().clone(), &link);
        }
    }

    /// Open the Sections window for a kind of profile.
    pub(super) fn edit_sections(&mut self, kind: &str) {
        if let Some(kind) = self.project.as_ref().and_then(|p| p.profile_kind(kind)) {
            self.editing_sections = Some(EditingSections { kind: kind.id.clone(), names: kind.sections.clone() });
        }
    }

    /// The Sections window: a kind's list of sections. Renaming one renames
    /// that section in every profile of the kind.
    pub(super) fn sections_window(&mut self, ctx: &egui::Context) {
        let Some(editing) = self.editing_sections.as_mut() else { return };
        let Some(kind) = self.project.as_ref().and_then(|p| p.profile_kind(&editing.kind)).cloned() else {
            self.editing_sections = None;
            return;
        };
        let title = if kind.id == CHARACTERS { "Characteristics".to_owned() } else { format!("{} Sections", kind.item) };
        let mut open = true;
        let mut renamed = None;
        let mut list_changed = None;
        egui::Window::new(title)
            .id(egui::Id::unique("profile-sections"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(320.0)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .show(ctx, |ui| {
                ui.weak(format!("The sections a {} can have.", kind.item.to_lowercase()));
                ui.add_space(6.0);
                let mut remove = None;
                for (i, name) in editing.names.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        let field = ui.add(egui::TextEdit::singleline(name).desired_width(200.0));
                        // Applied on leaving the field: a rename touches every profile.
                        if field.lost_focus()
                            && let Some(old) = kind.sections.get(i)
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
                if ui.button("Add Section").clicked() {
                    editing.names.push("New section".to_owned());
                    list_changed = Some(editing.names.clone());
                }
            });
        if !open {
            self.editing_sections = None;
        }
        if let Some((old, new)) = renamed {
            self.rename_section(&kind.id, &old, &new);
        } else if let Some(list) = list_changed
            && let Some(project) = self.project.as_mut()
        {
            project.set_sections(&kind.id, list.into_iter().map(|n| n.trim().to_owned()).filter(|n| !n.is_empty()).collect());
            self.manifest_dirty = true;
        }
    }

    /// Rename a section in a kind's list and in every profile of that kind
    /// that has it (open ones in the editor, the rest on disk).
    pub(super) fn rename_section(&mut self, kind: &str, old: &str, new: &str) {
        let Some(project) = self.project.as_mut() else { return };
        let Some(sections) = project.profile_kind(kind).map(|k| k.sections.clone()) else { return };
        let sections = sections.into_iter().map(|s| if s == old { new.to_owned() } else { s }).collect();
        project.set_sections(kind, sections);
        self.manifest_dirty = true;
        let items: Vec<String> = project.profile_items(kind).into_iter().map(|n| n.id.clone()).collect();
        let mut failed = Vec::new();
        for id in items {
            if let Some(tab) = self.open.get(&id).and_then(|&t| self.docs.get_mut(t)) {
                let mut sheet = Sheet::parse(&tab.doc.text);
                if sheet.rename(old, new) {
                    tab.doc.text = sheet.to_text();
                }
                continue;
            }
            let Some(path) = self.project.as_ref().and_then(|p| p.file_path(&id)) else { continue };
            let Ok(text) = crate::project::read_text(&path) else { continue };
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

    /// A scene's inspector: who and what appears in it, a part per kind
    /// (Characters, Locations…).
    ///
    /// Profiles the text refers to (`referenced`) are listed too, fainter:
    /// they come and go with their `[[references]]`. Clicking one opens it
    /// in the split pane (returned).
    pub(super) fn scene_profiles(&self, ui: &mut egui::Ui, meta: &mut Meta, referenced: &[crate::references::Target]) -> Option<String> {
        let Some(project) = &self.project else { return None };
        let mut open = None;
        for kind in &project.manifest.profiles {
            let all: Vec<(String, String)> =
                project.profile_items(&kind.id).into_iter().map(|n| (n.id.clone(), n.title.clone())).collect();
            let linked = meta.links.get(&kind.id).cloned().unwrap_or_default();
            ui.add_space(10.0);
            ui.label(RichText::new(&kind.name).small().weak());
            let mut remove = None;
            let mut add = None;
            ui.horizontal_wrapped(|ui| {
                for (id, title) in all.iter().filter(|(id, _)| linked.contains(id)) {
                    if ui.button(format!("{title}  \u{d7}")).on_hover_text("Remove from this scene").clicked() {
                        remove = Some(id.clone());
                    }
                }
                let in_text = referenced.iter().filter(|t| t.kind == kind.id && !linked.contains(&t.id));
                for target in in_text {
                    let Some(title) = project.node(&target.id).map(|n| n.title.clone()) else { continue };
                    let chip = egui::Button::new(RichText::new(format!("[[{title}]]")).small().color(ui.visuals().hyperlink_color)).frame(false);
                    if ui.add(chip).on_hover_text("Referenced in the text: click to open it").clicked() {
                        open = Some(target.id.clone());
                    }
                }
                ui.menu_button(format!("+ {}", kind.item), |ui| {
                    let unlinked: Vec<&(String, String)> = all.iter().filter(|(id, _)| !linked.contains(id)).collect();
                    if all.is_empty() {
                        ui.weak(format!("Add some in the {} folder first.", kind.name));
                    } else if unlinked.is_empty() {
                        ui.weak("They're all here.");
                    }
                    for (id, title) in unlinked {
                        if ui.button(title).clicked() {
                            add = Some(id.clone());
                            ui.close();
                        }
                    }
                });
            });
            if let Some(id) = add {
                meta.links.entry(kind.id.clone()).or_default().push(id);
            }
            if let Some(id) = remove {
                let list = meta.links.entry(kind.id.clone()).or_default();
                list.retain(|c| *c != id);
                if list.is_empty() {
                    meta.links.remove(&kind.id);
                }
            }
        }
        open
    }
}
