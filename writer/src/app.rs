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
use sloth_core::keys;
use sloth_core::tabs::{self, DiskNotice, NoticeChoice, Tab, TabId, Tabs};

use crate::binder::{self, BinderAction, Renaming};

#[path = "about_ui.rs"]
mod about_ui;
#[path = "profiles_ui.rs"]
mod profiles_ui;
#[path = "bins_ui.rs"]
mod bins_ui;
#[path = "breakup_ui.rs"]
mod breakup_ui;
#[path = "canvas_ui.rs"]
mod canvas_ui;
#[path = "card_file_ui.rs"]
mod card_file_ui;
#[path = "comments_ui.rs"]
mod comments_ui;
#[path = "compile_window.rs"]
mod compile_window;
#[path = "elevator_ui.rs"]
mod elevator_ui;
#[path = "get_project_ui.rs"]
mod get_project_ui;
#[path = "goal_ui.rs"]
mod goal_ui;
#[path = "inspector.rs"]
pub(crate) mod inspector;
#[path = "project_settings_ui.rs"]
mod project_settings_ui;
#[path = "projects_ui.rs"]
mod projects_ui;
#[path = "references_ui.rs"]
mod references_ui;
#[path = "repetition_ui.rs"]
mod repetition_ui;
#[path = "slothenings_ui.rs"]
mod slothenings_ui;
#[path = "search.rs"]
mod search;
#[path = "settings_ui.rs"]
pub(crate) mod settings_ui;
#[path = "sloth_ui.rs"]
mod sloth_ui;
#[path = "snapshots_ui.rs"]
mod snapshots_ui;
#[path = "stats_ui.rs"]
mod stats_ui;
#[path = "sprint_ui.rs"]
mod sprint_ui;
#[path = "split_ui.rs"]
mod split_ui;
#[path = "strays_ui.rs"]
mod strays_ui;
#[path = "sync_ui.rs"]
mod sync_ui;
#[path = "views.rs"]
mod views;
use search::{DocResults, Direction};
use crate::project::{MANIFEST_FILE, Meta, Node, PathChange, Place, Project, Role};

/// Where the last open project is remembered between launches.
const LAST_PROJECT_KEY: &str = "last_project";
/// Unsaved edits are written this often, and when switching documents.
const AUTOSAVE_SECONDS: f64 = 2.0;
/// How often to see whether the day's automatic backup is due.
const BACKUP_CHECK_SECONDS: f64 = 600.0;
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
    /// Replace All in the whole project, waiting to be confirmed: how many
    /// matches, in how many documents.
    confirm_project_replace: Option<(usize, usize)>,

    // Inspector (see inspector.rs).
    show_inspector: bool,
    show_binder: bool,
    /// Focus mode: full screen, nothing but the text (see focus_view).
    focus_mode: bool,
    /// Whether the window was full screen before Focus mode, to put back.
    was_fullscreen: bool,
    /// When the daily backup was last looked for (seconds, egui's clock).
    last_backup_check: f64,
    // Another copy of Writer and this one (see lock.rs).
    /// A project another copy has open, waiting for Open Anyway or Cancel.
    pending_open: Option<(PathBuf, crate::lock::Lock)>,
    /// Another copy opened this project meanwhile: nothing is saved here.
    lock_lost: Option<crate::lock::Lock>,
    last_lock_beat: f64,
    inspector_buffers: Option<inspector::Buffers>,
    editing_labels: bool,
    editing_statuses: bool,
    word_cache: inspector::WordCache,
    /// Metadata edited since project.ron was last written.
    manifest_dirty: bool,
    /// Corkboard or outline, per folder (see views.rs).
    folder_views: views::FolderViews,
    /// The keyword each folder's corkboard and outline are filtered by,
    /// this session (see views.rs).
    folder_keywords: HashMap<String, String>,
    /// The canvas is shown in place of the editor (see canvas_ui.rs).
    canvas_open: bool,
    /// The canvas, once it's been shown for this project.
    canvas: Option<canvas_ui::CanvasState>,
    /// The Compile window is open (see compile_window.rs).
    compile_open: bool,
    /// A preset's name as it's being typed, in the Compile window.
    compile_preset_name: Option<String>,

    // Snapshots (see snapshots_ui.rs).
    /// The name typed for the next snapshot.
    snapshot_name: String,
    comparing: Option<snapshots_ui::Comparing>,
    confirm_delete_snapshot: Option<crate::snapshots::Snapshot>,

    /// Projects opened lately, newest first (see projects_ui.rs).
    recent: Vec<PathBuf>,
    /// What each recent project's tile shows, once worked out.
    tile_facts: HashMap<PathBuf, Option<projects_ui::TileFacts>>,
    /// The start screen's facts (made when it's first shown), and how many
    /// times the fact's been clicked for another.
    start_facts: Option<Vec<String>>,
    fact_skip: usize,
    /// The start screen's card file: which card is in front.
    card_file: card_file_ui::CardFileState,
    renaming_project: Option<projects_ui::Renaming>,

    // Characters (see character_ui.rs).
    /// Profile forms' name fields as typed, by profile (two can be on
    /// screen at once, with the split view).
    profile_name: HashMap<String, String>,
    /// Open profiles as typed, by profile (see profiles_ui.rs).
    profile_sheet: HashMap<String, profiles_ui::ProfileSheet>,
    /// The shown profile's backlinks, as last gathered (references_ui.rs).
    backlinks: Option<references_ui::BacklinkCache>,
    /// Suggestions for a `[[reference]]` being typed, and the one put away
    /// with Esc (by document and where its `[[` is).
    suggesting: Option<references_ui::Suggesting>,
    suggest_dismissed: Option<(String, usize)>,
    /// New … for a ⌘-clicked reference to no profile yet.
    create_menu: Option<references_ui::CreateMenu>,
    editing_sections: Option<profiles_ui::EditingSections>,
    /// The New Profile Folder window for a folder of your own: its name, and
    /// what one item in it is called.
    new_profile_folder: Option<(String, String)>,

    /// The Break Up and Combine windows (see breakup_ui.rs).
    breaking_up: Option<breakup_ui::BreakingUp>,
    combining: Option<breakup_ui::Combining>,

    // The split view (see split_ui.rs).
    split_open: bool,
    /// What it shows.
    split: Option<String>,
    // Writing statistics (see stats.rs and stats_ui.rs).
    stats: crate::stats::Stats,
    stats_dirty: bool,
    /// Each document's words when last looked at, so a change can be
    /// added to the day's tally.
    word_counts: HashMap<String, u32>,
    stats_open: bool,
    /// The day's goal just reached, being celebrated (see goal_ui.rs).
    celebration: Option<goal_ui::Celebration>,
    /// Files the binder doesn't list, and copies of the project file (see
    /// strays.rs); the bar about them put away until the next opening.
    strays: Vec<crate::strays::Stray>,
    project_copies: Vec<String>,
    strays_dismissed: bool,
    /// Get Project from the Helper…, while its window's up.
    get_project: Option<get_project_ui::GetProject>,
    /// Opening or closing a project: the panel sliding away.
    elevator: Option<elevator_ui::Ride>,
    /// A bin in Focus mode whose lid is lifting, and since when.
    bin_lid: Option<(String, f64)>,
    /// A writing sprint under way (see sprint_ui.rs).
    sprint: Option<sprint_ui::Sprint>,
    /// Repeated words, as last worked out (see repetition_ui.rs).
    repetition: repetition_ui::RepetitionCache,
    /// A comment just added, whose card should take the keyboard (where its
    /// words start, in bytes; see comments_ui.rs).
    comment_focus: Option<usize>,
    /// Going on into a neighbouring document in Slothenings.
    crossing: Option<slothenings_ui::Crossing>,
    /// The sloth's poses, once shown (see sloth_ui.rs).
    sloth_art: sloth_ui::SlothArt,
    // Softer motion (see `motion_style` and `view_fade`).
    /// Animating, as last set in egui's style (`None` before the first frame).
    animating: Option<bool>,
    /// What the main area shows (document, folder view, Canvas), and since
    /// when, for a short fade-in when it changes.
    view_key: String,
    view_since: f64,
    view_opacity: f32,
    /// The Statistics window's goal fields as typed: daily, total, deadline.
    goal_fields: Option<[String; 3]>,

    // Settings (see settings_ui.rs).
    settings: settings_ui::WriterSettings,
    settings_open: bool,
    /// The Project Settings window is open.
    project_settings_open: bool,
    /// The About and Help window, open at this page (see about_ui.rs).
    help: Option<about_ui::HelpPage>,
    about_icon: Option<egui::TextureHandle>,
    /// The text filtering the Settings window's font list.
    font_filter: String,

    // Syncing with the web helper (see sync_ui.rs).
    /// The helper token, from the Keychain.
    helper_token: String,
    sync_running: Option<sync_ui::Running>,
    last_sync: Option<f64>,
    /// Sync as soon as possible (the Sync button, a new token).
    sync_now: bool,
    /// How the last sync went, for the Sync button's tooltip.
    sync_message: String,
    /// Syncs started so far, and which of them (by number) last finished
    /// well and last failed: for Break Up and Combine to wait for a sync
    /// begun after they opened (see breakup_ui.rs).
    syncs_started: u64,
    sync_ok: u64,
    sync_failed: u64,
}

impl WriterApp {
    /// `open`: a project folder named on the command line, opened instead
    /// of the last project.
    pub fn new(cc: &eframe::CreationContext<'_>, open: Option<PathBuf>) -> Self {
        // Fonts for the Settings window's list (scanned in the background).
        sloth_core::fonts::start_scan(&cc.egui_ctx);
        sloth_core::fonts::set_icon_font(include_bytes!("../icons/writer-icons.ttf"));
        let mut settings: settings_ui::WriterSettings =
            cc.storage.and_then(|s| eframe::get_value(s, settings_ui::SETTINGS_KEY)).unwrap_or_default();
        if settings.device_id.is_empty() {
            settings.device_id = crate::project::random_id(8);
        }
        let mut app = Self {
            project: None,
            selected: None,
            docs: Tabs::new(),
            open: HashMap::new(),
            // Prose, not code: set from the settings (see settings_ui.rs).
            editor: settings.editor(),
            settings,
            settings_open: false,
            project_settings_open: false,
            help: None,
            about_icon: None,
            font_filter: String::new(),
            // Tests never touch the real Keychain.
            helper_token: if cfg!(test) { String::new() } else { crate::keychain::load_token() },
            sync_running: None,
            last_sync: None,
            sync_now: false,
            sync_message: String::new(),
            syncs_started: 0,
            sync_ok: 0,
            sync_failed: 0,
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
            confirm_project_replace: None,
            show_inspector: true,
            show_binder: true,
            focus_mode: false,
            was_fullscreen: false,
            last_backup_check: 0.0,
            pending_open: None,
            lock_lost: None,
            last_lock_beat: 0.0,
            inspector_buffers: None,
            editing_labels: false,
            editing_statuses: false,
            word_cache: inspector::WordCache::default(),
            manifest_dirty: false,
            folder_views: views::FolderViews::default(),
            folder_keywords: HashMap::new(),
            canvas_open: false,
            canvas: None,
            compile_open: false,
            compile_preset_name: None,
            snapshot_name: String::new(),
            comparing: None,
            confirm_delete_snapshot: None,
            recent: cc.storage.and_then(|s| eframe::get_value(s, projects_ui::RECENT_KEY)).unwrap_or_default(),
            renaming_project: None,
            tile_facts: HashMap::new(),
            card_file: Default::default(),
            start_facts: None,
            fact_skip: 0,
            profile_name: HashMap::new(),
            profile_sheet: HashMap::new(),
            backlinks: None,
            suggesting: None,
            suggest_dismissed: None,
            create_menu: None,
            editing_sections: None,
            new_profile_folder: None,
            breaking_up: None,
            combining: None,
            split_open: false,
            split: None,
            stats: Default::default(),
            stats_dirty: false,
            word_counts: HashMap::new(),
            stats_open: false,
            celebration: None,
            bin_lid: None,
            elevator: None,
            get_project: None,
            strays: Vec::new(),
            project_copies: Vec::new(),
            strays_dismissed: false,
            sprint: None,
            repetition: Default::default(),
            comment_focus: None,
            crossing: None,
            sloth_art: Default::default(),
            animating: None,
            view_key: String::new(),
            view_since: 0.0,
            view_opacity: 1.0,
            goal_fields: None,
        };
        app.apply_settings(&cc.egui_ctx);
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

    /// Import from Scrivener…: a `.scriv` (only read), saved as a new
    /// Writer project, which then opens. See scrivener.rs.
    fn import_scrivener_dialog(&mut self, now: f64) {
        let Some(picked) = rfd::FileDialog::new()
            .set_title("Import a Scrivener Project")
            .add_filter("Scrivener project", &["scriv", "scrivx"])
            .pick_file()
        else {
            return;
        };
        // The .scriv itself, if its .scrivx was picked.
        let scriv = if picked.extension().is_some_and(|x| x == "scrivx") { picked.parent().map(Path::to_path_buf) } else { Some(picked) };
        let Some(scriv) = scriv else { return };
        let name = scriv.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Imported".into());
        let Some(dest) = rfd::FileDialog::new().set_title("Save the Writer Project As").set_file_name(&name).save_file() else {
            return;
        };
        self.import_scrivener(&scriv, &dest, now);
    }

    fn import_scrivener(&mut self, scriv: &Path, dest: &Path, now: f64) {
        let existed = dest.exists();
        match crate::scrivener::import(scriv, dest) {
            Ok((project, report)) => {
                self.set_project(project);
                self.set_status(
                    format!(
                        "Imported {} documents from Scrivener. What came across (and what didn't) is in Notes \u{2192} Imported from Scrivener.",
                        report.documents
                    ),
                    now,
                );
            }
            Err(e) => {
                // Don't leave a half-made project behind.
                if !existed {
                    let _ = std::fs::remove_dir_all(dest);
                }
                self.error = Some(format!("Couldn't import {}: {e}", scriv.display()));
            }
        }
    }

    fn open_project_dialog(&mut self) {
        if let Some(root) = rfd::FileDialog::new().set_title("Open Project").pick_folder() {
            self.open_project(&root);
        }
    }

    fn open_project(&mut self, root: &Path) {
        // Open in another copy of Writer: ask first.
        if let crate::lock::Holder::Other(lock) = crate::lock::holder(root) {
            self.pending_open = Some((root.to_path_buf(), lock));
            return;
        }
        self.open_project_anyway(root);
    }

    fn open_project_anyway(&mut self, root: &Path) {
        match Project::open(root) {
            Ok(project) => {
                let title = project.title().to_owned();
                self.set_project(project);
                self.ride_to(&title);
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                self.error = Some(format!("{} isn't a Sloth Writer project (it has no {MANIFEST_FILE}).", root.display()));
            }
            Err(e) => self.error = Some(format!("Couldn't open {}: {e}", root.display())),
        }
    }

    fn set_project(&mut self, project: Project) {
        self.close_project();
        // Sync as soon as it's open.
        self.last_sync = None;
        projects_ui::remember(&mut self.recent, &project.root);
        let first = first_document(&project.manifest.binder[0]);
        self.stats = crate::stats::Stats::load(&project.root);
        if let Err(e) = crate::lock::take(&project.root) {
            self.error = Some(format!("Couldn't mark the project as open here: {e}"));
        }
        self.lock_lost = None;
        self.project = Some(project);
        self.count_all_words();
        if let Some(id) = first {
            self.select(id);
        }
    }

    fn close_project(&mut self) {
        self.save_all();
        self.keep_unsaved_if_taken();
        self.auto_backup(crate::backup::When::Closing);
        if let Some(project) = &self.project {
            crate::lock::release(&project.root);
        }
        self.lock_lost = None;
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
        self.editing_statuses = false;
        self.project_settings_open = false;
        self.compile_open = false;
        self.comparing = None;
        self.confirm_delete_snapshot = None;
        self.profile_name.clear();
        self.profile_sheet.clear();
        self.editing_sections = None;
        self.new_profile_folder = None;
        self.breaking_up = None;
        self.combining = None;
        self.split = None;
        self.canvas_open = false;
        self.canvas = None;
        self.stats_open = false;
        self.goal_fields = None;
        self.sprint = None;
        // The project just closed is newly written in.
        self.tile_facts.clear();
        self.start_facts = None;
        self.strays.clear();
        self.project_copies.clear();
        self.strays_dismissed = false;
        self.ride_to("Sloth Writer");
    }

    // ----- documents ---------------------------------------------------------

    fn select(&mut self, id: String) {
        // Switching documents is a natural moment to save.
        self.save_all();
        if let Some(tab) = self.ensure_open(&id) {
            self.docs.activate(tab);
        }
        self.selected = Some(id);
        self.focus_editor = true;
    }

    /// Open a document (if it isn't already) without showing it: its tab.
    /// `None` for a folder, or a file that can't be read (with a message).
    fn ensure_open(&mut self, id: &str) -> Option<TabId> {
        if let Some(&tab) = self.open.get(id)
            && self.docs.get(tab).is_some()
        {
            return Some(tab);
        }
        let path = self.project.as_ref()?.file_path(id)?;
        match Document::open(&path) {
            Ok(doc) => {
                let tab = self.docs.add(doc);
                self.open.insert(id.to_owned(), tab);
                Some(tab)
            }
            Err(e) => {
                self.error = Some(format!("Couldn't open {}: {e}", path.display()));
                None
            }
        }
    }

    /// An item's open document, if it's open.
    fn tab_of(&mut self, id: &str) -> Option<&mut Tab> {
        let tab = *self.open.get(id)?;
        self.docs.get_mut(tab)
    }

    /// The selected item's open document, if it's a document.
    fn current(&mut self) -> Option<&mut Tab> {
        let id = self.selected.clone()?;
        self.tab_of(&id)
    }

    fn any_unsaved(&self) -> bool {
        self.docs.iter().any(|t| t.doc.path.is_some() && t.doc.is_dirty())
    }

    /// Save every open document with unsaved edits. One whose file another
    /// program changed since it was read is left alone: writing would throw
    /// away those changes, so it waits for the person to choose a version
    /// in the notice above the editor.
    fn save_all(&mut self) {
        // Another copy of Writer has the project now: write nothing over it.
        if self.lock_lost.is_some() {
            return;
        }
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
        // Cards moved, notes written and strings tied on the canvas.
        if let Some(canvas) = self.canvas.as_mut()
            && let Some(project) = &self.project
            && let Err(e) = canvas.save(&project.root)
        {
            failed.push(format!("the canvas ({e})"));
        }
        // Words written since the last save count towards today.
        let edited: Vec<(String, String)> = self
            .open
            .iter()
            .filter_map(|(id, &tab)| self.docs.get(tab).filter(|t| t.doc.is_dirty()).map(|t| (id.clone(), t.doc.text.clone())))
            .collect();
        for (id, text) in edited {
            self.note_words(&id, &text);
        }
        if self.stats_dirty
            && let Some(project) = &self.project
        {
            match self.stats.save(&project.root) {
                Ok(()) => self.stats_dirty = false,
                Err(e) => failed.push(format!("the writing statistics ({e})")),
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
        self.check_strays();
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
        self.keep_unsaved_if_taken();
        self.auto_backup(crate::backup::When::Closing);
        let Some(id) = self.unresolved_change() else {
            if let Some(project) = &self.project {
                crate::lock::release(&project.root);
            }
            return true;
        };
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
        let Some(id) = self.selected.clone() else { return };
        self.resolve_notice_for(&id, choice);
    }

    /// The answer in the notice above an item's editor (main or split).
    fn resolve_notice_for(&mut self, id: &str, choice: NoticeChoice) {
        let Some(tab) = self.tab_of(id) else { return };
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
    /// `near` is in the Trash). With the project set to newest first, it goes
    /// at the top of the folder instead: the one selected, or the one the
    /// selected document is in.
    fn place_near(&self, near: Option<&str>) -> Place {
        let manuscript = Role::Manuscript.folder();
        let Some(project) = &self.project else { return Place::Into(manuscript.to_owned()) };
        let near = near.and_then(|id| project.node(id)).filter(|n| project.role_of(&n.id) != Some(Role::Trash));
        let top = project.new_at_top();
        let folder = match near {
            None => project.node(manuscript),
            Some(node) if node.role.is_some() || !node.is_document() => Some(node),
            Some(node) if top => project.parent_of(&node.id),
            Some(node) => return Place::After(node.id.clone()),
        };
        let Some(folder) = folder else { return Place::Into(manuscript.to_owned()) };
        match folder.children.first() {
            Some(first) if top => Place::Before(first.id.clone()),
            _ => Place::Into(folder.id.clone()),
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

    /// Split at Cursor: the text after `at` (a character index) moves into
    /// a new document just after this one, which you then name. The new
    /// one keeps the status, label, keywords and links; the words count as
    /// moved, not written.
    fn split_document(&mut self, ctx: &egui::Context, id: &str, at: usize) {
        self.save_all();
        let Some(tab) = self.tab_of(id) else { return };
        let text = tab.doc.text.clone();
        let byte = text.char_indices().nth(at).map_or(text.len(), |(b, _)| b);
        let (before, after) = (text[..byte].trim_end().to_owned(), text[byte..].trim_start().to_owned());
        let Some(project) = self.project.as_mut() else { return };
        let Some(node) = project.node(id) else { return };
        let title = format!("{} (continued)", node.title);
        let meta = Meta {
            status: node.meta.status,
            label: node.meta.label,
            keywords: node.meta.keywords.clone(),
            links: node.meta.links.clone(),
            exclude_from_compile: node.meta.exclude_from_compile,
            ..Meta::default()
        };
        let made = project.add_document(&Place::After(id.to_owned()), &title).and_then(|new| {
            let path = project.file_path(&new).ok_or_else(|| io::Error::other("no file"))?;
            sloth_core::document::write_atomically(&path, after.as_bytes())?;
            project.set_meta(&new, meta);
            Ok(new)
        });
        let new = match made {
            Ok(new) => new,
            Err(e) => {
                self.error = Some(format!("Couldn't split the document: {e}"));
                return;
            }
        };
        self.manifest_dirty = true;
        if let Some(tab) = self.tab_of(id) {
            tab.doc.text = before.clone();
            // Undo here would bring back text that's now in the new one.
            editor::forget_undo(ctx, tab.id);
        }
        // Moved, not written: the two changes cancel out in today's words.
        self.note_words(id, &before);
        self.note_words(&new, &after);
        self.save_all();
        self.select(new.clone());
        self.start_rename(&new);
    }

    /// Back Up…: choose a folder (the last one is offered) for a dated zip
    /// of the whole project.
    fn back_up_dialog(&mut self, now: f64) {
        if self.project.is_none() {
            return;
        }
        let mut dialog = rfd::FileDialog::new().set_title("Back Up the Project To…");
        if let Some(folder) = self.settings.backup_folder.as_deref().filter(|f| Path::new(f).is_dir()) {
            dialog = dialog.set_directory(folder);
        }
        if let Some(folder) = dialog.pick_folder() {
            self.back_up_to(&folder, now);
        }
    }

    fn back_up_to(&mut self, folder: &Path, now: f64) {
        self.save_all();
        let Some(project) = &self.project else { return };
        let name = crate::backup::file_name(project.title(), chrono::Local::now().naive_local());
        match crate::backup::back_up(&project.root, folder, &name) {
            Ok(path) => {
                self.settings.backup_folder = Some(folder.to_string_lossy().into_owned());
                self.set_status(format!("Backed up to {}", path.display()), now);
            }
            Err(e) => self.error = Some(format!("Couldn't back up the project: {e}")),
        }
    }

    /// Closing a project another copy took over: what was typed here since
    /// isn't saved over theirs, but kept as a snapshot of each document.
    fn keep_unsaved_if_taken(&mut self) {
        let (Some(lock), Some(project)) = (&self.lock_lost, &self.project) else { return };
        let name = format!("written here while it was open on {}", lock.host);
        for (id, &tab) in &self.open {
            if let Some(tab) = self.docs.get(tab).filter(|t| t.doc.is_dirty()) {
                let _ = crate::snapshots::take(&project.root, id, &tab.doc.text, &name);
            }
        }
    }

    /// Keep this copy's claim on the project fresh, and notice if another
    /// copy has taken it.
    fn lock_tick(&mut self, now: f64) {
        if self.lock_lost.is_some() || now - self.last_lock_beat < crate::lock::BEAT_SECONDS {
            return;
        }
        self.last_lock_beat = now;
        let Some(project) = &self.project else { return };
        if let Ok(Some(lock)) = crate::lock::beat(&project.root) {
            self.lock_lost = Some(lock);
        }
    }

    /// "Open Anyway?" for a project another copy of Writer has open.
    fn open_anyway_dialog(&mut self, ctx: &egui::Context) {
        let Some((root, lock)) = self.pending_open.clone() else { return };
        let name = root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let mut choice = None;
        egui::Modal::new(egui::Id::unique("open-anyway")).show(ctx, |ui| {
            ui.set_width(380.0);
            ui.heading(format!("\u{201c}{name}\u{201d} is open on {}", lock.host));
            ui.label(egui::RichText::new(crate::lock::since(&lock)).weak());
            ui.add_space(6.0);
            ui.label("Opening it here as well could mean one copy saves over the other's changes. Close it there first. If Writer quit unexpectedly there, it's safe to open it here.");
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    choice = Some(false);
                }
                if ui.button("Open Anyway").clicked() {
                    choice = Some(true);
                }
            });
        });
        match choice {
            Some(true) => {
                self.pending_open = None;
                self.open_project_anyway(&root);
            }
            Some(false) => self.pending_open = None,
            None => {}
        }
    }

    /// The bar saying another copy took the project, with what to do.
    fn lock_lost_bar(&mut self, ui: &mut egui::Ui) {
        let Some(lock) = self.lock_lost.clone() else { return };
        let mut take_back = false;
        let mut close = false;
        egui::Panel::top(egui::Id::unique("lock-lost")).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    format!(
                        "This project was opened on {} ({}). Writer has stopped saving here so the two copies don't overwrite each other.",
                        lock.host,
                        crate::lock::since(&lock)
                    ),
                );
                if ui.button("Close It Here").on_hover_text("What you wrote here since is kept as snapshots").clicked() {
                    close = true;
                }
                if ui.button("Take It Back").on_hover_text("Save here again (close it on the other computer first)").clicked() {
                    take_back = true;
                }
            });
        });
        if take_back
            && let Some(project) = &self.project
        {
            let _ = crate::lock::take(&project.root);
            self.lock_lost = None;
            self.save_all();
        }
        if close {
            self.close_project();
        }
    }

    /// An automatic backup, if they're on and one is due (see backup.rs).
    fn auto_backup(&mut self, when: crate::backup::When) {
        if !self.settings.auto_backup {
            return;
        }
        let Some(project) = &self.project else { return };
        let keep = self.settings.backup_keep.max(1) as usize;
        if let Err(e) = crate::backup::automatic(&project.root, project.title(), &self.settings.backups_root(), keep, when) {
            self.error = Some(format!("Couldn't back up the project automatically: {e}"));
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
                if self.project.as_ref().is_none_or(|p| p.node(&id).is_some_and(|n| n.title == title.trim())) {
                    return;
                }
                // A profile's [[references]] follow it to its new name.
                let rewrites = self.reference_rewrites(&id, &title);
                let Some(project) = self.project.as_mut() else { return };
                match project.rename(&id, &title) {
                    Ok(change) => {
                        self.follow(change.as_slice());
                        self.apply_rewrites(rewrites);
                    }
                    Err(e) => self.error = Some(e.to_string()),
                }
            }
            BinderAction::NewDocument(near) => self.new_item(Some(near), true),
            BinderAction::NewFolder(near) => self.new_item(Some(near), false),
            BinderAction::Trash(id) => self.trash(&id),
            BinderAction::EmptyTrash => self.confirm_empty_trash = true,
            BinderAction::Move { id, place } => self.move_item(&id, &place),
            BinderAction::AddProfileFolder(Some(kind)) => self.add_profile_folder(kind),
            BinderAction::AddProfileFolder(None) => self.new_profile_folder = Some((String::new(), String::new())),
            BinderAction::OpenInSplit(id) => self.open_in_split(id),
            BinderAction::BreakUp(id) => self.start_break_up(&id),
            BinderAction::Combine(id) => self.start_combine(&id, false),
            BinderAction::CombineWithNext(id) => self.start_combine(&id, true),
            BinderAction::StatusInside { id, status, deep } => self.set_inside(&id, deep, |meta| meta.status = status),
            BinderAction::LabelInside { id, label, deep } => self.set_inside(&id, deep, |meta| meta.label = label),
            BinderAction::RemoveProfileFolder(kind) => {
                let Some(project) = self.project.as_mut() else { return };
                match project.remove_profile_folder(&kind) {
                    Ok(()) => {
                        if self.selected.as_deref() == Some(kind.as_str()) {
                            self.selected = None;
                        }
                    }
                    Err(e) => self.error = Some(e.to_string()),
                }
            }
        }
    }

    /// Change the metadata of what's in an item (not the item itself):
    /// what's directly in it, or (`deep`) everything under it. Says how
    /// many changed.
    fn set_inside(&mut self, id: &str, deep: bool, change: impl Fn(&mut Meta)) {
        let Some(project) = self.project.as_mut() else { return };
        let Some(node) = project.node(id) else { return };
        let inside: Vec<(String, Meta)> = binder::inside(node, deep).into_iter().map(|n| (n.id.clone(), n.meta.clone())).collect();
        let mut changed = 0;
        for (item, mut meta) in inside {
            change(&mut meta);
            if project.set_meta(&item, meta) {
                changed += 1;
            }
        }
        if changed > 0 {
            self.manifest_dirty = true;
            self.save_all();
        }
        let title = self.project.as_ref().and_then(|p| p.node(id)).map(|n| n.title.clone()).unwrap_or_default();
        let what = match changed {
            0 => "Nothing to change".to_owned(),
            1 => "Changed 1 item".to_owned(),
            n => format!("Changed {n} items"),
        };
        // The time of the last look at the disk: within a couple of seconds
        // of now (binder actions don't carry the time).
        self.status = Some((format!("{what} in \u{201c}{title}\u{201d}"), self.last_disk_check.max(0.0)));
    }

    fn add_profile_folder(&mut self, kind: crate::project::ProfileKind) {
        let Some(project) = self.project.as_mut() else { return };
        match project.add_profile_folder(kind) {
            Ok(id) => self.select(id),
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    /// New Profile Folder, for a folder of your own.
    fn new_profile_folder_window(&mut self, ctx: &egui::Context) {
        let Some((name, item)) = self.new_profile_folder.as_mut() else { return };
        let mut choice = None;
        egui::Modal::new(egui::Id::unique("new-profile-folder")).show(ctx, |ui| {
            ui.set_width(340.0);
            ui.heading("New Profile Folder");
            ui.label("Folder name");
            ui.add(egui::TextEdit::singleline(name).hint_text("Ships").desired_width(f32::INFINITY));
            ui.label("One of them is a…");
            ui.add(egui::TextEdit::singleline(item).hint_text("Ship").desired_width(f32::INFINITY));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.add_enabled(!name.trim().is_empty(), egui::Button::new("Add Folder")).clicked() {
                    choice = Some(true);
                }
                if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                    choice = Some(false);
                }
            });
        });
        match choice {
            Some(true) => {
                let (name, item) = self.new_profile_folder.take().expect("checked above");
                let kind = crate::project::ProfileKind {
                    id: String::new(),
                    name: name.trim().to_owned(),
                    item: item.trim().to_owned(),
                    sections: vec!["Description".to_owned(), "Notes".to_owned()],
                };
                self.add_profile_folder(kind);
            }
            Some(false) => self.new_profile_folder = None,
            None => {}
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
        match project.empty_trash(&mut keep_emptied) {
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
        // ⌥⌘ is Ctrl+Shift on Windows and Linux (see sloth_core::keys).
        // The longer shortcut first, so ⌥⌘N isn't taken as ⌘N.
        if self.project.is_some() && pressed(ctx, keys::OPT_CMD, Key::N) {
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
        // ⌘5: snapshot the open document.
        if pressed(ctx, Modifiers::COMMAND, Key::Num5)
            && let Some(id) = self.selected.clone()
            && self.project.as_ref().and_then(|p| p.node(&id)).is_some_and(Node::is_document)
        {
            let now = ctx.input(|i| i.time);
            self.take_snapshot(&id, "", now);
        }
        if self.project.is_some() && pressed(ctx, Modifiers::COMMAND, Key::Backslash) {
            self.toggle_split();
        }
        if self.project.is_some() && pressed(ctx, Modifiers::COMMAND | Modifiers::SHIFT, Key::E) {
            self.compile_open = true;
        }
        if self.project.is_some() && pressed(ctx, keys::OPT_CMD, Key::I) {
            self.show_inspector = !self.show_inspector;
        }
        // ⌥⌘F, as in Scrivener (F11 on Windows and Linux): Focus mode. Esc leaves it.
        if self.project.is_some() && ctx.input_mut(|i| i.consume_shortcut(&keys::FOCUS)) {
            self.set_focus_mode(ctx, !self.focus_mode);
        }
        // (Not while Esc is putting away suggestions or a menu.)
        let esc_taken = self.suggesting.is_some() || self.create_menu.is_some();
        if self.focus_mode && !esc_taken && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            self.set_focus_mode(ctx, false);
        }
        // ⌘1, ⌘2, ⌘3: a folder as Slothenings, corkboard or outline. With
        // a document selected, the folder it's in.
        for (key, view) in [(Key::Num1, views::FolderView::Slothenings), (Key::Num2, views::FolderView::Corkboard), (Key::Num3, views::FolderView::Outline)] {
            if self.project.is_some() && pressed(ctx, Modifiers::COMMAND, key) {
                self.show_folder_as(ctx, view);
            }
        }
        if self.project.is_some() && pressed(ctx, keys::OPT_CMD, Key::K) {
            self.add_comment(ctx);
        }
        if self.project.is_some() && pressed(ctx, keys::OPT_CMD, Key::R) {
            self.toggle_repetition();
        }
        if self.project.is_some() && pressed(ctx, keys::OPT_CMD, Key::C) {
            self.toggle_canvas();
        }
        if self.project.is_some() && pressed(ctx, keys::OPT_CMD, Key::B) {
            self.show_binder = !self.show_binder;
        }
        // ⌘K: split the open document at the cursor.
        if self.project.is_some()
            && pressed(ctx, Modifiers::COMMAND, Key::K)
            && let Some(id) = self.selected.clone()
            && let Some(tab) = self.tab_of(&id).map(|t| t.id)
        {
            let at = editor::selection(ctx, tab).map_or(0, |r| r.start);
            self.split_document(ctx, &id, at);
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
            && !esc_taken
            && self.renaming.is_none()
            && !self.confirm_empty_trash
            && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape))
        {
            self.find_action(ctx, FindAction::Close);
        }
        // ⌥⌘, before ⌘, so it isn't taken as App Settings.
        if pressed(ctx, keys::OPT_CMD, Key::Comma) {
            self.open_project_settings();
        }
        if pressed(ctx, Modifiers::COMMAND, Key::Comma) {
            self.settings_open = true;
        }
        // ⌘? (⇧⌘/): help.
        if pressed(ctx, Modifiers::COMMAND | Modifiers::SHIFT, Key::Slash)
            || pressed(ctx, Modifiers::COMMAND, Key::Questionmark)
        {
            self.help = Some(about_ui::HelpPage::Shortcuts);
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
            // Tiles for recent projects need the room: less space above.
            let top = ui.available_height()
                * if self.recent.is_empty() {
                    0.3
                } else if self.settings.start_view == settings_ui::StartView::CardFile {
                    0.05
                } else {
                    0.12
                };
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(top);
                // The sloth, and hello for the time of day.
                let hour = chrono::Timelike::hour(&chrono::Local::now());
                let (hello, pose) = sloth_ui::greeting(hour);
                if self.settings.theme.has_glow() {
                    self.sloth_emblem(ui);
                } else {
                    let sloth = self.sloth(ui.ctx(), pose);
                    ui.add(egui::Image::new(&sloth).fit_to_exact_size(egui::Vec2::splat(120.0)));
                }
                ui.heading(hello);
                ui.weak(format!("Sloth Writer \u{b7} Version {}", crate::version::VERSION));
                self.fact_of_the_day(ui);
                ui.add_space(16.0);
                // New, Open and Import: a row of icons, named on hover.
                let actions = [
                    (crate::icons::NEW, "New Project\u{2026}", "Start a new project".to_owned()),
                    (crate::icons::OPEN, "Open Project\u{2026}", sloth_core::keys::shown("Open a project folder (\u{2318}O)")),
                    (crate::icons::IMPORT, "Import from Scrivener\u{2026}", "Make a Writer project from a Scrivener 3 project (which isn't changed)".to_owned()),
                    (crate::icons::FROM_HELPER, "Get Project from the Helper\u{2026}", "Bring over a project another computer syncs whole through the web helper".to_owned()),
                ];
                let mut chosen = None;
                ui.horizontal(|ui| {
                    let (size, gap) = (52.0, 14.0);
                    ui.spacing_mut().item_spacing.x = gap;
                    let n = actions.len() as f32;
                    ui.add_space(((ui.available_width() - (n * size + (n - 1.0) * gap)) / 2.0).max(0.0));
                    for (i, (icon, name, hint)) in actions.iter().enumerate() {
                        if start_icon(ui, size, icon, name, hint) {
                            chosen = Some(i);
                        }
                    }
                });
                match chosen {
                    Some(0) => self.new_project_dialog(),
                    Some(1) => self.open_project_dialog(),
                    Some(2) => {
                        let now = ui.input(|i| i.time);
                        self.import_scrivener_dialog(now);
                    }
                    Some(3) => self.start_get_project(),
                    _ => {}
                }
                self.recent_list(ui);
                ui.add_space(20.0);
                ui.horizontal(|ui| {
                    // Centre the pair of buttons.
                    let width = 150.0;
                    ui.add_space(((ui.available_width() - width) / 2.0).max(0.0));
                    if ui.small_button("Settings\u{2026}").clicked() {
                        self.settings_open = true;
                    }
                    if ui.small_button("About Sloth Writer").clicked() {
                        self.help = Some(about_ui::HelpPage::About);
                    }
                });
                ui.add_space(16.0);
            });
            });
        });
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top(egui::Id::unique("toolbar")).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.toggle_value(&mut self.show_binder, "Binder").on_hover_text(sloth_core::keys::shown("Show or hide the binder (\u{2325}\u{2318}B)"));
                if let Some(project) = &self.project {
                    let title = egui::Button::new(egui::RichText::new(project.title()).strong()).frame(false);
                    if ui.add(title).on_hover_text("Rename the project or change its icon").clicked() {
                        self.start_renaming_project();
                    }
                }
                ui.separator();
                // Icons that match the binder's, so the two "new" buttons
                // read apart at a glance, and the Trash kept well away.
                let doc = ui.button(format!("{} New Document", crate::icons::DOCUMENT)).on_hover_text(sloth_core::keys::shown("Add a document (\u{2318}N)"));
                if doc.clicked() {
                    self.new_item(self.selected.clone(), true);
                }
                ui.add_space(6.0);
                let folder = ui.button(format!("{} New Folder", crate::icons::FOLDER)).on_hover_text(sloth_core::keys::shown("Add a folder (\u{2325}\u{2318}N)"));
                if folder.clicked() {
                    self.new_item(self.selected.clone(), false);
                }
                ui.add_space(12.0);
                ui.separator();
                ui.add_space(12.0);
                let can_trash = self
                    .selected
                    .as_deref()
                    .and_then(|id| self.project.as_ref()?.node(id))
                    .is_some_and(|n| n.role.is_none());
                let trash = egui::Button::new(format!("{} Move to Trash", crate::icons::TRASH));
                if ui.add_enabled(can_trash, trash).on_hover_text("Move the selected item to the Trash").clicked() {
                    let id = self.selected.clone().expect("enabled only with a selection");
                    self.trash(&id);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Less often wanted: in a menu, so the toolbar fits.
                    ui.menu_button("More\u{2026}", |ui| {
                        if ui.button(format!("{} Mobile Notes", crate::icons::NOTEPAD)).on_hover_text("The notepad you jot in on your phone, beside what you're writing").clicked() {
                            self.open_notepad();
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("Back Up\u{2026}").on_hover_text("Save a dated zip of the whole project somewhere else").clicked() {
                            let now = ui.input(|i| i.time);
                            self.back_up_dialog(now);
                            ui.close();
                        }
                        if ui.add(egui::Button::new("App Settings\u{2026}").shortcut_text(sloth_core::keys::shown("\u{2318},"))).clicked() {
                            self.settings_open = true;
                            ui.close();
                        }
                        let project_settings = egui::Button::new("Project Settings\u{2026}").shortcut_text(sloth_core::keys::shown("\u{2325}\u{2318},"));
                        if ui.add(project_settings).on_hover_text("Title, icon, where new items go, goals, statuses, labels and profile sections").clicked() {
                            self.open_project_settings();
                            ui.close();
                        }
                        let mut repeats = self.settings.show_repetition;
                        if ui.checkbox(&mut repeats, sloth_core::keys::shown("Highlight Repeated Words  \u{2325}\u{2318}R")).changed() {
                            self.toggle_repetition();
                            ui.close();
                        }
                        if ui.add(egui::Button::new("Help").shortcut_text(sloth_core::keys::shown("\u{2318}?"))).clicked() {
                            self.help = Some(about_ui::HelpPage::Shortcuts);
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("Close Project").clicked() {
                            self.close_project();
                            ui.close();
                        }
                    })
                    .response
                    .on_hover_text("Mobile Notes, Back Up, Settings, Help, Close Project");
                    ui.toggle_value(&mut self.show_inspector, "Inspector").on_hover_text(sloth_core::keys::shown("\u{2325}\u{2318}I"));
                    ui.toggle_value(&mut self.split_open, "Split").on_hover_text(sloth_core::keys::shown("A second pane beside the editor (\u{2318}\\)"));
                    ui.toggle_value(&mut self.canvas_open, "Canvas")
                        .on_hover_text(sloth_core::keys::shown("Every document on one surface, strung to what it refers to, with notes of your own (\u{2325}\u{2318}C)"));
                    if ui.button("Focus").on_hover_text(sloth_core::keys::shown("Full screen, just the text (\u{2325}\u{2318}F; Esc to leave)")).clicked() {
                        let ctx = ui.ctx().clone();
                        self.set_focus_mode(&ctx, true);
                    }
                    if ui.button("Compile\u{2026}").on_hover_text(sloth_core::keys::shown("\u{21e7}\u{2318}E")).clicked() {
                        self.compile_open = true;
                    }
                    if !self.settings.helper_url.trim().is_empty() && !self.helper_token.is_empty() {
                        let label = if self.sync_running() { "Syncing\u{2026}" } else { "Sync" };
                        let hint = if self.sync_message.is_empty() { "Sync with the web helper" } else { &self.sync_message };
                        if ui.add_enabled(!self.sync_running(), egui::Button::new(label)).on_hover_text(hint).clicked() {
                            self.sync_now = true;
                        }
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
        let ctx = ui.ctx().clone();
        let words = self.current().map(|t| {
            let all = inspector::count_words(&t.doc.text);
            // With text selected, its words too.
            let picked = editor::selection(&ctx, t.id).filter(|r| !r.is_empty()).map(|r| {
                let text: String = t.doc.text.chars().skip(r.start).take(r.len()).collect();
                inspector::count_words(&text)
            });
            (picked, all)
        });
        // A folder (as Slothenings, a corkboard or an outline): all its words.
        let words = match (words, self.selected.clone()) {
            (None, Some(id)) if self.project.as_ref().and_then(|p| p.node(&id)).is_some_and(|n| !n.is_document()) => {
                Some((None, self.words_under(&id)))
            }
            (words, _) => words,
        };
        let status = self.status.as_ref().map(|(message, _)| message.clone());
        let today = self.today_summary();
        let (written, goal) = self.today_goal();
        egui::Panel::bottom(egui::Id::unique("status")).show(ui, |ui| {
            ui.horizontal(|ui| {
                let plural = |n: usize| if n == 1 { "word" } else { "words" };
                match words {
                    Some((Some(picked), all)) => ui.label(format!("{picked} of {all} {}", plural(all))),
                    Some((None, n)) => ui.label(format!("{n} {}", plural(n))),
                    None => ui.label(""),
                };
                ui.separator();
                self.goal_ring(ui, &today, written, goal);
                ui.separator();
                self.sprint_button(ui);
                if self.settings.show_repetition {
                    ui.separator();
                    let chip = egui::Button::new(egui::RichText::new("Repeats shown").small()).frame(false);
                    if ui.add(chip).on_hover_text(sloth_core::keys::shown("Echoes are tinted, watched words underlined. Click (or \u{2325}\u{2318}R) to hide them.")).clicked() {
                        self.toggle_repetition();
                    }
                }
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
        // A folder shows its items (corkboard or outline) instead of text.
        let folder = match self.selected.as_deref().and_then(|id| self.project.as_ref()?.node(id)) {
            Some(node) if !node.is_document() => Some(node.id.clone()),
            _ => None,
        };
        // A profile (a character, a location…) shows its form.
        let profile = self.selected.clone().and_then(|id| self.profile_kind_of(&id).map(|kind| (id, kind)));
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
        let marks = self.repetition_marks(ui.visuals());
        // Where each comment is, for its card in the margin.
        let anchors: Vec<usize> = self.current().map(|t| sloth_core::markdown::comments(&t.doc.text).iter().map(|c| c.passage.as_ref().map_or(c.words.start, |p| p.start)).collect()).unwrap_or_default();
        let highlights = Highlights { all: &matches, current: self.find.current.clone(), marks: &marks, anchors: &anchors };
        let reveal = self.reveal.take();
        let mut split = None;
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(ui.visuals().extreme_bg_color).inner_margin(egui::Margin::symmetric(24, 16)))
            .show(ui, |ui| {
                ui.set_opacity(self.view_opacity);
                if let Some(folder) = &folder {
                    self.folder_view(ui, folder);
                    return;
                }
                if let Some((id, kind)) = &profile {
                    self.profile_form(ui, id, kind);
                    return;
                }
                if let Some(id) = self.selected.clone()
                    && self.current().is_some()
                {
                    self.notes_panel(ui, &id, settings.max_text_width);
                }
                let doc = self.selected.clone().unwrap_or_default();
                self.suggestion_keys(ui.ctx(), &doc);
                match self.current() {
                    Some(tab) => {
                        if focus {
                            ui.memory_mut(|m| m.request_focus(editor::editor_id(tab.id)));
                        }
                        // With comments, the text leaves room for their column.
                        let full = ui.available_rect_before_wrap();
                        let column = (!anchors.is_empty()).then(|| {
                            egui::Rect::from_min_max(egui::pos2(full.right() - comments_ui::COLUMN, full.top()), full.max)
                        });
                        let info = match column {
                            Some(column) => {
                                let text_rect = egui::Rect::from_min_max(full.min, egui::pos2(column.left() - comments_ui::GAP, full.bottom()));
                                let mut left = ui.new_child(egui::UiBuilder::new().max_rect(text_rect).layout(egui::Layout::top_down(egui::Align::Min)));
                                editor::show(&mut left, tab.id, &mut tab.doc.text, &settings, highlights, reveal, None)
                            }
                            None => editor::show(ui, tab.id, &mut tab.doc.text, &settings, highlights, reveal, None),
                        };
                        split = info.split_at;
                        let (hovered, clicked) = references_ui::reported(&tab.doc.text, &info);
                        self.reference_events(ui, hovered, clicked);
                        self.suggestions_ui(ui, &doc, &info);
                        if let Some(column) = column {
                            self.comment_cards(ui, column, &info.anchor_tops);
                        }
                    }
                    None => {
                        self.empty_state(ui, sloth_ui::Pose::Reading, "Nothing open", &sloth_core::keys::shown("Choose a document in the binder, or start one with \u{2318}N."));
                    }
                }
            });
        if let Some(at) = split
            && let Some(id) = self.selected.clone()
        {
            self.split_document(&ui.ctx().clone(), &id, at);
        }
    }

    /// Animate or not (the Mac's Reduce motion; never in tests): egui's own
    /// animations (panels sliding, sections folding, scrolling to the
    /// cursor) take their time from its style.
    fn motion_style(&mut self, ctx: &egui::Context) {
        let animate = !cfg!(test) && sloth_core::motion::animate();
        if self.animating == Some(animate) {
            return;
        }
        self.animating = Some(animate);
        ctx.all_styles_mut(|style| {
            style.animation_time = if animate { 0.18 } else { 0.0 };
            style.scroll_animation = if animate { egui::style::ScrollAnimation::default() } else { egui::style::ScrollAnimation::none() };
        });
    }

    /// A short fade-in when the main area changes what it shows: another
    /// document, the corkboard or outline, the Canvas.
    fn view_fade(&mut self, ctx: &egui::Context) {
        let mode = self.selected.as_ref().and_then(|id| self.folder_views.get(id));
        let key = format!("{:?}|{}|{mode:?}", self.selected, self.canvas_open);
        let now = ctx.input(|i| i.time);
        if key != self.view_key {
            self.view_key = key;
            self.view_since = now;
        }
        self.view_opacity = if self.animating == Some(true) {
            0.15 + 0.85 * sloth_core::motion::ease(now, self.view_since, 0.18)
        } else {
            1.0
        };
        if self.view_opacity < 1.0 {
            ctx.request_repaint();
        }
    }

    /// The usual window: toolbar, binder, inspector, editor…
    fn main_view(&mut self, ui: &mut egui::Ui) {
        self.view_fade(&ui.ctx().clone());
        self.toolbar(ui);
        let mut actions = Vec::new();
        let binder = egui::Panel::left(egui::Id::unique("binder")).resizable(true).default_size(240.0).size_range(160.0..=480.0);
        // Hidden, the binder keeps its width for next time.
        if self.show_binder {
            binder.show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    if let Some(project) = &self.project {
                        let selected = self.selected.as_deref();
                        let notepad = project.notepad().map(|n| n.id.as_str());
                        actions = binder::show(ui, &project.manifest, selected, notepad, &mut self.renaming);
                    }
                });
            });
        }
        for action in actions {
            self.apply(action);
        }
        self.inspector(ui);
        self.split_pane(ui);
        self.status_bar(ui);
        self.results_panel(ui);
        self.find_bar(ui);
        if self.canvas_open {
            self.canvas_view(ui);
        } else {
            self.notice_bar(ui);
            self.editor_area(ui);
        }
    }

    fn set_focus_mode(&mut self, ctx: &egui::Context, on: bool) {
        if on == self.focus_mode {
            return;
        }
        if on {
            if self.current().is_none() {
                let now = ctx.input(|i| i.time);
                self.set_status("Choose a document to write in, then Focus.".to_owned(), now);
                return;
            }
            self.was_fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
            self.focus_editor = true;
        } else if !self.was_fullscreen {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
        }
        self.focus_mode = on;
    }

    /// Focus mode: the document's text alone, in its column, with a faint
    /// line of figures at the bottom.
    fn focus_view(&mut self, ui: &mut egui::Ui) {
        let mut settings = self.editor.clone();
        settings.typewriter = self.settings.typewriter != settings_ui::Typewriter::Never;
        settings.focus_paragraph = self.settings.focus_paragraph;
        let words = self.current().map(|t| inspector::count_words(&t.doc.text)).unwrap_or(0);
        let today = self.today_summary();
        let fill = ui.visuals().extreme_bg_color;
        egui::Panel::bottom(egui::Id::unique("focus-status")).frame(egui::Frame::NONE.fill(fill).inner_margin(egui::Margin::symmetric(24, 6))).show(ui, |ui| {
            ui.horizontal(|ui| {
                let color = ui.visuals().weak_text_color().gamma_multiply(0.7);
                let faint = move |text: String| egui::RichText::new(text).small().color(color);
                ui.label(faint(format!("{} {}", crate::stats::thousands(words as u32), if words == 1 { "word" } else { "words" })));
                ui.label(faint("\u{b7}".into()));
                ui.label(faint(today));
                if let Some(sprint) = self.sprint_summary(ui.input(|i| i.time)) {
                    ui.label(faint("\u{b7}".into()));
                    ui.label(faint(format!("Sprint {sprint}")));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(egui::Button::new(faint("Esc to leave Focus".into())).frame(false)).clicked() {
                        let ctx = ui.ctx().clone();
                        self.set_focus_mode(&ctx, false);
                    }
                });
            });
        });
        self.focus_bins(ui, fill);
        self.focus_top_bar(ui, fill);
        let focus = std::mem::take(&mut self.focus_editor) && !ui.input(|i| i.pointer.any_pressed() || i.pointer.any_released());
        let top = if self.settings.show_bins { 24 } else { 48 };
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(fill).inner_margin(egui::Margin { left: 24, right: 24, top, bottom: 0 }))
            .show(ui, |ui| {
                if let Some(doc) = self.selected.clone() {
                    self.suggestion_keys(ui.ctx(), &doc);
                }
                if let Some(tab) = self.current() {
                    if focus {
                        ui.memory_mut(|m| m.request_focus(editor::editor_id(tab.id)));
                    }
                    let info = editor::show(ui, tab.id, &mut tab.doc.text, &settings, Highlights::default(), None, None);
                    let (hovered, clicked) = references_ui::reported(&tab.doc.text, &info);
                    self.reference_events(ui, hovered, clicked);
                    if let Some(doc) = self.selected.clone() {
                        self.suggestions_ui(ui, &doc, &info);
                    }
                }
            });
    }

    fn empty_trash_dialog(&mut self, ctx: &egui::Context) {
        if !self.confirm_empty_trash {
            return;
        }
        egui::Modal::new(egui::Id::unique("empty-trash")).show(ctx, |ui| {
            ui.heading("Empty the Trash?");
            ui.label("Everything in the Trash leaves the project, with its snapshots. It's kept as one zip file in your Mac's Trash, so it can still be got back from there until you empty that.");
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
        self.motion_style(&ctx);

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
        self.lock_lost_bar(ui);
        self.strays_bar(ui);
        self.open_anyway_dialog(&ctx);
        self.lock_tick(now);

        if self.project.is_none() {
            self.start_screen(ui);
            self.settings_window(&ctx);
            self.help_window(&ctx);
            self.get_project_window(&ctx);
            self.screen_glow_ui(&ctx);
            self.elevator_ui(&ctx);
            return;
        }

        if self.focus_mode && self.current().is_some() {
            self.focus_view(ui);
        } else {
            if self.focus_mode {
                // Nothing to write in (a folder, say): back to the usual view.
                self.set_focus_mode(&ctx, false);
            }
            self.main_view(ui);
        }
        self.check_goal(now);
        self.check_rewards(now);
        self.sprint_tick(&ctx, now);
        self.celebration_ui(&ctx);
        self.create_menu_ui(&ctx);
        self.empty_trash_dialog(&ctx);
        self.project_replace_dialog(&ctx);
        self.project_settings_window(&ctx);
        self.labels_window(&ctx);
        self.statuses_window(&ctx);
        self.compile_window(&ctx);
        self.compare_window(&ctx);
        self.rename_project_window(&ctx);
        self.sections_window(&ctx);
        self.new_profile_folder_window(&ctx);
        self.break_up_window(&ctx);
        self.combine_window(&ctx);
        self.settings_window(&ctx);
        self.help_window(&ctx);
        self.stats_window(&ctx);
        self.delete_snapshot_dialog(&ctx);
        self.screen_glow_ui(&ctx);
        self.elevator_ui(&ctx);

        if now - self.last_autosave >= AUTOSAVE_SECONDS {
            self.save_all();
            self.last_autosave = now;
        }
        // Every so often, see whether today's backup is due.
        if now - self.last_backup_check >= BACKUP_CHECK_SECONDS {
            self.last_backup_check = now;
            self.auto_backup(crate::backup::When::Daily);
        }
        if self.any_unsaved() {
            ctx.request_repaint_after(Duration::from_secs_f64(AUTOSAVE_SECONDS));
        }
        self.sync_tick(&ctx, now);
    }

    /// Called now and then and when quitting: save the words, and remember
    /// the project to reopen next time.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.save_all();
        let root = self.project.as_ref().map(|p| p.root.clone());
        eframe::set_value(storage, LAST_PROJECT_KEY, &root);
        eframe::set_value(storage, projects_ui::RECENT_KEY, &self.recent);
        eframe::set_value(storage, settings_ui::SETTINGS_KEY, &self.settings);
    }
}

/// One of the start screen's icons: a rounded square with the icon in it,
/// lit in the theme's accent on hover, with its name as hover text (and
/// for VoiceOver). Returns whether it was clicked.
fn start_icon(ui: &mut egui::Ui, size: f32, icon: &str, name: &str, hint: &str) -> bool {
    let (rect, response) = ui.allocate_exact_size(egui::Vec2::splat(size), egui::Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, name));
    let hovered = response.hovered();
    let visuals = ui.visuals();
    let accent = visuals.hyperlink_color;
    let fill = if hovered { accent.gamma_multiply(0.14) } else { visuals.widgets.inactive.weak_bg_fill };
    let stroke = egui::Stroke::new(if hovered { 1.5 } else { 1.0 }, if hovered { accent } else { visuals.widgets.noninteractive.bg_stroke.color });
    ui.painter().rect(rect, 12.0, fill, stroke, egui::StrokeKind::Inside);
    let color = if hovered { accent } else { visuals.text_color() };
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, icon, egui::FontId::proportional(size * 0.42), color);
    let response = response.on_hover_text(format!("{name}\n{hint}"));
    response.clicked()
}

/// Where what's emptied from the Trash goes: the Mac's own Trash (in tests,
/// a scratch folder, not the real one).
fn keep_emptied(zip: &Path) -> io::Result<()> {
    if cfg!(test) {
        let dir = std::env::temp_dir().join(format!("sloth-writer-test-trash-{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        std::fs::copy(zip, dir.join(zip.file_name().unwrap_or_default())).map(|_| ())
    } else {
        sloth_core::trash::move_to_trash(zip)
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
