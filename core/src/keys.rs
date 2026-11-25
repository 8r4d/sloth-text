//! Keyboard shortcuts as each system shows them. The app's labels are
//! written the Mac way (⌘K, ⌥⌘N, ⇧⌘F); on Windows and Linux `shown` turns
//! them into Ctrl+K, Ctrl+Shift+N, Ctrl+Shift+F.
//!
//! The Mac's ⌥⌘ shortcuts aren't Ctrl+Alt there: many Windows keyboard
//! layouts type characters such as @ € { } with Ctrl+Alt (AltGr), so they're
//! Ctrl+Shift instead (`OPT_CMD`). The one that would clash, ⌥⌘F for Focus
//! mode against Ctrl+Shift+F for Find in Project, is F11 there (`FOCUS`).

use eframe::egui::{Key, KeyboardShortcut, Modifiers};

/// The app is running on a Mac.
pub const MAC: bool = cfg!(target_os = "macos");

/// ⌥⌘ on the Mac, Ctrl+Shift elsewhere.
pub const OPT_CMD: Modifiers = if MAC { Modifiers::COMMAND.plus(Modifiers::ALT) } else { Modifiers::COMMAND.plus(Modifiers::SHIFT) };

/// Focus mode: ⌥⌘F on the Mac, F11 elsewhere.
pub const FOCUS: KeyboardShortcut =
    if MAC { KeyboardShortcut::new(OPT_CMD, Key::F) } else { KeyboardShortcut::new(Modifiers::NONE, Key::F11) };

/// A label written with the Mac's shortcuts, as this system shows them.
pub fn shown(text: &str) -> String {
    if MAC { text.to_owned() } else { for_pc(text) }
}

/// Every ⌘ ⌥ ⇧ ⌃ shortcut in `text` the way Windows and Linux write it.
pub fn for_pc(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if !is_modifier(c) {
            out.push(c);
            continue;
        }
        let (mut cmd, mut opt, mut shift, mut ctrl) = (false, false, false, false);
        let mut set = |c| match c {
            '\u{2318}' => cmd = true,
            '\u{2325}' => opt = true,
            '\u{21e7}' => shift = true,
            _ => ctrl = true,
        };
        set(c);
        while let Some(&next) = chars.peek().filter(|&&n| is_modifier(n)) {
            set(next);
            chars.next();
        }
        // The key itself; "⌘-click" has a gesture after a hyphen instead.
        let key = match chars.peek() {
            Some('-') => {
                chars.next();
                None
            }
            Some(&k) if !k.is_whitespace() && k != ')' => {
                chars.next();
                Some(k)
            }
            _ => None,
        };
        if cmd && opt && key == Some('F') {
            out.push_str("F11");
            continue;
        }
        let mut names = Vec::new();
        if cmd || ctrl {
            names.push("Ctrl");
        }
        // ⌥⌘ is Ctrl+Shift here; ⌥ alone is Alt.
        if shift || (opt && cmd) {
            names.push("Shift");
        }
        if opt && !cmd {
            names.push("Alt");
        }
        out.push_str(&names.join("+"));
        if key.is_some() || chars.peek().is_some_and(|c| c.is_alphabetic()) {
            out.push('+');
        }
        if let Some(k) = key {
            out.push(k);
        }
    }
    out
}

fn is_modifier(c: char) -> bool {
    matches!(c, '\u{2318}' | '\u{2325}' | '\u{21e7}' | '\u{2303}')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mac_shortcuts_written_for_a_pc() {
        assert_eq!(for_pc("\u{2318}K"), "Ctrl+K");
        assert_eq!(for_pc("\u{21e7}\u{2318}F"), "Ctrl+Shift+F");
        assert_eq!(for_pc("\u{2325}\u{2318}N"), "Ctrl+Shift+N");
        assert_eq!(for_pc("\u{2325}\u{2318},"), "Ctrl+Shift+,");
        assert_eq!(for_pc("\u{2318}?"), "Ctrl+?");
        assert_eq!(for_pc("\u{2318}\\"), "Ctrl+\\");
        assert_eq!(for_pc("Focus mode is \u{2325}\u{2318}F."), "Focus mode is F11.");
        assert_eq!(for_pc("Add a folder (\u{2325}\u{2318}N)"), "Add a folder (Ctrl+Shift+N)");
        assert_eq!(for_pc("\u{2318}1, \u{2318}2, \u{2318}3"), "Ctrl+1, Ctrl+2, Ctrl+3");
        assert_eq!(for_pc("\u{2318}-click to open"), "Ctrl+click to open");
        assert_eq!(for_pc("Or \u{2325}-click it"), "Or Alt+click it");
        assert_eq!(for_pc("pinch or \u{2318}-scroll to zoom"), "pinch or Ctrl+scroll to zoom");
        assert_eq!(for_pc("Highlight Repeated Words  \u{2325}\u{2318}R"), "Highlight Repeated Words  Ctrl+Shift+R");
        assert_eq!(for_pc("no shortcut here"), "no shortcut here");
    }

    #[test]
    fn the_mac_keeps_its_symbols() {
        if MAC {
            assert_eq!(shown("\u{2325}\u{2318}N"), "\u{2325}\u{2318}N");
        } else {
            assert_eq!(shown("\u{2325}\u{2318}N"), "Ctrl+Shift+N");
        }
    }
}
