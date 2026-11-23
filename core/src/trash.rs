//! Moving a file to the Mac's Trash, where it can still be recovered
//! (Finder → Trash → Put Back) until the Trash is emptied.

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
pub fn move_to_trash(_path: &Path) -> io::Result<()> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "There's no Trash to move it to here."))
}
