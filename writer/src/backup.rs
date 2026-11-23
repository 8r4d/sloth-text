//! Back Up: the whole project folder (documents, snapshots, the project
//! file, sync record and statistics) as one dated zip file somewhere else,
//! such as another drive or a cloud folder.
//!
//! Automatic backups go in a folder of their own per project (see
//! `auto_folder`), made when a project closes and once a day, only when
//! something changed, keeping the newest few. Backups made by hand are
//! never pruned.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use zip::write::SimpleFileOptions;

/// Files not worth keeping (the Finder's, and editors' leftovers).
fn skip(name: &str) -> bool {
    name == ".DS_Store" || name.ends_with('~') || name.starts_with("._")
}

/// The backup's file name: "The Long Winter 2026-11-22 14.05.zip".
pub fn file_name(title: &str, when: chrono::NaiveDateTime) -> String {
    let safe: String = title.chars().map(|c| if matches!(c, '/' | ':' | '\\') { '-' } else { c }).collect();
    format!("{} {}.zip", safe.trim(), when.format("%Y-%m-%d %H.%M"))
}

/// Zip the project at `root` into `folder`, as `name`. Inside, everything is
/// in a folder named like the project's, so unzipping gives the project
/// back as it was. Returns the zip's path.
pub fn back_up(root: &Path, folder: &Path, name: &str) -> io::Result<PathBuf> {
    // A backup inside the project would be backed up next time, and again…
    let (root_abs, folder_abs) = (root.canonicalize()?, folder.canonicalize()?);
    if folder_abs.starts_with(&root_abs) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "Choose a folder outside the project."));
    }
    let top = root_abs.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Project".into());
    let path = folder.join(name);
    // Written beside, then renamed: never a half-made zip under the real name.
    let partial = folder.join(format!(".{name}.partial"));
    let result = write_zip(&root_abs, &top, &partial).and_then(|()| fs::rename(&partial, &path));
    if result.is_err() {
        let _ = fs::remove_file(&partial);
    }
    result.map(|()| path)
}

/// Where automatic backups go unless another folder is chosen:
/// ~/Library/Application Support/Sloth Writer/Backups.
pub fn default_root() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    home.join("Library/Application Support/Sloth Writer/Backups")
}

/// A project's automatic backups: a folder named like the project's own.
pub fn auto_folder(backups: &Path, project_root: &Path) -> PathBuf {
    let name = project_root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Project".into());
    backups.join(name)
}

/// The newest backup in a folder, and when it was made.
pub fn newest(folder: &Path) -> Option<(PathBuf, SystemTime)> {
    zips(folder).into_iter().max_by_key(|(_, t)| *t)
}

/// The zips in a folder, with when each was made.
fn zips(folder: &Path) -> Vec<(PathBuf, SystemTime)> {
    let Ok(entries) = fs::read_dir(folder) else { return Vec::new() };
    entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "zip"))
        .filter_map(|p| Some((p.clone(), fs::metadata(&p).ok()?.modified().ok()?)))
        .collect()
}

/// Whether anything in the project changed after `since`.
pub fn changed_since(root: &Path, since: SystemTime) -> bool {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.filter_map(Result::ok) {
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                stack.push(entry.path());
            } else if meta.modified().is_ok_and(|t| t > since) && !skip(&entry.file_name().to_string_lossy()) {
                return true;
            }
        }
    }
    false
}

/// Delete all but the newest `keep` zips in a folder. Returns how many went.
pub fn prune(folder: &Path, keep: usize) -> io::Result<usize> {
    let mut all = zips(folder);
    all.sort_by_key(|(_, t)| std::cmp::Reverse(*t));
    let mut removed = 0;
    for (path, _) in all.into_iter().skip(keep.max(1)) {
        fs::remove_file(path)?;
        removed += 1;
    }
    Ok(removed)
}

/// Why an automatic backup is being considered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum When {
    /// The project is closing (or Writer quitting): back up if anything changed.
    Closing,
    /// A daily check: back up if the newest is a day old and anything changed.
    Daily,
}

/// An automatic backup, if one is due: into the project's folder under
/// `backups`, then pruned to the newest `keep`. `Ok(None)` when there was
/// nothing to do.
pub fn automatic(root: &Path, title: &str, backups: &Path, keep: usize, when: When) -> io::Result<Option<PathBuf>> {
    let folder = auto_folder(backups, root);
    let last = newest(&folder);
    if let Some((_, made)) = &last {
        let age = SystemTime::now().duration_since(*made).unwrap_or_default();
        if when == When::Daily && age < Duration::from_secs(24 * 60 * 60) {
            return Ok(None);
        }
        if !changed_since(root, *made) {
            return Ok(None);
        }
    }
    fs::create_dir_all(&folder)?;
    let name = file_name(title, chrono::Local::now().naive_local());
    let path = back_up(root, &folder, &name)?;
    prune(&folder, keep)?;
    Ok(Some(path))
}

fn write_zip(root: &Path, top: &str, to: &Path) -> io::Result<()> {
    let mut zip = zip::ZipWriter::new(fs::File::create(to)?);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    zip.add_directory(format!("{top}/"), options).map_err(io::Error::other)?;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let mut entries: Vec<_> = fs::read_dir(&dir)?.collect::<Result<_, _>>()?;
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let name = entry.file_name().to_string_lossy().into_owned();
            if skip(&name) {
                continue;
            }
            let path = entry.path();
            let rel = path.strip_prefix(root).map_err(io::Error::other)?;
            let inside = format!("{top}/{}", rel.to_string_lossy().replace('\\', "/"));
            let kind = entry.file_type()?;
            if kind.is_dir() {
                zip.add_directory(format!("{inside}/"), options).map_err(io::Error::other)?;
                stack.push(path);
            } else if kind.is_file() {
                zip.start_file(inside, options).map_err(io::Error::other)?;
                zip.write_all(&fs::read(&path)?)?;
            }
        }
    }
    zip.finish().map_err(io::Error::other)?.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_say_what_and_when() {
        let when = chrono::NaiveDate::from_ymd_opt(2026, 11, 22).unwrap().and_hms_opt(14, 5, 0).unwrap();
        assert_eq!(file_name("The Long Winter", when), "The Long Winter 2026-11-22 14.05.zip");
        assert_eq!(file_name("Either/Or: A Novel", when), "Either-Or- A Novel 2026-11-22 14.05.zip");
    }

    #[test]
    fn the_whole_project_goes_in_and_nothing_else() {
        let base = std::env::temp_dir().join(format!("sloth-writer-backup-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let root = base.join("Book");
        fs::create_dir_all(root.join("manuscript")).unwrap();
        fs::create_dir_all(root.join(".sloth")).unwrap();
        fs::write(root.join("project.ron"), "(title: \"Book\")").unwrap();
        fs::write(root.join("manuscript/one.md"), "Words.").unwrap();
        fs::write(root.join(".sloth/stats.ron"), "()").unwrap();
        fs::write(root.join(".DS_Store"), "junk").unwrap();
        let out = base.join("Backups");
        fs::create_dir_all(&out).unwrap();

        let zip_path = back_up(&root, &out, "Book.zip").unwrap();
        let mut zip = zip::ZipArchive::new(fs::File::open(&zip_path).unwrap()).unwrap();
        let mut names: Vec<String> = zip.file_names().map(String::from).collect();
        names.sort();
        assert_eq!(names, ["Book/", "Book/.sloth/", "Book/.sloth/stats.ron", "Book/manuscript/", "Book/manuscript/one.md", "Book/project.ron"]
            .map(String::from));
        let mut text = String::new();
        io::Read::read_to_string(&mut zip.by_name("Book/manuscript/one.md").unwrap(), &mut text).unwrap();
        assert_eq!(text, "Words.");
        assert_eq!(fs::read_dir(&out).unwrap().count(), 1, "no partial file left");

        // Not inside the project itself.
        assert!(back_up(&root, &root.join("manuscript"), "Book.zip").is_err());
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn automatic_backups_only_when_changed_and_keep_the_newest() {
        let base = std::env::temp_dir().join(format!("sloth-writer-auto-backup-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let root = base.join("Book");
        fs::create_dir_all(root.join("manuscript")).unwrap();
        fs::write(root.join("manuscript/one.md"), "Words.").unwrap();
        let backups = base.join("Backups");
        let folder = auto_folder(&backups, &root);

        // The first: there's none yet.
        assert!(automatic(&root, "Book", &backups, 2, When::Closing).unwrap().is_some());
        // Nothing changed since: nothing to do, closing or daily.
        assert!(automatic(&root, "Book", &backups, 2, When::Closing).unwrap().is_none());
        assert!(automatic(&root, "Book", &backups, 2, When::Daily).unwrap().is_none());
        // Daily waits a day even after a change; closing doesn't.
        std::thread::sleep(Duration::from_millis(1100));
        fs::write(root.join("manuscript/one.md"), "More words.").unwrap();
        assert!(automatic(&root, "Book", &backups, 2, When::Daily).unwrap().is_none());
        assert!(automatic(&root, "Book", &backups, 2, When::Closing).unwrap().is_some());

        // Pruning keeps the newest.
        for i in 0..3 {
            let path = folder.join(format!("Book 2020-01-0{} 10.00.zip", i + 1));
            fs::write(&path, "old").unwrap();
            let when = SystemTime::now() - Duration::from_secs(86_400 * (10 - i));
            fs::File::options().write(true).open(&path).unwrap().set_modified(when).unwrap();
        }
        // (The two real ones were made in the same minute, so the second
        // replaced the first: one real, three old.)
        assert_eq!(prune(&folder, 2).unwrap(), 2);
        let left: Vec<_> = zips(&folder).into_iter().map(|(p, _)| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(left.len(), 2);
        assert!(left.iter().any(|n| !n.starts_with("Book 2020")), "the newest stays: {left:?}");
        assert!(!left.iter().any(|n| n.starts_with("Book 2020-01-01")), "the oldest goes: {left:?}");
        let _ = fs::remove_dir_all(&base);
    }
}
