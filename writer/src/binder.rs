//! The binder: the project's tree in the sidebar. It only draws and
//! reports what the person did; the app makes the changes.

use eframe::egui::{self, Key};

use crate::app::inspector::label_color;
use crate::project::{Label, Node, Place, Role};

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
    /// Dropped by dragging.
    Move { id: String, place: Place },
}

/// What's being dragged: a binder item's id.
struct Dragged(String);

/// An item whose title is being edited in place.
pub struct Renaming {
    pub id: String,
    pub text: String,
    /// Put the cursor in the field on the next frame.
    pub focus: bool,
}

pub fn show(
    ui: &mut egui::Ui,
    binder: &[Node],
    labels: &[Label],
    selected: Option<&str>,
    renaming: &mut Option<Renaming>,
) -> Vec<BinderAction> {
    let mut actions = Vec::new();
    let dragging = egui::DragAndDrop::payload::<Dragged>(ui.ctx()).map(|d| d.0.clone());
    let mut tree = Tree { labels, selected, renaming, dragging: dragging.as_deref(), actions: &mut actions };
    for node in binder {
        node_ui(ui, node, false, &mut tree);
    }
    actions
}

/// What every row needs, bundled to keep the signatures short.
struct Tree<'a> {
    labels: &'a [Label],
    selected: Option<&'a str>,
    renaming: &'a mut Option<Renaming>,
    /// The item being dragged, if any.
    dragging: Option<&'a str>,
    actions: &'a mut Vec<BinderAction>,
}

/// `inside`: this row is the dragged item or inside it, so nothing can be
/// dropped on it.
fn node_ui(ui: &mut egui::Ui, node: &Node, inside: bool, tree: &mut Tree<'_>) {
    let inside = inside || tree.dragging == Some(node.id.as_str());
    if node.children.is_empty() {
        ui.horizontal(|ui| {
            // Line up with siblings that have an open/close arrow.
            ui.add_space(ui.spacing().icon_width + ui.spacing().item_spacing.x);
            row(ui, node, inside, tree);
        });
    } else {
        let id = ui.make_persistent_id(("binder", &node.id));
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true)
            .show_header(ui, |ui| row(ui, node, inside, tree))
            .body(|ui| {
                for child in &node.children {
                    node_ui(ui, child, inside, tree);
                }
            });
    }
}

fn row(ui: &mut egui::Ui, node: &Node, inside: bool, tree: &mut Tree<'_>) {
    let actions = &mut *tree.actions;
    if let Some(r) = tree.renaming.as_mut().filter(|r| r.id == node.id) {
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
    let label = format!("{icon} {}", node.title);
    // The item's label, as a coloured dot at the end of the row. The grow
    // atom fills the space after the title, keeping it on the left.
    let selected = tree.selected == Some(node.id.as_str());
    let button = match node.meta.label.and_then(|i| tree.labels.get(i)) {
        Some(l) => {
            let dot = egui::RichText::new("\u{25cf}").color(label_color(l.color));
            egui::Button::selectable(selected, (label.as_str(), egui::Atom::grow(), dot))
        }
        None => egui::Button::selectable(selected, (label.as_str(), egui::Atom::grow())),
    };
    // Full width, so the whole row is a click, drag and drop target.
    let response = ui.add(
        button
            .min_size(egui::vec2(ui.available_width(), 0.0))
            .sense(egui::Sense::click_and_drag()),
    );
    if node.role.is_none() {
        response.dnd_set_drag_payload(Dragged(node.id.clone()));
    }
    if response.dragged() {
        drag_ghost(ui.ctx(), &label);
    }
    // drop_place draws the drop marker while hovering, so it runs every
    // frame something hovers; the move happens on release.
    if !inside
        && response.dnd_hover_payload::<Dragged>().is_some()
        && let Some(place) = drop_place(ui, &response, node)
        && let Some(dragged) = response.dnd_release_payload::<Dragged>()
    {
        actions.push(BinderAction::Move { id: dragged.0.clone(), place });
    }
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

/// Where a drop on this row would put the item, drawn as a line (before or
/// after) or an outline (inside). The top and bottom quarters of a row mean
/// before and after it; the middle means inside. Role folders only take
/// items inside.
fn drop_place(ui: &egui::Ui, response: &egui::Response, node: &Node) -> Option<Place> {
    let pointer = ui.ctx().pointer_interact_pos()?;
    let rect = response.rect;
    let fraction = (pointer.y - rect.top()) / rect.height().max(1.0);
    let stroke = egui::Stroke::new(2.0, ui.visuals().selection.stroke.color);
    let painter = ui.painter();
    let id = node.id.clone();
    let place = if node.role.is_none() && fraction < 0.25 {
        painter.hline(rect.x_range(), rect.top(), stroke);
        Place::Before(id)
    } else if node.role.is_none() && fraction > 0.75 {
        painter.hline(rect.x_range(), rect.bottom(), stroke);
        Place::After(id)
    } else {
        painter.rect_stroke(rect, 4.0, stroke, egui::StrokeKind::Inside);
        Place::Into(id)
    };
    Some(place)
}

/// The dragged item's title, following the pointer.
fn drag_ghost(ctx: &egui::Context, label: &str) {
    let Some(pointer) = ctx.pointer_interact_pos() else { return };
    egui::Area::new(egui::Id::unique("binder-drag-ghost"))
        .order(egui::Order::Tooltip)
        .fixed_pos(pointer + egui::vec2(14.0, 6.0))
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| ui.label(label));
        });
}
