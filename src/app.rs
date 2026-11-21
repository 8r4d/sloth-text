//! Top-level application state and UI wiring.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use eframe::egui::{
    self, Align, Key, Layout, Margin, Modifiers, RichText, ScrollArea, ViewportCommand,
};

use crate::commands::{self, Command, MenuEntry};
use crate::document::{DiskState, Document, SaveError};
use crate::editing;
use crate::fonts;
use crate::editor::{self, CursorInfo, EditorSettings, Highlights};
use crate::file_types;
use crate::find::{self, FindAction, FindBarInfo, FindOptions, FindState};
use crate::search_results::{self, ResultRow, ResultsAction, TabResults};
use crate::help;
use crate::recovery::{Leftovers, Recovery};
use crate::settings::{self, Settings};
use crate::tabs::{self, DiskNotice, Tab, TabBarAction, TabId, Tabs};
use crate::text_format::TextEncoding;
use crate::version;

/// How often to check open files for changes made by other programs.
const DISK_CHECK_INTERVAL: Duration = Duration::from_secs(2);
/// How often to write crash-recovery copies of unsaved documents.
const RECOVERY_INTERVAL: f64 = 5.0;

/// Replacing a document's text with the file on disk (asks first if there
/// are unsaved changes).
#[derive(Clone, Copy, Debug)]
enum ReloadKind {
    Revert,
    WithEncoding(TextEncoding),
}

/// Search results, kept until the text or the search changes, so we don't
/// search again on every frame.
#[derive(Default)]
struct MatchCache {
    query: String,
    options: FindOptions,
    tabs: HashMap<TabId, CachedSearch>,
}

struct CachedSearch {
    /// The text that was searched (compared to notice edits).
    text: String,
    matches: Result<Arc<Vec<Range<usize>>>, String>,
    /// Built when the all-tabs results panel needs them.
    rows: Option<Arc<Vec<ResultRow>>>,
}

/// A save that needs the user's decision before it can go ahead.
struct SaveProblem {
    tab: TabId,
    /// For Save As: where it was going.
    path: Option<PathBuf>,
    kind: SaveProblemKind,
}

enum SaveProblemKind {
    /// Characters that the file's encoding can't store.
    Unmappable(&'static encoding_rs::Encoding),
    /// The file was read with errors; saving would write the `�`s.
    ReadWithErrors,
}

/// Something waiting on the "unsaved changes" prompt.
#[derive(Clone, Copy, Debug)]
enum PendingAction {
    CloseTab(TabId),
    /// Quitting: we walk through each tab with unsaved changes in turn.
    Quit,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum HelpPage {
    Shortcuts,
    ReleaseNotes,
}

pub struct SlothApp {
    tabs: Tabs,
    settings: Settings,
    find: FindState,
    cursor: CursorInfo,
    pending: Option<PendingAction>,
    /// Transient message shown in the status bar (errors, "Saved", …).
    status: Option<String>,
    /// Set once the user has confirmed quitting, so the close goes through.
    allow_close: bool,
    last_title: String,
    /// Character index to scroll into view on the next frame.
    reveal: Option<usize>,
    show_about: bool,
    help: Option<HelpPage>,
    /// Text typed into the Go to Line prompt, while it's open.
    go_to_line: Option<String>,
    confirm_reload: Option<ReloadKind>,
    save_problem: Option<SaveProblem>,
    /// `None` in tests unless a test turns it on.
    recovery: Option<Recovery>,
    /// Documents found from a session that crashed, waiting for the user.
    recovery_prompt: Option<Leftovers>,
    last_recovery_save: f64,
    /// Tabs waiting to be closed (Close Other Tabs etc.), one at a time so
    /// each can ask about unsaved changes.
    close_queue: Vec<TabId>,
    /// "123 words" (or a count for the selection), for the status bar.
    word_counts: Option<String>,
    match_cache: RefCell<MatchCache>,
    /// The all-tabs search results panel is open.
    show_results: bool,
    /// For noticing files changed by other programs.
    last_disk_check: f64,
    was_focused: bool,
    /// `None` in tests, which run without a real menu bar.
    #[cfg(target_os = "macos")]
    native_menu: Option<crate::native_menu::NativeMenu>,
}

impl SlothApp {
    pub fn new(cc: &eframe::CreationContext<'_>, files: Vec<PathBuf>) -> Self {
        let settings = Settings::load(cc.storage);
        #[allow(unused_mut)]
        let mut app = Self::with_settings(&cc.egui_ctx, settings, files);
        #[cfg(target_os = "macos")]
        {
            app.native_menu = Some(crate::native_menu::NativeMenu::install(&cc.egui_ctx));
            crate::macos_open::set_context(&cc.egui_ctx);
        }
        fonts::start_scan(&cc.egui_ctx);
        if let Some(dir) = eframe::storage_dir("Sloth") {
            app.enable_recovery(dir.join("recovery"));
        }
        app
    }

    /// Start writing crash-recovery copies into `dir`, and look for any left
    /// there by a session that didn't quit cleanly.
    pub fn enable_recovery(&mut self, dir: PathBuf) {
        let recovery = Recovery::new(dir);
        self.recovery_prompt = recovery.leftovers();
        self.recovery = Some(recovery);
    }

    /// Build the app without touching the real menu bar or settings file
    /// (shared by `new` and the tests).
    pub fn with_settings(ctx: &egui::Context, settings: Settings, files: Vec<PathBuf>) -> Self {
        // egui's iOS-style rubber-banding lets the text be dragged far past
        // its ends with a trackpad, which feels like the page is floating.
        // macOS already supplies trackpad momentum, so turn it off.
        ctx.all_styles_mut(|style| {
            style.spacing.scroll.kinetic.rubber_band = false;
        });

        let mut app = Self {
            tabs: Tabs::new(),
            settings,
            find: FindState::default(),
            cursor: CursorInfo::default(),
            pending: None,
            status: None,
            allow_close: false,
            last_title: String::new(),
            reveal: None,
            show_about: false,
            help: None,
            go_to_line: None,
            confirm_reload: None,
            save_problem: None,
            recovery: None,
            recovery_prompt: None,
            last_recovery_save: 0.0,
            close_queue: Vec::new(),
            word_counts: None,
            match_cache: RefCell::default(),
            show_results: false,
            last_disk_check: 0.0,
            was_focused: true,
            #[cfg(target_os = "macos")]
            native_menu: None,
        };
        app.settings.theme.apply(ctx);
        app.apply_fonts(ctx);

        // Reopen the tabs from last time, then any files we were asked to open.
        let session = std::mem::take(&mut app.settings.session);
        let mut active = None;
        for (i, path) in session.files.into_iter().enumerate() {
            if path.exists() {
                app.open_path(path);
                if i == session.active {
                    active = Some(app.tabs.active().id);
                }
            }
        }
        if let Some(id) = active {
            app.tabs.activate(id);
        }
        for path in files {
            if path.exists() {
                app.open_path(path);
            } else {
                // `sloth newfile.txt` starts an empty doc that saves there.
                let mut doc = Document::new();
                doc.path = Some(path);
                app.tabs.open(doc);
            }
        }
        app
    }

    // ----- command dispatch -------------------------------------------------

    pub fn execute(&mut self, ctx: &egui::Context, cmd: Command) {
        match cmd {
            Command::New => {
                self.tabs.add(Document::new());
                self.after_tab_switch(ctx);
            }
            Command::Open => self.open_dialog(),
            Command::Save => {
                self.save(self.tabs.active().id);
            }
            Command::SaveAs => {
                self.save_as(self.tabs.active().id);
            }
            Command::CloseTab => self.request_close_tab(self.tabs.active().id),
            Command::Quit => self.request_quit(ctx),
            Command::NextTab => {
                self.tabs.select_next();
                self.after_tab_switch(ctx);
            }
            Command::PreviousTab => {
                self.tabs.select_previous();
                self.after_tab_switch(ctx);
            }

            Command::Cut => self.edit_clipboard(ctx, ViewportCommand::RequestCut),
            Command::Copy => self.edit_clipboard(ctx, ViewportCommand::RequestCopy),
            Command::Paste => self.edit_clipboard(ctx, ViewportCommand::RequestPaste),
            Command::Undo => self.send_editor_key(ctx, Modifiers::COMMAND, Key::Z),
            Command::Redo => {
                self.send_editor_key(ctx, Modifiers::COMMAND | Modifiers::SHIFT, Key::Z)
            }
            Command::SelectAll => self.send_editor_key(ctx, Modifiers::COMMAND, Key::A),

            Command::Find => self.find.open(false, self.selected_text(ctx)),
            Command::FindAndReplace => self.find.open(true, self.selected_text(ctx)),
            Command::FindNext => {
                self.settings.add_search(&self.find.query);
                self.find_step(ctx, Direction::Next);
            }
            Command::FindPrevious => {
                self.settings.add_search(&self.find.query);
                self.find_step(ctx, Direction::Previous);
            }
            Command::FindInTabs => {
                self.find.open(false, self.selected_text(ctx));
                self.show_results = true;
            }
            Command::UseSelectionForFind => {
                if let Some(text) = self.selected_text(ctx) {
                    self.find.query = text;
                    self.find.visible = true;
                    self.find.current = None;
                }
            }

            Command::ToggleLineNumbers => {
                let e = &mut self.settings.editor;
                e.show_line_numbers = !e.show_line_numbers;
            }
            Command::ToggleWordWrap => {
                let e = &mut self.settings.editor;
                e.word_wrap = !e.word_wrap;
            }
            Command::ZoomIn => {
                let e = &mut self.settings.editor;
                e.font_size = (e.font_size + 1.0).min(72.0);
            }
            Command::ZoomOut => {
                let e = &mut self.settings.editor;
                e.font_size = (e.font_size - 1.0).max(6.0);
            }
            Command::ZoomReset => self.settings.editor.font_size = EditorSettings::DEFAULT_FONT_SIZE,
            Command::SetTheme(theme) => {
                self.settings.theme = theme;
                theme.apply(ctx);
            }

            Command::OpenRecent(i) => {
                if let Some(path) = self.settings.recent_files.get(i as usize).cloned() {
                    self.open_path(path);
                }
            }
            Command::ClearRecent => self.settings.recent_files.clear(),
            Command::GoToLine => self.go_to_line = Some(String::new()),
            Command::RevertToSaved => {
                if self.can_revert() {
                    self.confirm_reload = Some(ReloadKind::Revert);
                }
            }
            Command::ReopenWithEncoding(encoding) => {
                if self.tabs.active().doc.path.is_some() {
                    if self.tabs.active().doc.is_dirty() {
                        self.confirm_reload = Some(ReloadKind::WithEncoding(encoding));
                    } else {
                        self.reload_active(ReloadKind::WithEncoding(encoding));
                    }
                }
            }
            Command::ConvertEncoding(encoding) => {
                self.tabs.active_mut().doc.convert_encoding(encoding.encoding());
            }
            Command::SetLineEnding(line_ending) => {
                self.tabs.active_mut().doc.set_line_ending(line_ending);
            }

            Command::Indent | Command::Outdent => {
                let indent = self.settings.editor.indent();
                let tab = self.tabs.active_mut();
                editor::apply_edit(ctx, tab.id, &mut tab.doc.text, |t, s| {
                    if cmd == Command::Indent {
                        editing::indent_lines(t, s, indent)
                    } else {
                        editing::outdent_lines(t, s, indent)
                    }
                });
                self.focus_editor(ctx);
            }
            Command::IndentWithTabs => self.settings.editor.indent_with_spaces = false,
            Command::IndentWithSpaces => self.settings.editor.indent_with_spaces = true,
            Command::SetTabWidth(width) => {
                self.settings.editor.tab_width = width as usize;
                self.apply_fonts(ctx);
            }
            Command::ToggleAutoIndent => {
                let e = &mut self.settings.editor;
                e.auto_indent = !e.auto_indent;
            }
            Command::ToggleHighlightLine => {
                let e = &mut self.settings.editor;
                e.highlight_current_line = !e.highlight_current_line;
            }
            Command::DefaultFont => {
                self.settings.editor.font = None;
                self.apply_fonts(ctx);
            }
            Command::SetFont(i) => {
                if let Some(family) = fonts::families().get(i as usize) {
                    self.settings.editor.font = Some(family.clone());
                    self.apply_fonts(ctx);
                }
            }
            Command::CloseOtherTabs => {
                self.close_queue = self.tabs.others(self.tabs.active().id);
            }
            Command::CloseTabsToRight => {
                self.close_queue = self.tabs.to_the_right_of(self.tabs.active().id);
            }

            Command::About => self.show_about = true,
            Command::Help => self.help = Some(HelpPage::Shortcuts),
            Command::ReleaseNotes => self.help = Some(HelpPage::ReleaseNotes),
        }
    }

    fn is_checked(&self, cmd: Command) -> Option<bool> {
        match cmd {
            Command::ToggleLineNumbers => Some(self.settings.editor.show_line_numbers),
            Command::ToggleWordWrap => Some(self.settings.editor.word_wrap),
            Command::SetTheme(theme) => Some(self.settings.theme == theme),
            Command::ConvertEncoding(e) => {
                Some(e.encoding() == self.tabs.active().doc.format.encoding)
            }
            Command::SetLineEnding(l) => Some(self.tabs.active().doc.format.line_ending == l),
            Command::IndentWithTabs => Some(!self.settings.editor.indent_with_spaces),
            Command::IndentWithSpaces => Some(self.settings.editor.indent_with_spaces),
            Command::SetTabWidth(w) => Some(self.settings.editor.tab_width == w as usize),
            Command::ToggleAutoIndent => Some(self.settings.editor.auto_indent),
            Command::ToggleHighlightLine => Some(self.settings.editor.highlight_current_line),
            Command::DefaultFont => Some(self.settings.editor.font.is_none()),
            Command::SetFont(i) => Some(
                fonts::families().get(i as usize) == self.settings.editor.font.as_ref(),
            ),
            _ => None,
        }
    }

    /// Whether a menu item should be clickable right now.
    fn is_enabled(&self, cmd: Command) -> bool {
        match cmd {
            Command::RevertToSaved => self.can_revert(),
            Command::ReopenWithEncoding(_) => self.tabs.active().doc.path.is_some(),
            Command::CloseOtherTabs => self.tabs.iter().count() > 1,
            Command::CloseTabsToRight => {
                !self.tabs.to_the_right_of(self.tabs.active().id).is_empty()
            }
            Command::ClearRecent => !self.settings.recent_files.is_empty(),
            _ => true,
        }
    }

    fn can_revert(&self) -> bool {
        let doc = &self.tabs.active().doc;
        doc.path.is_some() && doc.is_dirty()
    }

    /// Word count for the status bar: the selection's, or the whole
    /// document's. Skipped for very large files to keep typing fast.
    fn count_words(&self, ctx: &egui::Context) -> Option<String> {
        let tab = self.tabs.active();
        if tab.doc.text.len() > 5_000_000 {
            return None;
        }
        let selection = editor::selection(ctx, tab.id).filter(|s| !s.is_empty());
        let plural = |n: usize, one: &str| if n == 1 { format!("1 {one}") } else { format!("{n} {one}s") };
        Some(match selection {
            Some(sel) => {
                let text = &tab.doc.text;
                let range = find::char_to_byte(text, sel.start)..find::char_to_byte(text, sel.end);
                let (words, chars) = editing::counts(&text[range]);
                format!("{}, {} selected", plural(words, "word"), plural(chars, "character"))
            }
            None => plural(editing::counts(&tab.doc.text).0, "word"),
        })
    }

    fn apply_fonts(&self, ctx: &egui::Context) {
        let e = &self.settings.editor;
        fonts::apply(ctx, e.font.as_deref(), e.tab_width);
    }

    /// Close queued tabs, stopping to ask about any with unsaved changes.
    fn process_close_queue(&mut self) {
        while self.pending.is_none() && !self.close_queue.is_empty() {
            let id = self.close_queue.remove(0);
            self.request_close_tab(id);
        }
    }

    /// A dialog is up, so menu commands and shortcuts should wait.
    fn modal_open(&self) -> bool {
        self.pending.is_some()
            || self.show_about
            || self.confirm_reload.is_some()
            || self.go_to_line.is_some()
            || self.save_problem.is_some()
            || self.recovery_prompt.is_some()
    }

    fn focus_editor(&self, ctx: &egui::Context) {
        let id = editor::editor_id(self.tabs.active().id);
        ctx.memory_mut(|m| m.request_focus(id));
    }

    fn after_tab_switch(&mut self, ctx: &egui::Context) {
        self.find.current = None;
        self.focus_editor(ctx);
    }

    /// Menu-driven Cut/Copy/Paste: ask the windowing layer to perform them,
    /// exactly as if the user had pressed the keys.
    fn edit_clipboard(&self, ctx: &egui::Context, request: ViewportCommand) {
        self.focus_editor(ctx);
        ctx.send_viewport_cmd(request);
    }

    /// Menu-driven Undo/Redo/Select All: feed the editor the same key press
    /// it would get from the keyboard. Must run before the editor is drawn.
    fn send_editor_key(&self, ctx: &egui::Context, modifiers: Modifiers, key: Key) {
        self.focus_editor(ctx);
        ctx.input_mut(|i| {
            i.events.push(egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            })
        });
    }

    // ----- closing tabs and quitting ---------------------------------------

    fn request_close_tab(&mut self, id: TabId) {
        match self.tabs.get(id) {
            Some(tab) if tab.doc.is_dirty() => {
                self.tabs.activate(id);
                self.pending = Some(PendingAction::CloseTab(id));
            }
            Some(_) => self.tabs.close(id),
            None => {}
        }
    }

    /// Ask about each tab with unsaved changes, then quit.
    fn request_quit(&mut self, ctx: &egui::Context) {
        if let Some(id) = self.tabs.first_unsaved() {
            self.tabs.activate(id);
            self.pending = Some(PendingAction::Quit);
        } else {
            // A clean quit: crash-recovery copies are no longer needed.
            if let Some(recovery) = &mut self.recovery {
                recovery.clear();
            }
            self.allow_close = true;
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }
    }

    fn unsaved_changes_dialog(&mut self, ctx: &egui::Context) {
        let Some(action) = self.pending else {
            return;
        };
        let tab_id = match action {
            PendingAction::CloseTab(id) => id,
            PendingAction::Quit => self.tabs.active().id,
        };
        let Some(tab) = self.tabs.get(tab_id) else {
            self.pending = None;
            return;
        };
        let name = tab.doc.display_name();

        let mut choice = None;
        egui::Modal::new(egui::Id::unique("unsaved-changes")).show(ctx, |ui| {
            ui.set_width(340.0);
            ui.heading(format!("Save changes to \u{201c}{name}\u{201d}?"));
            ui.label("Your changes will be lost if you don't save them.");
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    choice = Some(DialogChoice::Save);
                }
                if ui.button("Don't Save").clicked() {
                    choice = Some(DialogChoice::Discard);
                }
                if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                    choice = Some(DialogChoice::Cancel);
                }
            });
        });

        let Some(choice) = choice else {
            return;
        };
        self.pending = None;
        match (choice, action) {
            (DialogChoice::Cancel, _) => {
                for tab in self.tabs.iter_mut() {
                    tab.discard_on_quit = false;
                }
                self.close_queue.clear();
            }
            (DialogChoice::Save, PendingAction::CloseTab(id)) => {
                if self.save(id) {
                    self.tabs.close(id);
                }
            }
            (DialogChoice::Discard, PendingAction::CloseTab(id)) => self.tabs.close(id),
            (DialogChoice::Save, PendingAction::Quit) => {
                if self.save(tab_id) {
                    self.request_quit(ctx);
                }
            }
            (DialogChoice::Discard, PendingAction::Quit) => {
                if let Some(tab) = self.tabs.get_mut(tab_id) {
                    tab.discard_on_quit = true;
                }
                self.request_quit(ctx);
            }
        }
    }

    // ----- file operations --------------------------------------------------

    fn open_dialog(&mut self) {
        let mut dlg = rfd::FileDialog::new();
        if let Some(dir) = self.current_dir() {
            dlg = dlg.set_directory(dir);
        }
        for path in dlg.pick_files().unwrap_or_default() {
            self.open_path(path);
        }
    }

    /// Open a file in a tab, or switch to it if it's already open.
    fn open_path(&mut self, path: PathBuf) {
        if let Some(id) = self.tabs.find_by_path(&path) {
            self.tabs.activate(id);
            return;
        }
        match Document::open(&path) {
            Ok(doc) => {
                self.status = doc.read_with_errors.then(|| {
                    format!(
                        "Some of {} isn't valid {}; try File \u{2192} Reopen with Encoding",
                        doc.display_name(),
                        doc.format.encoding.name()
                    )
                });
                self.tabs.open(doc);
                self.settings.add_recent(&path);
                self.find.current = None;
            }
            Err(e) => {
                if !path.exists() {
                    self.settings.remove_recent(&path);
                }
                self.status = Some(format!("Could not open {}: {e}", path.display()));
            }
        }
    }

    /// Returns true if the document was saved.
    fn save(&mut self, id: TabId) -> bool {
        let Some(tab) = self.tabs.get_mut(id) else {
            return false;
        };
        if tab.doc.path.is_none() {
            return self.save_as(id);
        }
        let result = tab.doc.save();
        if result.is_ok() {
            tab.disk_notice = None;
        }
        self.report_save(id, None, result)
    }

    fn save_as(&mut self, id: TabId) -> bool {
        let dir = self.current_dir();
        let Some(tab) = self.tabs.get_mut(id) else {
            return false;
        };
        let current = tab.doc.path.as_deref();
        let mut dlg =
            rfd::FileDialog::new().set_file_name(file_types::suggested_name(current));
        for format in file_types::save_formats(current) {
            dlg = dlg.add_filter(format.name, format.extensions);
        }
        if let Some(dir) = dir {
            dlg = dlg.set_directory(dir);
        }
        let Some(path) = dlg.save_file() else {
            return false;
        };
        // macOS adds the selected format's extension itself; elsewhere, make
        // sure a name typed without one still gets the default.
        #[cfg(not(target_os = "macos"))]
        let path = file_types::with_default_extension(path);

        let result = tab.doc.save_as(&path);
        if result.is_ok() {
            tab.disk_notice = None;
            self.settings.add_recent(&path);
        }
        self.report_save(id, Some(path), result)
    }

    /// Show the outcome of a save. Problems that need a decision open a
    /// dialog. Returns true if the document was saved.
    fn report_save(&mut self, tab: TabId, path: Option<PathBuf>, result: Result<(), SaveError>) -> bool {
        let kind = match result {
            Ok(()) => {
                self.status = Some("Saved".into());
                return true;
            }
            Err(SaveError::Unmappable(encoding)) => SaveProblemKind::Unmappable(encoding),
            Err(SaveError::ReadWithErrors) => SaveProblemKind::ReadWithErrors,
            Err(e) => {
                self.status = Some(format!("Save failed: {e}"));
                return false;
            }
        };
        self.tabs.activate(tab);
        self.save_problem = Some(SaveProblem { tab, path, kind });
        false
    }

    fn save_problem_dialog(&mut self, ctx: &egui::Context) {
        let Some(problem) = &self.save_problem else {
            return;
        };
        let Some(tab) = self.tabs.get(problem.tab) else {
            self.save_problem = None;
            return;
        };
        let name = tab.doc.display_name();
        let encoding = tab.doc.format.encoding.name();
        let mut proceed = None;
        egui::Modal::new(egui::Id::unique("save-problem")).show(ctx, |ui| {
            ui.set_width(380.0);
            match problem.kind {
                SaveProblemKind::Unmappable(enc) => {
                    ui.heading(format!("Save \u{201c}{name}\u{201d} as UTF-8?"));
                    ui.label(format!(
                        "Some characters can't be stored in {}. Saving as UTF-8 keeps them all.",
                        enc.name()
                    ));
                }
                SaveProblemKind::ReadWithErrors => {
                    ui.heading(format!("Save \u{201c}{name}\u{201d} anyway?"));
                    ui.label(format!(
                        "Parts of this file couldn't be read as {encoding} and show as \u{fffd}. \
                         Saving will write them that way. To read it differently, cancel and \
                         use File \u{2192} Reopen with Encoding."
                    ));
                }
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let label = match problem.kind {
                    SaveProblemKind::Unmappable(_) => "Save as UTF-8",
                    SaveProblemKind::ReadWithErrors => "Save Anyway",
                };
                if ui.button(label).clicked() {
                    proceed = Some(true);
                }
                if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                    proceed = Some(false);
                }
            });
        });

        let Some(proceed) = proceed else {
            return;
        };
        let problem = self.save_problem.take().expect("checked above");
        if !proceed {
            return;
        }
        let Some(tab) = self.tabs.get_mut(problem.tab) else {
            return;
        };
        match problem.kind {
            SaveProblemKind::Unmappable(_) => tab.doc.convert_encoding(encoding_rs::UTF_8),
            SaveProblemKind::ReadWithErrors => tab.doc.accept_read_errors(),
        }
        let result = match &problem.path {
            Some(path) => tab.doc.save_as(path),
            None => tab.doc.save(),
        };
        if result.is_ok() {
            tab.disk_notice = None;
        }
        self.report_save(problem.tab, problem.path, result);
    }

    // ----- crash recovery ---------------------------------------------------

    /// Write recovery copies of unsaved documents; drop the rest.
    fn save_recovery_copies(&mut self) {
        let Some(recovery) = &mut self.recovery else {
            return;
        };
        for tab in self.tabs.iter() {
            if tab.doc.is_dirty() {
                if let Err(e) = recovery.save(tab.id, &tab.doc) {
                    self.status = Some(format!("Couldn't write recovery copy: {e}"));
                }
            } else {
                recovery.remove(tab.id);
            }
        }
        let tabs = &self.tabs;
        recovery.retain(|id| tabs.get(id).is_some());
    }

    fn recovery_dialog(&mut self, ctx: &egui::Context) {
        let Some(leftovers) = &self.recovery_prompt else {
            return;
        };
        let names: Vec<String> = leftovers.records.iter().map(|r| r.display_name()).collect();
        let mut restore = None;
        egui::Modal::new(egui::Id::unique("recover")).show(ctx, |ui| {
            ui.set_width(380.0);
            ui.heading("Restore unsaved documents?");
            ui.label("Sloth didn't quit properly last time. These documents had unsaved changes:");
            ui.add_space(4.0);
            for name in &names {
                ui.label(format!("\u{2022}  {name}"));
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Restore").clicked() {
                    restore = Some(true);
                }
                if ui.button("Discard").clicked() {
                    restore = Some(false);
                }
            });
        });

        let Some(restore) = restore else {
            return;
        };
        let leftovers = self.recovery_prompt.take().expect("checked above");
        if restore {
            for record in leftovers.records.clone() {
                // If that file is already open, put the recovered text there.
                let existing = record.path.as_deref().and_then(|p| self.tabs.find_by_path(p));
                match existing {
                    Some(id) => {
                        if let Some(tab) = self.tabs.get_mut(id) {
                            tab.doc.text = record.text;
                        }
                        self.tabs.activate(id);
                    }
                    None => {
                        self.tabs.open(record.into_document());
                    }
                }
            }
            self.status = Some("Restored unsaved documents".into());
            // Write them into this session's folder before deleting the old one.
            self.save_recovery_copies();
        }
        leftovers.remove();
    }

    fn current_dir(&self) -> Option<PathBuf> {
        self.tabs
            .active()
            .doc
            .path
            .as_ref()
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf())
    }

    // ----- session, outside changes, revert, go to line ---------------------

    /// The open files, to reopen next launch.
    fn session(&self) -> settings::Session {
        let active_id = self.tabs.active().id;
        let mut session = settings::Session::default();
        for tab in self.tabs.iter() {
            if let Some(path) = &tab.doc.path {
                if tab.id == active_id {
                    session.active = session.files.len();
                }
                session.files.push(path.clone());
            }
        }
        session
    }

    /// Look for open files that other programs changed or removed.
    fn check_disk(&mut self) {
        let mut reloaded = Vec::new();
        for tab in self.tabs.iter_mut() {
            match tab.doc.disk_state() {
                DiskState::Unchanged => {}
                DiskState::Changed if !tab.doc.is_dirty() => {
                    // Nothing to lose: just pick up the new version.
                    let encoding = Some(tab.doc.format.encoding);
                    if tab.doc.reload_with_encoding(encoding).is_ok() {
                        tab.disk_notice = None;
                        reloaded.push(tab.doc.display_name());
                    }
                }
                DiskState::Changed => tab.disk_notice = Some(DiskNotice::Changed),
                DiskState::Missing => tab.disk_notice = Some(DiskNotice::Missing),
            }
        }
        if !reloaded.is_empty() {
            self.find.current = None;
            self.status = Some(format!("Reloaded {} (changed on disk)", reloaded.join(", ")));
        }
    }

    fn reload_active(&mut self, kind: ReloadKind) {
        let tab = self.tabs.active_mut();
        let encoding = match kind {
            // Keep reading it the way it's being read now.
            ReloadKind::Revert => tab.doc.format.encoding,
            ReloadKind::WithEncoding(e) => e.encoding(),
        };
        match tab.doc.reload_with_encoding(Some(encoding)) {
            Ok(()) => {
                tab.disk_notice = None;
                self.find.current = None;
                let name = tab.doc.display_name();
                self.status = Some(match kind {
                    ReloadKind::Revert => format!("Reverted {name}"),
                    ReloadKind::WithEncoding(e) if tab.doc.read_with_errors => {
                        format!("{name} isn't valid {} either", e.short_name())
                    }
                    ReloadKind::WithEncoding(e) => format!("Reopened {name} as {}", e.short_name()),
                });
            }
            Err(e) => self.status = Some(format!("Could not reload: {e}")),
        }
    }

    fn disk_notice_bar(&mut self, ui: &mut egui::Ui) {
        let tab = self.tabs.active();
        let Some(notice) = tab.disk_notice else {
            return;
        };
        let name = tab.doc.display_name();
        let mut reload = false;
        let mut dismiss = false;
        egui::Panel::top("disk-notice")
            .frame(
                egui::Frame::NONE
                    .fill(ui.visuals().warn_fg_color.gamma_multiply(0.15))
                    .inner_margin(Margin::symmetric(10, 6)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| match notice {
                    DiskNotice::Changed => {
                        ui.label(format!(
                            "\u{201c}{name}\u{201d} was changed by another program. \
                             You also have unsaved changes."
                        ));
                        reload = ui.button("Reload from Disk").clicked();
                        dismiss = ui.button("Keep My Version").clicked();
                    }
                    DiskNotice::Missing => {
                        ui.label(format!(
                            "\u{201c}{name}\u{201d} was deleted or moved. Saving will create it again."
                        ));
                        dismiss = ui.button("OK").clicked();
                    }
                });
            });
        if reload {
            self.reload_active(ReloadKind::Revert);
        } else if dismiss {
            let tab = self.tabs.active_mut();
            tab.doc.acknowledge_disk_state();
            tab.disk_notice = None;
        }
    }

    /// Confirm discarding unsaved changes before Revert or Reopen with Encoding.
    fn reload_dialog(&mut self, ctx: &egui::Context) {
        let Some(kind) = self.confirm_reload else {
            return;
        };
        let name = self.tabs.active().doc.display_name();
        let (heading, button) = match kind {
            ReloadKind::Revert => (format!("Revert \u{201c}{name}\u{201d}?"), "Revert".to_owned()),
            ReloadKind::WithEncoding(e) => (
                format!("Reopen \u{201c}{name}\u{201d} as {}?", e.short_name()),
                "Reopen".to_owned(),
            ),
        };
        let mut choice = None;
        egui::Modal::new(egui::Id::unique("reload")).show(ctx, |ui| {
            ui.set_width(340.0);
            ui.heading(heading);
            ui.label("Your unsaved changes will be lost.");
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(button).clicked() {
                    choice = Some(true);
                }
                if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                    choice = Some(false);
                }
            });
        });
        if let Some(go) = choice {
            self.confirm_reload = None;
            if go {
                self.reload_active(kind);
            }
        }
    }

    fn go_to_line_dialog(&mut self, ctx: &egui::Context) {
        let Some(input) = &mut self.go_to_line else {
            return;
        };
        let line_count = self.tabs.active().doc.line_count();
        let mut target = None;
        let mut close = false;
        egui::Modal::new(egui::Id::unique("go-to-line")).show(ctx, |ui| {
            ui.set_width(260.0);
            ui.label("Go to line (or line:column)");
            let field = ui.add(
                egui::TextEdit::singleline(input)
                    .hint_text(format!("1–{line_count}"))
                    .desired_width(f32::INFINITY),
            );
            field.request_focus();
            let parsed = editor::parse_line_col(input);
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let go = ui.add_enabled(parsed.is_some(), egui::Button::new("Go")).clicked();
                if go || (parsed.is_some() && ui.input(|i| i.key_pressed(Key::Enter))) {
                    target = parsed;
                }
                if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                    close = true;
                }
            });
        });

        if let Some((line, col)) = target {
            let tab = self.tabs.active();
            let index = editor::char_index_of(&tab.doc.text, line, col);
            editor::set_selection(ctx, tab.id, index..index);
            self.reveal = Some(index);
            self.focus_editor(ctx);
            close = true;
        }
        if close {
            self.go_to_line = None;
        }
    }

    // ----- find & replace ---------------------------------------------------

    /// Matches in the active tab (empty when not searching or the pattern
    /// is invalid).
    fn matches(&self) -> Arc<Vec<Range<usize>>> {
        if !self.find.is_searching() {
            return Arc::default();
        }
        self.search_tab(self.tabs.active()).unwrap_or_default()
    }

    /// Why the current search can't run, if it can't (an invalid regex).
    fn find_error(&self) -> Option<String> {
        if !self.find.is_searching() {
            return None;
        }
        self.search_tab(self.tabs.active()).err()
    }

    /// Search one tab, reusing the last result if nothing has changed.
    fn search_tab(&self, tab: &Tab) -> Result<Arc<Vec<Range<usize>>>, String> {
        let mut cache = self.match_cache.borrow_mut();
        if cache.query != self.find.query || cache.options != self.find.options {
            *cache = MatchCache {
                query: self.find.query.clone(),
                options: self.find.options,
                tabs: HashMap::new(),
            };
        }
        if let Some(hit) = cache.tabs.get(&tab.id)
            && hit.text == tab.doc.text
        {
            return hit.matches.clone();
        }
        let matches = find::find_all(&tab.doc.text, &self.find.query, self.find.options).map(Arc::new);
        cache.tabs.insert(
            tab.id,
            CachedSearch { text: tab.doc.text.clone(), matches: matches.clone(), rows: None },
        );
        matches
    }

    /// Result rows for the all-tabs panel.
    fn result_rows(&self, tab: &Tab) -> Arc<Vec<ResultRow>> {
        let Ok(matches) = self.search_tab(tab) else {
            return Arc::default();
        };
        let mut cache = self.match_cache.borrow_mut();
        let entry = cache.tabs.get_mut(&tab.id).expect("just searched");
        entry
            .rows
            .get_or_insert_with(|| Arc::new(search_results::build_rows(&tab.doc.text, &matches)))
            .clone()
    }

    fn results_panel(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        if !self.show_results || !self.find.is_searching() {
            return;
        }
        let rows: Vec<(TabId, String, Arc<Vec<ResultRow>>)> = self
            .tabs
            .iter()
            .map(|t| (t.id, t.doc.display_name(), self.result_rows(t)))
            .collect();
        let results: Vec<TabResults<'_>> = rows
            .iter()
            .map(|(id, name, rows)| TabResults { tab: *id, name: name.clone(), rows })
            .collect();
        let error = self.find_error();
        let action = egui::Panel::bottom("search-results")
            .resizable(true)
            .default_size(200.0)
            .show(ui, |ui| search_results::results_panel(ui, &results, error.as_deref()))
            .inner;
        match action {
            Some(ResultsAction::Jump(tab, range)) => {
                self.tabs.activate(tab);
                self.select_match(ctx, range);
                self.focus_editor(ctx);
            }
            Some(ResultsAction::Close) => self.show_results = false,
            None => {}
        }
        // Forget closed tabs.
        let tabs = &self.tabs;
        self.match_cache.borrow_mut().tabs.retain(|id, _| tabs.get(*id).is_some());
    }

    /// The editor's selection as a byte range.
    fn selection_bytes(&self, ctx: &egui::Context) -> Range<usize> {
        let tab = self.tabs.active();
        let text = &tab.doc.text;
        editor::selection(ctx, tab.id)
            .map(|r| find::char_to_byte(text, r.start)..find::char_to_byte(text, r.end))
            .unwrap_or(0..0)
    }

    /// Selected text, if it's a short single line (used to pre-fill Find).
    fn selected_text(&self, ctx: &egui::Context) -> Option<String> {
        let range = self.selection_bytes(ctx);
        let text = self.tabs.active().doc.text.get(range)?;
        (!text.is_empty() && !text.contains('\n') && text.len() < 200).then(|| text.to_owned())
    }

    fn find_step(&mut self, ctx: &egui::Context, direction: Direction) {
        if self.find.query.is_empty() {
            self.find.open(false, self.selected_text(ctx));
            return;
        }
        self.find.visible = true;
        let matches = self.matches();
        if matches.is_empty() {
            self.find.current = None;
            return;
        }
        // Search relative to the current match, or else the cursor/selection.
        let anchor = self
            .find
            .current
            .clone()
            .filter(|c| matches.contains(c))
            .unwrap_or_else(|| self.selection_bytes(ctx));
        let index = match direction {
            // Typing in the search box: stay on a match at the same spot.
            Direction::FromCursor => matches.iter().position(|m| m.start >= anchor.start),
            Direction::Next => matches.iter().position(|m| m.start >= anchor.end && *m != anchor),
            Direction::Previous => matches.iter().rposition(|m| m.start < anchor.start),
        };
        // Wrap around the ends of the document.
        let index = index.unwrap_or(match direction {
            Direction::Previous => matches.len() - 1,
            _ => 0,
        });
        self.select_match(ctx, matches[index].clone());
    }

    fn select_match(&mut self, ctx: &egui::Context, range: Range<usize>) {
        let tab = self.tabs.active();
        let start = find::byte_to_char(&tab.doc.text, range.start);
        let end = find::byte_to_char(&tab.doc.text, range.end);
        editor::set_selection(ctx, tab.id, start..end);
        self.reveal = Some(start);
        self.find.current = Some(range);
    }

    fn replace_current(&mut self, ctx: &egui::Context) {
        let matches = self.matches();
        let Some(current) = self.find.current.clone().filter(|c| matches.contains(c)) else {
            // Nothing selected yet: the first press just finds.
            self.find_step(ctx, Direction::Next);
            return;
        };
        let replacement = match find::replacement_for(
            &self.tabs.active().doc.text,
            current.clone(),
            &self.find.query,
            &self.find.replacement,
            self.find.options,
        ) {
            Ok(r) => r,
            Err(e) => {
                self.status = Some(format!("Can't replace: {e}"));
                return;
            }
        };
        let tab = self.tabs.active_mut();
        tab.doc.text.replace_range(current.clone(), &replacement);
        let after = current.start + replacement.len();
        let after_char = find::byte_to_char(&tab.doc.text, after);
        let id = tab.id;
        editor::set_selection(ctx, id, after_char..after_char);
        self.find.current = None;
        self.find_step(ctx, Direction::Next);
    }

    fn replace_all(&mut self, ctx: &egui::Context) {
        let tab = self.tabs.active_mut();
        let (text, count) = match find::replace_all(
            &tab.doc.text,
            &self.find.query,
            &self.find.replacement,
            self.find.options,
        ) {
            Ok(result) => result,
            Err(e) => {
                self.status = Some(format!("Can't replace: {e}"));
                return;
            }
        };
        tab.doc.text = text;
        let id = tab.id;
        editor::set_selection(ctx, id, 0..0);
        self.find.current = None;
        self.status = Some(match count {
            1 => "Replaced 1 occurrence".to_owned(),
            n => format!("Replaced {n} occurrences"),
        });
    }

    fn handle_find_action(&mut self, ctx: &egui::Context, action: FindAction) {
        match action {
            FindAction::Search => {
                if self.find.query.is_empty() {
                    self.find.current = None;
                } else {
                    self.find_step(ctx, Direction::FromCursor);
                }
            }
            FindAction::Next => {
                self.settings.add_search(&self.find.query);
                self.find_step(ctx, Direction::Next);
            }
            FindAction::Previous => {
                self.settings.add_search(&self.find.query);
                self.find_step(ctx, Direction::Previous);
            }
            FindAction::Replace => {
                self.settings.add_search(&self.find.query);
                self.replace_current(ctx);
            }
            FindAction::ReplaceAll => {
                self.settings.add_search(&self.find.query);
                self.replace_all(ctx);
            }
            FindAction::ToggleResults => self.show_results = !self.show_results,
            FindAction::Close => {
                self.find.close();
                self.show_results = false;
                self.focus_editor(ctx);
            }
        }
    }

    // ----- UI pieces --------------------------------------------------------

    #[cfg_attr(target_os = "macos", allow(dead_code))] // used where there is no native menu
    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        // Check shortcuts with more modifiers first so ⌘⇧S isn't eaten by ⌘S.
        let mut cmds: Vec<_> = commands::all()
            .into_iter()
            .filter(|c| !c.is_handled_by_text_widget())
            .filter_map(|c| c.shortcut().map(|s| (c, s)))
            .collect();
        cmds.sort_by_key(|(_, s)| std::cmp::Reverse(modifier_count(s.modifiers)));

        let triggered: Vec<Command> = ctx.input_mut(|i| {
            cmds.iter()
                .filter(|(_, s)| i.consume_shortcut(s))
                .map(|(c, _)| *c)
                .collect()
        });
        for cmd in triggered {
            self.execute(ctx, cmd);
        }
    }

    #[cfg_attr(target_os = "macos", allow(dead_code))] // used where there is no native menu
    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        egui::MenuBar::new().ui(ui, |ui| {
            for (title, entries) in commands::MENUS {
                ui.menu_button(*title, |ui| self.menu_entries(ui, &ctx, entries));
            }
        });
    }

    #[cfg_attr(target_os = "macos", allow(dead_code))] // used where there is no native menu
    fn menu_entries(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, entries: &[MenuEntry]) {
        for entry in entries {
            match entry {
                MenuEntry::Separator => {
                    ui.separator();
                }
                MenuEntry::Submenu(title, children) => {
                    ui.menu_button(*title, |ui| self.menu_entries(ui, ctx, children));
                }
                MenuEntry::Generated(list) => {
                    let generated: Vec<MenuEntry> = list().into_iter().map(MenuEntry::Item).collect();
                    self.menu_entries(ui, ctx, &generated);
                }
                MenuEntry::Fonts => {
                    let count = fonts::families().len() as u16;
                    let fonts: Vec<MenuEntry> = std::iter::once(Command::DefaultFont)
                        .chain((0..count).map(Command::SetFont))
                        .map(MenuEntry::Item)
                        .collect();
                    self.menu_entries(ui, ctx, &fonts);
                }
                MenuEntry::RecentFiles => {
                    ui.menu_button("Open Recent", |ui| {
                        let labels = settings::recent_labels(&self.settings.recent_files);
                        for (i, label) in labels.into_iter().enumerate() {
                            if ui.button(label).clicked() {
                                self.execute(ctx, Command::OpenRecent(i as u8));
                                ui.close();
                            }
                        }
                        ui.separator();
                        let clear = egui::Button::new(Command::ClearRecent.label());
                        if ui.add_enabled(self.is_enabled(Command::ClearRecent), clear).clicked() {
                            self.execute(ctx, Command::ClearRecent);
                            ui.close();
                        }
                    });
                }
                MenuEntry::Item(cmd) => {
                    let cmd = *cmd;
                    let mut label = cmd.label().to_owned();
                    if let Some(true) = self.is_checked(cmd) {
                        label = format!("✔ {label}");
                    }
                    let mut button = egui::Button::new(label);
                    if let Some(sc) = cmd.shortcut() {
                        button = button.shortcut_text(ctx.format_shortcut(&sc));
                    }
                    if ui.add_enabled(self.is_enabled(cmd), button).clicked() {
                        self.execute(ctx, cmd);
                        ui.close();
                    }
                }
            }
        }
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        let doc = &self.tabs.active().doc;
        // Right-hand details first, so the path on the left gets whatever
        // room is left and is cut short (…) rather than overlapping them.
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(doc.format.summary())
                .on_hover_text("Encoding and line endings: change them in the File menu");
            ui.separator();
            ui.label(format!("{} lines", doc.line_count()));
            if let Some(counts) = &self.word_counts {
                ui.separator();
                ui.label(counts);
            }
            if let Some((line, col)) = self.cursor.line_col {
                ui.separator();
                ui.label(format!("Ln {line}, Col {col}"));
            }
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                if let Some(msg) = &self.status {
                    ui.add(egui::Label::new(msg.as_str()).truncate());
                    ui.separator();
                }
                let path = doc
                    .path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "Untitled".into());
                ui.add(egui::Label::new(RichText::new(path.clone()).weak()).truncate())
                    .on_hover_text(path);
            });
        });
    }

    fn about_window(&mut self, ctx: &egui::Context) {
        if !self.show_about {
            return;
        }
        let response = egui::Modal::new(egui::Id::unique("about")).show(ctx, |ui| {
            ui.set_width(300.0);
            ui.vertical_centered(|ui| {
                ui.add_space(6.0);
                ui.label(RichText::new("Sloth").size(28.0).strong());
                ui.label(format!("Version {}", version::VERSION));
                ui.label(RichText::new(version::build_info()).weak().small());
                ui.add_space(8.0);
                ui.label("A simple plain-text editor for macOS.");
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button("Release Notes").clicked() {
                        self.help = Some(HelpPage::ReleaseNotes);
                        self.show_about = false;
                    }
                    if ui.button("OK").clicked() || ui.input(|i| i.key_pressed(Key::Enter)) {
                        self.show_about = false;
                    }
                });
            });
        });
        if response.should_close() {
            self.show_about = false;
        }
    }

    fn help_window(&mut self, ctx: &egui::Context) {
        let Some(mut page) = self.help else {
            return;
        };
        let mut open = true;
        egui::Window::new("Sloth Help")
            .open(&mut open)
            .collapsible(false)
            .default_size([460.0, 520.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut page, HelpPage::Shortcuts, "Keyboard Shortcuts");
                    ui.selectable_value(&mut page, HelpPage::ReleaseNotes, "Release Notes");
                });
                ui.separator();
                ScrollArea::vertical().show(ui, |ui| match page {
                    HelpPage::Shortcuts => help::shortcuts_page(ui),
                    HelpPage::ReleaseNotes => help::release_notes_page(ui),
                });
            });
        self.help = open.then_some(page);
    }

    fn update_title(&mut self, ctx: &egui::Context) {
        let doc = &self.tabs.active().doc;
        let title = format!(
            "{}{} — Sloth",
            if doc.is_dirty() { "• " } else { "" },
            doc.display_name()
        );
        if title != self.last_title {
            ctx.send_viewport_cmd(ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }
    }
}

#[derive(Clone, Copy)]
enum Direction {
    FromCursor,
    Next,
    Previous,
}

enum DialogChoice {
    Save,
    Discard,
    Cancel,
}

#[cfg_attr(target_os = "macos", allow(dead_code))] // used where there is no native menu
fn modifier_count(m: egui::Modifiers) -> u8 {
    m.alt as u8 + m.ctrl as u8 + m.shift as u8 + m.mac_cmd as u8 + m.command as u8
}

impl eframe::App for SlothApp {
    /// Don't save egui's internal widget state (cursor positions and so on)
    /// between launches: it would be applied to whatever tabs happen to get
    /// the same ids next time. Our own settings are saved in `save` below.
    fn persist_egui_memory(&self) -> bool {
        false
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.settings.session = self.session();
        self.settings.save(storage);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // Window close button: intercept if there are unsaved changes.
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            if self.pending.is_none() {
                self.request_quit(&ctx);
            }
        }

        // Files dragged onto the window.
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .filter(|p| !p.as_os_str().is_empty())
                .collect()
        });
        #[cfg(target_os = "macos")]
        let dropped = [dropped, crate::macos_open::take_pending()].concat(); // + opened from Finder
        for path in dropped {
            self.open_path(path);
        }

        // Check for outside changes when the window regains focus, and every
        // couple of seconds while it has focus.
        let focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
        let now = ctx.input(|i| i.time);
        if focused
            && (!self.was_focused || now - self.last_disk_check >= DISK_CHECK_INTERVAL.as_secs_f64())
        {
            self.check_disk();
            self.last_disk_check = now;
        }
        // Recovery copies: every few seconds, and when the window loses focus.
        if (self.was_focused && !focused) || now - self.last_recovery_save >= RECOVERY_INTERVAL {
            self.save_recovery_copies();
            self.last_recovery_save = now;
        }
        self.was_focused = focused;
        if focused {
            ctx.request_repaint_after(DISK_CHECK_INTERVAL);
        }

        // On macOS, shortcuts are routed through the native menu bar.
        #[cfg(target_os = "macos")]
        let menu_commands = self.native_menu.as_ref().map(|m| m.drain()).unwrap_or_default();
        #[cfg(target_os = "macos")]
        for cmd in menu_commands {
            if !self.modal_open() {
                self.execute(&ctx, cmd);
            }
        }
        #[cfg(not(target_os = "macos"))]
        if !self.modal_open() {
            self.handle_shortcuts(&ctx);
        }

        if self.find.visible && !self.modal_open() && ctx.input(|i| i.key_pressed(Key::Escape))
        {
            self.handle_find_action(&ctx, FindAction::Close);
        }

        #[cfg(not(target_os = "macos"))]
        egui::Panel::top("menu").show(ui, |ui| self.menu_bar(ui));

        // Tab bar.
        let tab_action = egui::Panel::top("tabs")
            .frame(
                egui::Frame::NONE
                    .fill(ui.visuals().panel_fill)
                    .inner_margin(Margin { left: 6, right: 6, top: 4, bottom: 0 }),
            )
            .show(ui, |ui| tabs::tab_bar(ui, &self.tabs))
            .inner;
        match tab_action {
            Some(TabBarAction::Activate(id)) => {
                self.tabs.activate(id);
                self.after_tab_switch(&ctx);
            }
            Some(TabBarAction::Close(id)) => self.request_close_tab(id),
            Some(TabBarAction::CloseOthers(id)) => self.close_queue = self.tabs.others(id),
            Some(TabBarAction::CloseToTheRight(id)) => {
                self.close_queue = self.tabs.to_the_right_of(id);
            }
            Some(TabBarAction::Move { from, to }) => self.tabs.move_tab(from, to),
            None => {}
        }

        // Find bar.
        if self.find.visible {
            let matches = self.matches();
            let current_index = self
                .find
                .current
                .as_ref()
                .and_then(|c| matches.iter().position(|m| m == c));
            let error = self.find_error();
            let info = FindBarInfo {
                match_count: matches.len(),
                current_index,
                error: error.as_deref(),
                history: &self.settings.search_history,
                results_open: self.show_results,
            };
            let action = egui::Panel::top("find")
                .show(ui, |ui| find::find_bar(ui, &mut self.find, info))
                .inner;
            if let Some(action) = action {
                self.handle_find_action(&ctx, action);
            }
        }

        self.disk_notice_bar(ui);
        self.word_counts = self.count_words(&ctx);
        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));
        self.results_panel(&ctx, ui);

        // Computed after the find bar, which may have changed the text or query.
        let matches = self.matches();
        let highlights = Highlights {
            all: &matches,
            current: self.find.current.clone(),
        };
        let reveal = self.reveal.take();
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(ui.visuals().extreme_bg_color))
            .show(ui, |ui| {
                let tab = self.tabs.active_mut();
                self.cursor =
                    editor::show(ui, tab.id, &mut tab.doc.text, &self.settings.editor, highlights, reveal);
            });

        self.process_close_queue();
        self.unsaved_changes_dialog(&ctx);
        self.reload_dialog(&ctx);
        self.save_problem_dialog(&ctx);
        self.recovery_dialog(&ctx);
        self.go_to_line_dialog(&ctx);
        self.about_window(&ctx);
        self.help_window(&ctx);
        self.update_title(&ctx);

        #[cfg(target_os = "macos")]
        if let Some(mut menu) = self.native_menu.take() {
            for cmd in commands::all() {
                if let Some(checked) = self.is_checked(cmd) {
                    menu.set_checked(cmd, checked);
                }
                menu.set_enabled(cmd, self.is_enabled(cmd));
            }
            menu.set_recent_files(&self.settings.recent_files);
            menu.set_fonts(fonts::families());
            self.native_menu = Some(menu);
        }
    }
}

#[cfg(test)]
#[path = "app_ui_tests.rs"]
mod ui_tests;
