//! References: `[[Ways of Seeing]]` in the text points at a profile (a
//! source, a concept, a theme, a character: any profile folder's items).
//! Like a note in the margin, a reference isn't part of the prose: compile,
//! formatted copy and word counts leave it out.
//!
//! What's inside the brackets is the profile's name. `Kind: Name` picks the
//! kind when two share a name (`Concept: attention`, by the kind's item or
//! folder name), and anything after `|` is a pinpoint (`|p. 47`), kept for
//! the writer and ignored in finding the profile.
//!
//! A document's sections start at its headings (`#`, `##`, `###`…), so a
//! reference can be placed by the sub-essay it's in.

use crate::project::{Node, Project};

/// The profile a reference names: its kind's id and its own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub kind: String,
    pub id: String,
}

/// What's inside a reference's brackets: the name (with any `Kind:`), and
/// any pinpoint after `|`.
pub fn split(inner: &str) -> (&str, Option<&str>) {
    match inner.split_once('|') {
        Some((name, pin)) => (name.trim(), Some(pin.trim()).filter(|p| !p.is_empty())),
        None => (inner.trim(), None),
    }
}

/// The profile a reference names, if there is one. Names match without
/// regard to case; with no `Kind:`, the first profile folder (in binder
/// order) with that name wins.
pub fn resolve(project: &Project, inner: &str) -> Option<Target> {
    let (name, _) = split(inner);
    let same = |a: &str, b: &str| a.trim().to_lowercase() == b.trim().to_lowercase();
    // `Kind: Name`, if the part before the colon is a kind.
    let (wanted, name) = match kind_prefix(project, name) {
        Some((kind, rest)) => (Some(kind.id.clone()), rest),
        None => (None, name),
    };
    project
        .profile_kinds()
        .iter().filter(|k| wanted.as_ref().is_none_or(|w| *w == k.id)).find_map(|kind| {
        project.profile_items(&kind.id).into_iter().find(|n| same(&n.title, name)).map(|n| Target { kind: kind.id.clone(), id: n.id.clone() })
    })
}

/// A kind named by the start of a reference (`Concept: …`): the kind and
/// the rest, trimmed. `None` if what's before the colon isn't a kind.
fn kind_prefix<'a>(project: &'a Project, text: &'a str) -> Option<(&'a crate::project::ProfileKind, &'a str)> {
    let (kind, rest) = text.split_once(':')?;
    let kind = kind.trim().to_lowercase();
    let found = project.profile_kinds().iter().find(|k| k.item.to_lowercase() == kind || k.name.to_lowercase() == kind)?;
    Some((found, rest.trim()))
}

/// A reference being typed: the cursor (a byte of `text`) is after a `[[`
/// on its line that isn't closed before it. Gives where the `[[` starts
/// and what's been typed since.
pub fn typing_at(text: &str, cursor: usize) -> Option<(usize, &str)> {
    let before = text.get(..cursor)?;
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    let open = before[line_start..].rfind("[[").map(|i| line_start + i)?;
    let typed = &before[open + 2..];
    (!typed.contains(']') && !typed.contains('[')).then_some((open, typed))
}

/// One suggestion for a reference being typed.
#[derive(Clone, Debug, PartialEq)]
pub struct Suggestion {
    /// The profile's name, or "New Concept “attention”".
    pub label: String,
    /// The kind of profile, as one of them ("Concept").
    pub kind_item: String,
    /// What goes between the brackets.
    pub insert: String,
    /// Make a profile of this kind (its id) called `name` first.
    pub create: Option<(String, String)>,
}

/// Most suggestions offered at once (before the New … ones).
const SUGGESTIONS: usize = 8;

/// Profiles whose names contain what's typed (those starting with it
/// first), then, unless one is called exactly that, a New … for each kind.
/// `Kind: …` keeps to that kind.
pub fn suggestions(project: &Project, typed: &str) -> Vec<Suggestion> {
    let (only, query) = match kind_prefix(project, typed) {
        Some((kind, rest)) => (Some(kind.id.clone()), rest),
        None => (None, typed.trim()),
    };
    let q = query.to_lowercase();
    let kinds: Vec<_> = project.profile_kinds().iter().filter(|k| only.as_ref().is_none_or(|o| *o == k.id)).collect();
    // How many kinds have a profile of each name: shared names get `Kind:`.
    let mut names: std::collections::HashMap<String, usize> = Default::default();
    for kind in project.profile_kinds() {
        for n in project.profile_items(&kind.id) {
            *names.entry(n.title.to_lowercase()).or_default() += 1;
        }
    }
    let mut found: Vec<(bool, Suggestion)> = Vec::new();
    for kind in &kinds {
        for n in project.profile_items(&kind.id) {
            let title = n.title.to_lowercase();
            if !title.contains(&q) {
                continue;
            }
            let shared = names.get(&title).copied().unwrap_or(0) > 1;
            let insert = if shared || only.is_some() { format!("{}: {}", kind.item, n.title) } else { n.title.clone() };
            found.push((title.starts_with(&q), Suggestion { label: n.title.clone(), kind_item: kind.item.clone(), insert, create: None }));
        }
    }
    // Names starting with what's typed first; otherwise binder order.
    found.sort_by_key(|(starts, _)| !starts);
    let mut out: Vec<Suggestion> = found.into_iter().map(|(_, s)| s).take(SUGGESTIONS).collect();
    let exact = kinds.iter().any(|k| project.profile_items(&k.id).iter().any(|n| n.title.to_lowercase() == q));
    if !query.is_empty() && !exact {
        for kind in &kinds {
            out.push(Suggestion {
                label: format!("New {} \u{201c}{query}\u{201d}", kind.item),
                kind_item: kind.item.clone(),
                insert: if kinds.len() > 1 && only.is_none() { query.to_owned() } else { format!("{}: {query}", kind.item) },
                create: Some((kind.id.clone(), query.to_owned())),
            });
        }
    }
    out
}

/// After a profile is renamed: `text` with its references to it given the
/// new name (keeping any `Kind:` and `|pinpoint`), or `None` if it has
/// none. `project` is as it was before the rename.
pub fn renamed(project: &Project, text: &str, target: &Target, new_name: &str) -> Option<String> {
    let mut out = text.to_owned();
    let mut changed = false;
    for (range, inner) in sloth_core::markdown::references(text).into_iter().rev() {
        if resolve(project, inner).as_ref() != Some(target) {
            continue;
        }
        let (name, pin) = split(inner);
        let kind = kind_prefix(project, name).map(|(k, _)| format!("{}: ", k.item)).unwrap_or_default();
        let pin = pin.map(|p| format!("|{p}")).unwrap_or_default();
        out.replace_range(range, &format!("[[{kind}{}{pin}]]", new_name.trim()));
        changed = true;
    }
    changed.then_some(out)
}

/// Longest a line can be and still be a section's title (longer, it's
/// the start of a paragraph).
const TITLE_MAX: usize = 100;

/// A line that only separates: `#`s alone (up to six), or three or more
/// of `*`, `-` or `+` (spaces allowed between).
fn is_separator(line: &str) -> bool {
    let marks: String = line.chars().filter(|c| !c.is_whitespace()).collect();
    let hashes = !marks.is_empty() && marks.len() <= 6 && marks.chars().all(|c| c == '#');
    hashes || (marks.len() >= 3 && ['*', '-', '+'].iter().any(|&m| marks.chars().all(|c| c == m)))
}

/// A heading line's text: `## On images` gives "On images".
fn heading_text(line: &str) -> Option<&str> {
    let hashes = line.chars().take_while(|&c| c == '#').count();
    let title = line[hashes..].strip_prefix(' ')?.trim();
    ((1..=6).contains(&hashes) && !title.is_empty()).then_some(title)
}

/// A line as a title: without Markdown or references, if anything's left.
fn as_title(line: &str) -> Option<String> {
    let plain = sloth_core::markdown::formatted(line).plain;
    let plain = plain.trim();
    (!plain.is_empty()).then(|| plain.to_owned())
}

/// The title of the section a byte of `text` is in. A section starts at a
/// heading (`## On images`), or at a separator line (`#`, `##`, `***`,
/// `+++`) whose title is the first short line of text after it, as in
///
/// ```text
/// #
///
/// **wabi sabi** (September 2026)
/// ```
///
/// A separator followed straight away by a paragraph is a break inside a
/// section, so the search goes on to the one before.
pub fn section_heading(text: &str, at: usize) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    // The line `at` is on.
    let here = text[..at.min(text.len())].matches('\n').count();
    for i in (0..=here.min(lines.len().saturating_sub(1))).rev() {
        let line = lines[i].trim();
        if let Some(title) = heading_text(line) {
            return as_title(title);
        }
        if !is_separator(line) {
            continue;
        }
        // The first line with words after it (references alone don't count).
        let first = lines[i + 1..].iter().find_map(|l| as_title(l));
        if let Some(title) = first.filter(|t| t.chars().count() <= TITLE_MAX) {
            return Some(title);
        }
    }
    None
}

/// One place a profile is referenced.
#[derive(Clone, Debug, PartialEq)]
pub struct Backlink {
    /// The document it's in.
    pub doc: String,
    pub doc_title: String,
    /// The heading of the section (sub-essay) it's in, if it's under one.
    pub section: Option<String>,
    /// The line it's on, without references or Markdown, shortened.
    pub snippet: String,
    /// Any pinpoint (`|p. 47`).
    pub pin: Option<String>,
    /// Where the reference is, in bytes.
    pub at: std::ops::Range<usize>,
}

/// Longest a backlink's snippet gets, in characters.
const SNIPPET: usize = 90;

/// Every reference to a profile, in binder order (documents outside the
/// Trash). `text` gives a document's text (as open, or as saved).
pub fn backlinks(project: &Project, target: &Target, text: impl Fn(&Node) -> Option<String>) -> Vec<Backlink> {
    let mut out = Vec::new();
    for node in project.documents() {
        if node.id == target.id {
            continue;
        }
        let Some(text) = text(node) else { continue };
        for (range, inner) in sloth_core::markdown::references(&text) {
            if resolve(project, inner).as_ref() != Some(target) {
                continue;
            }
            let line_start = text[..range.start].rfind('\n').map_or(0, |i| i + 1);
            let line_end = text[range.end..].find('\n').map_or(text.len(), |i| range.end + i);
            out.push(Backlink {
                doc: node.id.clone(),
                doc_title: node.title.clone(),
                section: section_heading(&text, line_start),
                snippet: snippet(&text[line_start..line_end]),
                pin: split(inner).1.map(str::to_owned),
                at: range,
            });
        }
    }
    out
}

/// A line as it reads: without references or Markdown, shortened.
fn snippet(line: &str) -> String {
    let plain = sloth_core::markdown::formatted(line).plain;
    let chars: Vec<char> = plain.chars().collect();
    if chars.len() > SNIPPET {
        format!("{}\u{2026}", chars[..SNIPPET].iter().collect::<String>().trim_end())
    } else {
        plain
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{Place, profile_presets};
    use std::fs;
    use std::path::PathBuf;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("sloth-writer-refs-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// A project with Sources and Concepts, each with "Attention", and a
    /// source "Ways of Seeing".
    fn project(dir: &TempDir) -> (Project, String, String, String) {
        let mut p = Project::create(&dir.0, "Essays").unwrap();
        for id in ["sources", "concepts"] {
            let kind = profile_presets().into_iter().map(|(k, _)| k).find(|k| k.id == id).unwrap();
            p.add_profile_folder(kind).unwrap();
        }
        let berger = p.add_document(&Place::Into("sources".into()), "Ways of Seeing").unwrap();
        let source = p.add_document(&Place::Into("sources".into()), "Attention").unwrap();
        let concept = p.add_document(&Place::Into("concepts".into()), "Attention").unwrap();
        (p, berger, source, concept)
    }

    #[test]
    fn a_reference_finds_its_profile() {
        let dir = TempDir::new("resolve");
        let (p, berger, source, concept) = project(&dir);
        let id = |inner: &str| resolve(&p, inner).map(|t| t.id);
        assert_eq!(id("Ways of Seeing").as_ref(), Some(&berger));
        assert_eq!(id(" ways of seeing | p. 47 ").as_ref(), Some(&berger), "case, spaces and a pinpoint");
        assert_eq!(id("Attention").as_ref(), Some(&source), "the first folder wins");
        assert_eq!(id("Concept: Attention").as_ref(), Some(&concept), "unless the kind is named");
        assert_eq!(id("concepts: attention").as_ref(), Some(&concept), "by its folder's name too");
        assert_eq!(id("Nobody"), None);
        assert_eq!(split("Ways of Seeing|p. 47"), ("Ways of Seeing", Some("p. 47")));
    }

    #[test]
    fn a_reference_being_typed_is_noticed() {
        let text = "Done [[x]] and now [[Ways of";
        assert_eq!(typing_at(text, text.len()), Some((text.rfind("[[").unwrap(), "Ways of")));
        assert_eq!(typing_at(text, 9), None, "inside a finished one: no");
        assert_eq!(typing_at("[[a\nb", 6), None, "not past the end of a line");
        assert_eq!(typing_at("x [[", 4), Some((2, "")));
    }

    #[test]
    fn suggestions_match_what_is_typed() {
        let dir = TempDir::new("suggest");
        let (p, ..) = project(&dir);
        let labels = |typed: &str| suggestions(&p, typed).into_iter().map(|s| (s.label, s.insert)).collect::<Vec<_>>();
        assert_eq!(labels("ways")[0], ("Ways of Seeing".into(), "Ways of Seeing".into()));
        // A name in two kinds says which.
        assert_eq!(
            labels("att")[..2],
            [("Attention".into(), "Source: Attention".into()), ("Attention".into(), "Concept: Attention".into())]
        );
        // Nothing called that: offer to make one, of each kind (every
        // project has Characters).
        let new = suggestions(&p, "envy");
        let made: Vec<_> = new.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(made, ["New Character \u{201c}envy\u{201d}", "New Source \u{201c}envy\u{201d}", "New Concept \u{201c}envy\u{201d}"]);
        assert_eq!(new[2].create, Some(("concepts".into(), "envy".into())));
        // `Kind:` keeps to that kind.
        let concept = suggestions(&p, "Concept: env");
        assert_eq!(concept.len(), 1);
        assert_eq!(concept[0].insert, "Concept: env");
        assert!(suggestions(&p, "").len() >= 3, "everything, before anything's typed");
    }

    #[test]
    fn renaming_a_profile_renames_its_references() {
        let dir = TempDir::new("rename");
        let (p, berger, ..) = project(&dir);
        let target = Target { kind: "sources".into(), id: berger };
        let text = "A [[Ways of Seeing|p. 4]] b [[source: ways of seeing]] c [[Attention]].";
        assert_eq!(
            renamed(&p, text, &target, "Berger 1972").as_deref(),
            Some("A [[Berger 1972|p. 4]] b [[Source: Berger 1972]] c [[Attention]].")
        );
        assert_eq!(renamed(&p, "No references.", &target, "x"), None);
    }

    #[test]
    fn sections_start_at_headings() {
        let text = "Intro.\n# Part\nA.\n#\n## Sub-essay [[x]]\nB.";
        assert_eq!(section_heading(text, 3), None);
        assert_eq!(section_heading(text, text.find("A.").unwrap()).as_deref(), Some("Part"));
        assert_eq!(section_heading(text, text.find("B.").unwrap()).as_deref(), Some("Sub-essay"), "without references");
    }

    #[test]
    fn sections_start_at_separators_with_a_title_after_them() {
        // As in an essay collection imported from Scrivener.
        let text = "\n##\n[[Social Media|behaving badly]]\n**behaving badly**\nI was surprised by how driving behaviour differed so much when I vacationed in a country I had never visited before, it seemed.\n\
                    #\n\n**wabi sabi** (September 2026)\nI cling to this idea [[Wabi sabi]].\n\
                    #\nA scene break inside, followed straight away by a long paragraph that goes on well past the length of any title at all.\nMore [[Wabi sabi]].\n\
                    +++\n\npost holes\n\nShortly after we moved in [[Fences]].";
        let at = |needle: &str, nth: usize| text.match_indices(needle).nth(nth).unwrap().0;
        let section = |a: usize| section_heading(text, a);
        assert_eq!(section(at("[[Social", 0)).as_deref(), Some("behaving badly"), "the title can come after the reference");
        assert_eq!(section(at("[[Wabi", 0)).as_deref(), Some("wabi sabi (September 2026)"));
        assert_eq!(section(at("[[Wabi", 1)).as_deref(), Some("wabi sabi (September 2026)"), "a break inside a section");
        assert_eq!(section(at("[[Fences", 0)).as_deref(), Some("post holes"));
        assert!(is_separator("++++++++++++++") && is_separator("* * *") && is_separator("##") && !is_separator("#######"));
    }

    #[test]
    fn backlinks_list_every_reference_by_section() {
        let dir = TempDir::new("backlinks");
        let (mut p, berger, ..) = project(&dir);
        let essay = p.add_document(&Place::Into("manuscript".into()), "Looking").unwrap();
        let text = "## On images\nThe gaze [[Ways of Seeing|p. 47]] is **learned**.\n\n## On ads\nPublicity [[ways of seeing]] sells envy.";
        fs::write(p.file_path(&essay).unwrap(), text).unwrap();
        let target = Target { kind: "sources".into(), id: berger };
        let links = backlinks(&p, &target, |n| p.file_path(&n.id).and_then(|f| fs::read_to_string(f).ok()));
        let summary: Vec<_> = links.iter().map(|b| (b.doc_title.as_str(), b.section.as_deref(), b.snippet.as_str(), b.pin.as_deref())).collect();
        assert_eq!(
            summary,
            [("Looking", Some("On images"), "The gaze is learned.", Some("p. 47")), ("Looking", Some("On ads"), "Publicity sells envy.", None)]
        );
        assert_eq!(&text[links[0].at.clone()], "[[Ways of Seeing|p. 47]]");
    }
}
