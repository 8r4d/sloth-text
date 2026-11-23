//! Sloth → Settings… (⌘,): every option in one place. Changes apply as
//! soon as they're made, like the menu items they mirror.

use eframe::egui::{self, RichText};

use crate::editor::EditorSettings;
use crate::settings::Settings;
use crate::theme::{self, ThemeChoice};

/// What changed, so the app can re-apply it.
#[derive(Default)]
pub struct Changes {
    pub theme: bool,
    pub fonts: bool,
}

pub fn show(ctx: &egui::Context, open: &mut bool, settings: &mut Settings, fonts: &[String]) -> Changes {
    let mut changes = Changes::default();
    egui::Window::new("Settings")
        .open(open)
        .collapsible(false)
        .resizable(false)
        .default_width(380.0)
        .show(ctx, |ui| {
            egui::Grid::new("settings-grid")
                .num_columns(2)
                .spacing([16.0, 8.0])
                .show(ui, |ui| {
                    section(ui, "Appearance");

                    ui.label("Theme");
                    changes.theme |= theme::picker(ui, "theme", &mut settings.theme, 180.0);
                    ui.end_row();

                    let e = &mut settings.editor;
                    ui.label("Font");
                    let current = e.font.clone().unwrap_or_else(|| "Built-in (Hack)".to_owned());
                    egui::ComboBox::from_id_salt("font")
                        .selected_text(current)
                        .height(300.0)
                        .show_ui(ui, |ui| {
                            changes.fonts |= ui.selectable_value(&mut e.font, None, "Built-in (Hack)").changed();
                            for family in fonts {
                                changes.fonts |= ui
                                    .selectable_value(&mut e.font, Some(family.clone()), family)
                                    .changed();
                            }
                        });
                    ui.end_row();

                    ui.label("Text size");
                    ui.add(egui::DragValue::new(&mut e.font_size).range(6.0..=72.0).speed(0.25).suffix(" pt"));
                    ui.end_row();

                    ui.label("Show");
                    ui.vertical(|ui| {
                        ui.checkbox(&mut e.show_line_numbers, "Line numbers");
                        ui.checkbox(&mut e.word_wrap, "Word wrap");
                        ui.checkbox(&mut e.highlight_current_line, "Highlight the current line");
                        ui.checkbox(&mut e.syntax_highlighting, "Syntax highlighting");
                        ui.checkbox(&mut e.markdown_preview, "Markdown preview beside Markdown files");
                    });
                    ui.end_row();

                    section(ui, "Indentation");

                    ui.label("Indent with");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut e.indent_with_spaces, false, "Tabs");
                        ui.radio_value(&mut e.indent_with_spaces, true, "Spaces");
                    });
                    ui.end_row();

                    ui.label("Tab width");
                    ui.horizontal(|ui| {
                        for width in [2, 4, 8] {
                            changes.fonts |= ui.radio_value(&mut e.tab_width, width, width.to_string()).changed();
                        }
                    });
                    ui.end_row();

                    ui.label("");
                    ui.checkbox(&mut e.auto_indent, "New lines keep the indentation");
                    ui.end_row();

                    section(ui, "Files");

                    ui.label("At launch");
                    ui.checkbox(&mut settings.reopen_at_launch, "Reopen the windows and tabs from last time");
                    ui.end_row();

                    ui.label("Recent files");
                    ui.horizontal(|ui| {
                        let count = settings.recent_files.len();
                        ui.weak(format!("{count} remembered"));
                        if ui.add_enabled(count > 0, egui::Button::new("Clear")).clicked() {
                            settings.recent_files.clear();
                        }
                    });
                    ui.end_row();
                });

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Restore Defaults").clicked() {
                    let fresh = EditorSettings::default();
                    changes.fonts |= settings.editor.font != fresh.font || settings.editor.tab_width != fresh.tab_width;
                    changes.theme |= settings.theme != ThemeChoice::default();
                    settings.editor = fresh;
                    settings.theme = ThemeChoice::default();
                    settings.reopen_at_launch = true;
                }
            });
        });
    changes
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.label(RichText::new(title).strong());
    ui.end_row();
}
