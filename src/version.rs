//! Version and build information.

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Short git commit hash, with `+modified` if built from uncommitted changes.
pub const GIT_HASH: &str = env!("SLOTH_GIT_HASH");
pub const COMMIT_DATE: &str = env!("SLOTH_COMMIT_DATE");
pub const CHANGELOG: &str = include_str!("../CHANGELOG.md");

/// e.g. "Build a1b2c3d (2026-11-21)"
pub fn build_info() -> String {
    if COMMIT_DATE.is_empty() {
        format!("Build {GIT_HASH}")
    } else {
        format!("Build {GIT_HASH} ({COMMIT_DATE})")
    }
}
