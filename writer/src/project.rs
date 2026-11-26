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
    /// A research file's place (a PDF, a picture…), relative to the
    /// project folder: `Research Files/Ferry Timetable.pdf`. Such an item
    /// isn't a document: Writer shows the file. The file stays on each
    /// computer; syncing carries only the item.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
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
    /// A web address for it: where it's published, say. (Not `links`,
    /// which are to other documents.)
    #[serde(skip_serializing_if = "String::is_empty")]
    pub url: String,
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

/// Which group a ready-made profile kind is offered under.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresetGroup {
    Fiction,
    Worldbuilding,
    NonFiction,
}

impl PresetGroup {
    pub const ALL: [PresetGroup; 3] = [PresetGroup::Fiction, PresetGroup::Worldbuilding, PresetGroup::NonFiction];

    pub fn name(self) -> &'static str {
        match self {
            PresetGroup::Fiction => "Fiction",
            // Not "Worldbuilding": that's one of the kinds in it.
            PresetGroup::Worldbuilding => "Lore",
            PresetGroup::NonFiction => "Non-fiction",
        }
    }

    /// Kinds from another group that suit this one too, offered under both.
    pub fn also(self) -> &'static [&'static str] {
        match self {
            // Lore adds to fiction's (locations, factions, objects, the
            // world's rules), which are just above it in the menu.
            PresetGroup::Fiction | PresetGroup::Worldbuilding => &[],
            // Themes serve essays as well as novels.
            PresetGroup::NonFiction => &["themes"],
        }
    }
}

/// The kinds a project can add, ready to use, each with the group it's
/// offered under (see `PresetGroup::also` for those offered under two).
pub fn profile_presets() -> Vec<(ProfileKind, PresetGroup)> {
    let fiction = fiction_presets().into_iter().map(|k| (k, PresetGroup::Fiction));
    let world = worldbuilding_presets().into_iter().map(|k| (k, PresetGroup::Worldbuilding));
    let non_fiction = non_fiction_presets().into_iter().map(|k| (k, PresetGroup::NonFiction));
    fiction.chain(world).chain(non_fiction).collect()
}

/// For lore: a setting's realms, peoples, beliefs, creatures, tongues and
/// history (beside fiction's locations, factions, objects and rules).
fn worldbuilding_presets() -> Vec<ProfileKind> {
    vec![
        kind("realms", "Realms", "Realm", &["Rulers", "Geography", "Economy", "Allies and enemies", "History"]),
        kind("cultures", "Cultures", "Culture", &["Customs", "Values", "Daily life", "Art and stories", "Relations with others"]),
        kind("religions", "Religions", "Religion", &["Gods and powers", "Beliefs", "Practices", "Clergy", "History"]),
        kind("creatures", "Creatures", "Creature", &["Appearance", "Habitat", "Behaviour", "Uses and dangers", "Legends"]),
        kind("languages", "Languages", "Language", &["Who speaks it", "Sound", "Words and phrases", "Writing", "Names"]),
        kind("eras", "Eras", "Era", &["When", "What happened", "Who mattered", "What it left behind"]),
    ]
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

    /// A research file (a PDF, a picture…), shown rather than edited.
    pub fn is_file(&self) -> bool {
        self.file.is_some()
    }

    /// A folder: neither a document nor a file.
    pub fn is_folder(&self) -> bool {
        self.path.is_none() && self.file.is_none()
    }

    fn role_folder(role: Role) -> Self {
        Self {
            id: role.folder().to_owned(),
            title: role.title().to_owned(),
            role: Some(role),
            path: None,
            file: None,
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
    /// What sort of writing this is: its icon on the start screen.
    #[serde(default, skip_serializing_if = "Kind::is_default")]
    pub kind: Kind,
    /// The mobile notepad: the document jotted in on a phone, always on the
    /// web (see `notepad`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notepad: Option<String>,
    /// New documents and folders go at the top of their folder, newest
    /// first, rather than in order after what's selected.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub new_at_top: bool,
    /// How big the corkboard's cards are.
    #[serde(default, skip_serializing_if = "CardSize::is_default")]
    pub card_size: CardSize,
    /// Sync every document and the binder through the web helper, so
    /// other computers can share the project (see sync.rs), not only the
    /// documents marked for the web.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub whole_sync: bool,
}

/// What sort of writing a project is, shown by its icon and colour on the
/// start screen. Icons are ones egui's bundled emoji fonts have.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Novel,
    ShortFiction,
    Essay,
    Nonfiction,
    Blog,
    Podcast,
    Screenplay,
    StagePlay,
    Poetry,
    Journal,
    Personal,
    Letters,
    Children,
    Fantasy,
    Mystery,
    ScienceFiction,
    Horror,
    /// Worldbuilding: a setting's places, peoples and history.
    Lore,
    /// Anything, and a kind from a newer version that this one doesn't
    /// know (serde needs that catch-all last).
    #[default]
    #[serde(other)]
    Writing,
}

impl Kind {
    /// In the order they're offered.
    pub const ALL: [Kind; 19] = [
        Kind::Writing, Kind::Novel, Kind::ShortFiction, Kind::Essay, Kind::Nonfiction, Kind::Blog,
        Kind::Podcast, Kind::Screenplay, Kind::StagePlay, Kind::Poetry, Kind::Journal, Kind::Personal,
        Kind::Letters, Kind::Children, Kind::Fantasy, Kind::Mystery, Kind::ScienceFiction, Kind::Horror,
        Kind::Lore,
    ];

    fn is_default(&self) -> bool {
        *self == Kind::Writing
    }

    pub fn name(self) -> &'static str {
        match self {
            Kind::Writing => "Writing",
            Kind::Novel => "Novel",
            Kind::ShortFiction => "Short Fiction",
            Kind::Essay => "Essay",
            Kind::Nonfiction => "Nonfiction",
            Kind::Blog => "Blog",
            Kind::Podcast => "Podcast Script",
            Kind::Screenplay => "Screenplay",
            Kind::StagePlay => "Stage Play",
            Kind::Poetry => "Poetry",
            Kind::Journal => "Journal",
            Kind::Personal => "Personal",
            Kind::Letters => "Letters",
            Kind::Children => "Children\u{2019}s",
            Kind::Fantasy => "Fantasy",
            Kind::Mystery => "Mystery",
            Kind::ScienceFiction => "Science Fiction",
            Kind::Horror => "Horror",
            Kind::Lore => "Lore",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Kind::Writing => crate::icons::WRITING,
            Kind::Novel => crate::icons::NOVEL,
            Kind::ShortFiction => crate::icons::SHORT_FICTION,
            Kind::Essay => crate::icons::ESSAY,
            Kind::Nonfiction => crate::icons::NONFICTION,
            Kind::Blog => crate::icons::WEB,
            Kind::Podcast => crate::icons::PODCAST,
            Kind::Screenplay => crate::icons::SCREENPLAY,
            Kind::StagePlay => crate::icons::STAGE_PLAY,
            Kind::Poetry => crate::icons::POETRY,
            Kind::Journal => crate::icons::JOURNAL,
            Kind::Personal => crate::icons::PERSONAL,
            Kind::Letters => crate::icons::LETTERS,
            Kind::Children => crate::icons::CHILDREN,
            Kind::Fantasy => crate::icons::FANTASY,
            Kind::Mystery => crate::icons::MYSTERY,
            Kind::ScienceFiction => crate::icons::SCIENCE_FICTION,
            Kind::Horror => crate::icons::HORROR,
            Kind::Lore => crate::icons::LORE,
        }
    }

    /// Its colour, as (red, green, blue): mid tones, readable on light and
    /// dark backgrounds alike.
    pub fn color(self) -> (u8, u8, u8) {
        match self {
            Kind::Writing => (112, 124, 148),
            Kind::Novel => (66, 112, 196),
            Kind::ShortFiction => (40, 150, 160),
            Kind::Essay => (126, 92, 180),
            Kind::Nonfiction => (205, 150, 30),
            Kind::Blog => (30, 140, 210),
            Kind::Podcast => (210, 80, 130),
            Kind::Screenplay => (196, 76, 64),
            Kind::StagePlay => (164, 64, 152),
            Kind::Poetry => (96, 88, 200),
            Kind::Journal => (120, 146, 70),
            Kind::Personal => (220, 86, 96),
            Kind::Letters => (206, 108, 156),
            Kind::Children => (234, 120, 40),
            Kind::Fantasy => (50, 156, 96),
            Kind::Mystery => (120, 112, 132),
            Kind::ScienceFiction => (50, 120, 220),
            Kind::Horror => (124, 96, 150),
            Kind::Lore => (168, 120, 60),
        }
    }
}

/// Change the kind of a project that isn't open, in its manifest.
pub fn set_kind_of(root: &Path, kind: Kind) -> io::Result<()> {
    let path = root.join(MANIFEST_FILE);
    let mut manifest: Manifest = ron::from_str(&fs::read_to_string(&path)?).map_err(io::Error::other)?;
    manifest.kind = kind;
    let text = ron::ser::to_string_pretty(&manifest, ron::ser::PrettyConfig::default()).map_err(io::Error::other)?;
    write_atomically(&path, text.as_bytes())
}

/// A document's text as Writer works with it: decoded as the editor does
/// (whatever its encoding), with `\n` line endings however the file has
/// them, so one written on Windows (`\r\n`) reads the same as one written
/// here. Writer itself always writes `\n`.
pub fn read_text(path: &Path) -> io::Result<String> {
    Ok(sloth_core::text_format::decode(&fs::read(path)?, None).text)
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
                kind: Kind::default(),
                notepad: None,
                new_at_top: false,
                card_size: CardSize::default(),
                whole_sync: false,
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
                    file: None,
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
    pub fn kind(&self) -> Kind {
        self.manifest.kind
    }

    pub fn set_kind(&mut self, kind: Kind) -> io::Result<()> {
        self.manifest.kind = kind;
        self.save()
    }

    /// The target a new document made at or beside `near` starts with: the
    /// project's default for new documents, in the Manuscript only (not
    /// for notes, research or profiles).
    pub fn new_document_target(&self, near: &str) -> Option<u32> {
        (self.role_of(near) == Some(Role::Manuscript)).then_some(self.manifest.goals.document).flatten()
    }

    /// The targets of the documents inside a folder (or a document with
    /// documents in it), added up: what it's aiming at when it has no
    /// target of its own. `None` when none of them has one.
    pub fn targets_inside(&self, id: &str) -> Option<u32> {
        let node = self.node(id)?;
        let sum: u32 = node.subtree().into_iter().skip(1).filter(|n| n.is_document()).filter_map(|n| n.meta.target_words).sum();
        (sum > 0).then_some(sum)
    }

    /// Whether new items go at the top of their folder (see `Manifest`).
    pub fn new_at_top(&self) -> bool {
        self.manifest.new_at_top
    }

    pub fn set_new_at_top(&mut self, top: bool) -> io::Result<()> {
        self.manifest.new_at_top = top;
        self.save()
    }

    /// The folder (or document) an item is directly in; `None` at the top.
    pub fn parent_of(&self, id: &str) -> Option<&Node> {
        fn find<'a>(nodes: &'a [Node], id: &str) -> Option<&'a Node> {
            nodes.iter().find_map(|n| if n.children.iter().any(|c| c.id == id) { Some(n) } else { find(&n.children, id) })
        }
        find(&self.manifest.binder, id)
    }

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
    /// Every document, the Trash's included (`documents` leaves those out).
    pub fn all_documents(&self) -> Vec<&Node> {
        self.all_nodes().into_iter().filter(|n| n.is_document()).collect()
    }

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

    /// The project's kinds of profile, Characters first.
    pub fn profile_kinds(&self) -> &[ProfileKind] {
        &self.manifest.profiles
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
        let taken = |id: &str| {
            self.manifest.binder.iter().any(|n| n.id.eq_ignore_ascii_case(id))
                || fs::read_dir(&self.root).into_iter().flatten().flatten().any(|e| e.file_name().to_string_lossy().to_lowercase() == id)
        };
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

    /// A document becomes a plain folder (keeping its children, metadata
    /// and snapshots): its file is removed. Returns the file's path.
    pub fn make_folder(&mut self, id: &str) -> io::Result<PathBuf> {
        let node = self.node(id).ok_or_else(|| invalid("That item no longer exists."))?;
        let rel = node.path.clone().ok_or_else(|| invalid("That's already a folder."))?;
        let path = self.root.join(&rel);
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        self.node_mut(id).expect("checked above").path = None;
        self.save()?;
        Ok(path)
    }

    /// A folder becomes a document (keeping its children and metadata),
    /// with a new file holding `text`. Returns the file's path.
    pub fn make_document(&mut self, id: &str, text: &str) -> io::Result<PathBuf> {
        let node = self.node(id).ok_or_else(|| invalid("That item no longer exists."))?;
        if node.role.is_some() {
            return Err(invalid("The top-level folders can't hold text."));
        }
        if node.is_document() {
            return Err(invalid("That's already a document."));
        }
        let dir = self.top_of(id).expect("checked above").id.clone();
        let rel = self.free_path(&dir, &slug(&node.title), None);
        let path = self.root.join(&rel);
        write_atomically(&path, text.as_bytes())?;
        self.node_mut(id).expect("checked above").path = Some(rel);
        self.save()?;
        Ok(path)
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
        let target_words = if document { self.new_document_target(place.target()) } else { None };
        let meta = Meta { target_words, ..Meta::default() };
        let node = Node { id: self.new_id(), title, role: None, path, file: None, children: Vec::new(), meta };
        let id = node.id.clone();
        insert(&mut self.manifest.binder, place, node).expect("checked above");
        self.save()?;
        Ok(id)
    }

    /// Add a research file (a PDF, a picture…): a copy of `source` goes in
    /// the project's Research Files folder, and an item for it at `place`,
    /// titled `title` (the file's name if empty). Returns its id.
    pub fn add_file(&mut self, place: &Place, source: &Path, title: &str) -> io::Result<String> {
        self.check_place(None, place)?;
        let stem = source.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let title = clean_title(if title.trim().is_empty() { &stem } else { title })?;
        let ext = source.extension().map(|x| x.to_string_lossy().into_owned()).unwrap_or_default();
        let folder = self.root.join(RESEARCH_FILES);
        fs::create_dir_all(&folder)?;
        let safe = match portable_name(&title) {
            name if name.is_empty() => "Untitled".to_owned(),
            name => name,
        };
        let dot = if ext.is_empty() { String::new() } else { format!(".{ext}") };
        // Free on disk, and not taken by an item whose file isn't here.
        let taken: HashSet<String> = self.all_nodes().into_iter().filter_map(|n| n.file.as_deref()).map(str::to_lowercase).collect();
        let name = (1..)
            .map(|n| if n == 1 { format!("{safe}{dot}") } else { format!("{safe} ({n}){dot}") })
            .find(|n| !folder.join(n).exists() && !taken.contains(&format!("{RESEARCH_FILES}/{n}").to_lowercase()))
            .expect("a free name");
        fs::copy(source, folder.join(&name))?;
        let node = Node {
            id: self.new_id(),
            title,
            role: None,
            path: None,
            file: Some(format!("{RESEARCH_FILES}/{name}")),
            children: Vec::new(),
            meta: Meta::default(),
        };
        let id = node.id.clone();
        insert(&mut self.manifest.binder, place, node).expect("checked above");
        self.save()?;
        Ok(id)
    }

    /// Where a research file item's file is (whether or not it's there).
    pub fn research_file(&self, id: &str) -> Option<PathBuf> {
        self.node(id)?.file.as_ref().map(|rel| self.root.join(rel))
    }

    /// A project fetched from the helper, as another computer has it (see
    /// sync::fetch): written to `root` (missing or empty), every document
    /// named as this computer names files.
    pub fn from_shared(root: &Path, manifest: Manifest, texts: &BTreeMap<String, String>) -> io::Result<Self> {
        if root.exists() && fs::read_dir(root)?.next().is_some() {
            return Err(invalid("That folder isn't empty."));
        }
        fs::create_dir_all(root)?;
        let mut project = Project { root: root.to_path_buf(), manifest };
        fn clear(nodes: &mut [Node]) {
            for n in nodes {
                if n.path.is_some() {
                    n.path = Some(String::new());
                }
                clear(&mut n.children);
            }
        }
        clear(&mut project.manifest.binder);
        project.repair();
        let ids: Vec<String> = project.all_nodes().iter().filter(|n| n.path.is_some()).map(|n| n.id.clone()).collect();
        for id in &ids {
            project.node_mut(id).expect("listed").path = None;
        }
        for id in ids {
            let title = project.node(&id).expect("listed").title.clone();
            let dir = project.top_of(&id).map_or_else(|| Role::Manuscript.folder().to_owned(), |t| t.id.clone());
            let rel = project.free_path(&dir, &slug(&title), None);
            fs::create_dir_all(root.join(&dir))?;
            write_atomically(&root.join(&rel), texts.get(&id).map_or("", String::as_str).as_bytes())?;
            project.node_mut(&id).expect("listed").path = Some(rel);
        }
        project.save()?;
        Ok(project)
    }

    /// Take a binder merged with another computer's (see merge.rs and
    /// sync.rs): documents gone there are kept as a snapshot and removed;
    /// documents new from there are written, named as this computer names
    /// files; and each document's file goes to the folder the merge put it
    /// in (into the Trash, say). Returns where files moved, for open
    /// documents to follow.
    pub fn take_merged(&mut self, merged: Manifest, new: &BTreeMap<String, String>, gone: &[String]) -> io::Result<Vec<PathChange>> {
        for id in gone {
            if let Some(path) = self.file_path(id) {
                if let Ok(text) = read_text(&path) {
                    let _ = crate::snapshots::take(&self.root, id, &text, "removed on another computer");
                }
                match fs::remove_file(&path) {
                    Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
                    _ => {}
                }
            }
        }
        // This computer's own settings stay its own.
        let sync_id = self.manifest.sync_id.clone();
        self.manifest = Manifest { sync_id, ..merged };
        // New documents: their file names are the other computer's.
        for id in new.keys() {
            if let Some(node) = self.node_mut(id) {
                node.path = None;
            }
        }
        for (id, text) in new {
            let Some(node) = self.node(id) else { continue };
            let title = node.title.clone();
            let Some(dir) = self.top_of(id).map(|t| t.id.clone()) else { continue };
            let rel = self.free_path(&dir, &slug(&title), None);
            write_atomically(&self.root.join(&rel), text.as_bytes())?;
            self.node_mut(id).expect("just found").path = Some(rel);
        }
        // Files in another top folder than their document now is.
        let mut changes = Vec::new();
        // (Every document, the Trash's too: that's where the merge most
        // often moves one.)
        let misplaced: Vec<(String, String, String)> = self
            .all_nodes()
            .iter()
            .filter(|n| n.is_document())
            .filter_map(|n| {
                let path = n.path.clone()?;
                let dir = self.top_of(&n.id)?.id.clone();
                (!path.starts_with(&format!("{dir}/"))).then(|| (n.id.clone(), path, dir))
            })
            .collect();
        for (id, old, dir) in misplaced {
            let stem = Path::new(&old).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "untitled".into());
            let new_rel = self.free_path(&dir, &stem, None);
            let (from, to) = (self.root.join(&old), self.root.join(&new_rel));
            if from.is_file() {
                fs::create_dir_all(to.parent().expect("in a folder"))?;
                fs::rename(&from, &to)?;
                changes.push(PathChange { from, to });
            }
            self.node_mut(&id).expect("listed").path = Some(new_rel);
        }
        self.repair();
        self.save()?;
        Ok(changes)
    }

    /// Add a document for a file already in the project's folders (`rel`,
    /// as `manuscript/walking 2.md`), such as a synced folder's conflict
    /// copy (see strays.rs). The file's left as it is. Returns its id.
    pub fn add_existing(&mut self, place: &Place, title: &str, rel: &str) -> io::Result<String> {
        self.check_place(None, place)?;
        let title = clean_title(title)?;
        if !self.root.join(rel).is_file() {
            return Err(invalid("That file isn't in the project any more."));
        }
        let node = Node { id: self.new_id(), title, role: None, path: Some(rel.to_owned()), file: None, children: Vec::new(), meta: Meta::default() };
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
                for rel in node.path.iter().chain(&node.file) {
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
            if let Some(rel) = &node.file {
                let path = project.root.join(rel);
                let ext = path.extension().map(|x| format!(".{}", x.to_string_lossy())).unwrap_or_default();
                if path.is_file() {
                    files.push((format!("{name}{ext}"), path));
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

    /// For tests elsewhere: a copy to change independently, an item to
    /// change directly, and an item gone as if emptied from the Trash.
    #[cfg(test)]
    pub fn clone_for_test(&self) -> Project {
        Project { root: self.root.clone(), manifest: self.manifest.clone() }
    }

    #[cfg(test)]
    pub fn node_mut_for_test(&mut self, id: &str) -> &mut Node {
        self.node_mut(id).expect("in the binder")
    }

    #[cfg(test)]
    pub fn remove_for_test(&mut self, id: &str) {
        fn cut(nodes: &mut Vec<Node>, id: &str) {
            nodes.retain(|n| n.id != id);
            nodes.iter_mut().for_each(|n| cut(&mut n.children, id));
        }
        cut(&mut self.manifest.binder, id);
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
            // Names that differ only in capitals are one file on the Mac and
            // Windows, so they count as taken here too, even on Linux.
            .map(str::to_lowercase)
            .collect();
        let on_disk: HashSet<String> = fs::read_dir(self.root.join(dir))
            .into_iter()
            .flatten()
            .filter_map(|e| Some(format!("{dir}/{}", e.ok()?.file_name().to_str()?).to_lowercase()))
            .collect();
        (1..)
            .map(|n| match n {
                1 => format!("{dir}/{stem}.md"),
                n => format!("{dir}/{stem}-{n}.md"),
            })
            .find(|rel| {
                Some(rel.as_str()) == current
                    || (!used.contains(&rel.to_lowercase()) && !on_disk.contains(&rel.to_lowercase()))
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
        self.web_documents().into_iter().map(|(n, path)| (n, path.last().map(|p| p.title.clone()).unwrap_or_default())).collect()
    }

    /// The documents on the web (outside the Trash), in binder order, each
    /// with the items it's inside, from its top-level folder down (for the
    /// web editor to show the binder's folders as they are).
    pub fn web_documents(&self) -> Vec<(&Node, Vec<&Node>)> {
        fn walk<'a>(nodes: &'a [Node], path: &mut Vec<&'a Node>, marked: bool, out: &mut Vec<(&'a Node, Vec<&'a Node>)>) {
            for n in nodes {
                let marked = marked || n.meta.on_ipad;
                if marked && n.is_document() {
                    out.push((n, path.clone()));
                }
                path.push(n);
                walk(&n.children, path, marked, out);
                path.pop();
            }
        }
        let mut out = Vec::new();
        for top in self.manifest.binder.iter().filter(|n| n.role != Some(Role::Trash)) {
            walk(std::slice::from_ref(top), &mut Vec::new(), false, &mut out);
        }
        out
    }

    /// The mobile notepad, if there is one: a document, outside the Trash.
    /// It's on the web whether or not it's marked, at the top of the
    /// project, for notes taken when out and about.
    pub fn notepad(&self) -> Option<&Node> {
        let id = self.manifest.notepad.as_deref()?;
        self.node(id).filter(|n| n.is_document() && self.role_of(id) != Some(Role::Trash))
    }

    /// Make a document the notepad (there's one at most), or none.
    pub fn set_notepad(&mut self, id: Option<&str>) {
        self.manifest.notepad = id.map(str::to_owned);
    }

    /// The notepad, made if there isn't one: "Mobile Notes" in Notes,
    /// holding `text`. Returns its id.
    pub fn make_notepad(&mut self, text: &str) -> io::Result<String> {
        if let Some(node) = self.notepad() {
            return Ok(node.id.clone());
        }
        let id = self.add_document(&Place::Into(Role::Notes.folder().to_owned()), "Mobile Notes")?;
        let path = self.file_path(&id).ok_or_else(|| io::Error::other("no file"))?;
        sloth_core::document::write_atomically(&path, text.as_bytes())?;
        self.set_notepad(Some(&id));
        self.save()?;
        Ok(id)
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
    let name = portable_name(title);
    let name = name.trim_start_matches('.').trim();
    if name.is_empty() { "Untitled".to_owned() } else { portable_name(name) }
}

/// A file or folder name that works on the Mac, Windows and Linux, so a
/// project can move between them (a synced folder, say). Characters Windows
/// doesn't allow (`< > : " / \ | ? *` and control characters) become `-`,
/// or nothing for quotes; the end is trimmed of spaces and dots, which
/// Windows drops; and a name Windows keeps for devices (CON, NUL, COM1…,
/// before any dot) gets a `-` after that part. Can be empty.
pub fn portable_name(name: &str) -> String {
    let name: String = name
        .chars()
        .filter(|&c| c != '"')
        .map(|c| if matches!(c, '<' | '>' | ':' | '/' | '\\' | '|' | '?' | '*') || c.is_control() { '-' } else { c })
        .collect();
    let name = name.trim().trim_end_matches(['.', ' ']);
    if reserved_on_windows(name) {
        // After the part before any dot, which is what Windows looks at.
        let stem = name.split('.').next().unwrap_or(name).trim_end();
        format!("{stem}-{}", &name[stem.len()..])
    } else {
        name.to_owned()
    }
}

/// Windows keeps these names for devices, with any extension: `con.md` and
/// `nul.txt` can't be made there.
fn reserved_on_windows(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name).trim_end().to_lowercase();
    match stem.as_str() {
        "con" | "prn" | "aux" | "nul" => true,
        _ => {
            let digit = stem.strip_prefix("com").or_else(|| stem.strip_prefix("lpt"));
            matches!(digit, Some("1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "\u{b9}" | "\u{b2}" | "\u{b3}"))
        }
    }
}

/// A file-name stem from a title: lower case, letters and digits, words
/// joined by `-`. "Mara's Arrival!" becomes `maras-arrival`.
/// How big the corkboard's cards are: smaller to see more at once, larger
/// for longer synopses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CardSize {
    Small,
    #[default]
    Medium,
    Large,
}

impl CardSize {
    pub const ALL: [CardSize; 3] = [CardSize::Small, CardSize::Medium, CardSize::Large];

    fn is_default(&self) -> bool {
        *self == CardSize::default()
    }

    pub fn name(self) -> &'static str {
        match self {
            CardSize::Small => "Small",
            CardSize::Medium => "Medium",
            CardSize::Large => "Large",
        }
    }

    /// Width and height in points.
    pub fn size(self) -> (f32, f32) {
        match self {
            CardSize::Small => (170.0, 115.0),
            CardSize::Medium => (220.0, 150.0),
            CardSize::Large => (300.0, 210.0),
        }
    }
}

/// Where research files go, in the project folder.
pub const RESEARCH_FILES: &str = "Research Files";

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
    if out.is_empty() {
        "untitled".to_owned()
    } else if reserved_on_windows(out) {
        format!("{out}-")
    } else {
        out.to_owned()
    }
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
    fn a_document_becomes_a_folder_and_back() {
        let dir = TempDir::new("doc-folder");
        let mut p = Project::create(&dir.0, "Book").unwrap();
        let chapter = p.add_document(&manuscript(), "Chapter One").unwrap();
        let essay = p.add_document(&Place::Into(chapter.clone()), "Essay").unwrap();
        let file = p.make_folder(&chapter).unwrap();
        assert!(!file.exists());
        assert!(!p.node(&chapter).unwrap().is_document());
        assert_eq!(titles(&p.node(&chapter).unwrap().children), ["Essay"]);
        assert!(p.make_folder(&chapter).is_err(), "already a folder");
        assert!(p.make_document("manuscript", "").is_err(), "not a top-level folder");

        let file = p.make_document(&chapter, "All of it").unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "All of it");
        assert_eq!(p.file_path(&chapter).unwrap(), file);
        assert!(p.node(&essay).is_some());
        // Saved: it's still so when opened again.
        let again = Project::open(&dir.0).unwrap();
        assert!(again.node(&chapter).unwrap().is_document());
    }

    #[test]
    fn folder_names_keep_the_title() {
        assert_eq!(folder_name("The Long Winter"), "The Long Winter");
        assert_eq!(folder_name("Book 1: Snow/Ice"), "Book 1- Snow-Ice");
        assert_eq!(folder_name(" .hidden "), "hidden");
        assert_eq!(folder_name("..."), "Untitled");
        // Names Windows can't have.
        assert_eq!(folder_name("What Now?"), "What Now-");
        assert_eq!(folder_name("The \"Snow\" Book"), "The Snow Book");
        assert_eq!(folder_name("Wait..."), "Wait");
        assert_eq!(folder_name("A|B*C<D>"), "A-B-C-D-");
        assert_eq!(folder_name("CON"), "CON-");
        assert_eq!(folder_name("Aux. Notes"), "Aux-. Notes");
        assert_eq!(folder_name("Console"), "Console");
    }

    #[test]
    fn a_document_written_on_windows_reads_the_same() {
        let dir = TempDir::new("crlf");
        fs::create_dir_all(&dir.0).unwrap();
        let file = dir.0.join("windows.md");
        // Notepad's way: a byte-order mark and \r\n.
        fs::write(&file, "\u{feff}# Arrival\r\n\r\nMara was late.\r\n").unwrap();
        assert_eq!(read_text(&file).unwrap(), "# Arrival\n\nMara was late.\n");
        // A profile's sections are found in it as in any other.
        let sheet = crate::character::Sheet::parse(&read_text(&file).unwrap());
        assert_eq!(sheet.sections.len(), 0);
        fs::write(&file, "Eldest\r\n\r\n## Voice\r\n\r\nDry\r\n").unwrap();
        let sheet = crate::character::Sheet::parse(&read_text(&file).unwrap());
        assert_eq!(sheet.sections[0].heading, "Voice");
        assert_eq!(sheet.sections[0].text.trim(), "Dry");
    }

    #[test]
    fn new_documents_in_the_manuscript_start_with_the_default_target() {
        let dir = TempDir::new("default-target");
        let mut p = Project::create(&dir.0.join("Essays"), "Essays").unwrap();
        let before = p.add_document(&Place::Into("manuscript".into()), "Before").unwrap();
        assert_eq!(p.node(&before).unwrap().meta.target_words, None, "no default, no target");

        p.manifest.goals.document = Some(1500);
        p.save().unwrap();
        let chapter = p.add_folder(&Place::Into("manuscript".into()), "Chapter").unwrap();
        assert_eq!(p.node(&chapter).unwrap().meta.target_words, None, "not folders");
        let essay = p.add_document(&Place::Into(chapter.clone()), "Essay").unwrap();
        assert_eq!(p.node(&essay).unwrap().meta.target_words, Some(1500));
        let next = p.add_document(&Place::After(essay), "Next").unwrap();
        assert_eq!(p.node(&next).unwrap().meta.target_words, Some(1500));
        let note = p.add_document(&Place::Into("notes".into()), "Idea").unwrap();
        assert_eq!(p.node(&note).unwrap().meta.target_words, None, "not outside the Manuscript");
        assert_eq!(p.node(&before).unwrap().meta.target_words, None, "documents already there keep theirs");

        // Kept with the project.
        assert_eq!(Project::open(&dir.0.join("Essays")).unwrap().manifest.goals.document, Some(1500));

        // The chapter has no target of its own: its documents' add up.
        assert_eq!(p.targets_inside(&chapter), Some(3000));
        assert_eq!(p.targets_inside(&note), None, "nothing inside");
    }

    #[test]
    fn a_project_deep_in_folders_still_works() {
        // Past Windows' old 260-character limit for a whole path (Rust's
        // file functions use Windows' long-path form there by themselves).
        let dir = TempDir::new("long-paths");
        let deep = (0..6).fold(dir.0.clone(), |p, i| p.join(format!("A folder with a fairly long name, number {i}")));
        let root = deep.join("The Long Winter");
        assert!(root.join("manuscript/the-long-chapter-title.md").to_string_lossy().len() > 300);

        let mut p = Project::create(&root, "The Long Winter").unwrap();
        let id = p.add_document(&Place::Into("manuscript".into()), "The long chapter title").unwrap();
        let file = p.file_path(&id).unwrap();
        fs::write(&file, "Snow.").unwrap();
        assert_eq!(read_text(&file).unwrap(), "Snow.");
        p.rename(&id, "An even longer chapter title, for good measure").unwrap();
        p.move_node(&id, &Place::Into("notes".into())).unwrap();
        assert_eq!(read_text(&p.file_path(&id).unwrap()).unwrap(), "Snow.");
        crate::snapshots::take(&root, &id, "Snow.", "before").unwrap();
        assert_eq!(crate::snapshots::list(&root, &id).len(), 1);
        p.trash(&id).unwrap();
        let backups = dir.0.join("backups");
        fs::create_dir_all(&backups).unwrap();
        let zip = crate::backup::back_up(&root, &backups, "Long").unwrap();
        assert!(zip.is_file());
        assert_eq!(Project::open(&root).unwrap().title(), "The Long Winter");
    }

    #[test]
    fn windows_device_names_are_avoided() {
        for name in ["con", "PRN", "aux", "nul", "com1", "LPT9", "com\u{b2}", "nul.txt"] {
            assert!(reserved_on_windows(name), "{name}");
        }
        for name in ["cone", "com", "com0", "com10", "lpt", "null", "\u{e9}\u{e9}", "", "contents"] {
            assert!(!reserved_on_windows(name), "{name}");
        }
        assert_eq!(slug("Con"), "con-");
        assert_eq!(slug("COM 1"), "com-1");
        assert_eq!(slug("Com1"), "com1-");
        assert_eq!(portable_name("Before: the edit."), "Before- the edit");
    }

    #[test]
    fn a_file_differing_only_in_capitals_is_taken() {
        // On Linux, `Walking.md` and `walking.md` are two files, but not on
        // the Mac or Windows: a new one must not be named so.
        let dir = TempDir::new("capitals");
        let mut p = Project::create(&dir.0, "Walks").unwrap();
        fs::write(dir.0.join("manuscript/Walking.md"), "not in the binder").unwrap();
        let id = p.add_document(&Place::Into("manuscript".into()), "Walking").unwrap();
        assert_eq!(p.node(&id).unwrap().path.as_deref(), Some("manuscript/walking-2.md"));
        assert_eq!(fs::read_to_string(dir.0.join("manuscript/Walking.md")).unwrap(), "not in the binder");
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
            url: "https://example.com/the-harbour".into(),
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

        // Each with the whole way down to it: a part, then a chapter in it.
        let part = p.add_folder(&manuscript(), "Part One").unwrap();
        p.move_node(&chapter, &Place::Into(part.clone())).unwrap();
        let paths: Vec<(String, Vec<String>)> =
            p.web_documents().into_iter().map(|(n, path)| (n.title.clone(), path.iter().map(|f| f.title.clone()).collect())).collect();
        assert_eq!(paths[1], ("A".to_owned(), vec!["Manuscript".to_owned(), "Part One".to_owned(), "Chapter 1".to_owned()]));

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
        assert!(presets.iter().any(|(k, group)| k.id == "sources" && *group == PresetGroup::NonFiction));
        assert!(presets.iter().any(|(k, group)| k.id == CHARACTERS && *group == PresetGroup::Fiction));
        assert!(presets.iter().any(|(k, group)| k.id == "realms" && *group == PresetGroup::Worldbuilding));
        let mut ids: Vec<_> = presets.iter().map(|(k, _)| k.id.clone()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), presets.len(), "each kind once, with its own id");
        // What's offered under two groups is a kind that exists.
        for group in PresetGroup::ALL {
            for id in group.also() {
                assert!(presets.iter().any(|(k, _)| k.id == *id), "{id}");
            }
        }
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
    fn research_files_are_copied_in_and_go_with_the_trash() {
        let dir = TempDir::new("files");
        let outside = TempDir::new("files-outside");
        fs::create_dir_all(&outside.0).unwrap();
        let source = outside.0.join("Timetable.pdf");
        fs::write(&source, "%PDF-1.4 pretend").unwrap();
        let mut p = Project::create(&dir.0, "Book").unwrap();
        let research = Place::Into(Role::Research.folder().into());

        // Titled by its name unless given one; copied in under a name any
        // computer can have (no colon, for Windows).
        let plain = p.add_file(&research, &source, "").unwrap();
        assert_eq!(p.node(&plain).unwrap().title, "Timetable");
        assert_eq!(p.node(&plain).unwrap().file.as_deref(), Some("Research Files/Timetable.pdf"));
        let id = p.add_file(&research, &source, "Ferry: Timetable").unwrap();
        let node = p.node(&id).unwrap();
        assert_eq!(node.title, "Ferry: Timetable");
        assert_eq!(node.file.as_deref(), Some("Research Files/Ferry- Timetable.pdf"));
        assert!(node.is_file() && !node.is_document() && !node.is_folder());
        assert_eq!(fs::read_to_string(p.research_file(&id).unwrap()).unwrap(), "%PDF-1.4 pretend");
        assert!(source.exists(), "the original stays where it was");
        // Again: a name of its own.
        let again = p.add_file(&research, &source, "Ferry: Timetable").unwrap();
        assert_eq!(p.node(&again).unwrap().file.as_deref(), Some("Research Files/Ferry- Timetable (2).pdf"));
        // Not a document: no text to count, compile or sync.
        assert!(!p.documents().iter().any(|n| n.id == id));
        // Kept with the project.
        assert_eq!(Project::open(&dir.0).unwrap().manifest, p.manifest);

        // To the Trash, then emptied: the file goes, kept in the zip.
        p.trash(&id).unwrap();
        let mut names = Vec::new();
        p.empty_trash(&mut |zip| {
            let archive = zip::ZipArchive::new(fs::File::open(zip)?).map_err(io::Error::other)?;
            names = archive.file_names().map(String::from).collect();
            Ok(())
        })
        .unwrap();
        assert_eq!(names, ["Ferry: Timetable.pdf".replace(':', "-")]);
        assert!(!dir.0.join("Research Files/Ferry- Timetable.pdf").exists());
        assert!(dir.0.join("Research Files/Ferry- Timetable (2).pdf").exists(), "the other stays");
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

    #[test]
    fn a_projects_kind_is_kept_in_its_manifest() {
        let dir = TempDir::new("kind");
        let mut p = Project::create(&dir.0, "Book").unwrap();
        let manifest = || fs::read_to_string(dir.0.join(MANIFEST_FILE)).unwrap();
        assert_eq!(p.kind(), Kind::Writing);
        assert!(!manifest().contains("kind"), "the default isn't written");
        p.set_kind(Kind::Novel).unwrap();
        assert_eq!(Project::open(&dir.0).unwrap().kind(), Kind::Novel);
        // And for a project that isn't open.
        set_kind_of(&dir.0, Kind::Poetry).unwrap();
        assert_eq!(Project::open(&dir.0).unwrap().kind(), Kind::Poetry);
        // A kind from a newer version reads as plain Writing.
        fs::write(dir.0.join(MANIFEST_FILE), manifest().replace("Poetry", "Limericks")).unwrap();
        assert_eq!(Project::open(&dir.0).unwrap().kind(), Kind::Writing);
    }
}
