//! Every user-triggerable action lives here as a `Command`.
//!
//! Menus (native on macOS, in-window elsewhere) and keyboard shortcuts are
//! both generated from `MENUS`, so adding a feature is:
//!   1. add a variant to `Command`
//!   2. give it a label (and optionally a shortcut) below
//!   3. handle it in `SlothApp::execute` (app.rs)
//!   4. put it in a menu in `MENUS` — every command must appear in a menu

use eframe::egui::{Key, KeyboardShortcut, Modifiers};

use crate::settings::MAX_RECENT;
use crate::text_format::{ENCODINGS, LineEnding, TextEncoding};
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
    CloseTab,
    NextTab,
    PreviousTab,
    Find,
    FindAndReplace,
    FindNext,
    FindPrevious,
    UseSelectionForFind,
    About,
    Help,
    ReleaseNotes,
    /// Open the file at this position in File → Open Recent.
    OpenRecent(u8),
    ClearRecent,
    GoToLine,
    RevertToSaved,
    ReopenWithEncoding(TextEncoding),
    ConvertEncoding(TextEncoding),
    SetLineEnding(LineEnding),
    Indent,
    Outdent,
    IndentWithTabs,
    IndentWithSpaces,
    SetTabWidth(u8),
    ToggleAutoIndent,
    ToggleHighlightLine,
    /// The built-in editor font.
    DefaultFont,
    /// An installed font, by position in `fonts::families()`.
    SetFont(u16),
    CloseOtherTabs,
    CloseTabsToRight,
}

pub enum MenuEntry {
    Item(Command),
    Separator,
    Submenu(&'static str, &'static [MenuEntry]),
    /// File → Open Recent, whose items change at runtime.
    RecentFiles,
    /// Items listed by a function (for long, regular lists like encodings).
    Generated(fn() -> Vec<Command>),
    /// View → Font, filled in once installed fonts have been scanned.
    Fonts,
}

use MenuEntry::{Fonts, Generated, Item, RecentFiles, Separator, Submenu};

/// The menu bar. On macOS, `About` and `Quit` are moved into the application
/// menu automatically, and the Window and Help menus gain the standard items.
pub const MENUS: &[(&str, &[MenuEntry])] = &[
    (
        "File",
        &[
            Item(Command::New),
            Item(Command::Open),
            RecentFiles,
            Separator,
            Item(Command::CloseTab),
            Item(Command::CloseOtherTabs),
            Item(Command::CloseTabsToRight),
            Item(Command::Save),
            Item(Command::SaveAs),
            Item(Command::RevertToSaved),
            Separator,
            Submenu("Reopen with Encoding", &[Generated(reopen_with_encoding_items)]),
            Submenu("Convert to Encoding", &[Generated(convert_encoding_items)]),
            Submenu("Line Endings", LINE_ENDING_MENU),
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
            Separator,
            Submenu("Find", FIND_MENU),
            Item(Command::GoToLine),
            Separator,
            Item(Command::Indent),
            Item(Command::Outdent),
        ],
    ),
    (
        "View",
        &[
            Item(Command::ToggleLineNumbers),
            Item(Command::ToggleWordWrap),
            Item(Command::ToggleHighlightLine),
            Separator,
            Submenu("Theme", THEME_MENU),
            Submenu("Font", &[Fonts]),
            Submenu("Indentation", INDENT_MENU),
            Separator,
            Item(Command::ZoomIn),
            Item(Command::ZoomOut),
            Item(Command::ZoomReset),
        ],
    ),
    (
        "Window",
        &[Item(Command::NextTab), Item(Command::PreviousTab)],
    ),
    (
        "Help",
        &[
            Item(Command::Help),
            Item(Command::ReleaseNotes),
            Separator,
            Item(Command::About),
        ],
    ),
];

const INDENT_MENU: &[MenuEntry] = &[
    Item(Command::IndentWithTabs),
    Item(Command::IndentWithSpaces),
    Separator,
    Item(Command::SetTabWidth(2)),
    Item(Command::SetTabWidth(4)),
    Item(Command::SetTabWidth(8)),
    Separator,
    Item(Command::ToggleAutoIndent),
];

const LINE_ENDING_MENU: &[MenuEntry] = &[
    Item(Command::SetLineEnding(LineEnding::Lf)),
    Item(Command::SetLineEnding(LineEnding::CrLf)),
];

fn reopen_with_encoding_items() -> Vec<Command> {
    ENCODINGS.iter().map(|&e| Command::ReopenWithEncoding(e)).collect()
}

fn convert_encoding_items() -> Vec<Command> {
    ENCODINGS.iter().map(|&e| Command::ConvertEncoding(e)).collect()
}

const FIND_MENU: &[MenuEntry] = &[
    Item(Command::Find),
    Item(Command::FindAndReplace),
    Item(Command::FindNext),
    Item(Command::FindPrevious),
    Item(Command::UseSelectionForFind),
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
                RecentFiles => {
                    out.extend((0..MAX_RECENT as u8).map(Command::OpenRecent));
                    out.push(Command::ClearRecent);
                }
                Generated(items) => out.extend(items()),
                Fonts => {
                    out.push(Command::DefaultFont);
                    let count = crate::fonts::families().len() as u16;
                    out.extend((0..count).map(Command::SetFont));
                }
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
            Command::CloseTab => "Close Tab",
            Command::NextTab => "Show Next Tab",
            Command::PreviousTab => "Show Previous Tab",
            Command::Find => "Find…",
            Command::FindAndReplace => "Find and Replace…",
            Command::FindNext => "Find Next",
            Command::FindPrevious => "Find Previous",
            Command::UseSelectionForFind => "Use Selection for Find",
            Command::About => "About Sloth",
            Command::Help => "Sloth Help",
            Command::ReleaseNotes => "Release Notes",
            // Recent file items are labelled with the file name at runtime.
            Command::OpenRecent(_) => "Open Recent File",
            Command::ClearRecent => "Clear Menu",
            Command::GoToLine => "Go to Line…",
            Command::RevertToSaved => "Revert to Saved",
            Command::ReopenWithEncoding(e) | Command::ConvertEncoding(e) => e.label(),
            Command::SetLineEnding(l) => l.label(),
            Command::Indent => "Indent",
            Command::Outdent => "Outdent",
            Command::IndentWithTabs => "Indent with Tabs",
            Command::IndentWithSpaces => "Indent with Spaces",
            Command::SetTabWidth(2) => "Tab Width: 2",
            Command::SetTabWidth(4) => "Tab Width: 4",
            Command::SetTabWidth(8) => "Tab Width: 8",
            Command::SetTabWidth(_) => "Tab Width",
            Command::ToggleAutoIndent => "Auto-Indent",
            Command::ToggleHighlightLine => "Highlight Current Line",
            Command::DefaultFont => "Built-in (Hack)",
            Command::SetFont(i) => crate::fonts::families()
                .get(i as usize)
                .map_or("Font", String::as_str),
            Command::CloseOtherTabs => "Close Other Tabs",
            Command::CloseTabsToRight => "Close Tabs to the Right",
        }
    }

    /// `Modifiers::COMMAND` is ⌘ on macOS and Ctrl elsewhere.
    pub fn shortcut(self) -> Option<KeyboardShortcut> {
        let cmd = Modifiers::COMMAND;
        let cmd_shift = Modifiers::COMMAND | Modifiers::SHIFT;
        let cmd_alt = Modifiers::COMMAND | Modifiers::ALT;
        let ctrl = Modifiers::CTRL;
        let ctrl_shift = Modifiers::CTRL | Modifiers::SHIFT;
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
            Command::CloseTab => sc(cmd, Key::W),
            Command::NextTab => sc(ctrl, Key::Tab),
            Command::PreviousTab => sc(ctrl_shift, Key::Tab),
            Command::Find => sc(cmd, Key::F),
            Command::FindAndReplace => sc(cmd_alt, Key::F),
            Command::FindNext => sc(cmd, Key::G),
            Command::FindPrevious => sc(cmd_shift, Key::G),
            Command::UseSelectionForFind => sc(cmd, Key::E),
            Command::Help => sc(cmd_shift, Key::Slash), // ⌘?
            Command::GoToLine => sc(cmd, Key::L),
            Command::Indent => sc(cmd, Key::CloseBracket),
            Command::Outdent => sc(cmd, Key::OpenBracket),
            Command::SetTheme(_)
            | Command::About
            | Command::ReleaseNotes
            | Command::OpenRecent(_)
            | Command::ClearRecent
            | Command::RevertToSaved
            | Command::ReopenWithEncoding(_)
            | Command::ConvertEncoding(_)
            | Command::SetLineEnding(_)
            | Command::IndentWithTabs
            | Command::IndentWithSpaces
            | Command::SetTabWidth(_)
            | Command::ToggleAutoIndent
            | Command::ToggleHighlightLine
            | Command::DefaultFont
            | Command::SetFont(_)
            | Command::CloseOtherTabs
            | Command::CloseTabsToRight => None,
        }
    }

    /// Commands shown with a check mark reflecting current state.
    pub fn is_checkable(self) -> bool {
        matches!(
            self,
            Command::ToggleLineNumbers
                | Command::ToggleWordWrap
                | Command::SetTheme(_)
                | Command::ConvertEncoding(_)
                | Command::SetLineEnding(_)
                | Command::IndentWithTabs
                | Command::IndentWithSpaces
                | Command::SetTabWidth(_)
                | Command::ToggleAutoIndent
                | Command::ToggleHighlightLine
                | Command::DefaultFont
                | Command::SetFont(_)
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

    /// On macOS these live in the application menu rather than where `MENUS` puts them.
    pub fn in_app_menu_on_macos(self) -> bool {
        matches!(self, Command::About | Command::Quit)
    }

    /// Stable string id, used for native menu items.
    pub fn id(self) -> String {
        format!("{self:?}")
    }

    pub fn from_id(id: &str) -> Option<Command> {
        all().into_iter().find(|c| c.id() == id)
    }
}
