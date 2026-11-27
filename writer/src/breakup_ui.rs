//! Break Up and Combine, from the binder's menu (the text is worked out in
//! breakup.rs).
//!
//! Break Up…: a document's parts, from each heading (of the levels chosen)
//! or scene break, become documents of their own inside it, each named from
//! its heading or first words. Text before the first break stays in the
//! document; if there's none, it becomes a plain folder. A snapshot of the
//! whole text is taken first.
//!
//! Combine into One Document… (a folder) and Combine with Next… (a document
//! and the one after it): the text as one document, each part under its
//! title as a heading, or with scene breaks between, or just one after the
//! other. A folder becomes a document holding it all. The documents taken
//! in go to the Trash, so they can still be got back.
//!
//! The words move; none count as written.
//!
//! If anything they'd change is on the web (and syncing is set up), they
//! sync first, and wait for a sync begun after they opened: otherwise
//! something written on the web since the last sync would have no
//! document to come back to. If that sync fails, they say so and can go
//! ahead anyway.

use eframe::egui::{self, Key};
use sloth_core::editor;

use super::WriterApp;
use crate::breakup::{self, BreakAt, Join, Part};
use crate::project::{Meta, Node, Place};

/// Where the sync a window waits for has got to.
#[derive(Debug, PartialEq)]
enum Web {
    /// Not on the web (or syncing isn't set up): nothing to wait for.
    NotNeeded,
    Waiting,
    Synced,
    Failed,
}

/// How a window waiting on the sync begun after it opened (`after`: the
/// syncs started by then) stands, from the last syncs to finish well and
/// to fail.
fn web_state(after: Option<u64>, ok: u64, failed: u64) -> Web {
    match after {
        None => Web::NotNeeded,
        Some(after) if ok > after => Web::Synced,
        Some(after) if failed > after => Web::Failed,
        Some(_) => Web::Waiting,
    }
}

/// The Break Up window, open on a document.
pub(super) struct BreakingUp {
    id: String,
    title: String,
    /// The document's text when the window opened (read again once synced).
    text: String,
    /// Waiting for a sync started after this many (see `web_state`).
    web: Option<u64>,
    /// The text has been read again since the sync.
    refreshed: bool,
    /// At headings (else at scene breaks).
    headings: bool,
    /// Headings with at most this many `#`s.
    level: usize,
    keep_headings: bool,
}

impl BreakingUp {
    fn at(&self) -> BreakAt {
        if self.headings { BreakAt::Headings(self.level) } else { BreakAt::SceneBreaks }
    }
}

/// The Combine window.
pub(super) struct Combining {
    /// The item that ends up holding it all.
    id: String,
    title: String,
    /// Combine with Next: the document after it (else everything inside `id`).
    next: Option<String>,
    join: JoinChoice,
    /// The heading level for titles.
    level: usize,
    /// Waiting for a sync started after this many (see `web_state`).
    web: Option<u64>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum JoinChoice {
    Headings,
    SceneBreaks,
    Together,
}

/// What a heading level choice reads as.
fn levels_label(level: usize) -> &'static str {
    match level {
        1 => "# headings",
        2 => "# and ## headings",
        3 => "#, ## and ### headings",
        _ => "Any heading",
    }
}

fn level_label(level: usize) -> &'static str {
    match level {
        1 => "#",
        2 => "##",
        _ => "###",
    }
}

impl WriterApp {
    // ----- the web -------------------------------------------------------------------

    /// If any of these items is on the web and syncing is set up: sync now,
    /// and wait for that (the syncs started so far). Else nothing to wait for.
    fn sync_before(&mut self, ids: &[String]) -> Option<u64> {
        let on_web = self.project.as_ref().is_some_and(|p| ids.iter().any(|id| p.on_ipad(id)));
        if !on_web || !self.sync_ready() {
            return None;
        }
        self.sync_now = true;
        Some(self.syncs_started)
    }

    fn web(&self, after: Option<u64>) -> Web {
        web_state(after, self.sync_ok, self.sync_failed)
    }

}

/// The line about the web in a window, and whether it can go ahead.
/// `message`: how the last sync went.
fn web_note(ui: &mut egui::Ui, web: &Web, message: &str) -> bool {
    match web {
        Web::NotNeeded => true,
        Web::Waiting => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.weak("It\u{2019}s on the web: syncing first, so anything written there comes in\u{2026}");
            });
            false
        }
        Web::Synced => {
            ui.weak("Synced with the web: anything written there is in.");
            true
        }
        Web::Failed => {
            let why = message.trim_start_matches("Not synced: ");
            let warn = ui.visuals().warn_fg_color;
            ui.colored_label(warn, format!("Couldn\u{2019}t sync first ({why}). Anything written on the web since the last sync won\u{2019}t come in."));
            true
        }
    }
}

impl WriterApp {
    // ----- Break Up ------------------------------------------------------------------

    pub(super) fn start_break_up(&mut self, id: &str) {
        self.save_all();
        let Some(title) = self.project.as_ref().and_then(|p| p.node(id)).map(|n| n.title.clone()) else { return };
        let Some(text) = self.document_text(id) else { return };
        // Start from what the text has: its top headings, else scene breaks.
        let level = breakup::first_heading_level(&text);
        let headings = level.is_some() || breakup::break_up(&text, BreakAt::SceneBreaks, false).pieces.is_empty();
        let web = self.sync_before(&[id.to_owned()]);
        self.breaking_up =
            Some(BreakingUp { id: id.to_owned(), title, text, web, refreshed: false, headings, level: level.unwrap_or(1).min(4), keep_headings: false });
    }

    pub(super) fn break_up_window(&mut self, ctx: &egui::Context) {
        let Some(state) = self.breaking_up.as_ref() else { return };
        let web = self.web(state.web);
        // Synced: show what the web brought in.
        if web == Web::Synced && !state.refreshed {
            let id = state.id.clone();
            let text = self.document_text(&id);
            let state = self.breaking_up.as_mut().expect("checked above");
            state.text = text.unwrap_or_default();
            state.refreshed = true;
        }
        let message = self.sync_message.clone();
        let Some(state) = self.breaking_up.as_mut() else { return };
        let broken = breakup::break_up(&state.text, state.at(), state.keep_headings);
        let mut choice = None;
        egui::Modal::new(egui::Id::unique("break-up")).show(ctx, |ui| {
            ui.set_width(420.0);
            ui.heading(format!("Break Up \u{201c}{}\u{201d}", state.title));
            ui.label("Each part becomes a document of its own, inside this one.");
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.radio_value(&mut state.headings, true, "At");
                egui::ComboBox::from_id_salt("break-up-level").selected_text(levels_label(state.level)).show_ui(ui, |ui| {
                    for level in 1..=4 {
                        if ui.selectable_value(&mut state.level, level, levels_label(level)).clicked() {
                            state.headings = true;
                        }
                    }
                });
            });
            ui.radio_value(&mut state.headings, false, "At scene breaks (#, * * *, ---)");
            ui.add_enabled(state.headings, egui::Checkbox::new(&mut state.keep_headings, "Keep each heading in its text"))
                .on_hover_text("Each document is named from its heading either way");
            ui.add_space(6.0);
            if broken.pieces.is_empty() {
                ui.weak("Nothing to break at: try another choice above.");
            } else {
                let count = broken.pieces.len();
                ui.label(format!("{count} document{}:", if count == 1 { "" } else { "s" }));
                egui::ScrollArea::vertical().max_height(220.0).auto_shrink([false, true]).show(ui, |ui| {
                    for piece in &broken.pieces {
                        ui.horizontal(|ui| {
                            ui.label(format!("{} {}", crate::icons::DOCUMENT, piece.title));
                            ui.weak(words(&piece.text));
                        });
                    }
                });
                ui.add_space(4.0);
                let lead = super::inspector::count_words(&broken.lead);
                ui.weak(if broken.lead.is_empty() {
                    format!("\u{201c}{}\u{201d} becomes a folder holding them.", state.title)
                } else {
                    format!("The {lead} word{} before the first break stay in \u{201c}{}\u{201d} itself.", if lead == 1 { "" } else { "s" }, state.title)
                });
                ui.weak("A snapshot of the whole text is kept first.");
            }
            ui.add_space(4.0);
            let go = web_note(ui, &web, &message);
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let label = if web == Web::Failed { "Break Up Anyway" } else { "Break Up" };
                if ui.add_enabled(go && !broken.pieces.is_empty(), egui::Button::new(label)).clicked() {
                    choice = Some(true);
                }
                if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                    choice = Some(false);
                }
            });
        });
        match choice {
            Some(true) => {
                let state = self.breaking_up.take().expect("open");
                let now = ctx.input(|i| i.time);
                self.break_up(ctx, &state.id, state.at(), state.keep_headings, now);
            }
            Some(false) => self.breaking_up = None,
            None => {}
        }
    }

    /// Break a document up (see the module notes).
    pub(super) fn break_up(&mut self, ctx: &egui::Context, id: &str, at: BreakAt, keep_headings: bool, now: f64) {
        self.save_all();
        let Some(text) = self.document_text(id) else { return };
        let broken = breakup::break_up(&text, at, keep_headings);
        if broken.pieces.is_empty() {
            return;
        }
        let Some(project) = self.project.as_mut() else { return };
        let Some(node) = project.node(id) else { return };
        let title = node.title.clone();
        // As Split at Cursor: each keeps the status, label, keywords and links.
        let meta = Meta {
            target_words: project.new_document_target(id),
            status: node.meta.status,
            label: node.meta.label,
            keywords: node.meta.keywords.clone(),
            links: node.meta.links.clone(),
            exclude_from_compile: node.meta.exclude_from_compile,
            ..Meta::default()
        };
        // Ahead of anything already inside it.
        let mut place = match node.children.first() {
            Some(first) => Place::Before(first.id.clone()),
            None => Place::Into(id.to_owned()),
        };
        if let Err(e) = crate::snapshots::take(&project.root, id, &text, "Before Break Up") {
            self.error = Some(format!("Couldn't keep a snapshot, so nothing was broken up: {e}"));
            return;
        }
        // The new documents first: if one fails, the text is still all here.
        let mut made = Vec::new();
        for piece in &broken.pieces {
            // A beat from Build a Plot: its note becomes the synopsis.
            let (synopsis, text) = crate::guide::step_note(&piece.text).unwrap_or_else(|| (String::new(), piece.text.clone()));
            let result = project.add_document(&place, &piece.title).and_then(|new| {
                let path = project.file_path(&new).ok_or_else(|| std::io::Error::other("no file"))?;
                sloth_core::document::write_atomically(&path, text.as_bytes())?;
                project.set_meta(&new, Meta { synopsis: synopsis.clone(), ..meta.clone() });
                Ok(new)
            });
            match result {
                Ok(new) => {
                    place = Place::After(new.clone());
                    made.push((new, text));
                }
                Err(e) => {
                    self.error = Some(format!("Couldn't make \u{201c}{}\u{201d}, so \u{201c}{title}\u{201d} still has all its text: {e}", piece.title));
                    self.manifest_dirty = true;
                    self.save_all();
                    return;
                }
            }
        }
        self.manifest_dirty = true;
        // Moved, not written, headings (now titles) included: the counts
        // change before the text does, so saving it adds nothing to today.
        self.note_moved(id, &broken.lead);
        for (new, text) in &made {
            self.note_moved(new, text);
        }
        // What's left: the text before the first break, or nothing (a folder).
        if broken.lead.is_empty() {
            if let Some(tab) = self.open.remove(id) {
                self.docs.close(tab);
            }
            if self.split.as_deref() == Some(id) {
                self.split = None;
            }
            let project = self.project.as_mut().expect("checked above");
            if let Err(e) = project.make_folder(id) {
                self.error = Some(format!("Couldn't make \u{201c}{title}\u{201d} a folder: {e}"));
            }
        } else {
            self.set_text(ctx, id, &broken.lead);
        }
        self.save_all();
        self.select(id.to_owned());
        let n = made.len();
        self.set_status(format!("Broke \u{201c}{title}\u{201d} up into {n} document{}", if n == 1 { "" } else { "s" }), now);
    }

    /// Replace a document's text, open or not, as one change that undo
    /// doesn't reach (its text has gone elsewhere).
    fn set_text(&mut self, ctx: &egui::Context, id: &str, text: &str) {
        if let Some(tab) = self.tab_of(id) {
            tab.doc.text = text.to_owned();
            editor::forget_undo(ctx, tab.id);
            self.save_all();
            return;
        }
        let Some(path) = self.project.as_ref().and_then(|p| p.file_path(id)) else { return };
        if let Err(e) = sloth_core::document::write_atomically(&path, text.as_bytes()) {
            self.error = Some(format!("Couldn't save {}: {e}", path.display()));
        }
    }

    // ----- Combine -------------------------------------------------------------------

    /// Combine into One Document… (`next` None) or Combine with Next….
    pub(super) fn start_combine(&mut self, id: &str, with_next: bool) {
        self.save_all();
        let Some(project) = &self.project else { return };
        let Some(node) = project.node(id) else { return };
        let next = if with_next {
            match next_document(&project.manifest.binder, id) {
                Some(next) => Some(next),
                None => return,
            }
        } else {
            None
        };
        // A folder of essays wants titles; joining two halves back, not.
        let join = if with_next { JoinChoice::Together } else { JoinChoice::Headings };
        let title = node.title.clone();
        // What it would change: this, and what it takes in.
        let touched: Vec<String> = match &next {
            Some(next) => vec![id.to_owned(), next.clone()],
            None => node.subtree().iter().map(|n| n.id.clone()).collect(),
        };
        let web = self.sync_before(&touched);
        self.combining = Some(Combining { id: id.to_owned(), title, next, join, level: 1, web });
    }

    /// What Combine would take in: each part, and the items it'd trash.
    fn combine_parts(&self, state: &Combining) -> (Vec<Part>, Vec<String>) {
        let Some(project) = &self.project else { return (Vec::new(), Vec::new()) };
        let text = |n: &Node| if n.is_document() { self.document_text(&n.id).unwrap_or_default() } else { String::new() };
        let part = |n: &Node, depth| Part { title: n.title.clone(), text: text(n), depth, document: n.is_document() };
        if let Some(next) = state.next.as_deref().and_then(|id| project.node(id)) {
            return (vec![part(next, 0)], vec![next.id.clone()]);
        }
        fn walk(nodes: &[Node], depth: usize, part: &dyn Fn(&Node, usize) -> Part, out: &mut Vec<Part>) {
            for n in nodes {
                out.push(part(n, depth));
                walk(&n.children, depth + 1, part, out);
            }
        }
        let Some(node) = project.node(&state.id) else { return (Vec::new(), Vec::new()) };
        let mut parts = Vec::new();
        walk(&node.children, 0, &part, &mut parts);
        (parts, node.children.iter().map(|n| n.id.clone()).collect())
    }

    pub(super) fn combine_window(&mut self, ctx: &egui::Context) {
        let Some(state) = self.combining.as_ref() else { return };
        // Read each time: once synced, it shows what the web brought in.
        let (parts, _) = self.combine_parts(state);
        let web = self.web(state.web);
        let message = self.sync_message.clone();
        let state = self.combining.as_mut().expect("checked above");
        let mut choice = None;
        egui::Modal::new(egui::Id::unique("combine")).show(ctx, |ui| {
            ui.set_width(420.0);
            match &state.next {
                Some(_) => ui.heading(format!("Combine \u{201c}{}\u{201d} with \u{201c}{}\u{201d}", state.title, parts.first().map_or("", |p| p.title.as_str()))),
                None => ui.heading(format!("Combine \u{201c}{}\u{201d} into One Document", state.title)),
            };
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.radio_value(&mut state.join, JoinChoice::Headings, "Each under its title, as a");
                egui::ComboBox::from_id_salt("combine-level").width(50.0).selected_text(level_label(state.level)).show_ui(ui, |ui| {
                    for level in 1..=3 {
                        if ui.selectable_value(&mut state.level, level, level_label(level)).clicked() {
                            state.join = JoinChoice::Headings;
                        }
                    }
                });
                ui.label("heading");
            });
            ui.radio_value(&mut state.join, JoinChoice::SceneBreaks, "With a scene break between each");
            ui.radio_value(&mut state.join, JoinChoice::Together, "Just one after another");
            ui.add_space(6.0);
            if state.next.is_none() {
                let docs = parts.iter().filter(|p| p.document).count();
                ui.label(format!("{docs} document{}:", if docs == 1 { "" } else { "s" }));
                egui::ScrollArea::vertical().max_height(220.0).auto_shrink([false, true]).show(ui, |ui| {
                    for part in &parts {
                        ui.horizontal(|ui| {
                            ui.add_space(14.0 * part.depth as f32);
                            ui.label(format!("{} {}", if part.document { crate::icons::DOCUMENT } else { crate::icons::FOLDER }, part.title));
                            if part.document {
                                ui.weak(words(&part.text));
                            }
                        });
                    }
                });
                ui.add_space(4.0);
            }
            ui.weak(match &state.next {
                Some(_) => format!("Its text joins the end of \u{201c}{}\u{201d}, and it goes to the Trash, so it can still be got back.", state.title),
                None => format!("\u{201c}{}\u{201d} becomes one document holding it all. What was in it goes to the Trash, so it can still be got back.", state.title),
            });
            ui.add_space(4.0);
            let go = web_note(ui, &web, &message);
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let label = if web == Web::Failed { "Combine Anyway" } else { "Combine" };
                if ui.add_enabled(go, egui::Button::new(label)).clicked() {
                    choice = Some(true);
                }
                if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                    choice = Some(false);
                }
            });
        });
        match choice {
            Some(true) => {
                let state = self.combining.take().expect("open");
                let join = match state.join {
                    JoinChoice::Headings => Join::Headings(state.level),
                    JoinChoice::SceneBreaks => Join::SceneBreaks,
                    JoinChoice::Together => Join::Together,
                };
                let now = ctx.input(|i| i.time);
                self.combine(ctx, &state, join, now);
            }
            Some(false) => self.combining = None,
            None => {}
        }
    }

    fn combine(&mut self, ctx: &egui::Context, state: &Combining, join: Join, now: f64) {
        self.save_all();
        let (parts, taken) = self.combine_parts(state);
        let id = state.id.as_str();
        let Some(node) = self.project.as_ref().and_then(|p| p.node(id)) else { return };
        let (title, was_document) = (node.title.clone(), node.is_document());
        let lead = if was_document { self.document_text(id).unwrap_or_default() } else { String::new() };
        let text = breakup::combine(&lead, &parts, join);
        let taken_docs: Vec<String> = {
            let project = self.project.as_ref().expect("checked above");
            taken.iter().flat_map(|t| project.documents_under(t)).map(|n| n.id.clone()).collect()
        };
        // Moved, not written, titles (now headings) included: the counts
        // change before the text does, so saving it adds nothing to today.
        for doc in &taken_docs {
            self.note_moved(doc, "");
        }
        self.note_moved(id, &text);
        if was_document {
            let project = self.project.as_ref().expect("checked above");
            if let Err(e) = crate::snapshots::take(&project.root, id, &lead, "Before Combine") {
                self.error = Some(format!("Couldn't keep a snapshot, so nothing was combined: {e}"));
                return;
            }
            self.set_text(ctx, id, &text);
        } else {
            let project = self.project.as_mut().expect("checked above");
            if let Err(e) = project.make_document(id, &text) {
                self.error = Some(format!("Couldn't make \u{201c}{title}\u{201d} a document, so nothing was combined: {e}"));
                return;
            }
        }
        for item in &taken {
            self.trash(item);
        }
        self.save_all();
        self.select(id.to_owned());
        let n = taken_docs.len();
        self.set_status(format!("Combined {n} document{} into \u{201c}{title}\u{201d}", if n == 1 { "" } else { "s" }), now);
    }
}

/// The document right after `id` in its folder, if it's a plain document.
fn next_document(nodes: &[Node], id: &str) -> Option<String> {
    for (i, n) in nodes.iter().enumerate() {
        if n.id == id {
            return nodes.get(i + 1).filter(|n| n.is_document() && n.children.is_empty()).map(|n| n.id.clone());
        }
        if let Some(found) = next_document(&n.children, id) {
            return Some(found);
        }
    }
    None
}

fn words(text: &str) -> String {
    let n = super::inspector::count_words(text);
    format!("{n} word{}", if n == 1 { "" } else { "s" })
}

#[cfg(test)]
mod tests {
    use super::{Web, web_state};

    #[test]
    fn waits_for_a_sync_begun_after_it_opened() {
        assert_eq!(web_state(None, 0, 0), Web::NotNeeded);
        // Opened after 3 syncs had started: the 3rd finishing is too early.
        assert_eq!(web_state(Some(3), 3, 0), Web::Waiting);
        assert_eq!(web_state(Some(3), 4, 0), Web::Synced);
        assert_eq!(web_state(Some(3), 2, 4), Web::Failed);
        // A later sync went well after one failed.
        assert_eq!(web_state(Some(3), 5, 4), Web::Synced);
    }
}
