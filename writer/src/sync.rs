//! Syncing the scenes marked "Available on iPad" with the helper (helper/,
//! a small PHP service), where they can be edited in Safari.
//!
//! The Mac keeps, per project, what each scene was when last synced (its text
//! and the helper's version number) in `.sloth/sync.ron`. A sync then:
//!
//! 1. sends the list of marked scenes (the index the iPad shows);
//! 2. asks the helper what changed since the last sync;
//! 3. for each marked scene: changed only here → sent; changed only on the
//!    iPad → brought in; changed in both → the Mac's version is kept and
//!    sent, and the iPad's is kept as a snapshot of the scene, so nothing is
//!    lost; unchanged → left alone;
//! 4. collects new scenes written on the iPad (the inbox) into the binder.
//!
//! Every save to the helper names the version it was edited from, so if the
//! iPad saved in the meantime, the helper refuses and sends what it has
//! instead (see put_scene in helper/public/api.php).
//!
//! A scene's notes travel the same way, as a companion scene `<id>-notes`
//! (ids are letters and digits, so it can't be a real scene's), with the
//! same versions and clash rules; see `notes_of`.
//!
//! With the index goes the day's writing (words today, the day's goal, the
//! manuscript's length and target), for the iPad to show.
//!
//! The rules (`run`) work against `Remote`, so tests can use a stand-in;
//! `Http` is the real helper.

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Where a project's sync state lives, inside the project folder.
const STATE_FILE: &str = ".sloth/sync.ron";

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
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Synced {
    pub version: u32,
    pub text: String,
}

impl SyncState {
    pub fn load(root: &Path) -> Self {
        std::fs::read_to_string(root.join(STATE_FILE))
            .ok()
            .and_then(|text| ron::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, root: &Path) -> io::Result<()> {
        let path = root.join(STATE_FILE);
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
    /// The marked scenes as the iPad lists them: id, title, chapter…
    pub index: Vec<Value>,
    /// Each marked scene's text as it is now, by id.
    pub scenes: BTreeMap<String, String>,
    pub state: SyncState,
    /// The day's writing, for the iPad: {date, today, goal, total, target, deadline}.
    pub stats: Value,
}

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
    /// The iPad changed this scene: put `text` in, if the scene still reads
    /// `was` (otherwise keep both: see the app).
    Replace { id: String, was: String, text: String },
    /// Keep this text as a snapshot of the scene: the iPad's side of a clash.
    Snapshot { id: String, text: String, name: String },
    /// A new scene written on the iPad.
    NewScene { inbox_id: i64, title: String, text: String },
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
    let Input { project, title, index, scenes, state, stats } = input;
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
                Some(t) => {
                    clash(&mut out, id, &t.text);
                    send(remote, &project, &mut out, id, mine, t.version)?;
                }
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
                        out.actions.push(Action::Replace { id: id.clone(), was: mine.clone(), text: t.text.clone() });
                        out.state.scenes.insert(id.clone(), Synced { version: t.version, text: t.text.clone() });
                        out.received += 1;
                    }
                    Some(t) => {
                        clash(&mut out, id, &t.text);
                        send(remote, &project, &mut out, id, mine, t.version)?;
                    }
                }
            }
        }
    }

    for item in remote.inbox(&project)? {
        if !out.state.imported.contains(&item.id) {
            out.actions.push(Action::NewScene { inbox_id: item.id, title: item.title, text: item.text });
        }
    }
    out.state.seq = seq;
    Ok(out)
}

/// Both sides changed a scene: the iPad's text is kept as a snapshot.
fn clash(out: &mut Outcome, id: &str, theirs: &str) {
    out.actions.push(Action::Snapshot {
        id: id.to_owned(),
        text: theirs.to_owned(),
        name: "from the iPad (both changed)".to_owned(),
    });
    out.clashes += 1;
}

/// Send a scene, edited from version `base`. If the iPad saved it meanwhile,
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
    }

    impl Fake {
        /// An edit made on the iPad, from the version it read.
        pub fn ipad_edit(&mut self, id: &str, text: &str) {
            let version = self.scenes.get(id).map_or(0, |s| s.1);
            assert_eq!(self.put_scene("p", id, text, version).unwrap(), Put::Saved { version: version + 1 });
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
            let list = changed.into_iter().map(|(id, s)| Change { id: id.clone(), text: s.0.clone(), version: s.1 }).collect();
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
        }
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

        // The Mac edits a; the iPad edits b.
        helper.ipad_edit("b", "two, from the iPad");
        let out = run(&mut helper, input(&[("a", "one, from the Mac"), ("b", "two")], state)).unwrap();
        assert_eq!((out.sent, out.received, out.clashes), (1, 1, 0));
        assert_eq!(helper.text("a"), Some("one, from the Mac"));
        assert_eq!(
            out.actions,
            [Action::Replace { id: "b".into(), was: "two".into(), text: "two, from the iPad".into() }]
        );

        // Next time, with b brought in: nothing to do.
        let out = run(&mut helper, input(&[("a", "one, from the Mac"), ("b", "two, from the iPad")], out.state)).unwrap();
        assert_eq!((out.sent, out.received, out.clashes), (0, 0, 0));
        assert!(out.actions.is_empty());
    }

    #[test]
    fn both_changed_keeps_the_macs_and_snapshots_the_ipads() {
        let mut helper = Fake::default();
        let state = run(&mut helper, input(&[("a", "start")], SyncState::default())).unwrap().state;
        helper.ipad_edit("a", "the iPad's words");
        let out = run(&mut helper, input(&[("a", "the Mac's words")], state)).unwrap();
        assert_eq!(out.clashes, 1);
        assert_eq!(helper.text("a"), Some("the Mac's words"));
        assert!(matches!(&out.actions[..], [Action::Snapshot { text, .. }] if text == "the iPad's words"));
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
    fn sync_state_round_trips() {
        let dir = std::env::temp_dir().join(format!("sloth-writer-syncstate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut state = SyncState { seq: 4, ..Default::default() };
        state.scenes.insert("a".into(), Synced { version: 2, text: "words".into() });
        state.save(&dir).unwrap();
        assert_eq!(SyncState::load(&dir), state);
        assert_eq!(SyncState::load(&dir.join("missing")), SyncState::default());
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
        // An edit "on the iPad" through the API, as the web editor would make it.
        assert_eq!(helper.put_scene(&project, "b", "two, from the iPad", 1).unwrap(), Put::Saved { version: 2 });
        let out = run(&mut helper, i(&[("a", "one!"), ("b", "two")], out.state)).unwrap();
        assert_eq!((out.sent, out.received, out.clashes), (1, 1, 0));
        // And a clash.
        assert_eq!(helper.put_scene(&project, "a", "iPad", 2).unwrap(), Put::Saved { version: 3 });
        let out = run(&mut helper, i(&[("a", "Mac"), ("b", "two, from the iPad")], out.state)).unwrap();
        assert_eq!(out.clashes, 1);
        assert_eq!(helper.put_scene(&project, "a", "stale", 1).unwrap(), Put::Clash { current: Some(("Mac".into(), 4)) });

        let bad = run(&mut Http::new(&url, "not-a-token"), i(&[], SyncState::default()));
        assert!(matches!(bad, Err(SyncError::Unauthorized)));
    }
}
