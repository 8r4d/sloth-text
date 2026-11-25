//! The bar for files the binder doesn't list (a synced folder's conflict
//! copies, see strays.rs), and for copies of the project file itself.

use eframe::egui::{self, RichText};

use super::WriterApp;
use crate::strays;

impl WriterApp {
    /// Look again for strays (on opening, and with the checks for outside
    /// changes). Only while the bar hasn't been put away.
    pub(super) fn check_strays(&mut self) {
        let Some(project) = &self.project else { return };
        if self.strays_dismissed {
            return;
        }
        self.strays = strays::find(project);
        self.project_copies = strays::project_file_copies(&project.root);
    }

    pub(super) fn strays_bar(&mut self, ui: &mut egui::Ui) {
        if self.strays_dismissed || (self.strays.is_empty() && self.project_copies.is_empty()) {
            return;
        }
        let Some(project) = &self.project else { return };
        let title_of = |id: &str| project.node(id).map(|n| n.title.clone()).unwrap_or_default();
        // What was found, in a sentence.
        let copies: Vec<String> = self
            .strays
            .iter()
            .map(|s| match (&s.copy_of, &s.from) {
                (Some(id), Some(from)) => format!("a copy of \u{201c}{}\u{201d} from {from}", title_of(id)),
                (Some(id), None) => format!("a copy of \u{201c}{}\u{201d}", title_of(id)),
                (None, _) => format!("\u{201c}{}\u{201d}", s.rel.rsplit('/').next().unwrap_or(&s.rel)),
            })
            .collect();
        let mut bring_in = false;
        let mut show = false;
        let mut dismiss = false;
        let warn = ui.visuals().warn_fg_color;
        egui::Panel::top(egui::Id::unique("strays")).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                if !copies.is_empty() {
                    let what = if copies.len() == 1 { "a file" } else { "files" };
                    ui.label(RichText::new(format!(
                        "This project's folder has {what} the binder doesn't list, most likely from a synced folder when two computers changed the same document: {}.",
                        copies.join(", ")
                    )).color(warn));
                    if ui.button("Bring Them In").on_hover_text("Each copy goes just after its original, to compare and keep what you want").clicked() {
                        bring_in = true;
                    }
                }
                if !self.project_copies.is_empty() {
                    ui.label(RichText::new(format!(
                        "The project file has a copy from another computer ({}): changes made to the binder there may not be showing here. Look at it before deleting it.",
                        self.project_copies.join(", ")
                    )).color(warn));
                }
                if ui.button(crate::app::settings_ui::SHOW_FOLDER).clicked() {
                    show = true;
                }
                if ui.button("Not Now").on_hover_text("Until the project's opened again").clicked() {
                    dismiss = true;
                }
            });
        });
        if show {
            let folder = self.strays.first().and_then(|s| s.rel.split('/').next()).map_or(project.root.clone(), |d| project.root.join(d));
            let _ = opener::open(folder);
        }
        if bring_in {
            let found = std::mem::take(&mut self.strays);
            if let Some(project) = self.project.as_mut() {
                match strays::bring_in(project, &found) {
                    Ok(ids) => {
                        if let Some(first) = ids.first() {
                            self.select(first.clone());
                        }
                    }
                    Err(e) => self.error = Some(format!("Couldn't bring the files in: {e}")),
                }
            }
            self.check_strays();
        }
        if dismiss {
            self.strays_dismissed = true;
        }
    }
}
