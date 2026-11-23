# Sloth roadmap

The plan from 0.2.0 to 1.0. Each release groups related work so it can be
built, tested and tagged on its own. Sizes are rough: **S** is an hour or two,
**M** is about a day, **L** is several days.

## Decision to make first: keep egui for text editing?

Most of this plan works with the current egui text widget. A few things it
can't do well:

- spell check, dictation and the emoji picker (macOS text services)
- VoiceOver
- very large files: the whole document is laid out again on every frame

**Recommendation:** stay on egui through 0.8. Keep all editing code inside
`editor.rs` so the text engine could be swapped later. If any of the items
above become important, plan a 1.x change to a native `NSTextView`, embedded
through the `objc2` crates that are already dependencies. That would replace
the editor and find highlighting, but not tabs, menus, files or settings.

---

## 0.2.x: Groundwork ✅ (done in 0.3.0)

Things that make the later work easier and safer.

| Item | Size | Approach |
|---|---|---|
| Settings struct | S | Replace the separate `set_string` keys with one `Settings` struct saved as JSON or RON (needs `serde`). Recent files, open tabs, font and indent settings all go here. |
| Dynamic menus | M | Let `native_menu.rs` rebuild a submenu while the app runs; at the moment `MENUS` is fixed at compile time. Needed for Open Recent. |
| Headless UI tests | M | Add `egui_kittest` so UI behaviour (tabs, find bar, dialogs) can be tested without driving the real screen. |
| `scripts/install.sh` | S | `cargo bundle --release` and copy `Sloth.app` into `/Applications`. |

## 0.3.0: Default text editor ✅

Goal: Sloth can be your everyday plain-text app.

| Item | Size | Approach |
|---|---|---|
| Open from Finder (double-click, Open With, dropping a file on the Dock icon) | L | 1) Declare document types (`public.plain-text`, Markdown, source code) in the bundle's `Info.plist`; check whether cargo-bundle can merge in extra `Info.plist` entries. 2) winit doesn't pass macOS "open file" events on, so register an Apple Event handler for `odoc` with `NSAppleEventManager` at startup and send the paths to the app through a channel. This works for both a cold launch and an already-running app. |
| Open Recent | M | Keep the last 15 paths in Settings. Add a File → Open Recent submenu (uses dynamic menus) with "Clear Menu". *Not done yet: also listing them in the Dock menu via `NSDocumentController`.* |
| Restore tabs on launch | S | Save open file paths and the active tab on quit, and reopen them at launch (skipping missing files). Untitled tabs are handled in 0.4. |
| Detect outside changes | M | Record each file's modification time and size when loading or saving. Check again whenever the window regains focus, and every 2 s while it has focus. If the tab is clean, reload silently. If it has edits, show a bar: "Changed on disk: Reload / Keep Mine". If the file was deleted, mark the tab. |
| Go to Line (⌘L) | S | A small prompt for `line` or `line:col` that moves the cursor and scrolls there (reuses `set_selection` and `reveal`). |
| Revert to Saved | S | File menu command, with a confirmation prompt. |

## 0.4.0: Safe with files ✅

Goal: Sloth never damages a file or loses work.

| Item | Size | Approach |
|---|---|---|
| Text encodings | M | Detect the encoding with `chardetng` and decode with `encoding_rs`; remember the encoding and any BOM on `Document`, and save back in the same encoding. Show the encoding in the status bar, and add File → Reopen with Encoding. If a file can't be decoded cleanly, warn before saving. |
| Line endings | S | Detect LF or CRLF on load, edit with LF internally, and write back the original style. Show it in the status bar, with an option to convert. |
| Autosave and crash recovery | M | Every 30 s, copy tabs with unsaved changes (and untitled tabs) to `~/Library/Application Support/Sloth/recovery/`, and delete the copy when the tab is saved or closed. At launch, offer to restore any copies left behind. *Not done yet: keeping untitled tabs across a normal quit (for now Sloth still asks to save them).* |
| Atomic saves | S | Write to a temporary file and rename it over the original, so a crash mid-save can't leave a half-written file. |

## 0.5.0: Comfortable editing ✅

| Item | Size | Approach |
|---|---|---|
| Auto-indent | M | Catch Enter before the text widget sees it, and insert a newline plus the current line's leading whitespace. |
| Indent / outdent (⌘] / ⌘[, Shift-Tab) | M | Apply to every line in the selection; egui's own Shift-Tab only handles one line. |
| Tabs vs spaces, tab width | S | Setting under View → Indentation; Tab inserts spaces when set. |
| Highlight current line | S | Paint a band behind the cursor's row, using row positions from the previous frame's layout. |
| Matching brackets | M | When the cursor is next to `()[]{}`, highlight its partner. Reuses the highlight code from find. |
| Word and character count | S | In the status bar, for the selection or the whole document. Only recount when the text changes. |
| Font choice | M | View → Font: pick from installed monospace fonts (loaded by file path into egui's font set) and save in Settings. |
| Drag to reorder tabs | S | Drag and drop in the tab bar. |
| Close Other Tabs, Close Tabs to the Right | S | Right-click menu on a tab. |

## 0.6.0: Better search ✅

| Item | Size | Approach |
|---|---|---|
| Regex search | M | `regex` crate, with a ".*" toggle in the find bar; replacements can use `$1` captures. Show a message when the pattern is invalid. |
| Search all tabs | M | ⇧⌘F opens a results panel listing file:line:snippet; clicking a result jumps there. |
| Search history | S | Up and Down arrows in the find field step through recent searches (saved in Settings). |
| Faster matching | S | Cache matches until the text or the search changes, instead of searching again every frame. |

## 0.7.0: Code and Markdown ✅

| Item | Size | Approach |
|---|---|---|
| Syntax highlighting | L | `syntect`, chosen by file extension and driven through the existing layouter. Cache per line so typing only re-highlights what changed. Colours come from the theme palette. Off by default for `.txt`. |
| Markdown preview | M | Side panel rendered with `egui_commonmark`. View → Markdown Preview. *Not done yet: scrolling the preview along with the editor, and showing images.* |

## 0.8.0: Printing and windows ✅

| Item | Size | Approach |
|---|---|---|
| Print (⌘P) | M | Put the text into an off-screen `NSTextView` and run `NSPrintOperation`. That gives the standard macOS print dialog and Save as PDF. *Prints plain text for now: no syntax colours or line numbers.* |
| Multiple windows | L | Use egui's multi-viewport support. Each window gets its own tabs and find bar; menus act on the focused window. Requires splitting window state out of `SlothApp`. *Extra windows are drawn from the main window's frame; when the main window is closed it's hidden rather than closed.* |

## 1.0: Polish and decision ✅

- ✅ App icon (drawn by `scripts/make-icon.swift`).
- ⏭ Signing and notarisation: skipped. Sloth is only used on its author's
  Mac, where it opens without them.
- ✅ Settings window (⌘,).
- ✅ Performance review. Without accessibility tools running: about 4 ms per
  idle frame and 23 ms per typed character at 10 MB, and 20 ms / 110 ms at
  50 MB. **Decision: stay on egui.** Revisit the native `NSTextView` switch if
  files over ~20 MB, spell check or VoiceOver become important. Known
  limitation: with an accessibility tool active, egui rebuilds the text
  widget's accessibility tree every frame (about 200 ms at 10 MB).

## After 1.0

Smaller ideas, all done in 1.1.0 ✅:

- Keep untitled tabs across a normal quit (they're only kept after a crash).
- Recent files in the Dock icon's menu.
- Markdown preview that scrolls with the editor, and shows images.
- Print with syntax colours and line numbers.
- Copy Finder tags and other extended attributes when saving.

---

## How each release gets done

1. Branch from `master` and build the items, each with unit tests where the
   logic can be tested on its own (as `find.rs` and `tabs.rs` are now).
2. Run UI checks headlessly with `egui_kittest`. Hands-on testing in the
   running app is done by you, or by Claude only when you've said it's OK to
   use the screen.
3. Bump the version, add a `CHANGELOG.md` section, commit and tag.
