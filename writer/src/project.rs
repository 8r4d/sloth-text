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

use std::collections::{BTreeMap, BTreeSet, HashSet};
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

/// The folders at the top of every binder, in this order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    Manuscript,
    /// Character profiles (see character.rs). Never compiled.
    Characters,
    /// Another profile folder (Locations, Objects…): one per kind in the
    /// manifest's `profiles`, its id the kind's id. Added and removed by
    /// choice, so not in `ALL`.
    Profile,
    Research,
    Notes,
    Trash,
}

impl Role {
    pub const ALL: [Role; 5] = [Role::Manuscript, Role::Characters, Role::Research, Role::Notes, Role::Trash];

    pub fn title(self) -> &'static str {
        match self {
            Role::Manuscript => "Manuscript",
            Role::Characters => "Characters",
            Role::Profile => "Profiles",
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
            Role::Characters => "characters",
            Role::Profile => "profiles",
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
    /// Before profile folders, a scene's characters were kept here; read
    /// from older projects and moved into `links` (see `repair`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub characters: Vec<String>,
    /// The profiles that appear in this scene, by kind ("characters",
    /// "locations"…), each a list of binder ids.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub links: BTreeMap<String, Vec<String>>,
    /// Synced to the helper for editing in a web browser (a folder: everything in it).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub on_ipad: bool,
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
    ["To do", "Ongoing", "First draft", "Revised", "Done"].map(String::from).to_vec()
}

/// The statuses before Ongoing was added.
const STATUSES_BEFORE_ONGOING: [&str; 4] = ["To do", "First draft", "Revised", "Done"];

/// Characteristics a character profile can have sections for; a project
/// starts with these and can change them.
pub fn default_characteristics() -> Vec<String> {
    ["Physical appearance", "Personality", "Motivations", "Backstory", "Voice", "Relationships", "Arc"]
        .map(String::from)
        .to_vec()
}

/// A kind of profile: Characters, Locations… Its id is also its top-level
/// folder's binder id and the folder its files live in.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProfileKind {
    pub id: String,
    /// The folder's name: "Locations".
    pub name: String,
    /// One of them: "Location" (for "+ Location").
    pub item: String,
    /// The sections a profile can have.
    pub sections: Vec<String>,
}

pub const CHARACTERS: &str = "characters";

fn kind(id: &str, name: &str, item: &str, sections: &[&str]) -> ProfileKind {
    ProfileKind { id: id.into(), name: name.into(), item: item.into(), sections: sections.iter().map(|s| s.to_string()).collect() }
}

/// The kinds a project can add, ready to use: for fiction, then for
/// non-fiction (the bool says which).
pub fn profile_presets() -> Vec<(ProfileKind, bool)> {
    let fiction = fiction_presets().into_iter().map(|k| (k, true));
    let non_fiction = non_fiction_presets().into_iter().map(|k| (k, false));
    fiction.chain(non_fiction).collect()
}

fn non_fiction_presets() -> Vec<ProfileKind> {
    vec![
        kind("sources", "Sources", "Source", &["Citation", "Summary", "Key points", "Quotes", "Where I use it"]),
        kind("people", "People", "Person", &["Who they are", "Background", "Contact", "Interview notes", "Quotes"]),
        kind("concepts", "Concepts", "Concept", &["Definition", "Explanation", "Examples", "Related ideas"]),
        kind("arguments", "Arguments", "Argument", &["Claim", "Evidence", "Counterpoints", "Response"]),
        kind("events", "Events", "Event", &["When", "What happened", "Who was involved", "Significance"]),
        kind("glossary", "Glossary", "Term", &["Definition", "Usage", "Notes"]),
    ]
}

fn fiction_presets() -> Vec<ProfileKind> {
    let characteristics = default_characteristics();
    let characteristics: Vec<&str> = characteristics.iter().map(String::as_str).collect();
    vec![
        kind(CHARACTERS, "Characters", "Character", &characteristics),
        kind("locations", "Locations", "Location", &["Description", "Sights and sounds", "History", "Who lives there", "Significance"]),
        kind("objects", "Objects", "Object", &["Description", "Origin", "Who has it", "Significance"]),
        kind("factions", "Factions", "Faction", &["Members", "Goals", "Rivalries", "History"]),
        kind("worldbuilding", "Worldbuilding", "Entry", &["Rules", "History", "Limits", "Examples"]),
        kind("themes", "Themes", "Theme", &["What it means", "Where it appears", "How it changes"]),
    ]
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
    pub fn subtree(&self) -> Vec<&Node> {
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
    /// Before profile folders: the characters' sections, moved into
    /// `profiles` when an older project opens (see `repair`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub characteristics: Vec<String>,
    /// The kinds of profile this project has, Characters first.
    #[serde(default)]
    pub profiles: Vec<ProfileKind>,
    /// Compile choices (author name, format), remembered for next time.
    #[serde(default)]
    pub compile: crate::compile::CompileSettings,
    /// The project's id on the web helper, made when it's first synced.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync_id: Option<String>,
    /// The Characters folder was removed (it's there unless it was).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub no_characters: bool,
    /// Word goals: a day's, the manuscript's, a deadline (see stats.rs).
    #[serde(default, skip_serializing_if = "crate::stats::Goals::is_empty")]
    pub goals: crate::stats::Goals,
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
        let mut project = Self::create_empty(root, title)?;
        project.add_document(&Place::Into(Role::Manuscript.folder().into()), "Untitled")?;
        Ok(project)
    }

    /// A new project with nothing in it yet (for an import to fill).
    pub fn create_empty(root: &Path, title: &str) -> io::Result<Self> {
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
                characteristics: Vec::new(),
                profiles: Vec::new(),
                compile: Default::default(),
                sync_id: None,
                no_characters: false,
                goals: Default::default(),
            },
        };
        project.repair();
        project.save()?;
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
            if role == Role::Characters && self.manifest.no_characters {
                continue;
            }
            if !self.manifest.binder.iter().any(|n| n.role == Some(role)) {
                self.manifest.binder.push(Node::role_folder(role));
            }
            let _ = fs::create_dir_all(self.root.join(role.folder()));
        }
        // Characters is a profile kind every project has; an older project's
        // characteristics become its sections.
        if !self.manifest.no_characters && !self.manifest.profiles.iter().any(|k| k.id == CHARACTERS) {
            let sections = std::mem::take(&mut self.manifest.characteristics);
            let sections = if sections.is_empty() { default_characteristics() } else { sections };
            self.manifest.profiles.insert(0, ProfileKind { id: CHARACTERS.into(), name: "Characters".into(), item: "Character".into(), sections });
        }
        self.manifest.characteristics.clear();
        // Every other kind has its folder at the top of the binder.
        for kind in self.manifest.profiles.iter().filter(|k| k.id != CHARACTERS) {
            if !self.manifest.binder.iter().any(|n| n.id == kind.id) {
                self.manifest.binder.push(Node {
                    id: kind.id.clone(),
                    title: kind.name.clone(),
                    role: Some(Role::Profile),
                    path: None,
                    children: Vec::new(),
                    meta: Meta::default(),
                });
            }
            let _ = fs::create_dir_all(self.root.join(&kind.id));
        }
        // Ongoing joined the statuses after To do: renumber what's after it.
        if self.manifest.statuses == STATUSES_BEFORE_ONGOING {
            fn shift(node: &mut Node) {
                if let Some(i) = node.meta.status.as_mut().filter(|i| **i >= 1) {
                    *i += 1;
                }
                node.children.iter_mut().for_each(shift);
            }
            self.manifest.binder.iter_mut().for_each(shift);
            self.manifest.statuses = default_statuses();
        }
        // A scene's characters, from before profile folders.
        fn move_links(node: &mut Node) {
            let old = std::mem::take(&mut node.meta.characters);
            if !old.is_empty() {
                node.meta.links.entry(CHARACTERS.into()).or_default().extend(old);
            }
            node.children.iter_mut().for_each(move_links);
        }
        self.manifest.binder.iter_mut().for_each(move_links);
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

    /// Rename the project (its title, not its folder) and save.
    pub fn set_title(&mut self, title: &str) -> io::Result<()> {
        self.manifest.title = clean_title(title)?;
        self.save()
    }

    pub fn node(&self, id: &str) -> Option<&Node> {
        find(&self.manifest.binder, id)
    }

    /// The top-level folder an item is in (itself, if it's one).
    fn top_of(&self, id: &str) -> Option<&Node> {
        self.manifest.binder.iter().find(|top| top.subtree().iter().any(|n| n.id == id))
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

    // ----- profiles ------------------------------------------------------------

    pub fn profile_kind(&self, kind: &str) -> Option<&ProfileKind> {
        self.manifest.profiles.iter().find(|k| k.id == kind)
    }

    /// If this item is a profile (a document in a profile folder), its kind.
    pub fn profile_of(&self, id: &str) -> Option<&ProfileKind> {
        let top = self.top_of(id)?;
        let is_profile_folder = matches!(top.role, Some(Role::Characters | Role::Profile));
        let is_document = self.node(id).is_some_and(Node::is_document);
        if is_profile_folder && is_document && top.id != id { self.profile_kind(&top.id) } else { None }
    }

    /// The profiles of a kind (its folder's documents), in binder order.
    pub fn profile_items(&self, kind: &str) -> Vec<&Node> {
        self.manifest
            .binder
            .iter()
            .filter(|top| top.id == kind)
            .flat_map(Node::subtree)
            .filter(|n| n.is_document())
            .collect()
    }

    /// The scenes a profile appears in (outside the Trash), in binder order.
    pub fn appearances(&self, kind: &str, item: &str) -> Vec<&Node> {
        self.documents()
            .into_iter()
            .filter(|n| n.meta.links.get(kind).is_some_and(|ids| ids.iter().any(|i| i == item)))
            .collect()
    }

    /// Add a profile folder of this kind (a preset, or one of your own) at
    /// the top of the binder, after the other profile folders. Its id is made
    /// unique if needed.
    pub fn add_profile_folder(&mut self, mut kind: ProfileKind) -> io::Result<String> {
        // Characters back again, as the folder it always was.
        if kind.id == CHARACTERS && self.manifest.no_characters {
            self.manifest.no_characters = false;
            self.manifest.profiles.insert(0, kind);
            self.repair();
            self.save()?;
            return Ok(CHARACTERS.to_owned());
        }
        kind.name = clean_title(&kind.name)?;
        let taken = |id: &str| self.manifest.binder.iter().any(|n| n.id == id) || self.root.join(id).is_file();
        let base = slug(&kind.name);
        kind.id = (1..).map(|n| if n == 1 { base.clone() } else { format!("{base}-{n}") }).find(|id| !taken(id)).expect("free");
        if kind.item.trim().is_empty() {
            kind.item = kind.name.clone();
        }
        let id = kind.id.clone();
        self.manifest.profiles.push(kind);
        self.repair();
        self.save()?;
        Ok(id)
    }

    /// Remove an empty profile folder (not Characters) and its kind.
    pub fn remove_profile_folder(&mut self, kind: &str) -> io::Result<()> {
        let folder = self.manifest.binder.iter().find(|n| n.id == kind && matches!(n.role, Some(Role::Profile | Role::Characters)));
        match folder {
            None => return Err(invalid("That folder no longer exists.")),
            Some(f) if !f.children.is_empty() => return Err(invalid("Move or delete what's in it first.")),
            Some(_) => {}
        }
        if kind == CHARACTERS {
            self.manifest.no_characters = true;
        }
        self.manifest.binder.retain(|n| n.id != kind);
        self.manifest.profiles.retain(|k| k.id != kind);
        // Scenes no longer list any of them.
        fn unlink(node: &mut Node, kind: &str) {
            node.meta.links.remove(kind);
            node.children.iter_mut().for_each(|c| unlink(c, kind));
        }
        self.manifest.binder.iter_mut().for_each(|n| unlink(n, kind));
        let _ = fs::remove_dir(self.root.join(kind)); // only if empty
        self.save()
    }

    /// Change a kind's list of sections (in memory; saved with the autosave).
    pub fn set_sections(&mut self, kind: &str, sections: Vec<String>) {
        if let Some(k) = self.manifest.profiles.iter_mut().find(|k| k.id == kind) {
            k.sections = sections;
        }
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
            let dir = self.top_of(place.target()).expect("checked above").id.clone();
            let rel = self.free_path(&dir, &slug(&title), None);
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
            return Err(invalid("The top-level folders can't be renamed."));
        }
        let title = clean_title(title)?;
        let mut change = None;
        if let Some(old) = node.path.clone() {
            let dir = self.top_of(id).expect("node exists").id.clone();
            let new = self.free_path(&dir, &slug(&title), Some(&old));
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
        let from_dir = self.top_of(id).expect("checked above").id.clone();
        let to_dir = self.top_of(place.target()).expect("checked above").id.clone();
        let node = take(&mut self.manifest.binder, id).expect("checked above");
        insert(&mut self.manifest.binder, place, node).expect("checked above");

        let mut changes = Vec::new();
        if from_dir != to_dir {
            let documents: Vec<(String, String)> = self
                .node(id)
                .expect("just inserted")
                .subtree()
                .into_iter()
                .filter_map(|n| Some((n.id.clone(), n.path.clone()?)))
                .collect();
            for (doc_id, old) in documents {
                let stem = Path::new(&old).file_stem().map(|s| s.to_string_lossy().into_owned());
                let new = self.free_path(&to_dir, stem.as_deref().unwrap_or("untitled"), None);
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

    /// Delete everything in the Trash for good, snapshots included. Returns
    /// the files removed, so open copies can be closed.
    ///
    /// First, everything it removes (each document under its title, with
    /// its snapshots) is zipped and handed to `keep`, which puts the zip
    /// somewhere safe (the Mac's Trash). If that fails, nothing is removed.
    pub fn empty_trash(&mut self, keep: &mut dyn FnMut(&Path) -> io::Result<()>) -> io::Result<Vec<PathBuf>> {
        let trash = self.manifest.binder.iter().find(|n| n.role == Some(Role::Trash)).expect("repair keeps the trash");
        if trash.children.is_empty() {
            return Ok(Vec::new());
        }
        let zip = self.zip_of(&trash.children)?;
        let kept = keep(&zip);
        let _ = fs::remove_file(&zip);
        if let Some(parent) = zip.parent() {
            let _ = fs::remove_dir(parent);
        }
        kept?;

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
                crate::snapshots::delete_all(&self.root, &node.id)?;
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

    /// A zip of these items, for emptying the Trash: each document as
    /// "Folder/Title.md", its snapshots in "Folder/Title (snapshots)/".
    /// Made in a folder of its own under the temporary folder.
    fn zip_of(&self, items: &[Node]) -> io::Result<PathBuf> {
        fn gather(project: &Project, node: &Node, prefix: &str, files: &mut Vec<(String, PathBuf)>, used: &mut BTreeSet<String>) {
            let clean: String = node.title.chars().map(|c| if matches!(c, '/' | ':' | '\\') { '-' } else { c }).collect();
            let mut name = format!("{prefix}{clean}");
            let mut n = 2;
            while used.contains(&name) {
                name = format!("{prefix}{clean} ({n})");
                n += 1;
            }
            used.insert(name.clone());
            if let Some(rel) = &node.path {
                let path = project.root.join(rel);
                if path.is_file() {
                    files.push((format!("{name}.md"), path));
                }
            }
            for snapshot in crate::snapshots::list(&project.root, &node.id) {
                if let Some(file) = snapshot.path.file_name() {
                    files.push((format!("{name} (snapshots)/{}", file.to_string_lossy()), snapshot.path.clone()));
                }
            }
            for child in &node.children {
                gather(project, child, &format!("{name}/"), files, used);
            }
        }
        let mut files = Vec::new();
        let mut used = BTreeSet::new();
        for item in items {
            gather(self, item, "", &mut files, &mut used);
        }
        let dir = std::env::temp_dir().join(format!("sloth-writer-emptied-{}", random_id(8)));
        fs::create_dir_all(&dir)?;
        let when = chrono::Local::now().format("%Y-%m-%d %H.%M");
        let zip = dir.join(format!("{} \u{2014} emptied from the Trash {when}.zip", self.title().replace(['/', ':'], "-")));
        crate::backup::zip_files(&files, &zip)?;
        Ok(zip)
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

    pub fn add_status(&mut self, name: &str) -> usize {
        self.manifest.statuses.push(name.to_owned());
        self.manifest.statuses.len() - 1
    }

    /// Remove a status; items that had it have none, the others keep theirs.
    pub fn remove_status(&mut self, index: usize) {
        if index >= self.manifest.statuses.len() {
            return;
        }
        self.manifest.statuses.remove(index);
        let renumber = |i: usize| match i {
            i if i == index => None,
            i if i > index => Some(i - 1),
            i => Some(i),
        };
        for top in &mut self.manifest.binder {
            restatus(top, &renumber);
        }
    }

    /// Swap a status with the next one, every item keeping its own.
    pub fn swap_statuses(&mut self, index: usize) {
        if index + 1 >= self.manifest.statuses.len() {
            return;
        }
        self.manifest.statuses.swap(index, index + 1);
        let renumber = |i: usize| Some(if i == index { index + 1 } else if i == index + 1 { index } else { i });
        for top in &mut self.manifest.binder {
            restatus(top, &renumber);
        }
    }

    /// How many items have a status.
    pub fn status_uses(&self, index: usize) -> usize {
        self.manifest.binder.iter().flat_map(|n| n.subtree()).filter(|n| n.meta.status == Some(index)).count()
    }

    /// Refuse places that would break the binder's shape.
    fn check_place(&self, moving: Option<&str>, place: &Place) -> io::Result<()> {
        let target = self.node(place.target()).ok_or_else(|| invalid("That item no longer exists."))?;
        if target.role.is_some() && !matches!(place, Place::Into(_)) {
            return Err(invalid("Items go inside one of the top-level folders."));
        }
        if let Some(id) = moving {
            let node = self.node(id).ok_or_else(|| invalid("That item no longer exists."))?;
            if node.role.is_some() {
                return Err(invalid("The top-level folders can't be moved."));
            }
            if node.subtree().iter().any(|n| n.id == target.id) {
                return Err(invalid("An item can't go inside itself."));
            }
        }
        Ok(())
    }

    /// A relative path in folder `dir` (a top-level folder's id: manuscript,
    /// characters, locations…) that no other item uses and no file occupies:
    /// `stem.md`, else `stem-2.md`, `stem-3.md`… `current` is the item's own
    /// path, which it may keep.
    fn free_path(&self, dir: &str, stem: &str, current: Option<&str>) -> String {
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
                1 => format!("{dir}/{stem}.md"),
                n => format!("{dir}/{stem}-{n}.md"),
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
            let id = random_id(6);
            if !used.contains(id.as_str()) {
                return id;
            }
        }
    }

    // ----- the web editor ------------------------------------------------------------

    /// Is this item on the web: marked itself, or inside a marked folder?
    pub fn on_ipad(&self, id: &str) -> bool {
        fn walk(nodes: &[Node], id: &str, marked: bool) -> Option<bool> {
            nodes.iter().find_map(|n| {
                let marked = marked || n.meta.on_ipad;
                if n.id == id { Some(marked) } else { walk(&n.children, id, marked) }
            })
        }
        walk(&self.manifest.binder, id, false).unwrap_or(false)
    }

    /// The documents on the web (outside the Trash), in binder order, each
    /// with the folder it's in (for the web editor's list).
    pub fn ipad_documents(&self) -> Vec<(&Node, String)> {
        fn walk<'a>(nodes: &'a [Node], parent: &str, marked: bool, out: &mut Vec<(&'a Node, String)>) {
            for n in nodes {
                let marked = marked || n.meta.on_ipad;
                if marked && n.is_document() {
                    out.push((n, parent.to_owned()));
                }
                walk(&n.children, &n.title, marked, out);
            }
        }
        let mut out = Vec::new();
        for top in self.manifest.binder.iter().filter(|n| n.role != Some(Role::Trash)) {
            walk(std::slice::from_ref(top), "", false, &mut out);
        }
        out
    }

    /// The project's id on the helper, made the first time it's needed.
    pub fn sync_id(&mut self) -> String {
        self.manifest.sync_id.get_or_insert_with(|| random_id(16)).clone()
    }
}

/// `len` random lower-case letters and digits.
pub fn random_id(len: usize) -> String {
    let mut out = String::with_capacity(len);
    while out.len() < len {
        // Each RandomState has fresh random keys, so this is a random number.
        let mut n = std::collections::hash_map::RandomState::new().build_hasher().finish();
        for _ in 0..12.min(len - out.len()) {
            out.push(char::from_digit((n % 36) as u32, 36).expect("below 36"));
            n /= 36;
        }
    }
    out
}

/// A folder name for a project called `title`: as typed, minus what file
/// systems don't allow ("/", ":") and leading dots, which would hide it.
pub fn folder_name(title: &str) -> String {
    let name: String = title.chars().map(|c| if matches!(c, '/' | ':' | '\\') { '-' } else { c }).collect();
    let name = name.trim().trim_start_matches('.').trim();
    if name.is_empty() { "Untitled".to_owned() } else { name.to_owned() }
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

fn restatus(node: &mut Node, renumber: &dyn Fn(usize) -> Option<usize>) {
    node.meta.status = node.meta.status.and_then(renumber);
    for child in &mut node.children {
        restatus(child, renumber);
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
    fn folder_names_keep_the_title() {
        assert_eq!(folder_name("The Long Winter"), "The Long Winter");
        assert_eq!(folder_name("Book 1: Snow/Ice"), "Book 1- Snow-Ice");
        assert_eq!(folder_name(" .hidden "), "hidden");
        assert_eq!(folder_name("..."), "Untitled");
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
        assert_eq!(titles(&created.manifest.binder), ["Manuscript", "Characters", "Research", "Notes", "Trash"]);
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
    fn the_project_can_be_renamed() {
        let dir = TempDir::new("retitle");
        let mut p = Project::create(&dir.0, "Working Title").unwrap();
        p.set_title("  The Long Winter ").unwrap();
        assert_eq!(Project::open(&dir.0).unwrap().title(), "The Long Winter");
        assert!(p.set_title(" ").is_err());
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
        assert_eq!(titles(&project.manifest.binder), ["Manuscript", "Characters", "Research", "Notes", "Trash"]);
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
            characters: Vec::new(),
            links: [("characters".to_owned(), vec!["c0ffee".to_owned()])].into(),
            on_ipad: true,
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
    fn statuses_can_be_removed_and_reordered_without_changing_anyones() {
        let dir = TempDir::new("edit-statuses");
        let mut p = Project::create(&dir.0, "Book").unwrap();
        let scene = p.manifest.binder[0].children[0].id.clone();
        let other = p.add_document(&manuscript(), "Other").unwrap();
        let status = |p: &Project, id: &str| p.node(id).unwrap().meta.status.map(|i| p.manifest.statuses[i].clone());
        p.set_meta(&scene, Meta { status: Some(2), ..Meta::default() }); // First draft
        p.set_meta(&other, Meta { status: Some(1), ..Meta::default() }); // Ongoing
        assert_eq!(p.status_uses(1), 1);

        p.swap_statuses(1); // Ongoing ↔ First draft
        assert_eq!(p.manifest.statuses[1..3], ["First draft", "Ongoing"]);
        assert_eq!(status(&p, &scene).as_deref(), Some("First draft"));
        assert_eq!(status(&p, &other).as_deref(), Some("Ongoing"));

        p.remove_status(0); // To do: nobody had it
        assert_eq!(status(&p, &scene).as_deref(), Some("First draft"));
        p.remove_status(1); // Ongoing: Other loses it
        assert_eq!(status(&p, &other), None);
        assert_eq!(status(&p, &scene).as_deref(), Some("First draft"));

        let added = p.add_status("With editor");
        assert_eq!(p.manifest.statuses[added], "With editor");
    }

    #[test]
    fn ongoing_joins_the_statuses_without_changing_anyones() {
        let dir = TempDir::new("ongoing");
        let mut p = Project::create(&dir.0, "Book").unwrap();
        let scene = p.manifest.binder[0].children[0].id.clone();
        // As an older project had it: "Revised".
        p.manifest.statuses = STATUSES_BEFORE_ONGOING.map(String::from).to_vec();
        p.set_meta(&scene, Meta { status: Some(2), ..Meta::default() });
        p.save().unwrap();
        let p = Project::open(&dir.0).unwrap();
        assert_eq!(p.manifest.statuses[1], "Ongoing");
        let status = p.node(&scene).unwrap().meta.status.unwrap();
        assert_eq!(p.manifest.statuses[status], "Revised");
        // And only once.
        p.save().unwrap();
        assert_eq!(Project::open(&dir.0).unwrap().node(&scene).unwrap().meta.status, Some(status));
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
    fn marking_for_the_ipad_includes_whats_inside() {
        let dir = TempDir::new("ipad");
        let mut p = Project::create(&dir.0, "Book").unwrap();
        let untitled = p.manifest.binder[0].children[0].id.clone();
        let chapter = p.add_folder(&manuscript(), "Chapter 1").unwrap();
        let a = p.add_document(&Place::Into(chapter.clone()), "A").unwrap();
        let b = p.add_document(&Place::Into(chapter.clone()), "B").unwrap();
        p.set_meta(&chapter, Meta { on_ipad: true, ..Meta::default() });
        p.set_meta(&untitled, Meta { on_ipad: true, ..Meta::default() });

        assert!(p.on_ipad(&a) && p.on_ipad(&chapter));
        let listed: Vec<(String, String)> = p.ipad_documents().into_iter().map(|(n, parent)| (n.title.clone(), parent)).collect();
        assert_eq!(listed, [("Untitled".into(), "Manuscript".into()), ("A".into(), "Chapter 1".into()), ("B".into(), "Chapter 1".into())]);

        // Trashed: off the web editor.
        p.trash(&b).unwrap();
        assert_eq!(p.ipad_documents().len(), 2);

        let id = p.sync_id();
        assert_eq!(id.len(), 16);
        assert_eq!(p.sync_id(), id, "made once");
    }

    #[test]
    fn older_projects_move_into_profile_kinds() {
        let dir = TempDir::new("profiles-migrate");
        fs::create_dir_all(&dir.0).unwrap();
        // Before profile folders: characteristics on the project, characters on a scene.
        let old = r#"(format: 1, title: "Old", characteristics: ["Wants", "Fears"], binder: [
            (id: "manuscript", title: "Manuscript", role: Some(Manuscript), children: [
                (id: "scene1", title: "Scene", path: Some("manuscript/scene.md"), meta: (characters: ["mara01"])),
            ]),
        ])"#;
        fs::write(dir.0.join(MANIFEST_FILE), old).unwrap();
        let p = Project::open(&dir.0).unwrap();
        let characters = p.profile_kind(CHARACTERS).unwrap();
        assert_eq!(characters.sections, ["Wants", "Fears"]);
        assert_eq!(p.node("scene1").unwrap().meta.links["characters"], ["mara01"]);
        assert!(p.node("scene1").unwrap().meta.characters.is_empty());
        // Written back in the new form.
        p.save().unwrap();
        let text = fs::read_to_string(dir.0.join(MANIFEST_FILE)).unwrap();
        assert!(!text.contains("characteristics") && !text.contains("characters: ["), "{text}");
    }

    #[test]
    fn the_characters_folder_can_go_and_come_back() {
        let dir = TempDir::new("no-characters");
        let mut p = Project::create(&dir.0, "Essays").unwrap();
        let person = p.add_document(&Place::Into(CHARACTERS.into()), "Mara").unwrap();
        assert!(p.remove_profile_folder(CHARACTERS).is_err(), "not while it has someone in it");
        p.trash(&person).unwrap();
        p.remove_profile_folder(CHARACTERS).unwrap();
        assert!(p.profile_kind(CHARACTERS).is_none());
        assert!(!p.manifest.binder.iter().any(|n| n.role == Some(Role::Characters)));
        // Opening the project doesn't bring it back.
        let mut p = Project::open(&dir.0).unwrap();
        assert!(!p.manifest.binder.iter().any(|n| n.role == Some(Role::Characters)));
        // It's in the presets again, and comes back as it was.
        let preset = profile_presets().into_iter().map(|(k, _)| k).find(|k| k.id == CHARACTERS).unwrap();
        assert_eq!(p.add_profile_folder(preset).unwrap(), CHARACTERS);
        let p = Project::open(&dir.0).unwrap();
        assert!(p.manifest.binder.iter().any(|n| n.role == Some(Role::Characters)));
        assert_eq!(p.profile_kind(CHARACTERS).unwrap().sections, default_characteristics());
    }

    #[test]
    fn there_are_presets_for_non_fiction() {
        let presets = profile_presets();
        assert!(presets.iter().any(|(k, fiction)| k.id == "sources" && !fiction));
        assert!(presets.iter().any(|(k, fiction)| k.id == CHARACTERS && *fiction));
        let mut ids: Vec<_> = presets.iter().map(|(k, _)| k.id.clone()).collect();
        ids.dedup();
        assert_eq!(ids.len(), presets.len());
    }

    #[test]
    fn profile_folders_come_and_go() {
        let dir = TempDir::new("profiles");
        let mut p = Project::create(&dir.0, "Book").unwrap();
        assert_eq!(p.manifest.profiles.iter().map(|k| k.id.as_str()).collect::<Vec<_>>(), ["characters"]);

        let locations = profile_presets().into_iter().map(|(k, _)| k).find(|k| k.id == "locations").unwrap();
        let id = p.add_profile_folder(locations).unwrap();
        assert_eq!(id, "locations");
        // At the top, after Characters.
        assert_eq!(titles(&p.manifest.binder), ["Manuscript", "Characters", "Locations", "Research", "Notes", "Trash"]);
        assert!(dir.0.join("locations").is_dir());

        // A profile there lives in locations/ and is a Location.
        let harbour = p.add_document(&Place::Into("locations".into()), "The Harbour").unwrap();
        assert_eq!(p.node(&harbour).unwrap().path.as_deref(), Some("locations/the-harbour.md"));
        assert_eq!(p.profile_of(&harbour).unwrap().item, "Location");
        assert!(p.profile_of("locations").is_none(), "the folder itself isn't a profile");

        // Linked from a scene, it appears there.
        let scene = p.manifest.binder[0].children[0].id.clone();
        p.set_meta(&scene, Meta { links: [("locations".to_owned(), vec![harbour.clone()])].into(), ..Meta::default() });
        assert_eq!(p.appearances("locations", &harbour).len(), 1);

        // A folder of your own; a clashing name gets its own id.
        let custom = ProfileKind { id: String::new(), name: "Locations".into(), item: "Place".into(), sections: vec![] };
        assert_eq!(p.add_profile_folder(custom).unwrap(), "locations-2");

        // Only an empty one can be removed, and never Characters.
        assert!(p.remove_profile_folder("locations").is_err());
        p.trash(&harbour).unwrap();
        p.remove_profile_folder("locations").unwrap();
        assert!(p.profile_kind("locations").is_none());
        assert!(!titles(&p.manifest.binder).contains(&"Locations") || p.profile_kind("locations-2").is_some());
        assert_eq!(Project::open(&dir.0).unwrap().manifest, p.manifest);
    }

    #[test]
    fn trash_then_empty() {
        let dir = TempDir::new("trash");
        let mut p = Project::create(&dir.0, "Book").unwrap();
        let id = p.add_document(&manuscript(), "Cut scene").unwrap();

        let changes = p.trash(&id).unwrap();
        assert_eq!(changes[0].to, dir.0.join("trash/cut-scene.md"));
        assert_eq!(p.role_of(&id), Some(Role::Trash));

        crate::snapshots::take(&dir.0, &id, "an old version", "").unwrap();

        // Something that can't keep the zip: nothing goes.
        assert!(p.empty_trash(&mut |_| Err(io::Error::other("no room"))).is_err());
        assert!(dir.0.join("trash/cut-scene.md").exists());
        assert_eq!(p.role_of(&id), Some(Role::Trash));

        // Kept: the zip has the document under its title, and its snapshot.
        let mut names = Vec::new();
        let removed = p
            .empty_trash(&mut |zip| {
                let mut archive = zip::ZipArchive::new(fs::File::open(zip)?).map_err(io::Error::other)?;
                names = archive.file_names().map(String::from).collect();
                let mut text = String::new();
                io::Read::read_to_string(&mut archive.by_name("Cut scene.md").map_err(io::Error::other)?, &mut text)?;
                assert_eq!(text, "");
                Ok(())
            })
            .unwrap();
        names.sort();
        assert_eq!(names.len(), 2, "{names:?}");
        assert!(names[0].starts_with("Cut scene (snapshots)/"), "{names:?}");
        assert_eq!(names[1], "Cut scene.md");
        assert_eq!(removed, [dir.0.join("trash/cut-scene.md")]);
        assert!(crate::snapshots::list(&dir.0, &id).is_empty(), "snapshots go too");
        assert!(p.node(&id).is_none());
        assert!(!dir.0.join("trash/cut-scene.md").exists());
        assert_eq!(Project::open(&dir.0).unwrap().manifest, p.manifest);
    }
}
