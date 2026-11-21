//! Everything Sloth remembers between launches, saved as one value in
//! `~/Library/Application Support/Sloth/app.ron`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::editor::EditorSettings;
use crate::theme::ThemeChoice;

/// How many files File → Open Recent remembers.
pub const MAX_RECENT: usize = 15;

/// How many recent searches the find field remembers.
pub const MAX_SEARCH_HISTORY: usize = 20;

const STORAGE_KEY: &str = "settings";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: ThemeChoice,
    pub editor: EditorSettings,
    /// Most recent first.
    pub recent_files: Vec<PathBuf>,
    /// Tabs open when Sloth last quit, reopened at launch.
    pub session: Session,
    /// Recent searches, newest first.
    pub search_history: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Session {
    pub files: Vec<PathBuf>,
    /// Index into `files` of the tab that was active.
    pub active: usize,
}

impl Settings {
    pub fn load(storage: Option<&dyn eframe::Storage>) -> Self {
        let Some(storage) = storage else {
            return Self::default();
        };
        eframe::get_value(storage, STORAGE_KEY).unwrap_or_else(|| Self::from_legacy_keys(storage))
    }

    pub fn save(&self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, STORAGE_KEY, self);
    }

    /// Versions up to 0.2 saved each setting under its own key.
    fn from_legacy_keys(storage: &dyn eframe::Storage) -> Self {
        let mut s = Self::default();
        let get = |key| storage.get_string(key);
        if let Some(theme) = get("theme").and_then(|t| ThemeChoice::from_key(&t)) {
            s.theme = theme;
        }
        if let Some(v) = get("line_numbers").and_then(|v| v.parse().ok()) {
            s.editor.show_line_numbers = v;
        }
        if let Some(v) = get("word_wrap").and_then(|v| v.parse().ok()) {
            s.editor.word_wrap = v;
        }
        if let Some(v) = get("font_size").and_then(|v| v.parse().ok()) {
            s.editor.font_size = v;
        }
        s
    }

    /// Put `path` at the top of the recent files list.
    pub fn add_recent(&mut self, path: &Path) {
        self.recent_files.retain(|p| p != path);
        self.recent_files.insert(0, path.to_path_buf());
        self.recent_files.truncate(MAX_RECENT);
    }

    pub fn remove_recent(&mut self, path: &Path) {
        self.recent_files.retain(|p| p != path);
    }

    /// Remember a search (newest first, no duplicates).
    pub fn add_search(&mut self, query: &str) {
        if query.is_empty() {
            return;
        }
        self.search_history.retain(|q| q != query);
        self.search_history.insert(0, query.to_owned());
        self.search_history.truncate(MAX_SEARCH_HISTORY);
    }
}

/// Menu labels for recent files: the file name, plus its folder when two
/// entries share a name (e.g. "notes.txt — work" and "notes.txt — home").
pub fn recent_labels(paths: &[PathBuf]) -> Vec<String> {
    let name = |p: &PathBuf| {
        p.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| p.display().to_string())
    };
    paths
        .iter()
        .map(|p| {
            let n = name(p);
            let duplicate = paths.iter().filter(|q| name(q) == n).count() > 1;
            match p.parent().and_then(|d| d.file_name()) {
                Some(dir) if duplicate => format!("{n} — {}", dir.to_string_lossy()),
                _ => n,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_files_are_unique_newest_first_and_capped() {
        let mut s = Settings::default();
        for i in 0..20 {
            s.add_recent(Path::new(&format!("/f{i}.txt")));
        }
        s.add_recent(Path::new("/f10.txt"));
        assert_eq!(s.recent_files.len(), MAX_RECENT);
        assert_eq!(s.recent_files[0], PathBuf::from("/f10.txt"));
        assert_eq!(s.recent_files.iter().filter(|p| p.ends_with("f10.txt")).count(), 1);
    }

    #[test]
    fn labels_disambiguate_same_names() {
        let paths = vec![
            PathBuf::from("/work/notes.txt"),
            PathBuf::from("/home/notes.txt"),
            PathBuf::from("/home/todo.md"),
        ];
        assert_eq!(
            recent_labels(&paths),
            vec!["notes.txt — work", "notes.txt — home", "todo.md"]
        );
    }

    #[test]
    fn round_trips_through_ron() {
        let mut s = Settings::default();
        s.add_recent(Path::new("/a.txt"));
        s.session.files.push("/b.md".into());
        s.theme = ThemeChoice::Sepia;
        let text = ron::to_string(&s).unwrap();
        assert_eq!(ron::from_str::<Settings>(&text).unwrap(), s);
        // Missing fields fall back to defaults, so older files still load.
        assert_eq!(ron::from_str::<Settings>("()").unwrap(), Settings::default());
    }
}
