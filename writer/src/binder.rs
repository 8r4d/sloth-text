//! The binder: the project's tree in the sidebar. It only draws and
//! reports what the person did; the app makes the changes.

use eframe::egui::{self, Key};

use crate::app::inspector::label_color;
use crate::project::{Label, Manifest, Node, Place, ProfileKind, Role, profile_presets};

pub enum BinderAction {
    Select(String),
    StartRename(String),
    FinishRename { id: String, title: String },
    CancelRename,
    /// New items go next to (or inside) the given item; see `WriterApp::place_near`.
    NewDocument(String),
    NewFolder(String),
    /// Add File…: research files (PDFs, pictures…) beside it.
    AddFile(String),
    Trash(String),
    EmptyTrash,
    /// Dropped by dragging.
    Move { id: String, place: Place },
    /// A preset kind, or `None` for a folder of your own.
    AddProfileFolder(Option<ProfileKind>),
    /// Show this in the split view (⌥-click, or the menu).
    OpenInSplit(String),
    RemoveProfileFolder(String),
    /// Break Up…: a document's parts (at headings or scene breaks) as documents of their own.
    BreakUp(String),
    /// Combine into One Document…: a folder's documents as one.
    Combine(String),
    /// Combine with Next…: this document and the one after it as one.
    CombineWithNext(String),
    /// What's in an item gets this status (an index, or none): what's
    /// directly in it, or (`deep`) what's in its subfolders too.
    StatusInside { id: String, status: Option<usize>, deep: bool },
    /// What's in an item gets this label, likewise.
    LabelInside { id: String, label: Option<usize>, deep: bool },
    /// The documents in an item get this target (or none); with
    /// `only_missing`, just those without one.
    TargetInside { id: String, target: Option<u32>, only_missing: bool, deep: bool },
}

/// What's being dragged: a binder item's id.
pub(crate) struct Dragged(pub(crate) String);

/// An item whose title is being edited in place.
pub struct Renaming {
    pub id: String,
    pub text: String,
    /// Put the cursor in the field on the next frame.
    pub focus: bool,
}

/// `manifest`: the project's binder, and its statuses, labels and profile
/// kinds for the menus.
pub fn show(
    ui: &mut egui::Ui,
    manifest: &Manifest,
    selected: Option<&str>,
    notepad: Option<&str>,
    renaming: &mut Option<Renaming>,
) -> Vec<BinderAction> {
    let mut actions = Vec::new();
    let dragging = egui::DragAndDrop::payload::<Dragged>(ui.ctx()).map(|d| d.0.clone());
    let mut tree = Tree {
        statuses: &manifest.statuses,
        labels: &manifest.labels,
        default_target: manifest.goals.document,
        kinds: &manifest.profiles,
        selected,
        notepad,
        renaming,
        dragging: dragging.as_deref(),
        actions: &mut actions,
    };
    for node in &manifest.binder {
        node_ui(ui, node, false, node.role, None, &mut tree);
    }
    actions
}

/// What every row needs, bundled to keep the signatures short.
struct Tree<'a> {
    statuses: &'a [String],
    labels: &'a [Label],
    /// The target new documents start with (Project Settings).
    default_target: Option<u32>,
    /// The project's profile kinds, to offer the rest as new folders.
    kinds: &'a [ProfileKind],
    selected: Option<&'a str>,
    /// The mobile notepad, shown with its own icon.
    notepad: Option<&'a str>,
    renaming: &'a mut Option<Renaming>,
    /// The item being dragged, if any.
    dragging: Option<&'a str>,
    actions: &'a mut Vec<BinderAction>,
}

/// `inside`: this row is the dragged item or inside it, so nothing can be
/// dropped on it.
/// `top`: the role folder this node is in, for its icon.
/// `next`: the item after it in the same folder, if any.
fn node_ui(ui: &mut egui::Ui, node: &Node, inside: bool, top: Option<Role>, next: Option<&Node>, tree: &mut Tree<'_>) {
    let inside = inside || tree.dragging == Some(node.id.as_str());
    if node.children.is_empty() {
        ui.horizontal(|ui| {
            // Line up with siblings that have an open/close arrow.
            ui.add_space(ui.spacing().icon_width + ui.spacing().item_spacing.x);
            row(ui, node, inside, top, next, tree);
        });
    } else {
        let id = ui.make_persistent_id(("binder", &node.id));
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true)
            .show_header(ui, |ui| row(ui, node, inside, top, next, tree))
            .body(|ui| {
                for (i, child) in node.children.iter().enumerate() {
                    node_ui(ui, child, inside, top, node.children.get(i + 1), tree);
                }
            });
    }
}

fn row(ui: &mut egui::Ui, node: &Node, inside: bool, top: Option<Role>, next: Option<&Node>, tree: &mut Tree<'_>) {
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
        Some(Role::Trash) => crate::icons::TRASH,
        Some(Role::Characters) => crate::icons::PEOPLE,
        Some(Role::Profile) => crate::icons::PROFILES,
        Some(_) => crate::icons::LIBRARY,
        None if tree.notepad == Some(node.id.as_str()) => crate::icons::NOTEPAD,
        None if node.is_document() && top == Some(Role::Characters) => crate::icons::PERSON,
        None if node.is_document() && top == Some(Role::Profile) => crate::icons::CARD,
        None if node.is_document() => crate::icons::DOCUMENT,
        None if node.is_file() => crate::app::files_ui::icon(node.file.as_deref().unwrap_or_default()),
        None => crate::icons::FOLDER,
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
        // ⌥-click: into the split view, leaving the main editor as it is.
        let action = if ui.input(|i| i.modifiers.alt) && node.role.is_none() {
            BinderAction::OpenInSplit(node.id.clone())
        } else {
            BinderAction::Select(node.id.clone())
        };
        actions.push(action);
    }
    if response.double_clicked() && node.role.is_none() {
        actions.push(BinderAction::StartRename(node.id.clone()));
    }
    let kinds = tree.kinds;
    let (statuses, labels) = (tree.statuses, tree.labels);
    // Targets are for the manuscript's documents.
    let default_target = (top == Some(Role::Manuscript)).then_some(tree.default_target);
    response.context_menu(|ui| {
        // At the top level: profile folders to add (and this one to remove).
        if node.role.is_some() {
            ui.menu_button("New Profile Folder", |ui| {
                let unused: Vec<_> = profile_presets().into_iter().filter(|(p, _)| !kinds.iter().any(|k| k.id == p.id)).collect();
                for (heading, fiction) in [("Fiction", true), ("Non-fiction", false)] {
                    // Themes serve essays as well as novels: offered under both.
                    let group: Vec<_> = unused.iter().filter(|(p, f)| *f == fiction || p.id == "themes").collect();
                    if group.is_empty() {
                        continue;
                    }
                    ui.label(egui::RichText::new(heading).small().weak());
                    for (preset, _) in group {
                        if ui.button(&preset.name).clicked() {
                            actions.push(BinderAction::AddProfileFolder(Some(preset.clone())));
                            ui.close();
                        }
                    }
                    ui.separator();
                }
                if ui.button("Custom\u{2026}").clicked() {
                    actions.push(BinderAction::AddProfileFolder(None));
                    ui.close();
                }
            });
            if matches!(node.role, Some(Role::Profile | Role::Characters)) {
                let empty = node.children.is_empty();
                if ui.add_enabled(empty, egui::Button::new("Remove Folder")).on_disabled_hover_text("Only an empty folder can be removed").clicked() {
                    actions.push(BinderAction::RemoveProfileFolder(node.id.clone()));
                    ui.close();
                }
            }
            ui.separator();
        }
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
        if ui.button("Add File\u{2026}").on_hover_text("A PDF, a picture or any other file, kept in the project to look at while you write. Or drop files on the window.").clicked() {
            actions.push(BinderAction::AddFile(node.id.clone()));
            ui.close();
        }
        if node.role.is_none() {
            ui.separator();
            if ui.button("Open in Split View").on_hover_text(sloth_core::keys::shown("Or \u{2325}-click it")).clicked() {
                actions.push(BinderAction::OpenInSplit(node.id.clone()));
                ui.close();
            }
            if ui.button("Rename").clicked() {
                actions.push(BinderAction::StartRename(node.id.clone()));
                ui.close();
            }
            ui.separator();
            if node.is_document()
                && ui.button("Break Up\u{2026}").on_hover_text("Make each part (from a heading or a scene break) a document of its own").clicked()
            {
                actions.push(BinderAction::BreakUp(node.id.clone()));
                ui.close();
            }
            if !node.children.is_empty() {
                inside_menus(ui, node, statuses, labels, default_target, actions);
            }
            if !node.children.is_empty()
                && ui.button("Combine into One Document\u{2026}").on_hover_text("Everything in it as one document").clicked()
            {
                actions.push(BinderAction::Combine(node.id.clone()));
                ui.close();
            }
            if node.is_document()
                && next.is_some_and(|n| n.is_document() && n.children.is_empty())
                && ui.button("Combine with Next\u{2026}").on_hover_text("This document and the one after it as one").clicked()
            {
                actions.push(BinderAction::CombineWithNext(node.id.clone()));
                ui.close();
            }
            ui.separator();
            if ui.button("Move to Trash").clicked() {
                actions.push(BinderAction::Trash(node.id.clone()));
                ui.close();
            }
        }
    });
}

/// The items in `node` that Set Status / Set Label for All Inside change:
/// what's directly in it, or (`deep`) everything under it.
pub fn inside(node: &Node, deep: bool) -> Vec<&Node> {
    if deep { node.subtree().into_iter().skip(1).collect() } else { node.children.iter().collect() }
}

/// Set Status / Set Label for All Inside: what's in a folder at once. Only
/// what's directly in it, unless Include subfolders is ticked (remembered
/// while Writer's open). The choice they all have already is highlighted.
/// `default_target`: in the Manuscript, the project's default target for
/// new documents (if it has one); outside it, `None` and no target menu.
fn inside_menus(ui: &mut egui::Ui, node: &Node, statuses: &[String], labels: &[Label], default_target: Option<Option<u32>>, actions: &mut Vec<BinderAction>) {
    let key = egui::Id::unique("binder-inside-subfolders");
    let mut deep = ui.data(|d| d.get_temp::<bool>(key)).unwrap_or(false);
    let has_subfolders = node.children.iter().any(|c| !c.children.is_empty());
    let subfolders = |ui: &mut egui::Ui, deep: &mut bool| {
        if has_subfolders {
            ui.checkbox(deep, "Include subfolders").on_hover_text("Also what's inside the folders in it");
            ui.separator();
        }
    };
    // Ticking Include subfolders leaves the menu open; picking closes it.
    let submenu = |title: &'static str| {
        let stay = egui::containers::menu::MenuConfig::new().close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
        egui::containers::menu::SubMenuButton::new(title).config(stay)
    };
    submenu("Set Status for All Inside").ui(ui, |ui| {
        subfolders(ui, &mut deep);
        let items = inside(node, deep);
        let all = |value: Option<usize>| items.iter().all(|n| n.meta.status == value);
        for (i, name) in statuses.iter().enumerate() {
            if ui.add(egui::Button::selectable(all(Some(i)), name.as_str())).clicked() {
                actions.push(BinderAction::StatusInside { id: node.id.clone(), status: Some(i), deep });
                ui.close();
            }
        }
        ui.separator();
        if ui.add(egui::Button::selectable(all(None), "No Status")).clicked() {
            actions.push(BinderAction::StatusInside { id: node.id.clone(), status: None, deep });
            ui.close();
        }
    });
    submenu("Set Label for All Inside").ui(ui, |ui| {
        subfolders(ui, &mut deep);
        let items = inside(node, deep);
        let all = |value: Option<usize>| items.iter().all(|n| n.meta.label == value);
        for (i, label) in labels.iter().enumerate() {
            let text = egui::RichText::new(format!("\u{25cf} {}", label.name)).color(label_color(label.color));
            if ui.add(egui::Button::selectable(all(Some(i)), text)).clicked() {
                actions.push(BinderAction::LabelInside { id: node.id.clone(), label: Some(i), deep });
                ui.close();
            }
        }
        ui.separator();
        if ui.add(egui::Button::selectable(all(None), "No Label")).clicked() {
            actions.push(BinderAction::LabelInside { id: node.id.clone(), label: None, deep });
            ui.close();
        }
    });
    if let Some(default) = default_target {
        submenu("Set Target for All Inside").ui(ui, |ui| {
            subfolders(ui, &mut deep);
            let docs: Vec<&Node> = inside(node, deep).into_iter().filter(|n| n.is_document()).collect();
            match default {
                Some(target) => {
                    let words = crate::stats::thousands(target);
                    let missing = docs.iter().filter(|n| n.meta.target_words.is_none()).count();
                    let fill = egui::Button::new(format!("Where There\u{2019}s None: {words} Words"));
                    if ui.add_enabled(missing > 0, fill).on_hover_text("The project's default target, for the documents that don't have one yet").clicked() {
                        actions.push(BinderAction::TargetInside { id: node.id.clone(), target: Some(target), only_missing: true, deep });
                        ui.close();
                    }
                    let all = docs.iter().all(|n| n.meta.target_words == Some(target));
                    if ui.add(egui::Button::selectable(all, format!("All: {words} Words"))).on_hover_text("The project's default target, for every document, replacing any they have").clicked() {
                        actions.push(BinderAction::TargetInside { id: node.id.clone(), target: Some(target), only_missing: false, deep });
                        ui.close();
                    }
                }
                None => {
                    ui.weak(sloth_core::keys::shown("Set a default target in Project Settings (\u{2325}\u{2318},)"));
                }
            }
            ui.separator();
            let none = docs.iter().all(|n| n.meta.target_words.is_none());
            if ui.add(egui::Button::selectable(none, "No Target")).clicked() {
                actions.push(BinderAction::TargetInside { id: node.id.clone(), target: None, only_missing: false, deep });
                ui.close();
            }
        });
    }
    ui.data_mut(|d| d.insert_temp(key, deep));
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
pub(crate) fn drag_ghost(ctx: &egui::Context, label: &str) {
    let Some(pointer) = ctx.pointer_interact_pos() else { return };
    egui::Area::new(egui::Id::unique("binder-drag-ghost"))
        .order(egui::Order::Tooltip)
        .fixed_pos(pointer + egui::vec2(14.0, 6.0))
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| ui.label(label));
        });
}
