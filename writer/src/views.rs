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
    /// The folder's documents as one text (see slothenings_ui.rs).
    Slothenings,
}

/// Which view each folder was last shown in, for this session.
pub(super) type FolderViews = HashMap<String, FolderView>;

const CARD_SIZE: Vec2 = Vec2::new(220.0, 150.0);
const CARD_GAP: f32 = 16.0;

/// One item as a view shows it, copied out of the project so the view can
/// be drawn while the project changes underneath.
pub(super) struct Item {
    id: String,
    pub(super) title: String,
    is_document: bool,
    /// Nesting below the folder being shown (outline only).
    depth: usize,
    meta: Meta,
    words: usize,
    /// A document's first words, shown on its card or row in place of a
    /// synopsis it doesn't have.
    pub(super) opening: String,
    /// Its web address, if it has one that opens (where it's published).
    url: Option<String>,
}

impl Item {
    /// An item as copied from the binder, before its words and opening.
    fn of(node: &Node, depth: usize) -> Self {
        Item {
            id: node.id.clone(),
            title: node.title.clone(),
            is_document: node.is_document(),
            depth,
            meta: node.meta.clone(),
            words: 0,
            opening: String::new(),
            url: super::inspector::web_address(&node.meta.url),
        }
    }
}

/// What the person did in a view; applied once it's drawn.
enum ViewAction {
    Open(String),
    Meta(String, Meta),
    Move { id: String, place: Place },
    /// Its web mark was clicked: open this address.
    OpenUrl(String),
}

impl WriterApp {
    /// Show the selected folder's items, with a switch between the views.
    pub(super) fn folder_view(&mut self, ui: &mut egui::Ui, folder: &str) {
        let mode = self.folder_views.get(folder).copied().unwrap_or_default();
        ui.horizontal(|ui| {
            let mut chosen = mode;
            ui.selectable_value(&mut chosen, FolderView::Slothenings, "Slothenings").on_hover_text("Its documents as one text (\u{2318}1)");
            ui.selectable_value(&mut chosen, FolderView::Corkboard, "Corkboard").on_hover_text("\u{2318}2");
            ui.selectable_value(&mut chosen, FolderView::Outline, "Outline").on_hover_text("\u{2318}3");
            if chosen != mode {
                self.folder_views.insert(folder.to_owned(), chosen);
            }
            // Only what has a keyword (not for Slothenings, a single text).
            if mode != FolderView::Slothenings {
                self.keyword_filter(ui, folder);
            }
        });
        ui.add_space(8.0);

        let keyword = self.folder_keywords.get(folder).cloned().filter(|_| mode != FolderView::Slothenings);
        let items = match &keyword {
            Some(keyword) => self.keyword_items(folder, keyword),
            None => self.view_items(folder, mode != FolderView::Corkboard),
        };
        if items.is_empty() && keyword.is_some() {
            ui.weak("Nothing in this folder has that keyword now.");
            return;
        }
        if items.is_empty() {
            let trash = self.project.as_ref().and_then(|p| p.node(folder)).is_some_and(|n| n.role == Some(crate::project::Role::Trash));
            if trash {
                self.empty_state(ui, super::sloth_ui::Pose::Asleep, "The Trash is empty", "What you move to the Trash waits here until you empty it.");
            } else {
                self.empty_state(ui, super::sloth_ui::Pose::Reading, "This folder is empty", "Add a document to it with \u{2318}N, or drag one here from the binder.");
            }
            return;
        }
        let (statuses, labels) = match &self.project {
            Some(p) => (p.manifest.statuses.clone(), p.manifest.labels.clone()),
            None => return,
        };
        let dragging = egui::DragAndDrop::payload::<Dragged>(ui.ctx()).map(|d| d.0.clone());
        let mut actions = Vec::new();
        if mode == FolderView::Slothenings {
            self.slothenings(ui, folder);
            return;
        }
        // Filtered, the order shown isn't the binder's: no reordering.
        let reorder = keyword.is_none();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match mode {
            FolderView::Corkboard => corkboard(ui, &items, &statuses, &labels, dragging.as_deref(), reorder, &mut actions),
            _ => outline(ui, &items, &statuses, &labels, dragging.as_deref(), reorder, &mut actions),
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
                ViewAction::OpenUrl(url) => super::inspector::open_in_browser(&url),
            }
        }
    }

    /// The keyword menu above a folder's corkboard or outline: the keywords
    /// used in it (subfolders and all), with how many have each.
    fn keyword_filter(&mut self, ui: &mut egui::Ui, folder: &str) {
        let Some(node) = self.project.as_ref().and_then(|p| p.node(folder)) else { return };
        let counts = keyword_counts(node);
        let current = self.folder_keywords.get(folder).cloned();
        if counts.is_empty() && current.is_none() {
            return;
        }
        ui.add_space(12.0);
        let mut picked = current.clone();
        let shown = current.as_deref().map_or("All keywords".to_owned(), |k| format!("Keyword: {k}"));
        ui.menu_button(shown, |ui| {
            if ui.selectable_label(picked.is_none(), "All keywords").clicked() {
                picked = None;
                ui.close();
            }
            ui.separator();
            for (keyword, count) in &counts {
                let on = picked.as_deref().is_some_and(|p| p.eq_ignore_ascii_case(keyword));
                if ui.selectable_label(on, format!("{keyword}  ({count})")).clicked() {
                    picked = Some(keyword.clone());
                    ui.close();
                }
            }
        })
        .response
        .on_hover_text("Show only what has a keyword, from anywhere in this folder");
        if picked != current {
            match picked {
                Some(k) => self.folder_keywords.insert(folder.to_owned(), k),
                None => self.folder_keywords.remove(folder),
            };
        }
    }

    /// Everything under a folder with a keyword (any case), in binder order.
    pub(super) fn keyword_items(&mut self, folder: &str, keyword: &str) -> Vec<Item> {
        let Some(node) = self.project.as_ref().and_then(|p| p.node(folder)) else { return Vec::new() };
        let items: Vec<Item> = node
            .subtree()
            .into_iter()
            .skip(1)
            .filter(|n| n.meta.keywords.iter().any(|k| k.eq_ignore_ascii_case(keyword)))
            .map(|n| Item::of(n, 0))
            .collect();
        self.counted(items)
    }

    /// The folder's items: its children, or for the outline everything
    /// under it, in order.
    pub(super) fn view_items(&mut self, folder: &str, nested: bool) -> Vec<Item> {
        let Some(node) = self.project.as_ref().and_then(|p| p.node(folder)) else { return Vec::new() };
        let mut found = Vec::new();
        collect(&node.children, 0, nested, &mut found);
        let items = found.into_iter().map(|(n, depth)| Item::of(n, depth)).collect();
        self.counted(items)
    }

    /// Fill in items' words and openings (afterwards: counting needs
    /// `self` mutably, for its cache).
    fn counted(&mut self, mut items: Vec<Item>) -> Vec<Item> {
        for item in &mut items {
            item.words = self.words_under(&item.id);
            if item.is_document && item.meta.synopsis.trim().is_empty() {
                item.opening = self.opening_of(&item.id);
            }
        }
        items
    }
}

/// The keywords used in a folder (subfolders and all), each with how many
/// items have it, by name (any case counts as one, shown as first met).
fn keyword_counts(folder: &Node) -> Vec<(String, usize)> {
    let mut counts: Vec<(String, usize)> = Vec::new();
    for node in folder.subtree().into_iter().skip(1) {
        for keyword in &node.meta.keywords {
            match counts.iter_mut().find(|(k, _)| k.eq_ignore_ascii_case(keyword)) {
                Some((_, n)) => *n += 1,
                None => counts.push((keyword.clone(), 1)),
            }
        }
    }
    counts.sort_by_key(|(k, _)| k.to_lowercase());
    counts
}

/// An item's title, bold and cut short to fit, with a small globe at the
/// right if it has a web address (where it's published): click that to
/// open it in the browser.
fn title_row(ui: &mut egui::Ui, item: &Item, actions: &mut Vec<ViewAction>) {
    let icon = if item.is_document { "\u{1F4C4}" } else { "\u{1F4C1}" };
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if let Some(url) = &item.url {
                let mark = egui::Button::new("\u{1F310}").frame(false);
                if ui.add(mark).on_hover_text(format!("Open {url}")).clicked() {
                    actions.push(ViewAction::OpenUrl(url.clone()));
                }
            }
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.add(egui::Label::new(RichText::new(format!("{icon} {}", item.title)).strong()).truncate().selectable(false));
            });
        });
    });
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
    reorder: bool,
    actions: &mut Vec<ViewAction>,
) {
    dot_grid(ui, ui.max_rect().min, GRID_STEP, 1.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::splat(CARD_GAP);
        for item in items {
            card(ui, item, statuses, labels, dragging, reorder, actions);
        }
    });
}

/// A status's colour, by where it is in the project's list of statuses:
/// warm at the start (just begun), through blue, to green at the end (done).
pub(super) fn status_color(index: usize, count: usize) -> Color32 {
    let stops = [Color32::from_rgb(0xd0, 0x7a, 0x45), Color32::from_rgb(0x5b, 0x8b, 0xd0), Color32::from_rgb(0x4c, 0xa0, 0x66)];
    let t = if count <= 1 { 0.0 } else { index as f32 / (count - 1) as f32 };
    if t <= 0.5 { stops[0].lerp_to_gamma(stops[1], t * 2.0) } else { stops[1].lerp_to_gamma(stops[2], (t - 0.5) * 2.0) }
}

/// A status as a small stamp, tipped a little, with its bottom right
/// corner at `corner`.
pub(super) fn stamp(painter: &egui::Painter, corner: egui::Pos2, text: &str, color: Color32) {
    let galley = painter.layout_no_wrap(text.to_uppercase(), egui::FontId::proportional(8.5), color);
    let size = galley.size() + Vec2::new(10.0, 4.0);
    let angle: f32 = -0.07;
    let (sin, cos) = angle.sin_cos();
    let turn = |v: Vec2| Vec2::new(v.x * cos - v.y * sin, v.x * sin + v.y * cos);
    let center = corner - size / 2.0;
    let corners: Vec<egui::Pos2> =
        [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].iter().map(|&(x, y)| center + turn(Vec2::new(x * size.x, y * size.y) / 2.0)).collect();
    painter.add(egui::Shape::convex_polygon(corners, color.gamma_multiply(0.12), Stroke::new(1.0, color.gamma_multiply(0.85))));
    let at = center + turn(-galley.size() / 2.0);
    painter.add(egui::epaint::TextShape::new(at, galley, color).with_angle(angle));
}

/// Spacing of the board's dots.
const GRID_STEP: f32 = 24.0;

/// Faint dots behind the cards, a surface to put things on, `step` apart.
/// Anchored at `origin` so they move with the cards, and only the ones in
/// view are drawn. (The canvas passes a bigger step and radius when it's
/// zoomed out, so they stay a dot apart on screen.)
pub(super) fn dot_grid(ui: &egui::Ui, origin: egui::Pos2, step: f32, radius: f32) {
    let view = ui.clip_rect();
    let visuals = ui.visuals();
    let alpha = if visuals.dark_mode { 0.22 } else { 0.28 };
    let color = visuals.weak_text_color().gamma_multiply(alpha);
    let painter = ui.painter();
    // Half a step in from the corner, so no dot sits on a card's edge. The
    // canvas has no corner: dots go up and left of it too.
    let first = |o: f32, v: f32| o + step / 2.0 + ((v - o) / step).floor() * step;
    let mut y = first(origin.y, view.top());
    while y <= view.bottom() {
        let mut x = first(origin.x, view.left());
        while x <= view.right() {
            painter.circle_filled(egui::pos2(x, y), radius, color);
            x += step;
        }
        y += step;
    }
}

/// What an empty synopsis field shows: the document's opening, faint and
/// in italics, until a synopsis is written; else just "Synopsis".
fn synopsis_hint(ui: &egui::Ui, opening: &str) -> RichText {
    if opening.is_empty() {
        RichText::new("Synopsis")
    } else {
        RichText::new(opening).italics().color(opening_color(ui))
    }
}

/// The colour of a document's opening shown in place of a synopsis.
pub(super) fn opening_color(ui: &egui::Ui) -> Color32 {
    ui.visuals().weak_text_color().gamma_multiply(0.8)
}

/// Room kept at the bottom of a card for its status and words.
const FOOTER_HEIGHT: f32 = 16.0;

fn card(
    ui: &mut egui::Ui,
    item: &Item,
    statuses: &[String],
    labels: &[Label],
    dragging: Option<&str>,
    reorder: bool,
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
        title_row(ui, item, actions);
        // Nothing drawn past the card's edge…
        ui.set_clip_rect(rect.intersect(ui.clip_rect()));
        let mut synopsis = item.meta.synopsis.clone();
        let hint = synopsis_hint(ui, &item.opening);
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
                        .hint_text(hint)
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
    // Words along the bottom, and the status as a stamp in the corner.
    painter.text(
        rect.left_bottom() + Vec2::new(10.0, -8.0),
        egui::Align2::LEFT_BOTTOM,
        format!("{} words", item.words),
        egui::FontId::proportional(11.0),
        ui.visuals().weak_text_color(),
    );
    if let Some(i) = item.meta.status.filter(|&i| i < statuses.len()) {
        stamp(&painter, rect.right_bottom() + Vec2::new(-9.0, -7.0), &statuses[i], status_color(i, statuses.len()));
    }

    if response.double_clicked() {
        actions.push(ViewAction::Open(item.id.clone()));
    }
    if reorder {
        drag_and_drop(ui, &response, item, dragging, true, actions);
    }
}

// ----- outline ---------------------------------------------------------------

/// An outline row's height, and the space between rows.
const ROW_HEIGHT: f32 = 58.0;
/// Space above a row's title: the title and synopsis (about 36 points)
/// sit in the middle of the row, clear of its top edge.
const ROW_TOP: f32 = 8.0;
const ROW_GAP: f32 = 4.0;
/// How far each level of nesting steps a row in.
const INDENT: f32 = 22.0;
/// The room at a row's left before its title: the handle and a gap.
const GRIP_W: f32 = 32.0;
/// The handle: an end cap on the row in its label's colour, to drag it by,
/// rounded on the outside.
const HANDLE_W: f32 = 18.0;
const HANDLE_RADIUS: u8 = 6;
/// Thin lines in the label's colour run on from the cap along the row's
/// top and bottom, fading out this far across.
const EDGE_REACH: f32 = 0.25;
const EDGE_WIDTH: f32 = 1.5;

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
    reorder: bool,
    actions: &mut Vec<ViewAction>,
) {
    let row_height = ROW_HEIGHT;
    let columns = STATUS_W + LABEL_W + WORDS_W;
    // Headings, lined up with the rows' columns.
    ui.horizontal(|ui| {
        ui.add_space(GRIP_W);
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
    for (n, item) in items.iter().enumerate() {
        if n > 0 {
            ui.add_space(ROW_GAP);
        }
        if inside_depth.is_some_and(|d| item.depth <= d) {
            inside_depth = None;
        }
        if inside_depth.is_none() && dragging == Some(item.id.as_str()) {
            inside_depth = Some(item.depth);
        }
        let blocked = inside_depth.is_some();

        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), row_height), Sense::click_and_drag());
        // Nested, the whole row steps in (its handle with it), as in the
        // binder; the columns at the right stay lined up.
        let indent = INDENT * item.depth as f32;
        let shown = egui::Rect::from_min_max(rect.min + Vec2::new(indent, 0.0), rect.max);
        if response.hovered() {
            ui.painter().rect_filled(shown, CornerRadius::same(HANDLE_RADIUS), ui.visuals().widgets.hovered.weak_bg_fill);
        }
        // Its handle, in its label's colour: to drag it by (the whole row
        // drags; the handle says so), and to see its label at a glance.
        let color = item.meta.label.and_then(|i| labels.get(i)).map(|l| label_color(l.color));
        handle(ui, shown, &response, color, reorder && !blocked);
        let mut row = ui.new_child(UiBuilder::new().max_rect(rect).layout(egui::Layout::left_to_right(egui::Align::Center)));
        let title_w = (rect.width() - columns - GRIP_W).max(160.0 + indent);
        row.add_space(indent + GRIP_W);
        let mut meta = item.meta.clone();

        row.allocate_ui(Vec2::new(title_w - indent, row_height), |ui| {
            ui.vertical(|ui| {
                ui.set_width(title_w - indent - 12.0);
                ui.add_space(ROW_TOP);
                title_row(ui, item, actions);
                let hint = synopsis_hint(ui, &item.opening);
                ui.add(
                    egui::TextEdit::singleline(&mut meta.synopsis)
                        .id_salt(("row-synopsis", &item.id))
                        .hint_text(hint)
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
            if reorder {
                drag_and_drop(ui, &response, item, dragging, false, actions);
            }
        } else if response.dragged() {
            drag_ghost(ui.ctx(), &item.title);
        }
    }
}

// ----- dragging ----------------------------------------------------------------

/// A row's handle: an end cap on its left in its label's colour (a quiet
/// grey with none), with thin lines running on along the row's top and
/// bottom that fade out a quarter of the way across. When it can be
/// dragged, the cap carries grip dots and a hand under the pointer, and
/// shows its colour fully under the pointer or while carried.
fn handle(ui: &egui::Ui, row: egui::Rect, response: &egui::Response, label: Option<Color32>, draggable: bool) {
    let area = egui::Rect::from_min_size(row.min, Vec2::new(GRIP_W, row.height()));
    let over = draggable && ui.ctx().pointer_hover_pos().is_some_and(|p| area.contains(p));
    if draggable && response.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    } else if over {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }
    let visuals = ui.visuals();
    let lively = over || response.dragged();
    let fill = match label {
        Some(color) if lively => color,
        Some(color) => color.gamma_multiply(0.85),
        None if lively => visuals.weak_text_color().gamma_multiply(0.7),
        None => visuals.weak_text_color().gamma_multiply(0.35),
    };
    let painter = ui.painter();
    let cap = egui::Rect::from_min_size(row.min, Vec2::new(HANDLE_W, row.height()));
    let r = HANDLE_RADIUS;
    painter.rect_filled(cap, CornerRadius { nw: r, sw: r, ne: 0, se: 0 }, fill);
    // The edges, fading from the cap's colour to nothing.
    let reach = (row.width() * EDGE_REACH).max(HANDLE_W);
    for y in [row.top(), row.bottom() - EDGE_WIDTH] {
        let edge = egui::Rect::from_min_max(egui::pos2(cap.right(), y), egui::pos2(cap.right() + reach, y + EDGE_WIDTH));
        fade_right(painter, edge, fill);
    }
    if draggable {
        // Dots that show on the cap, whatever its colour.
        let dots = if visuals.dark_mode { Color32::from_black_alpha(110) } else { Color32::from_white_alpha(200) };
        for dx in [-3.0, 3.0] {
            for dy in [-6.5, 0.0, 6.5] {
                painter.circle_filled(cap.center() + Vec2::new(dx, dy), 1.5, dots);
            }
        }
    }
}

/// Fill a rect with a colour that fades out from left to right.
fn fade_right(painter: &egui::Painter, rect: egui::Rect, color: Color32) {
    let mut mesh = egui::Mesh::default();
    let clear = Color32::TRANSPARENT;
    mesh.colored_vertex(rect.left_top(), color);
    mesh.colored_vertex(rect.right_top(), clear);
    mesh.colored_vertex(rect.right_bottom(), clear);
    mesh.colored_vertex(rect.left_bottom(), color);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(mesh);
}

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


/// The folder (or document with children) an item is in.
fn parent_of(nodes: &[Node], id: &str) -> Option<String> {
    nodes.iter().find_map(|n| {
        if n.children.iter().any(|c| c.id == id) { Some(n.id.clone()) } else { parent_of(&n.children, id) }
    })
}

impl WriterApp {
    /// ⌘1–3: show a folder (the selected one, or the one the selected
    /// document is in) this way. Into Slothenings from a document, the
    /// cursor stays where it was.
    pub(super) fn show_folder_as(&mut self, ctx: &egui::Context, view: FolderView) {
        let Some(selected) = self.selected.clone() else { return };
        let Some(project) = &self.project else { return };
        let is_folder = project.node(&selected).is_some_and(|n| !n.is_document());
        let folder = if is_folder { Some(selected.clone()) } else { parent_of(&project.manifest.binder, &selected) };
        let Some(folder) = folder else { return };
        if !is_folder && view == FolderView::Slothenings
            && let Some(tab) = self.open.get(&selected).copied()
        {
            let at = sloth_core::editor::selection(ctx, tab).map_or(0, |r| r.start);
            self.crossing = Some(super::slothenings_ui::Crossing::to(selected.clone(), at));
        }
        self.folder_views.insert(folder.clone(), view);
        if folder != selected {
            self.select(folder);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_go_from_warm_to_green() {
        let first = status_color(0, 5);
        let last = status_color(4, 5);
        assert!(first.r() > first.g(), "warm at the start");
        assert!(last.g() > last.r() && last.g() > last.b(), "green when done");
        assert_eq!(status_color(0, 1), first, "a single status is the first colour");
        assert_eq!(status_color(2, 5), Color32::from_rgb(0x5b, 0x8b, 0xd0), "blue in the middle");
    }
}
