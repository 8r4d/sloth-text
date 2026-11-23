//! Whether to animate: macOS's Reduce Motion (System Settings →
//! Accessibility → Display) turns the apps' little animations off. Asked
//! now and then rather than every frame, so it notices a change.

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

// Tests always animate (and never touch AppKit).
#[cfg(any(not(target_os = "macos"), test))]
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
