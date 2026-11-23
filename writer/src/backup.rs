//! Back Up: the whole project folder (documents, snapshots, the project
//! file, sync record and statistics) as one dated zip file somewhere else,
//! such as another drive or a cloud folder.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

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
}
