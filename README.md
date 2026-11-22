# Sloth

A small plain-text editor written in Rust, built mainly for macOS.

## Running

```sh
cargo run --release                 # empty document
cargo run --release -- notes.txt    # open (or create) a file
```

To build `Sloth.app` and install it in Applications:

```sh
cargo install cargo-bundle --locked   # once
scripts/install.sh
```

After installing, Sloth appears in Finder's **Open With** menu for text files.
To make it the default for a type, select a file, choose **File → Get Info**,
pick Sloth under **Open with**, and click **Change All**.

## Features

| Action             | Shortcut |
|--------------------|----------|
| New                | ⌘N       |
| New window         | ⇧⌘N      |
| Open…              | ⌘O       |
| Save               | ⌘S       |
| Save As…           | ⇧⌘S      |
| Close tab          | ⌘W       |
| Close window       | ⇧⌘W      |
| Print              | ⌘P       |
| Go to line         | ⌘L       |
| Indent / outdent lines | ⌘] / ⌘[ |
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
| Find in all tabs   | ⇧⌘F      |
| Markdown preview   | ⌥⌘P      |
| Help (shortcut list) | ⇧⌘/ |
| Settings           | ⌘,       |

- Tabs: open several files at once; each keeps its own cursor, undo history and scroll position
- Find and replace with match highlighting, match case, whole-word and regex
  options (`$1` in replacements), recent searches (↑/↓), and Find in All Tabs
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
- Drag files onto the window to open them, or open them from Finder
- File → Open Recent, and the tabs you had open are reopened at launch
- Files changed by another program reload automatically, or Sloth asks if you
  have unsaved edits
- Keeps each file's encoding (17 common ones, detected automatically) and line
  endings; File menu options to reopen, convert or change them
- Safe saves (temporary file, then swapped into place) and crash recovery for
  unsaved documents
- Auto-indent, indent/outdent, tabs or spaces, current-line highlight, matching
  brackets, word count, and any installed fixed-width font
- Drag tabs to reorder them; right-click a tab for more close options
- Syntax highlighting in theme colours, and a Markdown preview
- Several windows, each with its own tabs; printing and Save as PDF
- Asks before discarding unsaved changes (closing a tab, quitting, closing the window)
- A `•` in the title bar marks unsaved changes

## Code layout

The repository is a Cargo workspace with two crates: `sloth-core` in `core/`
holds code meant to be shared with other apps (documents, the editor widget, find,
tabs, themes), and the `sloth` app in `sloth/` builds the editor from it.

| File | What's in it |
|------|--------------|
| `sloth/src/main.rs`     | Window setup and startup |
| `sloth/src/app.rs`      | App and window state, command handling, menu bar, status bar, dialogs |
| `sloth/src/commands.rs` | The `Command` enum: every action, its label, shortcut and menu placement |
| `core/src/document.rs` | Loading, saving and unsaved-change tracking for one file |
| `core/src/tabs.rs`     | The list of open documents and the tab bar |
| `core/src/find.rs`     | Search/replace logic (unit-tested) and the find bar |
| `core/src/search_results.rs` | The Find in All Tabs results panel |
| `core/src/syntax.rs`   | Syntax highlighting (syntect), cached line by line |
| `core/src/file_types.rs` | Save dialog formats and the default extension |
| `sloth/src/version.rs`, `sloth/build.rs` | Version, git commit and build date shown in About |
| `sloth/src/settings.rs` | Everything saved between launches (theme, view options, recent files, open tabs) |
| `core/src/macos_open.rs` | Receives files opened from Finder |
| `core/src/macos_print.rs` | Printing through the macOS print dialog |
| `sloth/src/help.rs`     | The Help window's pages |
| `sloth/src/settings_window.rs` | The Settings window |
| `scripts/make-icon.swift` | Draws the app icon (`sloth/icons/`); rerun after changing it |
| `core/src/text_format.rs` | Encodings, byte-order marks and line endings (reading and writing) |
| `core/src/recovery.rs` | Crash-recovery copies of unsaved documents |
| `core/src/editing.rs`  | Auto-indent, indent/outdent and bracket matching (pure, unit-tested) |
| `core/src/fonts.rs`    | Lists installed fixed-width fonts and installs the chosen one |
| `sloth/src/app_ui_tests.rs` | UI tests that run the app in memory (`cargo test`) |
| `sloth/macos/document-types.plist` | File types declared in Sloth.app, so Finder offers it |
| `core/src/editor.rs`   | The text area and line-number gutter |
| `core/src/theme.rs`    | Colour themes: each one is a small palette of colours |
| `sloth/src/native_menu.rs` | macOS menu bar, built from `commands::MENUS` (other platforms get an in-window menu) |

### Adding a feature

Most features start as a new `Command`:

1. Add a variant to `Command` in `commands.rs`.
2. Give it a `label()` and, if you want one, a `shortcut()`. Put it in a menu in `MENUS`.
3. Handle it in `SlothApp::execute` in `app.rs`.

The menu item and keyboard shortcut are created from that automatically. If the
shortcut uses a key that isn't mapped yet, add it to `to_accelerator` in `native_menu.rs`.

To add a theme, add a variant to `ThemeChoice` in `theme.rs` with a label and
palette, add it to `ALL` there, and add it to `THEME_MENU` in `commands.rs`.

Settings are saved to `~/Library/Application Support/Sloth/app.ron`, and
crash-recovery copies to the `recovery` folder next to it.

`cargo test` runs the unit tests and the UI tests. The UI tests use
`egui_kittest` to drive the app in memory, so they don't open a window.

To see what the editor looks like without opening it, render it to images:

```sh
SLOTH_PREVIEW_DIR=/tmp cargo test preview -- --ignored   # writes /tmp/editor*.png
```

## Versioning

The version number is in `Cargo.toml` and follows semantic versioning. For a release:

1. Bump `version` in `Cargo.toml`.
2. Add a section to `CHANGELOG.md`. It appears in Help → Release Notes.
3. Commit, then tag the commit: `git tag v0.2.0`.

The About window shows the version and the git commit it was built from.
`+modified` means the build included uncommitted changes.

## Large files

Measured on an M-series Mac (release build), typing stays smooth up to about
10 MB (around 25 ms a frame) and becomes sluggish around 50 MB (about 110 ms).
Syntax highlighting turns off above 2 MB. Very large files are also slow while
VoiceOver or another accessibility tool is active, because egui rebuilds its
accessibility information for every line on each frame. To re-measure:

```sh
cargo test --release perf_plain -- --ignored --nocapture
```

See [ROADMAP.md](ROADMAP.md) for what's planned.
