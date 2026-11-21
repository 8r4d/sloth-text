//! A single open text document: its contents, where it lives on disk,
//! and whether it has unsaved changes.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Default)]
pub struct Document {
    pub text: String,
    pub path: Option<PathBuf>,
    /// The text as it was last loaded/saved, used to compute the dirty flag.
    saved_text: String,
    /// The file's modification time and size when we last read or wrote it,
    /// used to notice changes made by other programs.
    disk_stamp: Option<DiskStamp>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DiskStamp {
    modified: Option<SystemTime>,
    len: u64,
}

impl DiskStamp {
    fn read(path: &Path) -> Option<Self> {
        let meta = fs::metadata(path).ok()?;
        Some(Self {
            modified: meta.modified().ok(),
            len: meta.len(),
        })
    }
}

/// How the file on disk compares with what we last loaded or saved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiskState {
    Unchanged,
    /// Another program changed the file.
    Changed,
    /// The file was deleted or moved.
    Missing,
}

impl Document {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn open(path: &Path) -> io::Result<Self> {
        let bytes = fs::read(path)?;
        // Plain-text editor: decode lossily so binary-ish files still open
        // rather than failing outright.
        let text = String::from_utf8_lossy(&bytes).into_owned();
        Ok(Self {
            saved_text: text.clone(),
            text,
            path: Some(path.to_path_buf()),
            disk_stamp: DiskStamp::read(path),
        })
    }

    /// Replace the contents with what's on disk now, discarding edits.
    pub fn reload(&mut self) -> io::Result<()> {
        let path = self.path.clone().ok_or_else(no_path)?;
        *self = Self::open(&path)?;
        Ok(())
    }

    pub fn disk_state(&self) -> DiskState {
        let Some(path) = &self.path else {
            return DiskState::Unchanged;
        };
        match (self.disk_stamp, DiskStamp::read(path)) {
            (Some(_), None) => DiskState::Missing,
            (Some(old), Some(now)) if old != now => DiskState::Changed,
            // Never read from disk (e.g. `sloth new.txt` for a file that
            // doesn't exist yet): nothing to compare against.
            _ => DiskState::Unchanged,
        }
    }

    /// Accept the current file on disk as the baseline without reloading
    /// ("keep my version"), so we don't ask about the same change again.
    pub fn acknowledge_disk_state(&mut self) {
        self.disk_stamp = self.path.as_deref().and_then(DiskStamp::read);
    }

    /// Save to the document's current path. Errors if it has no path yet.
    pub fn save(&mut self) -> io::Result<()> {
        let path = self.path.clone().ok_or_else(no_path)?;
        self.save_as(&path)
    }

    pub fn save_as(&mut self, path: &Path) -> io::Result<()> {
        fs::write(path, self.text.as_bytes())?;
        self.path = Some(path.to_path_buf());
        self.saved_text = self.text.clone();
        self.disk_stamp = DiskStamp::read(path);
        Ok(())
    }

    pub fn is_dirty(&self) -> bool {
        self.text != self.saved_text
    }

    pub fn display_name(&self) -> String {
        self.path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled".to_owned())
    }

    pub fn line_count(&self) -> usize {
        self.text.split('\n').count()
    }
}

fn no_path() -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, "document has no path")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dirty_tracking_and_round_trip() {
        let dir = std::env::temp_dir().join(format!("sloth-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.txt");

        let mut doc = Document::new();
        assert!(!doc.is_dirty());
        doc.text.push_str("hello\nworld");
        assert!(doc.is_dirty());
        assert!(doc.save().is_err(), "untitled doc has no path");
        doc.save_as(&path).unwrap();
        assert!(!doc.is_dirty());
        assert_eq!(doc.display_name(), "a.txt");

        let reopened = Document::open(&path).unwrap();
        assert_eq!(reopened.text, "hello\nworld");
        assert_eq!(reopened.line_count(), 2);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn notices_outside_changes_and_deletion() {
        let dir = std::env::temp_dir().join(format!("sloth-disk-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("b.txt");
        fs::write(&path, "one").unwrap();

        let mut doc = Document::open(&path).unwrap();
        assert_eq!(doc.disk_state(), DiskState::Unchanged);

        // Another program rewrites it (different length, so detectable even
        // if the timestamp resolution is coarse).
        fs::write(&path, "one two").unwrap();
        assert_eq!(doc.disk_state(), DiskState::Changed);
        doc.acknowledge_disk_state();
        assert_eq!(doc.disk_state(), DiskState::Unchanged);

        doc.reload().unwrap();
        assert_eq!(doc.text, "one two");
        assert!(!doc.is_dirty());

        // Our own save doesn't count as an outside change.
        doc.text.push('!');
        doc.save().unwrap();
        assert_eq!(doc.disk_state(), DiskState::Unchanged);

        fs::remove_file(&path).unwrap();
        assert_eq!(doc.disk_state(), DiskState::Missing);
        fs::remove_dir_all(&dir).ok();
    }
}
