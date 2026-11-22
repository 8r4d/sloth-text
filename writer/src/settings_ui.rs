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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WriterSettings {
    /// The prose font; `None` for the default reading face.
    pub font: Option<String>,
    pub font_size: f32,
    /// Line height as a multiple of the font's usual one.
    pub line_spacing: f32,
    /// The widest lines get, in points; `None` for the full width.
    pub text_width: Option<f32>,
    /// Typing `--` makes an em dash.
    pub smart_dashes: bool,
    /// Show *italics*, **bold**, headings and notes styled as you type.
    pub show_markdown: bool,
    /// The iPad helper's address (its token is in the Keychain).
    pub helper_url: String,
    pub theme: ThemeChoice,
}

impl Default for WriterSettings {
    fn default() -> Self {
        Self {
            font: None,
            font_size: 18.0,
            line_spacing: 1.3,
            text_width: Some(680.0),
            smart_dashes: true,
            show_markdown: true,
            helper_url: String::new(),
            theme: ThemeChoice::default(),
        }
    }
}

impl WriterSettings {
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
            max_text_width: self.text_width,
            markdown: self.show_markdown,
            ..EditorSettings::default()
        }
    }

    /// The prose font in use: the chosen one, or the default reading face.
    pub fn font_in_use(&self) -> Option<String> {
        self.font.clone().or_else(|| fonts::default_prose_font().map(str::to_owned))
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
        let before = self.settings.clone();
        let settings = &mut self.settings;
        let filter = &mut self.font_filter;
        let token = &mut self.helper_token;
        let mut token_changed = false;
        let mut open = true;
        egui::Window::new("Settings")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(380.0)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .show(ctx, |ui| {
                egui::Grid::new("writer-settings").num_columns(2).spacing([12.0, 10.0]).show(ui, |ui| {
                    ui.label("Font");
                    let default_name = format!("Default ({})", fonts::default_prose_font().unwrap_or("built-in"));
                    let shown = settings.font.clone().unwrap_or_else(|| default_name.clone());
                    egui::ComboBox::from_id_salt("prose-font").selected_text(shown).height(320.0).width(220.0).show_ui(ui, |ui| {
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
                    ui.end_row();

                    ui.label("Size");
                    ui.add(egui::Slider::new(&mut settings.font_size, 12.0..=32.0).step_by(1.0).suffix(" pt"));
                    ui.end_row();

                    ui.label("Line spacing");
                    ui.add(egui::Slider::new(&mut settings.line_spacing, 1.0..=2.2).step_by(0.05).suffix("\u{d7}"));
                    ui.end_row();

                    ui.label("Text width");
                    ui.vertical(|ui| {
                        let mut limited = settings.text_width.is_some();
                        if ui.checkbox(&mut limited, "Keep lines to a comfortable length").changed() {
                            settings.text_width = limited.then_some(680.0);
                        }
                        if let Some(width) = &mut settings.text_width {
                            ui.add(egui::Slider::new(width, 400.0..=1200.0).step_by(20.0).suffix(" pt"));
                        }
                    });
                    ui.end_row();

                    ui.label("Theme");
                    egui::ComboBox::from_id_salt("theme").selected_text(settings.theme.label()).show_ui(ui, |ui| {
                        for choice in theme::ALL {
                            ui.selectable_value(&mut settings.theme, *choice, choice.label());
                        }
                    });
                    ui.end_row();

                    ui.label("Typing");
                    ui.vertical(|ui| {
                        ui.checkbox(&mut settings.smart_dashes, "Two hyphens (--) make an em dash (\u{2014})");
                        ui.checkbox(&mut settings.show_markdown, "Show *italics*, **bold** and headings as you type");
                    });
                    ui.end_row();

                    ui.label("iPad helper");
                    ui.vertical(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut settings.helper_url)
                                .hint_text("https://writer.example.com/")
                                .desired_width(240.0),
                        );
                        let field = ui.add(
                            egui::TextEdit::singleline(token).password(true).hint_text("Token").desired_width(240.0),
                        );
                        // Saved to the Keychain on leaving the field, not per keystroke.
                        token_changed = field.lost_focus();
                        ui.weak("Make a token on the helper's page, then paste it here.");
                    });
                    ui.end_row();
                });

                // A sample in the chosen font, size and spacing.
                ui.add_space(10.0);
                ui.separator();
                let font = FontId::new(settings.font_size, fonts::prose_family());
                let sample = "The snow came early that year\u{2014}three weeks before the first frost was due.";
                ui.add(egui::Label::new(RichText::new(sample).font(font).line_height(Some(
                    settings.font_size * 1.2 * settings.line_spacing,
                ))));
                ui.add_space(6.0);
                if ui.button("Restore Defaults").clicked() {
                    *settings = WriterSettings::default();
                }
            });
        if self.settings != before {
            self.apply_settings(ctx);
        }
        if token_changed && !cfg!(test) && !crate::keychain::save_token(&self.helper_token) {
            self.error = Some("Couldn't save the helper token to the Keychain.".to_owned());
        }
        if token_changed {
            self.sync_now = true;
        }
        if !open {
            self.settings_open = false;
        }
    }
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
    }

    #[test]
    fn the_editor_follows_the_settings() {
        let s = WriterSettings { font_size: 21.0, smart_dashes: false, text_width: None, ..Default::default() };
        let e = s.editor();
        assert!(e.prose && e.word_wrap && !e.show_line_numbers);
        assert_eq!((e.font_size, e.smart_dashes, e.max_text_width), (21.0, false, None));
    }
}
