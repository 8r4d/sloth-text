//! Writer's icons: characters in its own icon font
//! (`icons/writer-icons.ttf`, made from Lucide by
//! scripts/make-writer-icons.py, which also writes this file). They're
//! text, so they go in labels and buttons like any other character and
//! take the text's colour.

/// A document (Lucide: file-text).
pub const DOCUMENT: &str = "\u{e0cc}";
/// A folder (Lucide: folder).
pub const FOLDER: &str = "\u{e0d7}";
/// Open a project (Lucide: folder-open).
pub const OPEN: &str = "\u{e247}";
/// Start something new (Lucide: plus).
pub const NEW: &str = "\u{e13d}";
/// Import (Lucide: import).
pub const IMPORT: &str = "\u{e22f}";
/// Get a project from the web helper (Lucide: cloud-download).
pub const FROM_HELPER: &str = "\u{e089}";
/// The Trash (Lucide: trash-2).
pub const TRASH: &str = "\u{e18e}";
/// The mobile notepad (Lucide: notebook-pen).
pub const NOTEPAD: &str = "\u{e596}";
/// The Characters folder (Lucide: users).
pub const PEOPLE: &str = "\u{e1a4}";
/// A character (Lucide: user).
pub const PERSON: &str = "\u{e19f}";
/// A folder of profiles (Lucide: folders).
pub const PROFILES: &str = "\u{e33f}";
/// The Manuscript, Research and Notes folders (Lucide: library-big).
pub const LIBRARY: &str = "\u{e54e}";
/// A profile (Lucide: contact).
pub const CARD: &str = "\u{e09c}";
/// A research file that's a picture (Lucide: image).
pub const PICTURE: &str = "\u{e0f6}";
/// A research file: a PDF or any other (Lucide: paperclip).
pub const ATTACHMENT: &str = "\u{e12d}";
/// On the web (Lucide: globe).
pub const WEB: &str = "\u{e0e8}";
/// App settings (Lucide: settings).
pub const SETTINGS: &str = "\u{e154}";
/// About Sloth Writer (Lucide: info).
pub const ABOUT: &str = "\u{e0f9}";
/// Show or hide the binder (Lucide: panel-left).
pub const BINDER: &str = "\u{e12a}";
/// Show or hide the inspector (Lucide: panel-right).
pub const INSPECTOR: &str = "\u{e431}";
/// A second pane beside the editor (Lucide: columns-2).
pub const SPLIT: &str = "\u{e098}";
/// The Canvas (Lucide: waypoints).
pub const CANVAS: &str = "\u{e542}";
/// Focus mode (Lucide: focus).
pub const FOCUS: &str = "\u{e29e}";
/// Compile (Lucide: book-up).
pub const COMPILE: &str = "\u{e3f4}";
/// Sync with the web helper (Lucide: refresh-cw).
pub const SYNC: &str = "\u{e145}";
/// More, in a menu (Lucide: ellipsis).
pub const MORE: &str = "\u{e0b6}";
/// Writing (Lucide: pencil).
pub const WRITING: &str = "\u{e1f9}";
/// Novel (Lucide: book-open).
pub const NOVEL: &str = "\u{e05f}";
/// Short fiction (Lucide: scroll-text).
pub const SHORT_FICTION: &str = "\u{e45f}";
/// Essay (Lucide: pen-tool).
pub const ESSAY: &str = "\u{e131}";
/// Nonfiction (Lucide: lightbulb).
pub const NONFICTION: &str = "\u{e1c2}";
/// Podcast script (Lucide: headphones).
pub const PODCAST: &str = "\u{e0f1}";
/// Screenplay (Lucide: clapperboard).
pub const SCREENPLAY: &str = "\u{e29b}";
/// Stage play (Lucide: drama).
pub const STAGE_PLAY: &str = "\u{e521}";
/// Poetry (Lucide: moon).
pub const POETRY: &str = "\u{e11e}";
/// Journal (Lucide: notebook).
pub const JOURNAL: &str = "\u{e595}";
/// Personal (Lucide: heart).
pub const PERSONAL: &str = "\u{e0f2}";
/// Letters (Lucide: mail-open).
pub const LETTERS: &str = "\u{e363}";
/// Children's (Lucide: balloon).
pub const CHILDREN: &str = "\u{e6af}";
/// Fantasy (Lucide: castle).
pub const FANTASY: &str = "\u{e3e0}";
/// Mystery (Lucide: fingerprint).
pub const MYSTERY: &str = "\u{e2cb}";
/// Science fiction (Lucide: rocket).
pub const SCIENCE_FICTION: &str = "\u{e286}";
/// Horror (Lucide: ghost).
pub const HORROR: &str = "\u{e20e}";
/// Three days running; and the finale's leaves (Lucide: leaf).
pub const LEAF: &str = "\u{e2de}";
/// A week (Lucide: flower-2).
pub const FLOWER: &str = "\u{e2d4}";
/// Two weeks (Lucide: coffee).
pub const TEA: &str = "\u{e096}";
/// Thirty days (Lucide: medal).
pub const MEDAL: &str = "\u{e36f}";
/// A quarter of the manuscript (Lucide: sprout).
pub const SEEDLING: &str = "\u{e1eb}";
/// Half (Lucide: leafy-green).
pub const SPRIG: &str = "\u{e46f}";
/// Three quarters (Lucide: tree-deciduous).
pub const TREE: &str = "\u{e2f3}";
/// The target reached (Lucide: party-popper).
pub const PARTY: &str = "\u{e343}";

/// Every icon, by name, for the icon sheet and tests.
#[cfg_attr(not(test), allow(dead_code))]
pub const ALL: &[(&str, &str)] = &[
    ("DOCUMENT", DOCUMENT),
    ("FOLDER", FOLDER),
    ("OPEN", OPEN),
    ("NEW", NEW),
    ("IMPORT", IMPORT),
    ("FROM_HELPER", FROM_HELPER),
    ("TRASH", TRASH),
    ("NOTEPAD", NOTEPAD),
    ("PEOPLE", PEOPLE),
    ("PERSON", PERSON),
    ("PROFILES", PROFILES),
    ("LIBRARY", LIBRARY),
    ("CARD", CARD),
    ("PICTURE", PICTURE),
    ("ATTACHMENT", ATTACHMENT),
    ("WEB", WEB),
    ("SETTINGS", SETTINGS),
    ("ABOUT", ABOUT),
    ("BINDER", BINDER),
    ("INSPECTOR", INSPECTOR),
    ("SPLIT", SPLIT),
    ("CANVAS", CANVAS),
    ("FOCUS", FOCUS),
    ("COMPILE", COMPILE),
    ("SYNC", SYNC),
    ("MORE", MORE),
    ("WRITING", WRITING),
    ("NOVEL", NOVEL),
    ("SHORT_FICTION", SHORT_FICTION),
    ("ESSAY", ESSAY),
    ("NONFICTION", NONFICTION),
    ("PODCAST", PODCAST),
    ("SCREENPLAY", SCREENPLAY),
    ("STAGE_PLAY", STAGE_PLAY),
    ("POETRY", POETRY),
    ("JOURNAL", JOURNAL),
    ("PERSONAL", PERSONAL),
    ("LETTERS", LETTERS),
    ("CHILDREN", CHILDREN),
    ("FANTASY", FANTASY),
    ("MYSTERY", MYSTERY),
    ("SCIENCE_FICTION", SCIENCE_FICTION),
    ("HORROR", HORROR),
    ("LEAF", LEAF),
    ("FLOWER", FLOWER),
    ("TEA", TEA),
    ("MEDAL", MEDAL),
    ("SEEDLING", SEEDLING),
    ("SPRIG", SPRIG),
    ("TREE", TREE),
    ("PARTY", PARTY),
];
