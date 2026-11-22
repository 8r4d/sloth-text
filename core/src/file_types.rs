//! File formats offered in the Save dialog, and default extensions.

use std::path::{Path, PathBuf};

pub struct FileType {
    pub name: &'static str,
    /// Empty means "any extension".
    pub extensions: &'static [&'static str],
}

pub const PLAIN_TEXT: FileType = FileType {
    name: "Plain Text",
    extensions: &["txt", "text"],
};
pub const MARKDOWN: FileType = FileType {
    name: "Markdown",
    extensions: &["md", "markdown"],
};
pub const ANY: FileType = FileType {
    name: "Any Format",
    extensions: &[],
};

/// Extension used for new documents.
pub const DEFAULT_EXTENSION: &str = "txt";

/// Formats for the Save dialog, best guess for this document first (the
/// first one is pre-selected). On macOS, typing a name without an extension
/// gets the selected format's extension added automatically.
pub fn save_formats(path: Option<&Path>) -> Vec<&'static FileType> {
    let ext = path
        .and_then(|p| p.extension())
        .map(|e| e.to_string_lossy().to_lowercase());
    let first: &'static FileType = match ext.as_deref() {
        None => &PLAIN_TEXT,
        Some(e) if MARKDOWN.extensions.contains(&e) => &MARKDOWN,
        Some(e) if PLAIN_TEXT.extensions.contains(&e) => &PLAIN_TEXT,
        // Something else (.rs, .csv, …): keep whatever the user has.
        Some(_) => &ANY,
    };
    let mut formats = vec![first];
    for t in [&PLAIN_TEXT, &MARKDOWN, &ANY] {
        if !std::ptr::eq(t, first) {
            formats.push(t);
        }
    }
    formats
}

/// Name suggested in the Save dialog.
pub fn suggested_name(path: Option<&Path>) -> String {
    match path.and_then(|p| p.file_name()) {
        Some(name) => name.to_string_lossy().into_owned(),
        None => format!("Untitled.{DEFAULT_EXTENSION}"),
    }
}

/// Add the default extension if the chosen path has none. Used where the
/// native dialog doesn't do it for us (macOS does).
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub fn with_default_extension(mut path: PathBuf) -> PathBuf {
    if path.extension().is_none() {
        path.set_extension(DEFAULT_EXTENSION);
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_format_from_extension() {
        assert_eq!(save_formats(None)[0].name, "Plain Text");
        assert_eq!(save_formats(Some(Path::new("a.MD")))[0].name, "Markdown");
        assert_eq!(save_formats(Some(Path::new("a.rs")))[0].name, "Any Format");
        assert_eq!(save_formats(None).len(), 3);
    }

    #[test]
    fn default_names_and_extensions() {
        assert_eq!(suggested_name(None), "Untitled.txt");
        assert_eq!(with_default_extension("notes".into()), PathBuf::from("notes.txt"));
        assert_eq!(with_default_extension("notes.md".into()), PathBuf::from("notes.md"));
    }
}
