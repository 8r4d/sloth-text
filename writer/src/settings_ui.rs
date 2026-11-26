//! Writer's settings (⌘,): the prose font, its size, line spacing and text
//! width, the colour theme, and smart dashes. Saved between launches.

use eframe::egui::{self, FontId, RichText};
use serde::{Deserialize, Serialize};
use sloth_core::editor::EditorSettings;
use sloth_core::fonts;
use sloth_core::theme::{self, ThemeChoice};

use super::WriterApp;

/// Where the settings are kept between launches.
pub(super) const SETTINGS_KEY: &str = "writer_settings";

/// Typewriter scrolling: the line being typed kept in the middle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Typewriter {
    Never,
    #[default]
    FocusMode,
    Always,
}

impl Typewriter {
    const ALL: [Typewriter; 3] = [Typewriter::FocusMode, Typewriter::Always, Typewriter::Never];

    fn label(self) -> &'static str {
        match self {
            Typewriter::Never => "Never",
            Typewriter::FocusMode => "In Focus mode",
            Typewriter::Always => "Always",
        }
    }
}

/// How the start screen shows recent projects.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum StartView {
    /// A tile each, most recent first.
    #[default]
    Tiles,
    /// A card file to flip through, A to Z (see card_file_ui.rs).
    CardFile,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WriterSettings {
    /// The prose font; `None` for the default reading face.
    pub font: Option<String>,
    pub font_size: f32,
    /// Line height as a multiple of the font's usual one.
    pub line_spacing: f32,
    /// Extra space after each paragraph, in lines (0 for none).
    pub paragraph_spacing: f32,
    /// How far each paragraph's first line is indented, as a multiple of
    /// the font size (0 for none).
    pub first_line_indent: f32,
    /// The widest lines get, in points; `None` for the full width.
    pub text_width: Option<f32>,
    /// Typing `--` makes an em dash.
    pub smart_dashes: bool,
    /// Show *italics*, **bold**, headings and notes styled as you type.
    pub show_markdown: bool,
    /// Hide the `**`, `*`, `#` and `>` except in the paragraph being edited.
    pub hide_markdown: bool,
    /// Copy and cut as formatted text (bold, italics) rather than Markdown.
    pub copy_formatted: bool,
    /// Underline misspelled words (the Mac's spell checker).
    pub spell_check: bool,
    /// When to keep the line being typed in the middle of the screen.
    pub typewriter: Typewriter,
    /// How tall a document's notes panel is (dragged by its bottom edge).
    pub notes_height: f32,
    /// Back up automatically: when a project closes, and once a day.
    pub auto_backup: bool,
    /// How many automatic backups to keep, per project.
    pub backup_keep: u32,
    /// Where automatic backups go (each project in a folder of its own);
    /// `None` for Backups in Writer's data folder (see places.rs).
    pub auto_backup_folder: Option<String>,
    /// Where the last backup went, to offer it next time.
    pub backup_folder: Option<String>,
    /// The web helper's address (its token is in the Keychain).
    pub helper_url: String,
    pub theme: ThemeChoice,
    /// A small moment when the day's goal is reached (see goal_ui.rs).
    pub celebrate_goals: bool,
    /// In Focus mode, fade all but the paragraph being written in.
    pub focus_paragraph: bool,
    /// Mark echoes and watch-list words in the open document (⌥⌘R).
    pub show_repetition: bool,
    /// Words and phrases to underline when they come up, commas between.
    pub watch_words: String,
    /// Your own common words, commas between: left out of the most used,
    /// and never echoes (as the built-in common words aren't).
    pub common_words: String,
    /// Recent projects on the start screen: tiles or the card file.
    pub start_view: StartView,
    /// A fact about your work under the start screen's greeting.
    pub show_fact: bool,
    /// Focus mode's progress bar and a bin per chapter.
    pub show_bins: bool,
    /// The elevator: a panel sliding away when a project opens or closes.
    pub elevator: bool,
    /// With a Severance theme: scanlines and a vignette, like an old screen.
    pub screen_glow: bool,
    /// This computer's own name for itself, for its sync records (see
    /// sync.rs). Made on first use.
    pub device_id: String,
    /// Who you are, for compiling (see compile.rs).
    pub profile: crate::compile::AuthorProfile,
}

impl Default for WriterSettings {
    fn default() -> Self {
        Self {
            font: None,
            font_size: 18.0,
            line_spacing: 1.3,
            paragraph_spacing: 0.0,
            first_line_indent: 0.0,
            text_width: Some(680.0),
            smart_dashes: true,
            show_markdown: true,
            hide_markdown: false,
            copy_formatted: false,
            spell_check: true,
            typewriter: Typewriter::FocusMode,
            notes_height: 140.0,
            // Not while testing: tests close projects all the time.
            auto_backup: !cfg!(test),
            backup_keep: 10,
            auto_backup_folder: None,
            backup_folder: None,
            helper_url: String::new(),
            celebrate_goals: true,
            focus_paragraph: true,
            show_repetition: false,
            watch_words: crate::repetition::default_watch_list().join(", "),
            common_words: String::new(),
            start_view: StartView::Tiles,
            show_fact: true,
            show_bins: true,
            // Not while testing: screenshots would catch it mid-ride.
            elevator: !cfg!(test),
            screen_glow: false,
            device_id: String::new(),
            profile: Default::default(),
            theme: ThemeChoice::default(),
        }
    }
}

impl WriterSettings {
    /// Where automatic backups go.
    pub fn backups_root(&self) -> std::path::PathBuf {
        self.auto_backup_folder.as_deref().map_or_else(crate::backup::default_root, std::path::PathBuf::from)
    }

    /// The editor's settings: prose, not code.
    pub fn editor(&self) -> EditorSettings {
        EditorSettings {
            show_line_numbers: false,
            word_wrap: true,
            font_size: self.font_size,
            auto_indent: false,
            highlight_current_line: false,
            syntax_highlighting: false,
            smart_dashes: self.smart_dashes,
            prose: true,
            line_spacing: self.line_spacing,
            paragraph_spacing: self.paragraph_spacing,
            first_line_indent: self.first_line_indent,
            max_text_width: self.text_width,
            markdown: self.show_markdown,
            hide_markers: self.hide_markdown,
            copy_formatted: self.copy_formatted,
            spell_check: self.spell_check,
            split_menu: true,
            comments: true,
            typewriter: self.typewriter == Typewriter::Always,
            ..EditorSettings::default()
        }
    }

    /// The prose font in use: the chosen one, or the default reading face.
    pub fn font_in_use(&self) -> Option<String> {
        // The Newsroom theme writes on a typewriter, unless a font's chosen.
        let typewriter = || {
            (self.theme == theme::ThemeChoice::Newsroom)
                .then(|| ["American Typewriter", "Courier Prime", "Courier New", "Courier"].into_iter().find(|f| fonts::installed(f)))
                .flatten()
                .map(str::to_owned)
        };
        self.font.clone().or_else(typewriter).or_else(|| fonts::default_prose_font().map(str::to_owned))
    }
}

impl WriterApp {
    /// Put the settings into effect: the editor's settings, fonts and theme.
    pub(super) fn apply_settings(&mut self, ctx: &egui::Context) {
        self.editor = self.settings.editor();
        fonts::apply(ctx, None, self.settings.font_in_use().as_deref(), 4);
        self.settings.theme.apply(ctx);
    }

    pub(super) fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.settings_open {
            return;
        }
        // When the open project was last backed up automatically.
        let last_backup = self.project.as_ref().and_then(|p| {
            let (_, made) = crate::backup::newest(&crate::backup::auto_folder(&self.settings.backups_root(), &p.root))?;
            Some(chrono::DateTime::<chrono::Local>::from(made).format("%-d %B %Y, %H:%M").to_string())
        });
        let before = self.settings.clone();
        let settings = &mut self.settings;
        let filter = &mut self.font_filter;
        let token = &mut self.helper_token;
        let mut token_changed = false;
        let mut show_about = false;
        let mut open = true;
        let mut show_you = std::mem::take(&mut self.settings_page_you);
        egui::Window::new("App Settings")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(420.0)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .show(ctx, |ui| {
                // The settings scroll rather than run off a small screen;
                // Restore Defaults stays in view below them.
                let height = (ctx.content_rect().height() * 0.62).max(200.0);
                egui::ScrollArea::vertical().max_height(height).show(ui, |ui| {
                    // Every row's label in one column, every control in the next.
                    const LABEL_WIDTH: f32 = 96.0;
                    let row = |ui: &mut egui::Ui, label: &str, add: &mut dyn FnMut(&mut egui::Ui)| {
                        ui.horizontal_top(|ui| {
                            let layout = egui::Layout::left_to_right(egui::Align::Center);
                            ui.allocate_ui_with_layout(egui::vec2(LABEL_WIDTH, 20.0), layout, |ui| {
                                ui.set_min_width(LABEL_WIDTH);
                                ui.label(label);
                            });
                            ui.vertical(|ui| add(ui));
                        });
                        ui.add_space(4.0);
                    };

                    group(ui, "Text");
                    row(ui, "Font", &mut |ui| {
                        let default_name = format!("Default ({})", fonts::default_prose_font().unwrap_or("built-in"));
                        let shown = settings.font.clone().unwrap_or_else(|| default_name.clone());
                        egui::ComboBox::from_id_salt("prose-font").selected_text(shown).height(320.0).width(240.0).show_ui(ui, |ui| {
                            ui.add(egui::TextEdit::singleline(filter).hint_text("Filter").desired_width(200.0));
                            ui.selectable_value(&mut settings.font, None, default_name);
                            let wanted = filter.to_lowercase();
                            let families = fonts::all_families();
                            if families.is_empty() {
                                ui.weak("Looking for fonts\u{2026}");
                            }
                            for family in families.iter().filter(|f| f.to_lowercase().contains(&wanted)) {
                                ui.selectable_value(&mut settings.font, Some(family.clone()), family);
                            }
                        });
                    });
                    row(ui, "Size", &mut |ui| {
                        ui.add(egui::Slider::new(&mut settings.font_size, 12.0..=32.0).step_by(1.0).suffix(" pt"));
                    });
                    row(ui, "Line spacing", &mut |ui| {
                        ui.add(egui::Slider::new(&mut settings.line_spacing, 1.0..=2.2).step_by(0.05).suffix("\u{d7}"));
                    });
                    row(ui, "Paragraphs", &mut |ui| {
                    ui.add(
                        egui::Slider::new(&mut settings.paragraph_spacing, 0.0..=2.0)
                            .step_by(0.1)
                            .custom_formatter(|v, _| if v == 0.0 { "No space".into() } else { format!("{v:.1} line") }),
                    )
                    .on_hover_text("Extra space after each paragraph (each line you end with Return)");
                });
                row(ui, "First-line indent", &mut |ui| {
                    ui.add(
                        egui::Slider::new(&mut settings.first_line_indent, 0.0..=4.0)
                            .step_by(0.25)
                            .custom_formatter(|v, _| if v == 0.0 { "None".into() } else { format!("{v:.2} em") }),
                    )
                    .on_hover_text(
                        "Start each paragraph a little way in. 1 em is the font size, about the width of an M. \
                         Only how it looks: no spaces are added to the text.",
                    );
                });
                row(ui, "Line length", &mut |ui| {
                        let mut limited = settings.text_width.is_some();
                        if ui.checkbox(&mut limited, "Keep lines to a comfortable length").changed() {
                            settings.text_width = limited.then_some(680.0);
                        }
                        if let Some(width) = &mut settings.text_width {
                            ui.add(egui::Slider::new(width, 400.0..=1200.0).step_by(20.0).suffix(" pt"));
                        }
                    });
                    // A sample in the chosen font, size and spacing.
                    egui::Frame::new()
                        .fill(ui.visuals().extreme_bg_color)
                        .corner_radius(6.0)
                        .inner_margin(egui::Margin::symmetric(12, 8))
                        .show(ui, |ui| {
                            let font = FontId::new(settings.font_size, fonts::prose_family());
                            let height = settings.font_size * 1.2 * settings.line_spacing;
                            let color = ui.visuals().text_color();
                            let indent = settings.first_line_indent * settings.font_size;
                            let sample = |text: &str| {
                                let format = egui::TextFormat {
                                    font_id: font.clone(),
                                    line_height: Some(height),
                                    color,
                                    ..Default::default()
                                };
                                let mut job = egui::text::LayoutJob::single_section(text.to_owned(), format);
                                job.sections[0].leading_space = indent;
                                job
                            };
                            ui.spacing_mut().item_spacing.y = 0.0;
                            ui.add(egui::Label::new(sample("The snow came early that year\u{2014}three weeks before the first frost was due.")));
                            ui.add_space(settings.paragraph_spacing * height);
                            ui.add(egui::Label::new(sample("By morning the harbour had frozen.")));
                        });

                    group(ui, "Writing");
                    ui.checkbox(&mut settings.smart_dashes, "Two hyphens make an em dash (\u{2014})");
                    ui.checkbox(&mut settings.show_markdown, "Show *italics*, **bold** and headings as you type");
                    ui.indent("markdown-options", |ui| {
                        ui.add_enabled_ui(settings.show_markdown, |ui| {
                            ui.checkbox(&mut settings.hide_markdown, "Hide the ** and # marks except where you're typing")
                                .on_hover_text(
                                    "The marks show again in the paragraph the cursor is in, so you can still change them.",
                                );
                            ui.checkbox(&mut settings.copy_formatted, "Copy as formatted text")
                                .on_hover_text(
                                    "Copy and Cut give other apps bold and italics, not ** and *. \
                                     Notes are left out. Pasting back into Writer keeps the Markdown.",
                                );
                        });
                    });
                    ui.checkbox(&mut settings.spell_check, "Check spelling as you type")
                        .on_hover_text("Uses your Mac's spelling language and learned words. Right-click a word for suggestions.");
                    ui.checkbox(&mut settings.show_bins, "In Focus mode, show progress: the manuscript's, and a bin per chapter")
                        .on_hover_text("A bin fills towards its chapter's share of the target, and its lid lifts when it's full.");
                    ui.checkbox(&mut settings.focus_paragraph, "In Focus mode, fade all but the paragraph you're writing")
                        .on_hover_text(sloth_core::keys::shown("The light moves with the cursor. Focus mode is \u{2325}\u{2318}F."));
                    ui.checkbox(&mut settings.show_repetition, sloth_core::keys::shown("Highlight repeated words (\u{2325}\u{2318}R)"))
                        .on_hover_text("A word used again within about 100 words is tinted; words you watch for are underlined.");
                    row(ui, "Watch for", &mut |ui| {
                        ui.add(egui::TextEdit::singleline(&mut settings.watch_words).desired_width(320.0).hint_text("just, suddenly, began to"));
                    });
                    row(ui, "Common words", &mut |ui| {
                        ui.add(egui::TextEdit::singleline(&mut settings.common_words).desired_width(320.0).hint_text("people, really, maybe"))
                            .on_hover_text("Left out of Statistics' most used words, and never tinted as repeats, like the little words Writer already leaves out. Commas between. In Statistics, right-click a word to add it.");
                    });
                    ui.checkbox(&mut settings.celebrate_goals, "Celebrate the day's goal, streaks and milestones")
                        .on_hover_text("Once a day: the goal ring pulses, and the sloth says so.");
                    ui.add_space(4.0);
                    row(ui, "Typewriter", &mut |ui| {
                        egui::ComboBox::from_id_salt("typewriter").selected_text(settings.typewriter.label()).width(160.0).show_ui(ui, |ui| {
                            for choice in Typewriter::ALL {
                                ui.selectable_value(&mut settings.typewriter, choice, choice.label());
                            }
                        });
                        ui.weak("Keeps the line you're typing in the middle of the screen.");
                    });

                    group(ui, "Appearance");
                    row(ui, "Theme", &mut |ui| {
                        theme::picker(ui, "theme", &mut settings.theme, 180.0);
                    });
                    if settings.theme.has_glow() {
                        ui.checkbox(&mut settings.screen_glow, "Old screen glow")
                            .on_hover_text("Faint scanlines and darker edges, like an old monitor. With the Severance and Fallout themes.");
                    }
                    ui.checkbox(&mut settings.elevator, "Slide between the start screen and a project")
                        .on_hover_text("A panel with the project's name slides up and away as it opens. Not with reduced motion.");
                    ui.checkbox(&mut settings.show_fact, "A fact about your work on the start screen")
                        .on_hover_text("One a day, from your writing statistics. Click it for another.");

                    group(ui, "Backups");
                    ui.checkbox(&mut settings.auto_backup, "Back up automatically when a project closes, and once a day");
                    row(ui, "Keep", &mut |ui| {
                        ui.add(egui::DragValue::new(&mut settings.backup_keep).range(1..=100).suffix(" backups of each project"));
                    });
                    row(ui, "Folder", &mut |ui| {
                        let root = settings.backups_root();
                        ui.add(egui::Label::new(RichText::new(root.display().to_string()).small()).truncate())
                            .on_hover_text(root.display().to_string());
                        ui.horizontal(|ui| {
                            if ui.button("Choose\u{2026}").clicked()
                                && let Some(folder) = rfd::FileDialog::new().set_title("Back Up Projects To…").set_directory(&root).pick_folder()
                            {
                                settings.auto_backup_folder = Some(folder.to_string_lossy().into_owned());
                            }
                            if ui.button(SHOW_FOLDER).clicked() {
                                let _ = std::fs::create_dir_all(&root);
                                let _ = opener::open(&root);
                            }
                            if settings.auto_backup_folder.is_some() && ui.button("Use the Default").clicked() {
                                settings.auto_backup_folder = None;
                            }
                        });
                        if let Some(made) = &last_backup {
                            ui.weak(format!("This project's last automatic backup: {made}"));
                        }
                    });

                    group(ui, "Web Helper");
                    row(ui, "Address", &mut |ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut settings.helper_url)
                                .hint_text("https://writer.example.com/")
                                .desired_width(260.0),
                        );
                    });
                    row(ui, "Token", &mut |ui| {
                        let field = ui.add(egui::TextEdit::singleline(token).password(true).hint_text("Paste it here").desired_width(260.0));
                        // Saved to the Keychain on leaving the field, not per keystroke.
                        token_changed = field.lost_focus();
                        ui.weak("Make one on the helper's Account page.");
                    });

                    group(ui, "You");
                    let you = ui.weak("For compiling: on title pages, the copyright page and About the Author, in every project (a project can give its own author in Compile).");
                    if std::mem::take(&mut show_you) {
                        you.scroll_to_me(Some(egui::Align::TOP));
                    }
                    let profile = &mut settings.profile;
                    row(ui, "Name", &mut |ui| {
                        ui.add(egui::TextEdit::singleline(&mut profile.name).hint_text("Your own name").desired_width(260.0));
                    });
                    row(ui, "Byline", &mut |ui| {
                        ui.add(egui::TextEdit::singleline(&mut profile.byline).hint_text("The name you write under, if not your own").desired_width(260.0));
                    });
                    row(ui, "Contact", &mut |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut profile.contact)
                                .hint_text("Address, email, phone, agent\u{2026} (one per line)")
                                .desired_rows(3)
                                .desired_width(260.0),
                        )
                        .on_hover_text("On a manuscript's title page, under your name.");
                    });
                    row(ui, "Website", &mut |ui| {
                        ui.add(egui::TextEdit::singleline(&mut profile.website).hint_text("example.com").desired_width(260.0));
                    });
                    row(ui, "About you", &mut |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut profile.bio)
                                .hint_text("A few lines for About the Author, at the end of a book")
                                .desired_rows(3)
                                .desired_width(260.0),
                        );
                    });

                });
                ui.add_space(6.0);
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Restore Defaults").on_hover_text("Everything but who you are").clicked() {
                        // Not who you are, nor this computer's name for syncing.
                        let (profile, device) = (settings.profile.clone(), settings.device_id.clone());
                        *settings = WriterSettings { profile, device_id: device, ..WriterSettings::default() };
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.link(format!("Sloth Writer {}", crate::version::VERSION)).on_hover_text("About Sloth Writer").clicked() {
                            show_about = true;
                        }
                    });
                });
            });
        if show_about {
            self.help = Some(super::about_ui::HelpPage::About);
        }
        if self.settings != before {
            self.apply_settings(ctx);
        }
        if token_changed && !cfg!(test) && !crate::keychain::save_token(&self.helper_token) {
            let place = if cfg!(target_os = "macos") { " to the Keychain" } else if cfg!(windows) { " to Credential Manager" } else { "" };
            self.error = Some(format!("Couldn't save the helper token{place}."));
        }
        if token_changed {
            self.sync_now = true;
        }
        if !open {
            self.settings_open = false;
        }
    }
}

/// The button that opens a folder in the system's file manager.
pub(super) const SHOW_FOLDER: &str = if cfg!(target_os = "macos") {
    "Show in Finder"
} else if cfg!(windows) {
    "Show in Explorer"
} else {
    "Open Folder"
};

/// A heading for a group of settings.
pub(super) fn group(ui: &mut egui::Ui, title: &str) {
    // No rule above the first group.
    if !matches!(title, "Text" | "Project") {
        ui.add_space(4.0);
        ui.separator();
    }
    ui.add_space(2.0);
    ui.label(RichText::new(title).size(15.0).strong());
    ui.add_space(4.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn older_saved_settings_fill_in_new_ones() {
        let saved: WriterSettings = ron::from_str("(font_size: 20.0)").unwrap();
        assert_eq!(saved.font_size, 20.0);
        assert!(saved.smart_dashes);
        assert_eq!(saved.text_width, Some(680.0));
        assert_eq!(saved.first_line_indent, 0.0, "no indent until asked for");
        assert!(!saved.hide_markdown && !saved.copy_formatted);
    }

    #[test]
    fn the_editor_follows_the_settings() {
        let s = WriterSettings { font_size: 21.0, smart_dashes: false, text_width: None, first_line_indent: 1.5, ..Default::default() };
        let e = s.editor();
        assert_eq!(e.first_line_indent, 1.5);
        let marks = WriterSettings { hide_markdown: true, copy_formatted: true, ..Default::default() }.editor();
        assert!(marks.hide_markers && marks.copy_formatted);
        assert!(e.prose && e.word_wrap && !e.show_line_numbers);
        assert_eq!((e.font_size, e.smart_dashes, e.max_text_width), (21.0, false, None));
    }
}
