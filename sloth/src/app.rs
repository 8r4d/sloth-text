//! Top-level application state and UI wiring.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use eframe::egui::{
    self, Align, Key, Layout, Margin, Modifiers, RichText, ScrollArea, ViewportCommand, ViewportId,
};

use crate::commands::{self, Command, MenuEntry};
use crate::document::{Document, SaveError};
use crate::editing;
use crate::fonts;
use crate::editor::{self, CursorInfo, EditorSettings, Highlights, SyntaxInput};
use crate::syntax;
use crate::file_types;
use crate::find::{self, FindAction, FindBarInfo, FindOptions, FindState};
use crate::search_results::{self, ResultRow, ResultsAction, ResultsWording, TabResults};
use crate::help;
use crate::recovery::{Leftovers, Recovery};
use crate::settings::{self, Settings};
use crate::tabs::{self, NoticeChoice, Tab, TabBarAction, TabId, Tabs};
use crate::text_format::TextEncoding;
use crate::version;

/// How often to check open files for changes made by other programs.
const DISK_CHECK_INTERVAL: Duration = Duration::from_secs(2);
/// Word counts are skipped above this size, and refreshed about once a
/// second (rather than every frame) above the smaller one.
const WORD_COUNT_MAX_BYTES: usize = 20_000_000;
const WORD_COUNT_EVERY_FRAME_BYTES: usize = 200_000;

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

/// Everything that belongs to one window.
struct WindowState {
    viewport: ViewportId,
    /// Where and how big a new window opens.
    builder: egui::ViewportBuilder,
    tabs: Tabs,
    find: FindState,
    cursor: CursorInfo,
    pending: Option<PendingAction>,
    /// Transient message shown in the status bar (errors, "Saved", …).
    status: Option<String>,
    last_title: String,
    /// Character index to scroll into view on the next frame.
    reveal: Option<usize>,
    show_about: bool,
    show_settings: bool,
    help: Option<HelpPage>,
    /// Text typed into the Go to Line prompt, while it's open.
    go_to_line: Option<String>,
    confirm_reload: Option<ReloadKind>,
    save_problem: Option<SaveProblem>,
    /// Tabs waiting to be closed (Close Other Tabs etc.), one at a time so
    /// each can ask about unsaved changes.
    close_queue: Vec<TabId>,
    /// "123 words" (or a count for the selection), for the status bar.
    word_counts: Option<String>,
    /// What the word count was for (tab and selection), and when.
    word_counts_key: Option<(TabId, Option<Range<usize>>)>,
    word_counts_at: f64,
    match_cache: RefCell<MatchCache>,
    /// The all-tabs search results panel is open.
    show_results: bool,
    /// The user asked to close this window; it goes once its tabs have closed.
    closing: bool,
    /// Finished closing: removed after this frame.
    closed: bool,
    /// The main window, hidden because it was closed while others are open.
    hidden: bool,
    /// The Markdown preview's layout, for scrolling it with the editor.
    preview: crate::preview::PreviewState,
}

impl WindowState {
    fn new(viewport: ViewportId, builder: egui::ViewportBuilder, tabs: Tabs) -> Self {
        Self {
            viewport,
            builder,
            tabs,
            find: FindState::default(),
            cursor: CursorInfo::default(),
            pending: None,
            status: None,
            last_title: String::new(),
            reveal: None,
            show_about: false,
            show_settings: false,
            help: None,
            go_to_line: None,
            confirm_reload: None,
            save_problem: None,
            close_queue: Vec::new(),
            word_counts: None,
            word_counts_key: None,
            word_counts_at: 0.0,
            match_cache: RefCell::default(),
            show_results: false,
            closing: false,
            closed: false,
            hidden: false,
            preview: Default::default(),
        }
    }
}

/// The open files in a window, to reopen next launch, and (`keep_untitled`)
/// its untitled documents with their text.
fn session_of(w: &WindowState, keep_untitled: bool) -> settings::Session {
    let active_id = w.tabs.active().id;
    let mut session = settings::Session::default();
    let mut at = 0;
    for tab in w.tabs.iter() {
        let kept = match &tab.doc.path {
            Some(path) => {
                session.files.push(path.clone());
                true
            }
            None if keep_untitled && !tab.doc.text.is_empty() => {
                let record = crate::recovery::Record::from_document(&tab.doc);
                session.untitled.push(settings::KeptTab { at, record });
                true
            }
            None => false,
        };
        if kept {
            if tab.id == active_id {
                session.active = at;
            }
            at += 1;
        }
    }
    session
}

/// The first tab that quitting should ask about: one with unsaved changes,
/// unless it's untitled and being kept for next time (`keep_untitled`).
fn first_to_ask(tabs: &Tabs, keep_untitled: bool) -> Option<TabId> {
    tabs.iter()
        .find(|t| t.doc.is_dirty() && !t.discard_on_quit && !(keep_untitled && t.doc.path.is_none()))
        .map(|t| t.id)
}

pub struct SlothApp {
    /// The window being drawn or acted on right now. Between frames this is
    /// the focused window, so menu commands apply to it.
    w: WindowState,
    /// All other windows. Windows are swapped in and out of `w` as needed.
    others: Vec<WindowState>,
    /// The window that last had keyboard focus.
    focused: ViewportId,
    /// Some window had focus during this frame.
    any_focused: bool,
    next_window: u64,
    settings: Settings,
    /// Set once the user has confirmed quitting, so the close goes through.
    allow_close: bool,
    /// `None` in tests unless a test turns it on.
    recovery: Option<Recovery>,
    /// Documents found from a session that crashed, waiting for the user.
    recovery_prompt: Option<Leftovers>,
    last_recovery_save: f64,
    markdown_cache: egui_commonmark::CommonMarkCache,
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
            w: WindowState::new(ViewportId::ROOT, egui::ViewportBuilder::default(), Tabs::new()),
            others: Vec::new(),
            focused: ViewportId::ROOT,
            any_focused: false,
            next_window: 0,
            settings,
            allow_close: false,
            recovery: None,
            recovery_prompt: None,
            last_recovery_save: 0.0,
            markdown_cache: Default::default(),
            last_disk_check: 0.0,
            was_focused: true,
            #[cfg(target_os = "macos")]
            native_menu: None,
        };
        app.settings.theme.apply(ctx);
        app.apply_fonts(ctx);
        // Pictures in the Markdown preview.
        egui_extras::install_image_loaders(ctx);

        // Reopen the windows and tabs from last time, then any files we were
        // asked to open.
        let session = std::mem::take(&mut app.settings.session);
        let others = std::mem::take(&mut app.settings.other_windows);
        if app.settings.reopen_at_launch {
            app.restore_session(session);
            for session in others {
                let id = app.add_window(None);
                app.select(id);
                app.restore_session(session);
            }
        }
        app.select(ViewportId::ROOT);
        for path in files {
            if path.exists() {
                app.open_path(path);
            } else {
                // `sloth newfile.txt` starts an empty doc that saves there.
                let mut doc = Document::new();
                doc.path = Some(path);
                app.w.tabs.open(doc);
            }
        }
        app
    }

    fn restore_session(&mut self, session: settings::Session) {
        let mut active = None;
        let total = session.files.len() + session.untitled.len();
        let mut files = session.files.into_iter();
        let mut kept: std::collections::HashMap<usize, crate::recovery::Record> =
            session.untitled.into_iter().map(|k| (k.at, k.record)).collect();
        for i in 0..total {
            // An untitled document kept from last time, still unsaved.
            let opened = if let Some(record) = kept.remove(&i) {
                self.w.tabs.open(record.into_document());
                true
            } else {
                match files.next() {
                    Some(path) if path.exists() => {
                        self.open_path(path);
                        true
                    }
                    _ => false,
                }
            };
            if opened && i == session.active {
                active = Some(self.w.tabs.active().id);
            }
        }
        if let Some(id) = active {
            self.w.tabs.activate(id);
        }
    }

    // ----- windows ----------------------------------------------------------

    /// Make the window with this viewport the current one (`self.w`).
    fn select(&mut self, viewport: ViewportId) {
        if self.w.viewport == viewport {
            return;
        }
        if let Some(i) = self.others.iter().position(|w| w.viewport == viewport) {
            std::mem::swap(&mut self.w, &mut self.others[i]);
        }
    }

    fn viewports(&self) -> Vec<ViewportId> {
        let mut ids: Vec<ViewportId> = self.windows().map(|w| w.viewport).collect();
        // The main window first: the others are drawn from inside its frame.
        ids.sort_by_key(|id| *id != ViewportId::ROOT);
        ids
    }

    fn windows(&self) -> impl Iterator<Item = &WindowState> {
        std::iter::once(&self.w).chain(&self.others)
    }

    fn window_mut(&mut self, viewport: ViewportId) -> Option<&mut WindowState> {
        if self.w.viewport == viewport {
            return Some(&mut self.w);
        }
        self.others.iter_mut().find(|w| w.viewport == viewport)
    }

    /// Windows the user can see (the main window may be hidden).
    fn visible_window_count(&self) -> usize {
        self.windows().filter(|w| !w.hidden && !w.closed).count()
    }

    /// Run `f` with each window in turn as the current one.
    fn for_each_window(&mut self, mut f: impl FnMut(&mut Self)) {
        let start = self.w.viewport;
        for id in self.viewports() {
            self.select(id);
            f(self);
        }
        self.select(start);
    }

    /// Create a window (it appears on the next frame) and return its
    /// viewport. With `tab`, the window starts with that tab.
    fn add_window(&mut self, tab: Option<tabs::Tab>) -> ViewportId {
        let tabs = tab.map(Tabs::with_tab).unwrap_or_default();
        self.next_window += 1;
        let id = ViewportId::from_hash_of(("sloth-window", self.next_window));
        // Cascade new windows a little down and to the right.
        let offset = 28.0 * (self.next_window % 8) as f32;
        let builder = egui::ViewportBuilder::default()
            .with_title("Sloth")
            .with_inner_size([900.0, 650.0])
            .with_min_inner_size([300.0, 200.0])
            .with_position([120.0 + offset, 120.0 + offset])
            .with_drag_and_drop(true);
        self.others.push(WindowState::new(id, builder, tabs));
        id
    }

    /// File → New Window (or bring back the main window if it's hidden).
    fn new_window(&mut self, ctx: &egui::Context, tab: Option<tabs::Tab>) {
        if let Some(root) = self.window_mut(ViewportId::ROOT)
            && root.hidden
        {
            root.hidden = false;
            if let Some(tab) = tab {
                root.tabs = Tabs::with_tab(tab);
            }
            ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Visible(true));
            ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Focus);
            self.focused = ViewportId::ROOT;
            return;
        }
        self.focused = self.add_window(tab);
    }

    /// Close the current window: its tabs first (asking about unsaved ones),
    /// then the window. Closing the last window quits.
    fn request_close_window(&mut self, ctx: &egui::Context) {
        if self.visible_window_count() <= 1 {
            self.request_quit(ctx);
            return;
        }
        self.w.closing = true;
        self.w.close_queue = self.w.tabs.iter().map(|t| t.id).collect();
    }

    /// Once a closing window's tabs are all closed, close (or for the main
    /// window, hide) it.
    fn finish_closing(&mut self, ctx: &egui::Context) {
        if !self.w.closing || !self.w.close_queue.is_empty() || self.w.pending.is_some() {
            return;
        }
        self.w.closing = false;
        if !self.w.tabs.is_pristine() {
            return; // something was cancelled
        }
        if self.w.viewport == ViewportId::ROOT {
            // The main window can't really close while others are open
            // (that would quit), so hide it until it's needed again.
            self.w.hidden = true;
            ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        } else {
            self.w.closed = true;
        }
    }

    // ----- command dispatch -------------------------------------------------

    pub fn execute(&mut self, ctx: &egui::Context, cmd: Command) {
        match cmd {
            Command::New => {
                self.w.tabs.add(Document::new());
                self.after_tab_switch(ctx);
            }
            Command::Open => self.open_dialog(),
            Command::Save => {
                self.save(self.w.tabs.active().id);
            }
            Command::SaveAs => {
                self.save_as(self.w.tabs.active().id);
            }
            Command::CloseTab => self.request_close_tab(self.w.tabs.active().id),
            Command::Quit => self.request_quit(ctx),
            Command::NextTab => {
                self.w.tabs.select_next();
                self.after_tab_switch(ctx);
            }
            Command::PreviousTab => {
                self.w.tabs.select_previous();
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

            Command::Find => self.w.find.open(false, self.selected_text(ctx)),
            Command::FindAndReplace => self.w.find.open(true, self.selected_text(ctx)),
            Command::FindNext => {
                self.settings.add_search(&self.w.find.query);
                self.find_step(ctx, Direction::Next);
            }
            Command::FindPrevious => {
                self.settings.add_search(&self.w.find.query);
                self.find_step(ctx, Direction::Previous);
            }
            Command::FindInTabs => {
                self.w.find.open(false, self.selected_text(ctx));
                self.w.show_results = true;
            }
            Command::UseSelectionForFind => {
                if let Some(text) = self.selected_text(ctx) {
                    self.w.find.query = text;
                    self.w.find.visible = true;
                    self.w.find.current = None;
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
            Command::GoToLine => self.w.go_to_line = Some(String::new()),
            Command::RevertToSaved => {
                if self.can_revert() {
                    self.w.confirm_reload = Some(ReloadKind::Revert);
                }
            }
            Command::ReopenWithEncoding(encoding) => {
                if self.w.tabs.active().doc.path.is_some() {
                    if self.w.tabs.active().doc.is_dirty() {
                        self.w.confirm_reload = Some(ReloadKind::WithEncoding(encoding));
                    } else {
                        self.reload_active(ReloadKind::WithEncoding(encoding));
                    }
                }
            }
            Command::ConvertEncoding(encoding) => {
                self.w.tabs.active_mut().doc.convert_encoding(encoding.encoding());
            }
            Command::SetLineEnding(line_ending) => {
                self.w.tabs.active_mut().doc.set_line_ending(line_ending);
            }

            Command::Indent | Command::Outdent => {
                let indent = self.settings.editor.indent();
                let tab = self.w.tabs.active_mut();
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
            Command::ToggleSyntaxHighlighting => {
                let e = &mut self.settings.editor;
                e.syntax_highlighting = !e.syntax_highlighting;
            }
            Command::Print => self.print(),
            Command::NewWindow => self.new_window(ctx, None),
            Command::CloseWindow => self.request_close_window(ctx),
            Command::MoveTabToNewWindow => {
                let id = self.w.tabs.active().id;
                if let Some(tab) = self.w.tabs.take(id) {
                    self.new_window(ctx, Some(tab));
                }
            }
            Command::ToggleMarkdownPreview => {
                let e = &mut self.settings.editor;
                e.markdown_preview = !e.markdown_preview;
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
                self.w.close_queue = self.w.tabs.others(self.w.tabs.active().id);
            }
            Command::CloseTabsToRight => {
                self.w.close_queue = self.w.tabs.to_the_right_of(self.w.tabs.active().id);
            }

            Command::About => self.w.show_about = true,
            Command::Settings => self.w.show_settings = true,
            Command::Help => self.w.help = Some(HelpPage::Shortcuts),
            Command::ReleaseNotes => self.w.help = Some(HelpPage::ReleaseNotes),
        }
    }

    fn is_checked(&self, cmd: Command) -> Option<bool> {
        match cmd {
            Command::ToggleLineNumbers => Some(self.settings.editor.show_line_numbers),
            Command::ToggleWordWrap => Some(self.settings.editor.word_wrap),
            Command::SetTheme(theme) => Some(self.settings.theme == theme),
            Command::ConvertEncoding(e) => {
                Some(e.encoding() == self.w.tabs.active().doc.format.encoding)
            }
            Command::SetLineEnding(l) => Some(self.w.tabs.active().doc.format.line_ending == l),
            Command::IndentWithTabs => Some(!self.settings.editor.indent_with_spaces),
            Command::IndentWithSpaces => Some(self.settings.editor.indent_with_spaces),
            Command::SetTabWidth(w) => Some(self.settings.editor.tab_width == w as usize),
            Command::ToggleAutoIndent => Some(self.settings.editor.auto_indent),
            Command::ToggleHighlightLine => Some(self.settings.editor.highlight_current_line),
            Command::ToggleSyntaxHighlighting => Some(self.settings.editor.syntax_highlighting),
            Command::ToggleMarkdownPreview => Some(self.settings.editor.markdown_preview),
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
            Command::ReopenWithEncoding(_) => self.w.tabs.active().doc.path.is_some(),
            Command::CloseOtherTabs => self.w.tabs.iter().count() > 1,
            Command::MoveTabToNewWindow => !self.w.tabs.is_pristine(),
            Command::CloseTabsToRight => {
                !self.w.tabs.to_the_right_of(self.w.tabs.active().id).is_empty()
            }
            Command::ClearRecent => !self.settings.recent_files.is_empty(),
            _ => true,
        }
    }

    fn can_revert(&self) -> bool {
        let doc = &self.w.tabs.active().doc;
        doc.path.is_some() && doc.is_dirty()
    }

    /// Word count for the status bar: the selection's, or the whole
    /// document's. Skipped for very large files to keep typing fast.
    fn count_words(&mut self, ctx: &egui::Context) -> Option<String> {
        let tab = self.w.tabs.active();
        let len = tab.doc.text.len();
        if len > WORD_COUNT_MAX_BYTES {
            return None;
        }
        let selection = editor::selection(ctx, tab.id).filter(|s| !s.is_empty());
        // Counting a big document takes a few milliseconds, so for those
        // only refresh the count about once a second.
        let key = (tab.id, selection.clone());
        let now = ctx.input(|i| i.time);
        if len > WORD_COUNT_EVERY_FRAME_BYTES
            && self.w.word_counts_key.as_ref() == Some(&key)
            && now - self.w.word_counts_at < 1.0
        {
            return self.w.word_counts.clone();
        }
        self.w.word_counts_key = Some(key);
        self.w.word_counts_at = now;
        let tab = self.w.tabs.active();
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

    fn print(&mut self) {
        let doc = &self.w.tabs.active().doc;
        // Coloured and numbered as the editor shows it, in the light theme's
        // colours (it's going on white paper).
        let e = &self.settings.editor;
        let spans = e
            .syntax_highlighting
            .then(|| syntax::detect(doc.path.as_deref(), &doc.text))
            .flatten()
            .map(|syntax| {
                let colors = sloth_core::theme::ThemeChoice::Light.syntax_colors(false);
                syntax::HighlightCache::default().highlight(&doc.text, syntax, colors)
            })
            .unwrap_or_default();
        let page = sloth_core::print_text::layout(&doc.text, &spans, e.show_line_numbers);
        #[cfg(target_os = "macos")]
        crate::macos_print::print(&doc.display_name(), &page, e.font.as_deref());
        #[cfg(not(target_os = "macos"))]
        {
            let _ = page;
            self.w.status = Some("Printing is only available on macOS".into());
        }
    }

    /// Rendered Markdown beside the editor, for Markdown documents.
    fn markdown_preview(&mut self, ui: &mut egui::Ui) {
        let doc = &self.w.tabs.active().doc;
        if !self.settings.editor.markdown_preview || !syntax::is_markdown(doc.path.as_deref()) {
            return;
        }
        let width = ui.available_width();
        let fill = ui.visuals().extreme_bg_color;
        let panel = self.panel_id("markdown-preview");
        let doc = &self.w.tabs.active().doc;
        let tab = self.w.tabs.active().id;
        let sections = crate::preview::sections(&doc.text, doc.path.as_deref().and_then(|p| p.parent()));
        let total_lines = doc.text.split('\n').count();
        let top = self.w.cursor.top_line;
        let cache = &mut self.markdown_cache;
        let state = &mut self.w.preview;
        // Follow the editor when it's moved (or another tab is showing), so
        // the preview can still be scrolled on its own meanwhile.
        let follow = !matches!(state.synced, Some((t, Some(line))) if t == tab && (line - top).abs() <= 0.01);
        egui::Panel::right(panel)
            .resizable(true)
            .default_size(width / 2.0)
            .size_range(200.0..=width * 0.8)
            .frame(egui::Frame::NONE.fill(fill).inner_margin(Margin::symmetric(16, 10)))
            .show(ui, |ui| {
                let mut area = egui::ScrollArea::vertical().id_salt("markdown-preview-scroll").auto_shrink([false, false]);
                // Last frame's layout is this tab's: line it up with the editor.
                let laid_out = state.synced.is_some_and(|(t, _)| t == tab);
                if follow && laid_out {
                    area = area.vertical_scroll_offset(crate::preview::offset_for(state, top, total_lines));
                }
                let image_width = ui.available_width().max(1.0) as usize;
                let out = area.show(ui, |ui| {
                    let origin = ui.cursor().top();
                    let mut placed = Vec::with_capacity(sections.len());
                    for (i, section) in sections.iter().enumerate() {
                        let top = ui.cursor().top() - origin;
                        ui.push_id(("preview-section", i), |ui| {
                            egui_commonmark::CommonMarkViewer::new()
                                .max_image_width(Some(image_width))
                                .show(ui, cache, &section.text);
                        });
                        let height = ui.cursor().top() - origin - top;
                        placed.push(crate::preview::Placed { first_line: section.first_line, lines: section.lines, top, height });
                    }
                    placed
                });
                state.placed = out.inner;
                state.content_height = out.content_size.y;
                state.view_height = out.inner_rect.height();
                if follow {
                    // Lined up now, or (first frame for this tab) next frame.
                    state.synced = Some((tab, laid_out.then_some(top)));
                }
            });
    }

    fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.w.show_settings {
            return;
        }
        let mut open = true;
        let changes =
            crate::settings_window::show(ctx, &mut open, &mut self.settings, fonts::families());
        self.w.show_settings = open;
        if changes.theme {
            self.settings.theme.apply(ctx);
        }
        if changes.fonts {
            self.apply_fonts(ctx);
        }
    }

    fn apply_fonts(&self, ctx: &egui::Context) {
        let e = &self.settings.editor;
        fonts::apply(ctx, e.font.as_deref(), None, e.tab_width);
    }

    /// Close queued tabs, stopping to ask about any with unsaved changes.
    fn process_close_queue(&mut self) {
        while self.w.pending.is_none() && !self.w.close_queue.is_empty() {
            let id = self.w.close_queue.remove(0);
            self.request_close_tab(id);
        }
    }

    /// Panel ids include the window, so each window keeps its own layout.
    fn panel_id(&self, name: &str) -> egui::Id {
        egui::Id::unique((name, self.w.viewport))
    }

    /// A dialog is up, so menu commands and shortcuts should wait.
    fn modal_open(&self) -> bool {
        self.w.pending.is_some()
            || self.w.show_about
            || self.w.confirm_reload.is_some()
            || self.w.go_to_line.is_some()
            || self.w.save_problem.is_some()
            || self.recovery_prompt.is_some()
    }

    fn focus_editor(&self, ctx: &egui::Context) {
        let id = editor::editor_id(self.w.tabs.active().id);
        ctx.memory_mut(|m| m.request_focus(id));
    }

    fn after_tab_switch(&mut self, ctx: &egui::Context) {
        self.w.find.current = None;
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
        match self.w.tabs.get(id) {
            Some(tab) if tab.doc.is_dirty() => {
                self.w.tabs.activate(id);
                self.w.pending = Some(PendingAction::CloseTab(id));
            }
            Some(_) => self.w.tabs.close(id),
            None => {}
        }
    }

    /// Ask about each tab with unsaved changes (untitled ones are kept for
    /// next time instead, if tabs are reopened at launch), then quit.
    fn request_quit(&mut self, ctx: &egui::Context) {
        let keep = self.keeps_untitled();
        if let Some(id) = first_to_ask(&self.w.tabs, keep) {
            self.w.tabs.activate(id);
            self.w.pending = Some(PendingAction::Quit);
        } else if let Some(other) = self.others.iter_mut().find(|w| first_to_ask(&w.tabs, keep).is_some()) {
            // Ask in the other window, and bring it to the front.
            let id = first_to_ask(&other.tabs, keep).expect("checked above");
            other.tabs.activate(id);
            other.pending = Some(PendingAction::Quit);
            ctx.send_viewport_cmd_to(other.viewport, ViewportCommand::Focus);
        } else {
            // A clean quit: crash-recovery copies are no longer needed.
            if let Some(recovery) = &mut self.recovery {
                recovery.clear();
            }
            self.allow_close = true;
            // Closing the main window ends the app (and its other windows).
            ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Close);
        }
    }

    fn unsaved_changes_dialog(&mut self, ctx: &egui::Context) {
        let Some(action) = self.w.pending else {
            return;
        };
        let tab_id = match action {
            PendingAction::CloseTab(id) => id,
            PendingAction::Quit => self.w.tabs.active().id,
        };
        let Some(tab) = self.w.tabs.get(tab_id) else {
            self.w.pending = None;
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
        self.w.pending = None;
        match (choice, action) {
            (DialogChoice::Cancel, _) => {
                for w in std::iter::once(&mut self.w).chain(self.others.iter_mut()) {
                    for tab in w.tabs.iter_mut() {
                        tab.discard_on_quit = false;
                    }
                }
                self.w.close_queue.clear();
                self.w.closing = false;
            }
            (DialogChoice::Save, PendingAction::CloseTab(id)) => {
                if self.save(id) {
                    self.w.tabs.close(id);
                }
            }
            (DialogChoice::Discard, PendingAction::CloseTab(id)) => self.w.tabs.close(id),
            (DialogChoice::Save, PendingAction::Quit) => {
                if self.save(tab_id) {
                    self.request_quit(ctx);
                }
            }
            (DialogChoice::Discard, PendingAction::Quit) => {
                if let Some(tab) = self.w.tabs.get_mut(tab_id) {
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
        if let Some(id) = self.w.tabs.find_by_path(&path) {
            self.w.tabs.activate(id);
            return;
        }
        match Document::open(&path) {
            Ok(doc) => {
                self.w.status = doc.read_with_errors.then(|| {
                    format!(
                        "Some of {} isn't valid {}; try File \u{2192} Reopen with Encoding",
                        doc.display_name(),
                        doc.format.encoding.name()
                    )
                });
                self.w.tabs.open(doc);
                self.settings.add_recent(&path);
                self.w.find.current = None;
            }
            Err(e) => {
                if !path.exists() {
                    self.settings.remove_recent(&path);
                }
                self.w.status = Some(format!("Could not open {}: {e}", path.display()));
            }
        }
    }

    /// Returns true if the document was saved.
    fn save(&mut self, id: TabId) -> bool {
        let Some(tab) = self.w.tabs.get_mut(id) else {
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
        let Some(tab) = self.w.tabs.get_mut(id) else {
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
                self.w.status = Some("Saved".into());
                return true;
            }
            Err(SaveError::Unmappable(encoding)) => SaveProblemKind::Unmappable(encoding),
            Err(SaveError::ReadWithErrors) => SaveProblemKind::ReadWithErrors,
            Err(e) => {
                self.w.status = Some(format!("Save failed: {e}"));
                return false;
            }
        };
        self.w.tabs.activate(tab);
        self.w.save_problem = Some(SaveProblem { tab, path, kind });
        false
    }

    fn save_problem_dialog(&mut self, ctx: &egui::Context) {
        let Some(problem) = &self.w.save_problem else {
            return;
        };
        let Some(tab) = self.w.tabs.get(problem.tab) else {
            self.w.save_problem = None;
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
        let problem = self.w.save_problem.take().expect("checked above");
        if !proceed {
            return;
        }
        let Some(tab) = self.w.tabs.get_mut(problem.tab) else {
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

    /// Write recovery copies of unsaved documents (in every window); drop
    /// the rest.
    fn save_recovery_copies(&mut self) {
        let Some(recovery) = &mut self.recovery else {
            return;
        };
        let mut error = None;
        let mut open = std::collections::HashSet::new();
        for w in std::iter::once(&self.w).chain(&self.others) {
            for tab in w.tabs.iter() {
                open.insert(tab.id);
                if tab.doc.is_dirty() {
                    if let Err(e) = recovery.save(tab.id, &tab.doc) {
                        error = Some(format!("Couldn't write recovery copy: {e}"));
                    }
                } else {
                    recovery.remove(tab.id);
                }
            }
        }
        recovery.retain(|id| open.contains(&id));
        if error.is_some() {
            self.w.status = error;
        }
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
                let existing = record.path.as_deref().and_then(|p| self.w.tabs.find_by_path(p));
                match existing {
                    Some(id) => {
                        if let Some(tab) = self.w.tabs.get_mut(id) {
                            tab.doc.text = record.text;
                        }
                        self.w.tabs.activate(id);
                    }
                    None => {
                        self.w.tabs.open(record.into_document());
                    }
                }
            }
            self.w.status = Some("Restored unsaved documents".into());
            // Write them into this session's folder before deleting the old one.
            self.save_recovery_copies();
        }
        leftovers.remove();
    }

    fn current_dir(&self) -> Option<PathBuf> {
        self.w.tabs
            .active()
            .doc
            .path
            .as_ref()
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf())
    }

    // ----- session, outside changes, revert, go to line ---------------------

    /// The open files in the current window, to reopen next launch.
    #[cfg(test)]
    fn session(&self) -> settings::Session {
        session_of(&self.w, false)
    }

    /// Untitled documents are kept across a quit when windows and tabs are
    /// reopened at launch.
    fn keeps_untitled(&self) -> bool {
        self.settings.reopen_at_launch
    }

    /// Main window, then the other windows that have files open. Untitled
    /// documents go in only on a clean quit: the saves made now and then
    /// while running leave them to crash recovery, so a crash can't bring
    /// them back twice.
    fn all_sessions(&self) -> (settings::Session, Vec<settings::Session>) {
        let keep = self.allow_close && self.keeps_untitled();
        let main = self
            .windows()
            .find(|w| w.viewport == ViewportId::ROOT)
            .map(|w| session_of(w, keep))
            .unwrap_or_default();
        let others = self
            .windows()
            .filter(|w| w.viewport != ViewportId::ROOT && !w.closed)
            .map(|w| session_of(w, keep))
            .filter(|s| !s.files.is_empty() || !s.untitled.is_empty())
            .collect();
        (main, others)
    }

    /// Look for open files that other programs changed or removed.
    fn check_disk(&mut self) {
        let reloaded = self.w.tabs.check_disk();
        if !reloaded.is_empty() {
            self.w.find.current = None;
            self.w.status = Some(format!("Reloaded {} (changed on disk)", reloaded.join(", ")));
        }
    }

    fn reload_active(&mut self, kind: ReloadKind) {
        let tab = self.w.tabs.active_mut();
        let encoding = match kind {
            // Keep reading it the way it's being read now.
            ReloadKind::Revert => tab.doc.format.encoding,
            ReloadKind::WithEncoding(e) => e.encoding(),
        };
        match tab.doc.reload_with_encoding(Some(encoding)) {
            Ok(()) => {
                tab.disk_notice = None;
                self.w.find.current = None;
                let name = tab.doc.display_name();
                self.w.status = Some(match kind {
                    ReloadKind::Revert => format!("Reverted {name}"),
                    ReloadKind::WithEncoding(e) if tab.doc.read_with_errors => {
                        format!("{name} isn't valid {} either", e.short_name())
                    }
                    ReloadKind::WithEncoding(e) => format!("Reopened {name} as {}", e.short_name()),
                });
            }
            Err(e) => self.w.status = Some(format!("Could not reload: {e}")),
        }
    }

    fn disk_notice_bar(&mut self, ui: &mut egui::Ui) {
        let id = self.panel_id("disk-notice");
        match tabs::disk_notice_bar(ui, id, self.w.tabs.active()) {
            Some(NoticeChoice::Reload) => self.reload_active(ReloadKind::Revert),
            Some(NoticeChoice::Keep) => {
                let tab = self.w.tabs.active_mut();
                tab.doc.acknowledge_disk_state();
                tab.disk_notice = None;
            }
            None => {}
        }
    }

    /// Confirm discarding unsaved changes before Revert or Reopen with Encoding.
    fn reload_dialog(&mut self, ctx: &egui::Context) {
        let Some(kind) = self.w.confirm_reload else {
            return;
        };
        let name = self.w.tabs.active().doc.display_name();
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
            self.w.confirm_reload = None;
            if go {
                self.reload_active(kind);
            }
        }
    }

    fn go_to_line_dialog(&mut self, ctx: &egui::Context) {
        let Some(input) = &mut self.w.go_to_line else {
            return;
        };
        let line_count = self.w.tabs.active().doc.line_count();
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
            let tab = self.w.tabs.active();
            let index = editor::char_index_of(&tab.doc.text, line, col);
            editor::set_selection(ctx, tab.id, index..index);
            self.w.reveal = Some(index);
            self.focus_editor(ctx);
            close = true;
        }
        if close {
            self.w.go_to_line = None;
        }
    }

    // ----- find & replace ---------------------------------------------------

    /// Matches in the active tab (empty when not searching or the pattern
    /// is invalid).
    fn matches(&self) -> Arc<Vec<Range<usize>>> {
        if !self.w.find.is_searching() {
            return Arc::default();
        }
        self.search_tab(self.w.tabs.active()).unwrap_or_default()
    }

    /// Why the current search can't run, if it can't (an invalid regex).
    fn find_error(&self) -> Option<String> {
        if !self.w.find.is_searching() {
            return None;
        }
        self.search_tab(self.w.tabs.active()).err()
    }

    /// Search one tab, reusing the last result if nothing has changed.
    fn search_tab(&self, tab: &Tab) -> Result<Arc<Vec<Range<usize>>>, String> {
        let mut cache = self.w.match_cache.borrow_mut();
        if cache.query != self.w.find.query || cache.options != self.w.find.options {
            *cache = MatchCache {
                query: self.w.find.query.clone(),
                options: self.w.find.options,
                tabs: HashMap::new(),
            };
        }
        if let Some(hit) = cache.tabs.get(&tab.id)
            && hit.text == tab.doc.text
        {
            return hit.matches.clone();
        }
        let matches = find::find_all(&tab.doc.text, &self.w.find.query, self.w.find.options).map(Arc::new);
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
        let mut cache = self.w.match_cache.borrow_mut();
        let entry = cache.tabs.get_mut(&tab.id).expect("just searched");
        entry
            .rows
            .get_or_insert_with(|| Arc::new(search_results::build_rows(&tab.doc.text, &matches)))
            .clone()
    }

    fn results_panel(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        if !self.w.show_results || !self.w.find.is_searching() {
            return;
        }
        let rows: Vec<(TabId, String, Arc<Vec<ResultRow>>)> = self
            .w.tabs
            .iter()
            .map(|t| (t.id, t.doc.display_name(), self.result_rows(t)))
            .collect();
        let results: Vec<TabResults<'_>> = rows
            .iter()
            .map(|(id, name, rows)| TabResults { key: *id, name: name.clone(), rows })
            .collect();
        let error = self.find_error();
        let action = egui::Panel::bottom(self.panel_id("search-results"))
            .resizable(true)
            .default_size(200.0)
            .show(ui, |ui| {
                search_results::results_panel(ui, &results, error.as_deref(), &ResultsWording::TABS)
            })
            .inner;
        match action {
            Some(ResultsAction::Jump(tab, range)) => {
                self.w.tabs.activate(tab);
                self.select_match(ctx, range);
                self.focus_editor(ctx);
            }
            Some(ResultsAction::Close) => self.w.show_results = false,
            None => {}
        }
        // Forget closed tabs.
        let tabs = &self.w.tabs;
        self.w.match_cache.borrow_mut().tabs.retain(|id, _| tabs.get(*id).is_some());
    }

    /// The editor's selection as a byte range.
    fn selection_bytes(&self, ctx: &egui::Context) -> Range<usize> {
        let tab = self.w.tabs.active();
        let text = &tab.doc.text;
        editor::selection(ctx, tab.id)
            .map(|r| find::char_to_byte(text, r.start)..find::char_to_byte(text, r.end))
            .unwrap_or(0..0)
    }

    /// Selected text, if it's a short single line (used to pre-fill Find).
    fn selected_text(&self, ctx: &egui::Context) -> Option<String> {
        let range = self.selection_bytes(ctx);
        let text = self.w.tabs.active().doc.text.get(range)?;
        (!text.is_empty() && !text.contains('\n') && text.len() < 200).then(|| text.to_owned())
    }

    fn find_step(&mut self, ctx: &egui::Context, direction: Direction) {
        if self.w.find.query.is_empty() {
            self.w.find.open(false, self.selected_text(ctx));
            return;
        }
        self.w.find.visible = true;
        let matches = self.matches();
        if matches.is_empty() {
            self.w.find.current = None;
            return;
        }
        // Search relative to the current match, or else the cursor/selection.
        let anchor = self
            .w.find
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
        let tab = self.w.tabs.active();
        let start = find::byte_to_char(&tab.doc.text, range.start);
        let end = find::byte_to_char(&tab.doc.text, range.end);
        editor::set_selection(ctx, tab.id, start..end);
        self.w.reveal = Some(start);
        self.w.find.current = Some(range);
    }

    fn replace_current(&mut self, ctx: &egui::Context) {
        let matches = self.matches();
        let Some(current) = self.w.find.current.clone().filter(|c| matches.contains(c)) else {
            // Nothing selected yet: the first press just finds.
            self.find_step(ctx, Direction::Next);
            return;
        };
        let replacement = match find::replacement_for(
            &self.w.tabs.active().doc.text,
            current.clone(),
            &self.w.find.query,
            &self.w.find.replacement,
            self.w.find.options,
        ) {
            Ok(r) => r,
            Err(e) => {
                self.w.status = Some(format!("Can't replace: {e}"));
                return;
            }
        };
        let tab = self.w.tabs.active_mut();
        tab.doc.text.replace_range(current.clone(), &replacement);
        let after = current.start + replacement.len();
        let after_char = find::byte_to_char(&tab.doc.text, after);
        let id = tab.id;
        editor::set_selection(ctx, id, after_char..after_char);
        self.w.find.current = None;
        self.find_step(ctx, Direction::Next);
    }

    fn replace_all(&mut self, ctx: &egui::Context) {
        let tab = self.w.tabs.active_mut();
        let (text, count) = match find::replace_all(
            &tab.doc.text,
            &self.w.find.query,
            &self.w.find.replacement,
            self.w.find.options,
        ) {
            Ok(result) => result,
            Err(e) => {
                self.w.status = Some(format!("Can't replace: {e}"));
                return;
            }
        };
        tab.doc.text = text;
        let id = tab.id;
        editor::set_selection(ctx, id, 0..0);
        self.w.find.current = None;
        self.w.status = Some(match count {
            1 => "Replaced 1 occurrence".to_owned(),
            n => format!("Replaced {n} occurrences"),
        });
    }

    fn handle_find_action(&mut self, ctx: &egui::Context, action: FindAction) {
        match action {
            FindAction::Search => {
                if self.w.find.query.is_empty() {
                    self.w.find.current = None;
                } else {
                    self.find_step(ctx, Direction::FromCursor);
                }
            }
            FindAction::Next => {
                self.settings.add_search(&self.w.find.query);
                self.find_step(ctx, Direction::Next);
            }
            FindAction::Previous => {
                self.settings.add_search(&self.w.find.query);
                self.find_step(ctx, Direction::Previous);
            }
            FindAction::Replace => {
                self.settings.add_search(&self.w.find.query);
                self.replace_current(ctx);
            }
            FindAction::ReplaceAll => {
                self.settings.add_search(&self.w.find.query);
                self.replace_all(ctx);
            }
            FindAction::ToggleResults => self.w.show_results = !self.w.show_results,
            FindAction::Close => {
                self.w.find.close();
                self.w.show_results = false;
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
        let doc = &self.w.tabs.active().doc;
        // Right-hand details first, so the path on the left gets whatever
        // room is left and is cut short (…) rather than overlapping them.
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(doc.format.summary())
                .on_hover_text("Encoding and line endings: change them in the File menu");
            ui.separator();
            if self.settings.editor.syntax_highlighting
                && let Some(syntax) = syntax::detect(doc.path.as_deref(), &doc.text)
            {
                ui.label(&syntax.name);
                ui.separator();
            }
            ui.label(match doc.line_count() {
                1 => "1 line".to_owned(),
                n => format!("{n} lines"),
            });
            if let Some(counts) = &self.w.word_counts {
                ui.separator();
                ui.label(counts);
            }
            if let Some((line, col)) = self.w.cursor.line_col {
                ui.separator();
                ui.label(format!("Ln {line}, Col {col}"));
            }
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                if let Some(msg) = &self.w.status {
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
        if !self.w.show_about {
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
                        self.w.help = Some(HelpPage::ReleaseNotes);
                        self.w.show_about = false;
                    }
                    if ui.button("OK").clicked() || ui.input(|i| i.key_pressed(Key::Enter)) {
                        self.w.show_about = false;
                    }
                });
            });
        });
        if response.should_close() {
            self.w.show_about = false;
        }
    }

    fn help_window(&mut self, ctx: &egui::Context) {
        let Some(mut page) = self.w.help else {
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
        self.w.help = open.then_some(page);
    }

    fn update_title(&mut self, ctx: &egui::Context) {
        let doc = &self.w.tabs.active().doc;
        let title = format!(
            "{}{} — Sloth",
            if doc.is_dirty() { "• " } else { "" },
            doc.display_name()
        );
        if title != self.w.last_title {
            ctx.send_viewport_cmd(ViewportCommand::Title(title.clone()));
            self.w.last_title = title;
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
        let (main, others) = self.all_sessions();
        self.settings.session = main;
        self.settings.other_windows = others;
        self.settings.save(storage);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // Files opened from Finder go to the focused window.
        #[cfg(target_os = "macos")]
        {
            let opened = crate::macos_open::take_pending();
            if !opened.is_empty() {
                self.select(self.focused);
                for path in opened {
                    self.open_path(path);
                }
            }
        }

        // Check for outside changes when the app regains focus, and every
        // couple of seconds while it has focus; write recovery copies every
        // few seconds and when it loses focus.
        let focused = std::mem::take(&mut self.any_focused) || self.was_focused_initially();
        let now = ctx.input(|i| i.time);
        if focused
            && (!self.was_focused || now - self.last_disk_check >= DISK_CHECK_INTERVAL.as_secs_f64())
        {
            self.for_each_window(|app| app.check_disk());
            self.last_disk_check = now;
        }
        if (self.was_focused && !focused) || now - self.last_recovery_save >= RECOVERY_INTERVAL {
            self.save_recovery_copies();
            self.last_recovery_save = now;
        }
        self.was_focused = focused;
        ctx.request_repaint_after(DISK_CHECK_INTERVAL);

        // Draw every window: the main one in this frame, the others in their
        // own viewports (drawn from inside this frame).
        for id in self.viewports() {
            self.select(id);
            if id == ViewportId::ROOT {
                self.window_ui(ui);
            } else if !self.w.closed {
                let builder = self.w.builder.clone();
                ctx.show_viewport_immediate(id, builder, |ui, _| self.window_ui(ui));
            }
        }

        // Windows that finished closing are no longer shown, which closes them.
        self.select(ViewportId::ROOT);
        self.others.retain(|w| !w.closed);
        let focus_ok = self.windows().any(|w| w.viewport == self.focused && !w.hidden);
        if !focus_ok {
            let first_visible = self.windows().find(|w| !w.hidden).map_or(ViewportId::ROOT, |w| w.viewport);
            self.focused = first_visible;
        }
        self.select(self.focused);

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

    /// Other windows are drawn during the main window's frame. When the main
    /// window is minimized or hidden, eframe skips that frame (and only calls
    /// this), so ask for it anyway or the other windows would freeze.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let main_out_of_sight = ctx.input(|i| {
            let v = i.viewport();
            v.minimized == Some(true) || v.visible() == Some(false)
        });
        if main_out_of_sight && !self.others.is_empty() {
            ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::RequestPaintWhileHidden);
        }
    }
}

impl SlothApp {
    /// Before any window has reported focus (the very first frame), assume
    /// the app is in front.
    fn was_focused_initially(&self) -> bool {
        self.last_disk_check == 0.0
    }

    /// One window's frame: its menus, tab bar, find bar, editor and dialogs.
    fn window_ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();

        if ctx.input(|i| i.viewport().focused == Some(true)) {
            self.focused = self.w.viewport;
            self.any_focused = true;
        }

        // Window close button: close this window's tabs (asking about
        // unsaved ones) first. Closing the last window quits.
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            if self.w.pending.is_none() {
                self.request_close_window(&ctx);
            }
        }

        // Files dragged onto this window.
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .filter(|p| !p.as_os_str().is_empty())
                .collect()
        });
        for path in dropped {
            self.open_path(path);
        }

        // On macOS, shortcuts are routed through the native menu bar.
        // Menu commands and shortcuts go to the focused window.
        if self.w.viewport == self.focused {
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
        }

        if self.w.find.visible && !self.modal_open() && ctx.input(|i| i.key_pressed(Key::Escape))
        {
            self.handle_find_action(&ctx, FindAction::Close);
        }

        #[cfg(not(target_os = "macos"))]
        egui::Panel::top(self.panel_id("menu")).show(ui, |ui| self.menu_bar(ui));

        // Tab bar.
        let tab_action = egui::Panel::top(self.panel_id("tabs"))
            .frame(
                egui::Frame::NONE
                    .fill(ui.visuals().panel_fill)
                    .inner_margin(Margin { left: 6, right: 6, top: 4, bottom: 0 }),
            )
            .show(ui, |ui| tabs::tab_bar(ui, &self.w.tabs))
            .inner;
        match tab_action {
            Some(TabBarAction::Activate(id)) => {
                self.w.tabs.activate(id);
                self.after_tab_switch(&ctx);
            }
            Some(TabBarAction::Close(id)) => self.request_close_tab(id),
            Some(TabBarAction::CloseOthers(id)) => self.w.close_queue = self.w.tabs.others(id),
            Some(TabBarAction::CloseToTheRight(id)) => {
                self.w.close_queue = self.w.tabs.to_the_right_of(id);
            }
            Some(TabBarAction::Move { from, to }) => self.w.tabs.move_tab(from, to),
            None => {}
        }

        // Find bar.
        if self.w.find.visible {
            let matches = self.matches();
            let current_index = self
                .w.find
                .current
                .as_ref()
                .and_then(|c| matches.iter().position(|m| m == c));
            let error = self.find_error();
            let info = FindBarInfo {
                match_count: matches.len(),
                current_index,
                error: error.as_deref(),
                history: &self.settings.search_history,
                results_open: self.w.show_results,
                results_label: "All Tabs",
                results_hint: "List matches in every open tab (\u{21e7}\u{2318}F)",
                allow_replace: true,
            };
            let action = egui::Panel::top(self.panel_id("find"))
                .show(ui, |ui| find::find_bar(ui, &mut self.w.find, info))
                .inner;
            if let Some(action) = action {
                self.handle_find_action(&ctx, action);
            }
        }

        self.disk_notice_bar(ui);
        self.w.word_counts = self.count_words(&ctx);
        egui::Panel::bottom(self.panel_id("status")).show(ui, |ui| self.status_bar(ui));
        self.results_panel(&ctx, ui);

        // Computed after the find bar, which may have changed the text or query.
        let matches = self.matches();
        let highlights = Highlights {
            all: &matches,
            current: self.w.find.current.clone(),
            ..Default::default()
        };
        let reveal = self.w.reveal.take();
        self.markdown_preview(ui);
        let colors = self.settings.theme.syntax_colors(ui.visuals().dark_mode);
        let highlighting = self.settings.editor.syntax_highlighting;
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(ui.visuals().extreme_bg_color))
            .show(ui, |ui| {
                let tab = self.w.tabs.active_mut();
                let syntax = highlighting
                    .then(|| syntax::detect(tab.doc.path.as_deref(), &tab.doc.text))
                    .flatten()
                    .map(|syntax| SyntaxInput { syntax, colors, cache: &mut tab.highlight_cache });
                self.w.cursor = editor::show(
                    ui,
                    tab.id,
                    &mut tab.doc.text,
                    &self.settings.editor,
                    highlights,
                    reveal,
                    syntax,
                );
            });

        self.process_close_queue();
        self.finish_closing(&ctx);
        self.unsaved_changes_dialog(&ctx);
        self.reload_dialog(&ctx);
        self.save_problem_dialog(&ctx);
        if self.w.viewport == ViewportId::ROOT {
            self.recovery_dialog(&ctx);
        }
        self.go_to_line_dialog(&ctx);
        self.about_window(&ctx);
        self.help_window(&ctx);
        self.settings_window(&ctx);
        self.update_title(&ctx);

    }
}

#[cfg(test)]
#[path = "app_ui_tests.rs"]
mod ui_tests;
