//! The canvas in the window (see canvas.rs): the whole project's documents
//! as cards on one pannable, zoomable surface, with free notes among them,
//! strung to what they refer to and to each other.
//!
//! Click a card to pick it (its strings and neighbours stand out, the rest
//! fades, and the inspector shows it); double-click to open it in the split
//! pane. Drag a card to move it, or the empty canvas to look around; pinch
//! or ⌘-scroll to zoom. Double-click the empty canvas for a note. Drag from
//! a card's pin to another card to tie a string between them, and again to
//! untie it; click a string to pick it, Delete to untie it, right-click to
//! say what it means.
//!
//! Depth (above it) shows only so many levels into each top-level folder:
//! an item at the last level with things inside is one card for them all,
//! and their strings roll up to it (see `canvas::shown` and `roll_up`).
//! Folders above that level are outlined round their cards.

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::Path;

use eframe::egui::{
    self, Color32, CornerRadius, CursorIcon, FontId, Key, Modifiers, Pos2, Rect, RichText, Sense, Stroke, StrokeKind,
    UiBuilder, Vec2, emath::TSTransform, epaint::CubicBezierShape,
};

use super::WriterApp;
use super::inspector::label_color;
use super::views::dot_grid;
use crate::canvas::{self, Canvas, Edge};
use crate::project::{Place, Role};

/// The strings drawn from references and links are gathered again this
/// often, so they keep up with typing.
const REFRESH_SECONDS: f64 = 3.0;

const DOC_SIZE: Vec2 = Vec2::new(200.0, 76.0);
const PROFILE_SIZE: Vec2 = Vec2::new(170.0, 44.0);
const NOTE_SIZE: Vec2 = Vec2::new(200.0, 120.0);
/// The dots' spacing, and what a dropped card's corner snaps to.
const GRID: f32 = 24.0;
/// The strip along a note's top that it's dragged by.
const NOTE_GRIP: f32 = 16.0;
/// How near (on screen) the pointer has to be to a string to pick it.
const STRING_REACH: f32 = 6.0;

pub(super) struct CanvasState {
    data: Canvas,
    dirty: bool,
    /// The part of the canvas in view (egui's Scene keeps it up to date).
    view: Rect,
    /// Fit everything into view on the next frame.
    fit: bool,
    edges: Vec<Edge>,
    edges_at: Option<f64>,
    /// The strings as drawn: rolled up to the cards shown (see `canvas::roll_up`).
    drawn_edges: Vec<Edge>,
    /// Each item's card: its own, or the one standing for it at the depth shown.
    stand_in: HashMap<String, String>,
    /// Folders to outline: title, the cards in them, folders nested inside.
    outlines: Vec<(String, Vec<String>, usize)>,
    /// The card picked: its strings and neighbours stand out.
    focus: Option<String>,
    /// The binder's selection when last seen, so the canvas can follow it.
    seen_selected: Option<String>,
    /// The frame the canvas was last drawn in, to tell when it's opened.
    drawn: u64,
    /// A string being tied, from this card.
    tying: Option<String>,
    /// The string picked (an index into the strings tied by hand).
    picked: Option<usize>,
    /// What the canvas was right-clicked on: a string, or empty canvas
    /// (where a new note would go).
    menu: Option<(Option<usize>, Pos2)>,
    /// Cards just dropped or tied to, and when: their strings sway a moment.
    sway: HashMap<String, f64>,
    /// How far each card is lifted (under the pointer, or carried), as
    /// last drawn, so its strings stay attached.
    lifts: HashMap<String, f32>,
    /// Put the keyboard focus in this note's text on the next frame.
    edit_note: Option<String>,
    /// From the canvas to the screen, as last drawn.
    pub(super) to_screen: TSTransform,
}

impl CanvasState {
    fn new(data: Canvas) -> Self {
        Self {
            data,
            dirty: false,
            view: Rect::ZERO,
            fit: true,
            edges: Vec::new(),
            edges_at: None,
            drawn_edges: Vec::new(),
            stand_in: HashMap::new(),
            outlines: Vec::new(),
            focus: None,
            seen_selected: None,
            drawn: 0,
            tying: None,
            picked: None,
            menu: None,
            edit_note: None,
            sway: HashMap::new(),
            lifts: HashMap::new(),
            to_screen: TSTransform::IDENTITY,
        }
    }

    #[cfg(test)]
    pub(super) fn data(&self) -> &Canvas {
        &self.data
    }

    #[cfg(test)]
    pub(super) fn edges(&self) -> &[Edge] {
        &self.edges
    }

    #[cfg(test)]
    pub(super) fn drawn_edges(&self) -> &[Edge] {
        &self.drawn_edges
    }

    /// Write the canvas out if it's changed.
    pub(super) fn save(&mut self, root: &Path) -> io::Result<()> {
        if self.dirty {
            self.data.save(root)?;
            self.dirty = false;
        }
        Ok(())
    }

    fn new_note(&mut self, center: Pos2) {
        let at = snap(center - NOTE_SIZE / 2.0);
        let id = self.data.add_note([at.x, at.y], "");
        self.focus = Some(id.clone());
        self.edit_note = Some(id);
        self.dirty = true;
    }
}

/// What a card is, as the canvas draws it.
enum Kind {
    /// With its status, as a stamp: the name and its colour. `inside`:
    /// documents in it, shown as part of this card at the depth chosen.
    Document { synopsis: String, label: Option<Color32>, status: Option<(String, Color32)>, inside: usize },
    /// A folder standing for what's in it, at the depth chosen.
    Folder { synopsis: String, label: Option<Color32>, inside: usize },
    /// A profile: its kind ("Character") and the kind's colour.
    Profile { kind: String, color: Color32 },
    Note,
}

struct Card {
    id: String,
    title: String,
    kind: Kind,
    rect: Rect,
}

/// What the person did on the canvas; applied once it's drawn.
enum CanvasAction {
    Select(String),
    OpenSplit(String),
    OpenEditor(String),
    MakeDocument(String),
}

/// The string under the pointer, if any: one tied by hand (by index), or
/// one the project draws.
#[derive(Default)]
struct Hovered {
    tie: Option<usize>,
    edge: Option<usize>,
}

impl WriterApp {
    pub(super) fn toggle_canvas(&mut self) {
        self.canvas_open = !self.canvas_open;
    }

    /// The canvas, in place of the editor.
    pub(super) fn canvas_view(&mut self, ui: &mut egui::Ui) {
        // For a blank canvas (see below).
        let blank = self.sloth(ui.ctx(), super::sloth_ui::Pose::Reading);
        let Some(project) = &self.project else { return };
        let now = ui.input(|i| i.time);
        let mut state = self.canvas.take().unwrap_or_else(|| CanvasState::new(Canvas::load(&project.root)));
        if state.edges_at.is_none_or(|at| now - at >= REFRESH_SECONDS) {
            state.edges = canvas::edges(project, |n| self.document_text(&n.id));
            state.edges_at = Some(now);
        }
        // Picking something in the binder while the canvas is open picks
        // its card. (What was picked before it opened doesn't: everything
        // starts in full view.)
        let frame = ui.ctx().cumulative_pass_nr();
        if state.drawn + 1 < frame {
            state.seen_selected = self.selected.clone();
        }
        state.drawn = frame;
        if self.selected != state.seen_selected {
            state.seen_selected = self.selected.clone();
            if self.selected.is_some() {
                state.focus = self.selected.clone();
            }
        }

        // Every top-level folder outside the Trash, as deep as chosen.
        let depth = state.data.depth;
        let groups: Vec<(String, String, canvas::Shown)> = project
            .manifest
            .binder
            .iter()
            .filter(|top| top.role != Some(Role::Trash))
            .map(|top| (top.id.clone(), top.title.clone(), canvas::shown(top, depth)))
            .collect();
        state.dirty |= state.data.prune(|id| project.node(id).is_some());
        // A card standing for a folder's contents goes where they are;
        // every document has a place, shown or not, so changing the depth
        // never moves anything.
        for (_, _, shown) in &groups {
            state.dirty |= state.data.place_with_contents(&shown.cards);
        }
        let ids: Vec<Vec<String>> = project
            .manifest
            .binder
            .iter()
            .filter(|top| top.role != Some(Role::Trash))
            .zip(&groups)
            .flat_map(|(top, (_, _, shown))| {
                // Each folder's documents a block, and folder cards (when
                // their contents have no places either) one more.
                let mut blocks = canvas::blocks(top);
                blocks.push(shown.cards.iter().filter(|n| !n.is_document()).map(|n| n.id.clone()).collect());
                blocks
            })
            .collect();
        state.dirty |= state.data.place_missing(&ids);
        state.stand_in.clear();
        state.outlines.clear();
        for (top, _, shown) in &groups {
            if state.data.hidden.contains(top) {
                continue;
            }
            state.stand_in.extend(shown.stand_in.iter().map(|(a, b)| (a.clone(), b.clone())));
            if !state.data.hide_folders {
                state.outlines.extend(shown.folders.iter().map(|(n, cards, nesting)| (n.title.clone(), cards.clone(), *nesting)));
            }
        }
        for note in &state.data.notes {
            state.stand_in.insert(note.id.clone(), note.id.clone());
        }
        state.drawn_edges = canvas::roll_up(&state.edges, &state.stand_in);

        let labels = &project.manifest.labels;
        let statuses = &project.manifest.statuses;
        let kinds = project.profile_kinds();
        let place = |id: &str, size: Vec2| {
            let [x, y] = state.data.places.get(id).copied().unwrap_or_default();
            Rect::from_min_size(Pos2::new(x, y), size)
        };
        let mut cards = Vec::new();
        for (top, _, shown) in &groups {
            if state.data.hidden.contains(top) {
                continue;
            }
            for node in &shown.cards {
                // What's inside it, shown as part of it.
                let inside = node.subtree().iter().skip(1).filter(|n| n.is_document()).count();
                let label = node.meta.label.and_then(|i| labels.get(i)).map(|l| label_color(l.color));
                let (kind, size) = match project.profile_of(&node.id) {
                    _ if !node.is_document() => (Kind::Folder { synopsis: node.meta.synopsis.clone(), label, inside }, DOC_SIZE),
                    Some(k) => {
                        let index = kinds.iter().position(|o| o.id == k.id).unwrap_or(0);
                        (Kind::Profile { kind: k.item.clone(), color: label_color(index as u8 + 1) }, PROFILE_SIZE)
                    }
                    None => {
                        let status = node.meta.status.filter(|&i| i < statuses.len()).map(|i| (statuses[i].clone(), super::views::status_color(i, statuses.len())));
                        (Kind::Document { synopsis: node.meta.synopsis.clone(), label, status, inside }, DOC_SIZE)
                    }
                };
                cards.push(Card { id: node.id.clone(), title: node.title.clone(), kind, rect: place(&node.id, size) });
            }
        }
        for note in &state.data.notes {
            cards.push(Card { id: note.id.clone(), title: String::new(), kind: Kind::Note, rect: place(&note.id, NOTE_SIZE) });
        }
        let folders: Vec<(String, String)> = groups.iter().map(|(id, title, _)| (id.clone(), title.clone())).collect();

        let mut actions = Vec::new();
        let fill = ui.visuals().extreme_bg_color;
        let opacity = self.view_opacity;
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(fill)).show(ui, |ui| {
            ui.set_opacity(opacity);
            canvas_bar(ui, &mut state, &folders);
            ui.separator();
            if state.fit {
                state.fit = false;
                state.view = fitted(&cards, ui.available_size());
            }
            let mut view = state.view;
            // Where the canvas is on screen (the scene's own rects are in
            // canvas units).
            let on_screen = ui.available_rect_before_wrap();
            let scene = egui::Scene::new().zoom_range(0.2..=1.5).show(ui, &mut view, |ui| draw(ui, &mut state, &cards, &mut actions));
            state.view = view;
            background(ui, &mut state, &cards, &scene.response, &scene.inner);
            if cards.is_empty() {
                blank_canvas(ui, on_screen, &blank);
            }
        });
        self.canvas = Some(state);

        for action in actions {
            match action {
                CanvasAction::Select(id) => self.select(id),
                CanvasAction::OpenSplit(id) => self.open_in_split(id),
                CanvasAction::OpenEditor(id) => {
                    self.canvas_open = false;
                    self.select(id);
                }
                CanvasAction::MakeDocument(note) => self.note_to_document(&note),
            }
        }
    }

    /// Make a note into a document in the Notes folder: its first line the
    /// title, the rest the text. It keeps its place and its strings.
    pub(super) fn note_to_document(&mut self, note: &str) {
        let Some(state) = self.canvas.as_mut() else { return };
        let Some(text) = state.data.note(note).map(|n| n.text.clone()) else { return };
        let (first, rest) = text.trim_start().split_once('\n').unwrap_or((text.trim(), ""));
        let mut title: String = first.trim().trim_start_matches('#').trim().chars().take(60).collect();
        if title.is_empty() {
            title = "Untitled Note".to_owned();
        }
        let Some(project) = self.project.as_mut() else { return };
        let made = project.add_document(&Place::Into(Role::Notes.folder().to_owned()), &title).and_then(|id| {
            let path = project.file_path(&id).ok_or_else(|| io::Error::other("no file"))?;
            sloth_core::document::write_atomically(&path, rest.trim_start().as_bytes())?;
            Ok(id)
        });
        match made {
            Ok(id) => {
                state.data.rename(note, &id);
                state.data.notes.retain(|n| n.id != note);
                state.focus = Some(id.clone());
                state.dirty = true;
                self.select(id);
            }
            Err(e) => self.error = Some(format!("Couldn't make the note a document: {e}")),
        }
    }
}

/// Above the canvas: a new note, which folders to show, and fitting
/// everything into view.
fn canvas_bar(ui: &mut egui::Ui, state: &mut CanvasState, folders: &[(String, String)]) {
    egui::Frame::NONE.inner_margin(egui::Margin { left: 16, right: 16, top: 8, bottom: 0 }).show(ui, |ui| {
        ui.horizontal(|ui| {
            if ui.button("New Note").on_hover_text("Or double-click the canvas").clicked() {
                state.new_note(state.view.center());
            }
            ui.menu_button("Show\u{2026}", |ui| {
                for (id, title) in folders {
                    let mut shown = !state.data.hidden.contains(id);
                    if ui.checkbox(&mut shown, title).changed() {
                        if shown {
                            state.data.hidden.remove(id);
                        } else {
                            state.data.hidden.insert(id.clone());
                        }
                        state.dirty = true;
                    }
                }
            })
            .response
            .on_hover_text("Which folders' documents are on the canvas");
            let before = state.data.depth;
            ui.add_space(8.0);
            ui.label("Depth").on_hover_text("How far into each folder to show cards: at 1, each chapter (say) is one card, standing for everything in it, its strings gathered together");
            for (d, name) in [(Some(1), "1"), (Some(2), "2"), (Some(3), "3"), (None, "All")] {
                ui.selectable_value(&mut state.data.depth, d, name);
            }
            if state.data.depth != before {
                state.dirty = true;
            }
            let mut folders = !state.data.hide_folders;
            if ui.checkbox(&mut folders, "Folders").on_hover_text("Outline each folder's cards, with its title").changed() {
                state.data.hide_folders = !folders;
                state.dirty = true;
            }
            if ui.button("Fit").on_hover_text("Bring everything into view").clicked() {
                state.fit = true;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new("Drag a card\u{2019}s pin to another card to tie a string \u{b7} pinch or \u{2318}-scroll to zoom")
                        .small()
                        .weak(),
                );
            });
        });
    });
}

/// A folder's outline: a faint rounded frame round its cards, its title at
/// the top left.
fn painter_outline(ui: &mut egui::Ui, area: Rect, title: &str, color: Color32) {
    ui.painter().rect(area, CornerRadius::same(12), color.gamma_multiply(0.04), Stroke::new(1.0, color.gamma_multiply(0.35)), StrokeKind::Inside);
    // A label (not just paint), so it can be read out; it takes no clicks.
    let at = Rect::from_min_size(area.min + Vec2::new(12.0, 5.0), Vec2::new((area.width() - 24.0).max(40.0), 16.0));
    let label = egui::Label::new(RichText::new(title).size(12.0).color(color)).selectable(false).truncate().sense(Sense::hover());
    ui.scope_builder(UiBuilder::new().max_rect(at).layout(egui::Layout::left_to_right(egui::Align::Min)), |ui| ui.add(label));
}

/// Nothing on the canvas (every folder hidden, say): the sloth and a hint,
/// in the middle, not in the way of a double-click.
fn blank_canvas(ui: &egui::Ui, area: Rect, sloth: &egui::TextureHandle) {
    let painter = ui.painter();
    let center = area.center();
    let image = Rect::from_center_size(center - Vec2::new(0.0, 40.0), Vec2::splat(112.0));
    painter.image(sloth.id(), image, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
    let visuals = ui.visuals();
    painter.text(image.center_bottom() + Vec2::new(0.0, 16.0), egui::Align2::CENTER_TOP, "A blank canvas", FontId::proportional(16.0), visuals.strong_text_color());
    painter.text(
        image.center_bottom() + Vec2::new(0.0, 40.0),
        egui::Align2::CENTER_TOP,
        "Double-click anywhere for a note, or Show\u{2026} a folder\u{2019}s documents.",
        FontId::proportional(13.0),
        visuals.weak_text_color(),
    );
}

/// A view of the canvas with every card in it, no closer than life size.
fn fitted(cards: &[Card], screen: Vec2) -> Rect {
    let screen = screen.max(Vec2::splat(100.0));
    let Some(all) = cards.iter().map(|c| c.rect).reduce(|a, b| a.union(b)) else {
        return Rect::from_min_size(Pos2::new(-GRID, -GRID), screen);
    };
    let all = all.expand(GRID * 2.0);
    Rect::from_center_size(all.center(), all.size().max(screen))
}

/// A card's corner, snapped to the dots.
fn snap(p: Pos2) -> Pos2 {
    Pos2::new((p.x / GRID).round() * GRID, (p.y / GRID).round() * GRID)
}

/// The four points of a string between two cards, sagging a little in the
/// middle, as string does. It ends at the cards' edges, where a line
/// between their middles crosses them, so it never runs under their text.
fn string_between(from: Rect, to: Rect) -> [Pos2; 4] {
    swaying_string((from, 0.0), (to, 0.0), 0.0)
}

/// As `string_between`, between cards with rounded corners (each a rect
/// and its corner radius: a profile's pill is all corner at its ends), its
/// sag stretched by `wobble` (a fraction, either way) as it swings and
/// settles.
fn swaying_string(from: (Rect, f32), to: (Rect, f32), wobble: f32) -> [Pos2; 4] {
    let (a, b) = (rounded_edge_point(from.0, from.1, to.0.center()), rounded_edge_point(to.0, to.1, from.0.center()));
    let d = b - a;
    let sag = Vec2::new(0.0, (d.length() * 0.12).min(48.0) * (1.0 + wobble));
    [a, a + d / 3.0 + sag, a + d * 2.0 / 3.0 + sag, b]
}

/// Animate? Not with the Mac's Reduce motion, and never in tests (which
/// look for cards where they were put).
fn animate() -> bool {
    !cfg!(test) && sloth_core::motion::animate()
}

/// How long a string swings after its card is set down.
const SWAY_SECONDS: f64 = 1.4;

/// A string's swing `t` seconds after its card was set down: a sway that
/// dies away, as a fraction of its sag.
fn swing(t: f64) -> f32 {
    if !(0.0..SWAY_SECONDS).contains(&t) {
        return 0.0;
    }
    (0.45 * (-3.2 * t).exp() * (11.0 * t).sin()) as f32
}

/// Where a line from the middle of `rect` towards `toward` leaves it.
fn edge_point(rect: Rect, toward: Pos2) -> Pos2 {
    let c = rect.center();
    let d = toward - c;
    let half = rect.size() / 2.0;
    let t = [(d.x, half.x), (d.y, half.y)]
        .into_iter()
        .filter(|(along, _)| along.abs() > f32::EPSILON)
        .map(|(along, h)| h / along.abs())
        .fold(f32::INFINITY, f32::min);
    if t.is_finite() { c + d * t.min(1.0) } else { c }
}

/// As `edge_point`, for a rect with corners rounded by `radius`: a line
/// leaving through a corner meets the curve, not the corner's empty point.
fn rounded_edge_point(rect: Rect, radius: f32, toward: Pos2) -> Pos2 {
    let square = edge_point(rect, toward);
    let r = radius.min(rect.width() / 2.0).min(rect.height() / 2.0);
    if r <= 0.0 {
        return square;
    }
    // The corners' circles are centred on the rect shrunk by the radius.
    let inner = rect.shrink(r);
    // Not `clamp`: for a pill (r half its height) rounding can leave the
    // top a hair below the bottom, and `clamp` panics on that.
    let between = |v: f32, lo: f32, hi: f32| v.max(lo.min(hi)).min(lo.max(hi));
    let k = Pos2::new(between(square.x, inner.left(), inner.right()), between(square.y, inner.top(), inner.bottom()));
    let in_corner = (square.x - k.x).abs() > f32::EPSILON && (square.y - k.y).abs() > f32::EPSILON;
    if !in_corner {
        return square;
    }
    // Where the line from the middle leaves that corner's circle.
    let c = rect.center();
    let d = toward - c;
    let f = c - k;
    let (a, b, cc) = (d.dot(d), 2.0 * f.dot(d), f.dot(f) - r * r);
    let disc = b * b - 4.0 * a * cc;
    if a <= f32::EPSILON || disc < 0.0 {
        return square;
    }
    let t = ((-b + disc.sqrt()) / (2.0 * a)).clamp(0.0, 1.0);
    c + d * t
}

/// How round a card's corners are: a pill's ends are half its height.
fn corner_radius(card: &Card) -> f32 {
    match card.kind {
        Kind::Profile { .. } => card.rect.height() / 2.0,
        Kind::Note => 4.0,
        Kind::Document { .. } | Kind::Folder { .. } => 6.0,
    }
}

fn point_on(p: &[Pos2; 4], t: f32) -> Pos2 {
    let u = 1.0 - t;
    let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
    Pos2::new(
        p.iter().zip(w).map(|(q, w)| q.x * w).sum(),
        p.iter().zip(w).map(|(q, w)| q.y * w).sum(),
    )
}

/// How far `q` is from a string.
fn distance_to(p: &[Pos2; 4], q: Pos2) -> f32 {
    const STEPS: usize = 24;
    let pts: Vec<Pos2> = (0..=STEPS).map(|i| point_on(p, i as f32 / STEPS as f32)).collect();
    pts.windows(2)
        .map(|w| {
            let (a, b) = (w[0], w[1]);
            let ab = b - a;
            let t = ((q - a).dot(ab) / ab.length_sq().max(f32::EPSILON)).clamp(0.0, 1.0);
            (a + ab * t).distance(q)
        })
        .fold(f32::INFINITY, f32::min)
}

/// The colour of strings tied by hand.
fn string_color(ui: &egui::Ui) -> Color32 {
    if ui.visuals().dark_mode { Color32::from_rgb(0xE0, 0x80, 0x70) } else { Color32::from_rgb(0xC2, 0x55, 0x45) }
}

fn note_fill(ui: &egui::Ui) -> Color32 {
    if ui.visuals().dark_mode { Color32::from_rgb(0x4A, 0x44, 0x2C) } else { Color32::from_rgb(0xFB, 0xF3, 0xD2) }
}

fn note_text_id(id: &str) -> egui::Id {
    egui::Id::unique(("canvas-note-text", id))
}

/// A card's title, on one line from the left, cut short if it's long.
fn title_label(ui: &mut egui::Ui, rect: Rect, title: &str) {
    let mut line = ui.new_child(UiBuilder::new().max_rect(rect).layout(egui::Layout::left_to_right(egui::Align::Center)));
    line.add(egui::Label::new(RichText::new(title).strong()).truncate().selectable(false));
}

/// The canvas's contents: dots, strings, then cards (the picked one on
/// top). Returns the string under the pointer.
fn draw(ui: &mut egui::Ui, state: &mut CanvasState, cards: &[Card], actions: &mut Vec<CanvasAction>) -> Hovered {
    let to_global = ui.ctx().layer_transform_to_global(ui.layer_id()).unwrap_or(TSTransform::IDENTITY);
    state.to_screen = to_global;
    let scale = to_global.scaling.max(f32::EPSILON);
    let pointer = ui.input(|i| i.pointer.latest_pos()).map(|p| to_global.inverse() * p);

    // Zoomed out, every other row of dots goes, so they never crowd.
    let mut step = GRID;
    while step * scale < 16.0 {
        step *= 2.0;
    }
    dot_grid(ui, Pos2::ZERO, step, 1.0 / scale);

    // Where the cards are drawn: lifted ones a little higher.
    let rects: HashMap<&str, Rect> = cards
        .iter()
        .map(|c| (c.id.as_str(), c.rect.translate(Vec2::new(0.0, -state.lifts.get(&c.id).copied().unwrap_or(0.0)))))
        .collect();
    let corners: HashMap<&str, f32> = cards.iter().map(|c| (c.id.as_str(), corner_radius(c))).collect();
    let shape = |id: &str, rect: &Rect| (*rect, corners.get(id).copied().unwrap_or(0.0));
    let focus = state.focus.clone().filter(|f| rects.contains_key(f.as_str()));
    let focus = focus.as_deref();
    let mut near: HashSet<String> = HashSet::new();
    if let Some(f) = focus {
        near.insert(f.to_owned());
        let pairs = state.drawn_edges.iter().map(|e| (e.from.as_str(), e.to.as_str()));
        let ties = state.data.strings.iter().filter_map(|t| Some((state.stand_in.get(&t.a)?.as_str(), state.stand_in.get(&t.b)?.as_str())));
        for (a, b) in pairs.chain(ties) {
            if a == f {
                near.insert(b.to_owned());
            } else if b == f {
                near.insert(a.to_owned());
            }
        }
    }
    let lit = |id: &str| focus.is_none() || near.contains(id);
    // How lit each card is, easing as the pick changes (0 faded, 1 lit).
    let ctx = ui.ctx().clone();
    let animate = animate();
    let fade_time = if animate { 0.22 } else { 0.0 };
    let glow: HashMap<&str, f32> =
        cards.iter().map(|c| (c.id.as_str(), ctx.animate_bool_with_time(egui::Id::unique(("canvas-lit", &c.id)), lit(&c.id), fade_time))).collect();
    let glow_of = |a: &str, b: &str| glow.get(a).copied().unwrap_or(1.0).min(glow.get(b).copied().unwrap_or(1.0));
    // Strings on a card just dropped or tied swing, and settle.
    let now = ui.input(|i| i.time);
    state.sway.retain(|_, since| now - *since < SWAY_SECONDS);
    if !animate {
        state.sway.clear();
    }
    if !state.sway.is_empty() {
        ctx.request_repaint();
    }
    let sway = &state.sway;
    let wobble_of = |a: &str, b: &str| -> f32 {
        [a, b].iter().filter_map(|id| sway.get(*id)).map(|since| swing(now - since)).fold(0.0, |w: f32, s| if s.abs() > w.abs() { s } else { w })
    };
    let over_card = pointer.is_some_and(|p| cards.iter().any(|c| c.rect.contains(p)));

    // The strings, under the cards.
    let mut hovered = Hovered::default();
    let reach = STRING_REACH / scale;
    let weak = ui.visuals().weak_text_color();
    let painter = ui.painter().clone();
    let mut nearest = reach;
    // Each folder's cards, outlined with its title: the outer ones wider.
    let outline = ui.visuals().weak_text_color();
    for (title, inside, nesting) in &state.outlines {
        let Some(bounds) = inside.iter().filter_map(|id| rects.get(id.as_str())).copied().reduce(|a, b| a.union(b)) else { continue };
        let pad = 14.0 + 14.0 * *nesting as f32;
        let area = Rect::from_min_max(bounds.min - Vec2::new(pad, pad + 16.0), bounds.max + Vec2::splat(pad));
        painter_outline(ui, area, title, outline);
    }

    for (i, edge) in state.drawn_edges.iter().enumerate() {
        let (Some(a), Some(b)) = (rects.get(edge.from.as_str()), rects.get(edge.to.as_str())) else { continue };
        let points = swaying_string(shape(&edge.from, a), shape(&edge.to, b), wobble_of(&edge.from, &edge.to));
        let full = if focus.is_some() { 0.9 } else { 0.7 };
        let alpha = 0.12 + (full - 0.12) * glow_of(&edge.from, &edge.to);
        let width = (1.2 + (edge.references as f32).ln_1p() * 0.6).min(3.0);
        painter.add(CubicBezierShape::from_points_stroke(points, false, Color32::TRANSPARENT, Stroke::new(width, weak.gamma_multiply(alpha))));
        if let Some(p) = pointer.filter(|_| !over_card) {
            let d = distance_to(&points, p);
            if d < nearest {
                nearest = d;
                hovered.edge = Some(i);
            }
        }
    }
    let red = string_color(ui);
    let mut nearest_tie = reach;
    let mut tie_points = Vec::new();
    for (i, tie) in state.data.strings.iter().enumerate() {
        // Tied to something inside a card shown for it: to that card.
        let (Some(ta), Some(tb)) = (state.stand_in.get(&tie.a), state.stand_in.get(&tie.b)) else { continue };
        if ta == tb {
            continue;
        }
        let (Some(a), Some(b)) = (rects.get(ta.as_str()), rects.get(tb.as_str())) else { continue };
        let points = swaying_string(shape(ta, a), shape(tb, b), wobble_of(ta, tb));
        if let Some(p) = pointer.filter(|_| !over_card) {
            let d = distance_to(&points, p);
            if d < nearest_tie {
                nearest_tie = d;
                hovered.tie = Some(i);
            }
        }
        tie_points.push((i, points, glow_of(ta, tb)));
    }
    // A string tied by hand wins over one the project draws.
    if hovered.tie.is_some() {
        hovered.edge = None;
    }
    for (i, points, on) in tie_points {
        let strong = state.picked == Some(i) || hovered.tie == Some(i);
        let color = red.gamma_multiply(0.25 + 0.75 * on);
        let stroke = Stroke::new(if strong { 2.8 } else { 1.6 }, color);
        painter.add(CubicBezierShape::from_points_stroke(points, false, Color32::TRANSPARENT, stroke));
        // A knot at each end, where it's tied on.
        for end in [points[0], points[3]] {
            painter.circle_filled(end, if strong { 3.5 } else { 2.5 }, color);
        }
        let label = &state.data.strings[i].label;
        if !label.is_empty() {
            let galley = painter.layout_no_wrap(label.clone(), FontId::proportional(11.0), color);
            let rect = Rect::from_center_size(point_on(&points, 0.5), galley.size() + Vec2::new(8.0, 2.0));
            painter.rect_filled(rect, CornerRadius::same(4), ui.visuals().extreme_bg_color);
            painter.galley(rect.min + Vec2::new(4.0, 1.0), galley, color);
        }
    }
    // A string being tied follows the pointer.
    if let (Some(from), Some(p)) = (state.tying.as_deref().and_then(|id| rects.get(id)), pointer) {
        painter.add(CubicBezierShape::from_points_stroke(
            string_between(*from, Rect::from_center_size(p, Vec2::ZERO)),
            false,
            Color32::TRANSPARENT,
            Stroke::new(1.6, red),
        ));
    }

    // The cards, the picked one (or the one being moved) last, on top.
    let mut order: Vec<&Card> = cards.iter().collect();
    order.sort_by_key(|c| Some(c.id.as_str()) == focus);
    let target = state.tying.as_ref().and(pointer).and_then(|p| cards.iter().rev().find(|c| c.rect.contains(p))).map(|c| c.id.clone());
    for card in order {
        let opacity = 0.35 + 0.65 * glow.get(card.id.as_str()).copied().unwrap_or(1.0);
        card_ui(ui, state, card, opacity, target.as_deref() == Some(card.id.as_str()), pointer, &rects, actions);
    }
    hovered
}

#[allow(clippy::too_many_arguments)]
fn card_ui(
    ui: &mut egui::Ui,
    state: &mut CanvasState,
    card: &Card,
    opacity: f32,
    targeted: bool,
    pointer: Option<Pos2>,
    rects: &HashMap<&str, Rect>,
    actions: &mut Vec<CanvasAction>,
) {
    let mut child = ui.new_child(
        UiBuilder::new().id_salt(("canvas-card", &card.id)).max_rect(card.rect).layout(egui::Layout::top_down(egui::Align::Min)),
    );
    let ui = &mut child;
    if opacity < 1.0 {
        ui.set_opacity(opacity);
    }
    // The card's background takes clicks and drags; a note's text, added
    // on top, takes its own.
    let id = egui::Id::unique(("canvas-card", &card.id));
    let response = ui.interact(card.rect, id, Sense::click_and_drag());
    // Lifted a little under the pointer, and more while carried, with a
    // shadow to match.
    let time = if animate() { 0.12 } else { 0.0 };
    let hover = ui.ctx().animate_bool_with_time(id.with("hover"), response.hovered() || response.dragged(), time);
    let carry = ui.ctx().animate_bool_with_time(id.with("carry"), response.dragged(), time);
    let lift = 1.5 * hover + 2.5 * carry;
    if lift > 0.0 {
        state.lifts.insert(card.id.clone(), lift);
    } else {
        state.lifts.remove(&card.id);
    }
    let rect = card.rect.translate(Vec2::new(0.0, -lift));
    let corner = corner_radius(card) as u8;
    let shadow = egui::epaint::Shadow {
        offset: [0, (2.0 + 3.0 * lift) as i8],
        blur: (6.0 + 5.0 * lift) as u8,
        spread: 0,
        color: Color32::from_black_alpha(if ui.visuals().dark_mode { 90 } else { 34 } + (10.0 * lift) as u8),
    };
    ui.painter().add(shadow.as_shape(rect, CornerRadius::same(corner)));
    let visuals = ui.visuals().clone();
    let picked = state.focus.as_deref() == Some(card.id.as_str());
    let border = if picked || targeted { Stroke::new(2.0, visuals.selection.stroke.color) } else { visuals.widgets.noninteractive.bg_stroke };
    let painter = ui.painter().clone();
    match &card.kind {
        Kind::Document { synopsis, label, status, inside } => {
            let fill = if visuals.dark_mode { Color32::from_gray(48) } else { Color32::from_rgb(0xFD, 0xFB, 0xF4) };
            // Holding more than itself: another card peeking out behind.
            if *inside > 0 {
                let behind = rect.translate(Vec2::new(4.0, 4.0));
                painter.rect(behind, CornerRadius::same(6), fill, visuals.widgets.noninteractive.bg_stroke, StrokeKind::Inside);
            }
            painter.rect(rect, CornerRadius::same(6), fill, border, StrokeKind::Inside);
            if let Some(color) = label {
                let strip = Rect::from_min_size(rect.min, Vec2::new(5.0, rect.height()));
                painter.rect_filled(strip, CornerRadius { nw: 6, sw: 6, ne: 0, se: 0 }, *color);
            }
            let inner = rect.shrink2(Vec2::new(12.0, 9.0));
            let title = Rect::from_min_size(inner.min, Vec2::new(inner.width(), 18.0));
            title_label(ui, title, &card.title);
            if !synopsis.is_empty() {
                let mut job = egui::text::LayoutJob::simple(synopsis.clone(), FontId::proportional(11.5), visuals.weak_text_color(), inner.width());
                job.wrap.max_rows = 2;
                job.wrap.overflow_character = Some('\u{2026}');
                painter.galley(Pos2::new(inner.left(), title.bottom() + 4.0), painter.layout_job(job), visuals.weak_text_color());
            }
            if let Some((name, color)) = status {
                super::views::stamp(&painter, rect.right_bottom() + Vec2::new(-8.0, -6.0), name, *color);
            }
            if *inside > 0 {
                let text = format!("+ {inside} inside");
                painter.text(rect.left_bottom() + Vec2::new(12.0, -7.0), egui::Align2::LEFT_BOTTOM, text, FontId::proportional(10.5), visuals.weak_text_color());
            }
        }
        Kind::Folder { synopsis, label, inside } => {
            // A folder: a tab on top, a tint, and what's in it.
            let tint = visuals.selection.bg_fill;
            let fill = if visuals.dark_mode { Color32::from_gray(44) } else { Color32::from_rgb(0xF6, 0xF4, 0xEC) }.lerp_to_gamma(tint, 0.12);
            let tab = Rect::from_min_size(rect.min - Vec2::new(0.0, 7.0), Vec2::new(64.0, 12.0));
            painter.rect(tab, CornerRadius { nw: 5, ne: 5, sw: 0, se: 0 }, fill, border, StrokeKind::Inside);
            painter.rect(rect, CornerRadius { nw: 0, ne: 6, sw: 6, se: 6 }, fill, border, StrokeKind::Inside);
            painter.rect_filled(Rect::from_min_size(tab.min + Vec2::new(1.0, 6.0), Vec2::new(62.0, 8.0)), CornerRadius::ZERO, fill);
            if let Some(color) = label {
                let strip = Rect::from_min_size(rect.min, Vec2::new(5.0, rect.height()));
                painter.rect_filled(strip, CornerRadius { nw: 0, sw: 6, ne: 0, se: 0 }, *color);
            }
            let inner = rect.shrink2(Vec2::new(12.0, 9.0));
            let title = Rect::from_min_size(inner.min, Vec2::new(inner.width(), 18.0));
            title_label(ui, title, &card.title);
            let count = format!("{inside} document{}", if *inside == 1 { "" } else { "s" });
            let line = if synopsis.is_empty() { count } else { format!("{count} \u{b7} {synopsis}") };
            let mut job = egui::text::LayoutJob::simple(line, FontId::proportional(11.5), visuals.weak_text_color(), inner.width());
            job.wrap.max_rows = 2;
            job.wrap.overflow_character = Some('\u{2026}');
            painter.galley(Pos2::new(inner.left(), title.bottom() + 4.0), painter.layout_job(job), visuals.weak_text_color());
        }
        Kind::Profile { kind, color } => {
            let radius = (rect.height() / 2.0) as u8;
            painter.rect(rect, CornerRadius::same(radius), visuals.extreme_bg_color.lerp_to_gamma(*color, 0.16), border, StrokeKind::Inside);
            if !(picked || targeted) {
                painter.rect_stroke(rect, CornerRadius::same(radius), Stroke::new(1.0, color.gamma_multiply(0.6)), StrokeKind::Inside);
            }
            let inner = rect.shrink2(Vec2::new(18.0, 5.0));
            painter.text(inner.left_top(), egui::Align2::LEFT_TOP, kind, FontId::proportional(10.0), *color);
            let title = Rect::from_min_size(inner.min + Vec2::new(0.0, 13.0), Vec2::new(inner.width(), 18.0));
            title_label(ui, title, &card.title);
        }
        Kind::Note => {
            painter.rect(rect, CornerRadius::same(4), note_fill(ui), border, StrokeKind::Inside);
            let grip = Rect::from_min_size(rect.min, Vec2::new(rect.width(), NOTE_GRIP));
            painter.text(grip.center(), egui::Align2::CENTER_CENTER, "\u{2022}\u{2022}\u{2022}", FontId::proportional(9.0), visuals.weak_text_color());
            let body = Rect::from_min_max(Pos2::new(rect.left() + 8.0, grip.bottom()), rect.max - Vec2::new(8.0, 6.0));
            if state.edit_note.as_deref() == Some(card.id.as_str()) {
                state.edit_note = None;
                ui.memory_mut(|m| m.request_focus(note_text_id(&card.id)));
            }
            let mut body_ui = ui.new_child(UiBuilder::new().max_rect(body).layout(egui::Layout::top_down(egui::Align::Min)));
            body_ui.set_clip_rect(body.intersect(body_ui.clip_rect()));
            if let Some(note) = state.data.notes.iter_mut().find(|n| n.id == card.id) {
                let field = egui::ScrollArea::vertical()
                    .id_salt(("canvas-note-scroll", &card.id))
                    .auto_shrink([false, false])
                    .show(&mut body_ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut note.text)
                                .id(note_text_id(&card.id))
                                .hint_text("A note\u{2026}")
                                .frame(egui::Frame::NONE)
                                .desired_rows(5)
                                .desired_width(f32::INFINITY),
                        )
                    })
                    .inner;
                if field.changed() {
                    state.dirty = true;
                }
                if field.gained_focus() {
                    state.focus = Some(card.id.clone());
                }
            }
        }
    }

    // Moving it.
    if response.dragged() {
        let at = state.data.places.entry(card.id.clone()).or_insert([rect.left(), rect.top()]);
        let d = response.drag_delta();
        *at = [at[0] + d.x, at[1] + d.y];
        state.focus = Some(card.id.clone());
        state.dirty = true;
    }
    if response.drag_stopped() {
        // Its strings swing as it's set down.
        state.sway.insert(card.id.clone(), ui.input(|i| i.time));
    }
    if response.drag_stopped()
        && let Some(at) = state.data.places.get_mut(&card.id)
    {
        let p = snap(Pos2::new(at[0], at[1]));
        *at = [p.x, p.y];
    }
    if response.clicked() {
        state.focus = Some(card.id.clone());
        state.picked = None;
        if !matches!(card.kind, Kind::Note) {
            actions.push(CanvasAction::Select(card.id.clone()));
        }
    }
    if response.double_clicked() {
        match card.kind {
            Kind::Note => state.edit_note = Some(card.id.clone()),
            _ => actions.push(CanvasAction::OpenSplit(card.id.clone())),
        }
    }
    response.context_menu(|ui| match card.kind {
        Kind::Note => {
            if ui.button("Make into Document").on_hover_text("A document in Notes: the first line its title").clicked() {
                actions.push(CanvasAction::MakeDocument(card.id.clone()));
            }
            if ui.button("Delete Note").clicked() {
                state.data.remove(&card.id);
                state.picked = None;
                state.dirty = true;
            }
        }
        _ => {
            if ui.button("Open in Split").clicked() {
                actions.push(CanvasAction::OpenSplit(card.id.clone()));
            }
            if ui.button("Open in Editor").clicked() {
                actions.push(CanvasAction::OpenEditor(card.id.clone()));
            }
        }
    });

    // The pin a string is tied from, shown when the pointer's near (or a
    // string is being tied from it).
    let tying_here = state.tying.as_deref() == Some(card.id.as_str());
    let near = pointer.is_some_and(|p| rect.expand(12.0).contains(p)) && state.tying.is_none();
    if near || tying_here {
        let pin = rect.center_top();
        let red = string_color(ui);
        let hit = ui.interact(Rect::from_center_size(pin, Vec2::splat(18.0)), egui::Id::unique(("canvas-pin", &card.id)), Sense::drag());
        let radius = if hit.hovered() || tying_here { 6.0 } else { 4.5 };
        ui.painter().circle(pin, radius, red, Stroke::new(1.5, ui.visuals().extreme_bg_color));
        if hit.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::Crosshair);
        }
        let hit = hit.on_hover_text("Drag to another card to tie a string (or untie one)");
        if hit.drag_started() {
            state.tying = Some(card.id.clone());
        }
        if hit.drag_stopped() {
            state.tying = None;
            let to = pointer.and_then(|p| rects.iter().filter(|(id, r)| **id != card.id && r.contains(p)).map(|(id, _)| *id).next());
            if let Some(to) = to {
                state.data.toggle_tie(&card.id, to);
                let now = ui.input(|i| i.time);
                state.sway.insert(card.id.clone(), now);
                state.sway.insert(to.to_owned(), now);
                state.picked = None;
                state.dirty = true;
            }
        }
    }
}

/// The empty canvas: clicking picks a string (or nothing), double-clicking
/// makes a note, right-clicking offers both, and the keys act on what's
/// picked.
fn background(ui: &egui::Ui, state: &mut CanvasState, cards: &[Card], response: &egui::Response, hovered: &Hovered) {
    if response.clicked() {
        state.focus = None;
        state.picked = hovered.tie;
    }
    if response.double_clicked()
        && hovered.tie.is_none()
        && let Some(p) = response.interact_pointer_pos()
    {
        state.new_note(p);
    }
    if let Some(i) = hovered.edge
        && let Some(edge) = state.drawn_edges.get(i)
    {
        let title = |id: &str| cards.iter().find(|c| c.id == id).map_or("", |c| c.title.as_str());
        let how = match (edge.references, edge.linked) {
            (0, _) => "linked".to_owned(),
            (1, linked) => format!("referred to once{}", if linked { ", and linked" } else { "" }),
            (n, linked) => format!("referred to {n} times{}", if linked { ", and linked" } else { "" }),
        };
        response.clone().on_hover_text_at_pointer(format!("{} \u{2192} {}: {how}", title(&edge.from), title(&edge.to)));
    }
    if response.secondary_clicked()
        && let Some(p) = response.interact_pointer_pos()
    {
        state.menu = Some((hovered.tie, p));
    }
    response.context_menu(|ui| match state.menu {
        Some((Some(i), _)) if i < state.data.strings.len() => {
            ui.label(RichText::new("What it means").small().weak());
            if ui.text_edit_singleline(&mut state.data.strings[i].label).changed() {
                state.dirty = true;
            }
            if ui.button("Untie String").clicked() {
                state.data.strings.remove(i);
                state.picked = None;
                state.dirty = true;
                ui.close();
            }
        }
        Some((_, at)) if ui.button("New Note Here").clicked() => {
            state.new_note(at);
            ui.close();
        }
        _ => {}
    });

    // Keys, unless a text field has them.
    if ui.memory(|m| m.focused().is_some()) {
        return;
    }
    if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
        state.focus = None;
        state.picked = None;
    }
    let delete = ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Delete) || i.consume_key(Modifiers::NONE, Key::Backspace));
    if delete
        && let Some(i) = state.picked.take()
        && i < state.data.strings.len()
    {
        state.data.strings.remove(i);
        state.dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pill_anywhere_never_trips_over_rounding() {
        // Lifted cards sit at fractional places, where a pill shrunk by
        // half its height can come out a hair inside out.
        let toward = Pos2::new(900.0, 300.0);
        for i in 0..400_000 {
            let y = -60.0 + i as f32 * 0.003_17;
            let lift = (i % 97) as f32 * 0.041;
            let pill = Rect::from_min_size(Pos2::new(240.0, y), PROFILE_SIZE).translate(Vec2::new(0.0, -lift));
            let p = rounded_edge_point(pill, PROFILE_SIZE.y / 2.0, toward);
            assert!(p.x.is_finite() && p.y.is_finite());
        }
    }

    #[test]
    fn strings_meet_rounded_ends() {
        // A pill 170 × 44: its ends are half circles of radius 22.
        let pill = Rect::from_min_size(Pos2::ZERO, Vec2::new(170.0, 44.0));
        let r = 22.0;
        let end_centre = Pos2::new(170.0 - r, 22.0);
        // Out through the rounded end at an angle: on the curve, inside the
        // rect's corner.
        let p = rounded_edge_point(pill, r, Pos2::new(400.0, 80.0));
        assert!((p.distance(end_centre) - r).abs() < 0.01, "on the curve: {p:?}");
        assert!(p.x < 170.0 && p.y < 44.0, "inside the corner's empty point");
        // Straight out of an end, or the flat top: as before.
        assert_eq!(rounded_edge_point(pill, r, Pos2::new(500.0, 22.0)), Pos2::new(170.0, 22.0));
        assert_eq!(rounded_edge_point(pill, r, Pos2::new(85.0, -300.0)), Pos2::new(85.0, 0.0));
        // No rounding: the rect's own edge.
        assert_eq!(rounded_edge_point(pill, 0.0, Pos2::new(400.0, 80.0)), edge_point(pill, Pos2::new(400.0, 80.0)));
    }

    #[test]
    fn a_string_swings_and_settles() {
        assert_eq!(swing(-0.1), 0.0);
        assert_eq!(swing(0.0), 0.0);
        let peak = (1..50).map(|i| swing(i as f64 * 0.02).abs()).fold(0.0, f32::max);
        assert!(peak > 0.2 && peak < 0.45, "a visible swing: {peak}");
        assert!(swing(1.2).abs() < 0.02, "nearly still by the end");
        assert_eq!(swing(SWAY_SECONDS), 0.0);
        // Swinging only changes the sag, not where it's tied.
        let (a, b) = (Rect::from_min_size(Pos2::ZERO, Vec2::splat(50.0)), Rect::from_min_size(Pos2::new(300.0, 0.0), Vec2::splat(50.0)));
        let (still, swung) = (string_between(a, b), swaying_string((a, 0.0), (b, 0.0), 0.4));
        assert_eq!((still[0], still[3]), (swung[0], swung[3]));
        assert!(swung[1].y > still[1].y);
    }

    #[test]
    fn strings_end_at_the_cards_edges() {
        let a = Rect::from_min_size(Pos2::ZERO, Vec2::new(200.0, 80.0));
        // Straight across, straight down, and slantwise (out of the bottom).
        assert_eq!(edge_point(a, Pos2::new(500.0, 40.0)), Pos2::new(200.0, 40.0));
        assert_eq!(edge_point(a, Pos2::new(100.0, 400.0)), Pos2::new(100.0, 80.0));
        assert_eq!(edge_point(a, Pos2::new(300.0, 140.0)), Pos2::new(180.0, 80.0));
        // Pointing at its own middle (or inside it): no further than that.
        assert_eq!(edge_point(a, a.center()), a.center());
        assert_eq!(edge_point(a, Pos2::new(110.0, 40.0)), Pos2::new(110.0, 40.0));

        let b = a.translate(Vec2::new(400.0, 0.0));
        let points = string_between(a, b);
        assert_eq!((points[0], points[3]), (Pos2::new(200.0, 40.0), Pos2::new(400.0, 40.0)));
    }
}
