//! Search all tabs (⇧⌘F): a panel listing every match in every open
//! document, grouped by tab. Clicking a result jumps to it.

use std::ops::Range;

use eframe::egui::{self, Color32, FontId, RichText, Sense, TextFormat, text::LayoutJob};

use crate::tabs::TabId;

/// One match, ready to show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResultRow {
    /// Byte range of the match in the document.
    pub range: Range<usize>,
    /// 1-based line number.
    pub line: usize,
    /// The line (or part of it, for long lines) the match is on.
    pub snippet: String,
    /// Where the match is within `snippet` (bytes).
    pub highlight: Range<usize>,
}

/// How much of a long line to show around a match, in characters.
const CONTEXT_BEFORE: usize = 40;
const SNIPPET_MAX: usize = 140;

/// Turn match ranges (sorted, as from `find_all`) into result rows.
pub fn build_rows(text: &str, matches: &[Range<usize>]) -> Vec<ResultRow> {
    let mut rows = Vec::with_capacity(matches.len());
    let mut line = 1;
    let mut counted_to = 0;
    for m in matches {
        line += text.as_bytes()[counted_to..m.start].iter().filter(|&&b| b == b'\n').count();
        counted_to = m.start;

        let line_start = text[..m.start].rfind('\n').map_or(0, |i| i + 1);
        let line_end = text[m.start..].find('\n').map_or(text.len(), |i| m.start + i);
        let (snippet, highlight) = snippet(&text[line_start..line_end], m.start - line_start, m.end.min(line_end) - line_start);
        rows.push(ResultRow { range: m.clone(), line, snippet, highlight });
    }
    rows
}

/// Cut a line down to the part around the match, trimming indentation.
fn snippet(line: &str, start: usize, end: usize) -> (String, Range<usize>) {
    // Start a little before the match on long lines.
    let mut from = line[..start]
        .char_indices()
        .rev()
        .nth(CONTEXT_BEFORE)
        .map_or(0, |(i, _)| i);
    // Skip leading whitespace (but never past the match).
    from += line[from..start].len() - line[from..start].trim_start().len();
    let to = line[from..]
        .char_indices()
        .nth(SNIPPET_MAX)
        .map_or(line.len(), |(i, _)| from + i)
        .max(end.min(line.len()));

    let mut text = String::new();
    let prefix = if from > 0 && !line[..from].trim().is_empty() { "\u{2026}" } else { "" };
    text.push_str(prefix);
    text.push_str(&line[from..to]);
    if to < line.len() {
        text.push('\u{2026}');
    }
    let shift = prefix.len();
    (text, (start - from + shift)..(end.min(to) - from + shift))
}

/// Results for one tab.
pub struct TabResults<'a> {
    pub tab: TabId,
    pub name: String,
    pub rows: &'a [ResultRow],
}

pub enum ResultsAction {
    /// Go to this match.
    Jump(TabId, Range<usize>),
    Close,
}

/// Rows shown per tab; the rest are summarised (the counts stay exact).
const ROWS_PER_TAB: usize = 500;

pub fn results_panel(ui: &mut egui::Ui, results: &[TabResults<'_>], error: Option<&str>) -> Option<ResultsAction> {
    let mut action = None;
    let total: usize = results.iter().map(|r| r.rows.len()).sum();
    let tabs_with_matches = results.iter().filter(|r| !r.rows.is_empty()).count();

    ui.horizontal(|ui| {
        let heading = match (error, total) {
            (Some(e), _) => RichText::new(e).color(ui.visuals().error_fg_color),
            (None, 0) => RichText::new("No matches in open tabs").strong(),
            (None, n) => RichText::new(format!(
                "{n} {} in {tabs_with_matches} {}",
                if n == 1 { "match" } else { "matches" },
                if tabs_with_matches == 1 { "tab" } else { "tabs" },
            ))
            .strong(),
        };
        ui.label(heading);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("\u{d7}").on_hover_text("Close results").clicked() {
                action = Some(ResultsAction::Close);
            }
        });
    });
    ui.separator();

    let mono = FontId::monospace(12.5);
    let text_color = ui.visuals().text_color();
    let weak = ui.visuals().weak_text_color();
    let match_bg = Color32::from_rgba_unmultiplied(255, 200, 0, 90);

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for tab in results.iter().filter(|r| !r.rows.is_empty()) {
            ui.add_space(2.0);
            ui.label(RichText::new(format!("{}  ({})", tab.name, tab.rows.len())).strong());
            for row in tab.rows.iter().take(ROWS_PER_TAB) {
                let mut job = LayoutJob::default();
                job.append(&format!("{:>6}  ", row.line), 0.0, TextFormat::simple(mono.clone(), weak));
                let plain = TextFormat::simple(mono.clone(), text_color);
                let h = row.highlight.clone();
                job.append(&row.snippet[..h.start], 0.0, plain.clone());
                job.append(&row.snippet[h.clone()], 0.0, TextFormat { background: match_bg, ..plain.clone() });
                job.append(&row.snippet[h.end..], 0.0, plain);
                let response = ui.add(egui::Label::new(job).sense(Sense::click()).selectable(false).truncate());
                if response.clicked() {
                    action = Some(ResultsAction::Jump(tab.tab, row.range.clone()));
                }
                if response.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
            }
            if tab.rows.len() > ROWS_PER_TAB {
                ui.weak(format!("        \u{2026} and {} more", tab.rows.len() - ROWS_PER_TAB));
            }
        }
    });
    action
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_have_line_numbers_and_highlighted_snippets() {
        let text = "first line\n    second cat line\nno\ncat";
        let rows = build_rows(text, &[22..25, 35..38]);
        assert_eq!(rows[0].line, 2);
        assert_eq!(rows[0].snippet, "second cat line"); // indentation trimmed
        assert_eq!(&rows[0].snippet[rows[0].highlight.clone()], "cat");
        assert_eq!(rows[1].line, 4);
        assert_eq!(rows[1].snippet, "cat");
    }

    #[test]
    fn long_lines_are_cut_around_the_match() {
        let line = format!("{}needle{}", "x".repeat(300), "y".repeat(300));
        let needle = std::iter::once(300..306).collect::<Vec<_>>();
        let rows = build_rows(&line, &needle);
        let row = &rows[0];
        assert!(row.snippet.starts_with('\u{2026}') && row.snippet.ends_with('\u{2026}'));
        assert!(row.snippet.chars().count() < 200);
        assert_eq!(&row.snippet[row.highlight.clone()], "needle");
    }
}
