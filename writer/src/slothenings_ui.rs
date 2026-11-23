//! Slothenings: a folder's documents as one continuous text, to read and
//! edit through (⌘1, or Slothenings above the corkboard). Each document
//! keeps its own editor, file and undo, one after another on a page,
//! divided by a thin rule with its title; the arrow keys run on across the
//! joins. Click a title to open that document on its own.

use eframe::egui::{self, Key, Modifiers, Sense, Stroke, Vec2};
use sloth_core::editor::{self, Highlights};

use super::WriterApp;

/// Going on into a neighbouring document: which, and where (a character).
pub(super) struct Crossing {
    id: String,
    at: usize,
}

impl Crossing {
    pub(super) fn to(id: String, at: usize) -> Self {
        Self { id, at }
    }
}

impl WriterApp {
    /// The documents with an editor in the main area: the one open, or a
    /// folder's in Slothenings. (Not while the Canvas is showing.)
    pub(super) fn main_editor_documents(&self) -> Vec<String> {
        let Some(selected) = self.selected.as_deref() else { return Vec::new() };
        let Some(node) = self.project.as_ref().and_then(|p| p.node(selected)) else { return Vec::new() };
        if self.canvas_open {
            Vec::new()
        } else if node.is_document() {
            vec![selected.to_owned()]
        } else if self.folder_views.get(selected) == Some(&super::views::FolderView::Slothenings) {
            self.slothenings_documents(selected).into_iter().map(|(id, _)| id).collect()
        } else {
            Vec::new()
        }
    }

    /// The documents under a folder, in binder order (any depth).
    fn slothenings_documents(&self, folder: &str) -> Vec<(String, String)> {
        let Some(project) = &self.project else { return Vec::new() };
        project.documents_under(folder).into_iter().map(|n| (n.id.clone(), n.title.clone())).collect()
    }

    pub(super) fn slothenings(&mut self, ui: &mut egui::Ui, folder: &str) {
        let documents = self.slothenings_documents(folder);
        let ctx = ui.ctx().clone();
        // Open them all (each keeps its cursor and undo).
        let tabs: Vec<_> = documents.iter().filter_map(|(id, title)| Some((id.clone(), title.clone(), self.ensure_open(id)?))).collect();
        self.cross_boundaries(&ctx, &tabs);

        let mut settings = self.editor.clone();
        settings.embedded = true;
        settings.typewriter = false;
        settings.focus_paragraph = false;
        let column = settings.max_text_width.unwrap_or(f32::INFINITY);
        let mut open_alone = None;
        let crossing = self.crossing.take();
        // In the split pane, a folder's documents the main area already has.
        let elsewhere = if self.selected.as_deref() == Some(folder) { Vec::new() } else { self.main_editor_documents() };
        egui::ScrollArea::vertical().id_salt(("slothenings", folder)).auto_shrink([false, false]).show(ui, |ui| {
            for (i, (id, title, tab_id)) in tabs.iter().enumerate() {
                if divider(ui, title, column, i == 0).clicked() {
                    open_alone = Some(id.clone());
                }
                // Shown here in the split pane, but open in the main area:
                // one editor of it is enough.
                if elsewhere.contains(id) {
                    ui.weak("Open in the main editor.");
                    continue;
                }
                let mut reveal = None;
                if let Some(c) = crossing.as_ref().filter(|c| c.id == *id) {
                    ui.memory_mut(|m| m.request_focus(editor::editor_id(*tab_id)));
                    editor::set_selection(&ctx, *tab_id, c.at..c.at);
                    reveal = Some(c.at);
                }
                let Some(tab) = self.docs.get_mut(*tab_id) else { continue };
                let info = editor::show(ui, *tab_id, &mut tab.doc.text, &settings, Highlights::default(), reveal, None);
                let (hovered, clicked) = super::references_ui::reported(&tab.doc.text, &info);
                self.reference_events(ui, hovered, clicked);
            }
            ui.add_space(ui.available_height().max(120.0) * 0.5);
        });
        if let Some(id) = open_alone {
            self.select(id);
        }
    }

    /// At the end of a document, ↓ or → go on into the next; at its start,
    /// ↑ or ← back into the previous.
    fn cross_boundaries(&mut self, ctx: &egui::Context, tabs: &[(String, String, sloth_core::tabs::TabId)]) {
        let Some(i) = tabs.iter().position(|(_, _, tab)| ctx.memory(|m| m.has_focus(editor::editor_id(*tab)))) else { return };
        let (_, _, tab) = &tabs[i];
        let Some(len) = self.docs.get(*tab).map(|t| t.doc.text.chars().count()) else { return };
        let Some(sel) = editor::selection(ctx, *tab) else { return };
        let key = |k| ctx.input(|input| input.key_pressed(k) && input.modifiers.is_none());
        if sel == (len..len) && i + 1 < tabs.len() && (key(Key::ArrowDown) || key(Key::ArrowRight)) {
            ctx.input_mut(|input| {
                input.consume_key(Modifiers::NONE, Key::ArrowDown);
                input.consume_key(Modifiers::NONE, Key::ArrowRight);
            });
            self.crossing = Some(Crossing { id: tabs[i + 1].0.clone(), at: 0 });
        } else if sel == (0..0) && i > 0 && (key(Key::ArrowUp) || key(Key::ArrowLeft)) {
            ctx.input_mut(|input| {
                input.consume_key(Modifiers::NONE, Key::ArrowUp);
                input.consume_key(Modifiers::NONE, Key::ArrowLeft);
            });
            let end = self.docs.get(tabs[i - 1].2).map_or(0, |t| t.doc.text.chars().count());
            self.crossing = Some(Crossing { id: tabs[i - 1].0.clone(), at: end });
        }
    }
}

/// Between documents: a faint rule across the text's column with the next
/// one's title in it. Clicking it opens that document on its own.
fn divider(ui: &mut egui::Ui, title: &str, column: f32, first: bool) -> egui::Response {
    ui.add_space(if first { 4.0 } else { 26.0 });
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 20.0), Sense::click());
    let text_width = column.min(width);
    let left = rect.left() + (width - text_width) / 2.0;
    let visuals = ui.visuals();
    let color = if response.hovered() { visuals.hyperlink_color } else { visuals.weak_text_color() };
    let painter = ui.painter();
    let galley = painter.layout_no_wrap(title.to_owned(), egui::FontId::proportional(11.0), color);
    let mid = rect.center().y;
    let label_left = left + 2.0;
    painter.galley(egui::pos2(label_left, mid - galley.size().y / 2.0), galley.clone(), color);
    let rule = Stroke::new(1.0, visuals.weak_text_color().gamma_multiply(0.35));
    painter.hline(label_left + galley.size().x + 10.0..=left + text_width, mid, rule);
    // Named for VoiceOver (and tests): the title is painted, not a label.
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, title));
    response.on_hover_text("Open this document on its own").on_hover_cursor(egui::CursorIcon::PointingHand)
}
