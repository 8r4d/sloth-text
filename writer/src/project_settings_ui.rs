//! Project Settings (⌥⌘,): the choices that belong to the open project,
//! saved in it, beside App Settings (⌘,), which are the same for every
//! project. Statuses, labels and profile sections keep their own editors
//! (also opened from where they're used); this window opens them, so
//! there's one place to find everything about a project.

use eframe::egui::{self, RichText};

use super::WriterApp;
use super::inspector::label_color;
use super::projects_ui::kind_picker;
use super::settings_ui::group;

impl WriterApp {
    pub(super) fn open_project_settings(&mut self) {
        if self.project.is_some() {
            self.project_settings_open = true;
            // The goal fields are filled afresh from the project.
            self.goal_fields = None;
        }
    }

    pub(super) fn project_settings_window(&mut self, ctx: &egui::Context) {
        if !self.project_settings_open {
            return;
        }
        let Some(project) = self.project.as_mut() else {
            self.project_settings_open = false;
            return;
        };
        let goals = &mut project.manifest.goals;
        let fields = self.goal_fields.get_or_insert_with(|| {
            [
                goals.daily.map(|n| n.to_string()).unwrap_or_default(),
                goals.total.map(|n| n.to_string()).unwrap_or_default(),
                goals.deadline.clone().unwrap_or_default(),
            ]
        });
        let goals_before = goals.clone();
        let title = project.manifest.title.clone();
        let kind = project.manifest.kind;
        let mut new_at_top = project.manifest.new_at_top;
        let statuses = project.manifest.statuses.clone();
        let labels = project.manifest.labels.clone();
        let profiles = project.manifest.profiles.clone();

        let mut open = true;
        let mut rename = false;
        let mut new_kind = None;
        let mut edit_statuses = false;
        let mut edit_labels = false;
        let mut edit_sections = None;
        egui::Window::new("Project Settings")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(440.0)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .show(ctx, |ui| {
                let height = (ctx.content_rect().height() * 0.7).max(200.0);
                egui::ScrollArea::vertical().max_height(height).show(ui, |ui| {
                    ui.weak("These are saved in this project. App Settings (\u{2318},) are the same for every project.");

                    group(ui, "Project");
                    egui::Grid::new("project-settings-project").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                        ui.label("Title");
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&title).strong());
                            if ui.button("Rename\u{2026}").clicked() {
                                rename = true;
                            }
                        });
                        ui.end_row();
                        ui.label("Icon");
                        let shown = RichText::new(format!("{}  {}", kind.icon(), kind.name()));
                        egui::ComboBox::from_id_salt("project-settings-kind").selected_text(shown).width(180.0).show_ui(ui, |ui| {
                            ui.set_min_width(380.0);
                            new_kind = kind_picker(ui, "project-settings-kinds", kind);
                        })
                        .response
                        .on_hover_text("Its icon on the start screen");
                        ui.end_row();
                    });

                    group(ui, "Binder");
                    ui.checkbox(&mut new_at_top, "New documents and folders go at the top of their folder")
                        .on_hover_text("Newest first, for a journal or blog. Off, a new item goes after the selected document, or at the bottom of the selected folder, in order.");

                    group(ui, "Goals");
                    super::stats_ui::goals_form(ui, goals, fields);

                    group(ui, "Statuses and Labels");
                    egui::Grid::new("project-settings-lists").num_columns(3).spacing([12.0, 8.0]).show(ui, |ui| {
                        ui.label("Statuses");
                        if ui.button("Edit\u{2026}").clicked() {
                            edit_statuses = true;
                        }
                        ui.add(egui::Label::new(RichText::new(statuses.join(", ")).weak()).wrap());
                        ui.end_row();
                        ui.label("Labels");
                        if ui.button("Edit\u{2026}").clicked() {
                            edit_labels = true;
                        }
                        ui.horizontal_wrapped(|ui| {
                            if labels.is_empty() {
                                ui.weak("None");
                            }
                            for label in &labels {
                                ui.label(RichText::new("\u{25cf}").color(label_color(label.color)));
                                ui.add_space(-6.0);
                                ui.weak(&label.name);
                            }
                        });
                        ui.end_row();
                    });

                    if !profiles.is_empty() {
                        group(ui, "Profiles");
                        ui.weak("The sections each kind of profile can have.");
                        ui.add_space(4.0);
                        egui::Grid::new("project-settings-profiles").num_columns(3).spacing([12.0, 8.0]).show(ui, |ui| {
                            for kind in &profiles {
                                ui.label(&kind.name);
                                if ui.button("Edit\u{2026}").clicked() {
                                    edit_sections = Some(kind.id.clone());
                                }
                                ui.add(egui::Label::new(RichText::new(kind.sections.join(", ")).weak()).wrap());
                                ui.end_row();
                            }
                        });
                    }
                });
            });

        if *goals != goals_before {
            self.manifest_dirty = true;
        }
        if new_at_top != project.manifest.new_at_top
            && let Err(e) = project.set_new_at_top(new_at_top)
        {
            self.error = Some(format!("Couldn't change where new items go: {e}"));
        }
        if let Some(kind) = new_kind
            && let Err(e) = project.set_kind(kind)
        {
            self.error = Some(format!("Couldn't change the project's icon: {e}"));
        }
        if rename {
            self.start_renaming_project();
        }
        if edit_statuses {
            self.editing_statuses = true;
        }
        if edit_labels {
            self.editing_labels = true;
        }
        if let Some(kind) = edit_sections {
            self.edit_sections(&kind);
        }
        if !open {
            self.project_settings_open = false;
        }
    }
}
