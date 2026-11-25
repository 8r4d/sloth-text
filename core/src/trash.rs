//! Moving a file to the system's trash, where it can still be recovered
//! until that's emptied: the Mac's Trash (Finder → Trash → Put Back), the
//! Recycle Bin on Windows, the desktop's trash on Linux.

use std::io;
use std::path::Path;

#[cfg(target_os = "macos")]
pub fn move_to_trash(path: &Path) -> io::Result<()> {
    use objc2_foundation::{NSFileManager, NSString, NSURL};
    let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
    NSFileManager::defaultManager()
        .trashItemAtURL_resultingItemURL_error(&url, None)
        .map_err(|e| io::Error::other(e.localizedDescription().to_string()))
}

#[cfg(not(target_os = "macos"))]
pub fn move_to_trash(path: &Path) -> io::Result<()> {
    trash::delete(path).map_err(|e| io::Error::other(e.to_string()))
}
