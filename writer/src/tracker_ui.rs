//! The Tracker, a folder's third view: a grid of profiles (characters,
//! locations, threads…) against the folder's scenes in order, showing who's
//! in each scene, who's only mentioned, how long anyone's away, and how
//! threads open and close (see presence.rs). Also the thread marks in a
//! scene's inspector.

use eframe::egui::{self, Color32, Pos2, RichText, Sense, Stroke, Vec2};

use super::WriterApp;
use crate::character::Sheet;
use crate::presence::{self, Cell, Gap, Issue, Scene, Summary};
use crate::project::{CHARACTERS, Meta, THREADS, ThreadMark};

/// Seconds a worked-out grid is kept before it's worked out again (texts
/// change as you write).
const REFRESH: f64 = 2.0;
const CELL: f32 = 16.0;
const ROW: f32 = 38.0;
const NAMES: f32 = 300.0;
/// Space between the names and the grid.
const GUTTER: f32 = 12.0;
const HEADER: f32 = 22.0;

/// A scene as the grid's columns show it.
pub(super) struct Column {
    id: String,
    title: String,
    chapter: String,
    synopsis: String,
}

/// A profile's row.
pub(super) struct Row {
    id: String,
    title: String,
    cells: Vec<Cell>,
    summary: Summary,
    issues: Vec<Issue>,
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
enum Act {
    Open(String),
    /// ⌥-click: in the scene, or out of it.
    Toggle { scene: String, profile: String },
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
                let column = Column { id: n.id.clone(), title: n.title.clone(), chapter: chapter.clone(), synopsis: n.meta.synopsis.clone() };
                found.push((column, n.meta.links.get(kind).cloned().unwrap_or_default(), n.meta.threads.clone()));
            }
        }
        // The rows: the kind's profiles, with the names to look for.
        let profiles: Vec<(String, String)> = project.profile_items(kind).into_iter().map(|n| (n.id.clone(), n.title.clone())).collect();
        let mut scenes = Vec::new();
        let mut columns = Vec::new();
        for (column, links, marks) in found {
            let text = crate::compile::strip_notes(&self.document_text(&column.id).unwrap_or_default());
            let words = self.words_under(&column.id);
            scenes.push(Scene { words, text, links, marks });
            columns.push(column);
        }
        let mut rows = Vec::new();
        for (id, title) in profiles {
            let names = if threads {
                Vec::new()
            } else {
                let sheet = Sheet::parse(&self.document_text(&id).unwrap_or_default());
                let aliases = sheet.sections.iter().find(|s| s.heading.trim().eq_ignore_ascii_case("Also known as")).map(|s| s.text.clone()).unwrap_or_default();
                presence::names(&title, &aliases)
            };
            let cells: Vec<Cell> = scenes.iter().map(|s| presence::cell(s, &id, &names)).collect();
            let summary = presence::summary(&cells, &scenes);
            rows.push(Row { id, title, cells, summary, issues: Vec::new() });
        }
        if threads {
            let pairs: Vec<(String, Vec<Cell>)> = rows.iter().map(|r| (r.title.clone(), r.cells.clone())).collect();
            for (row, issues) in rows.iter_mut().zip(presence::thread_issues(&pairs, &scenes)) {
                row.issues = issues;
            }
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
        ui.horizontal(|ui| {
            let name = kinds.iter().find(|(id, _)| *id == kind).map(|(_, n)| n.clone()).unwrap_or_default();
            egui::ComboBox::from_id_salt("tracker-kind").selected_text(name).show_ui(ui, |ui| {
                for (id, name) in &kinds {
                    ui.selectable_value(&mut kind, id.clone(), name);
                }
            });
            ui.add_space(12.0);
            legend(ui, threads);
        });
        if kind != self.tracker_kind() {
            self.tracker_kind = Some(kind.clone());
            self.tracker = None;
        }
        ui.add_space(6.0);
        let kind_name = kinds.iter().find(|(id, _)| *id == kind).map(|(_, n)| n.clone()).unwrap_or_default();
        let Some(grid) = self.tracker_grid(folder, now) else { return };
        if grid.columns.is_empty() {
            ui.weak("No documents in this folder to track.");
            return;
        }
        if grid.rows.is_empty() {
            let hint = if threads {
                "No threads yet. Add a Threads folder (New Profile Folder, in the binder's right-click menu on a top-level folder), a thread for each question the story opens, then mark in each scene's inspector where it opens, moves and closes."
            } else {
                "Nothing of this kind yet: add some in its folder."
            };
            ui.weak(format!("{kind_name}: {hint}"));
            return;
        }
        let mut acts = Vec::new();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                ui.vertical(|ui| {
                    ui.set_width(NAMES);
                    ui.add_space(HEADER);
                    for row in &grid.rows {
                        name_cell(ui, grid, row, &mut acts);
                    }
                });
                egui::ScrollArea::horizontal().id_salt("tracker-cells").auto_shrink([false, true]).show(ui, |ui| {
                    ui.vertical(|ui| {
                        chapters(ui, grid);
                        for row in &grid.rows {
                            cells(ui, grid, row, &mut acts);
                        }
                    });
                });
            });
        });
        for act in acts {
            match act {
                Act::Open(id) => self.select(id),
                Act::Toggle { scene, profile } => self.toggle_in_scene(&scene, &kind, &profile),
            }
        }
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

    #[cfg(test)]
    pub(super) fn tracker_rows(&self) -> Vec<(String, Vec<Cell>, Vec<Issue>)> {
        self.tracker.as_ref().map(|g| g.rows.iter().map(|r| (r.title.clone(), r.cells.clone(), r.issues.clone())).collect()).unwrap_or_default()
    }
}

/// The key above the grid: what the marks mean.
fn legend(ui: &mut egui::Ui, threads: bool) {
    let items: &[(Cell, &str)] = if threads {
        &[(Cell::Opens, "opens"), (Cell::Moves, "moves along"), (Cell::Closes, "closes")]
    } else {
        &[(Cell::In, "in the scene"), (Cell::Mentioned, "named in the text only")]
    };
    for (cell, words) in items {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(CELL), Sense::hover());
        draw(ui.painter(), rect.center(), *cell, ui.visuals().text_color());
        ui.label(RichText::new(*words).small().weak());
        ui.add_space(8.0);
    }
    let hint = "Click a mark to open the scene; \u{2325}-click a square to put it in the scene's In This Scene, or take it out.";
    ui.label(RichText::new("How to use").small().weak().underline()).on_hover_text(hint);
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
    if grid.threads {
        if let Some(issue) = row.issues.first() {
            let more = if row.issues.len() > 1 { format!(" (+{})", row.issues.len() - 1) } else { String::new() };
            return (format!("{}{more}", issue_text(issue, grid)), true);
        }
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
        Issue::NeverOpened => "No scene opens it".to_owned(),
        Issue::ClosedBeforeOpened => "Closes before it opens".to_owned(),
        Issue::NeverClosed => "Never closed".to_owned(),
        Issue::Quiet(gap) => format!("Quiet for {}", away(gap, grid)),
        Issue::OutOfOrder(other) => format!("Closes before \u{201c}{other}\u{201d}, which opened inside it"),
    }
}

/// A row's name and its figures, in the fixed column on the left.
fn name_cell(ui: &mut egui::Ui, grid: &Grid, row: &Row, acts: &mut Vec<Act>) {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(NAMES, ROW), Sense::click());
    let painter = ui.painter_at(rect.with_max_x(rect.right() - GUTTER));
    let visuals = ui.visuals();
    let (line, warn) = row_line(grid, row);
    let strong = egui::FontId::proportional(14.0);
    let small = egui::FontId::proportional(11.0);
    painter.text(rect.left_top() + Vec2::new(0.0, 3.0), egui::Align2::LEFT_TOP, &row.title, strong, if response.hovered() { visuals.hyperlink_color } else { visuals.text_color() });
    let line_color = if warn { visuals.warn_fg_color } else { visuals.weak_text_color() };
    painter.text(rect.left_top() + Vec2::new(0.0, 21.0), egui::Align2::LEFT_TOP, &line, small, line_color);
    let mut hover = format!("{}\nClick to open the profile.", row.title);
    for issue in row.issues.iter().skip(1) {
        hover.push_str(&format!("\nAlso: {}", issue_text(issue, grid)));
    }
    if response.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(hover).clicked() {
        acts.push(Act::Open(row.id.clone()));
    }
}

/// The chapters over the columns.
fn chapters(ui: &mut egui::Ui, grid: &Grid) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(CELL * grid.columns.len() as f32, HEADER), Sense::hover());
    let painter = ui.painter_at(rect);
    let mut start = 0;
    while start < grid.columns.len() {
        let chapter = &grid.columns[start].chapter;
        let end = grid.columns[start..].iter().position(|c| &c.chapter != chapter).map_or(grid.columns.len(), |n| start + n);
        let span = egui::Rect::from_min_max(rect.left_top() + Vec2::new(start as f32 * CELL, 0.0), rect.left_top() + Vec2::new(end as f32 * CELL, HEADER));
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

/// A row's cells.
fn cells(ui: &mut egui::Ui, grid: &Grid, row: &Row, acts: &mut Vec<Act>) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(CELL * grid.columns.len() as f32, ROW), Sense::hover());
    let visuals = ui.visuals().clone();
    // The longest time away (or a quiet thread), shaded behind its scenes.
    let shaded: Option<(Gap, bool)> = if grid.threads {
        row.issues.iter().find_map(|i| if let Issue::Quiet(g) = i { Some((*g, true)) } else { None })
    } else {
        row.summary.gap.filter(|g| g.scenes >= 2).map(|g| (g, false))
    };
    let painter = ui.painter_at(rect);
    // A line along the row, and the chapter breaks.
    painter.hline(rect.x_range(), rect.center().y, Stroke::new(1.0, visuals.widgets.noninteractive.bg_stroke.color.gamma_multiply(0.6)));
    if let Some((gap, warn)) = shaded {
        let tint = if warn { visuals.warn_fg_color.gamma_multiply(0.12) } else { visuals.selection.bg_fill.gamma_multiply(0.18) };
        let x0 = rect.left() + (gap.after + 1) as f32 * CELL;
        let x1 = rect.left() + gap.before as f32 * CELL;
        painter.rect_filled(egui::Rect::from_min_max(Pos2::new(x0, rect.top() + 8.0), Pos2::new(x1, rect.bottom() - 8.0)), 3.0, tint);
    }
    let mut chapter: Option<&str> = None;
    for (i, (column, cell)) in grid.columns.iter().zip(&row.cells).enumerate() {
        let cell_rect = egui::Rect::from_min_size(rect.left_top() + Vec2::new(i as f32 * CELL, 0.0), Vec2::new(CELL, ROW));
        if chapter.is_some_and(|c| c != column.chapter) {
            painter.vline(cell_rect.left(), cell_rect.y_range(), Stroke::new(1.0, visuals.widgets.noninteractive.bg_stroke.color));
        }
        chapter = Some(&column.chapter);
        let response = ui.interact(cell_rect, ui.scope_id().with(("tracker-cell", &row.id, i)), Sense::click());
        let color = if response.hovered() { visuals.hyperlink_color } else if *cell == Cell::Mentioned { visuals.weak_text_color() } else { visuals.text_color() };
        if response.hovered() {
            painter.rect_filled(cell_rect.shrink(1.0), 3.0, visuals.widgets.hovered.weak_bg_fill);
        }
        draw(&painter, cell_rect.center(), *cell, color);
        let state = match cell {
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
            ui.weak(format!("{} words", words(grid.words[i])));
        });
        if response.clicked() {
            if ui.input(|i| i.modifiers.alt) {
                acts.push(Act::Toggle { scene: column.id.clone(), profile: row.id.clone() });
            } else {
                acts.push(Act::Open(column.id.clone()));
            }
        }
    }
}
