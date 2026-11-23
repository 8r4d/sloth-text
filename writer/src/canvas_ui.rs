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
            focus: None,
            seen_selected: None,
            drawn: 0,
            tying: None,
            picked: None,
            menu: None,
            edit_note: None,
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
    Document { synopsis: String, label: Option<Color32> },
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

        // Every document outside the Trash, a top-level folder at a time.
        let groups: Vec<(String, String, Vec<&crate::project::Node>)> = project
            .manifest
            .binder
            .iter()
            .filter(|top| top.role != Some(Role::Trash))
            .map(|top| (top.id.clone(), top.title.clone(), top.subtree().into_iter().filter(|n| n.is_document()).collect()))
            .collect();
        state.dirty |= state.data.prune(|id| project.node(id).is_some());
        let ids: Vec<Vec<String>> = groups.iter().map(|(_, _, nodes)| nodes.iter().map(|n| n.id.clone()).collect()).collect();
        state.dirty |= state.data.place_missing(&ids);

        let labels = &project.manifest.labels;
        let kinds = project.profile_kinds();
        let place = |id: &str, size: Vec2| {
            let [x, y] = state.data.places.get(id).copied().unwrap_or_default();
            Rect::from_min_size(Pos2::new(x, y), size)
        };
        let mut cards = Vec::new();
        for (top, _, nodes) in &groups {
            if state.data.hidden.contains(top) {
                continue;
            }
            for node in nodes {
                let (kind, size) = match project.profile_of(&node.id) {
                    Some(k) => {
                        let index = kinds.iter().position(|o| o.id == k.id).unwrap_or(0);
                        (Kind::Profile { kind: k.item.clone(), color: label_color(index as u8 + 1) }, PROFILE_SIZE)
                    }
                    None => {
                        let label = node.meta.label.and_then(|i| labels.get(i)).map(|l| label_color(l.color));
                        (Kind::Document { synopsis: node.meta.synopsis.clone(), label }, DOC_SIZE)
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
    let (a, b) = (edge_point(from, to.center()), edge_point(to, from.center()));
    let d = b - a;
    let sag = Vec2::new(0.0, (d.length() * 0.12).min(48.0));
    [a, a + d / 3.0 + sag, a + d * 2.0 / 3.0 + sag, b]
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

    let rects: HashMap<&str, Rect> = cards.iter().map(|c| (c.id.as_str(), c.rect)).collect();
    let focus = state.focus.clone().filter(|f| rects.contains_key(f.as_str()));
    let focus = focus.as_deref();
    let mut near: HashSet<String> = HashSet::new();
    if let Some(f) = focus {
        near.insert(f.to_owned());
        let pairs = state.edges.iter().map(|e| (&e.from, &e.to));
        for (a, b) in pairs.chain(state.data.strings.iter().map(|t| (&t.a, &t.b))) {
            if a == f {
                near.insert(b.clone());
            } else if b == f {
                near.insert(a.clone());
            }
        }
    }
    let lit = |id: &str| focus.is_none() || near.contains(id);
    let over_card = pointer.is_some_and(|p| cards.iter().any(|c| c.rect.contains(p)));

    // The strings, under the cards.
    let mut hovered = Hovered::default();
    let reach = STRING_REACH / scale;
    let weak = ui.visuals().weak_text_color();
    let painter = ui.painter().clone();
    let mut nearest = reach;
    for (i, edge) in state.edges.iter().enumerate() {
        let (Some(a), Some(b)) = (rects.get(edge.from.as_str()), rects.get(edge.to.as_str())) else { continue };
        let points = string_between(*a, *b);
        let on = lit(&edge.from) && lit(&edge.to);
        let alpha = match (focus.is_some(), on) {
            (true, true) => 0.9,
            (true, false) => 0.12,
            (false, _) => 0.7,
        };
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
        let (Some(a), Some(b)) = (rects.get(tie.a.as_str()), rects.get(tie.b.as_str())) else { continue };
        let points = string_between(*a, *b);
        if let Some(p) = pointer.filter(|_| !over_card) {
            let d = distance_to(&points, p);
            if d < nearest_tie {
                nearest_tie = d;
                hovered.tie = Some(i);
            }
        }
        tie_points.push((i, points, lit(&tie.a) && lit(&tie.b)));
    }
    // A string tied by hand wins over one the project draws.
    if hovered.tie.is_some() {
        hovered.edge = None;
    }
    for (i, points, on) in tie_points {
        let strong = state.picked == Some(i) || hovered.tie == Some(i);
        let color = if on { red } else { red.gamma_multiply(0.25) };
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
        card_ui(ui, state, card, !lit(&card.id), target.as_deref() == Some(card.id.as_str()), pointer, &rects, actions);
    }
    hovered
}

#[allow(clippy::too_many_arguments)]
fn card_ui(
    ui: &mut egui::Ui,
    state: &mut CanvasState,
    card: &Card,
    dim: bool,
    targeted: bool,
    pointer: Option<Pos2>,
    rects: &HashMap<&str, Rect>,
    actions: &mut Vec<CanvasAction>,
) {
    let rect = card.rect;
    let mut child = ui.new_child(
        UiBuilder::new().id_salt(("canvas-card", &card.id)).max_rect(rect).layout(egui::Layout::top_down(egui::Align::Min)),
    );
    let ui = &mut child;
    if dim {
        ui.set_opacity(0.35);
    }
    // The card's background takes clicks and drags; a note's text, added
    // on top, takes its own.
    let response = ui.interact(rect, egui::Id::unique(("canvas-card", &card.id)), Sense::click_and_drag());
    let visuals = ui.visuals().clone();
    let picked = state.focus.as_deref() == Some(card.id.as_str());
    let border = if picked || targeted { Stroke::new(2.0, visuals.selection.stroke.color) } else { visuals.widgets.noninteractive.bg_stroke };
    let painter = ui.painter().clone();
    match &card.kind {
        Kind::Document { synopsis, label } => {
            let fill = if visuals.dark_mode { Color32::from_gray(48) } else { Color32::from_rgb(0xFD, 0xFB, 0xF4) };
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
        && let Some(edge) = state.edges.get(i)
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
