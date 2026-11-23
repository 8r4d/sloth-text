# Sloth

Two Mac apps written in Rust, sharing one editor, and a small web service:

- **Sloth** (1.0): a small plain-text editor.
- **Sloth Writer** (1.7): an app for writing long projects such as novels,
  in the spirit of Scrivener. A project is a folder of plain Markdown files.
- **The Writer helper** (`helper/`): a PHP service on ordinary web hosting
  that lets you write chosen scenes in a web browser, synced with Writer.

## Installing

Both apps build into `.app` bundles with cargo-bundle:

```sh
cargo install cargo-bundle --locked   # once
scripts/install.sh                    # Sloth.app
scripts/install-writer.sh             # Sloth Writer.app
```

To run them from the source instead:

```sh
cargo run --release                         # Sloth (an empty document)
cargo run --release -- notes.txt            # Sloth, opening (or creating) a file
cargo run --release -p sloth-writer         # Sloth Writer
```

After installing, Sloth appears in Finder's **Open With** menu for text files.
To make it the default for a type, select a file, choose **File → Get Info**,
pick Sloth under **Open with**, and click **Change All**.

## Sloth

### Shortcuts

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

### Features

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

## Sloth Writer

A project is an ordinary folder: each document is a Markdown file you can
open in any other program, and `project.ron` holds the binder (the order and
nesting of everything) and each item's details. Writer never writes over a
change another program made, and saves itself every few seconds.

### What it does

**Organising**
- A binder with Manuscript, Characters, Research, Notes and Trash folders.
  Drag items to reorder and nest them, and hide the binder (⌥⌘B) when you
  don't need it.
- An inspector (⌥⌘I) for each item: synopsis, status and label (both
  editable lists), keywords, a word target, and the scenes' characters and
  places.
- Corkboard and outline views of any folder.
- Profile folders for characters, locations, objects, factions,
  worldbuilding and themes, or non-fiction sources, people, concepts,
  arguments, events and a glossary. Each profile is built from sections you
  choose.

**Writing**
- Prose in a reading font, in a centred column.
- Bold, italics and headings shown as you type. Formatting, Change Case, Remove
  Blank Lines and Split at Cursor (⌘K) are in the right-click menu.
- Two hyphens make an em dash, and spelling is checked with the Mac's own
  spell checker.
- Notes to yourself, either `<!-- in the text -->` or in a panel above it,
  never compiled.
- Split view (⌘\\) beside the editor, and Focus mode (⌥⌘F): full screen,
  just the text, with typewriter scrolling.
- Find in the document (⌘F) or the whole project (⇧⌘F).

**Keeping track**
- Words written today against a daily goal, a streak, a 30-day chart, and
  progress towards a length and deadline (click "Today" in the status bar).
- Snapshots of a document (⌘5), compared word by word and restored.

**Keeping it safe**
- Automatic backups, as dated zips, when a project closes and once a day,
  keeping the newest ten. Back Up… (in the toolbar's More… menu) makes one
  whenever you like.
- Only one copy of Writer can have a project open at a time, so two Macs
  sharing a project can't save over each other.
- Emptying the Trash keeps what it removes as a zip in the Mac's Trash.

**Getting work in and out**
- Import from Scrivener… (on the start screen) brings a Scrivener 3 project
  across. The `.scriv` is only read. An import report lists what couldn't
  come across.
- Compile (⇧⌘E) the Manuscript as a standard manuscript (.docx), an ePub
  or Markdown, with saved presets.
- Write chosen scenes in a web browser through the helper (below).

The full list, release by release, is in [writer/CHANGELOG.md](writer/CHANGELOG.md),
and the app's Help (⌘?) lists every shortcut.

### Shortcuts

| Action | Shortcut |
|--------|----------|
| Open project | ⌘O |
| New document / folder | ⌘N / ⌥⌘N |
| Save everything now | ⌘S |
| Bold / italic | ⌘B / ⌘I |
| Split at cursor | ⌘K |
| Take a snapshot | ⌘5 |
| Focus mode (Esc to leave) | ⌥⌘F |
| Binder / inspector | ⌥⌘B / ⌥⌘I |
| Split view | ⌘\\ (⌥-click an item to open it there) |
| Find / find in project | ⌘F / ⇧⌘F |
| Find next / previous | ⌘G / ⇧⌘G |
| Compile | ⇧⌘E |
| Settings / help | ⌘, / ⌘? |

### Where things are kept

- A project: its folder. Inside, `.sloth/` holds the sync record, writing
  statistics and the lock that says which copy of Writer has it open.
- Settings: macOS's usual place for the app's saved state. The web helper's
  token is in the Keychain.
- Automatic backups: `~/Library/Application Support/Sloth Writer/Backups`,
  a folder per project (or another folder chosen in Settings).

## The web helper

`helper/` is a PHP service for shared web hosting (PHP and MySQL or
MariaDB). Writer sends it the scenes marked "Available on the web", and the
helper's web editor lets you write them in any browser, with their notes, word
targets and today's goal. Edits from either side are kept: a save that would
overwrite the other side's is refused and both are kept. Setting it up is in
[helper/README.md](helper/README.md). Its secrets go in
`helper/writer-config.php`, which git ignores.

## Code layout

The repository is a Cargo workspace with three crates, plus the helper:

- `sloth-core` in `core/`: code shared by both apps (documents, the editor
  widget, find, tabs, themes, Markdown styling, spell checking)
- `sloth` in `sloth/`: the plain-text editor
- `sloth-writer` in `writer/`: Sloth Writer
- `helper/`: the web helper (PHP, with its web page in `helper/public/`)

`cargo run` and `cargo test` at the root cover Sloth and core; add `--workspace`
to include Writer.

### Sloth and core

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
| `scripts/make-writer-icon.swift` | Draws Sloth Writer's icon (`writer/icons/`) |
| `core/src/text_format.rs` | Encodings, byte-order marks and line endings (reading and writing) |
| `core/src/recovery.rs` | Crash-recovery copies of unsaved documents |
| `core/src/editing.rs`  | Auto-indent, indent/outdent and bracket matching (pure, unit-tested) |
| `core/src/fonts.rs`    | Lists installed fixed-width fonts and installs the chosen one |
| `sloth/src/app_ui_tests.rs` | UI tests that run the app in memory (`cargo test`) |
| `sloth/macos/document-types.plist` | File types declared in Sloth.app, so Finder offers it |
| `core/src/editor.rs`   | The text area and line-number gutter |
| `core/src/theme.rs`    | Colour themes: each one is a small palette of colours |
| `sloth/src/native_menu.rs` | macOS menu bar, built from `commands::MENUS` (other platforms get an in-window menu) |
| `core/src/markdown.rs` | Markdown styling as you type (Writer) |
| `core/src/spell.rs`    | Spell checking with the macOS spell checker (Writer) |
| `core/src/trash.rs`    | Moving files to the Mac's Trash |

### Sloth Writer

| File | What's in it |
|------|--------------|
| `writer/src/project.rs` | The project on disk: the binder, metadata, profile folders, statuses and labels |
| `writer/src/app.rs` | The window: start screen, toolbar, editor area, saving, Focus mode |
| `writer/src/binder.rs`, `views.rs` | The binder; the corkboard and outline views |
| `writer/src/inspector.rs` | The inspector, the notes panel, and the Labels and Statuses windows |
| `writer/src/profiles_ui.rs`, `character.rs` | Profiles (characters, locations…) and their sections |
| `writer/src/search.rs`, `split_ui.rs` | Find in the document and the project; the split view |
| `writer/src/snapshots.rs`, `snapshots_ui.rs` | Snapshots, and comparing them |
| `writer/src/stats.rs`, `stats_ui.rs` | Words written each day, goals, and the Statistics window |
| `writer/src/compile.rs`, `compile_window.rs` | Compile to .docx, ePub and Markdown |
| `writer/src/backup.rs`, `lock.rs` | Backups (by hand and automatic); one copy of Writer per project |
| `writer/src/scrivener.rs`, `rtf.rs` | Importing from Scrivener; its RTF to Markdown |
| `writer/src/sync.rs`, `sync_ui.rs`, `keychain.rs` | Syncing with the web helper |
| `writer/src/settings_ui.rs`, `about_ui.rs`, `projects_ui.rs` | Settings, Help and About, recent projects |
| `writer/src/app_ui_tests.rs` | UI tests that run Writer in memory |
| `helper/public/api.php` | The helper: accounts, tokens, scenes with versions and history |
| `helper/public/app.js`, `style.css`, `index.php` | The web editor: the page you write in |

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

### Testing

```sh
cargo test --workspace        # both apps and core
helper/tests/run.sh           # the helper: its API, its web page in headless
                              # Chrome, and Writer syncing with it
```

`helper/tests/run.sh` needs `php`, `mariadb` and `node` (`brew install php
mariadb node`). It makes a throwaway database and server each time. If a copy of
Scrivener's `Tutorial.scriv` is in the repository's folder (git ignores
`.scriv` folders), `cargo test -p sloth-writer -- --ignored scriveners_tutorial`
imports it as a check.

To see what the editor looks like without opening it, render it to images:

```sh
SLOTH_PREVIEW_DIR=/tmp cargo test preview -- --ignored   # writes /tmp/editor*.png
```

## Versioning

Each app has its own version, in its `Cargo.toml`, following semantic
versioning: Sloth in `sloth/`, Writer in `writer/`. For a release:

1. Bump `version` in the app's `Cargo.toml`.
2. Add a section to its changelog (`sloth/CHANGELOG.md` or
   `writer/CHANGELOG.md`). It appears in the app's Help → Release Notes.
3. Commit, then tag the commit: `git tag v1.0.0` (Sloth) or
   `git tag writer-v1.7.0` (Writer).

The About window shows the version and the git commit it was built from.
`+modified` means the build included uncommitted changes.

## Large files (Sloth)

Measured on an M-series Mac (release build), typing stays smooth up to about
10 MB (around 25 ms a frame) and becomes sluggish around 50 MB (about 110 ms).
Syntax highlighting turns off above 2 MB. Very large files are also slow while
VoiceOver or another accessibility tool is active, because egui rebuilds its
accessibility information for every line on each frame. To re-measure:

```sh
cargo test --release perf_plain -- --ignored --nocapture
```

[ROADMAP.md](ROADMAP.md) is the plan Sloth followed to 1.0.
