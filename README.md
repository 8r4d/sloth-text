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
| Quit               | ⌘Q       |
| Toggle line numbers| ⌥⌘L      |
| Toggle word wrap   | ⌥⌘W      |
| Zoom in / out / reset | ⌘= / ⌘- / ⌘0 |

- Line-number gutter (also works with word wrap on)
- Cursor line/column and line count in the status bar
- Native macOS open/save dialogs
- Drag a file onto the window to open it
- Asks before discarding unsaved changes (New, Open, Quit, closing the window)
- A `•` in the title bar marks unsaved changes

## Code layout

| File | What's in it |
|------|--------------|
| `src/main.rs`     | Window setup and startup |
| `src/app.rs`      | App state, command handling, menu bar, status bar, dialogs |
| `src/commands.rs` | The `Command` enum: every action, its label, shortcut and menu placement |
| `src/document.rs` | Loading, saving and unsaved-change tracking for one file |
| `src/editor.rs`   | The text area and line-number gutter |

### Adding a feature

Most features start as a new `Command`:

1. Add a variant to `Command` in `commands.rs`.
2. Give it a `label()` and, if you want one, a `shortcut()`. Add it to `ALL`, and to a menu in `MENUS`.
3. Handle it in `SlothApp::execute` in `app.rs`.

The menu and keyboard shortcut are created from that automatically.

Possible next steps: find/replace, recent files, tabs or several documents
(`Document` is already separate, so `SlothApp` could hold a `Vec<Document>`),
syntax highlighting (through the editor's layouter), saving settings, and a
native macOS menu bar (the `muda` crate).
