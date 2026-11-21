//! Every user-triggerable action lives here as a `Command`.
//!
//! Menus (native on macOS, in-window elsewhere) and keyboard shortcuts are
//! both generated from `MENUS`, so adding a feature is:
//!   1. add a variant to `Command`
//!   2. give it a label (and optionally a shortcut) below
//!   3. handle it in `SlothApp::execute` (app.rs)
//!   4. put it in a menu in `MENUS` — every command must appear in a menu

use eframe::egui::{Key, KeyboardShortcut, Modifiers};

use crate::theme::ThemeChoice;

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
    SetTheme(ThemeChoice),
}

pub enum MenuEntry {
    Item(Command),
    Separator,
    Submenu(&'static str, &'static [MenuEntry]),
}

use MenuEntry::{Item, Separator, Submenu};

/// The menu bar. On macOS, `Quit` is moved into the application menu automatically.
pub const MENUS: &[(&str, &[MenuEntry])] = &[
    (
        "File",
        &[
            Item(Command::New),
            Item(Command::Open),
            Separator,
            Item(Command::Save),
            Item(Command::SaveAs),
            Separator,
            Item(Command::Quit),
        ],
    ),
    (
        "Edit",
        &[
            Item(Command::Undo),
            Item(Command::Redo),
            Separator,
            Item(Command::Cut),
            Item(Command::Copy),
            Item(Command::Paste),
            Item(Command::SelectAll),
        ],
    ),
    (
        "View",
        &[
            Item(Command::ToggleLineNumbers),
            Item(Command::ToggleWordWrap),
            Separator,
            Submenu("Theme", THEME_MENU),
            Separator,
            Item(Command::ZoomIn),
            Item(Command::ZoomOut),
            Item(Command::ZoomReset),
        ],
    ),
];

const THEME_MENU: &[MenuEntry] = &[
    Item(Command::SetTheme(ThemeChoice::System)),
    Separator,
    Item(Command::SetTheme(ThemeChoice::Light)),
    Item(Command::SetTheme(ThemeChoice::Dark)),
    Separator,
    Item(Command::SetTheme(ThemeChoice::SolarizedLight)),
    Item(Command::SetTheme(ThemeChoice::SolarizedDark)),
    Item(Command::SetTheme(ThemeChoice::Sepia)),
    Item(Command::SetTheme(ThemeChoice::Midnight)),
];

/// Every command, in menu order.
pub fn all() -> Vec<Command> {
    fn walk(entries: &[MenuEntry], out: &mut Vec<Command>) {
        for entry in entries {
            match entry {
                Item(cmd) => out.push(*cmd),
                Separator => {}
                Submenu(_, children) => walk(children, out),
            }
        }
    }
    let mut out = Vec::new();
    for (_, entries) in MENUS {
        walk(entries, &mut out);
    }
    out
}

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
            Command::SetTheme(theme) => theme.label(),
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
            Command::SetTheme(_) => None,
        }
    }

    /// Commands shown with a check mark reflecting current state.
    pub fn is_checkable(self) -> bool {
        matches!(
            self,
            Command::ToggleLineNumbers | Command::ToggleWordWrap | Command::SetTheme(_)
        )
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
        all().into_iter().find(|c| c.id() == id)
    }
}
