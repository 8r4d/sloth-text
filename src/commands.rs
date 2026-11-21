//! Every user-triggerable action lives here as a `Command`.
//!
//! Menus (native on macOS, in-window elsewhere) and keyboard shortcuts are
//! both generated from this list, so adding a feature is:
//!   1. add a variant to `Command` and to `ALL`
//!   2. give it a label (and optionally a shortcut) below
//!   3. handle it in `SlothApp::execute` (app.rs)
//!   4. list it in a menu in `MENUS`

use eframe::egui::{Key, KeyboardShortcut, Modifiers};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Command {
    New,
    Open,
    Save,
    SaveAs,
    Quit,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
    ToggleLineNumbers,
    ToggleWordWrap,
    ZoomIn,
    ZoomOut,
    ZoomReset,
}

/// Menu layout: (menu title, commands). `None` draws a separator.
///
/// On macOS, `Quit` is moved into the application menu automatically.
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
        "Edit",
        &[
            Some(Command::Undo),
            Some(Command::Redo),
            None,
            Some(Command::Cut),
            Some(Command::Copy),
            Some(Command::Paste),
            Some(Command::SelectAll),
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

pub const ALL: &[Command] = &[
    Command::New,
    Command::Open,
    Command::Save,
    Command::SaveAs,
    Command::Quit,
    Command::Undo,
    Command::Redo,
    Command::Cut,
    Command::Copy,
    Command::Paste,
    Command::SelectAll,
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
            Command::Quit => "Quit Sloth",
            Command::Undo => "Undo",
            Command::Redo => "Redo",
            Command::Cut => "Cut",
            Command::Copy => "Copy",
            Command::Paste => "Paste",
            Command::SelectAll => "Select All",
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
            Command::Undo => sc(cmd, Key::Z),
            Command::Redo => sc(cmd_shift, Key::Z),
            Command::Cut => sc(cmd, Key::X),
            Command::Copy => sc(cmd, Key::C),
            Command::Paste => sc(cmd, Key::V),
            Command::SelectAll => sc(cmd, Key::A),
            Command::ToggleLineNumbers => sc(cmd_alt, Key::L),
            Command::ToggleWordWrap => sc(cmd_alt, Key::W),
            Command::ZoomIn => sc(cmd, Key::Equals),
            Command::ZoomOut => sc(cmd, Key::Minus),
            Command::ZoomReset => sc(cmd, Key::Num0),
        }
    }

    /// Editing commands the text widget already handles from the keyboard.
    /// Their shortcuts are shown in menus but not intercepted by the app.
    #[cfg_attr(target_os = "macos", allow(dead_code))] // used where there is no native menu
    pub fn is_handled_by_text_widget(self) -> bool {
        matches!(
            self,
            Command::Undo
                | Command::Redo
                | Command::Cut
                | Command::Copy
                | Command::Paste
                | Command::SelectAll
        )
    }

    /// Stable string id, used for native menu items.
    pub fn id(self) -> String {
        format!("{self:?}")
    }

    pub fn from_id(id: &str) -> Option<Command> {
        ALL.iter().copied().find(|c| c.id() == id)
    }
}
