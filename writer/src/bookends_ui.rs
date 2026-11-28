//! Bookends: the first and last paragraphs of what's selected (a story,
//! or a folder of its parts), side by side, to see whether the ending
//! answers the opening. A window that follows the selection.

use eframe::egui::{self, RichText};

use super::WriterApp;

/// Seconds the paragraphs are kept before they're read again.
const REFRESH: f64 = 1.0;

/// The bookends as last read.
pub(super) struct Bookends {
    id: String,
    at: f64,
    found: Option<(String, Option<String>)>,
}

impl WriterApp {
    /// What's selected, as one text: a document's, or a folder's documents in order.
    fn story_text(&self, id: &str) -> Option<String> {
        let project = self.project.as_ref()?;
        let node = project.node(id)?;
        if node.is_file() {
            return None;
        }
        if node.is_folder() {
            let parts: Vec<String> = project.documents_under(id).into_iter().filter_map(|n| self.document_text(&n.id)).collect();
            return Some(parts.join("\n\n"));
        }
        self.document_text(id)
    }

    pub(super) fn bookends_window(&mut self, ctx: &egui::Context) {
        if !self.show_bookends {
            return;
        }
        let now = ctx.input(|i| i.time);
        let selected = self.selected.clone();
        if let Some(id) = &selected {
            let fresh = self.bookends.as_ref().is_some_and(|b| b.id == *id && now - b.at < REFRESH);
            if !fresh {
                let found = self.story_text(id).and_then(|t| crate::story::bookends(&t));
                self.bookends = Some(Bookends { id: id.clone(), at: now, found });
            }
        }
        let title = selected.as_deref().and_then(|id| self.project.as_ref()?.node(id)).map(|n| n.title.clone());
        let found = self.bookends.as_ref().filter(|b| Some(&b.id) == selected.as_ref()).and_then(|b| b.found.clone());
        let mut open = true;
        egui::Window::new("Bookends")
            .id(egui::Id::unique("bookends"))
            .open(&mut open)
            .default_width(560.0)
            .resizable(true)
            .show(ctx, |ui| {
                if let Some(title) = &title {
                    ui.weak(title);
                    ui.add_space(4.0);
                }
                match &found {
                    None => {
                        ui.weak("Select a story (or the folder of its parts) to see how it opens and how it ends.");
                    }
                    Some((first, last)) => {
                        ui.columns(2, |columns| {
                            columns[0].label(RichText::new("How it opens").small().strong());
                            columns[0].add_space(4.0);
                            columns[0].label(first);
                            columns[1].label(RichText::new("How it ends").small().strong());
                            columns[1].add_space(4.0);
                            match last {
                                Some(last) => columns[1].label(last),
                                None => columns[1].weak("Only one paragraph so far."),
                            };
                        });
                    }
                }
                ui.add_space(6.0);
                ui.label(RichText::new("Does the ending answer the opening? An image, a line or a question that comes back changed.").small().weak());
            });
        if !open {
            self.show_bookends = false;
        }
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(REFRESH));
    }
}
