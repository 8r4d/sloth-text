//! Top-level application state and UI wiring.

use std::ops::Range;
use std::path::PathBuf;

use eframe::egui::{
    self, Align, Key, Layout, Margin, Modifiers, RichText, ScrollArea, ViewportCommand,
};

use crate::commands::{self, Command, MenuEntry};
use crate::document::Document;
use crate::editor::{self, CursorInfo, EditorSettings, Highlights};
use crate::file_types;
use crate::find::{self, FindAction, FindState};
use crate::tabs::{self, TabBarAction, TabId, Tabs};
use crate::theme::ThemeChoice;
use crate::version;

// Keys for settings remembered between launches.
const KEY_THEME: &str = "theme";
const KEY_LINE_NUMBERS: &str = "line_numbers";
const KEY_WORD_WRAP: &str = "word_wrap";
const KEY_FONT_SIZE: &str = "font_size";

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
    settings: EditorSettings,
    theme: ThemeChoice,
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
    #[cfg(target_os = "macos")]
    native_menu: crate::native_menu::NativeMenu,
}

impl SlothApp {
    pub fn new(cc: &eframe::CreationContext<'_>, files: Vec<PathBuf>) -> Self {
        // egui's iOS-style rubber-banding lets the text be dragged far past
        // its ends with a trackpad, which feels like the page is floating.
        // macOS already supplies trackpad momentum, so turn it off.
        cc.egui_ctx.all_styles_mut(|style| {
            style.spacing.scroll.kinetic.rubber_band = false;
        });

        let mut app = Self {
            tabs: Tabs::new(),
            settings: EditorSettings::default(),
            theme: ThemeChoice::default(),
            find: FindState::default(),
            cursor: CursorInfo::default(),
            pending: None,
            status: None,
            allow_close: false,
            last_title: String::new(),
            reveal: None,
            show_about: false,
            help: None,
            #[cfg(target_os = "macos")]
            native_menu: crate::native_menu::NativeMenu::install(&cc.egui_ctx),
        };
        if let Some(storage) = cc.storage {
            app.restore_settings(storage);
        }
        app.theme.apply(&cc.egui_ctx);

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
            Command::FindNext => self.find_step(ctx, Direction::Next),
            Command::FindPrevious => self.find_step(ctx, Direction::Previous),
            Command::UseSelectionForFind => {
                if let Some(text) = self.selected_text(ctx) {
                    self.find.query = text;
                    self.find.visible = true;
                    self.find.current = None;
                }
            }

            Command::ToggleLineNumbers => {
                self.settings.show_line_numbers = !self.settings.show_line_numbers
            }
            Command::ToggleWordWrap => self.settings.word_wrap = !self.settings.word_wrap,
            Command::ZoomIn => self.settings.font_size = (self.settings.font_size + 1.0).min(72.0),
            Command::ZoomOut => self.settings.font_size = (self.settings.font_size - 1.0).max(6.0),
            Command::ZoomReset => self.settings.font_size = EditorSettings::DEFAULT_FONT_SIZE,
            Command::SetTheme(theme) => {
                self.theme = theme;
                theme.apply(ctx);
            }

            Command::About => self.show_about = true,
            Command::Help => self.help = Some(HelpPage::Shortcuts),
            Command::ReleaseNotes => self.help = Some(HelpPage::ReleaseNotes),
        }
    }

    fn is_checked(&self, cmd: Command) -> Option<bool> {
        match cmd {
            Command::ToggleLineNumbers => Some(self.settings.show_line_numbers),
            Command::ToggleWordWrap => Some(self.settings.word_wrap),
            Command::SetTheme(theme) => Some(self.theme == theme),
            _ => None,
        }
    }

    fn restore_settings(&mut self, storage: &dyn eframe::Storage) {
        let get = |key| storage.get_string(key);
        if let Some(theme) = get(KEY_THEME).and_then(|t| ThemeChoice::from_key(&t)) {
            self.theme = theme;
        }
        if let Some(v) = get(KEY_LINE_NUMBERS).and_then(|v| v.parse().ok()) {
            self.settings.show_line_numbers = v;
        }
        if let Some(v) = get(KEY_WORD_WRAP).and_then(|v| v.parse().ok()) {
            self.settings.word_wrap = v;
        }
        if let Some(v) = get(KEY_FONT_SIZE).and_then(|v| v.parse().ok()) {
            self.settings.font_size = v;
        }
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
                self.tabs.open(doc);
                self.find.current = None;
                self.status = None;
            }
            Err(e) => self.status = Some(format!("Could not open {}: {e}", path.display())),
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
        self.report_save(result)
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
        self.report_save(result)
    }

    fn report_save(&mut self, result: std::io::Result<()>) -> bool {
        match result {
            Ok(()) => {
                self.status = Some("Saved".into());
                true
            }
            Err(e) => {
                self.status = Some(format!("Save failed: {e}"));
                false
            }
        }
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

    // ----- find & replace ---------------------------------------------------

    fn matches(&self) -> Vec<Range<usize>> {
        if !self.find.is_searching() {
            return Vec::new();
        }
        find::find_all(&self.tabs.active().doc.text, &self.find.query, self.find.options)
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
        let tab = self.tabs.active_mut();
        tab.doc.text.replace_range(current.clone(), &self.find.replacement);
        let after = current.start + self.find.replacement.len();
        let after_char = find::byte_to_char(&tab.doc.text, after);
        let id = tab.id;
        editor::set_selection(ctx, id, after_char..after_char);
        self.find.current = None;
        self.find_step(ctx, Direction::Next);
    }

    fn replace_all(&mut self, ctx: &egui::Context) {
        let tab = self.tabs.active_mut();
        let (text, count) = find::replace_all(
            &tab.doc.text,
            &self.find.query,
            &self.find.replacement,
            self.find.options,
        );
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
            FindAction::Next => self.find_step(ctx, Direction::Next),
            FindAction::Previous => self.find_step(ctx, Direction::Previous),
            FindAction::Replace => self.replace_current(ctx),
            FindAction::ReplaceAll => self.replace_all(ctx),
            FindAction::Close => {
                self.find.close();
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
                    if ui.add(button).clicked() {
                        self.execute(ctx, cmd);
                        ui.close();
                    }
                }
            }
        }
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        let doc = &self.tabs.active().doc;
        ui.horizontal(|ui| {
            let path = doc
                .path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "Untitled".into());
            ui.label(RichText::new(path).weak());
            if let Some(msg) = &self.status {
                ui.separator();
                ui.label(msg);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(format!("{} lines", doc.line_count()));
                if let Some((line, col)) = self.cursor.line_col {
                    ui.separator();
                    ui.label(format!("Ln {line}, Col {col}"));
                }
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
                    HelpPage::Shortcuts => shortcuts_page(ui),
                    HelpPage::ReleaseNotes => release_notes_page(ui),
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

/// Shortcut reference, generated from the menus so it never goes stale.
fn shortcuts_page(ui: &mut egui::Ui) {
    fn collect(entries: &[MenuEntry], out: &mut Vec<Command>) {
        for entry in entries {
            match entry {
                MenuEntry::Item(cmd) if cmd.shortcut().is_some() => out.push(*cmd),
                MenuEntry::Submenu(_, children) => collect(children, out),
                _ => {}
            }
        }
    }
    let ctx = ui.ctx().clone();
    // One grid for every menu so the shortcut column lines up.
    egui::Grid::new("shortcuts")
        .num_columns(2)
        .spacing([40.0, 4.0])
        .show(ui, |ui| {
            for (title, entries) in commands::MENUS {
                let mut cmds = Vec::new();
                collect(entries, &mut cmds);
                if cmds.is_empty() {
                    continue;
                }
                ui.add_space(6.0);
                ui.end_row();
                ui.strong(*title);
                ui.end_row();
                for cmd in cmds {
                    ui.label(cmd.label().trim_end_matches('…'));
                    let sc = cmd.shortcut().expect("filtered above");
                    ui.monospace(ctx.format_shortcut(&sc));
                    ui.end_row();
                }
            }
        });
    ui.add_space(10.0);
    ui.strong("Tips");
    for tip in [
        "Drag files onto the window to open them.",
        "Middle-click a tab to close it.",
        "In the find bar, Enter finds the next match and Shift-Enter the previous one.",
        "Theme, line numbers, word wrap and text size are remembered between launches.",
    ] {
        ui.label(format!("•  {tip}"));
    }
}

/// CHANGELOG.md, rendered simply.
fn release_notes_page(ui: &mut egui::Ui) {
    for line in version::CHANGELOG.lines() {
        if line.starts_with("# ") {
            continue; // the document title
        } else if let Some(heading) = line.strip_prefix("## ") {
            ui.add_space(8.0);
            ui.heading(heading);
        } else if let Some(item) = line.strip_prefix("- ") {
            ui.label(format!("•  {item}"));
        } else if !line.trim().is_empty() {
            ui.label(RichText::new(line).weak());
        }
    }
}

impl eframe::App for SlothApp {
    /// Don't save egui's internal widget state (cursor positions and so on)
    /// between launches: it would be applied to whatever tabs happen to get
    /// the same ids next time. Our own settings are saved in `save` below.
    fn persist_egui_memory(&self) -> bool {
        false
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        storage.set_string(KEY_THEME, self.theme.key());
        storage.set_string(KEY_LINE_NUMBERS, self.settings.show_line_numbers.to_string());
        storage.set_string(KEY_WORD_WRAP, self.settings.word_wrap.to_string());
        storage.set_string(KEY_FONT_SIZE, self.settings.font_size.to_string());
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
        for path in dropped {
            self.open_path(path);
        }

        // On macOS, shortcuts are routed through the native menu bar.
        #[cfg(target_os = "macos")]
        for cmd in self.native_menu.drain() {
            if self.pending.is_none() {
                self.execute(&ctx, cmd);
            }
        }
        #[cfg(not(target_os = "macos"))]
        if self.pending.is_none() {
            self.handle_shortcuts(&ctx);
        }

        if self.find.visible
            && self.pending.is_none()
            && !self.show_about
            && ctx.input(|i| i.key_pressed(Key::Escape))
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
            let action = egui::Panel::top("find")
                .show(ui, |ui| find::find_bar(ui, &mut self.find, matches.len(), current_index))
                .inner;
            if let Some(action) = action {
                self.handle_find_action(&ctx, action);
            }
        }

        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));

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
                    editor::show(ui, tab.id, &mut tab.doc.text, &self.settings, highlights, reveal);
            });

        self.unsaved_changes_dialog(&ctx);
        self.about_window(&ctx);
        self.help_window(&ctx);
        self.update_title(&ctx);

        #[cfg(target_os = "macos")]
        for cmd in commands::all() {
            if let Some(checked) = self.is_checked(cmd) {
                self.native_menu.set_checked(cmd, checked);
            }
        }
    }
}
