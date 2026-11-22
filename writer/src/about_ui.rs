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
        ("Settings", "\u{2318},"),
        ("Help", "\u{2318}?"),
        ("Quit (saving first)", "\u{2318}Q"),
    ]),
    ("Writing", &[
        ("Bold", "\u{2318}B"),
        ("Italic", "\u{2318}I"),
        ("Em dash", "- -"),
        ("Take a Snapshot", "\u{2318}5"),
    ]),
    ("View", &[
        ("Inspector", "\u{2325}\u{2318}I"),
        ("Split View", "\u{2318}\\"),
        ("Open in Split View", "\u{2325}-click"),
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
    "Right-click in the text for Bold, Italic, Heading, Quote, Note and Scene Break.",
    "Write notes to yourself <!-- like this -->: they're left out of word counts and the compiled book.",
    "A line with only # on it is a scene break.",
    "Right-click in the binder for more: new profile folders, snapshots, Open in Split View.",
    "Double-click a binder item to rename it.",
    "Click \"Today\" in the status bar for writing statistics and goals.",
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
fn icon_texture(ctx: &egui::Context) -> egui::TextureHandle {
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
                ui.monospace(*keys);
                ui.end_row();
            }
        }
    });
    ui.add_space(10.0);
    ui.strong("Tips");
    for tip in TIPS {
        ui.label(format!("\u{2022}  {tip}"));
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
