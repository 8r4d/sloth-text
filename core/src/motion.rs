//! Whether to animate: the system's setting for less motion turns the apps'
//! little animations off. On a Mac that's Reduce Motion (System Settings →
//! Accessibility → Display); on Windows, "Animation effects" (Settings →
//! Accessibility → Visual effects); on Linux, GNOME's animations setting.
//! Asked now and then rather than every frame, so it notices a change.

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long an answer is trusted before asking again.
const RECHECK: Duration = Duration::from_secs(5);

static LAST: Mutex<Option<(Instant, bool)>> = Mutex::new(None);

/// Animate? False when the Mac asks for reduced motion.
pub fn animate() -> bool {
    let Ok(mut last) = LAST.lock() else { return true };
    if let Some((at, reduce)) = *last
        && at.elapsed() < RECHECK
    {
        return !reduce;
    }
    let reduce = reduce_motion();
    *last = Some((Instant::now(), reduce));
    !reduce
}

#[cfg(all(target_os = "macos", not(test)))]
fn reduce_motion() -> bool {
    objc2_app_kit::NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion()
}

#[cfg(all(windows, not(test)))]
fn reduce_motion() -> bool {
    // SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION): whether Windows
    // animates at all. Declared here rather than adding a crate for it.
    #[link(name = "user32")]
    unsafe extern "system" {
        fn SystemParametersInfoW(action: u32, param: u32, value: *mut std::ffi::c_void, ini: u32) -> i32;
    }
    const SPI_GETCLIENTAREAANIMATION: u32 = 0x1042;
    let mut animate: i32 = 1;
    // SAFETY: the call writes one BOOL (an i32) to the pointer given.
    let ok = unsafe { SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, (&raw mut animate).cast(), 0) };
    ok != 0 && animate == 0
}

#[cfg(all(not(target_os = "macos"), not(windows), not(test)))]
fn reduce_motion() -> bool {
    // GNOME's setting (other desktops: animate).
    std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "enable-animations"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .is_some_and(|o| String::from_utf8_lossy(&o.stdout).trim() == "false")
}

// Tests always animate (and never ask the system).
#[cfg(test)]
fn reduce_motion() -> bool {
    false
}

/// A value easing from 0 to 1 over `duration` seconds from `start` (egui's
/// clock), smoothly at both ends; 1 straight away without animation.
pub fn ease(now: f64, start: f64, duration: f64) -> f32 {
    if !animate() || duration <= 0.0 {
        return 1.0;
    }
    let t = ((now - start) / duration).clamp(0.0, 1.0) as f32;
    t * t * (3.0 - 2.0 * t)
}
