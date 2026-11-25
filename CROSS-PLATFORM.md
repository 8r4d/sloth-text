# Sloth Writer on Windows and Linux: a roadmap

A plan for building and running Sloth Writer on Windows and Linux as well as
macOS, and then syncing a project between computers through the web helper.
Nothing here is built yet. Sizes are rough, as in ROADMAP.md: **S** is an
hour or two, **M** about a day, **L** several days.

The plain-text editor (Sloth) shares `core`, so it benefits from part of this
work, but it relies more on macOS (native menus, opening files from Finder,
printing). It isn't covered here.

## Where things stand

Most of Writer is already cross-platform:

- **Already portable:** the interface (egui/eframe), file dialogs (`rfd`), the
  font scan (`fontique`), where settings are saved (eframe picks each system's
  folder), syncing with the helper (`ureq` with rustls), compile (docx, epub,
  Markdown), Scrivener import, and the web helper itself.
- **macOS-only:** about ten spots, listed in phase 1. Each has a known
  replacement.
- **Shortcuts:** egui's `Modifiers::COMMAND` is already Ctrl on Windows and
  Linux, so the shortcuts themselves work. About 70 places in the interface
  label them with ⌘, ⌥ and ⇧, and those labels need to change.
- **Syncing between computers:** the helper today syncs only the documents
  marked "Available on the web", and treats one Mac as the owner of each
  project's binder. Using it to sync between computers is the largest piece
  of work (phase 4).

## Decisions to make first

1. **How do computers share a project?**
   - **A. A synced folder** (Syncthing, Dropbox, OneDrive, iCloud for Windows).
     This works as soon as Writer runs on each system. Writer's project lock
     already stops two computers having the same project open at once. The
     helper stays the web companion.
   - **B. The helper as the hub.** Each computer has its own copy, and the
     helper carries everything between them. No third-party service is
     needed, and it works with the offline copies the web page now keeps. It
     needs the protocol work in phase 4.
   - **Recommendation:** use A as soon as phases 1 and 2 are done, and build
     B after that.
2. **Spell checking on Windows and Linux:** use each system's own checker
   (Windows has one; Linux has none built in), or one engine bundled with
   Writer everywhere except the Mac. Recommendation: bundle one engine
   (`spellbook`, a Hunspell reader written in Rust, with dictionaries
   included).
3. **Packaging:** a zip file is enough to start with. Proper installers can
   come later.

---

## Phase 1: build and run on Windows and Linux (about 1–2 weeks)

### 1a. Automated builds on all three systems (M)

Set up GitHub Actions for the existing GitHub repository, with a job for each
system that builds `core` and `writer` and runs the tests. This finds every
compile error at once and keeps the other systems working afterwards.

- The UI tests (`egui_kittest`) draw with wgpu. On a Linux build machine that
  needs Mesa's software renderer (lavapipe). Windows machines usually manage.
  If it's fiddly, run the tests that don't need drawing first.
- The test that checks e-book output ran `/usr/bin/xmllint`. *Done:* it
  checks the XML with `roxmltree` now.

### 1b. Replace what's macOS-only

| Where | What it does on the Mac | Windows and Linux | Size |
|---|---|---|---|
| `writer/src/keychain.rs` | Keeps the helper token in the Keychain (`/usr/bin/security`) | The `keyring` crate: Credential Manager on Windows, Secret Service (GNOME Keyring, KWallet) on Linux. If there isn't one, a file only the user can read, with a warning. | M |
| `core/src/spell.rs` | Spell checks with NSSpellChecker | `spellbook` with bundled en-US/en-GB dictionaries and a word list you can add to. Same interface, so the editor doesn't change. | L |
| `core/src/trash.rs` (and emptying Writer's Trash, `backup.rs`) | Moves files to the Mac's Trash | The `trash` crate: the Recycle Bin on Windows, the desktop's trash on Linux | S |
| `core/src/motion.rs` | Follows Reduce motion | Windows: the "Show animations" setting. Linux: GNOME's `enable-animations` setting. Otherwise, a Writer setting. | S |
| `settings_ui.rs`, `inspector.rs` | Opens folders and web addresses with `open` | *Done:* the `opener` crate (Explorer, `xdg-open`). The rule that only web addresses open is kept. "Show in Finder" says Show in Explorer on Windows, Open Folder on Linux. | S |
| `writer/src/lock.rs` | The computer's name (`scutil`, falling back to `hostname`); "another Mac" when there's none | *Done:* `COMPUTERNAME` on Windows, `/proc/sys/kernel/hostname` or `/etc/hostname` on Linux, with no command or new crate; "another computer" when there's none. The Mac is unchanged. | S |
| `backup.rs`, `crash_log.rs` | Paths under `~/Library/…` | *Done* (`places.rs`, no crate): `%LOCALAPPDATA%\Sloth Writer` on Windows (local, not roaming: backups are big), `~/.local/share` and `~/.local/state` on Linux, following XDG. Backups already put in the temp folder are moved over once. | S |
| `core/src/document.rs` | Keeps Finder's extended attributes when saving | Nothing needed (already skipped on other systems) | — |
| `writer/src/main.rs` | Turns off winit's Mac menu, so ⌘Q saves first | Nothing needed. Check that closing the window still saves. | — |
| Fonts | Mac fonts as the defaults | Check the default prose and interface fonts. Choose defaults for each system (Georgia and Cambria on Windows, DejaVu Serif and Noto Serif on Linux), falling back to egui's own. | S |

### 1c. Shortcuts and how they're labelled (M)

*Done:* `sloth_core::keys`. Labels are still written the Mac way and
`keys::shown` turns them into Ctrl, Shift and Alt elsewhere. ⌥⌘ is
Ctrl+Shift there (`keys::OPT_CMD`), and Focus mode is F11 (`keys::FOCUS`).
Still to check by hand on a PC: Home/End, word movement and Ctrl+wheel zoom
(the last bullet), and that Ctrl+Shift+, reaches the app on a US keyboard.

- A helper that writes a shortcut for the system it's on: ⌘K on the Mac,
  Ctrl+K elsewhere, ⌥ as Alt, ⇧ as Shift. Use it for the roughly 70 labels,
  tooltips, Help and the changelog's About text.
- **Ctrl+Alt:** many Windows keyboard layouts use Ctrl+Alt as AltGr, to type
  characters such as @ € { }. So ⌥⌘ shortcuts must not become Ctrl+Alt on
  Windows. These are: comment ⌥⌘K, repeated words ⌥⌘R, Canvas ⌥⌘C, new
  folder ⌥⌘N, inspector ⌥⌘I, Focus ⌥⌘F and binder ⌥⌘B. Make them Ctrl+Shift
  there, checking for clashes (Ctrl+Shift+F is already Find in Project).
- Check the other keys and gestures the Mac gives us: Home and End,
  Ctrl+arrow word movement, and pinch to zoom on the Canvas (Ctrl+wheel
  elsewhere).

### 1d. Projects that move between systems (M)

Project files must work on all three systems:

- **Windows-forbidden names.** *Done.* Windows doesn't allow files named
  CON, PRN, AUX, NUL, COM1–9 or LPT1–9 (with any extension), names ending in
  a dot or a space, or the characters `< > : " / \ | ? *`.
  `project::portable_name` makes any name safe (`con` becomes `con-`,
  `What Now?` becomes `What Now-`), and it's used for document files
  (`slug`), the project folder (`folder_name`), backup zips, snapshots and
  research files from Scrivener.
- **Capital letters.** *Done.* Linux treats `Walking.md` and `walking.md` as
  two files; the Mac and Windows don't. `free_path`, and the names of new
  profile folders, now count a file on disk that differs only in capitals as
  taken, so a project made on Linux opens on the others.
- **Line endings.** Writer writes `\n` everywhere. Keep that, and accept
  `\r\n` in what it reads, which the encoding code mostly does already.
- **Long paths.** The project's own paths are short, but a project kept deep
  in Windows folders can pass 260 characters. Test with long paths turned on.

## Phase 2: install it (about 2–4 days)

| System | First | Later |
|---|---|---|
| Windows | A zip with `Sloth Writer.exe` and its icon. It isn't signed, so SmartScreen asks once. | An installer (`cargo-packager` makes NSIS or MSI), Start menu entry, file icon |
| Linux | A `.tar.gz` and a `.desktop` file, with `scripts/install-writer-linux.sh` to put them in `~/.local` | AppImage or `.deb` (`cargo-packager`) |
| macOS | Unchanged | A universal build (Apple silicon and Intel) with `lipo` |

The automated builds from 1a can attach these to each release on GitHub, so
the Windows and Linux computers need only the download, not Rust.

## Phase 3: share through a synced folder (Option A, about 1–2 days)

With phases 1 and 2 done, put the project folder in Syncthing, Dropbox or
OneDrive.

- **One computer at a time:** the project lock (`.sloth/lock.ron`) already stops
  two computers having the project open. Check that it holds when the sync
  service delivers the lock file late. Show "Open on Writing PC since 2:05"
  clearly, and offer to take over.
- **Per computer:** settings, and the web helper token. Each computer gets its
  own token from the helper's Account page.
- **Web sync:** the web sync state (`.sloth/sync.ron`) travels with the
  project, so whichever computer is open does the web syncing.
- **Conflict copies:** when two computers change the same file while offline,
  sync services make a copy such as `walking (Writing PC's conflicted copy).md`.
  Writer should notice files the binder doesn't list and offer to bring them
  in, instead of ignoring them.

## Phase 4: the helper as the hub (Option B, about 3–5 weeks part-time)

Each computer keeps its own copy, and syncs the **whole** project through the
helper: every document, the binder and its metadata, and the Canvas. The web
editor keeps working as now.

### 4a. What the helper holds (L)

- **Every document:** every document in a project, not only those on the web.
  "Available on the web" becomes a flag on each document, still deciding what
  the web editor's list shows.
- **The project itself:** the binder (with titles, metadata, labels,
  statuses, compile settings and goals) as one versioned document,
  `manifest`, saved against the version it was edited from, as documents are
  now. The Canvas (`canvas.ron`) the same way.
- **Kept on each computer:** writing statistics (each computer's days, added
  together when shown), snapshots, and backups.
- **API additions** in `api.php`:
  - `manifest_get` and `manifest_put` with versions;
  - `changes` that also reports the manifest and Canvas;
  - `project_create` to upload a project for the first time.
- **Big projects:** saves done in batches, so a large project doesn't run into
  shared hosting's request size or time limits.

### 4b. Each computer's sync state (M)

- **Where it lives:** today the sync record is `.sloth/sync.ron` in the
  project folder. Key it by a device id kept in each computer's settings, so
  a project in a synced folder (Option A) and the hub (Option B) can't trip
  over each other.
- **Tokens:** each computer has its own token, already supported. Each save
  records which computer made it ("Mac", "PC", "Linux", "web").

### 4c. Merging the binder (L, the core of it)

Two computers may both change the binder while apart. For example, one adds
three essays to a chapter while the other renames the chapter and moves it.
The plan: a **three-way merge by item id** against the version both started
from, kept in the sync record.

- **Every item has a stable id**, so the merge works by id, not by position or
  name.
- **One field at a time:** title, parent and position, status, label,
  keywords, synopsis, notes and web address each merge separately. If only
  one side changed a field, take that change.
- **Both changed the same field:** take the later change, and record it
  ("Title changed on both: kept the PC's") in Notes, where they can be seen.
- **Adds and trashes:** both sides' additions are kept. Something trashed on
  one side and edited on the other goes back from the Trash, so writing is
  never lost.
- **Moves:** if moving an item would put it inside itself (both sides moved
  folders into each other), the later move is undone.
- **File names on disk follow the binder.** Each computer renames its own
  files to match; the helper knows documents only by id.
- **Snapshots first:** take a snapshot of any document before applying a
  change from elsewhere. That's already the rule when the web and the Mac
  clash.

`sync.rs` already runs its rules against a stand-in for the helper
(`Remote`). The merge can be written and tested there in plain Rust, before
any network or interface work: two pretend computers, one pretend helper,
every kind of change on both sides.

### 4d. In the app (M)

- **Get Project from the Helper…** on the start screen: download a project to
  a new computer.
- **Sync This Project with the Helper:** turn it on for a project that's on
  this computer only, uploading it the first time.
- **The Sync button** shows what came in ("3 documents and the binder from
  the PC"), and its tooltip shows the last sync per computer.
- **Clashes:** shown the same way as now, as snapshots and notes in the
  inspector.

### 4e. Tests (M)

- **Plain Rust:** the merge rules against the stand-in helper (4c).
- **`helper/tests/run.sh`:** two copies of Writer syncing one project through
  the real helper, each making changes, then checking both end up the same.
- **Builds:** the automated builds from 1a run all of this on every system.

### Risks

- **Losing writing in a merge.** Mitigation: snapshot before applying, never
  delete (only move to the Trash), and keep the clash record.
- **Shared hosting limits:** request size, execution time and database size.
  Mitigation: batch saves, and keep earlier versions under a limit (for
  example, the last 50 versions of each document).
- **Two kinds of sharing at once.** If both a synced folder and the hub are
  used for one project, the per-computer sync state (4b) keeps them apart.
  But Writer should warn when it sees both.

## Suggested order

1. Phase 1a: automated builds, so problems show up straight away.
2. Phase 1b–1d: Writer runs on Windows and Linux.
3. Phase 2: install it on your own Windows and Linux computers.
4. Phase 3: share projects through a synced folder. This is the point where
   you can use all three computers.
5. Phase 4c first, as plain Rust with tests. Then 4a, 4b, 4d and 4e.
