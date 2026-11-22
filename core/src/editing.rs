//! Editing operations the text widget doesn't do itself: auto-indent,
//! indenting and outdenting lines, bracket matching, and smart dashes.
//!
//! These are pure functions over the text and a selection (character
//! indices, like the text cursor), so they're easy to test. Each returns the
//! new text and the selection to put back.

use std::ops::Range;

use crate::find::{byte_to_char, char_to_byte};

/// The result of an edit.
#[derive(Debug, PartialEq, Eq)]
pub struct Edit {
    pub text: String,
    /// New selection, in characters.
    pub selection: Range<usize>,
}

/// What one level of indentation is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Indent {
    /// Insert spaces instead of a tab character.
    pub spaces: bool,
    /// Width of a tab stop, in columns.
    pub width: usize,
}

/// Byte offset of the start of the line containing `byte`.
fn line_start(text: &str, byte: usize) -> usize {
    text[..byte].rfind('\n').map_or(0, |i| i + 1)
}

/// Replace a character range with `insert`, putting the cursor after it.
fn replace(text: &str, chars: Range<usize>, insert: &str) -> Edit {
    let (start, end) = (char_to_byte(text, chars.start), char_to_byte(text, chars.end));
    let mut out = String::with_capacity(text.len() + insert.len());
    out.push_str(&text[..start]);
    out.push_str(insert);
    out.push_str(&text[end..]);
    let cursor = chars.start + insert.chars().count();
    Edit {
        text: out,
        selection: cursor..cursor,
    }
}

/// Enter with auto-indent: a new line indented like the current one.
pub fn newline(text: &str, selection: Range<usize>) -> Edit {
    let start = char_to_byte(text, selection.start);
    let line = &text[line_start(text, start)..start];
    let indent: String = line.chars().take_while(|c| *c == ' ' || *c == '\t').collect();
    replace(text, selection, &format!("\n{indent}"))
}

/// Tab key. With a selection spanning lines, indent them; otherwise insert
/// one level of indentation (spaces up to the next tab stop, or a tab).
pub fn tab(text: &str, selection: Range<usize>, indent: Indent) -> Edit {
    if spans_lines(text, &selection) {
        return indent_lines(text, selection, indent);
    }
    let insert = if indent.spaces {
        let start = char_to_byte(text, selection.start);
        let column = text[line_start(text, start)..start].chars().count();
        " ".repeat(indent.width - column % indent.width)
    } else {
        "\t".to_owned()
    };
    replace(text, selection, &insert)
}

fn spans_lines(text: &str, selection: &Range<usize>) -> bool {
    let (start, end) = (char_to_byte(text, selection.start), char_to_byte(text, selection.end));
    text[start..end].contains('\n')
}

/// The byte ranges of the lines touched by a selection. A selection that
/// ends at the very start of a line doesn't include that line.
fn selected_lines(text: &str, selection: &Range<usize>) -> Vec<Range<usize>> {
    let start = char_to_byte(text, selection.start);
    let mut end = char_to_byte(text, selection.end);
    if end > start && text[..end].ends_with('\n') {
        end -= 1;
    }
    let mut lines = Vec::new();
    let mut pos = line_start(text, start);
    loop {
        let line_end = text[pos..].find('\n').map_or(text.len(), |i| pos + i);
        lines.push(pos..line_end);
        if line_end >= end || line_end == text.len() {
            break;
        }
        pos = line_end + 1;
    }
    lines
}

/// Indent every line in the selection by one level (blank lines are skipped).
pub fn indent_lines(text: &str, selection: Range<usize>, indent: Indent) -> Edit {
    let unit = if indent.spaces { " ".repeat(indent.width) } else { "\t".to_owned() };
    edit_lines(text, selection, |line| {
        if line.trim().is_empty() { (0, String::new()) } else { (0, unit.clone()) }
    })
}

/// Remove one level of indentation from every line in the selection.
pub fn outdent_lines(text: &str, selection: Range<usize>, indent: Indent) -> Edit {
    edit_lines(text, selection, |line| {
        let remove = if line.starts_with('\t') {
            1
        } else {
            line.chars().take(indent.width).take_while(|c| *c == ' ').count()
        };
        (remove, String::new())
    })
}

/// Apply `change` to the start of each selected line: it returns how many
/// bytes to remove there and what to insert. Keeps the selection on the
/// same text where possible.
fn edit_lines(
    text: &str,
    selection: Range<usize>,
    change: impl Fn(&str) -> (usize, String),
) -> Edit {
    let lines = selected_lines(text, &selection);
    let (sel_start, sel_end) = (char_to_byte(text, selection.start), char_to_byte(text, selection.end));

    let mut out = String::with_capacity(text.len() + lines.len() * 4);
    let mut last = 0;
    // Byte deltas, to move the selection ends.
    let (mut start_shift, mut end_shift) = (0isize, 0isize);
    for line in &lines {
        let (remove, insert) = change(&text[line.clone()]);
        out.push_str(&text[last..line.start]);
        out.push_str(&insert);
        last = line.start + remove;
        let delta = insert.len() as isize - remove as isize;
        // A selection end inside removed indentation snaps to the line start.
        let shift_for = |pos: usize| {
            if pos >= line.start + remove { delta } else { line.start as isize - pos as isize }
        };
        if sel_start >= line.start {
            start_shift += if sel_start > line.end { delta } else { shift_for(sel_start) };
        }
        if sel_end >= line.start {
            end_shift += if sel_end > line.end { delta } else { shift_for(sel_end) };
        }
    }
    out.push_str(&text[last..]);

    let new_start = (sel_start as isize + start_shift).max(0) as usize;
    let new_end = (sel_end as isize + end_shift).max(new_start as isize) as usize;
    Edit {
        selection: byte_to_char(&out, new_start)..byte_to_char(&out, new_end),
        text: out,
    }
}

/// If the cursor is next to a bracket, the character indices of it and its
/// partner. Checks the character after the cursor first, then the one before.
pub fn matching_bracket(text: &str, cursor: usize) -> Option<(usize, usize)> {
    const PAIRS: [(char, char); 3] = [('(', ')'), ('[', ']'), ('{', '}')];
    /// Don't scan huge files end to end on every frame.
    const LIMIT: usize = 100_000;

    let at = char_to_byte(text, cursor);
    let after = text[at..].chars().next().map(|c| (cursor, at, c));
    let before = text[..at].chars().next_back().map(|c| (cursor - 1, at - c.len_utf8(), c));

    for (index, byte, c) in [after, before].into_iter().flatten() {
        if let Some(&(open, close)) = PAIRS.iter().find(|(o, _)| *o == c) {
            let mut depth = 0;
            let rest = text[byte..].chars().enumerate().take(LIMIT);
            for (offset, ch) in rest {
                if ch == open {
                    depth += 1;
                } else if ch == close {
                    depth -= 1;
                    if depth == 0 {
                        return Some((index, index + offset));
                    }
                }
            }
            return None;
        }
        if let Some(&(open, close)) = PAIRS.iter().find(|(_, cl)| *cl == c) {
            let mut depth = 0;
            let before = text[..byte + c.len_utf8()].chars().rev().enumerate().take(LIMIT);
            for (offset, ch) in before {
                if ch == close {
                    depth += 1;
                } else if ch == open {
                    depth -= 1;
                    if depth == 0 {
                        return Some((index - offset, index));
                    }
                }
            }
            return None;
        }
    }
    None
}

/// Smart dashes: should a `-` typed right after `tail` (the last few
/// characters before the cursor) turn the `--` into an em dash? Not in the
/// `<!--` that opens an HTML comment, nor anywhere inside one, so its closing
/// `-->` stays as typed too.
pub fn makes_em_dash(tail: &str, in_comment: bool) -> bool {
    !in_comment && tail.ends_with('-') && !tail.ends_with("<!-")
}

/// Is the end of `before` inside an HTML comment (`<!--` not yet closed)?
pub fn in_html_comment(before: &str) -> bool {
    before.rfind("<!--").is_some_and(|open| !before[open + 4..].contains("-->"))
}

/// Words and characters, for the status bar.
pub fn counts(text: &str) -> (usize, usize) {
    (text.split_whitespace().count(), text.chars().count())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABS: Indent = Indent { spaces: false, width: 4 };
    const SPACES: Indent = Indent { spaces: true, width: 4 };

    /// Write text with `|` marking the cursor or `[...]` a selection.
    fn parse(marked: &str) -> (String, Range<usize>) {
        let text: String = marked.chars().filter(|c| !"|[]".contains(*c)).collect();
        let find = |m| marked.chars().position(|c| c == m);
        let sel = match find('|') {
            Some(i) => i..i,
            None => find('[').unwrap()..find(']').unwrap() - 1,
        };
        (text, sel)
    }

    fn show(edit: &Edit) -> String {
        let chars: Vec<char> = edit.text.chars().collect();
        let mut out = String::new();
        for (i, c) in chars.iter().enumerate() {
            if edit.selection.is_empty() && i == edit.selection.start {
                out.push('|');
            } else if !edit.selection.is_empty() && i == edit.selection.start {
                out.push('[');
            }
            if !edit.selection.is_empty() && i == edit.selection.end {
                out.push(']');
            }
            out.push(*c);
        }
        if edit.selection.start == chars.len() && edit.selection.is_empty() {
            out.push('|');
        } else if edit.selection.end == chars.len() && !edit.selection.is_empty() {
            out.push(']');
        }
        out
    }

    fn run(marked: &str, f: impl Fn(&str, Range<usize>) -> Edit) -> String {
        let (text, sel) = parse(marked);
        show(&f(&text, sel))
    }

    #[test]
    fn newline_keeps_indentation() {
        assert_eq!(run("    foo|", newline), "    foo\n    |");
        assert_eq!(run("\tfoo|bar", newline), "\tfoo\n\t|bar");
        assert_eq!(run("  [foo]", newline), "  \n  |");
        // Cursor inside the indentation only copies what's before it.
        assert_eq!(run("  |  foo", newline), "  \n  |  foo");
    }

    #[test]
    fn tab_inserts_to_next_stop() {
        assert_eq!(run("ab|", |t, s| tab(t, s, SPACES)), "ab  |");
        assert_eq!(run("abcd|", |t, s| tab(t, s, SPACES)), "abcd    |");
        assert_eq!(run("ab|", |t, s| tab(t, s, TABS)), "ab\t|");
    }

    #[test]
    fn tab_with_multi_line_selection_indents() {
        assert_eq!(run("[a\nb]", |t, s| tab(t, s, SPACES)), "    [a\n    b]");
    }

    #[test]
    fn indent_skips_blank_lines_and_keeps_selection() {
        assert_eq!(run("a\n[b\n\nc]\nd", |t, s| indent_lines(t, s, TABS)), "a\n\t[b\n\n\tc]\nd");
        // Cursor stays on its text.
        assert_eq!(run("ab|c", |t, s| indent_lines(t, s, TABS)), "\tab|c");
    }

    #[test]
    fn selection_ending_at_line_start_excludes_that_line() {
        assert_eq!(run("[a\n]b", |t, s| indent_lines(t, s, TABS)), "\t[a\n]b");
    }

    #[test]
    fn outdent_removes_one_level() {
        assert_eq!(run("[\ta\n      b\n  c]", |t, s| outdent_lines(t, s, SPACES)), "[a\n  b\nc]");
        assert_eq!(run("  |  x", |t, s| outdent_lines(t, s, SPACES)), "|x");
        assert_eq!(run("nothing|", |t, s| outdent_lines(t, s, SPACES)), "nothing|");
    }

    #[test]
    fn finds_matching_brackets() {
        let text = "f(a[1], {b})";
        assert_eq!(matching_bracket(text, 1), Some((1, 11))); // before "("
        assert_eq!(matching_bracket(text, 12), Some((1, 11))); // after ")"
        assert_eq!(matching_bracket(text, 3), Some((3, 5))); // "[" at cursor
        assert_eq!(matching_bracket(text, 9), Some((8, 10))); // just after "{"
        assert_eq!(matching_bracket(text, 0), None);
        assert_eq!(matching_bracket("(()", 0), None); // unbalanced
        assert_eq!(matching_bracket("é(x)", 1), Some((1, 3))); // multibyte before
    }

    #[test]
    fn counts_words_and_characters() {
        assert_eq!(counts("héllo  big\nworld"), (3, 16));
        assert_eq!(counts(""), (0, 0));
    }

    #[test]
    fn a_second_dash_makes_an_em_dash() {
        assert!(makes_em_dash("Wait-", false));
        assert!(makes_em_dash("-", false));
        assert!(!makes_em_dash("Wait", false));
        assert!(!makes_em_dash("", false));
        assert!(!makes_em_dash("a\u{2014}", false), "after an em dash, a dash is just a dash");
        assert!(!makes_em_dash("<!-", false), "an HTML comment's opener");
        assert!(!makes_em_dash("e -", true), "inside a comment");
    }

    #[test]
    fn knows_when_inside_an_html_comment() {
        assert!(in_html_comment("text <!-- a note"));
        assert!(!in_html_comment("text <!-- a note --> more"));
        assert!(in_html_comment("<!-- one --> <!-- two"));
        assert!(!in_html_comment("no comment"));
    }
}
