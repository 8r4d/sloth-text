//! A Sloth Writer project on disk: a folder of Markdown files plus
//! `project.ron`, the manifest that holds the binder (titles, order and
//! nesting). Every change to the binder is saved straight away.
//!
//! ```text
//! The Long Winter/
//!   project.ron
//!   manuscript/   research/   notes/   trash/     one .md file per document
//! ```
//!
//! The binder's top level is always the four role folders, in that order;
//! everything else lives inside one of them. Which one decides the folder
//! a document's file sits in, so moving an item to another role folder
//! (or to the trash) moves its files too. Order and nesting live only in
//! the manifest: reordering never renames a file.

use std::collections::HashSet;
use std::fs;
use std::hash::{BuildHasher, Hasher};
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sloth_core::document::write_atomically;

pub const MANIFEST_FILE: &str = "project.ron";
/// The manifest format this version writes. A project with a higher number
/// was made by a newer Sloth Writer and isn't opened, rather than risk
/// dropping what this version doesn't understand.
pub const FORMAT: u32 = 1;

/// The four folders at the top of every binder.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    Manuscript,
    Research,
    Notes,
    Trash,
}

impl Role {
    pub const ALL: [Role; 4] = [Role::Manuscript, Role::Research, Role::Notes, Role::Trash];

    pub fn title(self) -> &'static str {
        match self {
            Role::Manuscript => "Manuscript",
            Role::Research => "Research",
            Role::Notes => "Notes",
            Role::Trash => "Trash",
        }
    }

    /// The folder its documents' files live in. Also used as the role
    /// folder's binder id, which never clashes with the six-character ids
    /// of everything else.
    pub fn folder(self) -> &'static str {
        match self {
            Role::Manuscript => "manuscript",
            Role::Research => "research",
            Role::Notes => "notes",
            Role::Trash => "trash",
        }
    }
}

/// One binder item: a document (it has a file), a folder (it doesn't), or a
/// document with children, which is both.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    pub title: String,
    /// Set only on the four top-level folders.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    /// The document's file, relative to the project folder, with `/`
    /// separators: `manuscript/the-arrival.md`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Node>,
    /// Synopsis, status, label and the rest, edited in the inspector.
    #[serde(default, skip_serializing_if = "Meta::is_empty")]
    pub meta: Meta,
}

/// What the inspector edits about an item. Everything is optional, and an
/// item with none of it set writes nothing extra to the manifest.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Meta {
    /// A few sentences on what happens: shown on cards and in the outliner.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub synopsis: String,
    /// Index into the project's `statuses`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<usize>,
    /// Index into the project's `labels`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<usize>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
    /// Words to aim for. On the Manuscript folder, the whole book's target.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_words: Option<u32>,
    /// Leave this out when compiling the manuscript.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub exclude_from_compile: bool,
    /// Private notes about the item, never compiled.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub notes: String,
}

impl Meta {
    fn is_empty(&self) -> bool {
        *self == Meta::default()
    }
}

/// A label: a name and a colour (an index into the app's label palette).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Label {
    pub name: String,
    pub color: u8,
}

/// Statuses a new or older project starts with.
pub fn default_statuses() -> Vec<String> {
    ["To do", "First draft", "Revised", "Done"].map(String::from).to_vec()
}

/// Labels a new project starts with; rename or remove them freely.
fn starter_labels() -> Vec<Label> {
    [("Main plot", 0), ("Subplot", 3), ("Idea", 5)]
        .map(|(name, color)| Label { name: name.to_owned(), color })
        .to_vec()
}

impl Node {
    pub fn is_document(&self) -> bool {
        self.path.is_some()
    }

    fn role_folder(role: Role) -> Self {
        Self {
            id: role.folder().to_owned(),
            title: role.title().to_owned(),
            role: Some(role),
            path: None,
            children: Vec::new(),
            meta: Meta::default(),
        }
    }

    /// This node and everything under it, parents first.
    fn subtree(&self) -> Vec<&Node> {
        let mut out = vec![self];
        for child in &self.children {
            out.extend(child.subtree());
        }
        out
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub format: u32,
    pub title: String,
    pub binder: Vec<Node>,
    #[serde(default = "default_statuses")]
    pub statuses: Vec<String>,
    #[serde(default)]
    pub labels: Vec<Label>,
    /// Compile choices (author name, format), remembered for next time.
    #[serde(default)]
    pub compile: crate::compile::CompileSettings,
}

/// Where to put an item, relative to another one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Place {
    Before(String),
    After(String),
    /// As the last child.
    Into(String),
}

impl Place {
    pub fn target(&self) -> &str {
        match self {
            Place::Before(id) | Place::After(id) | Place::Into(id) => id,
        }
    }
}

/// A document's file was renamed or moved, so an open copy must follow it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathChange {
    pub from: PathBuf,
    pub to: PathBuf,
}

pub struct Project {
    pub root: PathBuf,
    pub manifest: Manifest,
}

impl Project {
    /// Make a new project in `root`, which must be missing or empty. It
    /// starts with one empty document in the Manuscript, ready to type in.
    pub fn create(root: &Path, title: &str) -> io::Result<Self> {
        if root.exists() && fs::read_dir(root)?.next().is_some() {
            return Err(invalid("That folder already has files in it. Choose a new or empty folder."));
        }
        fs::create_dir_all(root)?;
        for role in Role::ALL {
            fs::create_dir_all(root.join(role.folder()))?;
        }
        let mut project = Self {
            root: root.to_path_buf(),
            manifest: Manifest {
                format: FORMAT,
                title: title.trim().to_owned(),
                binder: Role::ALL.into_iter().map(Node::role_folder).collect(),
                statuses: default_statuses(),
                labels: starter_labels(),
                compile: Default::default(),
            },
        };
        project.add_document(&Place::Into(Role::Manuscript.folder().into()), "Untitled")?;
        Ok(project)
    }

    pub fn open(root: &Path) -> io::Result<Self> {
        let text = fs::read_to_string(root.join(MANIFEST_FILE))?;
        let manifest: Manifest = ron::from_str(&text)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("{MANIFEST_FILE} couldn't be read: {e}")))?;
        if manifest.format > FORMAT {
            return Err(invalid("This project was made by a newer version of Sloth Writer."));
        }
        let mut project = Self { root: root.to_path_buf(), manifest };
        project.repair();
        Ok(project)
    }

    /// Put back any role folder missing from the binder or the disk, so the
    /// rest of the code can rely on all four being there.
    fn repair(&mut self) {
        for role in Role::ALL {
            if !self.manifest.binder.iter().any(|n| n.role == Some(role)) {
                self.manifest.binder.push(Node::role_folder(role));
            }
            let _ = fs::create_dir_all(self.root.join(role.folder()));
        }
        // Keep them in their usual order, whatever the file said.
        self.manifest.binder.sort_by_key(|n| n.role.map(|r| r as u8).unwrap_or(u8::MAX));
    }

    pub fn save(&self) -> io::Result<()> {
        let config = ron::ser::PrettyConfig::default();
        let text = ron::ser::to_string_pretty(&self.manifest, config).map_err(io::Error::other)?;
        write_atomically(&self.root.join(MANIFEST_FILE), text.as_bytes())
    }

    pub fn title(&self) -> &str {
        &self.manifest.title
    }

    pub fn node(&self, id: &str) -> Option<&Node> {
        find(&self.manifest.binder, id)
    }

    /// The role folder an item sits in.
    pub fn role_of(&self, id: &str) -> Option<Role> {
        self.manifest
            .binder
            .iter()
            .find(|top| top.subtree().iter().any(|n| n.id == id))
            .and_then(|top| top.role)
    }

    /// Every document outside the Trash, in binder order: what a project
    /// search looks through.
    pub fn documents(&self) -> Vec<&Node> {
        self.manifest
            .binder
            .iter()
            .filter(|top| top.role != Some(Role::Trash))
            .flat_map(Node::subtree)
            .filter(|n| n.is_document())
            .collect()
    }

    /// The documents in an item, itself included: what its word count adds up.
    pub fn documents_under(&self, id: &str) -> Vec<&Node> {
        self.node(id).map(|n| n.subtree().into_iter().filter(|n| n.is_document()).collect()).unwrap_or_default()
    }

    pub fn file_path(&self, id: &str) -> Option<PathBuf> {
        let rel = self.node(id)?.path.as_ref()?;
        Some(self.root.join(rel))
    }

    /// Add an empty document, creating its file. Returns its id.
    pub fn add_document(&mut self, place: &Place, title: &str) -> io::Result<String> {
        self.add(place, title, true)
    }

    /// Add a folder (no file). Returns its id.
    pub fn add_folder(&mut self, place: &Place, title: &str) -> io::Result<String> {
        self.add(place, title, false)
    }

    fn add(&mut self, place: &Place, title: &str, document: bool) -> io::Result<String> {
        self.check_place(None, place)?;
        let title = clean_title(title)?;
        let path = if document {
            let role = self.role_of(place.target()).expect("checked above");
            let rel = self.free_path(role, &slug(&title), None);
            fs::OpenOptions::new().write(true).create_new(true).open(self.root.join(&rel))?;
            Some(rel)
        } else {
            None
        };
        let node = Node { id: self.new_id(), title, role: None, path, children: Vec::new(), meta: Meta::default() };
        let id = node.id.clone();
        insert(&mut self.manifest.binder, place, node).expect("checked above");
        self.save()?;
        Ok(id)
    }

    /// Change an item's title. A document's file is renamed to match, so
    /// the files stay recognisable outside the app.
    pub fn rename(&mut self, id: &str, title: &str) -> io::Result<Option<PathChange>> {
        let node = self.node(id).ok_or_else(|| invalid("That item no longer exists."))?;
        if node.role.is_some() {
            return Err(invalid("Manuscript, Research, Notes and Trash can't be renamed."));
        }
        let title = clean_title(title)?;
        let mut change = None;
        if let Some(old) = node.path.clone() {
            let role = self.role_of(id).expect("node exists");
            let new = self.free_path(role, &slug(&title), Some(&old));
            if new != old {
                let (from, to) = (self.root.join(&old), self.root.join(&new));
                fs::rename(&from, &to)?;
                change = Some(PathChange { from, to });
                self.node_mut(id).expect("node exists").path = Some(new);
            }
        }
        self.node_mut(id).expect("node exists").title = title;
        self.save()?;
        Ok(change)
    }

    /// Move an item, with everything under it. If it lands in another role
    /// folder, its documents' files move to that folder on disk.
    pub fn move_node(&mut self, id: &str, place: &Place) -> io::Result<Vec<PathChange>> {
        self.check_place(Some(id), place)?;
        let from_role = self.role_of(id).expect("checked above");
        let to_role = self.role_of(place.target()).expect("checked above");
        let node = take(&mut self.manifest.binder, id).expect("checked above");
        insert(&mut self.manifest.binder, place, node).expect("checked above");

        let mut changes = Vec::new();
        if from_role != to_role {
            let documents: Vec<(String, String)> = self
                .node(id)
                .expect("just inserted")
                .subtree()
                .into_iter()
                .filter_map(|n| Some((n.id.clone(), n.path.clone()?)))
                .collect();
            for (doc_id, old) in documents {
                let stem = Path::new(&old).file_stem().map(|s| s.to_string_lossy().into_owned());
                let new = self.free_path(to_role, stem.as_deref().unwrap_or("untitled"), None);
                let (from, to) = (self.root.join(&old), self.root.join(&new));
                match fs::rename(&from, &to) {
                    Ok(()) => {
                        changes.push(PathChange { from, to });
                        self.node_mut(&doc_id).expect("in subtree").path = Some(new);
                    }
                    // Already gone from disk: nothing to move, keep the entry.
                    Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                    Err(e) => {
                        // Save what has moved so far, so the manifest matches the disk.
                        let _ = self.save();
                        return Err(e);
                    }
                }
            }
        }
        self.save()?;
        Ok(changes)
    }

    pub fn trash(&mut self, id: &str) -> io::Result<Vec<PathChange>> {
        self.move_node(id, &Place::Into(Role::Trash.folder().into()))
    }

    /// Delete everything in the Trash for good. Returns the files removed,
    /// so open copies can be closed.
    pub fn empty_trash(&mut self) -> io::Result<Vec<PathBuf>> {
        let trash = self
            .manifest
            .binder
            .iter_mut()
            .find(|n| n.role == Some(Role::Trash))
            .expect("repair keeps the trash");
        let items = std::mem::take(&mut trash.children);
        let mut removed = Vec::new();
        for item in &items {
            for node in item.subtree() {
                if let Some(rel) = &node.path {
                    let path = self.root.join(rel);
                    match fs::remove_file(&path) {
                        Ok(()) => removed.push(path),
                        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                        Err(e) => return Err(e),
                    }
                }
            }
        }
        self.save()?;
        Ok(removed)
    }

    fn node_mut(&mut self, id: &str) -> Option<&mut Node> {
        find_mut(&mut self.manifest.binder, id)
    }

    // ----- metadata ----------------------------------------------------------
    //
    // These change the manifest in memory only: they happen while typing in
    // the inspector, so the app saves them along with its regular autosave.

    /// Replace an item's metadata. Returns whether anything changed. A
    /// status or label that doesn't exist is dropped.
    pub fn set_meta(&mut self, id: &str, mut meta: Meta) -> bool {
        if meta.status.is_some_and(|i| i >= self.manifest.statuses.len()) {
            meta.status = None;
        }
        if meta.label.is_some_and(|i| i >= self.manifest.labels.len()) {
            meta.label = None;
        }
        match self.node_mut(id) {
            Some(node) if node.meta != meta => {
                node.meta = meta;
                true
            }
            _ => false,
        }
    }

    pub fn add_label(&mut self, name: &str, color: u8) -> usize {
        self.manifest.labels.push(Label { name: name.to_owned(), color });
        self.manifest.labels.len() - 1
    }

    /// Remove a label; items that had it lose it, and the others keep theirs.
    pub fn remove_label(&mut self, index: usize) {
        if index >= self.manifest.labels.len() {
            return;
        }
        self.manifest.labels.remove(index);
        for top in &mut self.manifest.binder {
            relabel(top, index);
        }
    }

    /// Refuse places that would break the binder's shape.
    fn check_place(&self, moving: Option<&str>, place: &Place) -> io::Result<()> {
        let target = self.node(place.target()).ok_or_else(|| invalid("That item no longer exists."))?;
        if target.role.is_some() && !matches!(place, Place::Into(_)) {
            return Err(invalid("Items go inside Manuscript, Research, Notes or Trash."));
        }
        if let Some(id) = moving {
            let node = self.node(id).ok_or_else(|| invalid("That item no longer exists."))?;
            if node.role.is_some() {
                return Err(invalid("Manuscript, Research, Notes and Trash can't be moved."));
            }
            if node.subtree().iter().any(|n| n.id == target.id) {
                return Err(invalid("An item can't go inside itself."));
            }
        }
        Ok(())
    }

    /// A relative path in `role`'s folder that no other item uses and no
    /// file occupies: `stem.md`, else `stem-2.md`, `stem-3.md`… `current` is
    /// the item's own path, which it may keep.
    fn free_path(&self, role: Role, stem: &str, current: Option<&str>) -> String {
        let used: HashSet<String> = self
            .all_nodes()
            .into_iter()
            .filter_map(|n| n.path.as_deref())
            .filter(|p| Some(*p) != current)
            // macOS file names usually ignore case.
            .map(str::to_lowercase)
            .collect();
        (1..)
            .map(|n| match n {
                1 => format!("{}/{stem}.md", role.folder()),
                n => format!("{}/{stem}-{n}.md", role.folder()),
            })
            .find(|rel| {
                Some(rel.as_str()) == current
                    || (!used.contains(&rel.to_lowercase()) && !self.root.join(rel).exists())
            })
            .expect("some number is free")
    }

    fn all_nodes(&self) -> Vec<&Node> {
        self.manifest.binder.iter().flat_map(Node::subtree).collect()
    }

    /// Six random lower-case letters and digits, unused in this project.
    fn new_id(&self) -> String {
        let used: HashSet<&str> = self.all_nodes().into_iter().map(|n| n.id.as_str()).collect();
        loop {
            // Each RandomState has fresh random keys, so this is a random number.
            let mut n = std::collections::hash_map::RandomState::new().build_hasher().finish();
            let id: String = (0..6)
                .map(|_| {
                    let digit = (n % 36) as u32;
                    n /= 36;
                    char::from_digit(digit, 36).expect("below 36")
                })
                .collect();
            if !used.contains(id.as_str()) {
                return id;
            }
        }
    }
}

/// A file-name stem from a title: lower case, letters and digits, words
/// joined by `-`. "Mara's Arrival!" becomes `maras-arrival`.
pub fn slug(title: &str) -> String {
    let mut out = String::new();
    let mut gap = false;
    for c in title.chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() {
            if gap && !out.is_empty() {
                out.push('-');
            }
            gap = false;
            out.push(c);
        } else if c != '\'' && c != '\u{2019}' {
            gap = true;
        }
    }
    let out: String = out.chars().take(60).collect();
    let out = out.trim_end_matches('-');
    if out.is_empty() { "untitled".to_owned() } else { out.to_owned() }
}

/// After label `removed` is deleted: drop it, and shift the later ones down.
fn relabel(node: &mut Node, removed: usize) {
    node.meta.label = match node.meta.label {
        Some(i) if i == removed => None,
        Some(i) if i > removed => Some(i - 1),
        other => other,
    };
    for child in &mut node.children {
        relabel(child, removed);
    }
}

fn clean_title(title: &str) -> io::Result<String> {
    let title = title.trim();
    if title.is_empty() {
        return Err(invalid("A title can't be empty."));
    }
    Ok(title.to_owned())
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

fn find<'a>(nodes: &'a [Node], id: &str) -> Option<&'a Node> {
    nodes.iter().find_map(|n| if n.id == id { Some(n) } else { find(&n.children, id) })
}

fn find_mut<'a>(nodes: &'a mut [Node], id: &str) -> Option<&'a mut Node> {
    for n in nodes {
        if n.id == id {
            return Some(n);
        }
        if let Some(found) = find_mut(&mut n.children, id) {
            return Some(found);
        }
    }
    None
}

/// Remove a node (with its children) from wherever it is.
fn take(nodes: &mut Vec<Node>, id: &str) -> Option<Node> {
    if let Some(i) = nodes.iter().position(|n| n.id == id) {
        return Some(nodes.remove(i));
    }
    nodes.iter_mut().find_map(|n| take(&mut n.children, id))
}

/// Put a node at `place`. Hands it back (boxed, as nodes are large) if
/// the target isn't found.
fn insert(nodes: &mut Vec<Node>, place: &Place, node: Node) -> Result<(), Box<Node>> {
    match place {
        Place::Into(target) => match find_mut(nodes, target) {
            Some(parent) => {
                parent.children.push(node);
                Ok(())
            }
            None => Err(Box::new(node)),
        },
        Place::Before(target) => insert_beside(nodes, target, false, node),
        Place::After(target) => insert_beside(nodes, target, true, node),
    }
}

fn insert_beside(nodes: &mut Vec<Node>, target: &str, after: bool, node: Node) -> Result<(), Box<Node>> {
    if let Some(i) = nodes.iter().position(|n| n.id == target) {
        nodes.insert(i + usize::from(after), node);
        return Ok(());
    }
    let mut node = node;
    for n in nodes.iter_mut() {
        match insert_beside(&mut n.children, target, after, node) {
            Ok(()) => return Ok(()),
            Err(back) => node = *back,
        }
    }
    Err(Box::new(node))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh, empty folder for one test, removed when dropped.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("sloth-writer-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn manuscript() -> Place {
        Place::Into("manuscript".into())
    }

    fn titles(nodes: &[Node]) -> Vec<&str> {
        nodes.iter().map(|n| n.title.as_str()).collect()
    }

    #[test]
    fn slugs() {
        assert_eq!(slug("The Arrival"), "the-arrival");
        assert_eq!(slug("Mara's Arrival!"), "maras-arrival");
        assert_eq!(slug("  -- Chapter 1: Snow --  "), "chapter-1-snow");
        assert_eq!(slug("Été à Paris"), "été-à-paris");
        assert_eq!(slug("?!"), "untitled");
        assert_eq!(slug(&"a".repeat(80)).len(), 60);
    }

    #[test]
    fn create_then_open_round_trips() {
        let dir = TempDir::new("roundtrip");
        let created = Project::create(&dir.0, " The Long Winter ").unwrap();
        assert_eq!(created.title(), "The Long Winter");
        assert_eq!(titles(&created.manifest.binder), ["Manuscript", "Research", "Notes", "Trash"]);
        for role in Role::ALL {
            assert!(dir.0.join(role.folder()).is_dir());
        }
        // One empty document to start in.
        let first = &created.manifest.binder[0].children[0];
        assert_eq!(first.path.as_deref(), Some("manuscript/untitled.md"));
        assert_eq!(fs::read_to_string(dir.0.join("manuscript/untitled.md")).unwrap(), "");

        let opened = Project::open(&dir.0).unwrap();
        assert_eq!(opened.manifest, created.manifest);
    }

    #[test]
    fn create_refuses_a_folder_with_files() {
        let dir = TempDir::new("nonempty");
        fs::create_dir_all(&dir.0).unwrap();
        fs::write(dir.0.join("keep.txt"), "mine").unwrap();
        assert!(Project::create(&dir.0, "X").is_err());
        assert_eq!(fs::read_to_string(dir.0.join("keep.txt")).unwrap(), "mine");
    }

    #[test]
    fn open_refuses_newer_formats_and_repairs_missing_roles() {
        let dir = TempDir::new("open");
        fs::create_dir_all(&dir.0).unwrap();
        let newer = r#"(format: 99, title: "Future", binder: [])"#;
        fs::write(dir.0.join(MANIFEST_FILE), newer).unwrap();
        assert!(Project::open(&dir.0).is_err());

        let old = r#"(format: 1, title: "Old", binder: [(id: "notes", title: "Notes", role: Some(Notes))])"#;
        fs::write(dir.0.join(MANIFEST_FILE), old).unwrap();
        let project = Project::open(&dir.0).unwrap();
        assert_eq!(titles(&project.manifest.binder), ["Manuscript", "Research", "Notes", "Trash"]);
    }

    #[test]
    fn adding_documents_creates_unique_files() {
        let dir = TempDir::new("add");
        let mut p = Project::create(&dir.0, "Book").unwrap();
        let a = p.add_document(&manuscript(), "The Arrival").unwrap();
        let b = p.add_document(&Place::After(a.clone()), "The arrival").unwrap();
        assert_eq!(p.node(&a).unwrap().path.as_deref(), Some("manuscript/the-arrival.md"));
        assert_eq!(p.node(&b).unwrap().path.as_deref(), Some("manuscript/the-arrival-2.md"));
        assert!(dir.0.join("manuscript/the-arrival-2.md").is_file());
        assert_eq!(titles(&p.manifest.binder[0].children), ["Untitled", "The Arrival", "The arrival"]);

        // In a research folder, the file goes in research/.
        let folder = p.add_folder(&Place::Into("research".into()), "Ships").unwrap();
        assert_eq!(p.node(&folder).unwrap().path, None);
        let log = p.add_document(&Place::Into(folder), "Logs").unwrap();
        assert_eq!(p.node(&log).unwrap().path.as_deref(), Some("research/logs.md"));

        // Saved straight away.
        assert_eq!(Project::open(&dir.0).unwrap().manifest, p.manifest);
    }

    #[test]
    fn ids_are_unique_and_short() {
        let dir = TempDir::new("ids");
        let mut p = Project::create(&dir.0, "Book").unwrap();
        let ids: HashSet<String> = (0..50).map(|i| p.add_folder(&manuscript(), &format!("F{i}")).unwrap()).collect();
        assert_eq!(ids.len(), 50);
        assert!(ids.iter().all(|id| id.len() == 6 && id.chars().all(|c| c.is_ascii_alphanumeric())));
    }

    #[test]
    fn renaming_renames_the_file() {
        let dir = TempDir::new("rename");
        let mut p = Project::create(&dir.0, "Book").unwrap();
        let id = p.add_document(&manuscript(), "Draft").unwrap();
        fs::write(dir.0.join("manuscript/draft.md"), "words").unwrap();

        let change = p.rename(&id, "The Harbour").unwrap().unwrap();
        assert_eq!(change.to, dir.0.join("manuscript/the-harbour.md"));
        assert_eq!(fs::read_to_string(&change.to).unwrap(), "words");
        assert!(!change.from.exists());

        // Same file name: nothing to move.
        assert_eq!(p.rename(&id, "The harbour").unwrap(), None);
        assert_eq!(p.node(&id).unwrap().title, "The harbour");

        assert!(p.rename(&id, "   ").is_err());
        assert!(p.rename("manuscript", "Book").is_err());
    }

    #[test]
    fn moving_reorders_and_nests() {
        let dir = TempDir::new("move");
        let mut p = Project::create(&dir.0, "Book").unwrap();
        let first = p.manifest.binder[0].children[0].id.clone();
        let a = p.add_document(&manuscript(), "A").unwrap();
        let b = p.add_document(&manuscript(), "B").unwrap();

        p.move_node(&b, &Place::Before(first.clone())).unwrap();
        assert_eq!(titles(&p.manifest.binder[0].children), ["B", "Untitled", "A"]);

        // Into a document: it becomes a document with children. Same role
        // folder, so no files move.
        let changes = p.move_node(&a, &Place::Into(b.clone())).unwrap();
        assert!(changes.is_empty());
        assert_eq!(titles(&p.node(&b).unwrap().children), ["A"]);

        assert!(p.move_node(&b, &Place::Into(a.clone())).is_err(), "into its own child");
        assert!(p.move_node(&b, &Place::Into(b.clone())).is_err(), "into itself");
        assert!(p.move_node("trash", &manuscript()).is_err(), "a role folder");
        assert!(p.move_node(&b, &Place::After("research".into())).is_err(), "beside a role folder");
    }

    #[test]
    fn moving_to_another_role_moves_files_too() {
        let dir = TempDir::new("cross");
        let mut p = Project::create(&dir.0, "Book").unwrap();
        let chapter = p.add_document(&manuscript(), "Chapter").unwrap();
        let scene = p.add_document(&Place::Into(chapter.clone()), "Scene").unwrap();
        // A file of that name already in notes/: the moved one gets -2.
        fs::write(dir.0.join("notes/scene.md"), "other").unwrap();

        let changes = p.move_node(&chapter, &Place::Into("notes".into())).unwrap();
        assert_eq!(changes.len(), 2);
        assert_eq!(p.node(&chapter).unwrap().path.as_deref(), Some("notes/chapter.md"));
        assert_eq!(p.node(&scene).unwrap().path.as_deref(), Some("notes/scene-2.md"));
        assert!(dir.0.join("notes/chapter.md").is_file());
        assert!(!dir.0.join("manuscript/chapter.md").exists());
        assert_eq!(fs::read_to_string(dir.0.join("notes/scene.md")).unwrap(), "other");
    }

    #[test]
    fn metadata_round_trips_and_stays_out_of_the_way() {
        let dir = TempDir::new("meta");
        let mut p = Project::create(&dir.0, "Book").unwrap();
        let id = p.add_document(&manuscript(), "Scene").unwrap();
        let meta = Meta {
            synopsis: "Mara arrives too late.".into(),
            status: Some(1),
            label: Some(0),
            keywords: vec!["harbour".into()],
            target_words: Some(2500),
            exclude_from_compile: true,
            notes: "Check the tide tables.".into(),
        };
        assert!(p.set_meta(&id, meta.clone()));
        assert!(!p.set_meta(&id, meta.clone()), "no change the second time");
        p.save().unwrap();
        assert_eq!(Project::open(&dir.0).unwrap().node(&id).unwrap().meta, meta);

        // Items without metadata write none.
        let text = fs::read_to_string(dir.0.join(MANIFEST_FILE)).unwrap();
        assert_eq!(text.matches("meta:").count(), 1, "{text}");

        // An unknown status or label is dropped rather than kept dangling.
        p.set_meta(&id, Meta { status: Some(99), label: Some(99), ..Meta::default() });
        assert_eq!(p.node(&id).unwrap().meta, Meta::default());
    }

    #[test]
    fn older_projects_get_the_default_statuses() {
        let dir = TempDir::new("statuses");
        fs::create_dir_all(&dir.0).unwrap();
        fs::write(dir.0.join(MANIFEST_FILE), r#"(format: 1, title: "Old", binder: [])"#).unwrap();
        let p = Project::open(&dir.0).unwrap();
        assert_eq!(p.manifest.statuses, default_statuses());
        assert!(p.manifest.labels.is_empty());
    }

    #[test]
    fn removing_a_label_updates_items() {
        let dir = TempDir::new("labels");
        let mut p = Project::create(&dir.0, "Book").unwrap();
        let (a, b, c) = (
            p.add_document(&manuscript(), "A").unwrap(),
            p.add_document(&manuscript(), "B").unwrap(),
            p.add_document(&manuscript(), "C").unwrap(),
        );
        for (id, label) in [(&a, 0), (&b, 1), (&c, 2)] {
            p.set_meta(id, Meta { label: Some(label), ..Meta::default() });
        }
        p.remove_label(1);
        assert_eq!(p.manifest.labels.len(), 2);
        assert_eq!(p.node(&a).unwrap().meta.label, Some(0));
        assert_eq!(p.node(&b).unwrap().meta.label, None);
        assert_eq!(p.node(&c).unwrap().meta.label, Some(1), "shifted down");
    }

    #[test]
    fn trash_then_empty() {
        let dir = TempDir::new("trash");
        let mut p = Project::create(&dir.0, "Book").unwrap();
        let id = p.add_document(&manuscript(), "Cut scene").unwrap();

        let changes = p.trash(&id).unwrap();
        assert_eq!(changes[0].to, dir.0.join("trash/cut-scene.md"));
        assert_eq!(p.role_of(&id), Some(Role::Trash));

        let removed = p.empty_trash().unwrap();
        assert_eq!(removed, [dir.0.join("trash/cut-scene.md")]);
        assert!(p.node(&id).is_none());
        assert!(!dir.0.join("trash/cut-scene.md").exists());
        assert_eq!(Project::open(&dir.0).unwrap().manifest, p.manifest);
    }
}
