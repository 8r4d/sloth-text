//! Syncing with the web helper, in the window: a sync on a background
//! thread about once a minute (and when Sync is clicked) while a project with
//! scenes on the web is open, and putting its results into the project.
//! The rules are in sync.rs.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use eframe::egui;
use serde_json::json;

use super::WriterApp;
use super::inspector::count_words;
use crate::project::{Meta, Place, Role};
use crate::snapshots;
use crate::sync::{self, Action, Http, Input, Outcome, SyncError, SyncState};

/// How often to sync while a project is open.
const SYNC_SECONDS: f64 = 60.0;

/// A sync under way: the project it's for, and where its result will come.
pub(super) struct Running {
    /// Which sync this is (see `WriterApp::syncs_started`).
    number: u64,
    root: PathBuf,
    result: mpsc::Receiver<Result<Outcome, SyncError>>,
}

impl WriterApp {
    /// The helper is set up, and this project has scenes on the web.
    pub(super) fn sync_ready(&self) -> bool {
        // Not while another copy of Writer has the project (see lock.rs).
        self.lock_lost.is_none()
            && !self.settings.helper_url.trim().is_empty()
            && !self.helper_token.trim().is_empty()
            && self.project.as_ref().is_some_and(|p| p.manifest.whole_sync || !p.ipad_documents().is_empty())
    }

    pub(super) fn sync_running(&self) -> bool {
        self.sync_running.is_some()
    }

    /// Each frame: pick up a finished sync, or start one when it's time.
    pub(super) fn sync_tick(&mut self, ctx: &egui::Context, now: f64) {
        if let Some(running) = &self.sync_running {
            match running.result.try_recv() {
                Ok(result) => {
                    let (root, number, ok) = (running.root.clone(), running.number, result.is_ok());
                    self.sync_running = None;
                    self.finish_sync(&root, result, now);
                    if ok {
                        self.sync_ok = number;
                    } else {
                        self.sync_failed = number;
                    }
                }
                Err(mpsc::TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_millis(300)),
                Err(mpsc::TryRecvError::Disconnected) => self.sync_running = None,
            }
            return;
        }
        if !self.sync_ready() {
            return;
        }
        if std::mem::take(&mut self.sync_now) || self.last_sync.is_none_or(|t| now - t >= SYNC_SECONDS) {
            self.start_sync(now);
        }
        ctx.request_repaint_after(Duration::from_secs_f64(SYNC_SECONDS));
    }

    /// Gather what the sync needs (texts as they are in the editor), and
    /// start it on a background thread so the window never waits on the network.
    fn start_sync(&mut self, now: f64) {
        self.last_sync = Some(now);
        self.syncs_started += 1;
        let number = self.syncs_started;
        // The day's writing, for the web to show (and add its own to).
        let (written, total) = self.today_and_total();
        let Some(project) = self.project.as_mut() else { return };
        let today = crate::stats::today();
        let goals = &project.manifest.goals;
        let stats = json!({
            "date": crate::stats::key(today),
            "today": written,
            "goal": goals.today(total, written, today),
            // Days in a row meeting the goal (or writing at all), for the web.
            "streak": self.stats.streak(today, goals.today(total, written, today)),
            "total": total,
            "target": goals.total,
            "deadline": goals.deadline,
            // How paragraphs look in Settings, for the web's paragraph editor
            // (it rides along with the day's figures: the helper keeps them as sent).
            // …and the theme's colours (null for Match System).
            "look": {
                "spacing": self.settings.paragraph_spacing,
                "indent": self.settings.first_line_indent,
                "theme": self.settings.theme.web_colors(),
                // A Severance theme: the web page takes on its office look
                // too, and the old screen's glow if that's on (SEVERANCE-UI.md).
                "office": self.settings.theme.is_severance(),
                "glow": self.settings.theme.has_glow() && self.settings.screen_glow,
            },
        });
        if project.manifest.sync_id.is_none() {
            self.manifest_dirty = true;
        }
        let id = project.sync_id();
        let statuses = project.manifest.statuses.clone();
        let mut index = Vec::new();
        let mut scenes = BTreeMap::new();
        let text_of = |id: &str| match self.open.get(id).and_then(|&tab| self.docs.get(tab)) {
            Some(tab) => Some(tab.doc.text.clone()),
            None => project.file_path(id).and_then(|p| crate::project::read_text(&p).ok()),
        };
        // The mobile notepad: first, and always on the web, as `notepad`.
        let notepad = project.notepad().map(|n| (n.id.clone(), n.title.clone()));
        if let Some((id, title)) = &notepad
            && let Some(text) = text_of(id)
        {
            index.push(json!({ "id": sync::NOTEPAD, "title": title, "notepad": true, "words": count_words(&text) }));
            scenes.insert(sync::NOTEPAD.to_owned(), text);
        }
        let labels = project.manifest.labels.clone();
        for (node, path) in project.web_documents() {
            if notepad.as_ref().is_some_and(|(id, _)| *id == node.id) {
                continue;
            }
            let Some(text) = text_of(&node.id) else { continue };
            let folder = path.last().map(|f| f.title.clone()).unwrap_or_default();
            // Its label, for a touch of its colour on the web.
            let label = node.meta.label.and_then(|i| labels.get(i)).map(|l| {
                let c = super::inspector::label_color(l.color);
                json!({ "name": l.name, "color": format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b()) })
            });
            index.push(json!({
                "id": node.id,
                "title": node.title,
                // The folder it's in (what older web pages group by), and
                // the whole way down to it, from its top-level folder.
                "chapter": folder,
                "path": path.iter().map(|f| json!({ "id": f.id, "title": f.title })).collect::<Vec<_>>(),
                "label": label,
                "synopsis": node.meta.synopsis,
                "status": node.meta.status.and_then(|i| statuses.get(i)),
                "target": node.meta.target_words,
                "words": count_words(&text),
            }));
            scenes.insert(node.id.clone(), text);
            // Its notes, as a scene of their own (see sync::notes_id).
            scenes.insert(sync::notes_id(&node.id), node.meta.notes.clone());
        }
        // A whole project: every document, and the binder (see sync.rs).
        let binder = project.manifest.whole_sync.then(|| {
            for node in project.documents() {
                if !scenes.contains_key(&node.id)
                    && notepad.as_ref().is_none_or(|(id, _)| *id != node.id)
                    && let Some(text) = text_of(&node.id)
                {
                    scenes.insert(node.id.clone(), text);
                }
            }
            // This computer's own tally of words, for the others to add up.
            let tally = self.stats.tally(crate::lock::host(), chrono::Utc::now().timestamp());
            scenes.insert(format!("{}{}", sync::STATS_PREFIX, self.settings.device_id), tally.to_text());
            sync::SharedBinder { from: crate::lock::host().to_owned(), manifest: project.manifest.clone() }
        });
        let input = Input {
            project: id,
            title: project.title().to_owned(),
            index,
            scenes,
            state: SyncState::load(&project.root, &self.settings.device_id),
            stats,
            // The Canvas as it is (the one open, or as saved), for a whole project.
            canvas: binder.as_ref().map(|_| self.canvas.as_ref().map_or_else(|| crate::canvas::Canvas::load(&project.root), |c| c.data().clone())),
            binder,
            today: crate::stats::key(crate::stats::today()),
        };
        let (url, token) = (self.settings.helper_url.clone(), self.helper_token.clone());
        let (send, result) = mpsc::channel();
        std::thread::spawn(move || {
            let mut helper = Http::new(&url, &token);
            let _ = send.send(sync::run(&mut helper, input));
        });
        self.sync_running = Some(Running { number, root: project.root.clone(), result });
    }

    /// Put a sync's results into the project: the web editor's edits, snapshots of
    /// the web editor's side of clashes, and new scenes from its inbox.
    pub(super) fn finish_sync(&mut self, root: &Path, result: Result<Outcome, SyncError>, now: f64) {
        // Another project was opened meanwhile: this one syncs next time.
        if self.project.as_ref().map(|p| p.root.as_path()) != Some(root) {
            return;
        }
        let mut outcome = match result {
            Ok(outcome) => outcome,
            Err(e) => {
                self.sync_message = format!("Not synced: {e}");
                if e == SyncError::Unauthorized {
                    self.set_status("The web helper didn't accept the token: check it in Settings".to_owned(), now);
                }
                return;
            }
        };
        let mut clashed = Vec::new();
        let mut new_scenes = 0;
        let mut binder_news: Vec<(String, usize, usize, usize)> = Vec::new();
        // The notepad is `notepad` on the helper, whichever document it is here.
        let notepad = self.project.as_ref().and_then(|p| p.notepad()).map(|n| n.id.clone());
        let local = |id: String| match &notepad {
            Some(local) if id == sync::NOTEPAD => local.clone(),
            _ => id,
        };
        for action in std::mem::take(&mut outcome.actions) {
            match action {
                Action::Replace { id, was, text } => self.take_ipad_text(root, &local(id), &was, &text, true),
                // Another computer's writing: in, but its words are its own.
                Action::FromComputer { id, was, text } => self.take_ipad_text(root, &local(id), &was, &text, false),
                Action::Canvas(canvas) => match self.canvas.as_mut() {
                    Some(open) => open.replace(*canvas),
                    None => {
                        if let Err(e) = canvas.save(root) {
                            self.error = Some(format!("Couldn't save the canvas from the other computer: {e}"));
                        }
                    }
                },
                Action::OtherStats { device, tally } => {
                    self.stats.set_other(&device, tally);
                    self.stats_dirty = true;
                }
                // Notes changed on both: the Mac's stay, with the web editor's added.
                Action::Snapshot { id, text, .. } if sync::notes_of(&id).is_some() => {
                    let scene = sync::notes_of(&id).expect("just checked").to_owned();
                    self.merge_ipad_notes(&scene, &text);
                }
                Action::Snapshot { id, text, name } => {
                    let id = local(id);
                    let _ = snapshots::take(root, &id, &text, &name);
                    clashed.push(self.title_of(&id));
                }
                Action::NewNotepad { text } => {
                    if let Some(project) = self.project.as_mut() {
                        match project.make_notepad(&text) {
                            Ok(_) => self.set_status("Mobile Notes, started on the web, are in Notes".to_owned(), now),
                            Err(e) => self.error = Some(format!("Couldn't make the mobile notepad: {e}")),
                        }
                    }
                }
                Action::Binder { manifest, new, gone, clashes, from } => {
                    // Gone there: closed here first (a snapshot's kept).
                    for id in &gone {
                        if let Some(tab) = self.open.remove(id) {
                            self.docs.close(tab);
                        }
                    }
                    let Some(project) = self.project.as_mut() else { continue };
                    match project.take_merged(*manifest, &new, &gone) {
                        Ok(changes) => self.follow(&changes),
                        Err(e) => self.error = Some(format!("Couldn't take in the binder from {from}: {e}")),
                    }
                    if self.selected.as_ref().is_some_and(|s| gone.contains(s)) {
                        self.selected = None;
                    }
                    binder_news.push((from, new.len(), gone.len(), clashes));
                }
                Action::NewScene { inbox_id, title, text } => {
                    if self.add_ipad_scene(&title, &text) {
                        outcome.state.imported.insert(inbox_id);
                        new_scenes += 1;
                    }
                }
            }
        }
        if let Err(e) = outcome.state.save(root, &self.settings.device_id) {
            self.error = Some(format!("Couldn't save the sync record: {e}"));
        }
        let clock = chrono::Local::now().format("%H:%M");
        self.sync_message = format!("Synced with the web helper at {clock}");
        // What came from another computer, said first.
        if let Some((from, new, gone, clashes)) = binder_news.pop() {
            let mut parts = Vec::new();
            if new > 0 {
                parts.push(format!("{new} new {}", if new == 1 { "document" } else { "documents" }));
            }
            if gone > 0 {
                parts.push(format!("{gone} removed (each kept as a snapshot)"));
            }
            if clashes > 0 {
                parts.push(format!("{clashes} changed on both, noted in {}", if clashes == 1 { "its Notes" } else { "their Notes" }));
            }
            let what = if parts.is_empty() { "the binder".to_owned() } else { parts.join(", ") };
            self.set_status(format!("From {from}: {what}"), now);
            return;
        }
        if let Some(first) = clashed.first() {
            let others = if clashed.len() > 1 { format!(" (and {} more)", clashed.len() - 1) } else { String::new() };
            self.set_status(
                format!("\u{201c}{first}\u{201d}{others} changed on both the Mac and the web editor: kept the Mac's, and the web editor's is in its snapshots"),
                now,
            );
        } else if outcome.received + new_scenes > 0 {
            let mut parts = Vec::new();
            if outcome.received > 0 {
                parts.push(format!("{} changed", outcome.received));
            }
            if new_scenes > 0 {
                parts.push(format!("{new_scenes} new, in the Manuscript"));
            }
            self.set_status(format!("From the web: {}", parts.join(", ")), now);
        }
    }

    fn title_of(&self, id: &str) -> String {
        self.project.as_ref().and_then(|p| p.node(id)).map(|n| n.title.clone()).unwrap_or_default()
    }

    /// An edit made on the web. If the scene changed here since the sync
    /// read it (typing meanwhile), the Mac's text stays and the web editor's is
    /// kept as a snapshot; the next sync sends the Mac's.
    /// (`from_web`: the web editor's, counted in today's words; else another
    /// computer's, which counts its own.)
    fn take_ipad_text(&mut self, root: &Path, id: &str, was: &str, text: &str, from_web: bool) {
        if let Some(scene) = sync::notes_of(id) {
            self.take_ipad_notes(scene, was, text);
            return;
        }
        if let Some(tab) = self.open.get(id).and_then(|&t| self.docs.get_mut(t)) {
            if tab.doc.text == was {
                tab.doc.text = text.to_owned();
                let _ = tab.doc.save();
                self.note_synced_words(id, text, from_web);
            } else {
                let _ = snapshots::take(root, id, text, "from the web (you were typing)");
            }
            return;
        }
        let Some(path) = self.project.as_ref().and_then(|p| p.file_path(id)) else { return };
        match crate::project::read_text(&path) {
            Ok(now) if now == was => {
                match sloth_core::document::write_atomically(&path, text.as_bytes()) {
                    Ok(()) => self.note_synced_words(id, text, from_web),
                    Err(e) => self.error = Some(format!("Couldn't save the web editor's edit: {e}")),
                }
            }
            _ => {
                let _ = snapshots::take(root, id, text, "from the web (changed here too)");
            }
        }
    }

    /// Notes edited on the web. If they changed here too meanwhile, both
    /// are kept (see `merge_ipad_notes`).
    fn take_ipad_notes(&mut self, scene: &str, was: &str, text: &str) {
        let Some(project) = self.project.as_mut() else { return };
        let Some(node) = project.node(scene) else { return };
        if node.meta.notes == was {
            let meta = Meta { notes: text.to_owned(), ..node.meta.clone() };
            if project.set_meta(scene, meta) {
                self.manifest_dirty = true;
            }
        } else {
            self.merge_ipad_notes(scene, text);
        }
    }

    /// Both sides changed a scene's notes: the web editor's go under the Mac's.
    fn merge_ipad_notes(&mut self, scene: &str, theirs: &str) {
        let Some(project) = self.project.as_mut() else { return };
        let Some(node) = project.node(scene) else { return };
        let mine = node.meta.notes.trim_end();
        if theirs.trim().is_empty() || mine.contains(theirs.trim()) {
            return;
        }
        let notes = if mine.is_empty() { theirs.to_owned() } else { format!("{mine}\n\nFrom the web:\n{theirs}") };
        if project.set_meta(scene, Meta { notes, ..node.meta.clone() }) {
            self.manifest_dirty = true;
        }
    }

    fn note_synced_words(&mut self, id: &str, text: &str, from_web: bool) {
        if from_web { self.note_web_words(id, text) } else { self.note_moved(id, text) }
    }

    /// A scene written on the web: into the Manuscript, and on the web.
    fn add_ipad_scene(&mut self, title: &str, text: &str) -> bool {
        let Some(project) = self.project.as_mut() else { return false };
        let Ok(id) = project.add_document(&Place::Into(Role::Manuscript.folder().into()), title) else { return false };
        let Some(path) = project.file_path(&id) else { return false };
        if sloth_core::document::write_atomically(&path, text.as_bytes()).is_err() {
            return false;
        }
        project.set_meta(&id, Meta { on_ipad: true, ..Meta::default() });
        self.manifest_dirty = true;
        self.note_words(&id, text);
        true
    }

    /// "Mobile notepad", in the inspector: this is the document jotted in on
    /// a phone (one per project).
    pub(super) fn notepad_checkbox(&mut self, ui: &mut egui::Ui, id: &str) {
        let Some(project) = self.project.as_mut() else { return };
        let mut on = project.notepad().is_some_and(|n| n.id == id);
        let hint = "For notes taken on your phone: on the web at the top of the project, with dated entries. One per project.";
        if ui.checkbox(&mut on, "Mobile notepad").on_hover_text(hint).changed() {
            project.set_notepad(on.then_some(id));
            self.manifest_dirty = true;
        }
        if on {
            ui.weak("On the web, at the top of the project.");
        }
    }

    /// Open the mobile notepad in the split pane, making one in Notes if
    /// there isn't one yet.
    pub(super) fn open_notepad(&mut self) {
        let Some(project) = self.project.as_mut() else { return };
        match project.make_notepad("") {
            Ok(id) => self.open_in_split(id),
            Err(e) => self.error = Some(format!("Couldn't make the mobile notepad: {e}")),
        }
    }

    /// The "Available on the web" checkbox, in the inspector.
    pub(super) fn ipad_checkbox(&self, ui: &mut egui::Ui, id: &str, meta: &mut Meta) {
        let Some(project) = &self.project else { return };
        ui.checkbox(&mut meta.on_ipad, "Available on the web");
        if !meta.on_ipad && project.on_ipad(id) {
            ui.weak("On the web with the folder it's in.");
        } else if meta.on_ipad && (self.settings.helper_url.trim().is_empty() || self.helper_token.trim().is_empty()) {
            ui.weak("Set up the web helper in Settings to sync.");
        }
    }
}
