//! Markdown as you type: which parts of the text are italic, bold, headings,
//! quotes, notes or scene breaks, and which characters are the markers
//! (`*`, `#`, `>`) that are drawn faintly. The text itself is never changed.
//!
//! This reads prose the way Sloth Writer does: line by line, emphasis inside
//! a line, `<!-- notes -->` across lines. It isn't a full Markdown parser
//! (no lists, links or code); it's for showing what's typed while typing.

use std::ops::Range;

/// How a stretch of text is shown. Several can apply at once.
pub type Style = u16;
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
/// `[[…]]`: a reference to a profile (a source, a concept, a character).
/// Like a note in the margin, it isn't part of the prose.
pub const REFERENCE: Style = 128;
/// A passage with a comment on it: `{==this==}{>>…<<}` (CriticMarkup).
pub const COMMENTED: Style = 256;
/// A comment's marks and words: `{==`, `==}` and `{>>…<<}`. Like a note in
/// the margin, not part of the prose; kept out of sight except where
/// you're writing.
pub const COMMENT: Style = 512;

/// A comment in the text: `{==passage==}{>>words<<}`, or just
/// `{>>words<<}` at a point.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Comment {
    /// All of it, marks and passage and words.
    pub whole: Range<usize>,
    /// The passage it's on, if any (without its marks).
    pub passage: Option<Range<usize>>,
    /// The comment's words (inside `{>>` and `<<}`).
    pub words: Range<usize>,
}

/// The comments in `text`, in order (outside notes).
pub fn comments(text: &str) -> Vec<Comment> {
    let notes = note_ranges(text);
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(open) = text[from..].find("{>>").map(|i| from + i) {
        let Some(close) = text[open + 3..].find("<<}").map(|i| open + 3 + i) else { break };
        from = close + 3;
        if notes.iter().any(|n| n.start <= open && open < n.end) {
            continue;
        }
        let words = open + 3..close;
        // A passage just before: `{==…==}` ending right where this starts.
        let passage = text[..open]
            .strip_suffix("==}")
            .and_then(|before| before.rfind("{==").map(|start| start..before.len()))
            .filter(|p| !text[p.start + 3..p.end].contains("==}") && out.last().is_none_or(|c: &Comment| c.whole.end <= p.start));
        let whole = passage.as_ref().map_or(open, |p| p.start)..close + 3;
        out.push(Comment { whole, passage: passage.map(|p| p.start + 3..p.end), words });
    }
    out
}

/// `text` without its comments: a commented passage stays, without its
/// marks; the comments go, with the extra space a lone one leaves.
pub fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut from = 0;
    for c in comments(text) {
        out.push_str(&text[from..c.whole.start]);
        if let Some(p) = &c.passage {
            out.push_str(&text[p.clone()]);
        }
        from = c.whole.end;
        if c.passage.is_none() && out.ends_with([' ', '\t']) && text[from..].starts_with([' ', '\t', '\n', '.', ',', ';', ':', '!', '?']) {
            out.pop();
        }
    }
    out.push_str(&text[from..]);
    out
}

/// The `<!-- notes -->` in `text`, as byte ranges (an unclosed one runs to
/// the end).
fn note_ranges(text: &str) -> Vec<Range<usize>> {
    let mut notes = Vec::new();
    let mut from = 0;
    while let Some(start) = text[from..].find("<!--").map(|i| from + i) {
        let end = text[start + 4..].find("-->").map_or(text.len(), |i| start + 4 + i + 3);
        notes.push(start..end);
        from = end;
    }
    notes
}

/// The references in `text`: each `[[…]]` within one line and outside
/// notes, as its byte range (brackets and all) and what's inside, trimmed.
pub fn references(text: &str) -> Vec<(Range<usize>, &str)> {
    let notes = note_ranges(text);
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(start) = text[from..].find("[[").map(|i| from + i) {
        let Some(close) = text[start + 2..].find("]]").map(|i| start + 2 + i) else { break };
        let inner = &text[start + 2..close];
        if inner.contains("[[") || inner.contains('\n') {
            // Not closed on this line: look again from the next `[[`.
            from = start + 2;
            continue;
        }
        let range = start..close + 2;
        if !inner.trim().is_empty() && !notes.iter().any(|n| n.start <= start && start < n.end) {
            out.push((range.clone(), inner.trim()));
        }
        from = range.end;
    }
    out
}

/// `text` without its references, and without the extra space each one
/// leaves (`the gaze [[Berger]] applies` reads `the gaze applies`).
pub fn strip_references(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut from = 0;
    for (range, _) in references(text) {
        out.push_str(&text[from..range.start]);
        from = range.end;
        let rest = &text[from..];
        let before_space = out.is_empty() || out.ends_with([' ', '\t', '\n']);
        if before_space && rest.starts_with([' ', '\t']) {
            from += 1;
        } else if out.ends_with([' ', '\t']) && (rest.is_empty() || rest.starts_with(['\n', '.', ',', ';', ':', '!', '?', ')'])) {
            out.pop();
        }
    }
    out.push_str(&text[from..]);
    out
}

/// The styled stretches of `text`, in order, without overlaps: each range
/// with every style that applies to it. Unstyled text isn't listed.
pub fn styles(text: &str) -> Vec<(Range<usize>, Style)> {
    // Join neighbouring bytes with the same style into stretches.
    let mut out: Vec<(Range<usize>, Style)> = Vec::new();
    for (i, &f) in flags(text).iter().enumerate() {
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

/// The styles of each byte of `text`.
fn flags(text: &str) -> Vec<Style> {
    let mut flags = vec![0 as Style; text.len()];
    let mut set = |range: Range<usize>, style: Style| {
        for f in &mut flags[range] {
            *f |= style;
        }
    };

    // Notes first: they can span lines.
    for note in note_ranges(text) {
        set(note, NOTE);
    }
    for (range, _) in references(text) {
        set(range.clone(), REFERENCE);
        set(range.start..range.start + 2, MARKER);
        set(range.end - 2..range.end, MARKER);
    }
    for c in comments(text) {
        if let Some(p) = &c.passage {
            set(p.start - 3..p.start, COMMENT | MARKER);
            set(p.clone(), COMMENTED);
            set(p.end..p.end + 3, COMMENT | MARKER);
        }
        set(c.words.start - 3..c.words.start, COMMENT | MARKER);
        set(c.words.clone(), COMMENT);
        set(c.words.end..c.words.end + 3, COMMENT | MARKER);
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
    flags
}

/// Text as it reads, without its Markdown: for copying to other apps.
/// `html` has the formatting (paragraphs, headings, quotes, bold and
/// italics); `plain` is the same words with the markers taken out, line
/// for line. Notes (`<!-- … -->`) and references (`[[…]]`) are left out
/// of both.
#[derive(Debug, PartialEq)]
pub struct Formatted {
    pub html: String,
    pub plain: String,
}

pub fn formatted(text: &str) -> Formatted {
    // References are margin notes: they go, with the space they leave.
    let text = &strip_references(text);
    let flags = flags(text);
    let mut html = String::new();
    let mut plain: Vec<String> = Vec::new();
    let mut line_start = 0;
    for line in text.split_inclusive('\n') {
        let content = line.trim_end_matches(['\n', '\r']);
        let range = line_start..line_start + content.len();
        line_start += line.len();
        if content.trim().is_empty() {
            plain.push(String::new());
            continue;
        }
        let line_flags = &flags[range.clone()];
        // The runs of shown text, each with its emphasis.
        let mut runs: Vec<(String, Style)> = Vec::new();
        for (i, c) in content.char_indices() {
            let f = line_flags[i];
            if f & (NOTE | COMMENT) != 0 || (f & MARKER != 0 && f & BREAK == 0) {
                continue;
            }
            let emphasis = f & (BOLD | ITALIC);
            match runs.last_mut() {
                Some((run, style)) if *style == emphasis => run.push(c),
                _ => runs.push((c.to_string(), emphasis)),
            }
        }
        let words: String = runs.iter().map(|(t, _)| t.as_str()).collect();
        if words.trim().is_empty() {
            continue; // only a note, say
        }
        plain.push(words.trim().to_owned());
        let inner: String = runs
            .iter()
            .map(|(t, style)| {
                let mut t = escape_html(t);
                if style & ITALIC != 0 {
                    t = format!("<em>{t}</em>");
                }
                if style & BOLD != 0 {
                    t = format!("<strong>{t}</strong>");
                }
                t
            })
            .collect();
        let inner = inner.trim();
        let first = line_flags.iter().copied().find(|f| f & NOTE == 0).unwrap_or(0);
        html.push_str(&if first & BREAK != 0 {
            format!("<p style=\"text-align:center\">{inner}</p>\n")
        } else if first & HEADING != 0 {
            let level = heading_level(content.trim_start()).unwrap_or(1);
            format!("<h{level}>{inner}</h{level}>\n")
        } else if line_flags.iter().any(|f| f & QUOTE != 0) {
            format!("<blockquote><p>{inner}</p></blockquote>\n")
        } else {
            format!("<p>{inner}</p>\n")
        });
    }
    // Blank lines at either end are just where the selection began or ended.
    let plain = plain.join("\n").trim_matches('\n').to_owned();
    let html = format!("<html><head><meta charset=\"utf-8\"></head><body>\n{html}</body></html>");
    Formatted { html, plain }
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
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

    #[test]
    fn comments_on_passages_and_points() {
        let text = "She {==left the lamp==}{>>why here?<<} on. Later.{>>check dates<<} End.";
        let found = comments(text);
        assert_eq!(found.len(), 2);
        assert_eq!(&text[found[0].passage.clone().unwrap()], "left the lamp");
        assert_eq!(&text[found[0].words.clone()], "why here?");
        assert_eq!(&text[found[0].whole.clone()], "{==left the lamp==}{>>why here?<<}");
        assert_eq!(found[1].passage, None);
        assert_eq!(&text[found[1].words.clone()], "check dates");
        assert_eq!(strip_comments(text), "She left the lamp on. Later. End.");
        assert_eq!(formatted(text).plain, "She left the lamp on. Later. End.");

        // The passage is tinted; the marks and words kept for out of sight.
        let style_of = |needle: &str| styles(text).into_iter().find(|(r, _)| text[r.clone()].contains(needle)).map(|(_, s)| s).unwrap();
        assert_eq!(style_of("left the"), COMMENTED);
        assert_eq!(style_of("why here"), COMMENT);
        assert!(comments("<!-- {>>in a note<<} -->").is_empty());
        assert!(comments("{>>never closed").is_empty());
    }

    #[test]
    fn references_are_found_and_taken_out() {
        let text = "The gaze [[Ways of Seeing|p. 47]] applies. [[Concept: attention]]\n[[not\nclosed]] <!-- [[in a note]] --> [[ ]] end [[x]].";
        let found: Vec<&str> = references(text).into_iter().map(|(_, inner)| inner).collect();
        assert_eq!(found, ["Ways of Seeing|p. 47", "Concept: attention", "x"]);
        assert_eq!(
            strip_references(text),
            "The gaze applies.\n[[not\nclosed]] <!-- [[in a note]] --> [[ ]] end."
        );
        assert_eq!(strip_references("[[A]] Starts here."), "Starts here.");
        assert_eq!(strip_references("Ends here [[A]]"), "Ends here");
        // Styled: the whole reference, its brackets as markers.
        let styled = show("a [[B]] c");
        assert_eq!(styled, [("[[", REFERENCE | MARKER), ("B", REFERENCE), ("]]", REFERENCE | MARKER)]);
        assert_eq!(formatted("She was [[Berger]] **sure**.").plain, "She was sure.");
    }

    #[test]
    fn formatted_text_has_no_markers() {
        let f = formatted("# The *Long* Winter\n\nShe was **sure**. <!-- check -->\n> A _quote_ & more\n#\n<!-- just a note -->\nThe end.");
        assert_eq!(f.plain, "The Long Winter\n\nShe was sure.\nA quote & more\n#\nThe end.");
        let body = f.html.split("<body>\n").nth(1).unwrap();
        assert_eq!(
            body,
            "<h1>The <em>Long</em> Winter</h1>\n\
             <p>She was <strong>sure</strong>.</p>\n\
             <blockquote><p>A <em>quote</em> &amp; more</p></blockquote>\n\
             <p style=\"text-align:center\">#</p>\n\
             <p>The end.</p>\n\
             </body></html>"
        );
        assert!(f.html.contains("charset=\"utf-8\""), "so em dashes and accents survive");
    }

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
