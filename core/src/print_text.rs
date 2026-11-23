//! A document as it's printed: optionally with line numbers, and with its
//! syntax colours as runs of the printed text. Kept apart from the macOS
//! printing (macos_print.rs) so it can be tested anywhere.

use std::ops::Range;

use eframe::egui::Color32;

use crate::syntax::Span;

/// The grey of printed line numbers.
pub const NUMBER_COLOR: Color32 = Color32::from_rgb(0x9a, 0x9a, 0x9a);

/// The text to print, with what colours which parts of it.
#[derive(Clone, Debug, PartialEq)]
pub struct PrintText {
    pub text: String,
    /// Coloured runs, in UTF-16 code units (as macOS counts text).
    pub runs: Vec<(Range<usize>, Color32)>,
    /// With line numbers: how many digits the widest has (0 without).
    pub number_digits: usize,
}

/// `text` laid out for printing. With `numbered`, each line starts with its
/// number and a tab (wrapped lines are indented to match by the printing).
/// `spans`: syntax colours, as byte ranges of `text`.
pub fn layout(text: &str, spans: &[Span], numbered: bool) -> PrintText {
    let lines: Vec<&str> = text.split('\n').collect();
    let digits = if numbered { lines.len().to_string().len() } else { 0 };
    let mut out = String::with_capacity(text.len() + lines.len() * (digits + 1));
    let mut runs = Vec::new();
    // Where each source byte that starts a line ends up, in UTF-16 units,
    // and the running UTF-16 length of the output.
    let mut units = 0usize;
    // Source byte offset → output UTF-16 offset, at each line's start.
    let mut line_starts: Vec<(usize, usize)> = Vec::with_capacity(lines.len());
    let mut byte = 0usize;
    for (i, line) in lines.iter().enumerate() {
        if numbered {
            let number = format!("{:>digits$}\t", i + 1);
            let len = number.encode_utf16().count();
            runs.push((units..units + len - 1, NUMBER_COLOR));
            out.push_str(&number);
            units += len;
        }
        line_starts.push((byte, units));
        out.push_str(line);
        units += line.encode_utf16().count();
        byte += line.len();
        if i + 1 < lines.len() {
            out.push('\n');
            units += 1;
            byte += 1;
        }
    }
    // Each span, mapped into the output (split where it crosses lines, as
    // the numbers come between).
    let to_units = |at: usize| -> usize {
        let i = line_starts.partition_point(|&(start, _)| start <= at).saturating_sub(1);
        let (start, unit) = line_starts[i];
        unit + text.get(start..at).map_or(0, |s| s.encode_utf16().count())
    };
    for span in spans {
        let (start, end) = (span.range.start.min(text.len()), span.range.end.min(text.len()));
        let mut from = start;
        while from < end {
            let line_end = text[from..end].find('\n').map_or(end, |i| from + i);
            if line_end > from {
                runs.push((to_units(from)..to_units(line_end), span.color));
            }
            from = line_end + 1;
        }
    }
    PrintText { text: out, runs, number_digits: digits }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(range: Range<usize>, color: Color32) -> Span {
        Span { range, color, italic: false }
    }

    #[test]
    fn plain_and_unchanged_without_numbers() {
        let red = Color32::RED;
        let p = layout("let x = 1;\nfoo()", &[span(0..3, red)], false);
        assert_eq!(p.text, "let x = 1;\nfoo()");
        assert_eq!(p.runs, vec![(0..3, red)]);
        assert_eq!(p.number_digits, 0);
    }

    #[test]
    fn numbered_lines_shift_the_colours_after_them() {
        let (red, blue) = (Color32::RED, Color32::BLUE);
        let text = (1..=10).map(|i| format!("l{i}")).collect::<Vec<_>>().join("\n");
        // "l10" is the last line, at byte 27.
        let p = layout(&text, &[span(27..30, red), span(0..2, blue)], true);
        assert!(p.text.starts_with(" 1\tl1\n 2\tl2\n"));
        assert!(p.text.ends_with("10\tl10"));
        assert_eq!(p.number_digits, 2);
        let at = p.text.find("l10").unwrap();
        assert!(p.runs.contains(&(at..at + 3, red)));
        assert!(p.runs.contains(&(3..5, blue)));
        assert!(p.runs.contains(&(0..2, NUMBER_COLOR)), "the number, not its tab");
    }

    #[test]
    fn spans_across_lines_and_wide_characters() {
        let green = Color32::GREEN;
        // A comment over two lines; "é" is 2 bytes but 1 UTF-16 unit, "😀" 4 and 2.
        let text = "/* é\n😀 */ x";
        let p = layout(text, &[span(0..text.find(" x").unwrap(), green)], true);
        assert_eq!(p.text, "1\t/* é\n2\t😀 */ x");
        assert_eq!(p.runs[2..], [(2..6, green), (9..14, green)]);
    }
}
