//! The Markdown preview beside the editor: rendered with egui_commonmark,
//! showing the document's images, and scrolling along with the editor.
//!
//! To scroll together, the preview is drawn a section at a time (cut at the
//! headings, which always start a new block, so nothing renders differently)
//! and remembers where on screen each section landed. When the editor's
//! scrolled, the preview puts the matching part of the matching section at
//! its top: the line at the top of the editor, a fraction of the way through
//! its section, shows that far through the section's rendering. Images and
//! headings, which make the two sides differ in height, then can't pull them
//! apart by more than a section.
//!
//! Link definitions (`[name]: url`) are shared by the whole document, so
//! they go with every section. Footnotes would then repeat in every section,
//! so a document with them is drawn whole and scrolled in proportion.

use std::path::Path;

/// Part of the document as the preview draws it.
#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    /// The source line it starts on (0-based), and how many it has.
    pub first_line: usize,
    pub lines: usize,
    /// Its Markdown, with image paths made whole and the link definitions.
    pub text: String,
}

/// Where a section landed in the preview's scrolling content, last frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Placed {
    pub first_line: usize,
    pub lines: usize,
    pub top: f32,
    pub height: f32,
}

/// What a window's preview remembers between frames.
#[derive(Default)]
pub struct PreviewState {
    pub placed: Vec<Placed>,
    /// The content's height and the view's, last frame.
    pub content_height: f32,
    pub view_height: f32,
    /// The editor position the preview was last moved to match: the tab,
    /// and its top line (`None` on a tab's first frame, before the preview
    /// has been laid out to match against).
    pub synced: Option<(sloth_core::tabs::TabId, Option<f32>)>,
}

/// An ATX heading (`#` to `######`, then a space or the end of the line).
fn is_heading(line: &str) -> bool {
    let trimmed = line.trim_start_matches(' ');
    if line.len() - trimmed.len() > 3 {
        return false;
    }
    let hashes = trimmed.chars().take_while(|&c| c == '#').count();
    (1..=6).contains(&hashes) && trimmed[hashes..].chars().next().is_none_or(|c| c == ' ' || c == '\t')
}

/// A code fence's opening or closing line: three or more ` or ~.
fn fence(line: &str) -> Option<char> {
    let trimmed = line.trim_start_matches(' ');
    if line.len() - trimmed.len() > 3 {
        return None;
    }
    let mark = trimmed.chars().next().filter(|&c| c == '`' || c == '~')?;
    (trimmed.chars().take_while(|&c| c == mark).count() >= 3).then_some(mark)
}

/// `[name]: url`, a link definition.
fn is_link_definition(line: &str) -> bool {
    let trimmed = line.trim_start_matches(' ');
    line.len() - trimmed.len() <= 3
        && trimmed.starts_with('[')
        && !trimmed.starts_with("[^")
        && trimmed.find("]:").is_some_and(|i| i > 1)
}

/// `[^1]: text`, a footnote.
fn is_footnote(line: &str) -> bool {
    let trimmed = line.trim_start_matches(' ');
    line.len() - trimmed.len() <= 3 && trimmed.starts_with("[^") && trimmed.contains("]:")
}

/// The document cut into sections at its headings (not ones inside code
/// blocks). With footnotes, one section for the whole document.
/// `folder`: the document's folder, which relative image paths start from.
pub fn sections(text: &str, folder: Option<&Path>) -> Vec<Section> {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut starts = vec![0];
    let mut in_fence: Option<char> = None;
    let mut definitions = Vec::new();
    let mut footnotes = false;
    for (i, line) in lines.iter().enumerate() {
        match (in_fence, fence(line)) {
            (None, Some(mark)) => in_fence = Some(mark),
            (Some(open), Some(mark)) if mark == open => in_fence = None,
            _ => {}
        }
        if in_fence.is_some() {
            continue;
        }
        if is_heading(line) && i > 0 {
            starts.push(i);
        }
        if is_link_definition(line) {
            definitions.push(*line);
        }
        footnotes |= is_footnote(line);
    }
    if footnotes {
        starts.truncate(1);
    }
    let shared = if definitions.is_empty() { String::new() } else { format!("\n\n{}", definitions.join("\n")) };
    starts
        .iter()
        .enumerate()
        .map(|(n, &first)| {
            let end = starts.get(n + 1).copied().unwrap_or(lines.len());
            let body = lines[first..end].join("\n");
            let body = match folder {
                Some(folder) => with_whole_image_paths(&body, folder),
                None => body,
            };
            // A section that has its own definitions doesn't need them twice.
            let text = if shared.is_empty() || starts.len() == 1 { body } else { format!("{body}{shared}") };
            Section { first_line: first, lines: end - first, text }
        })
        .collect()
}

/// `![alt](pictures/a.png)` with the path made whole from the document's
/// folder, so the preview can find it. Paths that are already whole, and
/// web addresses, are left as they are.
fn with_whole_image_paths(text: &str, folder: &Path) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("![") {
        let Some(close) = rest[at..].find("](").map(|i| at + i + 2) else { break };
        out.push_str(&rest[..close]);
        rest = &rest[close..];
        // `<a path with spaces.png>` or a plain one, up to a space or `)`.
        let (path, len, angled) = match rest.strip_prefix('<') {
            Some(inner) => match inner.find('>') {
                Some(end) => (&inner[..end], end + 2, true),
                None => continue,
            },
            None => {
                let end = rest.find([' ', ')']).unwrap_or(rest.len());
                (&rest[..end], end, false)
            }
        };
        let whole = !path.is_empty() && !path.contains("://") && !path.starts_with('/') && !path.starts_with('#') && !path.starts_with("data:");
        if whole {
            let full = folder.join(path);
            let full = full.to_string_lossy();
            if angled || full.contains(' ') {
                out.push_str(&format!("<{full}>"));
            } else {
                out.push_str(&full);
            }
        } else {
            out.push_str(&rest[..len]);
        }
        rest = &rest[len..];
    }
    out.push_str(rest);
    out
}

/// Where the preview should be scrolled to for the editor's `top_line`:
/// that far through the section it's in. Without sections (or before
/// they've been placed), in proportion to the whole document.
pub fn offset_for(state: &PreviewState, top_line: f32, total_lines: usize) -> f32 {
    let max = (state.content_height - state.view_height).max(0.0);
    let offset = match state.placed.iter().rev().find(|p| p.first_line as f32 <= top_line) {
        Some(p) if state.placed.len() > 1 => {
            let through = ((top_line - p.first_line as f32) / p.lines.max(1) as f32).clamp(0.0, 1.0);
            p.top + through * p.height
        }
        _ => top_line / total_lines.max(1) as f32 * state.content_height,
    };
    offset.clamp(0.0, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cut_at_headings_not_inside_code() {
        let text = "Intro\n# One\ntext\n```\n# not a heading\n```\n## Two\nmore\n####### seven hashes";
        let s = sections(text, None);
        let firsts: Vec<_> = s.iter().map(|s| (s.first_line, s.lines)).collect();
        assert_eq!(firsts, vec![(0, 1), (1, 5), (6, 3)]);
        assert!(s[1].text.contains("# not a heading"));
        assert_eq!(sections("#hashtag\n# Real", None).len(), 2, "#hashtag isn't a heading");
    }

    #[test]
    fn link_definitions_go_with_every_section_and_footnotes_keep_it_whole() {
        let text = "See [the site][s].\n# Later\nAgain [the site][s].\n\n[s]: https://example.org";
        let s = sections(text, None);
        assert_eq!(s.len(), 2);
        assert!(s[0].text.ends_with("[s]: https://example.org"));

        let text = "A note.[^1]\n# Later\n\n[^1]: The note.";
        assert_eq!(sections(text, None).len(), 1);
    }

    #[test]
    fn image_paths_are_made_whole() {
        let folder = Path::new("/Users/me/Notes");
        let fix = |t: &str| with_whole_image_paths(t, folder);
        assert_eq!(fix("![a](pics/a.png)"), "![a](/Users/me/Notes/pics/a.png)");
        assert_eq!(fix("![a](pics/a.png \"Title\")"), "![a](/Users/me/Notes/pics/a.png \"Title\")");
        assert_eq!(fix("![a](<my pic.png>)"), "![a](</Users/me/Notes/my pic.png>)");
        assert_eq!(fix("![a](/abs/a.png) ![b](https://x.org/b.png)"), "![a](/abs/a.png) ![b](https://x.org/b.png)");
        assert_eq!(fix("[not an image](a.png)"), "[not an image](a.png)");
    }

    #[test]
    fn scrolls_to_the_same_part_of_the_same_section() {
        let state = PreviewState {
            placed: vec![
                Placed { first_line: 0, lines: 10, top: 0.0, height: 100.0 },
                // A section with a big image: 10 lines, 600 points tall.
                Placed { first_line: 10, lines: 10, top: 100.0, height: 600.0 },
                Placed { first_line: 20, lines: 10, top: 700.0, height: 100.0 },
            ],
            content_height: 800.0,
            view_height: 200.0,
            synced: None,
        };
        assert_eq!(offset_for(&state, 5.0, 30), 50.0);
        assert_eq!(offset_for(&state, 15.0, 30), 400.0, "halfway through the image's section");
        assert_eq!(offset_for(&state, 25.0, 30), 600.0, "no further than the end");
        let whole = PreviewState { placed: vec![], ..state };
        assert_eq!(offset_for(&whole, 15.0, 30), 400.0, "in proportion");
    }
}
