//! Keeping two copies of Writer from editing one project at once (on two
//! computers sharing it through iCloud or Dropbox, say, or two windows here),
//! which could otherwise save over each other.
//!
//! While a project is open, `.sloth/lock.ron` in it names who has it open
//! and is refreshed every so often (`beat`). Opening a project another copy
//! has open (its lock refreshed recently) asks first. A lock left by a copy
//! that quit unexpectedly goes stale and is ignored.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

/// The lock's file name, inside the project's `.sloth` folder.
pub const FILE_NAME: &str = "lock.ron";

/// How often an open project's lock is refreshed (seconds).
pub const BEAT_SECONDS: f64 = 30.0;

/// A lock not refreshed for this long belongs to a copy that's gone.
const STALE_SECONDS: i64 = 120;

/// The same, for a lock from another computer: much longer, since its clock
/// and this one's may differ by minutes (and a synced folder can bring the
/// lock late). Within this, opening there asks first.
const STALE_ELSEWHERE_SECONDS: i64 = 30 * 60;

/// Whether a lock someone else holds is still live: refreshed recently, by
/// this computer's clock, or (from another computer) by either clock.
fn live(lock: &Lock) -> bool {
    let age = now() - lock.beat;
    if lock.host == host() { age < STALE_SECONDS } else { age.abs() < STALE_ELSEWHERE_SECONDS }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lock {
    /// This running copy of Writer (made fresh each launch).
    pub owner: String,
    /// The computer's name, to say where it's open.
    pub host: String,
    /// When it was opened, and last refreshed (seconds since 1970).
    pub since: i64,
    pub beat: i64,
}

/// Who has a project open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Holder {
    /// Nobody (or a copy that's gone).
    Nobody,
    /// This copy of Writer.
    Us,
    /// Another copy, here or on another computer.
    Other(Lock),
}

fn path(root: &Path) -> PathBuf {
    root.join(".sloth").join(FILE_NAME)
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

/// This running copy of Writer.
pub fn instance() -> &'static str {
    static ID: OnceLock<String> = OnceLock::new();
    ID.get_or_init(|| format!("{}-{}", std::process::id(), crate::project::random_id(8)))
}

/// This computer's name: on a Mac as in System Settings → General →
/// Sharing, on Windows and Linux its host name.
pub fn host() -> &'static str {
    static HOST: OnceLock<String> = OnceLock::new();
    HOST.get_or_init(|| computer_name().unwrap_or_else(|| "another computer".into()))
}

fn computer_name() -> Option<String> {
    let tidy = |s: String| Some(s.trim().to_owned()).filter(|s| !s.is_empty());
    if cfg!(target_os = "macos") {
        let run = |cmd: &str, args: &[&str]| {
            let out = std::process::Command::new(cmd).args(args).output().ok().filter(|o| o.status.success())?;
            tidy(String::from_utf8_lossy(&out.stdout).into_owned())
        };
        run("/usr/sbin/scutil", &["--get", "ComputerName"]).or_else(|| run("hostname", &[]))
    } else if cfg!(windows) {
        std::env::var("COMPUTERNAME").ok().and_then(tidy)
    } else {
        ["/proc/sys/kernel/hostname", "/etc/hostname"].into_iter().find_map(|f| fs::read_to_string(f).ok().and_then(tidy))
    }
}

fn read(root: &Path) -> Option<Lock> {
    ron::from_str(&fs::read_to_string(path(root)).ok()?).ok()
}

fn write(root: &Path, lock: &Lock) -> io::Result<()> {
    let file = path(root);
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir)?;
    }
    let text = ron::to_string(lock).map_err(io::Error::other)?;
    sloth_core::document::write_atomically(&file, text.as_bytes())
}

/// Who has the project at `root` open.
pub fn holder(root: &Path) -> Holder {
    match read(root) {
        Some(lock) if lock.owner == instance() => Holder::Us,
        Some(lock) if live(&lock) => Holder::Other(lock),
        _ => Holder::Nobody,
    }
}

/// Mark the project as open here (taking it from anyone else).
pub fn take(root: &Path) -> io::Result<()> {
    let t = now();
    write(root, &Lock { owner: instance().to_owned(), host: host().to_owned(), since: t, beat: t })
}

/// Refresh the lock. `Ok(Some(lock))` if another copy has taken the
/// project meanwhile (the lock is left alone then).
pub fn beat(root: &Path) -> io::Result<Option<Lock>> {
    match read(root) {
        Some(lock) if lock.owner == instance() => {
            write(root, &Lock { beat: now(), ..lock })?;
            Ok(None)
        }
        Some(lock) if live(&lock) => Ok(Some(lock)),
        _ => take(root).map(|()| None),
    }
}

/// The project is closing here: let it go, if it's still ours.
pub fn release(root: &Path) {
    if read(root).is_some_and(|lock| lock.owner == instance()) {
        let _ = fs::remove_file(path(root));
    }
}

/// "since 14:05" (today) or "since 21 November, 14:05".
pub fn since(lock: &Lock) -> String {
    use chrono::TimeZone;
    let Some(when) = chrono::Local.timestamp_opt(lock.since, 0).single() else { return String::new() };
    if when.date_naive() == chrono::Local::now().date_naive() {
        when.format("since %H:%M").to_string()
    } else {
        when.format("since %-d %B, %H:%M").to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("sloth-writer-lock-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_project_is_ours_until_another_copy_takes_it() {
        let root = dir("take");
        assert_eq!(holder(&root), Holder::Nobody);
        take(&root).unwrap();
        assert_eq!(holder(&root), Holder::Us);
        assert_eq!(beat(&root).unwrap(), None);

        // Another copy (another Mac) opens it anyway.
        let theirs = Lock { owner: "someone-else".into(), host: "Studio iMac".into(), since: now(), beat: now() };
        write(&root, &theirs).unwrap();
        assert_eq!(holder(&root), Holder::Other(theirs.clone()));
        assert_eq!(beat(&root).unwrap(), Some(theirs.clone()), "noticed, and left alone");
        assert_eq!(read(&root), Some(theirs));
        // Closing here doesn't remove their lock.
        release(&root);
        assert!(read(&root).is_some());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn another_computers_lock_holds_through_clocks_that_differ() {
        let root = dir("skew");
        // Refreshed "ten minutes ago" by this clock: the other computer's
        // clock is behind, or the synced folder was slow. Still theirs.
        let then = now() - 600;
        let theirs = Lock { owner: "pc".into(), host: format!("{} (elsewhere)", host()), since: then, beat: then };
        write(&root, &theirs).unwrap();
        assert_eq!(holder(&root), Holder::Other(theirs.clone()));
        // A clock ahead of this one, too.
        let ahead = Lock { beat: now() + 600, ..theirs };
        write(&root, &ahead).unwrap();
        assert_eq!(holder(&root), Holder::Other(ahead));
        // Hours old: gone.
        let old = now() - STALE_ELSEWHERE_SECONDS - 5;
        write(&root, &Lock { owner: "pc".into(), host: "Writing PC".into(), since: old, beat: old }).unwrap();
        assert_eq!(holder(&root), Holder::Nobody);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_lock_left_behind_goes_stale() {
        let root = dir("stale");
        let old = now() - STALE_SECONDS - 5;
        // Left by a copy on this computer: two minutes is enough to tell.
        write(&root, &Lock { owner: "gone".into(), host: host().into(), since: old, beat: old }).unwrap();
        assert_eq!(holder(&root), Holder::Nobody);
        assert_eq!(beat(&root).unwrap(), None, "taken over");
        assert_eq!(holder(&root), Holder::Us);
        release(&root);
        assert_eq!(read(&root), None);
        let _ = fs::remove_dir_all(&root);
    }
}
