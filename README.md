# Sloth

A small plain-text editor written in Rust, built mainly for macOS.

## Running

```sh
cargo run --release                 # empty document
cargo run --release -- notes.txt    # open (or create) a file
```

To build a double-clickable `Sloth.app`:

```sh
cargo install cargo-bundle
cargo bundle --release              # -> target/release/bundle/osx/Sloth.app
```

## Features

| Action             | Shortcut |
|--------------------|----------|
| New                | ⌘N       |
| Open…              | ⌘O       |
| Save               | ⌘S       |
| Save As…           | ⇧⌘S      |
| Close tab          | ⌘W       |
| Next / previous tab | ⌃⇥ / ⌃⇧⇥ |
| Quit               | ⌘Q       |
| Undo / Redo        | ⌘Z / ⇧⌘Z |
| Cut / Copy / Paste / Select All | ⌘X / ⌘C / ⌘V / ⌘A |
| Toggle line numbers| ⌥⌘L      |
| Toggle word wrap   | ⌥⌘W      |
| Zoom in / out / reset | ⌘= / ⌘- / ⌘0 |
| Find / Find and Replace | ⌘F / ⌥⌘F |
| Find next / previous | ⌘G / ⇧⌘G |
| Use selection for Find | ⌘E |
| Help (shortcut list) | ⇧⌘/ |

- Tabs: open several files at once; each keeps its own cursor, undo history and scroll position
- Find and replace with match highlighting, match case and whole-word options
- Save dialog offers Plain Text (.txt), Markdown (.md) or Any Format, and adds the
  extension if you type a name without one
- Native macOS menu bar (Sloth, File, Edit, View, Window, Help menus)
- About window (version and build) and Help window (shortcuts and release notes)
- Colour themes (View → Theme): Match System, Light, Dark, Solarized Light,
  Solarized Dark, Sepia, Midnight
- Remembers theme, line numbers, word wrap, text size and window size between launches
- Line-number gutter (also works with word wrap on)
- Cursor line/column and line count in the status bar
- Native macOS open/save dialogs
- Drag files onto the window to open them
- Asks before discarding unsaved changes (closing a tab, quitting, closing the window)
- A `•` in the title bar marks unsaved changes

## Code layout

| File | What's in it |
|------|--------------|
| `src/main.rs`     | Window setup and startup |
| `src/app.rs`      | App state, command handling, menu bar, status bar, dialogs |
| `src/commands.rs` | The `Command` enum: every action, its label, shortcut and menu placement |
| `src/document.rs` | Loading, saving and unsaved-change tracking for one file |
| `src/tabs.rs`     | The list of open documents and the tab bar |
| `src/find.rs`     | Search/replace logic (unit-tested) and the find bar |
| `src/file_types.rs` | Save dialog formats and the default extension |
| `src/version.rs`, `build.rs` | Version, git commit and build date shown in About |
| `src/editor.rs`   | The text area and line-number gutter |
| `src/theme.rs`    | Colour themes: each one is a small palette of colours |
| `src/native_menu.rs` | macOS menu bar, built from `commands::MENUS` (other platforms get an in-window menu) |

### Adding a feature

Most features start as a new `Command`:

1. Add a variant to `Command` in `commands.rs`.
2. Give it a `label()` and, if you want one, a `shortcut()`. Put it in a menu in `MENUS`.
3. Handle it in `SlothApp::execute` in `app.rs`.

The menu item and keyboard shortcut are created from that automatically. If the
shortcut uses a key that isn't mapped yet, add it to `to_accelerator` in `native_menu.rs`.

To add a theme, add a variant to `ThemeChoice` in `theme.rs` with a label and
palette, add it to `ALL` there, and add it to `THEME_MENU` in `commands.rs`.

Settings are saved to `~/Library/Application Support/Sloth/app.ron`.

## Versioning

The version number is in `Cargo.toml` and follows semantic versioning. For a release:

1. Bump `version` in `Cargo.toml`.
2. Add a section to `CHANGELOG.md`. It appears in Help → Release Notes.
3. Commit, then tag the commit: `git tag v0.2.0`.

The About window shows the version and the git commit it was built from.
`+modified` means the build included uncommitted changes.

Possible next steps: recent files, reopening tabs from the last session,
regex search, and syntax highlighting (through the editor's layouter).
