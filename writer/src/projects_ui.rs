//! Around projects rather than inside one: the recent projects on the
//! start screen, and renaming the open project (its title, and its folder
//! if that was named after it).

use std::fs;
use std::path::{Path, PathBuf};

use eframe::egui::{self, RichText};

use super::WriterApp;
use super::settings_ui::StartView;
use crate::project::{Kind, MANIFEST_FILE, Manifest, Role};
use crate::stats::{self, Stats};

/// Where the recent projects are remembered between launches.
pub(super) const RECENT_KEY: &str = "recent_projects";
/// How many the start screen lists.
const RECENT_MAX: usize = 10;

/// The Rename Project window's state.
pub(super) struct Renaming {
    title: String,
    /// Rename the folder to match.
    folder_too: bool,
    /// Put the cursor in the title field (once, when the window opens).
    focus: bool,
}

/// Put `root` at the top of the recent list.
pub(super) fn remember(recent: &mut Vec<PathBuf>, root: &Path) {
    recent.retain(|p| p != root);
    recent.insert(0, root.to_path_buf());
    recent.truncate(RECENT_MAX);
}

/// A project's title and kind, read from its manifest without opening it.
pub(super) fn title_of(root: &Path) -> Option<(String, Kind)> {
    let text = fs::read_to_string(root.join(MANIFEST_FILE)).ok()?;
    ron::from_str::<Manifest>(&text).ok().map(|m| (m.title, m.kind))
}

/// What a recent project's tile shows besides its name: its words, its
/// target, when it was last written in, and its streak. Worked out once
/// (it reads the project's files), and again after the project's closed.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct TileFacts {
    pub(super) words: u32,
    pub(super) target: Option<u32>,
    pub(super) last: Option<chrono::NaiveDate>,
    pub(super) streak: u32,
}

impl TileFacts {
    #[cfg(test)]
    pub(super) fn for_test(words: u32, target: Option<u32>, last: Option<chrono::NaiveDate>, streak: u32) -> Self {
        Self { words, target, last, streak }
    }
}

pub(super) fn facts_of(root: &Path) -> Option<TileFacts> {
    let manifest: Manifest = ron::from_str(&fs::read_to_string(root.join(MANIFEST_FILE)).ok()?).ok()?;
    let manuscript = manifest.binder.iter().find(|n| n.role == Some(Role::Manuscript))?;
    let words = manuscript
        .subtree()
        .into_iter()
        .filter(|n| n.is_document())
        .filter_map(|n| fs::read_to_string(root.join(n.path.as_ref()?)).ok())
        .map(|text| super::inspector::count_words(&text) as u32)
        .sum();
    let log = Stats::load(root);
    let last = log.days.iter().rev().find(|(_, w)| **w > 0).and_then(|(day, _)| stats::parse_date(day));
    Some(TileFacts { words, target: manifest.goals.total.filter(|&t| t > 0), last, streak: log.streak(stats::today(), manifest.goals.daily) })
}

/// "today", "yesterday", "3 days ago", "2 weeks ago", "5 months ago".
pub(super) fn ago(day: chrono::NaiveDate, today: chrono::NaiveDate) -> String {
    let days = (today - day).num_days().max(0);
    match days {
        0 => "today".to_owned(),
        1 => "yesterday".to_owned(),
        2..=13 => format!("{days} days ago"),
        14..=59 => format!("{} weeks ago", days / 7),
        60..=729 => format!("{} months ago", days / 30),
        _ => format!("{} years ago", days / 365),
    }
}

/// A kind's colour.
pub(super) fn kind_color(kind: Kind) -> egui::Color32 {
    let (r, g, b) = kind.color();
    egui::Color32::from_rgb(r, g, b)
}

/// The kinds to choose from, each with its icon in its colour, in three
/// columns. Returns the one clicked.
pub(super) fn kind_picker(ui: &mut egui::Ui, id: &str, current: Kind) -> Option<Kind> {
    let mut picked = None;
    egui::Grid::new(id).num_columns(3).spacing([4.0, 2.0]).show(ui, |ui| {
        let style = ui.style().clone();
        for (i, kind) in Kind::ALL.into_iter().enumerate() {
            let mut label = egui::text::LayoutJob::default();
            let (font, align) = (egui::FontSelection::Default, egui::Align::Center);
            RichText::new(kind.icon()).color(kind_color(kind)).append_to(&mut label, &style, font.clone(), align);
            RichText::new(format!("  {}", kind.name())).append_to(&mut label, &style, font, align);
            if ui.selectable_label(kind == current, label).clicked() {
                picked = Some(kind);
            }
            if i % 3 == 2 {
                ui.end_row();
            }
        }
    });
    picked
}

/// A recent project's tile.
const TILE: egui::Vec2 = egui::vec2(184.0, 104.0);

pub(super) enum TileChoice {
    Open,
    Forget,
    Kind(Kind),
}

/// One recent project as a tile: its icon in its kind's colour, title,
/// kind and folder. Click to open; right-click to change the icon or take
/// it off the list. A project that's gone is dimmed, with Remove.
fn recent_tile(ui: &mut egui::Ui, root: &Path, facts: Option<&TileFacts>) -> Option<TileChoice> {
    let found = title_of(root);
    let available = found.is_some();
    let (name, kind) = found.unwrap_or_else(|| {
        (root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), Kind::default())
    });
    let place = if available { short_place(root) } else { "(not found)".to_owned() };
    let color = kind_color(kind);

    let sense = if available { egui::Sense::click() } else { egui::Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(TILE, sense);
    // One name for VoiceOver (and tests): the tile's text is painted, not
    // separate labels.
    let summary = if available { format!("{name}, {}, {place}", kind.name()) } else { format!("{name}, {place}") };
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, available, &summary));
    let response = response.on_hover_text(match facts.and_then(|f| f.target.map(|t| (f.words, t))) {
        Some((words, target)) => format!("{}\n{} of {} words", root.display(), stats::thousands(words), stats::thousands(target)),
        None => root.display().to_string(),
    });

    let mut choice = None;
    if response.clicked() {
        choice = Some(TileChoice::Open);
    }
    response.context_menu(|ui| project_menu(ui, available, kind, &mut choice));

    if ui.is_rect_visible(rect) {
        let hovered = available && response.hovered();
        let painter = ui.painter_at(rect.expand(2.0));
        let fade = if available { 1.0 } else { 0.45 };
        let fill = color.gamma_multiply(if hovered { 0.22 } else { 0.12 } * fade);
        let stroke = egui::Stroke::new(if hovered { 1.5 } else { 1.0 }, color.gamma_multiply(if hovered { 0.9 } else { 0.35 } * fade));
        painter.rect(rect, 10.0, fill, stroke, egui::StrokeKind::Inside);

        let inner = rect.shrink2(egui::vec2(14.0, 12.0));
        painter.text(inner.left_top(), egui::Align2::LEFT_TOP, kind.icon(), egui::FontId::proportional(30.0), color.gamma_multiply(fade));
        let line = |text: &str, size: f32, text_color: egui::Color32| {
            let mut job = egui::text::LayoutJob::simple_singleline(text.to_owned(), egui::FontId::proportional(size), text_color);
            job.wrap = egui::text::TextWrapping::truncate_at_width(inner.width());
            ui.fonts_mut(|f| f.layout_job(job))
        };
        let strong = ui.visuals().strong_text_color().gamma_multiply(fade);
        let weak = ui.visuals().weak_text_color().gamma_multiply(fade);
        let title = line(&name, 15.0, strong);
        let detail = match facts {
            Some(f) if available && f.words > 0 => format!("{} \u{b7} {} words", kind.name(), stats::thousands(f.words)),
            _ if available => format!("{} \u{b7} {place}", kind.name()),
            _ => place.clone(),
        };
        let detail = line(&detail, 11.0, weak);
        let detail_top = inner.bottom() - detail.size().y;
        painter.galley(egui::pos2(inner.left(), detail_top - 2.0 - title.size().y), title, strong);
        painter.galley(egui::pos2(inner.left(), detail_top), detail, weak);
        if let Some(f) = facts.filter(|_| available) {
            // When it was last written in, and the streak, at the top right.
            let small = |text: &str| ui.fonts_mut(|fonts| fonts.layout_no_wrap(text.to_owned(), egui::FontId::proportional(10.5), weak));
            let mut y = inner.top();
            for text in [f.last.map(|d| ago(d, stats::today())), (f.streak >= 2).then(|| format!("{} days in a row", f.streak))].into_iter().flatten() {
                let galley = small(&text);
                let height = galley.size().y;
                painter.galley(egui::pos2(inner.right() - galley.size().x, y), galley, weak);
                y += height + 1.0;
            }
            // Towards its target: a thin bar along the bottom edge.
            if let Some(target) = f.target {
                let bar = egui::Rect::from_min_max(egui::pos2(rect.left() + 10.0, rect.bottom() - 5.0), egui::pos2(rect.right() - 10.0, rect.bottom() - 3.0));
                painter.rect_filled(bar, 1.0, color.gamma_multiply(0.18));
                let done = (f.words as f32 / target as f32).clamp(0.0, 1.0);
                painter.rect_filled(egui::Rect::from_min_size(bar.min, egui::vec2(bar.width() * done, bar.height())), 1.0, color.gamma_multiply(0.85));
            }
        }
    }

    // A project that's gone can't be opened, but can be taken off the list.
    if !available {
        let button = egui::Rect::from_min_size(rect.right_top() + egui::vec2(-70.0, 10.0), egui::vec2(60.0, 20.0));
        if ui.put(button, egui::Button::new(RichText::new("Remove").small())).clicked() {
            choice = Some(TileChoice::Forget);
        }
    }
    choice
}

/// A recent project's right-click menu: change its icon, or take it off
/// the list.
pub(super) fn project_menu(ui: &mut egui::Ui, available: bool, kind: Kind, choice: &mut Option<TileChoice>) {
    if available {
        ui.menu_button("Icon", |ui| {
            if let Some(picked) = kind_picker(ui, "tile-kind", kind) {
                *choice = Some(TileChoice::Kind(picked));
                ui.close();
            }
        });
    }
    if ui.button("Remove from List").clicked() {
        *choice = Some(TileChoice::Forget);
        ui.close();
    }
}

/// Where a project lives, briefly: the folder it's in, from the home
/// folder (`~/Documents/Writing`), with the start cut off if it's long.
pub(super) fn short_place(root: &Path) -> String {
    let parent = root.parent().unwrap_or(root).display().to_string();
    let home = std::env::var("HOME").unwrap_or_default();
    let place = match parent.strip_prefix(&home) {
        Some(rest) if !home.is_empty() => format!("~{rest}"),
        _ => parent,
    };
    let chars: Vec<char> = place.chars().collect();
    if chars.len() > 48 { format!("\u{2026}{}", chars[chars.len() - 47..].iter().collect::<String>()) } else { place }
}

/// The project's folder is named after its title (as new projects are).
fn folder_matches_title(root: &Path, title: &str) -> bool {
    root.file_name().is_some_and(|n| n.to_string_lossy() == title)
}

impl WriterApp {
    /// The recent projects, on the start screen: a tile each, with the
    /// icon and colour of its kind, in rows centred under the buttons.
    pub(super) fn recent_list(&mut self, ui: &mut egui::Ui) {
        if self.recent.is_empty() {
            return;
        }
        ui.add_space(28.0);
        ui.horizontal(|ui| {
            let width = 250.0;
            ui.add_space(((ui.available_width() - width) / 2.0).max(0.0));
            ui.label(RichText::new("Recent projects").small().weak());
            ui.add_space(8.0);
            let view = &mut self.settings.start_view;
            ui.selectable_value(view, StartView::Tiles, RichText::new("Tiles").small());
            ui.selectable_value(view, StartView::CardFile, RichText::new("Card File").small())
                .on_hover_text("Flip through your projects, A to Z");
        });
        ui.add_space(6.0);
        if self.settings.start_view == StartView::CardFile {
            let choice = self.card_file(ui);
            self.act_on(choice);
            return;
        }
        let mut open = None;
        let mut forget = None;
        let mut new_kind = None;
        let gap = 10.0;
        let columns = (((ui.available_width() + gap) / (TILE.x + gap)) as usize).clamp(1, 4).min(self.recent.len());
        for row in self.recent.chunks(columns) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                let width = row.len() as f32 * (TILE.x + gap) - gap;
                ui.add_space(((ui.available_width() - width) / 2.0).max(0.0));
                for root in row {
                    let facts = self.tile_facts.entry(root.clone()).or_insert_with(|| facts_of(root)).as_ref();
                    match recent_tile(ui, root, facts) {
                        Some(TileChoice::Open) => open = Some(root.clone()),
                        Some(TileChoice::Forget) => forget = Some(root.clone()),
                        Some(TileChoice::Kind(kind)) => new_kind = Some((root.clone(), kind)),
                        None => {}
                    }
                }
            });
            ui.add_space(gap - ui.spacing().item_spacing.y);
        }
        let choice = match (open, forget, new_kind) {
            (Some(root), _, _) => Some((root, TileChoice::Open)),
            (_, Some(root), _) => Some((root, TileChoice::Forget)),
            (_, _, Some((root, kind))) => Some((root, TileChoice::Kind(kind))),
            _ => None,
        };
        self.act_on(choice);
    }

    /// Do what was chosen on a recent project's tile or card.
    fn act_on(&mut self, choice: Option<(PathBuf, TileChoice)>) {
        match choice {
            Some((root, TileChoice::Kind(kind))) => {
                if let Err(e) = crate::project::set_kind_of(&root, kind) {
                    self.error = Some(format!("Couldn't change the project's icon: {e}"));
                }
            }
            Some((root, TileChoice::Forget)) => self.recent.retain(|p| *p != root),
            Some((root, TileChoice::Open)) => self.open_project(&root),
            None => {}
        }
    }

    pub(super) fn start_renaming_project(&mut self) {
        let Some(project) = &self.project else { return };
        self.renaming_project = Some(Renaming {
            title: project.title().to_owned(),
            folder_too: folder_matches_title(&project.root, project.title()),
            focus: true,
        });
    }

    pub(super) fn rename_project_window(&mut self, ctx: &egui::Context) {
        let Some(state) = self.renaming_project.as_mut() else { return };
        let Some(project) = &self.project else { return };
        let folder = project.root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let kind = project.kind();
        let mut new_kind = None;
        let mut choice = None;
        egui::Modal::new(egui::Id::unique("rename-project")).show(ctx, |ui| {
            ui.set_width(420.0);
            ui.heading("Rename Project");
            ui.add_space(6.0);
            let field = ui.add(egui::TextEdit::singleline(&mut state.title).desired_width(f32::INFINITY));
            if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                choice = Some(true);
            }
            if std::mem::take(&mut state.focus) {
                field.request_focus();
            }
            ui.checkbox(&mut state.folder_too, format!("Also rename the folder \u{201c}{folder}\u{201d}"));
            ui.add_space(10.0);
            ui.label(RichText::new("Icon on the start screen").small().weak());
            new_kind = kind_picker(ui, "rename-kind", kind);
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Rename").clicked() {
                    choice = Some(true);
                }
                if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    choice = Some(false);
                }
            });
        });
        if let Some(kind) = new_kind
            && let Some(project) = self.project.as_mut()
            && let Err(e) = project.set_kind(kind)
        {
            self.error = Some(format!("Couldn't change the project's icon: {e}"));
        }
        match choice {
            Some(true) => {
                let state = self.renaming_project.take().expect("checked above");
                self.rename_project(&state.title, state.folder_too);
            }
            Some(false) => self.renaming_project = None,
            None => {}
        }
    }

    /// Rename the project, and its folder if asked. A renamed folder means
    /// the project is reopened from its new place (after saving).
    pub(super) fn rename_project(&mut self, title: &str, folder_too: bool) {
        let Some(project) = self.project.as_mut() else { return };
        if let Err(e) = project.set_title(title) {
            self.error = Some(format!("Couldn't rename the project: {e}"));
            return;
        }
        let title = project.title().to_owned();
        let old_root = project.root.clone();
        let new_root = old_root.with_file_name(crate::project::folder_name(&title));
        if !folder_too || new_root == old_root {
            return;
        }
        if new_root.exists() {
            self.error = Some(format!(
                "Renamed the project, but not its folder: \u{201c}{}\u{201d} already exists there.",
                new_root.file_name().unwrap_or_default().to_string_lossy()
            ));
            return;
        }
        // Save everything, move the folder, and open it from there.
        let selected = self.selected.clone();
        self.close_project();
        if let Err(e) = fs::rename(&old_root, &new_root) {
            self.error = Some(format!("Renamed the project, but couldn't rename its folder: {e}"));
            self.open_project(&old_root);
            return;
        }
        self.recent.retain(|p| *p != old_root);
        self.open_project(&new_root);
        if let Some(id) = selected.filter(|id| self.project.as_ref().is_some_and(|p| p.node(id).is_some())) {
            self.select(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn how_long_ago() {
        let today = stats::parse_date("2026-11-23").unwrap();
        let day = |back: u64| today - chrono::Days::new(back);
        let said: Vec<String> = [0, 1, 5, 20, 90, 800].map(|b| ago(day(b), today)).to_vec();
        assert_eq!(said, ["today", "yesterday", "5 days ago", "2 weeks ago", "3 months ago", "2 years ago"]);
    }
}
