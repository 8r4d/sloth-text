//! The helper token, kept where each system keeps secrets rather than in a
//! settings file: the macOS Keychain (through the system's `security`
//! tool), Windows' Credential Manager, and on Linux a file in Writer's data
//! folder only you can read.

const SERVICE: &str = "Sloth Writer helper";
const ACCOUNT: &str = "token";

/// The saved token, or an empty string.
pub fn load_token() -> String {
    system::load().map(|t| t.trim().to_owned()).unwrap_or_default()
}

/// Save the token (or remove it, if empty). Returns whether it worked.
pub fn save_token(token: &str) -> bool {
    let token = token.trim();
    if token.is_empty() {
        system::delete();
        return true;
    }
    system::save(token)
}

#[cfg(target_os = "macos")]
mod system {
    use super::{ACCOUNT, SERVICE};
    use std::process::Command;

    fn security(args: &[&str]) -> Option<std::process::Output> {
        Command::new("/usr/bin/security").args(args).output().ok().filter(|o| o.status.success())
    }

    pub fn load() -> Option<String> {
        security(&["find-generic-password", "-s", SERVICE, "-a", ACCOUNT, "-w"]).map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
    }

    pub fn save(token: &str) -> bool {
        security(&["add-generic-password", "-U", "-s", SERVICE, "-a", ACCOUNT, "-w", token]).is_some()
    }

    pub fn delete() {
        let _ = security(&["delete-generic-password", "-s", SERVICE, "-a", ACCOUNT]);
    }
}

#[cfg(windows)]
mod system {
    use super::{ACCOUNT, SERVICE};

    fn entry() -> Option<keyring::Entry> {
        keyring::Entry::new(SERVICE, ACCOUNT).ok()
    }

    pub fn load() -> Option<String> {
        entry()?.get_password().ok()
    }

    pub fn save(token: &str) -> bool {
        entry().is_some_and(|e| e.set_password(token).is_ok())
    }

    pub fn delete() {
        if let Some(e) = entry() {
            let _ = e.delete_credential();
        }
    }
}

/// Linux and the rest: a file only this user can read. (The desktop's
/// keyring would be better, but needs D-Bus libraries to build.)
#[cfg(not(any(target_os = "macos", windows)))]
mod system {
    use std::fs;
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    use std::path::PathBuf;

    fn path() -> Option<PathBuf> {
        crate::places::data_dir().map(|d| d.join("helper-token"))
    }

    pub fn load() -> Option<String> {
        fs::read_to_string(path()?).ok()
    }

    pub fn save(token: &str) -> bool {
        let Some(path) = path() else { return false };
        let write = || -> std::io::Result<()> {
            if let Some(dir) = path.parent() {
                fs::create_dir_all(dir)?;
            }
            let _ = fs::remove_file(&path); // so the new one is made private
            let mut file = fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&path)?;
            file.write_all(token.as_bytes())
        };
        write().is_ok()
    }

    pub fn delete() {
        if let Some(path) = path() {
            let _ = fs::remove_file(path);
        }
    }
}
