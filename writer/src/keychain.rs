//! The helper token, kept in the macOS Keychain rather than in a settings
//! file, through the system's `security` tool.

use std::process::Command;

const SERVICE: &str = "Sloth Writer helper";
const ACCOUNT: &str = "token";

/// The saved token, or an empty string.
pub fn load_token() -> String {
    Command::new("/usr/bin/security")
        .args(["find-generic-password", "-s", SERVICE, "-a", ACCOUNT, "-w"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_default()
}

/// Save the token (or remove it, if empty). Returns whether it worked.
pub fn save_token(token: &str) -> bool {
    let token = token.trim();
    if token.is_empty() {
        let _ = Command::new("/usr/bin/security")
            .args(["delete-generic-password", "-s", SERVICE, "-a", ACCOUNT])
            .output();
        return true;
    }
    Command::new("/usr/bin/security")
        .args(["add-generic-password", "-U", "-s", SERVICE, "-a", ACCOUNT, "-w", token])
        .output()
        .is_ok_and(|o| o.status.success())
}
