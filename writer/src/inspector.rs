//! The inspector: a panel on the right (⌥⌘I) for the selected item's title,
//! synopsis, status, label, keywords, word target and notes, plus a small
//! window for editing the project's labels.
//!
//! Edits change the manifest in memory and mark it unsaved; the app writes
//! it with its regular autosave (see `WriterApp::save_all`), so typing in
//! the synopsis doesn't rewrite project.ron on every keystroke.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

use eframe::egui::{self, Color32, RichText};

use super::WriterApp;
use crate::binder::BinderAction;
use crate::project::{Meta, Role};

/// Label colours, by the index stored in the manifest. Mid tones, so they
/// read on both light and dark backgrounds. New ones go at the end, so a
/// project's labels keep their colours.
pub const LABEL_COLORS: [Color32; 16] = [
    Color32::from_rgb(0xE5, 0x48, 0x4D), // red
    Color32::from_rgb(0xF7, 0x6B, 0x15), // orange
    Color32::from_rgb(0xE8, 0xB0, 0x2E), // yellow
    Color32::from_rgb(0x46, 0xA7, 0x58), // green
    Color32::from_rgb(0x12, 0xA5, 0x94), // teal
    Color32::from_rgb(0x00, 0x90, 0xFF), // blue
    Color32::from_rgb(0x8E, 0x4E, 0xC6), // purple
    Color32::from_rgb(0xD6, 0x40, 0x9F), // pink
    // Added in 1.39.0.
    Color32::from_rgb(0x8C, 0xB8, 0x2E), // lime
    Color32::from_rgb(0x38, 0xB6, 0xE0), // sky
    Color32::from_rgb(0x5B, 0x61, 0xD6), // indigo
    Color32::from_rgb(0xB4, 0x84, 0xE2), // lavender
    Color32::from_rgb(0xA8, 0x32, 0x5E), // burgundy
    Color32::from_rgb(0xA6, 0x72, 0x48), // brown
    Color32::from_rgb(0x9C, 0x98, 0x3C), // olive
    Color32::from_rgb(0x70, 0x80, 0x96), // slate
];

/// Each colour's name, for the colour picker.
pub const LABEL_COLOR_NAMES: [&str; 16] = [
    "Red", "Orange", "Yellow", "Green", "Teal", "Blue", "Purple", "Pink", "Lime", "Sky", "Indigo", "Lavender", "Burgundy", "Brown", "Olive", "Slate",
];

/// The colours there were first: what labels from Scrivener are matched to
/// (by hue, which the greyer later ones would muddle).
pub const FIRST_LABEL_COLORS: usize = 8;

pub fn label_color(index: u8) -> Color32 {
    LABEL_COLORS[index as usize % LABEL_COLORS.len()]
}

/// Text fields whose text isn't stored as typed: the title (renamed when
/// you leave the field) and the keywords (stored as a list).
pub(super) struct Buffers {
    id: String,
    title: String,
    keywords: String,
}

/// Word counts of documents that aren't open, by file, remembered until
/// the file changes.
#[derive(Default)]
pub(super) struct WordCache(HashMap<PathBuf, (Option<SystemTime>, u64, Counted)>);

/// What's kept of a saved document: its words, and how it opens.
#[derive(Clone, Default)]
struct Counted {
    words: usize,
    opening: String,
}

impl WordCache {
    /// A file's words and opening, read again only once it's changed.
    fn get(&mut self, path: PathBuf) -> Counted {
        let Ok(stat) = fs::metadata(&path) else { return Counted::default() };
        let stamp = (stat.modified().ok(), stat.len());
        if let Some((modified, len, counted)) = self.0.get(&path)
            && (*modified, *len) == stamp
        {
            return counted.clone();
        }
        let text = crate::project::read_text(&path).unwrap_or_default();
        let counted = Counted { words: count_words(&text), opening: opening(&text) };
        self.0.insert(path, (stamp.0, stamp.1, counted.clone()));
        counted
    }

    fn count(&mut self, path: PathBuf) -> usize {
        self.get(path).words
    }
}

/// What's typed in the Web address field as a web address to open, if it is one:
/// `https://` is assumed when it's left off. Only web addresses, so a
/// file path or app name typed there can never be opened.
pub fn web_address(typed: &str) -> Option<String> {
    let typed = typed.trim();
    if typed.is_empty() || typed.chars().any(char::is_whitespace) {
        return None;
    }
    let lower = typed.to_lowercase();
    let url = if lower.starts_with("https://") || lower.starts_with("http://") {
        typed.to_owned()
    } else if typed.contains("://") || typed.starts_with('/') || typed.starts_with('~') || !typed.contains('.') {
        // Another scheme (file://, an app's), a path, or not an address.
        return None;
    } else {
        format!("https://{typed}")
    };
    // Something after the scheme, with a dot in the host.
    let host = url.split("://").nth(1)?.split(['/', '?', '#']).next()?;
    (host.contains('.') && !host.starts_with('.') && !host.ends_with('.')).then_some(url)
}

/// The label colours as swatches, two rows of eight, the current one
/// ringed. Returns the one clicked.
fn color_picker(ui: &mut egui::Ui, current: u8) -> Option<u8> {
    let mut picked = None;
    let size = egui::Vec2::splat(22.0);
    for row in LABEL_COLORS.chunks(8).enumerate() {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            for (j, color) in row.1.iter().enumerate() {
                let index = (row.0 * 8 + j) as u8;
                let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
                let painter = ui.painter();
                painter.circle_filled(rect.center(), 8.0, *color);
                if index == current || response.hovered() {
                    painter.circle_stroke(rect.center(), 10.5, egui::Stroke::new(1.5, ui.visuals().text_color()));
                }
                let name = LABEL_COLOR_NAMES[index as usize];
                response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, index == current, name));
                if response.on_hover_text(name).clicked() {
                    picked = Some(index);
                }
            }
        });
    }
    picked
}

/// Put formatted text on the clipboard. (Tests keep it for themselves,
/// rather than overwriting the real clipboard: see `copied_for_test`.)
fn to_clipboard(ctx: &egui::Context, html: &str, plain: &str) -> bool {
    #[cfg(not(test))]
    {
        let _ = ctx;
        sloth_core::editor::copy_html(html, plain)
    }
    #[cfg(test)]
    {
        ctx.data_mut(|d| d.insert_temp(egui::Id::unique("copied-for-test"), (html.to_owned(), plain.to_owned())));
        true
    }
}

/// What a test copied: its HTML and plain text.
#[cfg(test)]
pub(super) fn copied_for_test(ctx: &egui::Context) -> Option<(String, String)> {
    ctx.data(|d| d.get_temp(egui::Id::unique("copied-for-test")))
}

/// Open a web address in the default browser.
pub(super) fn open_in_browser(url: &str) {
    #[cfg(not(test))]
    {
        let _ = opener::open_browser(url);
    }
    // Tests note it instead (see `opened_for_test`).
    #[cfg(test)]
    OPENED.lock().unwrap().push(url.to_owned());
}

#[cfg(test)]
static OPENED: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Every address tests have asked to open, in any test.
#[cfg(test)]
pub(super) fn opened_for_test() -> Vec<String> {
    OPENED.lock().unwrap().clone()
}

/// How many words of a document's opening a card shows.
const OPENING_WORDS: usize = 40;

/// A document's first words as they read, for a card with no synopsis:
/// Markdown, notes, comments and references left out, and headings and
/// scene breaks skipped.
pub fn opening(text: &str) -> String {
    let text = sloth_core::markdown::strip_comments(text);
    let text = crate::compile::strip_notes(&text);
    let mut words: Vec<String> = Vec::new();
    let mut more = false;
    for line in text.lines().map(str::trim) {
        if line.is_empty() || sloth_core::markdown::is_scene_break(line) || sloth_core::markdown::heading_level(line).is_some() {
            continue;
        }
        let plain = sloth_core::markdown::formatted(line).plain;
        for word in plain.split_whitespace() {
            if words.len() == OPENING_WORDS {
                more = true;
                break;
            }
            words.push(word.to_owned());
        }
        if more {
            break;
        }
    }
    let mut out = words.join(" ");
    if more {
        out.push('\u{2026}');
    }
    out
}

/// Words as targets and compile count them: inline notes (`<!-- … -->`)
/// and references (`[[…]]`) aren't part of the manuscript, so they don't
/// count.
pub fn count_words(text: &str) -> usize {
    if text.contains("{>>") {
        count_words(&sloth_core::markdown::strip_comments(text))
    } else if text.contains("<!--") || text.contains("[[") {
        crate::compile::strip_notes(&sloth_core::markdown::strip_references(text)).split_whitespace().count()
    } else {
        text.split_whitespace().count()
    }
}

impl WriterApp {
    /// Words in an item: its own text and everything under it. Open
    /// documents are counted as they are in the editor.
    pub(super) fn words_under(&mut self, id: &str) -> usize {
        let Some(project) = &self.project else { return 0 };
        let mut total = 0;
        for doc in project.documents_under(id) {
            let open = self.open.get(&doc.id).and_then(|&tab| self.docs.get(tab));
            total += match open {
                Some(tab) => count_words(&tab.doc.text),
                None => project.file_path(&doc.id).map_or(0, |path| self.word_cache.count(path)),
            };
        }
        total
    }

    /// Copy a document's text, formatted (bold, italics, headings, quotes,
    /// paragraphs), for pasting into WordPress or anywhere else: notes,
    /// comments and references left out.
    pub(super) fn copy_document(&mut self, ctx: &egui::Context, id: &str, now: f64) {
        let Some(text) = self.document_text(id) else { return };
        let formatted = sloth_core::markdown::formatted(&text);
        let title = self.project.as_ref().and_then(|p| p.node(id)).map(|n| n.title.clone()).unwrap_or_default();
        if to_clipboard(ctx, &formatted.html, &formatted.plain) {
            let words = count_words(&text);
            self.set_status(format!("Copied \u{201c}{title}\u{201d} ({} words), ready to paste", crate::stats::thousands(words as u32)), now);
        } else {
            self.error = Some("Couldn\u{2019}t reach the clipboard to copy the text.".to_owned());
        }
    }

    /// A document's opening words (see `opening`): as open in the editor,
    /// or as saved.
    pub(super) fn opening_of(&mut self, id: &str) -> String {
        if let Some(tab) = self.open.get(id).and_then(|&tab| self.docs.get(tab)) {
            return opening(&tab.doc.text);
        }
        let Some(path) = self.project.as_ref().and_then(|p| p.file_path(id)) else { return String::new() };
        self.word_cache.get(path).opening
    }

    pub(super) fn inspector(&mut self, ui: &mut egui::Ui) {
        // Slides in and out (instantly with Reduce motion).
        let mut open = self.show_inspector;
        egui::Panel::right(egui::Id::unique("inspector"))
            .resizable(true)
            .default_size(280.0)
            .size_range(220.0..=480.0)
            .show_collapsible(ui, &mut open, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.add_space(6.0);
                    self.inspector_contents(ui);
                });
            });
        self.show_inspector = open;
    }

    fn inspector_contents(&mut self, ui: &mut egui::Ui) {
        let Some(id) = self.selected.clone() else {
            ui.weak("Select an item in the binder.");
            return;
        };
        let Some(project) = &self.project else { return };
        let Some(node) = project.node(&id) else { return };
        let (title, role, mut meta) = (node.title.clone(), node.role, node.meta.clone());
        let is_document = node.is_document();
        let in_manuscript = project.role_of(&id) == Some(Role::Manuscript);
        // Ticking Target on a document starts it at the project's default.
        let default_target = project.new_document_target(&id);
        // A folder with no target of its own aims at its documents' total.
        let targets_inside = if role.is_none() && meta.target_words.is_none() { project.targets_inside(&id) } else { None };
        let is_profile = project.profile_of(&id).is_some();
        let statuses = project.manifest.statuses.clone();
        let labels = project.manifest.labels.clone();

        // Start the text buffers afresh for a new selection.
        if self.inspector_buffers.as_ref().is_none_or(|b| b.id != id) {
            self.inspector_buffers = Some(Buffers {
                id: id.clone(),
                title: title.clone(),
                keywords: meta.keywords.join(", "),
            });
        }

        // The title, and its words (against a target, if it has one).
        if role.is_some() {
            ui.label(RichText::new(&title).size(16.0).strong());
        } else {
            let buffers = self.inspector_buffers.as_mut().expect("set above");
            let field = ui.add(
                egui::TextEdit::singleline(&mut buffers.title)
                    .font(egui::FontId::proportional(16.0))
                    .desired_width(f32::INFINITY),
            );
            if field.lost_focus() {
                let typed = buffers.title.clone();
                if typed.trim() != title {
                    self.apply(BinderAction::FinishRename { id: id.clone(), title: typed });
                }
            } else if !field.has_focus() && buffers.title != title {
                // Renamed elsewhere (the binder): show the new title.
                buffers.title = title.clone();
            }
        }
        let words = self.words_under(&id);
        ui.add_space(4.0);
        let mut copy = false;
        ui.horizontal(|ui| {
            let what = if role == Some(Role::Manuscript) { "in the manuscript" } else { "" };
            let count = format!("{} {}", crate::stats::thousands(words as u32), if words == 1 { "word" } else { "words" });
            ui.label(if what.is_empty() { count } else { format!("{count} {what}") });
            // The text, formatted, ready to paste into WordPress (or anywhere).
            if is_document
                && ui
                    .small_button("Copy")
                    .on_hover_text("Copy the text with its formatting, to paste into WordPress or anywhere else. Notes and comments are left out; the title isn\u{2019}t included.")
                    .clicked()
            {
                copy = true;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(target) = &mut meta.target_words {
                    ui.add(egui::DragValue::new(target).speed(50).range(1..=10_000_000));
                }
                let mut has_target = meta.target_words.is_some();
                if ui.checkbox(&mut has_target, "Target").changed() {
                    let start = match role {
                        Some(Role::Manuscript) => 80_000,
                        _ => default_target.filter(|_| is_document).unwrap_or(2_000),
                    };
                    meta.target_words = has_target.then_some(start);
                }
                if let Some(sum) = targets_inside {
                    ui.weak(format!("{} inside", crate::stats::thousands(sum))).on_hover_text("Its documents' targets, added up. Tick Target to give it one of its own.");
                }
            });
        });
        if copy {
            let now = ui.input(|i| i.time);
            self.copy_document(ui.ctx(), &id, now);
        }
        if let Some(target) = meta.target_words.or(targets_inside).filter(|&t| t > 0) {
            let fraction = (words as f32 / target as f32).min(1.0);
            ui.add(egui::ProgressBar::new(fraction).desired_height(6.0));
        }

        if role.is_none() {
            section(ui, "Synopsis", "synopsis", true, |ui| {
                // Two lines to start; it grows with the text, then scrolls.
                egui::ScrollArea::vertical().id_salt("synopsis-scroll").max_height(260.0).show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut meta.synopsis)
                            .hint_text("What happens here, in a sentence or two")
                            .desired_rows(2)
                            .desired_width(f32::INFINITY),
                    );
                });
            });

            let editing_labels = &mut self.editing_labels;
            let editing_statuses = &mut self.editing_statuses;
            let buffers = self.inspector_buffers.as_mut().expect("set above");
            section(ui, "Details", "details", true, |ui| {
                egui::Grid::new("inspector-grid").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                    ui.label("Status");
                    let shown = meta.status.and_then(|i| statuses.get(i)).map_or("None", String::as_str);
                    egui::ComboBox::from_id_salt("status").selected_text(shown).width(150.0).show_ui(ui, |ui| {
                        ui.selectable_value(&mut meta.status, None, "None");
                        for (i, name) in statuses.iter().enumerate() {
                            ui.selectable_value(&mut meta.status, Some(i), name);
                        }
                        ui.separator();
                        if ui.button("Edit Statuses\u{2026}").clicked() {
                            *editing_statuses = true;
                        }
                    });
                    ui.end_row();

                    ui.label("Label");
                    let shown = match meta.label.and_then(|i| labels.get(i)) {
                        Some(label) => RichText::new(format!("\u{25cf} {}", label.name)).color(label_color(label.color)),
                        None => RichText::new("None"),
                    };
                    egui::ComboBox::from_id_salt("label").selected_text(shown).width(150.0).show_ui(ui, |ui| {
                        ui.selectable_value(&mut meta.label, None, "None");
                        for (i, label) in labels.iter().enumerate() {
                            let text = RichText::new(format!("\u{25cf} {}", label.name)).color(label_color(label.color));
                            ui.selectable_value(&mut meta.label, Some(i), text);
                        }
                        ui.separator();
                        if ui.button("Edit Labels\u{2026}").clicked() {
                            *editing_labels = true;
                        }
                    });
                    ui.end_row();

                    ui.label("Keywords");
                    let field = ui.add(
                        egui::TextEdit::singleline(&mut buffers.keywords)
                            .hint_text("Separated by commas")
                            .desired_width(f32::INFINITY),
                    );
                    if field.changed() {
                        meta.keywords = buffers
                            .keywords
                            .split(',')
                            .map(str::trim)
                            .filter(|k| !k.is_empty())
                            .map(str::to_owned)
                            .collect();
                    }
                    ui.end_row();

                    // Where it's published, say, with a button to go there.
                    ui.label("Web address");
                    // Right to left: the button, then the field takes the rest.
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let url = web_address(&meta.url);
                        let open = ui.add_enabled(url.is_some(), egui::Button::new("Open"));
                        let open = match &url {
                            Some(url) => open.on_hover_text(format!("Open {url} in your browser")),
                            None => open.on_disabled_hover_text("A web address, like example.com/my-post"),
                        };
                        if open.clicked()
                            && let Some(url) = &url
                        {
                            open_in_browser(url);
                        }
                        ui.add(
                            egui::TextEdit::singleline(&mut meta.url)
                                .hint_text("Where it\u{2019}s published")
                                .desired_width(ui.available_width()),
                        );
                    });
                    ui.end_row();
                });
                ui.add_space(4.0);
                if in_manuscript {
                    let mut include = !meta.exclude_from_compile;
                    if ui.checkbox(&mut include, "Include when compiling").changed() {
                        meta.exclude_from_compile = !include;
                    }
                }
            });
            // (Outside the closure above: it needs the whole app.)
            ui.indent("ipad", |ui| self.ipad_checkbox(ui, &id, &mut meta));
            if is_document && !is_profile {
                ui.indent("notepad", |ui| self.notepad_checkbox(ui, &id));
            }

            // A document's notes are above its text; anything else's are here.
            if !is_document || is_profile {
                section(ui, "Notes", "notes", true, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut meta.notes)
                            .hint_text("Anything to remember about it")
                            .desired_rows(4)
                            .desired_width(f32::INFINITY),
                    );
                });
            }

            // Who and what appears here (not for the profiles themselves).
            if !is_profile && self.project.as_ref().is_some_and(|p| !p.manifest.profiles.is_empty()) {
                ui.add_space(4.0);
                let referenced = self.referenced_by(&id);
                let open = egui::CollapsingHeader::new(RichText::new("In This Scene").strong())
                    .id_salt("in-this-scene")
                    .default_open(true)
                    .show(ui, |ui| self.scene_profiles(ui, &mut meta, &referenced))
                    .body_returned
                    .flatten();
                if let Some(profile) = open {
                    self.open_in_split(profile);
                }
            }
        }

        // Links to and from other documents (a profile has its own
        // Referenced in, in its form).
        let mut link_pick = None;
        if is_document && !is_profile {
            ui.add_space(4.0);
            let now = ui.input(|i| i.time);
            let to: Vec<(String, String)> = self
                .referenced_by(&id)
                .into_iter()
                .filter(|t| t.is_document() && t.id != id)
                .filter_map(|t| Some((t.id.clone(), self.project.as_ref()?.node(&t.id)?.title.clone())))
                .collect();
            let target = crate::references::Target { kind: crate::references::DOCUMENT.to_owned(), id: id.clone() };
            let from = self.backlinks(now, &target);
            link_pick = egui::CollapsingHeader::new(RichText::new("Links").strong())
                .id_salt("links")
                .default_open(true)
                .show(ui, |ui| Self::links_section(ui, &to, &from))
                .body_returned
                .flatten();
        }

        if let Some(project) = self.project.as_mut()
            && project.set_meta(&id, meta)
        {
            self.manifest_dirty = true;
        }
        match link_pick {
            Some(super::references_ui::LinkPick::Open(doc)) => self.open_in_split(doc),
            Some(super::references_ui::LinkPick::From(link)) => {
                let ctx = ui.ctx().clone();
                self.go_to_backlink(&ctx, &link);
                return;
            }
            None => {}
        }

        if is_document {
            self.snapshot_section(ui, &id);
        }
    }

    /// The Statuses window: rename, reorder, add and remove statuses.
    /// Every document keeps its status through a reorder; removing one
    /// leaves the documents that had it with none.
    pub(super) fn statuses_window(&mut self, ctx: &egui::Context) {
        if !self.editing_statuses {
            return;
        }
        let Some(project) = self.project.as_mut() else { return };
        let mut open = true;
        let mut changed = false;
        let (mut remove, mut swap) = (None, None);
        let count = project.manifest.statuses.len();
        let uses: Vec<usize> = (0..count).map(|i| project.status_uses(i)).collect();
        egui::Window::new("Statuses")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(340.0)
            .show(ctx, |ui| {
                ui.weak("In the order the Status menu offers them.");
                ui.add_space(4.0);
                for (i, status) in project.manifest.statuses.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        if ui.add_enabled(i > 0, egui::Button::new("\u{2191}").small()).on_hover_text("Move up").clicked() {
                            swap = Some(i - 1);
                        }
                        if ui.add_enabled(i + 1 < count, egui::Button::new("\u{2193}").small()).on_hover_text("Move down").clicked() {
                            swap = Some(i);
                        }
                        changed |= ui.add(egui::TextEdit::singleline(status).desired_width(170.0)).changed();
                        let hint = match uses[i] {
                            0 => "Nothing has this status".to_owned(),
                            1 => "1 item has this status: it will have none".to_owned(),
                            n => format!("{n} items have this status: they will have none"),
                        };
                        if ui.button("Remove").on_hover_text(hint).clicked() {
                            remove = Some(i);
                        }
                    });
                }
                ui.add_space(6.0);
                if ui.button("Add Status").clicked() {
                    project.add_status("New status");
                    changed = true;
                }
            });
        if let Some(i) = swap {
            project.swap_statuses(i);
            changed = true;
        }
        if let Some(i) = remove {
            project.remove_status(i);
            changed = true;
        }
        if changed {
            self.manifest_dirty = true;
        }
        self.editing_statuses = open;
    }

    /// The Labels window: rename, recolour, add and remove labels.
    pub(super) fn labels_window(&mut self, ctx: &egui::Context) {
        if !self.editing_labels {
            return;
        }
        let Some(project) = self.project.as_mut() else { return };
        let mut open = true;
        let mut changed = false;
        let mut remove = None;
        egui::Window::new("Labels")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(300.0)
            .show(ctx, |ui| {
                for (i, label) in project.manifest.labels.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        // Click the dot to pick a colour.
                        let dot = RichText::new("\u{25cf}").size(18.0).color(label_color(label.color));
                        ui.menu_button(dot, |ui| {
                            if let Some(color) = color_picker(ui, label.color) {
                                label.color = color;
                                changed = true;
                                ui.close();
                            }
                        })
                        .response
                        .on_hover_text("Change colour");
                        changed |= ui.add(egui::TextEdit::singleline(&mut label.name).desired_width(180.0)).changed();
                        if ui.button("Remove").clicked() {
                            remove = Some(i);
                        }
                    });
                }
                ui.add_space(6.0);
                if ui.button("Add Label").clicked() {
                    let color = (project.manifest.labels.len() % LABEL_COLORS.len()) as u8;
                    project.add_label("New label", color);
                    changed = true;
                }
            });
        if let Some(i) = remove {
            project.remove_label(i);
            changed = true;
        }
        if changed {
            self.manifest_dirty = true;
        }
        self.editing_labels = open;
    }
}

impl WriterApp {
    /// A document's notes (never compiled), folded above its text in the
    /// text's column; open when there are any.
    pub(super) fn notes_panel(&mut self, ui: &mut egui::Ui, id: &str, column: Option<f32>) {
        let Some(project) = &self.project else { return };
        let Some(node) = project.node(id) else { return };
        let mut notes = node.meta.notes.clone();
        let width = column.map_or(ui.available_width(), |w| w.min(ui.available_width()));
        let first_line = notes.lines().find(|l| !l.trim().is_empty()).unwrap_or("").to_owned();
        let mut changed = false;
        let mut height = self.settings.notes_height;
        ui.horizontal(|ui| {
            ui.add_space(((ui.available_width() - width) / 2.0).max(0.0));
            ui.vertical(|ui| {
                ui.set_width(width);
                let header = egui::CollapsingHeader::new(RichText::new("Notes").strong())
                    .id_salt(("doc-notes", id))
                    .default_open(!notes.trim().is_empty());
                let shown = header.show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt(("doc-notes-scroll", id))
                        .min_scrolled_height(height)
                        .max_height(height)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            changed = ui
                                .add(
                                    egui::TextEdit::multiline(&mut notes)
                                        .hint_text("Notes about this document: research, reminders, what to fix. Not part of the compiled book.")
                                        .desired_rows(2)
                                        .desired_width(f32::INFINITY),
                                )
                                .changed();
                        });
                    // Drag the bottom edge for more or less of them.
                    let (handle, drag) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 8.0), egui::Sense::drag());
                    let hovered = drag.hovered() || drag.dragged();
                    if hovered {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
                    }
                    let color = if hovered { ui.visuals().strong_text_color() } else { ui.visuals().weak_text_color() };
                    let mid = handle.center();
                    ui.painter().line_segment([mid - egui::vec2(18.0, 0.0), mid + egui::vec2(18.0, 0.0)], egui::Stroke::new(2.0, color));
                    if drag.dragged() {
                        height = (height + drag.drag_delta().y).clamp(48.0, 900.0);
                    }
                    let _ = drag.on_hover_text("Drag to make the notes taller or shorter");
                });
                // Folded: a glimpse of what's in them.
                if shown.body_returned.is_none() && !first_line.is_empty() {
                    let glimpse = egui::Label::new(RichText::new(&first_line).small().weak()).truncate();
                    ui.add(glimpse);
                }
            });
        });
        ui.separator();
        self.settings.notes_height = height;
        if changed
            && let Some(project) = self.project.as_mut()
        {
            let meta = Meta { notes, ..project.node(id).map(|n| n.meta.clone()).unwrap_or_default() };
            if project.set_meta(id, meta) {
                self.manifest_dirty = true;
            }
        }
    }
}

/// A part of the inspector that can be folded away (remembered by `id`).
fn section(ui: &mut egui::Ui, title: &str, id: &str, open: bool, add: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(4.0);
    egui::CollapsingHeader::new(RichText::new(title).strong()).id_salt(id).default_open(open).show(ui, add);
}

#[cfg(test)]
mod tests {
    use super::{count_words, opening, web_address};

    #[test]
    fn only_web_addresses_open() {
        assert_eq!(web_address("https://example.com/my-post").as_deref(), Some("https://example.com/my-post"));
        assert_eq!(web_address("  example.com/essays/walking?x=1 ").as_deref(), Some("https://example.com/essays/walking?x=1"));
        assert_eq!(web_address("HTTP://Example.com").as_deref(), Some("HTTP://Example.com"));
        for not in ["", "my essay", "file:///etc/passwd", "/Applications/Calculator.app", "~/notes.md", "localhost", "javascript:alert(1)", "https://", "https://.com", "ssh://host.example"] {
            assert_eq!(web_address(not), None, "{not}");
        }
    }

    #[test]
    fn a_documents_opening_reads_as_text() {
        let text = "# On Walking\n\n<!-- fix this -->I *walk* to [[Alice]]'s house, {==slowly==}{>>too slow?<<}.\n\n* * *\n\nThen **back**.";
        assert_eq!(opening(text), "I walk to 's house, slowly. Then back.");
        let long = (1..=50).map(|n| n.to_string()).collect::<Vec<_>>().join(" ");
        let shown = opening(&long);
        assert!(shown.starts_with("1 2 3") && shown.ends_with("40\u{2026}"), "{shown}");
        assert_eq!(opening("# Only a heading"), "");
    }

    #[test]
    fn notes_dont_count_as_words() {
        assert_eq!(count_words("one two three"), 3);
        assert_eq!(count_words("one <!-- not these four words --> two"), 2);
        assert_eq!(count_words("one <!-- an unfinished note"), 1);
    }
}
