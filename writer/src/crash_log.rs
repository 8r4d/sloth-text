//! If Writer ever stops on an error (a panic), what went wrong is added to
//! `crash.log` in `~/Library/Logs/Sloth Writer` (on a Mac; see places.rs
//! for Windows and Linux): when, which version, the
//! message, where in the code, and the backtrace. Opened from Finder, the
//! app's error output goes nowhere, so without this it would be lost.
//!
//! The log stays small: past `MAX_BYTES` it's moved to `crash.old.log`
//! (replacing the one before) and a new one is started.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const FILE: &str = "crash.log";
const OLD_FILE: &str = "crash.old.log";
const MAX_BYTES: u64 = 256 * 1024;

/// Where the log goes: `~/Library/Logs/Sloth Writer` on a Mac (see
/// places.rs for the others).
pub fn dir() -> Option<PathBuf> {
    crate::places::log_dir()
}

/// Record panics in the log, then report them as usual (to the terminal,
/// if there is one).
pub fn install() {
    let Some(dir) = dir() else { return };
    let usual = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| (*s).to_owned())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "(no message)".to_owned());
        let place = info.location().map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column())).unwrap_or_default();
        let backtrace = std::backtrace::Backtrace::force_capture().to_string();
        let when = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        // A log that can't be written mustn't stop the usual report.
        let _ = append(&dir, &entry(&when, &message, &place, &backtrace));
        usual(info);
    }));
}

/// One crash, as written to the log.
fn entry(when: &str, message: &str, place: &str, backtrace: &str) -> String {
    let thread = std::thread::current().name().unwrap_or("unnamed").to_owned();
    format!(
        "===== {when} \u{b7} Sloth Writer {} \u{b7} {}\n{message}\nat {place} (thread {thread})\n\n{backtrace}\n",
        crate::version::VERSION,
        crate::version::build_info(),
    )
}

fn append(dir: &Path, text: &str) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let path = dir.join(FILE);
    if fs::metadata(&path).is_ok_and(|m| m.len() > MAX_BYTES) {
        fs::rename(&path, dir.join(OLD_FILE))?;
    }
    let mut file = fs::OpenOptions::new().create(true).append(true).open(&path)?;
    file.write_all(text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crashes_are_added_and_the_log_stays_small() {
        let dir = std::env::temp_dir().join(format!("sloth-writer-crash-log-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let one = entry("2026-11-24 09:00:00", "min > max", "writer/src/canvas_ui.rs:511:5", "0: main");
        assert!(one.starts_with("===== 2026-11-24 09:00:00 \u{b7} Sloth Writer "), "{one}");
        assert!(one.contains("min > max\nat writer/src/canvas_ui.rs:511:5"), "{one}");
        append(&dir, &one).unwrap();
        append(&dir, "second\n").unwrap();
        let log = fs::read_to_string(dir.join(FILE)).unwrap();
        assert!(log.starts_with("=====") && log.ends_with("second\n"));

        // Too big: moved aside, and a new one started.
        fs::write(dir.join(FILE), "x".repeat(MAX_BYTES as usize + 1)).unwrap();
        append(&dir, "fresh\n").unwrap();
        assert_eq!(fs::read_to_string(dir.join(FILE)).unwrap(), "fresh\n");
        assert_eq!(fs::metadata(dir.join(OLD_FILE)).unwrap().len(), MAX_BYTES + 1);
        let _ = fs::remove_dir_all(&dir);
    }
}
