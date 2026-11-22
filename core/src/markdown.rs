//! Markdown as you type: which parts of the text are italic, bold, headings,
//! quotes, notes or scene breaks, and which characters are the markers
//! (`*`, `#`, `>`) that are drawn faintly. The text itself is never changed.
//!
//! This reads prose the way Sloth Writer does: line by line, emphasis inside
//! a line, `<!-- notes -->` across lines. It isn't a full Markdown parser
//! (no lists, links or code); it's for showing what's typed while typing.

use std::ops::Range;

/// How a stretch of text is shown. Several can apply at once.
pub type Style = u8;
pub const ITALIC: Style = 1;
pub const BOLD: Style = 2;
/// A Markdown marker: drawn faintly.
pub const MARKER: Style = 4;
pub const HEADING: Style = 8;
pub const QUOTE: Style = 16;
/// Inside `<!-- … -->`.
pub const NOTE: Style = 32;
/// A scene break line: `#`, `***`, `---`, `* * *`.
pub const BREAK: Style = 64;

/// The styled stretches of `text`, in order, without overlaps: each range
/// with every style that applies to it. Unstyled text isn't listed.
pub fn styles(text: &str) -> Vec<(Range<usize>, Style)> {
    let mut flags = vec![0 as Style; text.len()];
    let mut set = |range: Range<usize>, style: Style| {
        for f in &mut flags[range] {
            *f |= style;
        }
    };

    // Notes first: they can span lines.
    let mut from = 0;
    while let Some(start) = text[from..].find("<!--").map(|i| from + i) {
        let end = text[start + 4..].find("-->").map_or(text.len(), |i| start + 4 + i + 3);
        set(start..end, NOTE);
        from = end;
    }

    let mut line_start = 0;
    for line in text.split_inclusive('\n') {
        let content = line.trim_end_matches(['\n', '\r']);
        let range = line_start..line_start + content.len();
        line_start += line.len();
        if content.trim().is_empty() {
            continue;
        }
        let indent = content.len() - content.trim_start().len();
        let body = range.start + indent..range.end;
        let trimmed = &text[body.clone()];

        if is_scene_break(trimmed) {
            set(body, BREAK | MARKER);
            continue;
        }
        if let Some(level) = heading_level(trimmed) {
            let marker_end = body.start + level + 1;
            set(body.start..marker_end, MARKER | HEADING);
            set(marker_end..body.end, HEADING);
            emphasis(text, marker_end..body.end, &mut set);
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix('>') {
            let marker_end = body.start + 1 + usize::from(rest.starts_with(' '));
            set(body.start..marker_end, MARKER);
            set(marker_end..body.end, QUOTE);
            emphasis(text, marker_end..body.end, &mut set);
            continue;
        }
        emphasis(text, body, &mut set);
    }

    // Join neighbouring bytes with the same style into stretches.
    let mut out: Vec<(Range<usize>, Style)> = Vec::new();
    for (i, &f) in flags.iter().enumerate() {
        if f == 0 {
            continue;
        }
        match out.last_mut() {
            Some((r, style)) if r.end == i && *style == f => r.end = i + 1,
            _ => out.push((i..i + 1, f)),
        }
    }
    out
}

/// `#` alone, or three or more `*` or `-` (spaces allowed between).
pub fn is_scene_break(line: &str) -> bool {
    let marks: String = line.chars().filter(|c| !c.is_whitespace()).collect();
    marks == "#" || (marks.len() >= 3 && (marks.chars().all(|c| c == '*') || marks.chars().all(|c| c == '-')))
}

/// `# Title` to `###### Title`: the number of `#`s.
fn heading_level(line: &str) -> Option<usize> {
    let hashes = line.chars().take_while(|&c| c == '#').count();
    ((1..=6).contains(&hashes) && line[hashes..].starts_with(' ')).then_some(hashes)
}

/// `*italic*`, `_italic_`, `**bold**`, `***both***` and nestings of them,
/// within one line. Markers must touch their words (`* not this *`), `_`
/// doesn't count inside a word (snake_case_names), and `\*` is a plain star.
fn emphasis(text: &str, line: Range<usize>, set: &mut impl FnMut(Range<usize>, Style)) {
    let bytes = text.as_bytes();
    let at = |i: usize| text[i..].chars().next();
    let before = |i: usize| text[..i].chars().next_back();
    // Openers waiting for a closer: (marker char, run length, start).
    let mut open: Vec<(u8, usize, usize)> = Vec::new();
    let mut i = line.start;
    while i < line.end {
        let c = bytes[i];
        if c == b'\\' {
            i += 2; // an escaped character is never a marker
            continue;
        }
        if c != b'*' && c != b'_' {
            i += at(i).map_or(1, char::len_utf8);
            continue;
        }
        let run = bytes[i..line.end].iter().take_while(|&&b| b == c).count();
        let (prev, next) = (if i > line.start { before(i) } else { None }, at(i + run).filter(|_| i + run < line.end));
        let flanks_left = next.is_some_and(|n| !n.is_whitespace());
        let flanks_right = prev.is_some_and(|p| !p.is_whitespace());
        let (can_open, can_close) = if c == b'_' {
            // Not inside a word.
            (flanks_left && !prev.is_some_and(char::is_alphanumeric), flanks_right && !next.is_some_and(char::is_alphanumeric))
        } else {
            (flanks_left, flanks_right)
        };
        let closes = can_close
            .then(|| open.iter().rposition(|&(m, len, _)| m == c && len == run.min(3)))
            .flatten();
        if let Some(k) = closes {
            let (_, len, start) = open[k];
            open.truncate(k);
            let style = match len {
                1 => ITALIC,
                2 => BOLD,
                _ => ITALIC | BOLD,
            };
            set(start..start + len, MARKER);
            set(start + len..i, style);
            set(i..i + len, MARKER);
        } else if can_open && run <= 3 {
            open.push((c, run, i));
        }
        i += run;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The styled stretches as (text, style) pairs, for readable tests.
    fn show(text: &str) -> Vec<(&str, Style)> {
        styles(text).into_iter().map(|(r, s)| (&text[r], s)).collect()
    }

    #[test]
    fn emphasis_and_its_markers() {
        assert_eq!(show("a *soft* word"), [("*", MARKER), ("soft", ITALIC), ("*", MARKER)]);
        assert_eq!(show("**loud**"), [("**", MARKER), ("loud", BOLD), ("**", MARKER)]);
        assert_eq!(show("***both***"), [("***", MARKER), ("both", ITALIC | BOLD), ("***", MARKER)]);
        assert_eq!(show("_under_"), [("_", MARKER), ("under", ITALIC), ("_", MARKER)]);
        assert_eq!(
            show("**bold *and* bold**"),
            [
                ("**", MARKER),
                ("bold ", BOLD),
                ("*", MARKER | BOLD),
                ("and", BOLD | ITALIC),
                ("*", MARKER | BOLD),
                (" bold", BOLD),
                ("**", MARKER),
            ]
        );
    }

    #[test]
    fn things_that_arent_emphasis() {
        for text in ["2 * 3 * 4", "snake_case_name", r"a \*star\*", "*unclosed", "one*two", "* list item"] {
            let found = show(text);
            assert!(found.iter().all(|(_, s)| s & (ITALIC | BOLD) == 0), "{text}: {found:?}");
        }
        assert_eq!(show("plain prose"), []);
    }

    #[test]
    fn emphasis_stays_within_a_line() {
        assert_eq!(show("*not\nclosed*"), []);
    }

    #[test]
    fn headings_quotes_and_breaks() {
        assert_eq!(show("## Part Two"), [("## ", MARKER | HEADING), ("Part Two", HEADING)]);
        assert_eq!(show("#hashtag"), []);
        assert_eq!(show("> she *wrote*"), [("> ", MARKER), ("she ", QUOTE), ("*", MARKER | QUOTE), ("wrote", ITALIC | QUOTE), ("*", MARKER | QUOTE)]);
        for line in ["#", "***", "---", "* * *", "  #  "] {
            let found = show(line);
            assert_eq!(found.len(), 1, "{line}");
            assert_eq!(found[0].1, BREAK | MARKER, "{line}");
        }
    }

    #[test]
    fn notes_span_lines() {
        let text = "Before <!-- a note\nstill the note --> after";
        assert_eq!(show(text), [("<!-- a note\nstill the note -->", NOTE)]);
        assert_eq!(show("text <!-- never closed"), [("<!-- never closed", NOTE)]);
    }

    #[test]
    fn works_with_accented_text() {
        assert_eq!(show("*café* noir"), [("*", MARKER), ("café", ITALIC), ("*", MARKER)]);
    }
}
