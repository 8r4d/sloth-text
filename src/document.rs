//! A single open text document: its contents, where it lives on disk,
//! and whether it has unsaved changes.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct Document {
    pub text: String,
    pub path: Option<PathBuf>,
    /// The text as it was last loaded/saved, used to compute the dirty flag.
    saved_text: String,
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
        })
    }

    /// Save to the document's current path. Errors if it has no path yet.
    pub fn save(&mut self) -> io::Result<()> {
        let path = self
            .path
            .clone()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "document has no path"))?;
        self.save_as(&path)
    }

    pub fn save_as(&mut self, path: &Path) -> io::Result<()> {
        fs::write(path, self.text.as_bytes())?;
        self.path = Some(path.to_path_buf());
        self.saved_text = self.text.clone();
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
}
