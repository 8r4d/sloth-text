//! Break Up and Combine: one document's text as several, and several
//! documents' text as one. Only the text is worked out here; the app makes
//! the documents (see breakup_ui.rs).

use sloth_core::markdown;

/// Where Break Up breaks the text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BreakAt {
    /// At headings with at most this many `#`s: 1 is `#` headings only,
    /// 2 is `#` and `##`, 6 is any heading.
    Headings(usize),
    /// At scene breaks (`#` alone, `* * *`, `---`).
    SceneBreaks,
}

/// One document to be.
#[derive(Clone, Debug, PartialEq)]
pub struct Piece {
    pub title: String,
    pub text: String,
}

/// A text broken up: what came before the first break, and the pieces.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Broken {
    pub lead: String,
    pub pieces: Vec<Piece>,
}

/// Break `text` up at each break. A heading names the piece it starts (and
/// stays in its text only if `keep_headings`); a piece after a scene break
/// is named from its first words, and one with no words is dropped. Lines
/// inside a note (`<!-- … -->`) never break.
pub fn break_up(text: &str, at: BreakAt, keep_headings: bool) -> Broken {
    let mut lead = String::new();
    // Each piece: its heading's title (None after a scene break), its text.
    let mut pieces: Vec<(Option<String>, String)> = Vec::new();
    let mut in_note = false;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim();
        let broke = !in_note
            && match at {
                BreakAt::SceneBreaks => markdown::is_scene_break(trimmed),
                BreakAt::Headings(most) => markdown::heading_level(trimmed).is_some_and(|level| level <= most),
            };
        in_note = still_in_note(in_note, line);
        if broke {
            let title = match at {
                BreakAt::Headings(_) => Some(plain(trimmed.trim_start_matches('#'))),
                BreakAt::SceneBreaks => None,
            };
            let start = if keep_headings && title.is_some() { line.to_owned() } else { String::new() };
            pieces.push((title, start));
            continue;
        }
        match pieces.last_mut() {
            Some((_, piece)) => piece.push_str(line),
            None => lead.push_str(line),
        }
    }
    let pieces = pieces
        .into_iter()
        .filter_map(|(heading, text)| {
            let text = text.trim().to_owned();
            // After a scene break with nothing there: nothing to make.
            if heading.is_none() && text.is_empty() {
                return None;
            }
            let title = heading.filter(|t| !t.is_empty()).unwrap_or_else(|| first_words(&text));
            Some(Piece { title, text })
        })
        .collect();
    Broken { lead: lead.trim().to_owned(), pieces }
}

/// Whether a note is still open after this line.
fn still_in_note(mut open: bool, line: &str) -> bool {
    let mut rest = line;
    loop {
        let mark = if open { "-->" } else { "<!--" };
        match rest.find(mark) {
            Some(at) => {
                rest = &rest[at + mark.len()..];
                open = !open;
            }
            None => return open,
        }
    }
}

/// A line's words without their Markdown (emphasis, notes, references).
fn plain(line: &str) -> String {
    markdown::formatted(line.trim()).plain.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A name for a piece with no heading: its first few words.
fn first_words(text: &str) -> String {
    const WORDS: usize = 6;
    let line = text.lines().map(plain).find(|l| !l.is_empty()).unwrap_or_default();
    let words: Vec<&str> = line.split_whitespace().collect();
    if words.is_empty() {
        return "Untitled".to_owned();
    }
    let mut title = words.iter().take(WORDS).copied().collect::<Vec<_>>().join(" ");
    if words.len() > WORDS {
        title = title.trim_end_matches([',', ';', ':', '.']).to_owned();
        title.push('\u{2026}');
    }
    title
}

/// How Combine joins documents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Join {
    /// Each under its title as a heading with this many `#`s (more for
    /// what's in a subfolder).
    Headings(usize),
    /// A scene break between each.
    SceneBreaks,
    /// One after another, a blank line between.
    Together,
}

/// One item going into a combined document.
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub title: String,
    /// Empty for a folder.
    pub text: String,
    /// 0 for an item straight inside, 1 for one in a subfolder, and so on.
    pub depth: usize,
    pub document: bool,
}

/// The text of `parts` as one, after `lead` (the combined document's own
/// text, if it had any). Folders only add their title, as a heading.
pub fn combine(lead: &str, parts: &[Part], join: Join) -> String {
    let mut blocks: Vec<String> = Vec::new();
    if !lead.trim().is_empty() {
        blocks.push(lead.trim().to_owned());
    }
    for part in parts {
        let text = part.text.trim();
        match join {
            Join::Headings(level) => {
                let hashes = "#".repeat((level + part.depth).clamp(1, 6));
                let title = part.title.trim();
                blocks.push(if text.is_empty() { format!("{hashes} {title}") } else { format!("{hashes} {title}\n\n{text}") });
            }
            Join::SceneBreaks | Join::Together if part.document && !text.is_empty() => blocks.push(text.to_owned()),
            Join::SceneBreaks | Join::Together => {}
        }
    }
    let between = if join == Join::SceneBreaks { "\n\n#\n\n" } else { "\n\n" };
    blocks.join(between)
}

/// The top heading level used, for Break Up to start from: 1 if there are
/// `#` headings, else 2 if there are `##`, and so on.
pub fn first_heading_level(text: &str) -> Option<usize> {
    text.lines().filter_map(|l| markdown::heading_level(l.trim())).min()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn titles(b: &Broken) -> Vec<&str> {
        b.pieces.iter().map(|p| p.title.as_str()).collect()
    }

    #[test]
    fn breaks_at_headings_naming_each_piece() {
        let text = "A few words first.\n\n# On Walking\n\nI walk.\n\n## A detour\n\nStill walking.\n\n# The *Long* Way\n\nIt was long.\n";
        let b = break_up(text, BreakAt::Headings(1), false);
        assert_eq!(b.lead, "A few words first.");
        assert_eq!(titles(&b), ["On Walking", "The Long Way"]);
        assert_eq!(b.pieces[0].text, "I walk.\n\n## A detour\n\nStill walking.");
        assert_eq!(b.pieces[1].text, "It was long.");
    }

    #[test]
    fn deeper_headings_break_too_when_asked() {
        let text = "# One\n\na\n\n## Two\n\nb\n\n### Three\n\nc";
        assert_eq!(titles(&break_up(text, BreakAt::Headings(2), false)), ["One", "Two"]);
        assert_eq!(titles(&break_up(text, BreakAt::Headings(6), false)), ["One", "Two", "Three"]);
    }

    #[test]
    fn headings_can_stay_in_the_text() {
        let b = break_up("# One\n\na\n# Two\nb", BreakAt::Headings(1), true);
        assert_eq!(b.pieces[0].text, "# One\n\na");
        assert_eq!(b.pieces[1].text, "# Two\nb");
        assert_eq!(b.lead, "");
    }

    #[test]
    fn a_heading_with_nothing_under_it_is_still_a_piece() {
        let b = break_up("# One\n\n# Two\n\nwords", BreakAt::Headings(1), false);
        assert_eq!(titles(&b), ["One", "Two"]);
        assert_eq!(b.pieces[0].text, "");
    }

    #[test]
    fn scene_breaks_name_pieces_from_their_first_words() {
        let text = "Opening.\n\n* * *\n\nThe *snow* came early that year, and stayed.\n\n#\n\nShort one.\n\n---\n\n* * *\n";
        let b = break_up(text, BreakAt::SceneBreaks, false);
        assert_eq!(b.lead, "Opening.");
        assert_eq!(titles(&b), ["The snow came early that year\u{2026}", "Short one."]);
        assert_eq!(b.pieces[0].text, "The *snow* came early that year, and stayed.");
    }

    #[test]
    fn nothing_breaks_inside_a_note() {
        let text = "# One\n\na <!-- a note\n# not a heading\n-->\n\n# Two\n\nb";
        let b = break_up(text, BreakAt::Headings(1), false);
        assert_eq!(titles(&b), ["One", "Two"]);
        assert!(b.pieces[0].text.contains("# not a heading"));
    }

    #[test]
    fn no_breaks_means_no_pieces() {
        let b = break_up("Just words.\n\nMore.", BreakAt::Headings(1), false);
        assert!(b.pieces.is_empty());
        assert_eq!(b.lead, "Just words.\n\nMore.");
    }

    fn part(title: &str, text: &str, depth: usize) -> Part {
        Part { title: title.into(), text: text.into(), depth, document: true }
    }

    #[test]
    fn combines_under_headings_and_breaks_back_up_the_same() {
        let parts = [part("On Walking", "I walk.", 0), part("The Long Way", "It was long.\n\nVery.", 0)];
        let text = combine("Intro.", &parts, Join::Headings(1));
        assert_eq!(text, "Intro.\n\n# On Walking\n\nI walk.\n\n# The Long Way\n\nIt was long.\n\nVery.");
        let b = break_up(&text, BreakAt::Headings(1), false);
        assert_eq!(b.lead, "Intro.");
        assert_eq!(b.pieces, [Piece { title: "On Walking".into(), text: "I walk.".into() }, Piece { title: "The Long Way".into(), text: "It was long.\n\nVery.".into() }]);
    }

    #[test]
    fn subfolders_go_a_level_deeper() {
        let parts = [Part { title: "Part".into(), text: String::new(), depth: 0, document: false }, part("Inside", "x", 1)];
        assert_eq!(combine("", &parts, Join::Headings(2)), "## Part\n\n### Inside\n\nx");
    }

    #[test]
    fn combines_with_scene_breaks_or_together() {
        let parts = [part("A", "one", 0), Part { title: "F".into(), text: String::new(), depth: 0, document: false }, part("B", "", 0), part("C", "two", 0)];
        assert_eq!(combine("", &parts, Join::SceneBreaks), "one\n\n#\n\ntwo");
        assert_eq!(combine("lead", &parts, Join::Together), "lead\n\none\n\ntwo");
    }

    #[test]
    fn finds_the_first_heading_level_used() {
        assert_eq!(first_heading_level("## a\n### b"), Some(2));
        assert_eq!(first_heading_level("# a\n## b"), Some(1));
        assert_eq!(first_heading_level("no headings\n#"), None);
    }
}
