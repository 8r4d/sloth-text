//! Where Writer keeps its own files (backups, the crash log), in each
//! system's usual place:
//!
//! | | Mac | Windows | Linux |
//! |---|---|---|---|
//! | data | `~/Library/Application Support/Sloth Writer` | `%LOCALAPPDATA%\Sloth Writer` | `~/.local/share/sloth-writer` |
//! | logs | `~/Library/Logs/Sloth Writer` | `%LOCALAPPDATA%\Sloth Writer\Logs` | `~/.local/state/sloth-writer` |
//!
//! On Linux, `XDG_DATA_HOME` and `XDG_STATE_HOME` are followed when set.
//! On Windows it's the local app data, not the roaming one: backups are
//! big, and shouldn't be copied around a network at each sign-in.

use std::env;
use std::path::PathBuf;

/// Writer's own data folder. `None` only if the system says nothing of
/// where home is.
pub fn data_dir() -> Option<PathBuf> {
    pick(&Env::real(), Kind::Data)
}

/// Where Writer's logs go.
pub fn log_dir() -> Option<PathBuf> {
    pick(&Env::real(), Kind::Logs)
}

#[derive(Clone, Copy)]
enum Kind {
    Data,
    Logs,
}

/// What the choice depends on, so tests can try each system.
struct Env {
    os: &'static str,
    var: fn(&str) -> Option<String>,
}

impl Env {
    fn real() -> Self {
        Env { os: env::consts::OS, var: |name| env::var(name).ok().filter(|v| !v.is_empty()) }
    }
}

fn pick(env: &Env, kind: Kind) -> Option<PathBuf> {
    let var = |name: &str| (env.var)(name).map(PathBuf::from);
    match env.os {
        "macos" => {
            let home = var("HOME")?;
            Some(match kind {
                Kind::Data => home.join("Library/Application Support/Sloth Writer"),
                Kind::Logs => home.join("Library/Logs/Sloth Writer"),
            })
        }
        "windows" => {
            let base = var("LOCALAPPDATA").or_else(|| var("USERPROFILE").map(|p| p.join("AppData").join("Local")))?;
            let dir = base.join("Sloth Writer");
            Some(match kind {
                Kind::Data => dir,
                Kind::Logs => dir.join("Logs"),
            })
        }
        _ => {
            let (xdg, fallback) = match kind {
                Kind::Data => ("XDG_DATA_HOME", ".local/share"),
                Kind::Logs => ("XDG_STATE_HOME", ".local/state"),
            };
            // The XDG rules: a relative path there is to be ignored.
            let base = var(xdg).filter(|p| p.is_absolute()).or_else(|| var("HOME").map(|h| h.join(fallback)))?;
            Some(base.join("sloth-writer"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn on(os: &'static str, var: fn(&str) -> Option<String>) -> (Option<PathBuf>, Option<PathBuf>) {
        let env = Env { os, var };
        (pick(&env, Kind::Data), pick(&env, Kind::Logs))
    }

    #[test]
    fn the_mac_is_as_it_was() {
        let (data, logs) = on("macos", |v| (v == "HOME").then(|| "/Users/ann".into()));
        assert_eq!(data.unwrap(), PathBuf::from("/Users/ann/Library/Application Support/Sloth Writer"));
        assert_eq!(logs.unwrap(), PathBuf::from("/Users/ann/Library/Logs/Sloth Writer"));
    }

    #[test]
    fn windows_uses_local_app_data() {
        let (data, logs) = on("windows", |v| (v == "LOCALAPPDATA").then(|| "C:/Users/ann/AppData/Local".into()));
        assert_eq!(data.unwrap(), PathBuf::from("C:/Users/ann/AppData/Local/Sloth Writer"));
        assert_eq!(logs.unwrap(), PathBuf::from("C:/Users/ann/AppData/Local/Sloth Writer/Logs"));
        // No HOME on Windows, and that's fine.
        let (data, _) = on("windows", |v| (v == "USERPROFILE").then(|| "C:/Users/ann".into()));
        assert_eq!(data.unwrap(), PathBuf::from("C:/Users/ann/AppData/Local/Sloth Writer"));
        assert_eq!(on("windows", |v| (v == "HOME").then(|| "/home/ann".into())), (None, None));
    }

    #[test]
    fn linux_follows_xdg() {
        let (data, logs) = on("linux", |v| (v == "HOME").then(|| "/home/ann".into()));
        assert_eq!(data.unwrap(), PathBuf::from("/home/ann/.local/share/sloth-writer"));
        assert_eq!(logs.unwrap(), PathBuf::from("/home/ann/.local/state/sloth-writer"));
        let (data, logs) = on("linux", |v| match v {
            "HOME" => Some("/home/ann".into()),
            "XDG_DATA_HOME" => Some("/data".into()),
            "XDG_STATE_HOME" => Some("relative/ignored".into()),
            _ => None,
        });
        assert_eq!(data.unwrap(), PathBuf::from("/data/sloth-writer"));
        assert_eq!(logs.unwrap(), PathBuf::from("/home/ann/.local/state/sloth-writer"));
    }
}
