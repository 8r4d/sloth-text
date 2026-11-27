//! The About and Help window: the version, release notes and keyboard
//! shortcuts. Opened from the toolbar's Help button, the start screen,
//! the Settings window, or ⇧⌘/ (⌘?).

use eframe::egui::{self, RichText};

use super::WriterApp;
use crate::version;

/// The pages of the window.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum HelpPage {
    About,
    Shortcuts,
    ReleaseNotes,
}

/// Every keyboard shortcut, by where it applies.
const SHORTCUTS: &[(&str, &[(&str, &str)])] = &[
    ("Project", &[
        ("Open Project", "\u{2318}O"),
        ("New Document", "\u{2318}N"),
        ("New Folder", "\u{2325}\u{2318}N"),
        ("Save Everything Now", "\u{2318}S"),
        ("Compile", "\u{21e7}\u{2318}E"),
        ("App Settings", "\u{2318},"),
        ("Project Settings", "\u{2325}\u{2318},"),
        ("Help", "\u{2318}?"),
        ("Quit (saving first)", "\u{2318}Q"),
    ]),
    ("Writing", &[
        ("Bold", "\u{2318}B"),
        ("Italic", "\u{2318}I"),
        ("Note (not compiled): <!-- … -->", "\u{21e7}\u{2318}A"),
        ("Em dash", "- -"),
        ("Split at Cursor", "\u{2318}K"),
        ("Remove Blank Lines", "\u{21e7}\u{2318}L"),
        ("Take a Snapshot", "\u{2318}5"),
    ]),
    ("View", &[
        ("Focus Mode (Esc to leave)", "\u{2325}\u{2318}F"),
        ("Binder", "\u{2325}\u{2318}B"),
        ("Inspector", "\u{2325}\u{2318}I"),
        ("Highlight Repeated Words", "\u{2325}\u{2318}R"),
        ("Slothenings, Corkboard, Outline", "\u{2318}1, \u{2318}2, \u{2318}3"),
        ("Add Comment", "\u{2325}\u{2318}K"),
        ("Split View", "\u{2318}\\"),
        ("Open in Split View", "\u{2325}-click"),
        ("Canvas", "\u{2325}\u{2318}C"),
    ]),
    ("Find", &[
        ("Find", "\u{2318}F"),
        ("Find in Project", "\u{21e7}\u{2318}F"),
        ("Find Next", "\u{2318}G"),
        ("Find Previous", "\u{21e7}\u{2318}G"),
        ("Close Find", "Esc"),
    ]),
];

const TIPS: &[&str] = &[
    "Right-click in the text for Bold, Italic, Heading, Quote, Note, Scene Break, Change Case, Remove Blank Lines and Split at Cursor.",
    "Write notes to yourself <!-- like this -->: they're left out of word counts and the compiled book.",
    "A line with only # on it is a scene break.",
    "Build a Plot\u{2026} (in More, or Notes' right-click menu) walks through a story structure, from Story Circle to Save the Cat!, a beat at a time: your answers become a document in Notes with a heading per beat, ready for Break Up when it's time to draft.",
    "Build a Character\u{2026} (right-click Characters, or a character) asks about a character a question at a time, from a quick sketch to a full arc, and puts the answers in their profile. Build an Essay\u{2026} (More, or right-click in the Manuscript) walks an essay's shape, from Argument to Personal Essay, into a draft with its synopsis, target and tags.",
    "Select a word, right-click, Add to Glossary\u{2026}: the project's glossary is one document of # terms and their definitions, kept A to Z, and can go at the end of a compiled book.",
    "Keep PDFs and pictures in the project to look at as you write: Add File\u{2026} in the binder's right-click menu, or drop them on the window.",
    "Refer to a profile (a source, concept, theme or character) with [[its name]] where it comes up: typing [[ offers names (or makes a new one), hover to see it, \u{2318}-click to open it. Renaming a profile renames its references, and Compile can turn references to Sources into footnotes and a list of works cited. Like notes, references are left out of word counts and the compiled book, and each profile lists where it's referenced.",
    "Link to another document the same way, with [[its title]]: hover to see its synopsis, \u{2318}-click to open it beside. A document's inspector lists what it links to and what links to it, and renaming a document renames the links to it.",
    "Right-click in the binder for more: new profile folders, snapshots, Open in Split View, and on a folder, Set Status or Set Label for everything in it at once.",
    "Double-click a binder item to rename it.",
    "Posting somewhere? Copy, beside the word count in the inspector, puts the document's text on the clipboard with its formatting, ready to paste into WordPress, without notes or comments.",
    "Give documents keywords in the inspector, then filter a folder's corkboard or outline by one (the menu beside Outline) to see everything that has it, across chapters. A globe on a card means it has a web address: click it to open the post.",
    "Wrote several pieces in one document? Right-click it in the binder and choose Break Up\u{2026}: each part, from a heading or a scene break, becomes a document of its own. Combine into One Document\u{2026} (on a folder) and Combine with Next\u{2026} put them back together. For essays, tick Documents \u{2192} Their titles as headings in Compile.",
    "\u{2318}1 shows a folder (or the one the open document is in) as Slothenings: all its documents as one text to read and write through. \u{2318}2 is the corkboard, \u{2318}3 the outline.",
    "Comment on a passage: select it and press \u{2325}\u{2318}K. The comment goes in the margin beside it; Resolve takes it away and leaves the words.",
    "\u{2325}\u{2318}R highlights repeated words: echoes close together are tinted, and the words on your watch list (App Settings \u{2192} Writing) underlined.",
    "For a burst of writing, click Sprint in the status bar and pick a time. The words you write in it are counted, and Statistics keeps the week's sprints.",
    "In Focus mode (\u{2325}\u{2318}F), the paragraph you're writing stays bright and the rest fade back. Turn that off in App Settings \u{2192} Writing.",
    "To replace, choose Replace\u{2026} in the find bar (\u{2318}F). With the whole project's matches showing (\u{21e7}\u{2318}F), Replace All changes every document, after asking, and snapshots each one first.",
    "Mobile Notes (in More\u{2026}) is a notepad for jotting on your phone: on the web it's first in each project, with a New Entry button that adds the date and time. On the Mac it opens beside what you're writing. Any document can be the notepad: tick Mobile notepad in its inspector. Notes added on both sides between syncs are all kept.",
    "The Canvas (\u{2325}\u{2318}C) puts every document on one surface, strung to the profiles it refers to, whatever the binder's order. Double-click it for a note of your own; drag from a card's pin to another card to tie a string between them (right-click the string to say what it means). Right-click a note to make it a document. When it gets crowded, set Depth to 1 to see each chapter as one card, its strings gathered together.",
    "Coming from Scrivener? Close the project and choose Import from Scrivener… on the start screen.",
    "Click the goal ring (\"Today\") in the status bar for writing statistics, goals, and a calendar of the year's writing. Reaching the day's goal is celebrated once a day.",
    "Right-click an underlined word for spelling suggestions, Learn Spelling or Ignore Spelling.",
];

impl WriterApp {
    pub(super) fn help_window(&mut self, ctx: &egui::Context) {
        let Some(mut page) = self.help else { return };
        let mut open = true;
        egui::Window::new("Sloth Writer")
            .id(egui::Id::unique("help"))
            .open(&mut open)
            .collapsible(false)
            .default_size([540.0, 540.0])
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut page, HelpPage::About, "About");
                    ui.selectable_value(&mut page, HelpPage::Shortcuts, "Keyboard Shortcuts");
                    ui.selectable_value(&mut page, HelpPage::ReleaseNotes, "Release Notes");
                });
                ui.separator();
                let icon = self.about_icon.get_or_insert_with(|| icon_texture(ctx)).clone();
                egui::ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| match page {
                    HelpPage::About => about_page(ui, &icon),
                    HelpPage::Shortcuts => shortcuts_page(ui),
                    HelpPage::ReleaseNotes => release_notes_page(ui),
                });
            });
        self.help = open.then_some(page);
    }
}

/// The app icon, for the About page.
pub(super) fn icon_texture(ctx: &egui::Context) -> egui::TextureHandle {
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../icons/icon-512.png")).expect("icon is a valid PNG");
    let image = egui::ColorImage::from_rgba_unmultiplied([icon.width as usize, icon.height as usize], &icon.rgba);
    ctx.load_texture("writer-icon", image, egui::TextureOptions::LINEAR)
}

fn about_page(ui: &mut egui::Ui, icon: &egui::TextureHandle) {
    ui.vertical_centered(|ui| {
        ui.add_space(12.0);
        ui.add(egui::Image::new(icon).fit_to_exact_size(egui::vec2(96.0, 96.0)));
        ui.add_space(6.0);
        ui.label(RichText::new("Sloth Writer").size(26.0).strong());
        ui.label(format!("Version {}", version::VERSION));
        ui.label(RichText::new(version::build_info()).weak().small());
        ui.add_space(10.0);
        ui.label("A writing app for long projects: novels, scripts and anything else with chapters.");
        ui.add_space(4.0);
        ui.weak("Your work is kept as plain Markdown files in an ordinary folder.");
    });
}

fn shortcuts_page(ui: &mut egui::Ui) {
    egui::Grid::new("writer-shortcuts").num_columns(2).spacing([40.0, 4.0]).show(ui, |ui| {
        for (n, (group, items)) in SHORTCUTS.iter().enumerate() {
            if n > 0 {
                ui.label(""); // a gap between groups
                ui.end_row();
            }
            ui.strong(*group);
            ui.end_row();
            for (what, keys) in *items {
                ui.label(*what);
                ui.monospace(sloth_core::keys::shown(keys));
                ui.end_row();
            }
        }
    });
    ui.add_space(10.0);
    ui.strong("Tips");
    for tip in TIPS {
        ui.label(format!("\u{2022}  {}", sloth_core::keys::shown(tip)));
    }
}

/// CHANGELOG.md, rendered simply.
fn release_notes_page(ui: &mut egui::Ui) {
    for line in version::CHANGELOG.lines() {
        if line.starts_with("# ") {
            continue; // the document's title
        } else if let Some(heading) = line.strip_prefix("## ") {
            ui.add_space(8.0);
            ui.heading(heading);
        } else if let Some(item) = line.strip_prefix("- ") {
            ui.label(format!("\u{2022}  {}", item.replace('`', "")));
        } else if line.trim().is_empty() {
            continue;
        } else if line.starts_with("Sloth Writer uses") {
            ui.label(RichText::new(line).weak());
        } else {
            ui.add_space(4.0);
            ui.strong(line);
        }
    }
}
