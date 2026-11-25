//! Snapshots in the window: the inspector's Snapshots section (take one,
//! see the list), the Compare window, restoring and deleting. ⌘5 takes a
//! snapshot of the open document.

use eframe::egui::{self, Color32, RichText, Stroke, text::LayoutJob};

use super::WriterApp;
use crate::snapshots::{self, Piece, Snapshot};

/// A snapshot being compared with the current text, and its document.
pub(super) struct Comparing {
    doc_id: String,
    snapshot: Snapshot,
}

impl WriterApp {
    /// The document's text as it is now: in the editor if open, else saved.
    fn text_now(&mut self, doc_id: &str) -> Option<String> {
        if let Some(tab) = self.open.get(doc_id).and_then(|&t| self.docs.get(t)) {
            return Some(tab.doc.text.clone());
        }
        let path = self.project.as_ref()?.file_path(doc_id)?;
        crate::project::read_text(&path).ok()
    }

    pub(super) fn take_snapshot(&mut self, doc_id: &str, name: &str, now: f64) {
        let Some(text) = self.text_now(doc_id) else { return };
        let Some(project) = &self.project else { return };
        match snapshots::take(&project.root, doc_id, &text, name) {
            Ok(s) => self.set_status(format!("Snapshot taken ({})", s.when()), now),
            Err(e) => self.error = Some(format!("Couldn't take a snapshot: {e}")),
        }
    }

    /// Put a snapshot's text back in its document, after snapshotting what's
    /// there now, so the restore can itself be undone.
    pub(super) fn restore_snapshot(&mut self, doc_id: &str, snapshot: &Snapshot, now: f64) {
        let text = match snapshot.text() {
            Ok(t) => t,
            Err(e) => {
                self.error = Some(format!("Couldn't read the snapshot: {e}"));
                return;
            }
        };
        self.take_snapshot(doc_id, &format!("before restoring {}", snapshot.when()), now);
        if self.selected.as_deref() != Some(doc_id) {
            self.select(doc_id.to_owned());
        }
        if let Some(tab) = self.open.get(doc_id).and_then(|&t| self.docs.get_mut(t)) {
            tab.doc.text = text;
            self.set_status(format!("Restored the snapshot from {}", snapshot.when()), now);
        }
    }

    /// The Snapshots section of the inspector, for a document.
    pub(super) fn snapshot_section(&mut self, ui: &mut egui::Ui, doc_id: &str) {
        let Some(project) = &self.project else { return };
        let list = snapshots::list(&project.root, doc_id);
        let now = ui.input(|i| i.time);

        ui.add_space(4.0);
        egui::CollapsingHeader::new(RichText::new(format!("Snapshots ({})", list.len())).strong())
            .id_salt("snapshots")
            .default_open(true)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let field = ui.add(
                        egui::TextEdit::singleline(&mut self.snapshot_name)
                            .hint_text("Name (optional)")
                            .desired_width(ui.available_width() - 70.0),
                    );
                    let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if ui.button("Take").on_hover_text(sloth_core::keys::shown("Take a snapshot (\u{2318}5)")).clicked() || enter {
                        let name = std::mem::take(&mut self.snapshot_name);
                        self.take_snapshot(doc_id, &name, now);
                    }
                });
                if list.is_empty() {
                    // Short, so the inspector keeps its width.
                    ui.horizontal(|ui| {
                        let sloth = self.sloth(ui.ctx(), super::sloth_ui::Pose::Asleep);
                        ui.add(egui::Image::new(&sloth).fit_to_exact_size(egui::Vec2::splat(28.0)));
                        ui.weak("None yet").on_hover_text("Take one before a big change, to come back to.");
                    });
                }
                for snapshot in list {
                    let label = if snapshot.name.is_empty() {
                        snapshot.when()
                    } else {
                        format!("{} \u{2014} {}", snapshot.when(), snapshot.name)
                    };
                    if ui.link(label).on_hover_text("Compare with the current text").clicked() {
                        self.comparing = Some(Comparing { doc_id: doc_id.to_owned(), snapshot });
                    }
                }
            });
    }

    /// The Compare window: the snapshot against the current text, word by
    /// word, with Restore and Delete.
    pub(super) fn compare_window(&mut self, ctx: &egui::Context) {
        let Some(Comparing { doc_id, snapshot }) = &self.comparing else { return };
        let (doc_id, snapshot) = (doc_id.clone(), snapshot.clone());
        let older = snapshot.text().unwrap_or_default();
        let newer = self.text_now(&doc_id).unwrap_or_default();
        let pieces = snapshots::compare(&older, &newer);
        let changed = pieces.iter().any(|p| !matches!(p, Piece::Same(_)));

        let mut open = true;
        let (mut restore, mut delete) = (false, false);
        let title = if snapshot.name.is_empty() { snapshot.when() } else { format!("{} \u{2014} {}", snapshot.when(), snapshot.name) };
        egui::Window::new(format!("Snapshot: {title}"))
            .id(egui::Id::unique("compare-snapshot"))
            .open(&mut open)
            .collapsible(false)
            .default_size([640.0, 480.0])
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if changed {
                        ui.label(RichText::new("struck through").strikethrough().color(removed_color(ui)));
                        ui.weak("only in the snapshot;");
                        ui.label(RichText::new("highlighted").color(added_color(ui)));
                        ui.weak("added since.");
                    } else {
                        ui.weak("No changes since this snapshot.");
                    }
                });
                ui.separator();
                egui::ScrollArea::vertical().max_height(360.0).auto_shrink([false, true]).show(ui, |ui| {
                    ui.add(egui::Label::new(layout(ui, &pieces)).wrap());
                });
                ui.separator();
                ui.horizontal(|ui| {
                    restore = ui
                        .add_enabled(changed, egui::Button::new("Restore This Version"))
                        .on_hover_text("The current text is kept as a snapshot first")
                        .clicked();
                    delete = ui.button("Delete Snapshot\u{2026}").clicked();
                });
            });

        let now = ctx.input(|i| i.time);
        if restore {
            self.restore_snapshot(&doc_id, &snapshot, now);
            self.comparing = None;
        } else if delete {
            self.confirm_delete_snapshot = Some(snapshot);
            self.comparing = None;
        } else if !open {
            self.comparing = None;
        }
    }

    pub(super) fn delete_snapshot_dialog(&mut self, ctx: &egui::Context) {
        let Some(snapshot) = self.confirm_delete_snapshot.clone() else { return };
        let mut choice = None;
        egui::Modal::new(egui::Id::unique("delete-snapshot")).show(ctx, |ui| {
            ui.heading("Delete this snapshot?");
            ui.label(format!("The snapshot from {} will be deleted for good.", snapshot.when()));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Delete").clicked() {
                    choice = Some(true);
                }
                if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    choice = Some(false);
                }
            });
        });
        if let Some(go) = choice {
            self.confirm_delete_snapshot = None;
            if go && let Err(e) = snapshots::delete(&snapshot) {
                self.error = Some(format!("Couldn't delete the snapshot: {e}"));
            }
        }
    }
}

fn removed_color(ui: &egui::Ui) -> Color32 {
    if ui.visuals().dark_mode { Color32::from_rgb(0xF2, 0x8B, 0x82) } else { Color32::from_rgb(0xB4, 0x23, 0x18) }
}

fn added_color(ui: &egui::Ui) -> Color32 {
    if ui.visuals().dark_mode { Color32::from_rgb(0x8C, 0xE0, 0x9A) } else { Color32::from_rgb(0x1A, 0x7F, 0x37) }
}

/// The comparison as one block of text: removed words struck through in
/// red, added words in green on a light tint.
fn layout(ui: &egui::Ui, pieces: &[Piece]) -> LayoutJob {
    let font = egui::FontId::proportional(15.0);
    let normal = egui::TextFormat::simple(font.clone(), ui.visuals().text_color());
    let mut job = LayoutJob::default();
    for piece in pieces {
        match piece {
            Piece::Same(t) => job.append(t, 0.0, normal.clone()),
            Piece::Removed(t) => job.append(
                t,
                0.0,
                egui::TextFormat {
                    color: removed_color(ui),
                    strikethrough: Stroke::new(1.0, removed_color(ui)),
                    ..normal.clone()
                },
            ),
            Piece::Added(t) => job.append(
                t,
                0.0,
                egui::TextFormat { color: added_color(ui), background: added_color(ui).gamma_multiply(0.15), ..normal.clone() },
            ),
        }
    }
    job
}
