//! Syncing the scenes marked "Available on the web" with the helper (helper/,
//! a small PHP service), where they can be edited in a browser.
//!
//! The Mac keeps, per project, what each scene was when last synced (its text
//! and the helper's version number) in `.sloth/sync.ron`. A sync then:
//!
//! 1. sends the list of marked scenes (the index the web editor shows);
//! 2. asks the helper what changed since the last sync;
//! 3. for each marked scene: changed only here → sent; changed only on the
//!    web → brought in; changed in both → the Mac's version is kept and
//!    sent, and the web editor's is kept as a snapshot of the scene, so nothing is
//!    lost; unchanged → left alone;
//! 4. collects new scenes written on the web (the inbox) into the binder.
//!
//! Every save to the helper names the version it was edited from, so if the
//! web editor saved in the meantime, the helper refuses and sends what it has
//! instead (see put_scene in helper/public/api.php).
//!
//! A scene's notes travel the same way, as a companion scene `<id>-notes`
//! (ids are letters and digits, so it can't be a real scene's), with the
//! same versions and clash rules; see `notes_of`.
//!
//! The mobile notepad (see `Project::notepad`) is the scene `notepad` on
//! the helper, whichever document it is here, so the web can start one
//! before the Mac has it. It's added to from both sides, so when both
//! changed, both sets of additions are kept (see `merge_additions`) rather
//! than one going to a snapshot. If the web started it and the Mac has
//! none, the Mac makes one (`Action::NewNotepad`).
//!
//! With the index goes the day's writing (words today, the day's goal, the
//! manuscript's length and target), for the web to show.
//!
//! **The whole project** (CROSS-PLATFORM.md, phase 4): with the project set
//! to sync whole, every document goes to the helper, not only the marked
//! ones (the web editor still lists just those), and so does the binder:
//! as the scene `_binder` (an id no document can have), the project file
//! with the name of the computer that sent it. When the helper's binder has
//! moved on since this computer last agreed with it, the two are merged
//! (merge.rs) and the result sent back; documents new from the other
//! computer come with their text, and ones gone there (and unchanged here)
//! go here too (`Action::Binder`). That makes the helper a hub: each
//! computer keeps its own copy and syncs everything through it.
//!
//! Each computer keeps its own sync record (`.sloth/sync-<device>.ron`), so
//! a project in a synced folder can't have two computers sharing one.
//!
//! The rules (`run`) work against `Remote`, so tests can use a stand-in;
//! `Http` is the real helper.

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Where a project's sync state lived before each computer had its own.
const OLD_STATE_FILE: &str = ".sloth/sync.ron";

/// Where this computer's sync state lives, inside the project folder.
fn state_file(device: &str) -> String {
    format!(".sloth/sync-{device}.ron")
}

/// The binder's id on the helper (ids of documents are letters and digits).
pub const BINDER: &str = "_binder";

/// The Canvas's id on the helper (see canvas.rs), for a project synced whole.
pub const CANVAS: &str = "_canvas";

/// Each computer's own tally of words, as `_stats-<device>` (see stats.rs).
pub const STATS_PREFIX: &str = "_stats-";

/// The binder as sent to the helper: the project file, and who sent it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SharedBinder {
    /// The computer's name (lock::host), for notes about clashes.
    pub from: String,
    pub manifest: crate::project::Manifest,
}

impl SharedBinder {
    pub fn to_text(&self) -> String {
        ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default()).unwrap_or_default()
    }

    pub fn from_text(text: &str) -> Option<Self> {
        ron::from_str(text).ok()
    }
}

/// What this Mac last synced, per project.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SyncState {
    /// The helper's change counter as of the last sync.
    pub seq: i64,
    /// Each scene as last synced.
    pub scenes: BTreeMap<String, Synced>,
    /// Inbox items already taken into the binder, to be marked collected on
    /// the helper at the next sync (and never taken twice meanwhile).
    pub imported: BTreeSet<i64>,
    /// The binder as this computer and the helper last agreed (whole
    /// projects only): the base for merging.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binder: Option<Synced>,
    /// The Canvas, likewise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canvas: Option<Synced>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Synced {
    pub version: u32,
    pub text: String,
}

impl SyncState {
    /// This computer's record. The first computer to sync after records
    /// became per computer takes over the old shared one.
    pub fn load(root: &Path, device: &str) -> Self {
        let read = |file: &str| std::fs::read_to_string(root.join(file)).ok().and_then(|text| ron::from_str(&text).ok());
        if let Some(state) = read(&state_file(device)) {
            return state;
        }
        match read(OLD_STATE_FILE) {
            Some(state) => {
                let _ = std::fs::remove_file(root.join(OLD_STATE_FILE));
                state
            }
            None => SyncState::default(),
        }
    }

    pub fn save(&self, root: &Path, device: &str) -> io::Result<()> {
        let path = root.join(state_file(device));
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default()).map_err(io::Error::other)?;
        sloth_core::document::write_atomically(&path, text.as_bytes())
    }
}

/// Everything a sync needs, gathered on the main thread.
pub struct Input {
    /// The project's sync id (its id on the helper).
    pub project: String,
    pub title: String,
    /// The marked scenes as the web editor lists them: id, title, chapter…
    pub index: Vec<Value>,
    /// Each marked scene's text as it is now, by id.
    pub scenes: BTreeMap<String, String>,
    pub state: SyncState,
    /// The day's writing, for the web: {date, today, goal, total, target, deadline}.
    pub stats: Value,
    /// A whole project: the binder as it is here (and `scenes` has every
    /// document, not only the marked ones).
    pub binder: Option<SharedBinder>,
    /// The day, for notes about clashes ("2026-11-25").
    pub today: String,
    /// A whole project's Canvas as it is here.
    pub canvas: Option<crate::canvas::Canvas>,
}

/// The mobile notepad's id on the helper. Ids here are six letters and
/// digits, so it can't be a real scene's.
pub const NOTEPAD: &str = "notepad";

/// The companion "scene" holding a scene's notes on the helper.
pub fn notes_id(scene: &str) -> String {
    format!("{scene}-notes")
}

/// If `id` is a scene's notes, that scene's id.
pub fn notes_of(id: &str) -> Option<&str> {
    id.strip_suffix("-notes")
}

/// What the main thread should do with a sync's results.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// The web editor changed this scene: put `text` in, if the scene still reads
    /// `was` (otherwise keep both: see the app).
    Replace { id: String, was: String, text: String },
    /// Keep this text as a snapshot of the scene: the web editor's side of a clash.
    Snapshot { id: String, text: String, name: String },
    /// A new scene written on the web.
    NewScene { inbox_id: i64, title: String, text: String },
    /// The notepad was started on the web, and the Mac has none yet.
    NewNotepad { text: String },
    /// Another computer changed this document (a whole project): put `text`
    /// in, as `Replace` does, but it's that computer's writing, not counted
    /// in today's words here (it counts its own).
    FromComputer { id: String, was: String, text: String },
    /// The Canvas, merged with another computer's: take it.
    Canvas(Box<crate::canvas::Canvas>),
    /// Another computer's own tally of words (see stats.rs), by its device id.
    OtherStats { device: String, tally: crate::stats::Tally },
    /// A whole project's binder, merged with another computer's: take it,
    /// write the documents new from there (`new`, by id), and remove the
    /// ones gone there (`gone`; keep a snapshot of each first).
    Binder { manifest: Box<crate::project::Manifest>, new: BTreeMap<String, String>, gone: Vec<String>, clashes: usize, from: String },
}

#[derive(Debug, Default)]
pub struct Outcome {
    pub state: SyncState,
    pub actions: Vec<Action>,
    pub sent: usize,
    pub received: usize,
    pub clashes: usize,
}

#[derive(Debug, PartialEq)]
pub enum SyncError {
    /// No connection, or the helper couldn't be reached.
    Network(String),
    /// The token was refused.
    Unauthorized,
    /// The helper answered with an error.
    Helper(u16, String),
}

impl std::fmt::Display for SyncError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncError::Network(e) => write!(f, "couldn't reach the helper ({e})"),
            SyncError::Unauthorized => write!(f, "the helper didn't accept the token"),
            SyncError::Helper(status, e) => write!(f, "the helper said: {e} ({status})"),
        }
    }
}

/// The result of saving a scene on the helper.
#[derive(Debug, PartialEq)]
pub enum Put {
    Saved { version: u32 },
    /// It moved on since `base`: what it holds now (if anything).
    Clash { current: Option<(String, u32)> },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    pub id: String,
    pub text: String,
    pub version: u32,
    /// Saved in the web editor (rather than by a copy of Writer).
    pub from_web: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct InboxItem {
    pub id: i64,
    pub title: String,
    pub text: String,
}

/// The helper, as a sync sees it.
pub trait Remote {
    fn put_index(&mut self, project: &str, title: &str, index: &[Value], stats: &Value) -> Result<(), SyncError>;
    /// Scenes saved since `since`, and the counter now.
    fn changes(&mut self, project: &str, since: i64) -> Result<(Vec<Change>, i64), SyncError>;
    fn put_scene(&mut self, project: &str, id: &str, text: &str, base: u32) -> Result<Put, SyncError>;
    fn inbox(&mut self, project: &str) -> Result<Vec<InboxItem>, SyncError>;
    fn collect(&mut self, project: &str, ids: &[i64]) -> Result<(), SyncError>;
}

/// Sync once. See the module's description for the rules.
pub fn run(remote: &mut dyn Remote, input: Input) -> Result<Outcome, SyncError> {
    let Input { project, title, index, scenes, state, stats, binder, today, canvas } = input;
    let mut out = Outcome { state, ..Outcome::default() };

    remote.put_index(&project, &title, &index, &stats)?;
    // Inbox items taken in last time can now be marked collected.
    if !out.state.imported.is_empty() {
        let ids: Vec<i64> = out.state.imported.iter().copied().collect();
        remote.collect(&project, &ids)?;
        out.state.imported.clear();
    }

    let (changes, seq) = remote.changes(&project, out.state.seq)?;
    let theirs: BTreeMap<&str, &Change> = changes.iter().map(|c| (c.id.as_str(), c)).collect();

    for (id, mine) in &scenes {
        let synced = out.state.scenes.get(id).cloned();
        let theirs = theirs.get(id.as_str()).copied();
        match synced {
            // Never synced from this Mac.
            None => match theirs {
                Some(t) if t.text == *mine => {
                    out.state.scenes.insert(id.clone(), Synced { version: t.version, text: t.text.clone() });
                }
                Some(t) => both_changed(remote, &project, &mut out, id, "", mine, t)?,
                None => send(remote, &project, &mut out, id, mine, 0)?,
            },
            Some(last) => {
                let mine_changed = *mine != last.text;
                match theirs.filter(|t| t.version > last.version) {
                    None if mine_changed => send(remote, &project, &mut out, id, mine, last.version)?,
                    None => {}
                    Some(t) if t.text == *mine => {
                        out.state.scenes.insert(id.clone(), Synced { version: t.version, text: t.text.clone() });
                    }
                    Some(t) if !mine_changed => {
                        out.actions.push(if t.from_web {
                            Action::Replace { id: id.clone(), was: mine.clone(), text: t.text.clone() }
                        } else {
                            Action::FromComputer { id: id.clone(), was: mine.clone(), text: t.text.clone() }
                        });
                        out.state.scenes.insert(id.clone(), Synced { version: t.version, text: t.text.clone() });
                        out.received += 1;
                    }
                    Some(t) => both_changed(remote, &project, &mut out, id, &last.text, mine, t)?,
                }
            }
        }
    }

    // Notes started on the web, with no notepad here yet. (Not recorded as
    // synced: once the Mac has it, the next sync finds them the same.)
    if !scenes.contains_key(NOTEPAD)
        && let Some(t) = theirs.get(NOTEPAD)
        && !t.text.trim().is_empty()
    {
        out.actions.push(Action::NewNotepad { text: t.text.clone() });
    }

    for item in remote.inbox(&project)? {
        if !out.state.imported.contains(&item.id) {
            out.actions.push(Action::NewScene { inbox_id: item.id, title: item.title, text: item.text });
        }
    }
    if let Some(ours) = binder {
        sync_binder(remote, &project, &mut out, &scenes, &theirs, ours, &today)?;
    }
    if let Some(ours) = canvas {
        sync_canvas(remote, &project, &mut out, &theirs, ours)?;
    }
    // Other computers' tallies of words (this one's is among `scenes`).
    for (id, change) in &theirs {
        if let Some(device) = id.strip_prefix(STATS_PREFIX).filter(|_| !scenes.contains_key(*id))
            && let Some(tally) = crate::stats::Tally::from_text(&change.text)
        {
            out.actions.push(Action::OtherStats { device: device.to_owned(), tally });
        }
    }
    out.state.seq = seq;
    Ok(out)
}

/// What `fetch` brings.
pub struct Fetched {
    pub binder: SharedBinder,
    /// Each document's text, by id.
    pub texts: BTreeMap<String, String>,
    /// The sync record to carry on from.
    pub state: SyncState,
    /// The other computers' tallies of words.
    pub tallies: BTreeMap<String, crate::stats::Tally>,
    pub canvas: Option<crate::canvas::Canvas>,
}

/// A whole project from the helper, for a computer that doesn't have it
/// yet: its binder, every document's text (by id), and the sync record to
/// carry on from. `None` if the project isn't synced whole.
pub fn fetch(remote: &mut dyn Remote, project: &str) -> Result<Option<Fetched>, SyncError> {
    let (changes, seq) = remote.changes(project, 0)?;
    let Some((binder, synced)) = changes.iter().find(|c| c.id == BINDER).and_then(|c| Some((SharedBinder::from_text(&c.text)?, c))) else {
        return Ok(None);
    };
    let docs = documents(&binder.manifest);
    let mut texts: BTreeMap<String, String> = changes.iter().filter(|c| docs.contains(&c.id)).map(|c| (c.id.clone(), c.text.clone())).collect();
    // The notepad is `notepad` there, whichever document it is.
    if let (Some(id), Some(c)) = (&binder.manifest.notepad, changes.iter().find(|c| c.id == NOTEPAD)) {
        texts.insert(id.clone(), c.text.clone());
    }
    let state = SyncState {
        seq,
        scenes: changes.iter().filter(|c| c.id != BINDER).map(|c| (c.id.clone(), Synced { version: c.version, text: c.text.clone() })).collect(),
        imported: BTreeSet::new(),
        binder: Some(Synced { version: synced.version, text: synced.text.clone() }),
        canvas: changes.iter().find(|c| c.id == CANVAS).map(|c| Synced { version: c.version, text: c.text.clone() }),
    };
    let tallies = changes
        .iter()
        .filter_map(|c| Some((c.id.strip_prefix(STATS_PREFIX)?.to_owned(), crate::stats::Tally::from_text(&c.text)?)))
        .collect();
    let canvas = changes.iter().find(|c| c.id == CANVAS).and_then(|c| ron::from_str(&c.text).ok());
    Ok(Some(Fetched { binder, texts, state, tallies, canvas }))
}

/// A whole project's binder: send it if only this side changed; merge if
/// the helper's moved on; then send the result. See the module's notes.
#[allow(clippy::too_many_arguments)]
fn sync_binder(
    remote: &mut dyn Remote,
    project: &str,
    out: &mut Outcome,
    scenes: &BTreeMap<String, String>,
    theirs: &BTreeMap<&str, &Change>,
    ours: SharedBinder,
    today: &str,
) -> Result<(), SyncError> {
    let last = out.state.binder.clone();
    let base = last.as_ref().and_then(|s| SharedBinder::from_text(&s.text));
    let newer = theirs.get(BINDER).filter(|t| last.as_ref().is_none_or(|l| t.version > l.version)).copied();
    let (send, base_version) = match newer.and_then(|t| Some((t, SharedBinder::from_text(&t.text)?))) {
        Some((change, there)) => {
            // Documents changed since the last sync, on each side.
            let edited_here: BTreeSet<String> = scenes
                .iter()
                .filter(|(id, text)| out.state.scenes.get(*id).is_none_or(|s| s.text != **text))
                .map(|(id, _)| id.clone())
                .collect();
            let edited_there: BTreeSet<String> = theirs.keys().filter(|id| !id.starts_with('_')).map(|id| (*id).to_owned()).collect();
            // Never agreed before: everything on both sides is new to the other.
            let empty = crate::project::Manifest { binder: Vec::new(), ..ours.manifest.clone() };
            let base_manifest = base.as_ref().map_or(&empty, |b| &b.manifest);
            let merged = crate::merge::merge(base_manifest, &ours.manifest, &there.manifest, &edited_here, &edited_there, &there.from, today);
            let mine: BTreeSet<String> = documents(&ours.manifest);
            let all: BTreeSet<String> = documents(&merged.manifest);
            // New from there, with their text (and recorded as synced).
            let mut new = BTreeMap::new();
            for id in all.difference(&mine) {
                let text = theirs.get(id.as_str()).map_or(String::new(), |c| c.text.clone());
                if let Some(c) = theirs.get(id.as_str()) {
                    out.state.scenes.insert(id.clone(), Synced { version: c.version, text: c.text.clone() });
                }
                new.insert(id.clone(), text);
            }
            let gone: Vec<String> = mine.difference(&all).cloned().collect();
            for id in &gone {
                out.state.scenes.remove(id);
            }
            out.received += new.len();
            out.clashes += merged.clashes.len();
            let manifest = merged.manifest.clone();
            out.actions.push(Action::Binder { manifest: Box::new(merged.manifest), new, gone, clashes: merged.clashes.len(), from: there.from });
            (SharedBinder { from: ours.from.clone(), manifest }, change.version)
        }
        None => {
            // Nothing new there: send ours if it changed since.
            if base.as_ref().is_some_and(|b| b.manifest == ours.manifest) {
                return Ok(());
            }
            let version = last.as_ref().map_or_else(|| theirs.get(BINDER).map_or(0, |t| t.version), |l| l.version);
            (ours, version)
        }
    };
    let text = send.to_text();
    match remote.put_scene(project, BINDER, &text, base_version)? {
        Put::Saved { version } => out.state.binder = Some(Synced { version, text }),
        // Changed again meanwhile: the next sync merges that too.
        Put::Clash { .. } => {}
    }
    Ok(())
}

/// A whole project's Canvas: sent if only this side changed; merged with
/// the helper's if that's moved on (canvas::merge), and the result sent.
fn sync_canvas(remote: &mut dyn Remote, project: &str, out: &mut Outcome, theirs: &BTreeMap<&str, &Change>, ours: crate::canvas::Canvas) -> Result<(), SyncError> {
    use crate::canvas::Canvas;
    let parse = |text: &str| ron::from_str::<Canvas>(text).ok();
    let last = out.state.canvas.clone();
    let base = last.as_ref().and_then(|s| parse(&s.text));
    let newer = theirs.get(CANVAS).filter(|t| last.as_ref().is_none_or(|l| t.version > l.version)).copied();
    let (send, version) = match newer.and_then(|t| Some((t, parse(&t.text)?))) {
        Some((change, there)) => {
            let merged = crate::canvas::merge(&base.unwrap_or_default(), &ours, &there);
            if merged != ours {
                out.actions.push(Action::Canvas(Box::new(merged.clone())));
            }
            (merged, change.version)
        }
        None => {
            // Only the view changed, or nothing: nothing to send.
            let shared = |c: &Canvas| (c.places.clone(), c.notes.clone(), c.strings.clone());
            if base.as_ref().is_some_and(|b| shared(b) == shared(&ours)) {
                return Ok(());
            }
            let version = last.as_ref().map_or_else(|| theirs.get(CANVAS).map_or(0, |t| t.version), |l| l.version);
            (ours, version)
        }
    };
    let text = ron::to_string(&send).unwrap_or_default();
    if let Put::Saved { version } = remote.put_scene(project, CANVAS, &text, version)? {
        out.state.canvas = Some(Synced { version, text });
    }
    Ok(())
}

/// The documents in a binder, by id.
fn documents(manifest: &crate::project::Manifest) -> BTreeSet<String> {
    fn walk(nodes: &[crate::project::Node], out: &mut BTreeSet<String>) {
        for n in nodes {
            if n.path.is_some() {
                out.insert(n.id.clone());
            }
            walk(&n.children, out);
        }
    }
    let mut out = BTreeSet::new();
    walk(&manifest.binder, &mut out);
    out
}

/// Both sides changed a scene since `base`. The notepad keeps both sides'
/// additions; any other scene keeps the Mac's, with the web editor's as a
/// snapshot.
#[allow(clippy::too_many_arguments)]
fn both_changed(
    remote: &mut dyn Remote,
    project: &str,
    out: &mut Outcome,
    id: &str,
    base: &str,
    mine: &str,
    theirs: &Change,
) -> Result<(), SyncError> {
    let merged = if id == NOTEPAD { merge_additions(base, mine, &theirs.text) } else { None };
    match merged {
        Some(merged) => {
            if merged != mine {
                out.actions.push(Action::Replace { id: id.to_owned(), was: mine.to_owned(), text: merged.clone() });
                out.received += 1;
            }
            send(remote, project, out, id, &merged, theirs.version)
        }
        None => {
            clash(out, id, &theirs.text);
            send(remote, project, out, id, mine, theirs.version)
        }
    }
}

/// A notepad added to on both sides since `base`: the base, then the
/// Mac's additions, then the web's. `None` if either side changed more
/// than its end (then it's an ordinary clash).
pub fn merge_additions(base: &str, mine: &str, theirs: &str) -> Option<String> {
    let (m, t) = (mine.trim_end(), theirs.trim_end());
    if t.trim().is_empty() || m.contains(t.trim_start()) {
        return Some(mine.to_owned());
    }
    if m.trim().is_empty() || t.contains(m.trim_start()) {
        return Some(theirs.to_owned());
    }
    let base = base.trim_end();
    let (mine_added, theirs_added) = (m.strip_prefix(base)?, t.strip_prefix(base)?);
    let gap = if base.is_empty() { "" } else { "\n\n" };
    Some(format!("{base}{gap}{}\n\n{}\n", mine_added.trim_start(), theirs_added.trim_start()))
}

/// Both sides changed a scene: the web editor's text is kept as a snapshot.
fn clash(out: &mut Outcome, id: &str, theirs: &str) {
    out.actions.push(Action::Snapshot {
        id: id.to_owned(),
        text: theirs.to_owned(),
        name: "from the web (both changed)".to_owned(),
    });
    out.clashes += 1;
}

/// Send a scene, edited from version `base`. If the web editor saved it meanwhile,
/// keep that as a snapshot and send ours on top of it.
fn send(remote: &mut dyn Remote, project: &str, out: &mut Outcome, id: &str, text: &str, base: u32) -> Result<(), SyncError> {
    let version = match remote.put_scene(project, id, text, base)? {
        Put::Saved { version } => version,
        Put::Clash { current } => {
            let base = match current {
                Some((theirs, version)) => {
                    if theirs != text {
                        clash(out, id, &theirs);
                    }
                    version
                }
                None => 0,
            };
            match remote.put_scene(project, id, text, base)? {
                Put::Saved { version } => version,
                // Changing that fast: leave it for the next sync.
                Put::Clash { .. } => return Ok(()),
            }
        }
    };
    out.state.scenes.insert(id.to_owned(), Synced { version, text: text.to_owned() });
    out.sent += 1;
    Ok(())
}

// ----- the real helper ---------------------------------------------------------

/// The helper over HTTPS, with the Mac's token.
pub struct Http {
    base: String,
    token: String,
    agent: ureq::Agent,
}

impl Http {
    /// `base`: the helper's address, as shown in the browser
    /// (https://writer.example.com/ or …/writer/).
    pub fn new(base: &str, token: &str) -> Self {
        let mut base = base.trim().to_owned();
        if let Some(stripped) = base.strip_suffix("index.html") {
            base = stripped.to_owned();
        }
        if !base.ends_with('/') {
            base.push('/');
        }
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(30)))
            .build()
            .into();
        Self { base, token: token.trim().to_owned(), agent }
    }

    fn url(&self, action: &str, query: &str) -> String {
        format!("{}api.php?action={action}{query}", self.base)
    }

    /// Send a request and read its JSON answer: (status, body).
    fn call(&self, action: &str, query: &str, body: Option<Value>) -> Result<(u16, Value), SyncError> {
        let url = self.url(action, query);
        let auth = format!("Bearer {}", self.token);
        let response = match body {
            Some(body) => self.agent.post(&url).header("Authorization", &auth).send_json(body),
            None => self.agent.get(&url).header("Authorization", &auth).call(),
        };
        let mut response = response.map_err(|e| SyncError::Network(e.to_string()))?;
        let status = response.status().as_u16();
        let json: Value = response.body_mut().read_json().unwrap_or(Value::Null);
        match status {
            200 | 409 => Ok((status, json)),
            401 | 403 => Err(SyncError::Unauthorized),
            _ => Err(SyncError::Helper(status, json["error"].as_str().unwrap_or("unknown error").to_owned())),
        }
    }
}

/// A project on the helper, as its list shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct RemoteProject {
    pub id: String,
    pub title: String,
    pub documents: u64,
}

impl Http {
    /// The projects on the helper, most recently changed first.
    pub fn projects(&self) -> Result<Vec<RemoteProject>, SyncError> {
        let (_, body) = self.call("projects", "", None)?;
        Ok(body
            .as_array()
            .map(|list| {
                list.iter()
                    .filter_map(|p| {
                        Some(RemoteProject { id: p["id"].as_str()?.to_owned(), title: p["title"].as_str()?.to_owned(), documents: p["scenes"].as_u64().unwrap_or(0) })
                    })
                    .collect()
            })
            .unwrap_or_default())
    }
}

fn query(project: &str, id: Option<&str>) -> String {
    let encode = |s: &str| s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect::<String>();
    match id {
        Some(id) => format!("&p={}&id={}", encode(project), encode(id)),
        None => format!("&p={}", encode(project)),
    }
}

impl Remote for Http {
    fn put_index(&mut self, project: &str, title: &str, index: &[Value], stats: &Value) -> Result<(), SyncError> {
        let body = json!({ "title": title, "index": index, "stats": stats });
        self.call("index_put", &query(project, None), Some(body)).map(|_| ())
    }

    fn changes(&mut self, project: &str, since: i64) -> Result<(Vec<Change>, i64), SyncError> {
        let (_, body) = self.call("changes", &format!("{}&since={since}", query(project, None)), None)?;
        let scenes = body["scenes"].as_array().cloned().unwrap_or_default();
        let changes = scenes
            .iter()
            .filter_map(|s| {
                Some(Change {
                    id: s["id"].as_str()?.to_owned(),
                    text: s["text"].as_str()?.to_owned(),
                    version: s["version"].as_u64()? as u32,
                    from_web: s["by"].as_str() == Some("web"),
                })
            })
            .collect();
        Ok((changes, body["seq"].as_i64().unwrap_or(0)))
    }

    fn put_scene(&mut self, project: &str, id: &str, text: &str, base: u32) -> Result<Put, SyncError> {
        let (status, body) = self.call("scene_put", &query(project, Some(id)), Some(json!({ "text": text, "base": base })))?;
        if status == 409 {
            let current = body["current"]["text"]
                .as_str()
                .map(|t| (t.to_owned(), body["current"]["version"].as_u64().unwrap_or(0) as u32));
            return Ok(Put::Clash { current });
        }
        Ok(Put::Saved { version: body["version"].as_u64().unwrap_or(0) as u32 })
    }

    fn inbox(&mut self, project: &str) -> Result<Vec<InboxItem>, SyncError> {
        let (_, body) = self.call("inbox", &query(project, None), None)?;
        Ok(body
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|i| {
                        Some(InboxItem {
                            id: i["id"].as_i64()?,
                            title: i["title"].as_str()?.to_owned(),
                            text: i["text"].as_str()?.to_owned(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    fn collect(&mut self, project: &str, ids: &[i64]) -> Result<(), SyncError> {
        self.call("inbox_collect", &query(project, None), Some(json!({ "ids": ids }))).map(|_| ())
    }
}

#[cfg(test)]
pub mod fake {
    //! A stand-in helper with the real one's rules (versions, clashes, the
    //! change counter, the inbox), for testing syncs without a server.
    use super::*;

    #[derive(Default)]
    pub struct Fake {
        pub index: Vec<Value>,
        pub scenes: BTreeMap<String, (String, u32, i64)>, // text, version, seq
        pub seq: i64,
        pub inbox: Vec<(InboxItem, bool)>, // item, collected
        /// Scenes last saved in the web editor.
        pub by_web: BTreeSet<String>,
    }

    impl Fake {
        /// An edit made on the web, from the version it read.
        pub fn ipad_edit(&mut self, id: &str, text: &str) {
            let version = self.scenes.get(id).map_or(0, |s| s.1);
            assert_eq!(self.put_scene("p", id, text, version).unwrap(), Put::Saved { version: version + 1 });
            self.by_web.insert(id.to_owned());
        }

        pub fn text(&self, id: &str) -> Option<&str> {
            self.scenes.get(id).map(|s| s.0.as_str())
        }
    }

    impl Remote for Fake {
        fn put_index(&mut self, _: &str, _: &str, index: &[Value], _: &Value) -> Result<(), SyncError> {
            self.index = index.to_vec();
            Ok(())
        }

        fn changes(&mut self, _: &str, since: i64) -> Result<(Vec<Change>, i64), SyncError> {
            let mut changed: Vec<_> = self.scenes.iter().filter(|(_, s)| s.2 > since).collect();
            changed.sort_by_key(|(_, s)| s.2);
            let list = changed.into_iter().map(|(id, s)| Change { id: id.clone(), text: s.0.clone(), version: s.1, from_web: self.by_web.contains(id) }).collect();
            Ok((list, self.seq))
        }

        fn put_scene(&mut self, _: &str, id: &str, text: &str, base: u32) -> Result<Put, SyncError> {
            let current = self.scenes.get(id).map(|s| (s.0.clone(), s.1));
            if current.as_ref().map_or(0, |c| c.1) != base {
                return Ok(Put::Clash { current });
            }
            if current.as_ref().is_some_and(|c| c.0 == text) {
                return Ok(Put::Saved { version: base });
            }
            self.seq += 1;
            self.by_web.remove(id);
            self.scenes.insert(id.to_owned(), (text.to_owned(), base + 1, self.seq));
            Ok(Put::Saved { version: base + 1 })
        }

        fn inbox(&mut self, _: &str) -> Result<Vec<InboxItem>, SyncError> {
            Ok(self.inbox.iter().filter(|(_, collected)| !collected).map(|(i, _)| i.clone()).collect())
        }

        fn collect(&mut self, _: &str, ids: &[i64]) -> Result<(), SyncError> {
            for (item, collected) in &mut self.inbox {
                if ids.contains(&item.id) {
                    *collected = true;
                }
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::Fake;
    use super::*;

    fn input(scenes: &[(&str, &str)], state: SyncState) -> Input {
        Input {
            project: "p".into(),
            title: "Book".into(),
            index: scenes.iter().map(|(id, _)| json!({ "id": id })).collect(),
            scenes: scenes.iter().map(|(id, t)| (id.to_string(), t.to_string())).collect(),
            state,
            stats: Value::Null,
            binder: None,
            today: "2026-11-25".into(),
            canvas: None,
        }
    }

    // ----- a whole project between two computers --------------------------

    /// One computer's sync, as the app does it: every document, the binder,
    /// and the results put into the project.
    fn sync_computer(helper: &mut Fake, p: &mut crate::project::Project, state: &mut SyncState, host: &str) {
        let mut stats = crate::stats::Stats::default();
        sync_counting(helper, p, state, host, &mut stats);
    }

    /// The same, with this computer's statistics sent and the others' taken in.
    fn sync_counting(helper: &mut Fake, p: &mut crate::project::Project, state: &mut SyncState, host: &str, stats: &mut crate::stats::Stats) {
        let mut scenes: BTreeMap<String, String> = p
            .documents()
            .iter()
            .map(|n| (n.id.clone(), crate::project::read_text(&p.file_path(&n.id).unwrap()).unwrap_or_default()))
            .collect();
        scenes.insert(format!("{STATS_PREFIX}{host}"), stats.tally().to_text());
        let input = Input {
            project: "p".into(),
            title: "Book".into(),
            index: Vec::new(),
            scenes,
            state: state.clone(),
            stats: Value::Null,
            binder: Some(SharedBinder { from: host.into(), manifest: p.manifest.clone() }),
            today: "2026-11-25".into(),
            canvas: Some(crate::canvas::Canvas::load(&p.root)),
        };
        let out = run(helper, input).unwrap();
        for action in out.actions {
            match action {
                Action::Replace { id, text, .. } | Action::FromComputer { id, text, .. } => std::fs::write(p.file_path(&id).unwrap(), text).unwrap(),
                Action::OtherStats { device, tally } => stats.set_other(&device, tally),
                Action::Canvas(canvas) => canvas.save(&p.root).unwrap(),
                Action::Binder { manifest, new, gone, .. } => {
                    p.take_merged(*manifest, &new, &gone).unwrap();
                }
                Action::Snapshot { id, text, name } => {
                    crate::snapshots::take(&p.root, &id, &text, &name).unwrap();
                }
                _ => {}
            }
        }
        *state = out.state;
    }

    fn text(p: &crate::project::Project, title: &str) -> String {
        let node = p.documents().into_iter().find(|n| n.title == title).unwrap_or_else(|| panic!("no {title}"));
        crate::project::read_text(&p.file_path(&node.id).unwrap()).unwrap()
    }

    fn titles(p: &crate::project::Project) -> Vec<String> {
        let mut t: Vec<String> = p.documents().iter().map(|n| n.title.clone()).collect();
        t.sort();
        t
    }

    #[test]
    fn two_computers_share_a_whole_project_through_the_helper() {
        use crate::project::{Place, Project};
        let base = std::env::temp_dir().join(format!("sloth-writer-hub-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let mut helper = Fake::default();

        // The Mac starts it, and syncs it whole.
        let mut mac = Project::create(&base.join("mac"), "Book").unwrap();
        mac.manifest.whole_sync = true;
        let one = mac.add_document(&Place::Into("manuscript".into()), "One").unwrap();
        let two = mac.add_document(&Place::Into("manuscript".into()), "Two").unwrap();
        std::fs::write(mac.file_path(&one).unwrap(), "The first.").unwrap();
        std::fs::write(mac.file_path(&two).unwrap(), "The second.").unwrap();
        let mut mac_state = SyncState::default();
        sync_computer(&mut helper, &mut mac, &mut mac_state, "Mac");

        // The PC gets it from the helper.
        let Fetched { binder, texts, state: mut pc_state, .. } = fetch(&mut helper, "p").unwrap().expect("synced whole");
        let mut pc = Project::from_shared(&base.join("pc"), binder.manifest, &texts).unwrap();
        assert_eq!(titles(&pc), titles(&mac));
        assert_eq!(text(&pc, "Two"), "The second.");

        // Each changes something different.
        mac.rename(&one, "First").unwrap();
        std::fs::write(mac.file_path(&two).unwrap(), "The second, revised.").unwrap();
        let three = pc.add_document(&Place::Into("manuscript".into()), "Three").unwrap();
        std::fs::write(pc.file_path(&three).unwrap(), "Written on the PC.").unwrap();
        pc.node_mut_for_test(&one).meta.synopsis = "Mara lands.".into();
        pc.save().unwrap();

        // Sync round: both end up with everything.
        sync_computer(&mut helper, &mut mac, &mut mac_state, "Mac");
        sync_computer(&mut helper, &mut pc, &mut pc_state, "Writing PC");
        sync_computer(&mut helper, &mut mac, &mut mac_state, "Mac");
        for p in [&mac, &pc] {
            assert_eq!(titles(p), ["First", "Three", "Two", "Untitled"]);
            assert_eq!(text(p, "Two"), "The second, revised.");
            assert_eq!(text(p, "Three"), "Written on the PC.");
            assert_eq!(p.node(&one).unwrap().meta.synopsis, "Mara lands.");
        }

        // Trashed on the PC: in the Trash on the Mac too, its file with it.
        pc.trash(&two).unwrap();
        sync_computer(&mut helper, &mut pc, &mut pc_state, "Writing PC");
        sync_computer(&mut helper, &mut mac, &mut mac_state, "Mac");
        assert_eq!(mac.role_of(&two), Some(crate::project::Role::Trash));
        assert!(mac.file_path(&two).unwrap().starts_with(mac.root.join("trash")), "{:?}", mac.file_path(&two));
        assert_eq!(crate::project::read_text(&mac.file_path(&two).unwrap()).unwrap(), "The second, revised.");

        // The Canvas: a note written on the Mac, a card moved on the PC.
        let mut canvas = crate::canvas::Canvas::load(&mac.root);
        let note = canvas.add_note([40.0, 40.0], "Ferry timetable?");
        canvas.save(&mac.root).unwrap();
        let mut theirs = crate::canvas::Canvas::load(&pc.root);
        theirs.places.insert(three.clone(), [500.0, 120.0]);
        theirs.save(&pc.root).unwrap();
        sync_computer(&mut helper, &mut mac, &mut mac_state, "Mac");
        sync_computer(&mut helper, &mut pc, &mut pc_state, "Writing PC");
        sync_computer(&mut helper, &mut mac, &mut mac_state, "Mac");
        for p in [&mac, &pc] {
            let c = crate::canvas::Canvas::load(&p.root);
            assert_eq!(c.note(&note).map(|n| n.text.as_str()), Some("Ferry timetable?"));
            assert_eq!(c.places.get(&three), Some(&[500.0, 120.0]));
        }

        // Both change the same document: this side's kept, the other's in a snapshot.
        std::fs::write(mac.file_path(&three).unwrap(), "The Mac's version.").unwrap();
        std::fs::write(pc.file_path(&three).unwrap(), "The PC's version.").unwrap();
        sync_computer(&mut helper, &mut mac, &mut mac_state, "Mac");
        sync_computer(&mut helper, &mut pc, &mut pc_state, "Writing PC");
        assert_eq!(text(&pc, "Three"), "The PC's version.");
        let kept: Vec<String> = crate::snapshots::list(&pc.root, &three).iter().map(|s| s.text().unwrap()).collect();
        assert!(kept.contains(&"The Mac's version.".to_owned()), "{kept:?}");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn each_computers_words_are_added_up_once() {
        use crate::project::Project;
        let base = std::env::temp_dir().join(format!("sloth-writer-tallies-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let mut helper = Fake::default();
        let mut mac = Project::create(&base.join("mac"), "Book").unwrap();
        mac.manifest.whole_sync = true;
        let today = crate::stats::today();
        let (mut mac_stats, mut pc_stats) = (crate::stats::Stats::default(), crate::stats::Stats::default());
        // 300 typed on the Mac, 200 on the PC, and the same 50 from the web
        // editor reaching both.
        mac_stats.add(today, 300);
        mac_stats.add_web(today, 50);
        pc_stats.add(today, 200);
        pc_stats.add_web(today, 50);
        let mut mac_state = SyncState::default();
        sync_counting(&mut helper, &mut mac, &mut mac_state, "mac", &mut mac_stats);
        let Fetched { binder, texts, state: mut pc_state, tallies, .. } = fetch(&mut helper, "p").unwrap().unwrap();
        let mut pc = Project::from_shared(&base.join("pc"), binder.manifest, &texts).unwrap();
        for (device, tally) in tallies {
            pc_stats.set_other(&device, tally);
        }
        sync_counting(&mut helper, &mut pc, &mut pc_state, "pc", &mut pc_stats);
        sync_counting(&mut helper, &mut mac, &mut mac_state, "mac", &mut mac_stats);
        assert_eq!(pc_stats.written(today), 550, "300 + 200, and the web's 50 once");
        assert_eq!(mac_stats.written(today), 550);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn first_sync_sends_everything() {
        let mut helper = Fake::default();
        let out = run(&mut helper, input(&[("a", "one"), ("b", "two")], SyncState::default())).unwrap();
        assert_eq!((out.sent, out.received, out.clashes), (2, 0, 0));
        assert_eq!(helper.text("a"), Some("one"));
        assert_eq!(helper.index.len(), 2);
        assert_eq!(out.state.scenes["b"], Synced { version: 1, text: "two".into() });
        assert!(out.actions.is_empty());
    }

    #[test]
    fn edits_flow_both_ways() {
        let mut helper = Fake::default();
        let state = run(&mut helper, input(&[("a", "one"), ("b", "two")], SyncState::default())).unwrap().state;

        // The Mac edits a; the web editor edits b.
        helper.ipad_edit("b", "two, from the web");
        let out = run(&mut helper, input(&[("a", "one, from the Mac"), ("b", "two")], state)).unwrap();
        assert_eq!((out.sent, out.received, out.clashes), (1, 1, 0));
        assert_eq!(helper.text("a"), Some("one, from the Mac"));
        assert_eq!(
            out.actions,
            [Action::Replace { id: "b".into(), was: "two".into(), text: "two, from the web".into() }]
        );

        // Next time, with b brought in: nothing to do.
        let out = run(&mut helper, input(&[("a", "one, from the Mac"), ("b", "two, from the web")], out.state)).unwrap();
        assert_eq!((out.sent, out.received, out.clashes), (0, 0, 0));
        assert!(out.actions.is_empty());
    }

    #[test]
    fn both_changed_keeps_the_macs_and_snapshots_the_ipads() {
        let mut helper = Fake::default();
        let state = run(&mut helper, input(&[("a", "start")], SyncState::default())).unwrap().state;
        helper.ipad_edit("a", "the web editor's words");
        let out = run(&mut helper, input(&[("a", "the Mac's words")], state)).unwrap();
        assert_eq!(out.clashes, 1);
        assert_eq!(helper.text("a"), Some("the Mac's words"));
        assert!(matches!(&out.actions[..], [Action::Snapshot { text, .. }] if text == "the web editor's words"));
        assert_eq!(out.state.scenes["a"].version, 3);

        // The same edit on both sides isn't a clash.
        helper.ipad_edit("a", "same");
        let out = run(&mut helper, input(&[("a", "same")], out.state)).unwrap();
        assert_eq!((out.sent, out.clashes), (0, 0));
        assert!(out.actions.is_empty());
    }

    #[test]
    fn an_ipad_save_between_reading_and_sending_is_kept() {
        // The Mac thinks it's at version 1, but the helper is at 2 and the
        // change feed missed it (it happened after the feed was read).
        let mut helper = Fake::default();
        let mut state = run(&mut helper, input(&[("a", "start")], SyncState::default())).unwrap().state;
        helper.ipad_edit("a", "sneaky");
        state.seq = helper.seq;
        let out = run(&mut helper, input(&[("a", "mine")], state)).unwrap();
        assert_eq!(helper.text("a"), Some("mine"));
        assert!(matches!(&out.actions[..], [Action::Snapshot { text, .. }] if text == "sneaky"));
    }

    #[test]
    fn a_scene_already_on_the_helper_is_adopted_or_kept() {
        // Sync state lost (a new Mac, say): matching text is just adopted.
        let mut helper = Fake::default();
        helper.ipad_edit("a", "same");
        helper.ipad_edit("b", "theirs");
        let out = run(&mut helper, input(&[("a", "same"), ("b", "mine")], SyncState::default())).unwrap();
        assert_eq!(out.state.scenes["a"].version, 1);
        assert_eq!(helper.text("b"), Some("mine"));
        assert_eq!(out.clashes, 1);
    }

    #[test]
    fn inbox_scenes_are_taken_once_then_collected() {
        let mut helper = Fake::default();
        helper.inbox.push((InboxItem { id: 7, title: "On the train".into(), text: "An idea.".into() }, false));
        let out = run(&mut helper, input(&[], SyncState::default())).unwrap();
        assert_eq!(out.actions, [Action::NewScene { inbox_id: 7, title: "On the train".into(), text: "An idea.".into() }]);

        // The app records it as imported; the next sync collects it.
        let mut state = out.state;
        state.imported.insert(7);
        let out = run(&mut helper, input(&[], state)).unwrap();
        assert!(out.actions.is_empty());
        assert!(helper.inbox[0].1, "collected");
        assert!(out.state.imported.is_empty());
    }

    #[test]
    fn notepad_additions_from_both_sides_are_kept() {
        let mut helper = Fake::default();
        let start = "## Monday\nThe harbour at dawn.";
        let state = run(&mut helper, input(&[(NOTEPAD, start)], SyncState::default())).unwrap().state;
        helper.ipad_edit(NOTEPAD, &format!("{start}\n\n## Tuesday\nOn the train: the letter."));
        let mine = format!("{start}\n\nAsk Tom about the ferry.");
        let out = run(&mut helper, input(&[(NOTEPAD, &mine)], state)).unwrap();
        let both = format!("{start}\n\nAsk Tom about the ferry.\n\n## Tuesday\nOn the train: the letter.\n");
        assert_eq!(out.clashes, 0, "no snapshot");
        assert_eq!(out.actions, [Action::Replace { id: NOTEPAD.into(), was: mine.clone(), text: both.clone() }]);
        assert_eq!(helper.text(NOTEPAD), Some(both.as_str()));

        // Next time, with that brought in: nothing to do.
        let out = run(&mut helper, input(&[(NOTEPAD, &both)], out.state)).unwrap();
        assert!(out.actions.is_empty() && out.sent == 0);
    }

    #[test]
    fn merging_additions() {
        assert_eq!(merge_additions("a", "a\nb", "a\nc").as_deref(), Some("a\n\nb\n\nc\n"));
        assert_eq!(merge_additions("", "b", "c").as_deref(), Some("b\n\nc\n"), "both new");
        assert_eq!(merge_additions("a", "a b", "a").as_deref(), Some("a b"), "only one side added");
        assert_eq!(merge_additions("a", "x", "a c"), None, "the Mac's changed more than its end");
    }

    #[test]
    fn a_notepad_started_on_the_web_comes_to_the_mac() {
        let mut helper = Fake::default();
        helper.ipad_edit(NOTEPAD, "Idea on the bus.");
        let out = run(&mut helper, input(&[("a", "one")], SyncState::default())).unwrap();
        assert!(out.actions.contains(&Action::NewNotepad { text: "Idea on the bus.".into() }));

        // An empty one is left alone, and other scenes' rules are unchanged.
        let mut helper = Fake::default();
        helper.ipad_edit(NOTEPAD, "  ");
        let out = run(&mut helper, input(&[], SyncState::default())).unwrap();
        assert!(out.actions.is_empty());
    }

    #[test]
    fn sync_state_round_trips() {
        let dir = std::env::temp_dir().join(format!("sloth-writer-syncstate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut state = SyncState { seq: 4, ..Default::default() };
        state.scenes.insert("a".into(), Synced { version: 2, text: "words".into() });
        state.save(&dir, "mac").unwrap();
        assert_eq!(SyncState::load(&dir, "mac"), state);
        assert_eq!(SyncState::load(&dir.join("missing"), "mac"), SyncState::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Against the real helper (helper/tests/run.sh sets HELPER_URL and a
    /// token): run with `cargo test -p sloth-writer sync_with_the_real_helper -- --ignored`.
    #[test]
    #[ignore]
    fn sync_with_the_real_helper() {
        let url = std::env::var("HELPER_URL").expect("HELPER_URL");
        let token = std::env::var("HELPER_TOKEN").expect("HELPER_TOKEN");
        let mut helper = Http::new(&url, &token);
        let project = format!("rust{}", std::process::id());
        let i = |scenes: &[(&str, &str)], state| Input { project: project.clone(), ..input(scenes, state) };

        let out = run(&mut helper, i(&[("a", "one"), ("b", "two")], SyncState::default())).unwrap();
        assert_eq!(out.sent, 2);
        // An edit "on the web" through the API, as the web editor would make it.
        assert_eq!(helper.put_scene(&project, "b", "two, from the web", 1).unwrap(), Put::Saved { version: 2 });
        let out = run(&mut helper, i(&[("a", "one!"), ("b", "two")], out.state)).unwrap();
        assert_eq!((out.sent, out.received, out.clashes), (1, 1, 0));
        // And a clash.
        assert_eq!(helper.put_scene(&project, "a", "web", 2).unwrap(), Put::Saved { version: 3 });
        let out = run(&mut helper, i(&[("a", "Mac"), ("b", "two, from the web")], out.state)).unwrap();
        assert_eq!(out.clashes, 1);
        assert_eq!(helper.put_scene(&project, "a", "stale", 1).unwrap(), Put::Clash { current: Some(("Mac".into(), 4)) });

        let bad = run(&mut Http::new(&url, "not-a-token"), i(&[], SyncState::default()));
        assert!(matches!(bad, Err(SyncError::Unauthorized)));
    }
}
