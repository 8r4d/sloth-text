//! Bakes build information into the binary for the About window.

use std::process::Command;

fn main() {
    let git = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
    };

    let hash = git(&["rev-parse", "--short", "HEAD"]).unwrap_or_else(|| "unknown".into());
    let modified = git(&["status", "--porcelain"]).is_some_and(|s| !s.is_empty());
    let date = git(&["log", "-1", "--format=%cs"]).unwrap_or_default();

    println!(
        "cargo:rustc-env=SLOTH_GIT_HASH={hash}{}",
        if modified { "+modified" } else { "" }
    );
    println!("cargo:rustc-env=SLOTH_COMMIT_DATE={date}");

    // Re-run when commits are made or sources change.
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/index");
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=CHANGELOG.md");
}
