//! The main editing surface: a monospace text area with a line-number gutter.

use eframe::egui::{
    self, Align2, FontId, Id, Rect, ScrollArea, Sense, TextEdit, Vec2, text::CCursor,
};

pub struct EditorSettings {
    pub show_line_numbers: bool,
    pub word_wrap: bool,
    pub font_size: f32,
}

impl EditorSettings {
    pub const DEFAULT_FONT_SIZE: f32 = 14.0;
}

impl Default for EditorSettings {
    fn default() -> Self {
        Self {
            show_line_numbers: true,
            word_wrap: false,
            font_size: Self::DEFAULT_FONT_SIZE,
        }
    }
}

/// Information the editor reports back to the app each frame.
#[derive(Default, Clone, Copy)]
pub struct CursorInfo {
    /// 1-based line and column of the cursor, if the editor has a cursor.
    pub line_col: Option<(usize, usize)>,
}

pub fn editor_id() -> Id {
    Id::unique("sloth-editor")
}

pub fn show(ui: &mut egui::Ui, text: &mut String, settings: &EditorSettings) -> CursorInfo {
    let font = FontId::monospace(settings.font_size);
    let line_count = text.split('\n').count();
    let digits = line_count.to_string().len().max(2);

    let gutter_width = if settings.show_line_numbers {
        let char_w = ui.fonts_mut(|f| f.glyph_width(&font, '0'));
        char_w * digits as f32 + 16.0
    } else {
        0.0
    };

    let scroll = if settings.word_wrap {
        ScrollArea::vertical()
    } else {
        ScrollArea::both()
    };

    let mut info = CursorInfo::default();

    scroll.auto_shrink([false, false]).show(ui, |ui| {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;

            // Reserve the gutter column; numbers are painted after layout,
            // once we know where each text row landed.
            let gutter_rect = if settings.show_line_numbers {
                let (rect, _) = ui.allocate_exact_size(Vec2::new(gutter_width, 0.0), Sense::hover());
                Some(rect)
            } else {
                None
            };
            ui.add_space(6.0); // breathing room between gutter and text

            let edit = TextEdit::multiline(text)
                .id(editor_id())
                .font(font.clone())
                .code_editor()
                .frame(egui::Frame::NONE)
                .lock_focus(true) // keep Tab inside the editor
                .desired_rows(1)
                .desired_width(if settings.word_wrap {
                    ui.available_width()
                } else {
                    f32::INFINITY
                });

            // Custom layouter so we control wrapping: infinite width = no wrap.
            let wrap = settings.word_wrap;
            let layout_font = font.clone();
            let mut layouter = move |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap_width: f32| {
                let job = egui::text::LayoutJob::simple(
                    buf.as_str().to_owned(),
                    layout_font.clone(),
                    ui.visuals().text_color(),
                    if wrap { wrap_width } else { f32::INFINITY },
                );
                ui.fonts_mut(|f| f.layout_job(job))
            };
            let edit = edit.layouter(&mut layouter);

            // Fill the rest of the viewport so clicking below the text focuses it.
            let min_height = ui.available_height();
            let output = edit.min_size(Vec2::new(0.0, min_height)).show(ui);

            if let Some(cursor) = output.cursor_range {
                info.line_col = Some(line_col(text, cursor.primary));
            }

            if let Some(gutter) = gutter_rect {
                paint_line_numbers(ui, gutter, &output, &font);
            }
        });
    });

    info
}

fn paint_line_numbers(
    ui: &egui::Ui,
    gutter: Rect,
    output: &egui::text_edit::TextEditOutput,
    font: &FontId,
) {
    let painter = ui.painter();
    let color = ui.visuals().weak_text_color();
    let bg = ui.visuals().faint_bg_color;
    let full = Rect::from_min_max(
        gutter.min,
        egui::pos2(gutter.max.x, output.response.rect.max.y),
    );
    painter.rect_filled(full, 0.0, bg);
    painter.vline(
        full.max.x - 4.0,
        full.y_range(),
        egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
    );

    // Only paint rows that are visible.
    let clip = ui.clip_rect();
    let mut line_no = 1usize;
    let mut at_line_start = true;
    for row in &output.galley.rows {
        let y = output.galley_pos.y + row.pos.y;
        if at_line_start && y + row.height() >= clip.min.y && y <= clip.max.y {
            painter.text(
                egui::pos2(full.max.x - 10.0, y),
                Align2::RIGHT_TOP,
                line_no.to_string(),
                font.clone(),
                color,
            );
        }
        // A wrapped line spans several rows; only number the first.
        at_line_start = row.ends_with_newline;
        if row.ends_with_newline {
            line_no += 1;
        }
    }
}

/// Convert a character cursor into a 1-based (line, column).
fn line_col(text: &str, cursor: CCursor) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    for c in text.chars().take(cursor.index.0) {
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}
