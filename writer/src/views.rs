//! What a folder shows when selected: its items as index cards on a
//! corkboard, or as rows in an outline. Both edit the same metadata as the
//! inspector, and items can be dragged to reorder them, here or between a
//! view and the binder (they share the binder's drag payload).

use std::collections::HashMap;

use eframe::egui::{self, Color32, CornerRadius, RichText, Sense, Stroke, StrokeKind, UiBuilder, Vec2};

use super::WriterApp;
use super::inspector::label_color;
use crate::binder::{BinderAction, Dragged, drag_ghost};
use crate::project::{Label, Meta, Node, Place};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum FolderView {
    #[default]
    Corkboard,
    Outline,
}

/// Which view each folder was last shown in, for this session.
pub(super) type FolderViews = HashMap<String, FolderView>;

const CARD_SIZE: Vec2 = Vec2::new(220.0, 150.0);
const CARD_GAP: f32 = 16.0;

/// One item as a view shows it, copied out of the project so the view can
/// be drawn while the project changes underneath.
struct Item {
    id: String,
    title: String,
    is_document: bool,
    has_children: bool,
    /// Nesting below the folder being shown (outline only).
    depth: usize,
    meta: Meta,
    words: usize,
}

/// What the person did in a view; applied once it's drawn.
enum ViewAction {
    Open(String),
    Meta(String, Meta),
    Move { id: String, place: Place },
}

impl WriterApp {
    /// Show the selected folder's items, with a switch between the views.
    pub(super) fn folder_view(&mut self, ui: &mut egui::Ui, folder: &str) {
        let mode = self.folder_views.get(folder).copied().unwrap_or_default();
        ui.horizontal(|ui| {
            let mut chosen = mode;
            ui.selectable_value(&mut chosen, FolderView::Corkboard, "Corkboard");
            ui.selectable_value(&mut chosen, FolderView::Outline, "Outline");
            if chosen != mode {
                self.folder_views.insert(folder.to_owned(), chosen);
            }
        });
        ui.add_space(8.0);

        let items = self.view_items(folder, mode == FolderView::Outline);
        if items.is_empty() {
            ui.weak("This folder is empty. Add a document to it with \u{2318}N.");
            return;
        }
        let (statuses, labels) = match &self.project {
            Some(p) => (p.manifest.statuses.clone(), p.manifest.labels.clone()),
            None => return,
        };
        let dragging = egui::DragAndDrop::payload::<Dragged>(ui.ctx()).map(|d| d.0.clone());
        let mut actions = Vec::new();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match mode {
            FolderView::Corkboard => corkboard(ui, &items, &statuses, &labels, dragging.as_deref(), &mut actions),
            FolderView::Outline => outline(ui, &items, &statuses, &labels, dragging.as_deref(), &mut actions),
        });

        for action in actions {
            match action {
                ViewAction::Open(id) => self.select(id),
                ViewAction::Meta(id, meta) => {
                    if let Some(project) = self.project.as_mut()
                        && project.set_meta(&id, meta)
                    {
                        self.manifest_dirty = true;
                    }
                }
                ViewAction::Move { id, place } => self.apply(BinderAction::Move { id, place }),
            }
        }
    }

    /// The folder's items: its children, or for the outline everything
    /// under it, in order.
    fn view_items(&mut self, folder: &str, nested: bool) -> Vec<Item> {
        let Some(node) = self.project.as_ref().and_then(|p| p.node(folder)) else { return Vec::new() };
        let mut found = Vec::new();
        collect(&node.children, 0, nested, &mut found);
        // Counted afterwards: counting needs `self` mutably (its cache).
        let mut items: Vec<Item> = found
            .into_iter()
            .map(|(node, depth)| Item {
                id: node.id.clone(),
                title: node.title.clone(),
                is_document: node.is_document(),
                has_children: !node.children.is_empty(),
                depth,
                meta: node.meta.clone(),
                words: 0,
            })
            .collect();
        for item in &mut items {
            item.words = self.words_under(&item.id);
        }
        items
    }
}

fn collect<'a>(nodes: &'a [Node], depth: usize, nested: bool, out: &mut Vec<(&'a Node, usize)>) {
    for node in nodes {
        out.push((node, depth));
        if nested {
            collect(&node.children, depth + 1, nested, out);
        }
    }
}

// ----- corkboard -------------------------------------------------------------

fn corkboard(
    ui: &mut egui::Ui,
    items: &[Item],
    statuses: &[String],
    labels: &[Label],
    dragging: Option<&str>,
    actions: &mut Vec<ViewAction>,
) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::splat(CARD_GAP);
        for item in items {
            card(ui, item, statuses, labels, dragging, actions);
        }
    });
}

/// Room kept at the bottom of a card for its status and words.
const FOOTER_HEIGHT: f32 = 16.0;

fn card(
    ui: &mut egui::Ui,
    item: &Item,
    statuses: &[String],
    labels: &[Label],
    dragging: Option<&str>,
    actions: &mut Vec<ViewAction>,
) {
    // The card's background takes clicks and drags; the synopsis field,
    // added on top, takes its own clicks. The title isn't selectable text,
    // so pressing on it drags the card.
    let (rect, response) = ui.allocate_exact_size(CARD_SIZE, Sense::click_and_drag());
    let visuals = ui.visuals();
    let fill = if visuals.dark_mode { Color32::from_gray(48) } else { Color32::from_rgb(0xFD, 0xFB, 0xF4) };
    let painter = ui.painter_at(rect);
    painter.rect(rect, CornerRadius::same(6), fill, visuals.widgets.noninteractive.bg_stroke, StrokeKind::Inside);
    // The label's colour, as a strip along the top.
    if let Some(label) = item.meta.label.and_then(|i| labels.get(i)) {
        let strip = egui::Rect::from_min_size(rect.min, Vec2::new(rect.width(), 6.0));
        painter.rect_filled(strip, CornerRadius { nw: 6, ne: 6, sw: 0, se: 0 }, label_color(label.color));
    }

    // Laid out on its own, top down, so it neither shifts the other cards
    // nor takes the row's layout.
    let inner = rect.shrink2(Vec2::new(10.0, 12.0));
    let mut contents = ui.new_child(UiBuilder::new().max_rect(inner).layout(egui::Layout::top_down(egui::Align::Min)));
    {
        let ui = &mut contents;
        ui.spacing_mut().item_spacing = Vec2::new(6.0, 4.0);
        let icon = if item.is_document { "\u{1F4C4}" } else { "\u{1F4C1}" };
        ui.add(egui::Label::new(RichText::new(format!("{icon} {}", item.title)).strong()).truncate().selectable(false));
        // Nothing drawn past the card's edge…
        ui.set_clip_rect(rect.intersect(ui.clip_rect()));
        let mut synopsis = item.meta.synopsis.clone();
        // …and a long synopsis scrolls inside the card, above the footer.
        let room = (ui.available_height() - FOOTER_HEIGHT).max(20.0);
        let field = egui::ScrollArea::vertical()
            .id_salt(("card-synopsis-scroll", &item.id))
            .max_height(room)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut synopsis)
                        .id_salt(("card-synopsis", &item.id))
                        .hint_text("Synopsis")
                        .frame(egui::Frame::NONE)
                        .desired_rows(4)
                        .desired_width(f32::INFINITY),
                )
            })
            .inner;
        if field.changed() {
            actions.push(ViewAction::Meta(item.id.clone(), Meta { synopsis, ..item.meta.clone() }));
        }
    }
    // Status and words along the bottom.
    let status = item.meta.status.and_then(|i| statuses.get(i)).map_or("", String::as_str);
    let footer = format!("{}{}{} words", status, if status.is_empty() { "" } else { "  \u{b7}  " }, item.words);
    painter.text(
        rect.left_bottom() + Vec2::new(10.0, -8.0),
        egui::Align2::LEFT_BOTTOM,
        footer,
        egui::FontId::proportional(11.0),
        ui.visuals().weak_text_color(),
    );

    if response.double_clicked() {
        actions.push(ViewAction::Open(item.id.clone()));
    }
    drag_and_drop(ui, &response, item, dragging, true, actions);
}

// ----- outline ---------------------------------------------------------------

/// Column widths, after the title (which takes what's left).
const STATUS_W: f32 = 120.0;
const LABEL_W: f32 = 130.0;
const WORDS_W: f32 = 90.0;

fn outline(
    ui: &mut egui::Ui,
    items: &[Item],
    statuses: &[String],
    labels: &[Label],
    dragging: Option<&str>,
    actions: &mut Vec<ViewAction>,
) {
    let row_height = 44.0;
    let columns = STATUS_W + LABEL_W + WORDS_W;
    // Headings.
    ui.horizontal(|ui| {
        let title_w = (ui.available_width() - columns).max(160.0);
        for (text, width) in [("Title and synopsis", title_w), ("Status", STATUS_W), ("Label", LABEL_W), ("Words", WORDS_W)] {
            ui.allocate_ui_with_layout(Vec2::new(width, 18.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.set_width(width);
                ui.label(RichText::new(text).small().weak());
            });
        }
    });
    ui.separator();

    // Items inside the one being dragged can't take it.
    let mut inside_depth: Option<usize> = None;
    for item in items {
        if inside_depth.is_some_and(|d| item.depth <= d) {
            inside_depth = None;
        }
        if inside_depth.is_none() && dragging == Some(item.id.as_str()) {
            inside_depth = Some(item.depth);
        }
        let blocked = inside_depth.is_some();

        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), row_height), Sense::click_and_drag());
        if response.hovered() {
            ui.painter().rect_filled(rect, CornerRadius::same(4), ui.visuals().widgets.hovered.weak_bg_fill);
        }
        let mut row = ui.new_child(UiBuilder::new().max_rect(rect).layout(egui::Layout::left_to_right(egui::Align::Center)));
        let title_w = (rect.width() - columns).max(160.0);
        let indent = 18.0 * item.depth as f32;
        let mut meta = item.meta.clone();

        row.allocate_ui(Vec2::new(title_w, row_height), |ui| {
            ui.add_space(indent);
            ui.vertical(|ui| {
                let icon = match (item.is_document, item.has_children) {
                    (true, _) => "\u{1F4C4}",
                    (false, _) => "\u{1F4C1}",
                };
                ui.add(egui::Label::new(RichText::new(format!("{icon} {}", item.title)).strong()).truncate().selectable(false));
                ui.add(
                    egui::TextEdit::singleline(&mut meta.synopsis)
                        .id_salt(("row-synopsis", &item.id))
                        .hint_text("Synopsis")
                        .frame(egui::Frame::NONE)
                        .desired_width(title_w - indent - 12.0),
                );
            });
        });
        row.allocate_ui(Vec2::new(STATUS_W, row_height), |ui| {
            let shown = meta.status.and_then(|i| statuses.get(i)).map_or("\u{2014}", String::as_str);
            egui::ComboBox::from_id_salt(("row-status", &item.id))
                .selected_text(shown)
                .width(STATUS_W - 12.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut meta.status, None, "None");
                    for (i, name) in statuses.iter().enumerate() {
                        ui.selectable_value(&mut meta.status, Some(i), name);
                    }
                });
        });
        row.allocate_ui(Vec2::new(LABEL_W, row_height), |ui| {
            let shown = match meta.label.and_then(|i| labels.get(i)) {
                Some(l) => RichText::new(format!("\u{25cf} {}", l.name)).color(label_color(l.color)),
                None => RichText::new("\u{2014}"),
            };
            egui::ComboBox::from_id_salt(("row-label", &item.id))
                .selected_text(shown)
                .width(LABEL_W - 12.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut meta.label, None, "None");
                    for (i, l) in labels.iter().enumerate() {
                        let text = RichText::new(format!("\u{25cf} {}", l.name)).color(label_color(l.color));
                        ui.selectable_value(&mut meta.label, Some(i), text);
                    }
                });
        });
        row.allocate_ui(Vec2::new(WORDS_W, row_height), |ui| {
            match meta.target_words {
                // Green once the target is reached.
                Some(target) if target > 0 && item.words >= target as usize => {
                    ui.label(RichText::new(format!("{} / {target}", item.words)).color(label_color(3)))
                }
                Some(target) if target > 0 => ui.label(format!("{} / {target}", item.words)),
                _ => ui.label(format!("{}", item.words)),
            };
        });

        if meta != item.meta {
            actions.push(ViewAction::Meta(item.id.clone(), meta));
        }
        if response.double_clicked() {
            actions.push(ViewAction::Open(item.id.clone()));
        }
        if !blocked {
            drag_and_drop(ui, &response, item, dragging, false, actions);
        } else if response.dragged() {
            drag_ghost(ui.ctx(), &item.title);
        }
    }
}

// ----- dragging ----------------------------------------------------------------

/// Make an item draggable, and a place to drop others. On the corkboard
/// (`cards`), the left or right half of a card means before or after it.
/// In the outline, as in the binder, the top or bottom quarter of a row
/// means before or after, and the middle means inside.
fn drag_and_drop(
    ui: &egui::Ui,
    response: &egui::Response,
    item: &Item,
    dragging: Option<&str>,
    cards: bool,
    actions: &mut Vec<ViewAction>,
) {
    response.dnd_set_drag_payload(Dragged(item.id.clone()));
    if response.dragged() {
        drag_ghost(ui.ctx(), &item.title);
    }
    if dragging == Some(item.id.as_str()) || response.dnd_hover_payload::<Dragged>().is_none() {
        return;
    }
    let Some(pointer) = ui.ctx().pointer_interact_pos() else { return };
    let rect = response.rect;
    let stroke = Stroke::new(2.0, ui.visuals().selection.stroke.color);
    let painter = ui.painter();
    let id = item.id.clone();
    let place = if cards {
        if pointer.x < rect.center().x {
            painter.vline(rect.left() - CARD_GAP / 2.0, rect.y_range(), stroke);
            Place::Before(id)
        } else {
            painter.vline(rect.right() + CARD_GAP / 2.0, rect.y_range(), stroke);
            Place::After(id)
        }
    } else {
        let fraction = (pointer.y - rect.top()) / rect.height().max(1.0);
        if fraction < 0.25 {
            painter.hline(rect.x_range(), rect.top(), stroke);
            Place::Before(id)
        } else if fraction > 0.75 {
            painter.hline(rect.x_range(), rect.bottom(), stroke);
            Place::After(id)
        } else {
            painter.rect_stroke(rect, 4.0, stroke, StrokeKind::Inside);
            Place::Into(id)
        }
    };
    if let Some(dragged) = response.dnd_release_payload::<Dragged>() {
        actions.push(ViewAction::Move { id: dragged.0.clone(), place });
    }
}
