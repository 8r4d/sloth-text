//! A single open text document: its contents, where it lives on disk, how
//! the file is stored (encoding, line endings), and whether it has unsaved
//! changes.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use encoding_rs::Encoding;

use crate::text_format::{self, FileFormat, LineEnding};

#[derive(Default)]
pub struct Document {
    pub text: String,
    pub path: Option<PathBuf>,
    /// How the file is (or will be) stored on disk.
    pub format: FileFormat,
    /// Some bytes couldn't be read in `format.encoding` and show as `�`.
    /// Saving over the original would lose them, so we ask first.
    pub read_with_errors: bool,
    /// The text and format as last loaded/saved, for the dirty flag.
    saved_text: String,
    saved_format: FileFormat,
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

#[derive(Debug)]
pub enum SaveError {
    Io(io::Error),
    NoPath,
    /// Some characters can't be written in the document's encoding.
    Unmappable(&'static Encoding),
    /// The file was read with errors; saving over it needs confirmation
    /// (see [`Document::read_with_errors`]).
    ReadWithErrors,
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SaveError::Io(e) => write!(f, "{e}"),
            SaveError::NoPath => write!(f, "the document has no file yet"),
            SaveError::Unmappable(enc) => {
                write!(f, "some characters can't be saved as {}", enc.name())
            }
            SaveError::ReadWithErrors => write!(f, "the file wasn't read cleanly"),
        }
    }
}

impl From<io::Error> for SaveError {
    fn from(e: io::Error) -> Self {
        SaveError::Io(e)
    }
}

impl Document {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn open(path: &Path) -> io::Result<Self> {
        Self::open_with_encoding(path, None)
    }

    /// Open with a specific encoding instead of detecting one.
    pub fn open_with_encoding(path: &Path, encoding: Option<&'static Encoding>) -> io::Result<Self> {
        let bytes = fs::read(path)?;
        let decoded = text_format::decode(&bytes, encoding);
        Ok(Self {
            saved_text: decoded.text.clone(),
            text: decoded.text,
            path: Some(path.to_path_buf()),
            format: decoded.format,
            saved_format: decoded.format,
            read_with_errors: decoded.had_errors,
            disk_stamp: DiskStamp::read(path),
        })
    }

    /// Replace the contents with what's on disk now, discarding edits.
    /// `None` detects the encoding again; otherwise it's read as `encoding`.
    pub fn reload_with_encoding(&mut self, encoding: Option<&'static Encoding>) -> io::Result<()> {
        let path = self.path.clone().ok_or_else(no_path)?;
        *self = Self::open_with_encoding(&path, encoding)?;
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

    /// Save to the document's current path.
    pub fn save(&mut self) -> Result<(), SaveError> {
        let path = self.path.clone().ok_or(SaveError::NoPath)?;
        if self.read_with_errors {
            return Err(SaveError::ReadWithErrors);
        }
        self.write_to(&path)
    }

    /// Save to a (possibly new) path. Saving a copy elsewhere doesn't touch
    /// the original, so it doesn't need the read-with-errors confirmation.
    pub fn save_as(&mut self, path: &Path) -> Result<(), SaveError> {
        if self.read_with_errors && self.path.as_deref() == Some(path) {
            return Err(SaveError::ReadWithErrors);
        }
        self.write_to(path)?;
        self.read_with_errors = false;
        Ok(())
    }

    /// The user confirmed saving even though the file wasn't read cleanly.
    pub fn accept_read_errors(&mut self) {
        self.read_with_errors = false;
    }

    fn write_to(&mut self, path: &Path) -> Result<(), SaveError> {
        let bytes =
            text_format::encode(&self.text, &self.format).map_err(|u| SaveError::Unmappable(u.encoding))?;
        write_atomically(path, &bytes)?;
        self.path = Some(path.to_path_buf());
        self.saved_text = self.text.clone();
        self.saved_format = self.format;
        self.disk_stamp = DiskStamp::read(path);
        Ok(())
    }

    /// Change the encoding used for the next save.
    pub fn convert_encoding(&mut self, encoding: &'static Encoding) {
        self.format.encoding = encoding;
        // UTF-16 files need a byte-order mark to be recognised; others
        // conventionally go without.
        self.format.bom = encoding == encoding_rs::UTF_16LE || encoding == encoding_rs::UTF_16BE;
    }

    pub fn set_line_ending(&mut self, line_ending: LineEnding) {
        self.format.line_ending = line_ending;
    }

    pub fn is_dirty(&self) -> bool {
        self.format != self.saved_format || self.text != self.saved_text
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

/// Write a file so that a crash or full disk can never leave it half
/// written: write a temporary file next to it, then swap it into place.
/// Public so other files an app keeps (such as a project manifest) get the
/// same protection as documents.
pub fn write_atomically(path: &Path, bytes: &[u8]) -> io::Result<()> {
    // Write through symlinks to the real file, rather than replacing the link.
    let target = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let dir = target.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let name = target.file_name().map(|n| n.to_string_lossy()).unwrap_or_default();
    let temp = dir.join(format!(".{name}.sloth-{}.tmp", std::process::id()));

    let result = (|| {
        let mut file = File::create(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        // Keep the original's permissions (e.g. an executable script).
        if let Ok(meta) = fs::metadata(&target) {
            fs::set_permissions(&temp, meta.permissions())?;
            keep_attributes(&target, &file, &temp, &meta);
        }
        fs::rename(&temp, &target)
    })();

    match result {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = fs::remove_file(&temp);
            if e.kind() == io::ErrorKind::PermissionDenied && target.exists() {
                // The folder isn't writable but the file may be: write in place.
                fs::write(&target, bytes)
            } else {
                Err(e)
            }
        }
    }
}

/// What the swap in `write_atomically` would otherwise lose, copied from the
/// original onto the new file: its extended attributes (Finder tags and
/// colour labels, where it was downloaded from…), its access list, and its
/// creation date. Best effort: the words matter more than these.
#[cfg(target_os = "macos")]
fn keep_attributes(original: &Path, new: &File, new_path: &Path, meta: &fs::Metadata) {
    use std::ffi::CString;
    use std::os::macos::fs::FileTimesExt;
    use std::os::unix::ffi::OsStrExt;

    unsafe extern "C" {
        // copyfile(3), from the system library.
        fn copyfile(from: *const std::ffi::c_char, to: *const std::ffi::c_char, state: *mut std::ffi::c_void, flags: u32) -> i32;
    }
    const COPYFILE_ACL: u32 = 1 << 0;
    const COPYFILE_XATTR: u32 = 1 << 2;
    if let (Ok(from), Ok(to)) = (CString::new(original.as_os_str().as_bytes()), CString::new(new_path.as_os_str().as_bytes())) {
        // SAFETY: two valid C strings for paths, no state, documented flags.
        unsafe { copyfile(from.as_ptr(), to.as_ptr(), std::ptr::null_mut(), COPYFILE_ACL | COPYFILE_XATTR) };
    }
    if let Ok(created) = meta.created() {
        let _ = new.set_times(fs::FileTimes::new().set_created(created));
    }
}

#[cfg(not(target_os = "macos"))]
fn keep_attributes(_original: &Path, _new: &File, _new_path: &Path, _meta: &fs::Metadata) {}

fn no_path() -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, "document has no path")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sloth-doc-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Finder tags and other extended attributes, and the creation date,
    /// survive a save (which swaps in a new file).
    #[cfg(target_os = "macos")]
    #[test]
    fn saving_keeps_tags_attributes_and_creation_date() {
        use std::process::Command;
        let dir = temp_dir("xattr");
        let path = dir.join("tagged.txt");
        fs::write(&path, "before").unwrap();
        let ok = Command::new("xattr").args(["-w", "com.example.sloth-test", "kept"]).arg(&path).status().unwrap();
        assert!(ok.success());
        let created = fs::metadata(&path).unwrap().created().unwrap();

        write_atomically(&path, b"after").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "after");
        let out = Command::new("xattr").args(["-p", "com.example.sloth-test"]).arg(&path).output().unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "kept");
        assert_eq!(fs::metadata(&path).unwrap().created().unwrap(), created);
    }

    #[test]
    fn dirty_tracking_and_round_trip() {
        let dir = temp_dir("roundtrip");
        let path = dir.join("a.txt");

        let mut doc = Document::new();
        assert!(!doc.is_dirty());
        doc.text.push_str("hello\nworld");
        assert!(doc.is_dirty());
        assert!(matches!(doc.save(), Err(SaveError::NoPath)));
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
        let dir = temp_dir("disk");
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

        doc.reload_with_encoding(None).unwrap();
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

    #[test]
    fn keeps_encoding_and_line_endings_when_saving() {
        let dir = temp_dir("format");
        let path = dir.join("win.txt");
        fs::write(&path, b"caf\xE9 cr\xE8me \xE0 la plage\r\nd\xE9j\xE0 vu\r\n").unwrap();

        let mut doc = Document::open(&path).unwrap();
        assert_eq!(doc.text, "café crème à la plage\ndéjà vu\n");
        doc.text.push_str("été\n");
        doc.save().unwrap();
        assert_eq!(
            fs::read(&path).unwrap(),
            b"caf\xE9 cr\xE8me \xE0 la plage\r\nd\xE9j\xE0 vu\r\n\xE9t\xE9\r\n"
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn converting_format_marks_dirty_and_saves_new_format() {
        let dir = temp_dir("convert");
        let path = dir.join("c.txt");
        fs::write(&path, "a\nb").unwrap();

        let mut doc = Document::open(&path).unwrap();
        doc.set_line_ending(LineEnding::CrLf);
        assert!(doc.is_dirty());
        doc.save().unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"a\r\nb");

        doc.convert_encoding(encoding_rs::UTF_16LE);
        doc.save().unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"\xFF\xFEa\0\r\0\n\0b\0");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn refuses_unmappable_characters() {
        let dir = temp_dir("unmappable");
        let path = dir.join("d.txt");
        fs::write(&path, b"caf\xE9 cr\xE8me \xE0 la plage").unwrap();

        let mut doc = Document::open(&path).unwrap();
        doc.text.push_str(" ☃");
        assert!(matches!(doc.save(), Err(SaveError::Unmappable(_))));
        // Nothing was written.
        assert_eq!(fs::read(&path).unwrap(), b"caf\xE9 cr\xE8me \xE0 la plage");
        doc.convert_encoding(encoding_rs::UTF_8);
        doc.save().unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "café crème à la plage ☃");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn asks_before_overwriting_a_file_read_with_errors() {
        let dir = temp_dir("damaged");
        let path = dir.join("e.txt");
        fs::write(&path, b"ok \xFF").unwrap();

        let mut doc = Document::open_with_encoding(&path, Some(encoding_rs::UTF_8)).unwrap();
        assert!(doc.read_with_errors);
        assert!(matches!(doc.save(), Err(SaveError::ReadWithErrors)));
        // A copy elsewhere is fine.
        doc.save_as(&dir.join("copy.txt")).unwrap();
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    #[cfg(unix)] // Permissions and symlinks as Unix has them.
    fn atomic_save_keeps_permissions_and_symlinks() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let dir = temp_dir("atomic");
        let real = dir.join("script.sh");
        fs::write(&real, "echo hi\n").unwrap();
        fs::set_permissions(&real, fs::Permissions::from_mode(0o755)).unwrap();
        let link = dir.join("link.sh");
        symlink(&real, &link).unwrap();

        let mut doc = Document::open(&link).unwrap();
        doc.text.push_str("echo bye\n");
        doc.save().unwrap();

        assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert_eq!(fs::read_to_string(&real).unwrap(), "echo hi\necho bye\n");
        assert_eq!(fs::metadata(&real).unwrap().permissions().mode() & 0o777, 0o755);
        // No temporary files left behind.
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 2);
        fs::remove_dir_all(&dir).ok();
    }
}
