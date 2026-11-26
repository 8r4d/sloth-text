//! References in the window (see references.rs): hovering a `[[reference]]`
//! says what it refers to, ⌘-clicking one opens it in the split pane (or,
//! if there's no such profile yet, offers to make one), typing `[[` offers
//! profiles' names, and a profile lists every place it's referenced, by
//! document and section.

use std::fs;

use eframe::egui::{self, Key, Modifiers, RichText};
use sloth_core::{editing, editor, find};

use super::WriterApp;
use crate::character::Sheet;
use crate::project::Place;
use crate::references::{self, Backlink, Suggestion, Target};

/// While a profile is shown, its backlinks are gathered again this often,
/// so they keep up with typing elsewhere.
const REFRESH_SECONDS: f64 = 2.0;

/// Longest a tooltip's description gets, in characters.
const DESCRIPTION: usize = 240;

/// Backlinks as last gathered, by what they point at (a profile, or a
/// document: the inspector and the split pane can each show one), and when.
#[derive(Default)]
pub(super) struct BacklinkCache {
    found: std::collections::HashMap<Target, (f64, Vec<Backlink>)>,
}

/// What was clicked in a document's Links.
pub(super) enum LinkPick {
    /// A document it links to: open it beside.
    Open(String),
    /// A place that links to it: go there.
    From(Backlink),
}

/// Suggestions for a reference being typed.
pub(super) struct Suggesting {
    doc: String,
    /// Where its `[[` is, and the cursor, in bytes.
    open: usize,
    cursor: usize,
    /// Where they're shown (under the cursor).
    at: egui::Pos2,
    items: Vec<Suggestion>,
    chosen: usize,
}

/// The New … menu for a ⌘-clicked reference to no profile yet.
pub(super) struct CreateMenu {
    items: Vec<Suggestion>,
    at: egui::Pos2,
}

/// The references an editor's frame reported, as their text: the one under
/// the pointer, and one that was ⌘-clicked.
pub(super) fn reported(text: &str, info: &editor::CursorInfo) -> (Option<String>, Option<String>) {
    let inner = |(a, b): (usize, usize)| text.get(a..b).map(|r| r.trim_start_matches("[[").trim_end_matches("]]").trim().to_owned());
    (info.reference_hovered.and_then(inner), info.reference_clicked.and_then(inner))
}

impl WriterApp {
    /// After an editor's frame: explain the reference under the pointer,
    /// and open one that was ⌘-clicked.
    pub(super) fn reference_events(&mut self, ui: &egui::Ui, hovered: Option<String>, clicked: Option<String>) {
        if let Some(inner) = hovered {
            self.reference_tooltip(ui, &inner);
        }
        let Some(inner) = clicked else { return };
        let now = ui.input(|i| i.time);
        match self.project.as_ref().and_then(|p| references::resolve(p, &inner)) {
            Some(target) => {
                if self.focus_mode {
                    self.set_focus_mode(ui.ctx(), false);
                }
                self.open_in_split(target.id);
            }
            None => {
                let (name, _) = references::split(&inner);
                let items: Vec<Suggestion> = self
                    .project
                    .as_ref()
                    .map(|p| references::suggestions(p, name).into_iter().filter(|s| s.create.is_some()).collect())
                    .unwrap_or_default();
                match ui.input(|i| i.pointer.interact_pos()) {
                    Some(at) if !items.is_empty() => self.create_menu = Some(CreateMenu { items, at }),
                    _ => self.set_status(format!("No profile called \u{201c}{name}\u{201d} yet"), now),
                }
            }
        }
    }

    /// The New … menu for a reference to no profile yet: make it, then
    /// open it in the split pane.
    pub(super) fn create_menu_ui(&mut self, ctx: &egui::Context) {
        let Some(menu) = &self.create_menu else { return };
        let mut picked = None;
        let shown = egui::Area::new(egui::Id::unique("reference-create"))
            .order(egui::Order::Foreground)
            .fixed_pos(menu.at + egui::vec2(0.0, 12.0))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.label(RichText::new("No profile by that name yet").small().weak());
                    for item in &menu.items {
                        if ui.button(&item.label).clicked() {
                            picked = Some(item.clone());
                        }
                    }
                });
            });
        let escape = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
        if picked.is_some() || escape || shown.response.clicked_elsewhere() {
            self.create_menu = None;
        }
        if let Some(item) = picked
            && let Some(id) = self.create_profile(&item)
        {
            self.open_in_split(id);
        }
    }

    /// Make the profile a suggestion offers (`create`), returning its id.
    fn create_profile(&mut self, item: &Suggestion) -> Option<String> {
        let (kind, name) = item.create.as_ref()?;
        let project = self.project.as_mut()?;
        match project.add_document(&Place::Into(kind.clone()), name) {
            Ok(id) => Some(id),
            Err(e) => {
                self.error = Some(format!("Couldn't make the profile: {e}"));
                None
            }
        }
    }

    /// Before an editor's frame: while suggestions are showing, the arrow
    /// keys choose one, Return or Tab puts it in, and Esc puts them away.
    pub(super) fn suggestion_keys(&mut self, ctx: &egui::Context, doc: &str) {
        let Some(s) = self.suggesting.as_mut().filter(|s| s.doc == doc && !s.items.is_empty()) else { return };
        let key = |k| ctx.input_mut(|i| i.consume_key(Modifiers::NONE, k));
        if key(Key::ArrowDown) {
            s.chosen = (s.chosen + 1) % s.items.len();
        } else if key(Key::ArrowUp) {
            s.chosen = (s.chosen + s.items.len() - 1) % s.items.len();
        } else if key(Key::Escape) {
            self.suggest_dismissed = Some((s.doc.clone(), s.open));
            self.suggesting = None;
        } else if key(Key::Enter) || key(Key::Tab) {
            let chosen = s.chosen;
            self.accept_suggestion(ctx, chosen);
        }
    }

    /// After an editor's frame: if a reference is being typed, show what
    /// it could be under the cursor.
    pub(super) fn suggestions_ui(&mut self, ui: &egui::Ui, doc: &str, info: &editor::CursorInfo) {
        let Some(tab) = self.open.get(doc).and_then(|&t| self.docs.get(t)) else { return };
        let ctx = ui.ctx().clone();
        let text = &tab.doc.text;
        let focused = ctx.memory(|m| m.has_focus(editor::editor_id(tab.id)));
        let mine = self.suggesting.as_ref().filter(|s| s.doc == doc);
        // While the text has focus: what's being typed at the cursor.
        let typing = editor::selection(&ctx, tab.id)
            .filter(|r| focused && r.is_empty())
            .map(|r| find::char_to_byte(text, r.start))
            .and_then(|c| references::typing_at(text, c).map(|(open, typed)| (open, c, typed.to_owned())))
            .zip(info.cursor_rect.map(|r| r.left_bottom() + egui::vec2(0.0, 4.0)));
        // A click on a suggestion takes the focus (and the cursor) from the
        // text for a moment: keep them as they were while it goes through.
        let clicking = ctx.input(|i| i.pointer.any_down() || i.pointer.any_released());
        let kept = mine.filter(|_| clicking && typing.is_none()).and_then(|s| {
            let typed = text.get(s.open + 2..s.cursor)?;
            Some(((s.open, s.cursor, typed.to_owned()), s.at))
        });
        let (Some(((open, cursor, typed), at)), Some(project)) = (typing.or(kept), &self.project) else {
            if mine.is_some() {
                self.suggesting = None;
            }
            return;
        };
        if self.suggest_dismissed == Some((doc.to_owned(), open)) {
            return;
        }
        self.suggest_dismissed = None;
        let items = references::suggestions(project, &typed);
        if items.is_empty() {
            self.suggesting = None;
            return;
        }
        let chosen = match mine {
            Some(s) if s.open == open && s.items == items => s.chosen,
            _ => 0,
        };
        let mut picked = None;
        egui::Area::new(egui::Id::unique("reference-suggestions"))
            .order(egui::Order::Foreground)
            .fixed_pos(at)
            .show(&ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(260.0);
                    for (i, item) in items.iter().enumerate() {
                        let mut label = egui::text::LayoutJob::default();
                        let style = ui.style().clone();
                        let (font, align) = (egui::FontSelection::Default, egui::Align::Center);
                        RichText::new(&item.label).append_to(&mut label, &style, font.clone(), align);
                        RichText::new(format!("   {}", item.kind_item)).small().weak().append_to(&mut label, &style, font, align);
                        let row = ui.add(egui::Button::selectable(i == chosen, label).min_size(egui::vec2(260.0, 0.0)));
                        if i == chosen {
                            row.scroll_to_me(None);
                        }
                        if row.clicked() {
                            picked = Some(i);
                        }
                    }
                    ui.label(RichText::new("\u{2191}\u{2193} choose \u{b7} Return to put it in \u{b7} Esc to close").small().weak());
                });
            });
        self.suggesting = Some(Suggesting { doc: doc.to_owned(), open, cursor, at, items, chosen });
        if let Some(i) = picked {
            self.accept_suggestion(&ctx, i);
        }
    }

    /// Put a suggestion in: what's been typed since `[[` (and a `]]` just
    /// after it) becomes the whole reference, making the profile first if
    /// it's a New … one.
    fn accept_suggestion(&mut self, ctx: &egui::Context, index: usize) {
        let Some(s) = self.suggesting.take() else { return };
        let Some(item) = s.items.get(index).cloned() else { return };
        if item.create.is_some() && self.create_profile(&item).is_none() {
            return;
        }
        let Some(tab) = self.tab_of(&s.doc) else { return };
        let text = &tab.doc.text;
        if !text.get(s.open..).is_some_and(|t| t.starts_with("[[")) || text.get(..s.cursor).is_none() {
            return; // changed meanwhile
        }
        let end = if text[s.cursor..].starts_with("]]") { s.cursor + 2 } else { s.cursor };
        let chars = find::byte_to_char(text, s.open)..find::byte_to_char(text, end);
        let id = tab.id;
        editor::set_selection(ctx, id, chars);
        let reference = format!("[[{}]]", item.insert);
        editor::apply_edit(ctx, id, &mut tab.doc.text, |t, sel| editing::replace(t, sel, &reference));
        ctx.memory_mut(|m| m.request_focus(editor::editor_id(id)));
    }

    /// What a reference refers to, beside the pointer: its kind, name and
    /// the start of its description.
    fn reference_tooltip(&mut self, ui: &egui::Ui, inner: &str) {
        let Some(project) = &self.project else { return };
        let (name, pin) = references::split(inner);
        let found = references::resolve(project, inner).map(|t| {
            // A profile's kind ("Character"), or a document's folder ("Manuscript").
            let item = match project.profile_kind(&t.kind) {
                Some(k) => k.item.clone(),
                None => project.role_of(&t.id).map_or("Document", |r| r.title()).to_owned(),
            };
            let title = project.node(&t.id).map(|n| n.title.clone()).unwrap_or_default();
            (t, item, title)
        });
        let description = found.as_ref().and_then(|(t, ..)| {
            // A document's synopsis; a profile's description.
            if t.is_document() { project.node(&t.id).map(|n| n.meta.synopsis.clone()) } else { self.document_text(&t.id) }
        });
        let profile = found.as_ref().is_some_and(|(t, ..)| !t.is_document());
        let description = description.map(|text| {
            let description = if profile { Sheet::parse(&text).description } else { text };
            let chars: Vec<char> = description.chars().collect();
            if chars.len() > DESCRIPTION {
                format!("{}\u{2026}", chars[..DESCRIPTION].iter().collect::<String>().trim_end())
            } else {
                description
            }
        });
        egui::Tooltip::always_open(ui.ctx().clone(), ui.layer_id(), egui::Id::unique("reference-tooltip"), egui::PopupAnchor::Pointer)
            .show(|ui| match &found {
                Some((_, item, title)) => {
                    ui.label(RichText::new(item).small().weak());
                    ui.label(RichText::new(title).strong());
                    if let Some(pin) = pin {
                        ui.label(RichText::new(pin).small());
                    }
                    if let Some(description) = description.filter(|d| !d.is_empty()) {
                        ui.label(description);
                    }
                    ui.label(RichText::new(sloth_core::keys::shown("\u{2318}-click to open")).small().weak());
                }
                None => {
                    ui.label(format!("No profile or document called \u{201c}{name}\u{201d} yet."));
                    ui.label(RichText::new("Add one by that name, and this reference finds it.").small().weak());
                }
            });
    }

    /// Before a profile or document is renamed: the documents whose
    /// references or links to it need the new name, each with its new text.
    pub(super) fn reference_rewrites(&self, id: &str, title: &str) -> Vec<(String, String)> {
        let Some(project) = &self.project else { return Vec::new() };
        let kind = match project.profile_of(id) {
            Some(kind) => kind.id.clone(),
            None if project.node(id).is_some_and(|n| n.is_document()) => references::DOCUMENT.to_owned(),
            None => return Vec::new(),
        };
        let target = Target { kind, id: id.to_owned() };
        project
            .documents()
            .into_iter()
            .filter(|n| n.id != id)
            .filter_map(|n| references::renamed(project, &self.document_text(&n.id)?, &target, title).map(|t| (n.id.clone(), t)))
            .collect()
    }

    /// After the rename: put those texts in, in the editor for open
    /// documents (saved with the rest) and on disk for the others.
    pub(super) fn apply_rewrites(&mut self, rewrites: Vec<(String, String)>) {
        for (doc, text) in rewrites {
            if let Some(tab) = self.tab_of(&doc) {
                tab.doc.text = text;
                continue;
            }
            let Some(path) = self.project.as_ref().and_then(|p| p.file_path(&doc)) else { continue };
            if let Err(e) = sloth_core::document::write_atomically(&path, text.as_bytes()) {
                self.error = Some(format!("Couldn't update the references in {}: {e}", path.display()));
            }
        }
        self.backlinks = None;
    }

    /// The profiles a document's references point to, each once, in the
    /// order they first come up.
    pub(super) fn referenced_by(&self, doc: &str) -> Vec<Target> {
        let (Some(project), Some(text)) = (&self.project, self.document_text(doc)) else { return Vec::new() };
        let mut out: Vec<Target> = Vec::new();
        for (_, inner) in sloth_core::markdown::references(&text) {
            if let Some(target) = references::resolve(project, inner)
                && !out.contains(&target)
            {
                out.push(target);
            }
        }
        out
    }

    /// A document's text: as open in the editor, or as saved.
    pub(super) fn document_text(&self, id: &str) -> Option<String> {
        if let Some(tab) = self.open.get(id).and_then(|&t| self.docs.get(t)) {
            return Some(tab.doc.text.clone());
        }
        let path = self.project.as_ref()?.file_path(id)?;
        fs::read(path).ok().map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
    }

    /// Where a profile is referenced, gathered now and then.
    pub(super) fn backlinks(&mut self, now: f64, target: &Target) -> Vec<Backlink> {
        if let Some((at, links)) = self.backlinks.as_ref().and_then(|c| c.found.get(target))
            && now - at < REFRESH_SECONDS
        {
            return links.clone();
        }
        let Some(project) = &self.project else { return Vec::new() };
        let links = references::backlinks(project, target, |n| self.document_text(&n.id));
        self.backlinks.get_or_insert_with(Default::default).found.insert(target.clone(), (now, links.clone()));
        links
    }

    /// A profile's "Referenced in": each document, with the sections (and
    /// lines) that refer to it. Returns the one clicked.
    pub(super) fn referenced_in(ui: &mut egui::Ui, title: &str, links: &[Backlink]) -> Option<Backlink> {
        let mut picked = None;
        ui.add_space(22.0);
        ui.label(RichText::new("Referenced in").small().weak());
        if links.is_empty() {
            ui.weak(format!("Nothing yet. Put [[{title}]] in a document where it comes up."));
        }
        let mut last_doc = None;
        for link in links {
            if last_doc != Some(&link.doc) {
                ui.add_space(6.0);
                ui.label(RichText::new(&link.doc_title).strong());
                last_doc = Some(&link.doc);
            }
            ui.indent(("backlink", &link.doc, link.at.start), |ui| {
                let mut place = link.section.clone().unwrap_or_else(|| "Before the first section".to_owned());
                if let Some(pin) = &link.pin {
                    place.push_str(&format!(" \u{b7} {pin}"));
                }
                if ui.link(place).on_hover_text("Go to it").clicked() {
                    picked = Some(link.clone());
                }
                if !link.snippet.is_empty() {
                    ui.label(RichText::new(&link.snippet).small().weak());
                }
            });
        }
        picked
    }

    /// A document's Links, in the inspector: the documents it links to, and
    /// the places that link to it. Returns what was clicked.
    pub(super) fn links_section(ui: &mut egui::Ui, to: &[(String, String)], from: &[Backlink]) -> Option<LinkPick> {
        let mut picked = None;
        if to.is_empty() && from.is_empty() {
            ui.weak("Link to another document by writing [[its title]] in the text.");
            return None;
        }
        if !to.is_empty() {
            ui.label(RichText::new("Links to").small().weak());
            ui.horizontal_wrapped(|ui| {
                for (id, title) in to {
                    // A link, as Linked from's are below: the same size and look.
                    if ui.link(title.as_str()).on_hover_text("Open it beside this one").clicked() {
                        picked = Some(LinkPick::Open(id.clone()));
                    }
                }
            });
        }
        if !from.is_empty() {
            ui.add_space(6.0);
            ui.label(RichText::new("Linked from").small().weak());
            for link in from {
                let place = match &link.section {
                    Some(section) => format!("{} \u{b7} {section}", link.doc_title),
                    None => link.doc_title.clone(),
                };
                let hint = if link.snippet.is_empty() { "Go to it".to_owned() } else { link.snippet.clone() };
                if ui.link(place).on_hover_text(hint).clicked() {
                    picked = Some(LinkPick::From(link.clone()));
                }
            }
        }
        picked
    }

    /// Open the document a backlink is in, with the reference selected.
    pub(super) fn go_to_backlink(&mut self, ctx: &egui::Context, link: &Backlink) {
        self.select(link.doc.clone());
        let Some(tab) = self.current() else { return };
        let text = &tab.doc.text;
        if text.get(link.at.clone()).is_none() {
            return;
        }
        let (start, end) = (find::byte_to_char(text, link.at.start), find::byte_to_char(text, link.at.end));
        editor::set_selection(ctx, tab.id, start..end);
        self.reveal = Some(start);
    }
}
