//! Files in a project that its binder doesn't list: usually a synced
//! folder's conflict copies, made when two computers changed the same
//! document while out of touch (see CROSS-PLATFORM.md, phase 3). Writer
//! would otherwise never show them. Each service names them its own way:
//!
//! - Dropbox: `walking (Writing PC's conflicted copy 2026-11-25).md`
//! - OneDrive: `walking-DESKTOP-1A2B3C.md`
//! - iCloud: `walking 2.md`
//! - Syncthing: `walking.sync-conflict-20261125-140512-ABC1234.md`
//!
//! A stray is matched to the document it's a copy of by name, and brought
//! into the binder just after it, to compare and keep what's wanted.

use std::fs;
use std::path::Path;

use crate::project::{MANIFEST_FILE, Place, Project};

/// A document file the binder doesn't list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stray {
    /// Its path in the project, `manuscript/walking 2.md`.
    pub rel: String,
    /// The document it's a copy of, if its name says so.
    pub copy_of: Option<String>,
    /// The computer it came from, where the name says (Dropbox, OneDrive).
    pub from: Option<String>,
}

/// Every stray document file in the project's folders.
pub fn find(project: &Project) -> Vec<Stray> {
    // Every document, the Trash's too: those files are listed, just trashed.
    let listed: Vec<(&str, &str)> = project.all_documents().iter().filter_map(|n| Some((n.path.as_deref()?, n.id.as_str()))).collect();
    let mut out = Vec::new();
    for top in &project.manifest.binder {
        let dir = &top.id;
        let Ok(entries) = fs::read_dir(project.root.join(dir)) else { continue };
        let mut names: Vec<String> = entries.filter_map(|e| e.ok()?.file_name().into_string().ok()).collect();
        names.sort();
        for name in names {
            let rel = format!("{dir}/{name}");
            let is_document = Path::new(&name).extension().is_some_and(|x| x.eq_ignore_ascii_case("md"));
            if !is_document || name.starts_with('.') || listed.iter().any(|(p, _)| p.eq_ignore_ascii_case(&rel)) {
                continue;
            }
            let stem = &name[..name.len() - 3];
            // The listed document in the same folder whose name it starts
            // with (the longest, so `walking-home 2` is Walking Home's).
            let original = listed
                .iter()
                .filter_map(|(p, id)| {
                    let theirs = p.strip_prefix(&format!("{dir}/"))?.strip_suffix(".md")?;
                    let rest = stem.strip_prefix(theirs)?;
                    rest.starts_with([' ', '-', '.', '(', '_']).then_some((theirs.len(), *id, rest))
                })
                .max_by_key(|(len, _, _)| *len);
            let (copy_of, from) = match original {
                Some((_, id, rest)) => (Some(id.to_owned()), computer(rest)),
                None => (None, None),
            };
            out.push(Stray { rel, copy_of, from });
        }
    }
    out
}

/// The computer a conflict copy's name gives, if it gives one: Dropbox's
/// " (Writing PC's conflicted copy …)", OneDrive's "-DESKTOP-1A2B3C".
fn computer(rest: &str) -> Option<String> {
    if let Some(inner) = rest.strip_prefix(" (")
        && let Some(at) = inner.find("'s conflicted copy").or_else(|| inner.find("\u{2019}s conflicted copy"))
    {
        return Some(inner[..at].to_owned());
    }
    // OneDrive adds the computer's name, which has capitals; Writer's own
    // file names never do.
    let name = rest.strip_prefix('-')?;
    (name.chars().any(|c| c.is_uppercase()) && !name.contains(' ')).then(|| name.to_owned())
}

/// Copies of the project file itself (`project (… conflicted copy).ron`,
/// `project 2.ron`): changes to the binder made on another computer that
/// aren't showing here.
pub fn project_file_copies(root: &Path) -> Vec<String> {
    let stem = MANIFEST_FILE.trim_end_matches(".ron");
    let Ok(entries) = fs::read_dir(root) else { return Vec::new() };
    let mut out: Vec<String> = entries
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n != MANIFEST_FILE && n.ends_with(".ron") && n.starts_with(stem) && n[stem.len()..].starts_with([' ', '-', '.', '(', '_']))
        .collect();
    out.sort();
    out
}

/// Bring strays into the binder: each copy just after its original, titled
/// to say what it is; the rest at the end of their folder. Returns the new
/// items' ids.
pub fn bring_in(project: &mut Project, strays: &[Stray]) -> std::io::Result<Vec<String>> {
    let mut ids = Vec::new();
    for stray in strays {
        let stem = Path::new(&stray.rel).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let original = stray.copy_of.as_deref().and_then(|id| project.node(id)).map(|n| (n.id.clone(), n.title.clone()));
        let (title, place) = match original {
            Some((id, title)) => {
                let from = stray.from.as_deref().map_or("another computer".to_owned(), |f| f.to_owned());
                (format!("{title} (copy from {from})"), Place::After(id))
            }
            None => {
                let dir = stray.rel.split('/').next().unwrap_or("manuscript").to_owned();
                (stem.clone(), Place::Into(dir))
            }
        };
        ids.push(project.add_existing(&place, &title, &stray.rel)?);
    }
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conflict_copies_are_found_and_matched_to_their_originals() {
        let dir = std::env::temp_dir().join(format!("sloth-writer-strays-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let mut p = Project::create(&dir, "Book").unwrap();
        let walking = p.add_document(&Place::Into("manuscript".into()), "Walking").unwrap();
        let home = p.add_document(&Place::Into("manuscript".into()), "Walking Home").unwrap();
        // A trashed document's file is in the trash folder, and listed.
        let cut = p.add_document(&Place::Into("manuscript".into()), "Cut scene").unwrap();
        p.trash(&cut).unwrap();
        for name in [
            "walking (Writing PC's conflicted copy 2026-11-25).md",
            "walking-DESKTOP-1A2B3C.md",
            "walking-home 2.md",
            "walking.sync-conflict-20261125-140512-ABC1234.md",
            "ideas.md",
            ".DS_Store",
            "notes.txt",
        ] {
            fs::write(dir.join("manuscript").join(name), "text").unwrap();
        }
        fs::write(dir.join("project (Writing PC's conflicted copy).ron"), "()").unwrap();

        let found = find(&p);
        let summary: Vec<(&str, Option<&str>, Option<&str>)> =
            found.iter().map(|s| (s.rel.as_str(), s.copy_of.as_deref(), s.from.as_deref())).collect();
        assert_eq!(
            summary,
            [
                ("manuscript/ideas.md", None, None),
                ("manuscript/walking (Writing PC's conflicted copy 2026-11-25).md", Some(walking.as_str()), Some("Writing PC")),
                ("manuscript/walking-DESKTOP-1A2B3C.md", Some(walking.as_str()), Some("DESKTOP-1A2B3C")),
                ("manuscript/walking-home 2.md", Some(home.as_str()), None),
                ("manuscript/walking.sync-conflict-20261125-140512-ABC1234.md", Some(walking.as_str()), None),
            ]
        );
        assert_eq!(project_file_copies(&dir), ["project (Writing PC's conflicted copy).ron"]);

        // Brought in: each copy just after its original; then nothing's stray.
        bring_in(&mut p, &found).unwrap();
        assert!(find(&p).is_empty());
        let titles: Vec<&str> = p.manifest.binder[0].children.iter().map(|n| n.title.as_str()).collect();
        assert_eq!(
            titles,
            [
                "Untitled",
                "Walking",
                "Walking (copy from another computer)",
                "Walking (copy from DESKTOP-1A2B3C)",
                "Walking (copy from Writing PC)",
                "Walking Home",
                "Walking Home (copy from another computer)",
                "ideas",
            ]
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
