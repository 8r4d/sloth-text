//! Crash recovery: every few seconds, copies of documents with unsaved
//! changes are written to `~/Library/Application Support/Sloth/recovery/`.
//!
//! Each running Sloth writes into its own folder (named after its process
//! id). A clean quit deletes it. A folder whose process is no longer running
//! is left over from a crash, and at launch Sloth offers to restore it.

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::document::Document;
use crate::tabs::TabId;
use crate::text_format::{FileFormat, LineEnding};

/// One recovered document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Record {
    /// The file it belongs to, or `None` for an untitled document.
    pub path: Option<PathBuf>,
    pub text: String,
    pub encoding: String,
    pub bom: bool,
    pub crlf: bool,
}

impl Record {
    pub fn from_document(doc: &Document) -> Self {
        Self {
            path: doc.path.clone(),
            text: doc.text.clone(),
            encoding: doc.format.encoding.name().to_owned(),
            bom: doc.format.bom,
            crlf: doc.format.line_ending == LineEnding::CrLf,
        }
    }

    /// Rebuild a document with the recovered text (marked as unsaved).
    pub fn into_document(self) -> Document {
        let mut doc = match &self.path {
            Some(path) if path.exists() => Document::open(path).unwrap_or_default(),
            _ => Document::new(),
        };
        doc.path = self.path;
        doc.text = self.text;
        doc.format = FileFormat {
            encoding: encoding_rs::Encoding::for_label(self.encoding.as_bytes())
                .unwrap_or(encoding_rs::UTF_8),
            bom: self.bom,
            line_ending: if self.crlf { LineEnding::CrLf } else { LineEnding::Lf },
        };
        doc
    }

    pub fn display_name(&self) -> String {
        self.path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled".to_owned())
    }
}

/// This session's recovery folder.
pub struct Recovery {
    base: PathBuf,
    dir: PathBuf,
    /// Hash of what was last written per tab, to skip unchanged documents.
    written: HashMap<TabId, u64>,
}

/// Documents left behind by a session that didn't quit cleanly.
pub struct Leftovers {
    pub records: Vec<Record>,
    dirs: Vec<PathBuf>,
}

impl Leftovers {
    /// Delete the leftover files (after restoring or discarding them).
    pub fn remove(self) {
        for dir in self.dirs {
            let _ = fs::remove_dir_all(dir);
        }
    }
}

impl Recovery {
    pub fn new(base: PathBuf) -> Self {
        let dir = base.join(std::process::id().to_string());
        Self {
            base,
            dir,
            written: HashMap::new(),
        }
    }

    /// Write a recovery copy of a document if it changed since last time.
    pub fn save(&mut self, tab: TabId, doc: &Document) -> io::Result<()> {
        let hash = hash_of(doc);
        if self.written.get(&tab) == Some(&hash) {
            return Ok(());
        }
        fs::create_dir_all(&self.dir)?;
        let record = Record::from_document(doc);
        let text = ron::to_string(&record).map_err(io::Error::other)?;
        let path = self.file_for(tab);
        let temp = path.with_extension("tmp");
        fs::write(&temp, text)?;
        fs::rename(&temp, &path)?;
        self.written.insert(tab, hash);
        Ok(())
    }

    /// The document was saved or closed: its recovery copy isn't needed.
    pub fn remove(&mut self, tab: TabId) {
        if self.written.remove(&tab).is_some() {
            let _ = fs::remove_file(self.file_for(tab));
        }
    }

    /// Keep copies only for these tabs.
    pub fn retain(&mut self, keep: impl Fn(TabId) -> bool) {
        let gone: Vec<TabId> = self.written.keys().copied().filter(|&t| !keep(t)).collect();
        for tab in gone {
            self.remove(tab);
        }
    }

    /// Clean quit: remove this session's folder.
    pub fn clear(&mut self) {
        self.written.clear();
        let _ = fs::remove_dir_all(&self.dir);
    }

    /// Recovery folders from sessions that are no longer running.
    pub fn leftovers(&self) -> Option<Leftovers> {
        let mut records = Vec::new();
        let mut dirs = Vec::new();
        for entry in fs::read_dir(&self.base).ok()?.flatten() {
            let dir = entry.path();
            let Some(pid) = dir.file_name().and_then(|n| n.to_str()?.parse::<u32>().ok()) else {
                continue;
            };
            if dir == self.dir || process_is_running(pid) {
                continue;
            }
            records.extend(read_records(&dir));
            dirs.push(dir);
        }
        (!dirs.is_empty()).then_some(Leftovers { records, dirs })
    }

    fn file_for(&self, tab: TabId) -> PathBuf {
        self.dir.join(format!("tab-{tab}.ron"))
    }
}

fn read_records(dir: &Path) -> Vec<Record> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut records: Vec<(PathBuf, Record)> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "ron"))
        .filter_map(|p| {
            let text = fs::read_to_string(&p).ok()?;
            Some((p, ron::from_str(&text).ok()?))
        })
        .collect();
    // Restore in the order the tabs were created.
    records.sort_by(|a, b| a.0.cmp(&b.0));
    records.into_iter().map(|(_, r)| r).collect()
}

fn hash_of(doc: &Document) -> u64 {
    let mut h = DefaultHasher::new();
    doc.text.hash(&mut h);
    doc.path.hash(&mut h);
    doc.format.encoding.name().hash(&mut h);
    doc.format.bom.hash(&mut h);
    doc.format.line_ending.hash(&mut h);
    h.finish()
}

fn process_is_running(pid: u32) -> bool {
    // `kill -0` checks whether a process exists without signalling it.
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_base(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sloth-recovery-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn doc(path: Option<&str>, text: &str) -> Document {
        let mut d = Document::new();
        d.path = path.map(PathBuf::from);
        d.text = text.to_owned();
        d
    }

    #[test]
    fn leftovers_from_a_crashed_session_are_found_and_restored() {
        let base = temp_base("crash");
        // A session from a process that no longer exists (pid 999999999).
        let mut crashed = Recovery::new(base.clone());
        crashed.dir = base.join("999999999");
        crashed.save(0, &doc(None, "draft one")).unwrap();
        crashed.save(1, &doc(Some("/tmp/does-not-exist/notes.txt"), "draft two")).unwrap();

        // Our own session's files are never treated as leftovers.
        let mut current = Recovery::new(base.clone());
        current.save(0, &doc(None, "mine")).unwrap();

        let leftovers = current.leftovers().expect("found leftovers");
        assert_eq!(leftovers.records.len(), 2);
        assert_eq!(leftovers.records[0].text, "draft one");
        assert_eq!(leftovers.records[1].display_name(), "notes.txt");

        let restored = leftovers.records[1].clone().into_document();
        assert_eq!(restored.text, "draft two");
        assert!(restored.is_dirty());

        leftovers.remove();
        assert!(current.leftovers().is_none());
        current.clear();
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn saved_or_closed_documents_are_removed() {
        let base = temp_base("remove");
        let mut r = Recovery::new(base.clone());
        r.save(3, &doc(None, "x")).unwrap();
        r.save(4, &doc(None, "y")).unwrap();
        r.remove(3);
        r.retain(|t| t != 4);
        assert_eq!(fs::read_dir(&r.dir).unwrap().count(), 0);
        r.clear();
        assert!(!r.dir.exists());
        let _ = fs::remove_dir_all(&base);
    }
}
