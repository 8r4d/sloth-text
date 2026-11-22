//! Around projects rather than inside one: the recent projects on the
//! start screen, and renaming the open project (its title, and its folder
//! if that was named after it).

use std::fs;
use std::path::{Path, PathBuf};

use eframe::egui::{self, RichText};

use super::WriterApp;
use crate::project::{MANIFEST_FILE, Manifest};

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

/// A project's title, read from its manifest without opening the project.
fn title_of(root: &Path) -> Option<String> {
    let text = fs::read_to_string(root.join(MANIFEST_FILE)).ok()?;
    ron::from_str::<Manifest>(&text).ok().map(|m| m.title)
}

/// Where a project lives, briefly: the folder it's in, from the home
/// folder (`~/Documents/Writing`), with the start cut off if it's long.
fn short_place(root: &Path) -> String {
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
    /// The recent projects, on the start screen.
    pub(super) fn recent_list(&mut self, ui: &mut egui::Ui) {
        if self.recent.is_empty() {
            return;
        }
        ui.add_space(28.0);
        ui.label(RichText::new("Recent projects").small().weak());
        ui.add_space(4.0);
        let mut open = None;
        let mut forget = None;
        for root in &self.recent {
            let title = title_of(root);
            let available = title.is_some();
            let name = title.unwrap_or_else(|| root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
            let place = if available { short_place(root) } else { "(not found)".to_owned() };
            let text = (RichText::new(format!("{name}   ")).strong(), RichText::new(place).small().weak());
            let response = ui
                .add_enabled(available, egui::Button::new(text).frame(false))
                .on_hover_text(root.display().to_string());
            if response.clicked() {
                open = Some(root.clone());
            }
            response.context_menu(|ui| {
                if ui.button("Remove from List").clicked() {
                    forget = Some(root.clone());
                    ui.close();
                }
            });
            // A disabled button gets no menu: offer removal beside it.
            if !available && ui.small_button("Remove").clicked() {
                forget = Some(root.clone());
            }
        }
        if let Some(root) = forget {
            self.recent.retain(|p| *p != root);
        }
        if let Some(root) = open {
            self.open_project(&root);
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
        let mut choice = None;
        egui::Modal::new(egui::Id::unique("rename-project")).show(ctx, |ui| {
            ui.set_width(360.0);
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
