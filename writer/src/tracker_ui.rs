//! The Tracker, a folder's third view: a grid of profiles (characters,
//! locations, threads…) against the folder's scenes in order, showing who's
//! in each scene, who's only mentioned, how long anyone's away, and how
//! threads open and close (see presence.rs). Also the thread marks in a
//! scene's inspector.

use std::collections::BTreeMap;

use eframe::egui::{self, Color32, Pos2, RichText, Sense, Stroke, Vec2};

use super::WriterApp;
use crate::character::Sheet;
use crate::presence::{self, Cell, Gap, Issue, Scene, Summary, Track};
use crate::project::{CHARACTERS, Meta, Place, THREADS, ThreadMark, TrackRules};

/// Seconds a worked-out grid is kept before it's worked out again (texts
/// change as you write).
const REFRESH: f64 = 2.0;
const CELL: f32 = 16.0;
const ROW: f32 = 38.0;
const NAMES: f32 = 300.0;
/// Space between the names and the grid.
const GUTTER: f32 = 12.0;
/// The chapters' line over the columns.
const CHAPTERS: f32 = 22.0;
/// Under it, in the compact grid: a mark for each scene, drafted or planned.
const STRIP: f32 = 14.0;
/// With notes shown: the scenes' titles.
const TITLES: f32 = 24.0;
/// With notes shown, a column's width and a row's height.
const WIDE_CELL: f32 = 140.0;
/// With notes shown, the names' column: narrower, to leave room.
const WIDE_NAMES: f32 = 190.0;
const WIDE_ROW: f32 = 60.0;

/// How big the grid's squares are: small marks, or wide enough to read
/// the notes in.
#[derive(Clone, Copy)]
struct Layout {
    names: f32,
    cell: f32,
    row: f32,
    notes: bool,
}

impl Layout {
    fn new(notes: bool) -> Self {
        if notes {
            Layout { names: WIDE_NAMES, cell: WIDE_CELL, row: WIDE_ROW, notes }
        } else {
            Layout { names: NAMES, cell: CELL, row: ROW, notes }
        }
    }

    fn header(self) -> f32 {
        CHAPTERS + if self.notes { TITLES } else { STRIP }
    }
}

/// A scene as the grid's columns show it.
pub(super) struct Column {
    id: String,
    title: String,
    chapter: String,
    synopsis: String,
    /// No words yet: a scene planned, not written.
    planned: bool,
    /// Its notes, by profile (`Meta::plan`).
    plan: BTreeMap<String, String>,
}

impl Column {
    fn note(&self, profile: &str) -> Option<&str> {
        self.plan.get(profile).map(String::as_str).filter(|n| !n.trim().is_empty())
    }
}

/// A profile's row.
pub(super) struct Row {
    id: String,
    title: String,
    cells: Vec<Cell>,
    summary: Summary,
    issues: Vec<Issue>,
    rules: TrackRules,
}

impl Row {
    /// Scenes with a note for it that don't have it (and should, being written).
    fn not_in(&self) -> &[usize] {
        self.issues.iter().find_map(|i| if let Issue::NotIn(v) = i { Some(v.as_slice()) } else { None }).unwrap_or(&[])
    }
}

/// A grid worked out, for a folder and a kind.
pub(super) struct Grid {
    folder: String,
    kind: String,
    at: f64,
    threads: bool,
    columns: Vec<Column>,
    /// Each column's words.
    words: Vec<usize>,
    pub(super) rows: Vec<Row>,
}

/// What was done in the grid; applied once it's drawn.
pub(super) enum Act {
    Open(String),
    /// ⌥-click: in the scene, or out of it.
    Toggle { scene: String, profile: String },
    /// Write (or change) a profile's note in a scene.
    Note { scene: String, profile: String, at: Pos2 },
    /// Add a scene there, from its title and synopsis.
    NewScene { place: Place, at: Pos2 },
    /// A column dragged somewhere else.
    Move { scene: String, place: Place },
    /// A row's warnings changed.
    Rules { profile: String, rules: TrackRules },
}

/// A box open over the Tracker.
pub(super) enum Edit {
    Note { scene: String, profile: String, heading: String, text: String, at: Pos2 },
    Scene { place: Place, title: String, synopsis: String, at: Pos2 },
}

impl WriterApp {
    /// The kind of profile the Tracker shows, as last chosen.
    fn tracker_kind(&self) -> String {
        let project = self.project.as_ref();
        let has = |k: &str| project.is_some_and(|p| p.profile_kind(k).is_some());
        match &self.tracker_kind {
            Some(k) if has(k) => k.clone(),
            _ if has(CHARACTERS) => CHARACTERS.to_owned(),
            _ => project.and_then(|p| p.profile_kinds().first()).map(|k| k.id.clone()).unwrap_or_default(),
        }
    }

    /// Show the Manuscript in the Tracker, with this kind's rows (from a
    /// profile folder's menu).
    pub(super) fn track(&mut self, kind: &str) {
        self.tracker_kind = Some(kind.to_owned());
        let manuscript = crate::project::Role::Manuscript.folder().to_owned();
        self.folder_views.insert(manuscript.clone(), super::views::FolderView::Tracker);
        self.tracker = None;
        self.select(manuscript);
    }

    /// The grid for this folder and kind, worked out again if it's old.
    pub(super) fn tracker_grid(&mut self, folder: &str, now: f64) -> Option<&Grid> {
        let kind = self.tracker_kind();
        let fresh = self.tracker.as_ref().is_some_and(|g| g.folder == folder && g.kind == kind && now - g.at < REFRESH);
        if !fresh {
            self.tracker = self.work_out_grid(folder, &kind, now);
        }
        self.tracker.as_ref()
    }

    fn work_out_grid(&mut self, folder: &str, kind: &str, now: f64) -> Option<Grid> {
        let project = self.project.as_ref()?;
        let node = project.node(folder)?;
        let threads = kind == THREADS;
        // The scenes in order, each with the folder it's in.
        let mut found = Vec::new();
        for child in &node.children {
            let chapter = if child.is_document() && child.children.is_empty() { String::new() } else { child.title.clone() };
            for n in child.subtree().into_iter().filter(|n| n.is_document()) {
                let column = Column {
                    id: n.id.clone(),
                    title: n.title.clone(),
                    chapter: chapter.clone(),
                    synopsis: n.meta.synopsis.clone(),
                    planned: false,
                    plan: n.meta.plan.clone(),
                };
                found.push((column, n.meta.links.get(kind).cloned().unwrap_or_default(), n.meta.threads.clone()));
            }
        }
        // The rows: the kind's profiles, with the names to look for.
        let profiles: Vec<(String, String, TrackRules)> = project.profile_items(kind).into_iter().map(|n| (n.id.clone(), n.title.clone(), n.meta.tracker.clone())).collect();
        let mut scenes = Vec::new();
        let mut columns = Vec::new();
        for (mut column, links, marks) in found {
            let text = crate::compile::strip_notes(&self.document_text(&column.id).unwrap_or_default());
            let words = self.words_under(&column.id);
            column.planned = words == 0;
            let noted = column.plan.iter().filter(|(_, n)| !n.trim().is_empty()).map(|(id, _)| id.clone()).collect();
            scenes.push(Scene { words, text, links, marks, noted });
            columns.push(column);
        }
        let mut rows = Vec::new();
        for (id, title, rules) in profiles {
            let names = if threads {
                Vec::new()
            } else {
                let sheet = Sheet::parse(&self.document_text(&id).unwrap_or_default());
                let aliases = sheet.sections.iter().find(|s| s.heading.trim().eq_ignore_ascii_case("Also known as")).map(|s| s.text.clone()).unwrap_or_default();
                presence::names(&title, &aliases)
            };
            let cells: Vec<Cell> = scenes.iter().map(|s| presence::cell(s, &id, &names)).collect();
            let summary = presence::summary(&cells, &scenes);
            rows.push(Row { id, title, cells, summary, issues: Vec::new(), rules });
        }
        let tracks: Vec<Track> = rows.iter().map(|r| Track { id: &r.id, title: &r.title, cells: &r.cells, rules: &r.rules }).collect();
        let issues = presence::issues(&tracks, &scenes, threads);
        for (row, issues) in rows.iter_mut().zip(issues) {
            row.issues = issues;
        }
        let words = scenes.iter().map(|s| s.words).collect();
        Some(Grid { folder: folder.to_owned(), kind: kind.to_owned(), at: now, threads, columns, words, rows })
    }

    /// The Tracker for a folder.
    pub(super) fn tracker_view(&mut self, ui: &mut egui::Ui, folder: &str) {
        let now = ui.input(|i| i.time);
        // Which kind: a menu of the project's.
        let kinds: Vec<(String, String)> = self.project.as_ref().map(|p| p.profile_kinds().iter().map(|k| (k.id.clone(), k.name.clone())).collect()).unwrap_or_default();
        let mut kind = self.tracker_kind();
        let threads = kind == THREADS;
        let mut notes = self.tracker_notes;
        ui.horizontal_wrapped(|ui| {
            let name = kinds.iter().find(|(id, _)| *id == kind).map(|(_, n)| n.clone()).unwrap_or_default();
            egui::ComboBox::from_id_salt("tracker-kind").selected_text(name).show_ui(ui, |ui| {
                for (id, name) in &kinds {
                    ui.selectable_value(&mut kind, id.clone(), name);
                }
            });
            ui.add_space(8.0);
            ui.toggle_value(&mut notes, "Notes").on_hover_text("Wide squares, with each one's note written in them: plan the story in the grid");
            ui.add_space(12.0);
            legend(ui, threads, notes);
        });
        self.tracker_notes = notes;
        if kind != self.tracker_kind() {
            self.tracker_kind = Some(kind.clone());
            self.tracker = None;
        }
        ui.add_space(6.0);
        let kind_name = kinds.iter().find(|(id, _)| *id == kind).map(|(_, n)| n.clone()).unwrap_or_default();
        let layout = Layout::new(notes);
        let Some(grid) = self.tracker_grid(folder, now) else { return };
        let mut acts = Vec::new();
        if grid.columns.is_empty() {
            ui.weak("No documents in this folder to track.");
            if ui.button("New Scene\u{2026}").on_hover_text("A scene to plan: its title and what happens").clicked() {
                acts.push(Act::NewScene { place: Place::Into(folder.to_owned()), at: ui.ctx().pointer_latest_pos().unwrap_or_default() });
            }
        } else if grid.rows.is_empty() {
            let hint = if threads {
                "No threads yet. Add a Threads folder (New Profile Folder, in the binder's right-click menu on a top-level folder), a thread for each question the story opens, then mark in each scene's inspector where it opens, moves and closes."
            } else {
                "Nothing of this kind yet: add some in its folder."
            };
            ui.weak(format!("{kind_name}: {hint}"));
        } else {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::ZERO;
                    ui.vertical(|ui| {
                        ui.set_width(layout.names);
                        ui.add_space(layout.header());
                        for row in &grid.rows {
                            name_cell(ui, grid, row, layout, &mut acts);
                        }
                    });
                    egui::ScrollArea::horizontal().id_salt("tracker-cells").auto_shrink([false, true]).show(ui, |ui| {
                        ui.vertical(|ui| {
                            chapters(ui, grid, layout);
                            scenes(ui, grid, layout, &mut acts);
                            for row in &grid.rows {
                                cells(ui, grid, row, layout, &mut acts);
                            }
                        });
                    });
                });
            });
        }
        for act in acts {
            self.tracker_act(act, &kind);
        }
        self.tracker_edit_window(ui.ctx());
    }

    /// Do what was done in the grid.
    pub(super) fn tracker_act(&mut self, act: Act, kind: &str) {
        match act {
            Act::Open(id) => self.select(id),
            Act::Toggle { scene, profile } => self.toggle_in_scene(&scene, kind, &profile),
            Act::Note { scene, profile, at } => {
                let Some(project) = &self.project else { return };
                let (Some(s), Some(p)) = (project.node(&scene), project.node(&profile)) else { return };
                let heading = format!("{} in \u{201c}{}\u{201d}", p.title, s.title);
                let text = s.meta.plan.get(&profile).cloned().unwrap_or_default();
                self.tracker_edit = Some(Edit::Note { scene, profile, heading, text, at });
            }
            Act::NewScene { place, at } => {
                self.tracker_edit = Some(Edit::Scene { place, title: String::new(), synopsis: String::new(), at });
            }
            Act::Move { scene, place } => {
                self.move_item(&scene, &place);
                self.tracker = None;
            }
            Act::Rules { profile, rules } => self.set_track_rules(&profile, rules),
        }
    }

    /// The box for a note or a new scene, while one's open.
    fn tracker_edit_window(&mut self, ctx: &egui::Context) {
        let Some(edit) = self.tracker_edit.as_mut() else { return };
        let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        let save_key = ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter));
        let mut open = true;
        let (mut save, mut remove, mut cancel) = (false, false, escape);
        let (title, at) = match edit {
            Edit::Note { heading, at, .. } => (heading.clone(), *at),
            Edit::Scene { at, .. } => ("New Scene".to_owned(), *at),
        };
        egui::Window::new(title)
            .id(egui::Id::unique("tracker-edit"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_pos(at + Vec2::new(12.0, 12.0))
            .show(ctx, |ui| {
                ui.set_width(320.0);
                match edit {
                    Edit::Note { text, .. } => {
                        ui.weak("What it does in this scene, or is meant to");
                        let r = ui.add(egui::TextEdit::multiline(text).desired_rows(3).desired_width(f32::INFINITY).hint_text("Finds the forged letter"));
                        if !r.has_focus() && !r.lost_focus() && ui.memory(|m| m.focused().is_none()) {
                            r.request_focus();
                        }
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            save |= ui.button("Save").clicked();
                            cancel |= ui.button("Cancel").clicked();
                            if ui.button("Remove Note").clicked() {
                                remove = true;
                            }
                        });
                    }
                    Edit::Scene { title, synopsis, .. } => {
                        ui.weak("Title");
                        let r = ui.add(egui::TextEdit::singleline(title).desired_width(f32::INFINITY).hint_text("Untitled"));
                        if !r.has_focus() && !r.lost_focus() && ui.memory(|m| m.focused().is_none()) {
                            r.request_focus();
                        }
                        ui.add_space(4.0);
                        ui.weak("What happens (its synopsis)");
                        ui.add(egui::TextEdit::multiline(synopsis).desired_rows(3).desired_width(f32::INFINITY));
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            save |= ui.button("Add Scene").clicked();
                            cancel |= ui.button("Cancel").clicked();
                        });
                    }
                }
            });
        save |= save_key;
        if !open || cancel {
            self.tracker_edit = None;
            return;
        }
        if !(save || remove) {
            return;
        }
        match self.tracker_edit.take() {
            Some(Edit::Note { scene, profile, text, .. }) => {
                let text = if remove { String::new() } else { text };
                self.set_tracker_note(&scene, &profile, &text);
            }
            Some(Edit::Scene { place, title, synopsis, .. }) => self.add_planned_scene(&place, &title, &synopsis),
            None => {}
        }
    }

    /// What a profile's row warns about.
    pub(super) fn set_track_rules(&mut self, profile: &str, rules: TrackRules) {
        let Some(project) = self.project.as_mut() else { return };
        let Some(mut meta) = project.node(profile).map(|n| n.meta.clone()) else { return };
        meta.tracker = rules;
        if project.set_meta(profile, meta) {
            self.manifest_dirty = true;
        }
        self.tracker = None;
    }

    /// A profile's note in a scene (empty: none).
    pub(super) fn set_tracker_note(&mut self, scene: &str, profile: &str, text: &str) {
        let Some(project) = self.project.as_mut() else { return };
        let Some(mut meta) = project.node(scene).map(|n| n.meta.clone()) else { return };
        let text = text.trim();
        if text.is_empty() {
            meta.plan.remove(profile);
        } else {
            meta.plan.insert(profile.to_owned(), text.to_owned());
        }
        if project.set_meta(scene, meta) {
            self.manifest_dirty = true;
        }
        self.tracker = None;
    }

    /// A new, empty scene: a column to plan in.
    pub(super) fn add_planned_scene(&mut self, place: &Place, title: &str, synopsis: &str) {
        let Some(project) = self.project.as_mut() else { return };
        let title = if title.trim().is_empty() { "Untitled" } else { title.trim() };
        let target = project.new_document_target(place.target());
        match project.add_document(place, title) {
            Ok(id) => {
                let meta = Meta { synopsis: synopsis.trim().to_owned(), target_words: target, ..Meta::default() };
                if project.set_meta(&id, meta) {
                    self.manifest_dirty = true;
                }
            }
            Err(e) => self.error = Some(format!("Couldn't add the scene: {e}")),
        }
        self.tracker = None;
    }

    /// Put a profile in a scene's In This Scene, or take it out (and any
    /// thread mark with it).
    fn toggle_in_scene(&mut self, scene: &str, kind: &str, profile: &str) {
        let Some(project) = self.project.as_mut() else { return };
        let Some(mut meta) = project.node(scene).map(|n| n.meta.clone()) else { return };
        let list = meta.links.entry(kind.to_owned()).or_default();
        if list.iter().any(|p| p == profile) {
            list.retain(|p| p != profile);
            meta.threads.remove(profile);
        } else {
            list.push(profile.to_owned());
        }
        if meta.links.get(kind).is_some_and(Vec::is_empty) {
            meta.links.remove(kind);
        }
        if project.set_meta(scene, meta) {
            self.manifest_dirty = true;
        }
        self.tracker = None;
    }

    /// A thread in a scene's inspector: a menu to say whether the scene
    /// opens it, moves it along or closes it, or to take it out.
    pub(super) fn thread_chip(ui: &mut egui::Ui, meta: &mut Meta, id: &str, title: &str) -> bool {
        let mark = meta.threads.get(id).copied();
        let label = match mark {
            Some(ThreadMark::Opens) => format!("Opens: {title}"),
            Some(ThreadMark::Closes) => format!("Closes: {title}"),
            None => title.to_owned(),
        };
        let mut remove = false;
        ui.menu_button(label, |ui| {
            let mut chosen = mark;
            ui.radio_value(&mut chosen, Some(ThreadMark::Opens), "Opens here").on_hover_text("The scene that raises the question");
            ui.radio_value(&mut chosen, None, "Moves along").on_hover_text("It comes up, and moves on");
            ui.radio_value(&mut chosen, Some(ThreadMark::Closes), "Closes here").on_hover_text("The scene that answers it");
            if chosen != mark {
                match chosen {
                    Some(m) => meta.threads.insert(id.to_owned(), m),
                    None => meta.threads.remove(id),
                };
                ui.close();
            }
            ui.separator();
            if ui.button("Remove from this scene").clicked() {
                meta.threads.remove(id);
                remove = true;
                ui.close();
            }
        });
        remove
    }

    #[cfg(test)]
    pub(super) fn toggle_in_scene_for_test(&mut self, scene: &str, kind: &str, profile: &str) {
        self.toggle_in_scene(scene, kind, profile);
    }

    /// The columns: each scene's title, and whether it's only planned.
    #[cfg(test)]
    pub(super) fn tracker_columns(&self) -> Vec<(String, bool)> {
        self.tracker.as_ref().map(|g| g.columns.iter().map(|c| (c.title.clone(), c.planned)).collect()).unwrap_or_default()
    }

    /// Where a column dragged from `from` to the gap `gap` would go.
    #[cfg(test)]
    pub(super) fn tracker_drop(&self, from: usize, gap: usize) -> Option<Place> {
        place_for(self.tracker.as_ref()?, from, gap)
    }

    #[cfg(test)]
    pub(super) fn tracker_rows(&self) -> Vec<(String, Vec<Cell>, Vec<Issue>)> {
        self.tracker.as_ref().map(|g| g.rows.iter().map(|r| (r.title.clone(), r.cells.clone(), r.issues.clone())).collect()).unwrap_or_default()
    }
}

/// The key above the grid: what the marks mean.
fn legend(ui: &mut egui::Ui, threads: bool, notes: bool) {
    let items: &[(Cell, &str)] = if threads {
        &[(Cell::Opens, "opens"), (Cell::Moves, "moves along"), (Cell::Closes, "closes")]
    } else {
        &[(Cell::In, "in the scene"), (Cell::Mentioned, "named in the text only")]
    };
    // Each mark with its words, so they wrap together.
    let key = |ui: &mut egui::Ui, mark: &dyn Fn(&egui::Painter, Pos2, Color32), words: &str| {
        let color = ui.visuals().weak_text_color();
        let galley = ui.painter().layout_no_wrap(words.to_owned(), egui::FontId::proportional(10.0), color);
        let size = Vec2::new(CELL + 2.0 + galley.size().x + 10.0, CELL);
        let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
        mark(ui.painter(), rect.left_center() + Vec2::new(CELL / 2.0, 0.0), ui.visuals().text_color());
        ui.painter().galley(rect.left_center() + Vec2::new(CELL + 2.0, -galley.size().y / 2.0), galley, color);
    };
    for (cell, words) in items {
        key(ui, &|p, c, color| draw(p, c, *cell, color), words);
    }
    key(ui, &draw_planned, "noted, not in it");
    let hint = if notes {
        "Click a square to write what it does in that scene. Click a scene's title to open it; drag it to move the scene; right-click it to add a scene beside it. \u{2325}-click a square to put it in the scene's In This Scene, or take it out."
    } else {
        "Click a mark to open the scene; right-click a square to write a note in it. \u{2325}-click a square to put it in the scene's In This Scene, or take it out. Above the grid, a mark for each scene (hollow: no words yet): drag it to move the scene, right-click it to add a scene beside it."
    };
    ui.label(RichText::new("How to use").small().weak().underline()).on_hover_text(hint);
}

/// A note where it isn't in the scene (yet): a hollow square.
fn draw_planned(painter: &egui::Painter, c: Pos2, color: Color32) {
    let rect = egui::Rect::from_center_size(c, Vec2::splat(9.0));
    painter.rect_stroke(rect, 2.0, Stroke::new(1.2, color), egui::StrokeKind::Inside);
}

/// A cell's mark, drawn (the bundled fonts don't all have the shapes).
fn draw(painter: &egui::Painter, c: Pos2, cell: Cell, color: Color32) {
    let r = 4.5;
    match cell {
        Cell::Empty => {}
        Cell::In | Cell::Moves => {
            painter.circle_filled(c, r, color);
        }
        Cell::Mentioned => {
            painter.circle_stroke(c, r - 0.5, Stroke::new(1.2, color));
            // The left half filled: there, but only in name.
            let half: Vec<Pos2> = (0..=12).map(|i| {
                let a = std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * i as f32 / 12.0;
                c + Vec2::new(a.cos(), a.sin()) * r
            }).collect();
            painter.add(egui::Shape::convex_polygon(half, color, Stroke::NONE));
        }
        Cell::Opens => {
            painter.circle_stroke(c, r - 0.5, Stroke::new(1.6, color));
        }
        Cell::Closes => {
            let d = r + 0.5;
            let points = vec![c + Vec2::new(0.0, -d), c + Vec2::new(d, 0.0), c + Vec2::new(0.0, d), c + Vec2::new(-d, 0.0)];
            painter.add(egui::Shape::convex_polygon(points, color, Stroke::NONE));
        }
    }
}

fn words(n: usize) -> String {
    if n >= 1000 { format!("{},{:03}", n / 1000, n % 1000) } else { n.to_string() }
}

fn away(gap: &Gap, grid: &Grid) -> String {
    format!(
        "{} scene{} ({} words) after \u{201c}{}\u{201d}",
        gap.scenes,
        if gap.scenes == 1 { "" } else { "s" },
        words(gap.words),
        grid.columns[gap.after].title
    )
}

/// What a row's second line says.
fn row_line(grid: &Grid, row: &Row) -> (String, bool) {
    if let Some(issue) = row.issues.first() {
        let more = if row.issues.len() > 1 { format!(" (+{})", row.issues.len() - 1) } else { String::new() };
        return (format!("{}{more}", issue_text(issue, grid)), true);
    }
    if grid.threads {
        let opens = row.cells.iter().position(|c| *c == Cell::Opens);
        let closes = row.cells.iter().position(|c| *c == Cell::Closes);
        return match (opens, closes) {
            (Some(o), Some(c)) => (format!("Opens in \u{201c}{}\u{201d}, closes in \u{201c}{}\u{201d}", grid.columns[o].title, grid.columns[c].title), false),
            _ => ("Not in any scene yet".to_owned(), false),
        };
    }
    let s = &row.summary;
    match (s.scenes, &s.gap) {
        (0, _) => ("In no scene here".to_owned(), false),
        (n, Some(gap)) if gap.scenes >= 2 => (format!("{n} scene{} \u{00b7} away {}", if n == 1 { "" } else { "s" }, away(gap, grid)), false),
        (n, _) => (format!("{n} scene{}", if n == 1 { "" } else { "s" }), false),
    }
}

fn issue_text(issue: &Issue, grid: &Grid) -> String {
    match issue {
        Issue::Away(gap) => format!("Away for {}", away(gap, grid)),
        Issue::NotIn(scenes) => match scenes.as_slice() {
            [one] => format!("Noted in \u{201c}{}\u{201d}, not in it", grid.columns[*one].title),
            many => format!("Noted in {} written scenes it isn't in", many.len()),
        },
        Issue::NeverOpened => "No scene opens it".to_owned(),
        Issue::ClosedBeforeOpened => "Closes before it opens".to_owned(),
        Issue::NeverClosed => "Never closed".to_owned(),
        Issue::Quiet(gap) => format!("Quiet for {}", away(gap, grid)),
        Issue::OutOfOrder(other) => format!("Closes before \u{201c}{other}\u{201d}, which opened inside it"),
    }
}

/// A row's name and its figures, in the fixed column on the left.
fn name_cell(ui: &mut egui::Ui, grid: &Grid, row: &Row, layout: Layout, acts: &mut Vec<Act>) {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(layout.names, layout.row), Sense::click());
    let painter = ui.painter_at(rect.with_max_x(rect.right() - GUTTER));
    let visuals = ui.visuals();
    let (line, warn) = row_line(grid, row);
    let strong = egui::FontId::proportional(14.0);
    let small = egui::FontId::proportional(11.0);
    painter.text(rect.left_top() + Vec2::new(0.0, 3.0), egui::Align2::LEFT_TOP, &row.title, strong, if response.hovered() { visuals.hyperlink_color } else { visuals.text_color() });
    let line_color = if warn { visuals.warn_fg_color } else { visuals.weak_text_color() };
    painter.text(rect.left_top() + Vec2::new(0.0, 21.0), egui::Align2::LEFT_TOP, &line, small, line_color);
    let mut hover = format!("{}\nClick to open the profile; right-click to choose what it warns about.", row.title);
    for issue in row.issues.iter().skip(1) {
        hover.push_str(&format!("\nAlso: {}", issue_text(issue, grid)));
    }
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(hover);
    if response.clicked() {
        acts.push(Act::Open(row.id.clone()));
    }
    response.context_menu(|ui| {
        let mut rules = row.rules.clone();
        rules_menu(ui, &mut rules, grid.threads);
        if rules != row.rules {
            acts.push(Act::Rules { profile: row.id.clone(), rules });
        }
    });
}

/// What a row warns about: set in its name's right-click menu.
fn rules_menu(ui: &mut egui::Ui, rules: &mut TrackRules, threads: bool) {
    ui.label(RichText::new("Warn about").small().weak());
    // How long away (a thread: quiet) before it says so.
    let usual = if threads { Some(presence::QUIET) } else { None };
    let limit = match rules.away {
        Some(0) => None,
        Some(n) => Some(n),
        None => usual,
    };
    let mut on = limit.is_some();
    let mut n = limit.unwrap_or(if threads { presence::QUIET } else { 6 });
    ui.horizontal(|ui| {
        ui.checkbox(&mut on, if threads { "Quiet for" } else { "Away for" });
        ui.add_enabled(on, egui::DragValue::new(&mut n).range(1..=99));
        ui.label("scenes or more");
    });
    let chosen = if on { Some(n) } else { None };
    if chosen != limit {
        rules.away = match chosen {
            c if c == usual => None,
            Some(n) => Some(n),
            None => Some(0),
        };
    }
    if threads {
        let mut never = !rules.open_on_purpose;
        ui.checkbox(&mut never, "Never closed").on_hover_text("Off for a thread left open on purpose: for a sequel, say");
        rules.open_on_purpose = !never;
        let mut order = !rules.any_order;
        ui.checkbox(&mut order, "Closing out of order").on_hover_text("Threads usually close in the reverse of the order they opened; off lets this one close whenever");
        rules.any_order = !order;
    }
    let mut notes = !rules.ignore_notes;
    ui.checkbox(&mut notes, "Noted in a scene, not in it").on_hover_text("A written scene with a note for it, without it in In This Scene");
    rules.ignore_notes = !notes;
}

/// The chapters over the columns.
fn chapters(ui: &mut egui::Ui, grid: &Grid, layout: Layout) {
    let w = layout.cell;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w * grid.columns.len() as f32, CHAPTERS), Sense::hover());
    let painter = ui.painter_at(rect);
    let mut start = 0;
    while start < grid.columns.len() {
        let chapter = &grid.columns[start].chapter;
        let end = grid.columns[start..].iter().position(|c| &c.chapter != chapter).map_or(grid.columns.len(), |n| start + n);
        let span = egui::Rect::from_min_max(rect.left_top() + Vec2::new(start as f32 * w, 0.0), rect.left_top() + Vec2::new(end as f32 * w, CHAPTERS));
        if start > 0 {
            painter.vline(span.left(), span.y_range(), Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color));
        }
        if !chapter.is_empty() {
            let clipped = painter.with_clip_rect(span.shrink2(Vec2::new(3.0, 0.0)));
            clipped.text(span.left_center() + Vec2::new(4.0, 0.0), egui::Align2::LEFT_CENTER, chapter, egui::FontId::proportional(11.0), ui.visuals().weak_text_color());
        }
        start = end;
    }
}

/// Where a column dropped at `x` goes: the gap between columns nearest
/// it (0 is before the first, `len` after the last).
fn drop_gap(left: f32, x: f32, cell: f32, len: usize) -> usize {
    (((x - left) / cell) + 0.5).floor().clamp(0.0, len as f32) as usize
}

/// Where a column moved to `gap` goes in the binder, or `None` if that's
/// where it is.
fn place_for(grid: &Grid, from: usize, gap: usize) -> Option<Place> {
    if gap == from || gap == from + 1 {
        return None;
    }
    Some(match grid.columns.get(gap) {
        Some(c) => Place::Before(c.id.clone()),
        None => Place::After(grid.columns.last()?.id.clone()),
    })
}

/// A line over the columns for each scene: a mark (or, with notes shown,
/// its title), hollow for a scene with no words yet. Click to open it,
/// drag to move it, right-click to add a scene beside it.
fn scenes(ui: &mut egui::Ui, grid: &Grid, layout: Layout, acts: &mut Vec<Act>) {
    let w = layout.cell;
    let h = layout.header() - CHAPTERS;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w * grid.columns.len() as f32, h), Sense::hover());
    let visuals = ui.visuals().clone();
    let painter = ui.painter_at(rect.expand2(Vec2::new(2.0, 0.0)));
    let mut dropping = None;
    for (i, column) in grid.columns.iter().enumerate() {
        let r = egui::Rect::from_min_size(rect.left_top() + Vec2::new(i as f32 * w, 0.0), Vec2::new(w, h));
        let response = ui.interact(r, ui.scope_id().with(("tracker-scene", &column.id)), Sense::click_and_drag());
        let color = if response.hovered() || response.dragged() { visuals.hyperlink_color } else { visuals.weak_text_color() };
        if layout.notes {
            let clipped = painter.with_clip_rect(r.shrink2(Vec2::new(4.0, 0.0)));
            let mut text = RichText::new(&column.title).size(12.0);
            text = if column.planned { text.italics() } else { text.strong() };
            let galley = egui::WidgetText::from(text).into_galley(ui, Some(egui::TextWrapMode::Truncate), w - 8.0, egui::FontSelection::Default);
            clipped.galley(Pos2::new(r.left() + 4.0, r.center().y - galley.size().y / 2.0), galley, color);
        } else {
            let mark = egui::Rect::from_center_size(r.center(), Vec2::new(8.0, 6.0));
            if column.planned {
                painter.rect_stroke(mark, 1.5, Stroke::new(1.0, color), egui::StrokeKind::Inside);
            } else {
                painter.rect_filled(mark, 1.5, color);
            }
        }
        let response = response.on_hover_ui(|ui| {
            ui.set_max_width(300.0);
            ui.label(RichText::new(&column.title).strong());
            if !column.chapter.is_empty() {
                ui.weak(&column.chapter);
            }
            if !column.synopsis.trim().is_empty() {
                ui.label(column.synopsis.trim());
            }
            if column.planned {
                ui.weak("Planned: no words yet");
            } else {
                ui.weak(format!("{} words", words(grid.words[i])));
            }
            ui.weak("Click to open; drag to move; right-click to add a scene beside it.");
        });
        if response.clicked() {
            acts.push(Act::Open(column.id.clone()));
        }
        let pointer = ui.ctx().pointer_latest_pos();
        if response.dragged()
            && let Some(p) = pointer
        {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            dropping = Some(drop_gap(rect.left(), p.x, w, grid.columns.len()));
        }
        if response.drag_stopped()
            && let Some(p) = pointer
            && let Some(place) = place_for(grid, i, drop_gap(rect.left(), p.x, w, grid.columns.len()))
        {
            acts.push(Act::Move { scene: column.id.clone(), place });
        }
        let at = pointer.unwrap_or(r.center());
        response.context_menu(|ui| {
            if ui.button("Open").clicked() {
                acts.push(Act::Open(column.id.clone()));
                ui.close();
            }
            if ui.button("New Scene Before\u{2026}").clicked() {
                acts.push(Act::NewScene { place: Place::Before(column.id.clone()), at });
                ui.close();
            }
            if ui.button("New Scene After\u{2026}").clicked() {
                acts.push(Act::NewScene { place: Place::After(column.id.clone()), at });
                ui.close();
            }
        });
    }
    // Where a dragged scene would go.
    if let Some(gap) = dropping {
        let x = rect.left() + gap as f32 * w;
        painter.vline(x, rect.y_range(), Stroke::new(2.0, visuals.hyperlink_color));
    }
}

/// A row's cells.
fn cells(ui: &mut egui::Ui, grid: &Grid, row: &Row, layout: Layout, acts: &mut Vec<Act>) {
    let w = layout.cell;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w * grid.columns.len() as f32, layout.row), Sense::hover());
    let visuals = ui.visuals().clone();
    // The longest time away (or a quiet thread), shaded behind its scenes.
    let shaded: Option<(Gap, bool)> = if grid.threads {
        row.issues.iter().find_map(|i| if let Issue::Quiet(g) = i { Some((*g, true)) } else { None })
    } else if let Some(gap) = row.issues.iter().find_map(|i| if let Issue::Away(g) = i { Some(*g) } else { None }) {
        Some((gap, true))
    } else {
        row.summary.gap.filter(|g| g.scenes >= 2).map(|g| (g, false))
    };
    let painter = ui.painter_at(rect);
    // A line along the row (through the marks), and the chapter breaks.
    let line_y = if layout.notes { rect.top() + 14.0 } else { rect.center().y };
    let rule = visuals.widgets.noninteractive.bg_stroke.color;
    painter.hline(rect.x_range(), line_y, Stroke::new(1.0, rule.gamma_multiply(0.6)));
    if layout.notes {
        painter.hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, rule.gamma_multiply(0.4)));
    }
    if let Some((gap, warn)) = shaded {
        let tint = if warn { visuals.warn_fg_color.gamma_multiply(0.12) } else { visuals.selection.bg_fill.gamma_multiply(0.18) };
        let x0 = rect.left() + (gap.after + 1) as f32 * w;
        let x1 = rect.left() + gap.before as f32 * w;
        painter.rect_filled(egui::Rect::from_min_max(Pos2::new(x0, rect.top() + 8.0), Pos2::new(x1, rect.bottom() - 8.0)), 3.0, tint);
    }
    let mut chapter: Option<&str> = None;
    for (i, (column, cell)) in grid.columns.iter().zip(&row.cells).enumerate() {
        let cell_rect = egui::Rect::from_min_size(rect.left_top() + Vec2::new(i as f32 * w, 0.0), Vec2::new(w, layout.row));
        if chapter.is_some_and(|c| c != column.chapter) {
            painter.vline(cell_rect.left(), cell_rect.y_range(), Stroke::new(1.0, rule));
        } else if layout.notes && i > 0 {
            painter.vline(cell_rect.left(), cell_rect.y_range(), Stroke::new(1.0, rule.gamma_multiply(0.4)));
        }
        chapter = Some(&column.chapter);
        let note = column.note(&row.id);
        let response = ui.interact(cell_rect, ui.scope_id().with(("tracker-cell", &row.id, i)), Sense::click());
        let color = if response.hovered() { visuals.hyperlink_color } else if *cell == Cell::Mentioned { visuals.weak_text_color() } else { visuals.text_color() };
        if response.hovered() {
            painter.rect_filled(cell_rect.shrink(1.0), 3.0, visuals.widgets.hovered.weak_bg_fill);
        }
        let mark_at = if layout.notes { Pos2::new(cell_rect.left() + 12.0, line_y) } else { cell_rect.center() };
        let missing = row.not_in().contains(&i);
        if missing {
            draw_planned(&painter, mark_at, if response.hovered() { color } else { visuals.warn_fg_color });
        } else if *cell == Cell::Empty && note.is_some() {
            draw_planned(&painter, mark_at, color);
        } else {
            draw(&painter, mark_at, *cell, color);
        }
        if let Some(note) = note {
            if layout.notes {
                // The note, under the mark, as much as fits.
                let text_rect = egui::Rect::from_min_max(Pos2::new(cell_rect.left() + 5.0, line_y + 8.0), cell_rect.max - Vec2::new(5.0, 3.0));
                let galley = painter.layout(note.to_owned(), egui::FontId::proportional(11.0), visuals.text_color(), text_rect.width());
                painter.with_clip_rect(text_rect).galley(text_rect.left_top(), galley, visuals.text_color());
            } else if cell.present() {
                // A corner turned down: there's a note.
                let c = cell_rect.right_top() + Vec2::new(-2.0, 9.0);
                let corner = vec![c, c + Vec2::new(-5.0, 0.0), c + Vec2::new(0.0, 5.0)];
                painter.add(egui::Shape::convex_polygon(corner, visuals.weak_text_color(), Stroke::NONE));
            }
        }
        let state = match cell {
            _ if missing => "A note, but not in In This Scene, and the scene's written",
            Cell::Empty if note.is_some() => "A note, not in the scene yet",
            Cell::Empty => "Not here",
            Cell::Mentioned => "Named in the text, not in In This Scene",
            Cell::In => "In the scene",
            Cell::Opens => "Opens here",
            Cell::Moves => "Moves along",
            Cell::Closes => "Closes here",
        };
        let response = response.on_hover_ui(|ui| {
            ui.set_max_width(300.0);
            ui.label(RichText::new(&column.title).strong());
            if !column.chapter.is_empty() {
                ui.weak(&column.chapter);
            }
            if !column.synopsis.trim().is_empty() {
                ui.label(column.synopsis.trim());
            }
            ui.label(format!("{}: {state}", row.title));
            if let Some(note) = note {
                ui.label(RichText::new(note).italics());
            }
            if column.planned {
                ui.weak("Planned: no words yet");
            } else {
                ui.weak(format!("{} words", words(grid.words[i])));
            }
        });
        let at = ui.ctx().pointer_latest_pos().unwrap_or(cell_rect.center());
        let note_act = || Act::Note { scene: column.id.clone(), profile: row.id.clone(), at };
        let toggle = || Act::Toggle { scene: column.id.clone(), profile: row.id.clone() };
        if response.clicked() {
            if ui.input(|i| i.modifiers.alt) {
                acts.push(toggle());
            } else if layout.notes {
                acts.push(note_act());
            } else {
                acts.push(Act::Open(column.id.clone()));
            }
        }
        response.context_menu(|ui| {
            if ui.button(if note.is_some() { "Edit the Note\u{2026}" } else { "Write a Note\u{2026}" }).clicked() {
                acts.push(note_act());
                ui.close();
            }
            if ui.button("Open the Scene").clicked() {
                acts.push(Act::Open(column.id.clone()));
                ui.close();
            }
            let linked = matches!(cell, Cell::In | Cell::Opens | Cell::Moves | Cell::Closes);
            if ui.button(if linked { "Take Out of In This Scene" } else { "Put in In This Scene" }).clicked() {
                acts.push(toggle());
                ui.close();
            }
        });
    }
}
