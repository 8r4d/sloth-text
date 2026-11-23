//! The canvas: every document on one surface, for thinking rather than
//! ordering. Cards sit wherever they're put, with no bearing on the binder's
//! order. They're strung to the profiles they refer to (`[[references]]` and
//! links made in the inspector), and to each other by strings tied by hand.
//! Free notes sit among them, ideas that aren't documents yet.
//!
//! All of it is kept in `.sloth/canvas.ron`, beside the writing statistics:
//! it's a thinking space, so moving cards about doesn't touch the project
//! file. Notes and strings are kept in backups with the rest of `.sloth`.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::project::{Node, Project, random_id};
use crate::references;

const CANVAS_FILE: &str = ".sloth/canvas.ron";

/// Where new cards go: a grid of cells this size (whole steps of the dots
/// behind them), this many to a row.
pub const CELL: [f32; 2] = [240.0, 120.0];
const COLUMNS: usize = 4;
/// Space between one folder's block of cards and the next.
const GROUP_GAP: f32 = 80.0;
/// Folders' blocks go side by side until a row is this wide.
const ROW_WIDTH: f32 = 1400.0;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Canvas {
    /// Each card's top-left corner, by binder id or note id.
    pub places: BTreeMap<String, [f32; 2]>,
    pub notes: Vec<Note>,
    /// Strings tied by hand.
    pub strings: Vec<Tie>,
    /// Top-level folders (by id) whose documents are left off.
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    pub hidden: BTreeSet<String>,
}

/// A free note: an idea on the canvas that isn't a document (yet).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Note {
    pub id: String,
    pub text: String,
}

/// A string between two cards, tied by hand. It has no direction.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tie {
    pub a: String,
    pub b: String,
    /// What the connection is ("sets up", "echoes").
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
}

impl Tie {
    pub fn touches(&self, id: &str) -> bool {
        self.a == id || self.b == id
    }

    fn joins(&self, x: &str, y: &str) -> bool {
        (self.a == x && self.b == y) || (self.a == y && self.b == x)
    }
}

/// A string the project draws itself: a document's references to a
/// profile, and whether it's linked to it in the inspector.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edge {
    /// The document, and the profile it points at.
    pub from: String,
    pub to: String,
    /// `[[references]]` to it in the text.
    pub references: usize,
    /// Linked by hand, in the inspector's In This Scene.
    pub linked: bool,
}

impl Canvas {
    pub fn load(root: &Path) -> Self {
        std::fs::read_to_string(root.join(CANVAS_FILE)).ok().and_then(|text| ron::from_str(&text).ok()).unwrap_or_default()
    }

    pub fn save(&self, root: &Path) -> io::Result<()> {
        let path = root.join(CANVAS_FILE);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default()).map_err(io::Error::other)?;
        sloth_core::document::write_atomically(&path, text.as_bytes())
    }

    pub fn note(&self, id: &str) -> Option<&Note> {
        self.notes.iter().find(|n| n.id == id)
    }

    /// A new, empty note with its corner at `at`. Returns its id.
    pub fn add_note(&mut self, at: [f32; 2], text: &str) -> String {
        let used: HashSet<&str> = self.notes.iter().map(|n| n.id.as_str()).collect();
        // Binder ids are six characters, so these never clash with them.
        let id = loop {
            let id = format!("note-{}", random_id(6));
            if !used.contains(id.as_str()) {
                break id;
            }
        };
        self.notes.push(Note { id: id.clone(), text: text.to_owned() });
        self.places.insert(id.clone(), at);
        id
    }

    /// Take a card off: a note goes for good, and any strings tied to it.
    pub fn remove(&mut self, id: &str) {
        self.notes.retain(|n| n.id != id);
        self.places.remove(id);
        self.strings.retain(|t| !t.touches(id));
    }

    /// Tie a string between two cards, or untie it if there is one.
    /// Returns whether it's now tied.
    pub fn toggle_tie(&mut self, a: &str, b: &str) -> bool {
        if a == b {
            return false;
        }
        if let Some(i) = self.strings.iter().position(|t| t.joins(a, b)) {
            self.strings.remove(i);
            false
        } else {
            self.strings.push(Tie { a: a.to_owned(), b: b.to_owned(), label: String::new() });
            true
        }
    }

    /// A card now stands for something else (a note made into a
    /// document): it keeps its place and its strings.
    pub fn rename(&mut self, old: &str, new: &str) {
        if let Some(at) = self.places.remove(old) {
            self.places.insert(new.to_owned(), at);
        }
        for tie in &mut self.strings {
            for end in [&mut tie.a, &mut tie.b] {
                if end == old {
                    *end = new.to_owned();
                }
            }
        }
    }

    /// Forget places and strings of items that are gone from the project
    /// (emptied from the Trash). What's in the Trash is kept, in case it
    /// comes back. Returns whether anything changed.
    pub fn prune(&mut self, exists: impl Fn(&str) -> bool) -> bool {
        let notes: HashSet<String> = self.notes.iter().map(|n| n.id.clone()).collect();
        let keep = |id: &str| notes.contains(id) || exists(id);
        let (places, strings) = (self.places.len(), self.strings.len());
        self.places.retain(|id, _| keep(id));
        self.strings.retain(|t| keep(&t.a) && keep(&t.b));
        places != self.places.len() || strings != self.strings.len()
    }

    /// Give a place to cards that have none, a group (a top-level folder's
    /// documents, in binder order) at a time. On an empty canvas the groups
    /// sit side by side; later arrivals go in rows below everything else, so
    /// nothing already placed moves. Returns whether any were placed.
    pub fn place_missing(&mut self, groups: &[Vec<String>]) -> bool {
        let fresh = self.places.is_empty();
        let missing: Vec<Vec<&String>> =
            groups.iter().map(|g| g.iter().filter(|id| !self.places.contains_key(*id)).collect()).collect();
        if missing.iter().all(Vec::is_empty) {
            return false;
        }
        if fresh {
            let (mut left, mut top, mut row_height) = (0.0, 0.0, 0.0_f32);
            for group in missing.iter().filter(|g| !g.is_empty()) {
                let width = group.len().min(COLUMNS) as f32 * CELL[0];
                let height = group.len().div_ceil(COLUMNS) as f32 * CELL[1];
                // A wide canvas fits on screen only when zoomed too far out
                // to read, so the blocks wrap into rows.
                if left > 0.0 && left + width > ROW_WIDTH {
                    (left, top, row_height) = (0.0, top + row_height + GROUP_GAP, 0.0);
                }
                for (i, id) in group.iter().enumerate() {
                    let (col, row) = (i % COLUMNS, i / COLUMNS);
                    self.places.insert((*id).clone(), [left + col as f32 * CELL[0], top + row as f32 * CELL[1]]);
                }
                left += width + GROUP_GAP;
                row_height = row_height.max(height);
            }
        } else {
            let left = self.places.values().map(|p| p[0]).fold(f32::INFINITY, f32::min);
            let top = self.places.values().map(|p| p[1]).fold(f32::NEG_INFINITY, f32::max) + CELL[1];
            const ROW: usize = COLUMNS * 2;
            for (i, id) in missing.into_iter().flatten().enumerate() {
                let (col, row) = (i % ROW, i / ROW);
                self.places.insert(id.clone(), [left + col as f32 * CELL[0], top + row as f32 * CELL[1]]);
            }
        }
        true
    }
}

/// The strings the project draws itself, from each document (outside the
/// Trash) to the profiles it refers to or is linked to. `text` gives a
/// document's text (as open, or as saved).
pub fn edges(project: &Project, text: impl Fn(&Node) -> Option<String>) -> Vec<Edge> {
    let mut found: BTreeMap<(String, String), (usize, bool)> = BTreeMap::new();
    for node in project.documents() {
        if let Some(text) = text(node) {
            for (_, inner) in sloth_core::markdown::references(&text) {
                if let Some(target) = references::resolve(project, inner)
                    && target.id != node.id
                {
                    found.entry((node.id.clone(), target.id)).or_default().0 += 1;
                }
            }
        }
        for id in node.meta.links.values().flatten() {
            if *id != node.id {
                found.entry((node.id.clone(), id.clone())).or_default().1 = true;
            }
        }
    }
    found.into_iter().map(|((from, to), (references, linked))| Edge { from, to, references, linked }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::Place;
    use std::fs;
    use std::path::PathBuf;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("sloth-writer-canvas-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn strings_tie_and_untie_either_way_round() {
        let mut c = Canvas::default();
        assert!(c.toggle_tie("a", "b"));
        assert!(!c.toggle_tie("a", "a"), "not to itself");
        assert!(!c.toggle_tie("b", "a"), "the same string, untied");
        assert!(c.strings.is_empty());
    }

    #[test]
    fn a_note_made_into_a_document_keeps_its_place_and_strings() {
        let mut c = Canvas::default();
        let note = c.add_note([10.0, 20.0], "An idea");
        c.toggle_tie(&note, "scene1");
        c.rename(&note, "doc123");
        c.notes.retain(|n| n.id != note);
        assert_eq!(c.places.get("doc123"), Some(&[10.0, 20.0]));
        assert_eq!((c.strings[0].a.as_str(), c.strings[0].b.as_str()), ("doc123", "scene1"));

        c.remove("doc123");
        assert!(c.strings.is_empty() && c.places.is_empty());
    }

    #[test]
    fn pruning_forgets_what_is_gone_but_keeps_notes() {
        let mut c = Canvas::default();
        let note = c.add_note([0.0, 0.0], "");
        c.places.insert("kept".into(), [1.0, 1.0]);
        c.places.insert("gone".into(), [2.0, 2.0]);
        c.toggle_tie(&note, "kept");
        c.toggle_tie(&note, "gone");
        assert!(c.prune(|id| id == "kept"));
        assert_eq!(c.places.keys().cloned().collect::<Vec<_>>(), ["kept".to_owned(), note.clone()]);
        assert_eq!(c.strings.len(), 1);
        assert!(!c.prune(|id| id == "kept"), "nothing more to do");
    }

    #[test]
    fn new_cards_get_places_without_moving_old_ones() {
        let mut c = Canvas::default();
        let ids = |r: std::ops::Range<usize>| r.map(|i| format!("d{i}")).collect::<Vec<_>>();
        // An empty canvas: folders side by side, wrapping when it's wide.
        assert!(c.place_missing(&[ids(0..6), ids(6..7), ids(7..9)]));
        assert_eq!(c.places["d0"], [0.0, 0.0]);
        assert_eq!(c.places["d4"], [0.0, CELL[1]], "four to a row, then the next");
        assert_eq!(c.places["d6"], [4.0 * CELL[0] + GROUP_GAP, 0.0], "the next folder beside the first");
        assert_eq!(c.places["d7"], [0.0, 2.0 * CELL[1] + GROUP_GAP], "too wide: below");
        assert!(!c.place_missing(&[ids(0..9)]), "all placed");

        // A newcomer goes below the lot.
        c.places.insert("d0".into(), [-50.0, 0.0]);
        assert!(c.place_missing(&[ids(0..10)]));
        assert_eq!(c.places["d9"], [-50.0, 3.0 * CELL[1] + GROUP_GAP]);
        assert_eq!(c.places.len(), 10);
    }

    #[test]
    fn edges_come_from_references_and_links() {
        let dir = TempDir::new("edges");
        let mut p = Project::create(&dir.0, "Book").unwrap();
        let alice = p.add_document(&Place::Into("characters".into()), "Alice").unwrap();
        let scene = p.add_document(&Place::Into("manuscript".into()), "Scene").unwrap();
        let other = p.add_document(&Place::Into("manuscript".into()), "Other").unwrap();
        let mut meta = p.node(&other).unwrap().meta.clone();
        meta.links.insert("characters".into(), vec![alice.clone()]);
        p.set_meta(&other, meta);
        let text = |n: &Node| Some(if n.id == scene { "[[Alice]] met [[alice|p. 2]] and [[Nobody]]".to_owned() } else { String::new() });

        let mut edges = edges(&p, text);
        edges.sort_by(|a, b| a.from.cmp(&b.from));
        let mut want = vec![
            Edge { from: scene.clone(), to: alice.clone(), references: 2, linked: false },
            Edge { from: other.clone(), to: alice.clone(), references: 0, linked: true },
        ];
        want.sort_by(|a, b| a.from.cmp(&b.from));
        assert_eq!(edges, want);
    }

    #[test]
    fn saves_and_loads() {
        let dir = TempDir::new("save");
        fs::create_dir_all(&dir.0).unwrap();
        let mut c = Canvas::default();
        let note = c.add_note([3.0, 4.0], "Maybe the letter was never sent");
        c.toggle_tie(&note, "abc123");
        c.strings[0].label = "explains".into();
        c.hidden.insert("research".into());
        c.save(&dir.0).unwrap();
        assert_eq!(Canvas::load(&dir.0), c);
        assert_eq!(Canvas::load(&dir.0.join("nowhere")), Canvas::default());
    }
}
