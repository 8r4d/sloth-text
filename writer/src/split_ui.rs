//! The split view: a second pane beside the main editor, holding any scene,
//! note or profile (a character beside the scene you're writing, say).
//! Open it with the toolbar's Split button or ⌘\, then pick what it shows:
//! Open in Split View in the binder's menu, ⌥-click in the binder, or the
//! pane's own Show… menu. Its text is edited and autosaved like any other;
//! find stays with the main editor.

use eframe::egui::{self, RichText};
use sloth_core::editor::{self, Highlights};
use sloth_core::tabs;

use super::WriterApp;
use crate::project::{Node, Role};

impl WriterApp {
    pub(super) fn toggle_split(&mut self) {
        self.split_open = !self.split_open;
    }

    /// Show `id` in the split pane, opening the pane.
    pub(super) fn open_in_split(&mut self, id: String) {
        self.split_open = true;
        self.split = Some(id);
    }

    /// The split pane, on the right of the editor (inside the inspector).
    pub(super) fn split_pane(&mut self, ui: &mut egui::Ui) {
        if !self.split_open || self.project.is_none() {
            return;
        }
        // Gone (deleted for good, say): show nothing rather than a stale id.
        if self.split.as_ref().is_some_and(|id| self.project.as_ref().is_some_and(|p| p.node(id).is_none())) {
            self.split = None;
        }
        let width = ui.available_width();
        let fill = ui.visuals().extreme_bg_color;
        egui::Panel::right(egui::Id::unique("split"))
            .resizable(true)
            .default_size(width * 0.45)
            .size_range(260.0..=(width * 0.75).max(300.0))
            .frame(egui::Frame::NONE.fill(fill).inner_margin(egui::Margin::symmetric(16, 10)))
            .show(ui, |ui| {
                self.split_header(ui);
                ui.separator();
                self.split_contents(ui);
            });
    }

    fn split_header(&mut self, ui: &mut egui::Ui) {
        let Some(project) = &self.project else { return };
        let title = self.split.as_deref().and_then(|id| project.node(id)).map(|n| n.title.clone());
        // Everything the pane can show, by top-level folder (not the Trash).
        let mut choices: Vec<(String, Vec<(String, String)>)> = Vec::new();
        for top in project.manifest.binder.iter().filter(|n| n.role != Some(Role::Trash)) {
            let items: Vec<(String, String)> = top.children.iter().flat_map(Node::subtree).map(|n| (n.id.clone(), n.title.clone())).collect();
            if !items.is_empty() {
                choices.push((top.title.clone(), items));
            }
        }
        let mut pick = None;
        let mut close = false;
        ui.horizontal(|ui| {
            ui.label(RichText::new(title.as_deref().unwrap_or("Split view")).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("\u{d7}").on_hover_text("Close the split view (\u{2318}\\)").clicked() {
                    close = true;
                }
                ui.menu_button("Show\u{2026}", |ui| {
                    egui::ScrollArea::vertical().max_height(400.0).show(ui, |ui| {
                        for (folder, items) in &choices {
                            ui.label(RichText::new(folder).small().weak());
                            for (id, title) in items {
                                if ui.button(title).clicked() {
                                    pick = Some(id.clone());
                                    ui.close();
                                }
                            }
                        }
                    });
                });
            });
        });
        if let Some(id) = pick {
            self.split = Some(id);
        }
        if close {
            self.split_open = false;
        }
    }

    fn split_contents(&mut self, ui: &mut egui::Ui) {
        let Some(id) = self.split.clone() else {
            ui.add_space(12.0);
            ui.weak("Choose what to show here with Show\u{2026}, or \u{2325}-click something in the binder.");
            return;
        };
        // The same item twice would be two editors of one text, fighting.
        if self.selected.as_deref() == Some(id.as_str()) {
            ui.add_space(12.0);
            ui.weak("This is open in the main editor.");
            return;
        }
        if let Some(kind) = self.profile_kind_of(&id) {
            if self.ensure_open(&id).is_some() {
                self.profile_form(ui, &id, &kind);
            }
            return;
        }
        let is_folder = self.project.as_ref().and_then(|p| p.node(&id)).is_some_and(|n| !n.is_document());
        if is_folder {
            self.folder_view(ui, &id);
            return;
        }
        if self.ensure_open(&id).is_none() {
            return;
        }
        let settings = self.editor.clone();
        let Some(tab) = self.tab_of(&id) else { return };
        // Changed by another program while edited here: the same choice as
        // in the main editor.
        let choice = tabs::disk_notice_bar(ui, egui::Id::unique("split-notice"), tab);
        if let Some(choice) = choice {
            self.resolve_notice_for(&id, choice);
        }
        self.suggestion_keys(ui.ctx(), &id);
        let Some(tab) = self.tab_of(&id) else { return };
        let info = editor::show(ui, tab.id, &mut tab.doc.text, &settings, Highlights::default(), None, None);
        let (hovered, clicked) = super::references_ui::reported(&tab.doc.text, &info);
        self.reference_events(ui, hovered, clicked);
        self.suggestions_ui(ui, &id, &info);
    }
}
