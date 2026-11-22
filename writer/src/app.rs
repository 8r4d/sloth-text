//! Sloth Writer's window: a start screen when no project is open; otherwise
//! a toolbar, the binder on the left and the selected document's editor.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};
use sloth_core::document::{DiskState, Document};
use sloth_core::editor::{self, EditorSettings, Highlights};
use sloth_core::find::{FindAction, FindOptions, FindState};
use sloth_core::tabs::{self, DiskNotice, NoticeChoice, Tab, TabId, Tabs};

use crate::binder::{self, BinderAction, Renaming};

#[path = "inspector.rs"]
pub(crate) mod inspector;
#[path = "search.rs"]
mod search;
use search::{DocResults, Direction};
use crate::project::{MANIFEST_FILE, Node, PathChange, Place, Project, Role};

/// Where the last open project is remembered between launches.
const LAST_PROJECT_KEY: &str = "last_project";
/// Unsaved edits are written this often, and when switching documents.
const AUTOSAVE_SECONDS: f64 = 2.0;
/// While the window is in front, open files are checked for changes made by
/// other programs this often (and whenever it comes back to the front).
const DISK_CHECK_SECONDS: f64 = 2.0;
/// How long a status-bar message stays up.
const STATUS_SECONDS: f64 = 8.0;

pub struct WriterApp {
    project: Option<Project>,
    /// The binder item shown in the editor.
    selected: Option<String>,
    /// Documents opened this session, so each keeps its cursor and undo
    /// history when you switch away and back. Core's tab list does the
    /// bookkeeping; Writer doesn't show tabs.
    docs: Tabs,
    /// Binder id → its open document.
    open: HashMap<String, TabId>,
    editor: EditorSettings,
    renaming: Option<Renaming>,
    error: Option<String>,
    confirm_empty_trash: bool,
    last_autosave: f64,
    last_disk_check: f64,
    was_focused: bool,
    /// A short message for the status bar, and when it was set.
    status: Option<(String, f64)>,
    window_title: String,
    /// Put the keyboard focus in the editor on the next frame, so a newly
    /// opened document can be typed in straight away.
    focus_editor: bool,
    /// Scroll the editor to this character on the next frame.
    reveal: Option<usize>,

    // Find (see search.rs).
    find: FindState,
    /// The project-wide results panel is open.
    show_results: bool,
    results: Vec<DocResults>,
    results_error: Option<String>,
    /// The search the results were built for, and when.
    results_key: Option<(String, FindOptions)>,
    results_at: f64,

    // Inspector (see inspector.rs).
    show_inspector: bool,
    inspector_buffers: Option<inspector::Buffers>,
    editing_labels: bool,
    word_cache: inspector::WordCache,
    /// Metadata edited since project.ron was last written.
    manifest_dirty: bool,
}

impl WriterApp {
    /// `open`: a project folder named on the command line, opened instead
    /// of the last project.
    pub fn new(cc: &eframe::CreationContext<'_>, open: Option<PathBuf>) -> Self {
        sloth_core::fonts::apply(&cc.egui_ctx, None, sloth_core::fonts::default_prose_font(), 4);
        let mut app = Self {
            project: None,
            selected: None,
            docs: Tabs::new(),
            open: HashMap::new(),
            // Prose, not code: a reading face, roomy lines, a centred column.
            editor: EditorSettings {
                show_line_numbers: false,
                word_wrap: true,
                font_size: 18.0,
                prose: true,
                line_spacing: 1.3,
                max_text_width: Some(680.0),
                auto_indent: false,
                highlight_current_line: false,
                syntax_highlighting: false,
                smart_dashes: true,
                ..EditorSettings::default()
            },
            renaming: None,
            error: None,
            confirm_empty_trash: false,
            last_autosave: 0.0,
            last_disk_check: 0.0,
            was_focused: false,
            status: None,
            window_title: String::new(),
            focus_editor: false,
            reveal: None,
            find: FindState::default(),
            show_results: false,
            results: Vec::new(),
            results_error: None,
            results_key: None,
            results_at: 0.0,
            show_inspector: true,
            inspector_buffers: None,
            editing_labels: false,
            word_cache: inspector::WordCache::default(),
            manifest_dirty: false,
        };
        if let Some(root) = open {
            app.open_project(&root);
            return app;
        }
        let last: Option<PathBuf> = cc.storage.and_then(|s| eframe::get_value(s, LAST_PROJECT_KEY)).flatten();
        // A project that has since been moved or deleted just isn't reopened.
        if let Some(root) = last.filter(|root| root.join(MANIFEST_FILE).is_file()) {
            app.open_project(&root);
        }
        app
    }

    // ----- projects ----------------------------------------------------------

    fn new_project_dialog(&mut self) {
        let Some(root) = rfd::FileDialog::new()
            .set_title("New Project")
            .set_file_name("My Novel")
            .save_file()
        else {
            return;
        };
        let title = root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled".to_owned());
        match Project::create(&root, &title) {
            Ok(project) => {
                self.set_project(project);
                // Straight into naming the first document.
                if let Some(id) = self.selected.clone() {
                    self.start_rename(&id);
                }
            }
            Err(e) => self.error = Some(format!("Couldn't create the project: {e}")),
        }
    }

    fn open_project_dialog(&mut self) {
        if let Some(root) = rfd::FileDialog::new().set_title("Open Project").pick_folder() {
            self.open_project(&root);
        }
    }

    fn open_project(&mut self, root: &Path) {
        match Project::open(root) {
            Ok(project) => self.set_project(project),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                self.error = Some(format!("{} isn't a Sloth Writer project (it has no {MANIFEST_FILE}).", root.display()));
            }
            Err(e) => self.error = Some(format!("Couldn't open {}: {e}", root.display())),
        }
    }

    fn set_project(&mut self, project: Project) {
        self.close_project();
        let first = first_document(&project.manifest.binder[0]);
        self.project = Some(project);
        if let Some(id) = first {
            self.select(id);
        }
    }

    fn close_project(&mut self) {
        self.save_all();
        self.project = None;
        self.selected = None;
        self.docs = Tabs::new();
        self.open.clear();
        self.renaming = None;
        self.confirm_empty_trash = false;
        self.find.close();
        self.show_results = false;
        self.results.clear();
        self.results_key = None;
        self.inspector_buffers = None;
        self.editing_labels = false;
    }

    // ----- documents ---------------------------------------------------------

    fn select(&mut self, id: String) {
        // Switching documents is a natural moment to save.
        self.save_all();
        let Some(project) = &self.project else { return };
        if let Some(path) = project.file_path(&id) {
            match self.open.get(&id) {
                Some(&tab) if self.docs.get(tab).is_some() => self.docs.activate(tab),
                _ => match Document::open(&path) {
                    Ok(doc) => {
                        let tab = self.docs.add(doc);
                        self.open.insert(id.clone(), tab);
                    }
                    Err(e) => self.error = Some(format!("Couldn't open {}: {e}", path.display())),
                },
            }
        }
        self.selected = Some(id);
        self.focus_editor = true;
    }

    /// The selected item's open document, if it's a document.
    fn current(&mut self) -> Option<&mut Tab> {
        let tab = *self.open.get(self.selected.as_deref()?)?;
        self.docs.get_mut(tab)
    }

    fn any_unsaved(&self) -> bool {
        self.docs.iter().any(|t| t.doc.path.is_some() && t.doc.is_dirty())
    }

    /// Save every open document with unsaved edits. One whose file another
    /// program changed since it was read is left alone: writing would throw
    /// away those changes, so it waits for the person to choose a version
    /// in the notice above the editor.
    fn save_all(&mut self) {
        let mut failed = Vec::new();
        // Metadata edited in the inspector.
        if self.manifest_dirty
            && let Some(project) = &self.project
        {
            match project.save() {
                Ok(()) => self.manifest_dirty = false,
                Err(e) => failed.push(format!("the project file ({e})")),
            }
        }
        for tab in self.docs.iter_mut() {
            if tab.doc.path.is_none() || !tab.doc.is_dirty() || tab.disk_notice == Some(DiskNotice::Changed) {
                continue;
            }
            // Checked here too, not just every couple of seconds, so an
            // autosave can't land between an outside change and the check.
            if tab.doc.disk_state() == DiskState::Changed {
                tab.disk_notice = Some(DiskNotice::Changed);
                continue;
            }
            if let Err(e) = tab.doc.save() {
                failed.push(format!("{} ({e})", tab.doc.display_name()));
            }
        }
        if !failed.is_empty() {
            self.error = Some(format!("Couldn't save {}", failed.join(", ")));
        }
    }

    /// Pick up changes other programs made to open documents. Unedited ones
    /// reload quietly; edited ones get a notice (see `save_all`). A file that
    /// was deleted or moved is written back from the open copy: the binder
    /// still lists it, and the words are safer on disk than only in memory.
    fn check_outside_changes(&mut self, now: f64) {
        let reloaded = self.docs.check_disk();
        let mut restored = Vec::new();
        for tab in self.docs.iter_mut() {
            if tab.disk_notice == Some(DiskNotice::Missing) && tab.doc.save().is_ok() {
                tab.disk_notice = None;
                restored.push(tab.doc.display_name());
            }
        }
        if !reloaded.is_empty() {
            self.set_status(format!("Reloaded {} (changed by another program)", reloaded.join(", ")), now);
        }
        if !restored.is_empty() {
            self.set_status(format!("Saved {} again (its file had been deleted or moved)", restored.join(", ")), now);
        }
    }

    fn set_status(&mut self, message: String, now: f64) {
        self.status = Some((message, now));
    }

    /// The binder id of an open document whose file changed on disk while it
    /// has unsaved edits, waiting for the person to pick a version.
    fn unresolved_change(&self) -> Option<String> {
        let tab = self.docs.iter().find(|t| t.disk_notice == Some(DiskNotice::Changed))?;
        self.open.iter().find(|(_, t)| **t == tab.id).map(|(id, _)| id.clone())
    }

    /// Asked to quit or close the window: save, and say whether it may go
    /// ahead. Not while a version choice is waiting, since either answer
    /// could lose words; that document is shown instead.
    fn before_close(&mut self) -> bool {
        self.save_all();
        let Some(id) = self.unresolved_change() else { return true };
        self.select(id);
        let name = self.current().map(|t| t.doc.display_name()).unwrap_or_default();
        self.error = Some(format!(
            "\u{201c}{name}\u{201d} was changed by another program while you were editing it. \
             Choose which version to keep before quitting."
        ));
        false
    }

    /// The person's answer in the notice above the editor.
    fn resolve_notice(&mut self, choice: NoticeChoice) {
        let Some(tab) = self.current() else { return };
        match choice {
            NoticeChoice::Reload => {
                let encoding = Some(tab.doc.format.encoding);
                match tab.doc.reload_with_encoding(encoding) {
                    Ok(()) => tab.disk_notice = None,
                    Err(e) => self.error = Some(format!("Couldn't reload: {e}")),
                }
            }
            NoticeChoice::Keep => {
                tab.doc.acknowledge_disk_state();
                tab.disk_notice = None;
                // Write it now rather than at the next autosave.
                self.save_all();
            }
        }
    }

    /// Point open documents at their files' new paths after a rename or move.
    fn follow(&mut self, changes: &[PathChange]) {
        for change in changes {
            if let Some(tab) = self.docs.find_by_path(&change.from).and_then(|t| self.docs.get_mut(t)) {
                tab.doc.path = Some(change.to.clone());
                tab.doc.acknowledge_disk_state();
            }
        }
    }

    // ----- binder actions ----------------------------------------------------

    /// Where a new item goes when made from `near`: inside a folder, after a
    /// document, and into the Manuscript when there's no sensible spot (or
    /// `near` is in the Trash).
    fn place_near(&self, near: Option<&str>) -> Place {
        let manuscript = Place::Into(Role::Manuscript.folder().to_owned());
        let Some(project) = &self.project else { return manuscript };
        let Some(node) = near.and_then(|id| project.node(id)) else { return manuscript };
        if project.role_of(&node.id) == Some(Role::Trash) {
            manuscript
        } else if node.role.is_some() || !node.is_document() {
            Place::Into(node.id.clone())
        } else {
            Place::After(node.id.clone())
        }
    }

    fn new_item(&mut self, near: Option<String>, document: bool) {
        let place = self.place_near(near.as_deref());
        let Some(project) = self.project.as_mut() else { return };
        let made = if document {
            project.add_document(&place, "Untitled")
        } else {
            project.add_folder(&place, "New Folder")
        };
        match made {
            Ok(id) => {
                self.select(id.clone());
                self.start_rename(&id);
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    fn start_rename(&mut self, id: &str) {
        let Some(node) = self.project.as_ref().and_then(|p| p.node(id)) else { return };
        if node.role.is_none() {
            self.renaming = Some(Renaming { id: id.to_owned(), text: node.title.clone(), focus: true });
            // The title field gets the focus, not the editor.
            self.focus_editor = false;
        }
    }

    fn apply(&mut self, action: BinderAction) {
        match action {
            BinderAction::Select(id) => self.select(id),
            BinderAction::StartRename(id) => self.start_rename(&id),
            BinderAction::CancelRename => self.renaming = None,
            BinderAction::FinishRename { id, title } => {
                self.renaming = None;
                let Some(project) = self.project.as_mut() else { return };
                if project.node(&id).is_some_and(|n| n.title == title.trim()) {
                    return;
                }
                match project.rename(&id, &title) {
                    Ok(change) => self.follow(change.as_slice()),
                    Err(e) => self.error = Some(e.to_string()),
                }
            }
            BinderAction::NewDocument(near) => self.new_item(Some(near), true),
            BinderAction::NewFolder(near) => self.new_item(Some(near), false),
            BinderAction::Trash(id) => self.trash(&id),
            BinderAction::EmptyTrash => self.confirm_empty_trash = true,
            BinderAction::Move { id, place } => self.move_item(&id, &place),
        }
    }

    fn move_item(&mut self, id: &str, place: &Place) {
        self.renaming = None;
        // Save first: the file may move to another folder.
        self.save_all();
        let Some(project) = self.project.as_mut() else { return };
        match project.move_node(id, place) {
            Ok(changes) => self.follow(&changes),
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    fn trash(&mut self, id: &str) {
        // Save first, so the file that moves holds the latest words.
        self.save_all();
        let Some(project) = self.project.as_mut() else { return };
        match project.trash(id) {
            Ok(changes) => self.follow(&changes),
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    fn empty_trash(&mut self) {
        let Some(project) = self.project.as_mut() else { return };
        match project.empty_trash() {
            Ok(removed) => {
                for path in removed {
                    if let Some(tab) = self.docs.find_by_path(&path) {
                        self.docs.close(tab);
                    }
                }
                let project = self.project.as_ref().expect("checked above");
                self.open.retain(|id, _| project.node(id).is_some());
                if self.selected.as_deref().is_some_and(|id| project.node(id).is_none()) {
                    self.selected = None;
                }
            }
            Err(e) => self.error = Some(format!("Couldn't empty the Trash: {e}")),
        }
    }

    // ----- drawing -----------------------------------------------------------

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let pressed = |ctx: &egui::Context, modifiers, key| {
            ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(modifiers, key)))
        };
        // The longer shortcut first, so ⌥⌘N isn't taken as ⌘N.
        if self.project.is_some() && pressed(ctx, Modifiers::COMMAND | Modifiers::ALT, Key::N) {
            self.new_item(self.selected.clone(), false);
        }
        if self.project.is_some() && pressed(ctx, Modifiers::COMMAND, Key::N) {
            self.new_item(self.selected.clone(), true);
        }
        if pressed(ctx, Modifiers::COMMAND, Key::S) {
            self.save_all();
        }
        if pressed(ctx, Modifiers::COMMAND, Key::O) {
            self.open_project_dialog();
        }
        if self.project.is_some() && pressed(ctx, Modifiers::COMMAND | Modifiers::ALT, Key::I) {
            self.show_inspector = !self.show_inspector;
        }
        // Find. The longer shortcuts first, so ⇧⌘F isn't taken as ⌘F.
        if self.project.is_some() && pressed(ctx, Modifiers::COMMAND | Modifiers::SHIFT, Key::F) {
            self.open_find(true);
        }
        if self.project.is_some() && pressed(ctx, Modifiers::COMMAND, Key::F) {
            self.open_find(false);
        }
        if self.project.is_some() && pressed(ctx, Modifiers::COMMAND | Modifiers::SHIFT, Key::G) {
            self.find_step(ctx, Direction::Previous);
        }
        if self.project.is_some() && pressed(ctx, Modifiers::COMMAND, Key::G) {
            self.find_step(ctx, Direction::Next);
        }
        // Escape closes the find bar, unless it's ending a rename or a dialog.
        if self.find.visible
            && self.renaming.is_none()
            && !self.confirm_empty_trash
            && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape))
        {
            self.find_action(ctx, FindAction::Close);
        }
        // Quit by closing the window, which saves first (see `before_close`).
        if pressed(ctx, Modifiers::COMMAND, Key::Q) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn update_title(&mut self, ctx: &egui::Context) {
        let title = match &self.project {
            Some(p) => format!("{} — Sloth Writer", p.title()),
            None => "Sloth Writer".to_owned(),
        };
        if title != self.window_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.window_title = title;
        }
    }

    fn error_bar(&mut self, ui: &mut egui::Ui) {
        let Some(message) = self.error.clone() else { return };
        egui::Panel::top(egui::Id::unique("error")).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.colored_label(ui.visuals().error_fg_color, message);
                if ui.button("Dismiss").clicked() {
                    self.error = None;
                }
            });
        });
    }

    fn start_screen(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(ui.available_height() * 0.3);
                ui.heading("Sloth Writer");
                ui.add_space(16.0);
                if ui.button("New Project…").clicked() {
                    self.new_project_dialog();
                }
                if ui.button("Open Project…  (⌘O)").clicked() {
                    self.open_project_dialog();
                }
            });
        });
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top(egui::Id::unique("toolbar")).show(ui, |ui| {
            ui.horizontal(|ui| {
                if let Some(project) = &self.project {
                    ui.strong(project.title());
                }
                ui.separator();
                if ui.button("New Document").on_hover_text("⌘N").clicked() {
                    self.new_item(self.selected.clone(), true);
                }
                if ui.button("New Folder").on_hover_text("⌥⌘N").clicked() {
                    self.new_item(self.selected.clone(), false);
                }
                let can_trash = self
                    .selected
                    .as_deref()
                    .and_then(|id| self.project.as_ref()?.node(id))
                    .is_some_and(|n| n.role.is_none());
                if ui.add_enabled(can_trash, egui::Button::new("Move to Trash")).clicked() {
                    let id = self.selected.clone().expect("enabled only with a selection");
                    self.trash(&id);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.toggle_value(&mut self.show_inspector, "Inspector").on_hover_text("\u{2325}\u{2318}I");
                    if ui.button("Close Project").clicked() {
                        self.close_project();
                    }
                });
            });
        });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        let now = ui.input(|i| i.time);
        if self.status.as_ref().is_some_and(|(_, at)| now - at > STATUS_SECONDS) {
            self.status = None;
        }
        let words = self.current().map(|t| t.doc.text.split_whitespace().count());
        let status = self.status.as_ref().map(|(message, _)| message.clone());
        egui::Panel::bottom(egui::Id::unique("status")).show(ui, |ui| {
            ui.horizontal(|ui| {
                match words {
                    Some(1) => ui.label("1 word"),
                    Some(n) => ui.label(format!("{n} words")),
                    None => ui.label(""),
                };
                if let Some(message) = status {
                    ui.separator();
                    ui.label(message);
                }
            });
        });
        if self.status.is_some() {
            ui.ctx().request_repaint_after(Duration::from_secs_f64(STATUS_SECONDS));
        }
    }

    fn notice_bar(&mut self, ui: &mut egui::Ui) {
        let Some(tab) = self.current() else { return };
        if let Some(choice) = tabs::disk_notice_bar(ui, egui::Id::unique("disk-notice"), tab) {
            self.resolve_notice(choice);
        }
    }

    fn editor_area(&mut self, ui: &mut egui::Ui) {
        let settings = self.editor.clone();
        let placeholder = match self.selected.as_deref().and_then(|id| self.project.as_ref()?.node(id)) {
            None => "Select a document in the binder.",
            Some(node) if !node.is_document() => "This is a folder. Select a document inside it, or add one with ⌘N.",
            Some(_) => "",
        };
        // Not in a frame with a mouse press or release (such as the click in
        // the binder that opened the document): egui takes focus away from a
        // text field when a click lands outside it, so wait a frame.
        let pressed = ui.input(|i| i.pointer.any_pressed() || i.pointer.any_released());
        let focus = self.focus_editor && !pressed;
        if focus {
            self.focus_editor = false;
        } else if self.focus_editor {
            ui.ctx().request_repaint();
        }
        // Find matches in this document, highlighted while the find bar is open.
        let matches = if self.find.visible { self.current_matches().unwrap_or_default() } else { Vec::new() };
        let highlights = Highlights { all: &matches, current: self.find.current.clone() };
        let reveal = self.reveal.take();
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(ui.visuals().extreme_bg_color).inner_margin(egui::Margin::symmetric(24, 16)))
            .show(ui, |ui| match self.current() {
                Some(tab) => {
                    if focus {
                        ui.memory_mut(|m| m.request_focus(editor::editor_id(tab.id)));
                    }
                    editor::show(ui, tab.id, &mut tab.doc.text, &settings, highlights, reveal, None);
                }
                None => {
                    ui.weak(placeholder);
                }
            });
    }

    fn empty_trash_dialog(&mut self, ctx: &egui::Context) {
        if !self.confirm_empty_trash {
            return;
        }
        egui::Modal::new(egui::Id::unique("empty-trash")).show(ctx, |ui| {
            ui.heading("Empty the Trash?");
            ui.label("Everything in the Trash will be deleted for good, including its files.");
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Empty Trash").clicked() {
                    self.confirm_empty_trash = false;
                    self.empty_trash();
                }
                if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                    self.confirm_empty_trash = false;
                }
            });
        });
    }
}

impl eframe::App for WriterApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if ctx.input(|i| i.viewport().close_requested()) && !self.before_close() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        self.shortcuts(&ctx);

        // Look for outside changes when the window comes to the front, and
        // every couple of seconds while it's there. Before autosave below,
        // so a change is noticed before anything could write over it.
        let now = ctx.input(|i| i.time);
        let focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
        if focused && (!self.was_focused || now - self.last_disk_check >= DISK_CHECK_SECONDS) {
            self.check_outside_changes(now);
            self.last_disk_check = now;
        }
        self.was_focused = focused;
        if self.project.is_some() {
            ctx.request_repaint_after(Duration::from_secs_f64(DISK_CHECK_SECONDS));
        }
        self.update_title(&ctx);
        self.error_bar(ui);

        if self.project.is_none() {
            self.start_screen(ui);
            return;
        }

        self.toolbar(ui);
        let mut actions = Vec::new();
        egui::Panel::left(egui::Id::unique("binder"))
            .resizable(true)
            .default_size(240.0)
            .size_range(160.0..=480.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    if let Some(project) = &self.project {
                        let selected = self.selected.as_deref();
                        let labels = &project.manifest.labels;
                        actions = binder::show(ui, &project.manifest.binder, labels, selected, &mut self.renaming);
                    }
                });
            });
        for action in actions {
            self.apply(action);
        }
        self.inspector(ui);
        self.status_bar(ui);
        self.results_panel(ui);
        self.find_bar(ui);
        self.notice_bar(ui);
        self.editor_area(ui);
        self.empty_trash_dialog(&ctx);
        self.labels_window(&ctx);

        if now - self.last_autosave >= AUTOSAVE_SECONDS {
            self.save_all();
            self.last_autosave = now;
        }
        if self.any_unsaved() {
            ctx.request_repaint_after(Duration::from_secs_f64(AUTOSAVE_SECONDS));
        }
    }

    /// Called now and then and when quitting: save the words, and remember
    /// the project to reopen next time.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.save_all();
        let root = self.project.as_ref().map(|p| p.root.clone());
        eframe::set_value(storage, LAST_PROJECT_KEY, &root);
    }
}

/// The first document in reading order under `node`.
fn first_document(node: &Node) -> Option<String> {
    node.children
        .iter()
        .find_map(|child| if child.is_document() { Some(child.id.clone()) } else { first_document(child) })
}

#[cfg(test)]
#[path = "app_ui_tests.rs"]
mod ui_tests;
