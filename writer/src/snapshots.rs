//! Snapshots: saved copies of a document, taken before a big rewrite.
//!
//! Each is a plain file in the project, named by when it was taken and an
//! optional name, so they can be read in Finder without the app:
//!
//! ```text
//! snapshots/7f3k2a/2026-11-22 14.30.05 before cutting the flashback.md
//! ```
//!
//! Also here: a word-by-word comparison of two versions, for the Compare
//! window.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use chrono::{Local, NaiveDateTime};
use similar::{ChangeTag, TextDiff};

/// The folder in a project that holds every document's snapshots.
pub const FOLDER: &str = "snapshots";
/// How the time is written at the start of a snapshot's file name.
const STAMP: &str = "%Y-%m-%d %H.%M.%S";

#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub path: PathBuf,
    pub taken: NaiveDateTime,
    /// Empty if it wasn't given one.
    pub name: String,
}

impl Snapshot {
    pub fn text(&self) -> io::Result<String> {
        fs::read_to_string(&self.path)
    }

    /// "22 Nov 2026, 14:30".
    pub fn when(&self) -> String {
        self.taken.format("%-d %b %Y, %H:%M").to_string()
    }
}

fn folder(root: &Path, doc_id: &str) -> PathBuf {
    root.join(FOLDER).join(doc_id)
}

/// Save `text` as a snapshot of document `doc_id`, taken now.
pub fn take(root: &Path, doc_id: &str, text: &str, name: &str) -> io::Result<Snapshot> {
    take_at(root, doc_id, text, name, Local::now().naive_local())
}

pub fn take_at(root: &Path, doc_id: &str, text: &str, name: &str, taken: NaiveDateTime) -> io::Result<Snapshot> {
    let dir = folder(root, doc_id);
    fs::create_dir_all(&dir)?;
    let name = clean_name(name);
    let stem = taken.format(STAMP).to_string();
    let base = if name.is_empty() { stem } else { format!("{stem} {name}") };
    // Two in the same second get a number, rather than one replacing the other.
    let path = (1..)
        .map(|n| if n == 1 { dir.join(format!("{base}.md")) } else { dir.join(format!("{base} ({n}).md")) })
        .find(|p| !p.exists())
        .expect("some number is free");
    sloth_core::document::write_atomically(&path, text.as_bytes())?;
    Ok(Snapshot { path, taken, name })
}

/// A name that's safe in a file name: no slashes or colons, one line.
fn clean_name(name: &str) -> String {
    // Characters Windows doesn't allow in a name become spaces, so the
    // project's snapshots can go to any computer.
    let forbidden = |c: char| matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || c.is_control();
    let cleaned: String = name.chars().map(|c| if forbidden(c) { ' ' } else { c }).collect();
    let cleaned: String = cleaned.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(80).collect();
    // Nor a name ending in a dot.
    cleaned.trim_end_matches(['.', ' ']).to_owned()
}

/// A document's snapshots, newest first. Files that aren't snapshots are
/// ignored.
pub fn list(root: &Path, doc_id: &str) -> Vec<Snapshot> {
    let Ok(entries) = fs::read_dir(folder(root, doc_id)) else { return Vec::new() };
    let mut found: Vec<Snapshot> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "md"))
        .filter_map(|path| {
            let stem = path.file_stem()?.to_str()?.to_owned();
            let stamp = stem.get(..19)?;
            let taken = NaiveDateTime::parse_from_str(stamp, STAMP).ok()?;
            let name = without_copy_number(stem[19..].trim()).to_owned();
            Some(Snapshot { path, taken, name })
        })
        .collect();
    found.sort_by(|a, b| b.taken.cmp(&a.taken).then_with(|| b.path.cmp(&a.path)));
    found
}

/// "before the edit (2)" → "before the edit": the number only keeps two
/// snapshots from the same second apart.
fn without_copy_number(name: &str) -> &str {
    let Some(open) = name.rfind('(') else { return name };
    let inside = &name[open + 1..];
    match inside.strip_suffix(')') {
        Some(n) if !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) => name[..open].trim_end(),
        _ => name,
    }
}

pub fn delete(snapshot: &Snapshot) -> io::Result<()> {
    fs::remove_file(&snapshot.path)
}

/// Remove all of a document's snapshots (when it's deleted for good).
pub fn delete_all(root: &Path, doc_id: &str) -> io::Result<()> {
    match fs::remove_dir_all(folder(root, doc_id)) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// One piece of a comparison.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Piece {
    Same(String),
    /// In the older version only.
    Removed(String),
    /// In the newer version only.
    Added(String),
}

/// Compare two versions word by word (spaces and line breaks kept), joining
/// neighbouring pieces of the same kind.
pub fn compare(older: &str, newer: &str) -> Vec<Piece> {
    let diff = TextDiff::from_words(older, newer);
    let mut out: Vec<Piece> = Vec::new();
    for change in diff.iter_all_changes() {
        let text = change.value();
        match (out.last_mut(), change.tag()) {
            (Some(Piece::Same(s)), ChangeTag::Equal)
            | (Some(Piece::Removed(s)), ChangeTag::Delete)
            | (Some(Piece::Added(s)), ChangeTag::Insert) => s.push_str(text),
            (_, ChangeTag::Equal) => out.push(Piece::Same(text.to_owned())),
            (_, ChangeTag::Delete) => out.push(Piece::Removed(text.to_owned())),
            (_, ChangeTag::Insert) => out.push(Piece::Added(text.to_owned())),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn snapshot_names_work_on_windows_too() {
        assert_eq!(clean_name("Before: the \"big\" edit?"), "Before the big edit");
        assert_eq!(clean_name("Draft 2..."), "Draft 2");
        assert_eq!(clean_name("  a\nb  "), "a b");
    }

    struct TempDir(PathBuf);
    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("sloth-writer-snap-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            Self(dir)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn at(h: u32, m: u32, s: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, 22).unwrap().and_hms_opt(h, m, s).unwrap()
    }

    #[test]
    fn snapshots_are_readable_files_listed_newest_first() {
        let dir = TempDir::new("list");
        take_at(&dir.0, "abc123", "first draft", "", at(9, 0, 0)).unwrap();
        let named = take_at(&dir.0, "abc123", "second", "before: cutting/the flashback", at(14, 30, 5)).unwrap();
        assert_eq!(
            named.path.file_name().unwrap().to_str().unwrap(),
            "2026-11-22 14.30.05 before cutting the flashback.md"
        );
        // Same second: kept side by side.
        let again = take_at(&dir.0, "abc123", "third", "", at(9, 0, 0)).unwrap();
        assert!(again.path.to_str().unwrap().ends_with("2026-11-22 09.00.00 (2).md"));
        fs::write(dir.0.join(FOLDER).join("abc123").join("stray.txt"), "x").unwrap();

        let found = list(&dir.0, "abc123");
        let summary: Vec<(String, String)> = found.iter().map(|s| (s.when(), s.name.clone())).collect();
        assert_eq!(
            summary,
            [
                ("22 Nov 2026, 14:30".to_owned(), "before cutting the flashback".to_owned()),
                ("22 Nov 2026, 09:00".to_owned(), String::new()),
                ("22 Nov 2026, 09:00".to_owned(), String::new()),
            ]
        );
        assert_eq!(found[0].text().unwrap(), "second");
        assert!(list(&dir.0, "other").is_empty());

        delete(&found[0]).unwrap();
        assert_eq!(list(&dir.0, "abc123").len(), 2);
        delete_all(&dir.0, "abc123").unwrap();
        assert!(list(&dir.0, "abc123").is_empty());
        delete_all(&dir.0, "abc123").unwrap(); // already gone: fine
    }

    #[test]
    fn compares_word_by_word() {
        assert_eq!(
            compare("The ferry had gone.", "The last ferry was gone."),
            [
                Piece::Same("The ".into()),
                Piece::Added("last ".into()),
                Piece::Same("ferry ".into()),
                Piece::Removed("had".into()),
                Piece::Added("was".into()),
                Piece::Same(" gone.".into()),
            ]
        );
        assert_eq!(compare("same", "same"), [Piece::Same("same".into())]);
    }
}
