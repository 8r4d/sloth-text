//! Every user-triggerable action lives here as a `Command`.
//!
//! Menus and keyboard shortcuts are both generated from this list, so adding
//! a feature is:
//!   1. add a variant to `Command`
//!   2. give it a label (and optionally a shortcut) below
//!   3. handle it in `SlothApp::execute` (app.rs)
//!   4. list it in a menu in `MENUS`

use eframe::egui::{Key, KeyboardShortcut, Modifiers};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    New,
    Open,
    Save,
    SaveAs,
    Quit,
    ToggleLineNumbers,
    ToggleWordWrap,
    ZoomIn,
    ZoomOut,
    ZoomReset,
}

/// Menu layout: (menu title, commands). `None` draws a separator.
pub const MENUS: &[(&str, &[Option<Command>])] = &[
    (
        "File",
        &[
            Some(Command::New),
            Some(Command::Open),
            None,
            Some(Command::Save),
            Some(Command::SaveAs),
            None,
            Some(Command::Quit),
        ],
    ),
    (
        "View",
        &[
            Some(Command::ToggleLineNumbers),
            Some(Command::ToggleWordWrap),
            None,
            Some(Command::ZoomIn),
            Some(Command::ZoomOut),
            Some(Command::ZoomReset),
        ],
    ),
];

/// All commands, used for shortcut dispatch.
pub const ALL: &[Command] = &[
    Command::New,
    Command::Open,
    Command::Save,
    Command::SaveAs,
    Command::Quit,
    Command::ToggleLineNumbers,
    Command::ToggleWordWrap,
    Command::ZoomIn,
    Command::ZoomOut,
    Command::ZoomReset,
];

impl Command {
    pub fn label(self) -> &'static str {
        match self {
            Command::New => "New",
            Command::Open => "Open…",
            Command::Save => "Save",
            Command::SaveAs => "Save As…",
            Command::Quit => "Quit",
            Command::ToggleLineNumbers => "Line Numbers",
            Command::ToggleWordWrap => "Word Wrap",
            Command::ZoomIn => "Zoom In",
            Command::ZoomOut => "Zoom Out",
            Command::ZoomReset => "Actual Size",
        }
    }

    /// `Modifiers::COMMAND` is ⌘ on macOS and Ctrl elsewhere.
    pub fn shortcut(self) -> Option<KeyboardShortcut> {
        let cmd = Modifiers::COMMAND;
        let cmd_shift = Modifiers::COMMAND | Modifiers::SHIFT;
        let cmd_alt = Modifiers::COMMAND | Modifiers::ALT;
        let sc = |m, k| Some(KeyboardShortcut::new(m, k));
        match self {
            Command::New => sc(cmd, Key::N),
            Command::Open => sc(cmd, Key::O),
            Command::Save => sc(cmd, Key::S),
            Command::SaveAs => sc(cmd_shift, Key::S),
            Command::Quit => sc(cmd, Key::Q),
            Command::ToggleLineNumbers => sc(cmd_alt, Key::L),
            Command::ToggleWordWrap => sc(cmd_alt, Key::W),
            Command::ZoomIn => sc(cmd, Key::Equals),
            Command::ZoomOut => sc(cmd, Key::Minus),
            Command::ZoomReset => sc(cmd, Key::Num0),
        }
    }
}
