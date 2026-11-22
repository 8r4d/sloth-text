//! The binder: the project's tree in the sidebar. It only draws and
//! reports what the person did; the app makes the changes.

use eframe::egui::{self, Key};

use crate::project::{Node, Role};

pub enum BinderAction {
    Select(String),
    StartRename(String),
    FinishRename { id: String, title: String },
    CancelRename,
    /// New items go next to (or inside) the given item; see `WriterApp::place_near`.
    NewDocument(String),
    NewFolder(String),
    Trash(String),
    EmptyTrash,
}

/// An item whose title is being edited in place.
pub struct Renaming {
    pub id: String,
    pub text: String,
    /// Put the cursor in the field on the next frame.
    pub focus: bool,
}

pub fn show(ui: &mut egui::Ui, binder: &[Node], selected: Option<&str>, renaming: &mut Option<Renaming>) -> Vec<BinderAction> {
    let mut actions = Vec::new();
    for node in binder {
        node_ui(ui, node, selected, renaming, &mut actions);
    }
    actions
}

fn node_ui(ui: &mut egui::Ui, node: &Node, selected: Option<&str>, renaming: &mut Option<Renaming>, actions: &mut Vec<BinderAction>) {
    if node.children.is_empty() {
        ui.horizontal(|ui| {
            // Line up with siblings that have an open/close arrow.
            ui.add_space(ui.spacing().icon_width + ui.spacing().item_spacing.x);
            row(ui, node, selected, renaming, actions);
        });
    } else {
        let id = ui.make_persistent_id(("binder", &node.id));
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true)
            .show_header(ui, |ui| row(ui, node, selected, renaming, actions))
            .body(|ui| {
                for child in &node.children {
                    node_ui(ui, child, selected, renaming, actions);
                }
            });
    }
}

fn row(ui: &mut egui::Ui, node: &Node, selected: Option<&str>, renaming: &mut Option<Renaming>, actions: &mut Vec<BinderAction>) {
    if let Some(r) = renaming.as_mut().filter(|r| r.id == node.id) {
        let response = ui.add(egui::TextEdit::singleline(&mut r.text).desired_width(f32::INFINITY));
        if r.focus {
            // Focus with the whole title selected, so typing replaces it.
            response.request_focus();
            let mut state = egui::text_edit::TextEditState::load(ui.ctx(), response.id).unwrap_or_default();
            let all = egui::text::CCursorRange::two(
                egui::text::CCursor::new(0),
                egui::text::CCursor::new(r.text.chars().count()),
            );
            state.cursor.set_char_range(Some(all));
            state.store(ui.ctx(), response.id);
            r.focus = false;
        }
        if response.lost_focus() {
            // Return or a click elsewhere keeps the new title; Escape doesn't.
            if ui.input(|i| i.key_pressed(Key::Escape)) {
                actions.push(BinderAction::CancelRename);
            } else {
                actions.push(BinderAction::FinishRename { id: node.id.clone(), title: r.text.clone() });
            }
        }
        return;
    }

    let icon = match node.role {
        Some(Role::Trash) => "🗑",
        Some(_) => "📚",
        None if node.is_document() => "📄",
        None => "📁",
    };
    let response = ui.selectable_label(selected == Some(node.id.as_str()), format!("{icon} {}", node.title));
    if response.clicked() {
        actions.push(BinderAction::Select(node.id.clone()));
    }
    if response.double_clicked() && node.role.is_none() {
        actions.push(BinderAction::StartRename(node.id.clone()));
    }
    response.context_menu(|ui| {
        if node.role == Some(Role::Trash) {
            let empty = !node.children.is_empty();
            if ui.add_enabled(empty, egui::Button::new("Empty Trash…")).clicked() {
                actions.push(BinderAction::EmptyTrash);
                ui.close();
            }
            return;
        }
        if ui.button("New Document").clicked() {
            actions.push(BinderAction::NewDocument(node.id.clone()));
            ui.close();
        }
        if ui.button("New Folder").clicked() {
            actions.push(BinderAction::NewFolder(node.id.clone()));
            ui.close();
        }
        if node.role.is_none() {
            ui.separator();
            if ui.button("Rename").clicked() {
                actions.push(BinderAction::StartRename(node.id.clone()));
                ui.close();
            }
            if ui.button("Move to Trash").clicked() {
                actions.push(BinderAction::Trash(node.id.clone()));
                ui.close();
            }
        }
    });
}
