//! Top-level application state and UI wiring.

use std::path::PathBuf;

use eframe::egui::{self, Align, Layout, RichText, ViewportCommand};

use crate::commands::{self, Command};
use crate::document::Document;
use crate::editor::{self, CursorInfo, EditorSettings};

/// An action that would discard the current document, held while we ask the
/// user what to do about unsaved changes.
#[derive(Clone, Debug)]
enum PendingAction {
    New,
    Open(Option<PathBuf>),
    Quit,
}

pub struct SlothApp {
    doc: Document,
    settings: EditorSettings,
    cursor: CursorInfo,
    pending: Option<PendingAction>,
    /// Transient message shown in the status bar (errors, "Saved", …).
    status: Option<String>,
    /// Set once the user has confirmed quitting, so the close goes through.
    allow_close: bool,
    last_title: String,
}

impl SlothApp {
    pub fn new(_cc: &eframe::CreationContext<'_>, initial_file: Option<PathBuf>) -> Self {
        let mut app = Self {
            doc: Document::new(),
            settings: EditorSettings::default(),
            cursor: CursorInfo::default(),
            pending: None,
            status: None,
            allow_close: false,
            last_title: String::new(),
        };
        if let Some(path) = initial_file {
            if path.exists() {
                app.load(path);
            } else {
                // `sloth newfile.txt` starts an empty doc that saves there.
                app.doc.path = Some(path);
            }
        }
        app
    }

    // ----- command dispatch -------------------------------------------------

    pub fn execute(&mut self, ctx: &egui::Context, cmd: Command) {
        match cmd {
            Command::New => self.request(ctx, PendingAction::New),
            Command::Open => self.request(ctx, PendingAction::Open(None)),
            Command::Save => {
                self.save();
            }
            Command::SaveAs => {
                self.save_as();
            }
            Command::Quit => self.request(ctx, PendingAction::Quit),
            Command::ToggleLineNumbers => {
                self.settings.show_line_numbers = !self.settings.show_line_numbers
            }
            Command::ToggleWordWrap => self.settings.word_wrap = !self.settings.word_wrap,
            Command::ZoomIn => self.settings.font_size = (self.settings.font_size + 1.0).min(72.0),
            Command::ZoomOut => self.settings.font_size = (self.settings.font_size - 1.0).max(6.0),
            Command::ZoomReset => self.settings.font_size = EditorSettings::DEFAULT_FONT_SIZE,
        }
    }

    fn is_checked(&self, cmd: Command) -> Option<bool> {
        match cmd {
            Command::ToggleLineNumbers => Some(self.settings.show_line_numbers),
            Command::ToggleWordWrap => Some(self.settings.word_wrap),
            _ => None,
        }
    }

    /// Run a destructive action now, or queue it behind an unsaved-changes prompt.
    fn request(&mut self, ctx: &egui::Context, action: PendingAction) {
        if self.doc.is_dirty() {
            self.pending = Some(action);
        } else {
            self.perform(ctx, action);
        }
    }

    fn perform(&mut self, ctx: &egui::Context, action: PendingAction) {
        match action {
            PendingAction::New => {
                self.doc = Document::new();
                self.status = None;
            }
            PendingAction::Open(path) => {
                let path = path.or_else(|| {
                    let mut dlg = rfd::FileDialog::new();
                    if let Some(dir) = self.current_dir() {
                        dlg = dlg.set_directory(dir);
                    }
                    dlg.pick_file()
                });
                if let Some(path) = path {
                    self.load(path);
                }
            }
            PendingAction::Quit => {
                self.allow_close = true;
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
        }
    }

    // ----- file operations --------------------------------------------------

    fn load(&mut self, path: PathBuf) {
        match Document::open(&path) {
            Ok(doc) => {
                self.doc = doc;
                self.status = None;
            }
            Err(e) => self.status = Some(format!("Could not open {}: {e}", path.display())),
        }
    }

    /// Returns true if the document was saved.
    fn save(&mut self) -> bool {
        if self.doc.path.is_none() {
            return self.save_as();
        }
        match self.doc.save() {
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

    fn save_as(&mut self) -> bool {
        let mut dlg = rfd::FileDialog::new().set_file_name(self.doc.display_name());
        if let Some(dir) = self.current_dir() {
            dlg = dlg.set_directory(dir);
        }
        let Some(path) = dlg.save_file() else {
            return false;
        };
        match self.doc.save_as(&path) {
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
        self.doc
            .path
            .as_ref()
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf())
    }

    // ----- UI pieces --------------------------------------------------------

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        // Check shortcuts with more modifiers first so ⌘⇧S isn't eaten by ⌘S.
        let mut cmds: Vec<_> = commands::ALL
            .iter()
            .filter_map(|&c| c.shortcut().map(|s| (c, s)))
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

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        egui::MenuBar::new().ui(ui, |ui| {
            for (title, items) in commands::MENUS {
                ui.menu_button(*title, |ui| {
                    for item in *items {
                        let Some(cmd) = *item else {
                            ui.separator();
                            continue;
                        };
                        let mut label = cmd.label().to_owned();
                        if let Some(true) = self.is_checked(cmd) {
                            label = format!("✔ {label}");
                        }
                        let mut button = egui::Button::new(label);
                        if let Some(sc) = cmd.shortcut() {
                            button = button.shortcut_text(ctx.format_shortcut(&sc));
                        }
                        if ui.add(button).clicked() {
                            self.execute(&ctx, cmd);
                            ui.close();
                        }
                    }
                });
            }
        });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let path = self
                .doc
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
                ui.label(format!("{} lines", self.doc.line_count()));
                if let Some((line, col)) = self.cursor.line_col {
                    ui.separator();
                    ui.label(format!("Ln {line}, Col {col}"));
                }
            });
        });
    }

    fn unsaved_changes_dialog(&mut self, ctx: &egui::Context) {
        let Some(action) = self.pending.clone() else {
            return;
        };
        let mut choice = None;
        egui::Modal::new(egui::Id::unique("unsaved-changes")).show(ctx, |ui| {
            ui.set_width(320.0);
            ui.heading(format!(
                "Save changes to \u{201c}{}\u{201d}?",
                self.doc.display_name()
            ));
            ui.label("Your changes will be lost if you don't save them.");
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    choice = Some(DialogChoice::Save);
                }
                if ui.button("Don't Save").clicked() {
                    choice = Some(DialogChoice::Discard);
                }
                if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    choice = Some(DialogChoice::Cancel);
                }
            });
        });

        match choice {
            Some(DialogChoice::Save) => {
                self.pending = None;
                if self.save() {
                    self.perform(ctx, action);
                }
            }
            Some(DialogChoice::Discard) => {
                self.pending = None;
                self.perform(ctx, action);
            }
            Some(DialogChoice::Cancel) => self.pending = None,
            None => {}
        }
    }

    fn update_title(&mut self, ctx: &egui::Context) {
        let title = format!(
            "{}{} — Sloth",
            if self.doc.is_dirty() { "• " } else { "" },
            self.doc.display_name()
        );
        if title != self.last_title {
            ctx.send_viewport_cmd(ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }
    }
}

enum DialogChoice {
    Save,
    Discard,
    Cancel,
}

fn modifier_count(m: egui::Modifiers) -> u8 {
    m.alt as u8 + m.ctrl as u8 + m.shift as u8 + m.mac_cmd as u8 + m.command as u8
}

impl eframe::App for SlothApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // Window close button / ⌘Q from the OS: intercept if there are unsaved changes.
        if ctx.input(|i| i.viewport().close_requested())
            && !self.allow_close
            && self.doc.is_dirty()
        {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.pending = Some(PendingAction::Quit);
        }

        // Files dragged onto the window.
        if let Some(path) = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).find(|p| !p.as_os_str().is_empty())) {
            self.request(&ctx, PendingAction::Open(Some(path)));
        }

        if self.pending.is_none() {
            self.handle_shortcuts(&ctx);
        }

        egui::Panel::top("menu").show(ui, |ui| self.menu_bar(ui));
        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(ui.visuals().extreme_bg_color))
            .show(ui, |ui| {
                self.cursor = editor::show(ui, &mut self.doc.text, &self.settings);
            });

        self.unsaved_changes_dialog(&ctx);
        self.update_title(&ctx);
    }
}
