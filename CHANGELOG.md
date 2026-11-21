# Changelog

Sloth uses semantic versioning (major.minor.patch).

## 0.7.0

- Syntax highlighting for code, Markdown and config files (about 75 languages, detected from the file name or a `#!` line). Colours follow the chosen theme. Turn it off in View → Syntax Highlighting. Plain text and files over 2 MB aren't highlighted.
- The status bar shows the language.
- Markdown preview (⌥⌘P, View → Markdown Preview): a rendered view beside Markdown documents. Images aren't shown yet.

## 0.6.0

- Regular-expression search: turn on ".*" in the find bar. Replacements can use $1, $2 or ${name} to insert parts of the match. An invalid pattern shows a short explanation.
- Find in All Tabs (⇧⌘F, or "All Tabs" in the find bar): a panel listing every match in every open tab, with line numbers. Click a result to jump to it.
- Recent searches: ↑ and ↓ in the find field step through them. They're remembered between launches.
- Faster searching: all searches use the regex engine, and results are reused until the text or the search changes.

## 0.5.0

- Auto-indent: a new line starts with the previous line's indentation (View → Indentation → Auto-Indent).
- Indent and Outdent selected lines (⌘] and ⌘[); Tab and Shift-Tab do the same for multi-line selections.
- Indent with tabs or spaces, and a tab width of 2, 4 or 8 (View → Indentation).
- The current line is highlighted (View → Highlight Current Line), and matching brackets are outlined.
- Word count in the status bar, or words and characters for the selection.
- View → Font: choose any installed fixed-width font, such as Menlo or SF Mono.
- Drag tabs to reorder them. Right-click a tab for Close Other Tabs and Close Tabs to the Right (also in the File menu).
- The status bar shortens long file paths instead of overlapping the details on the right.

## 0.4.0

- Files keep their text encoding when saved. Non-UTF-8 files (e.g. Windows Latin 1, Shift JIS) are detected on open instead of being damaged.
- File → Reopen with Encoding and File → Convert to Encoding, with 17 common encodings.
- Line endings (LF or CRLF) are kept when saving, and can be changed in File → Line Endings.
- The status bar shows the encoding and line endings, e.g. "UTF-8 · LF".
- If characters can't be stored in a file's encoding, Sloth offers to save as UTF-8. If a file wasn't read cleanly, Sloth asks before saving over it.
- Saves are atomic: a temporary file is written and then swapped into place, so a crash or full disk can't leave a half-written file. Permissions and symlinks are kept.
- Crash recovery: copies of unsaved documents are written every few seconds, and Sloth offers to restore them after a crash.

## 0.3.0

- Open files from Finder: double-click (once Sloth is the default app for a type), Open With, or drop them on the Dock icon.
- File → Open Recent, with Clear Menu.
- Tabs that were open when you quit are reopened next launch.
- Files changed by another program reload automatically; if you have unsaved edits, Sloth asks whether to reload or keep your version. Deleted files are flagged.
- Go to Line (⌘L), with optional column (e.g. 12:5).
- File → Revert to Saved.
- The text area is now named for VoiceOver.
- scripts/install.sh builds Sloth.app and installs it in Applications.

## 0.2.0

- Tabs: open several documents at once. ⌘W closes a tab; Window → Show Next/Previous Tab switches.
- Open… can select several files; dropping several files opens them all.
- Find and replace (⌘F, ⌥⌘F), with match case and whole-word options, Find Next/Previous (⌘G, ⇧⌘G) and Replace All.
- Save dialog offers Plain Text, Markdown or Any Format, and adds the extension automatically.
- About window with version and build information.
- Help menu with a keyboard shortcut reference and these release notes.

## 0.1.0

- Plain-text editing with line numbers, word wrap and zoom.
- New, Open, Save and Save As, with a prompt before losing unsaved changes.
- Native macOS menu bar.
- Colour themes, and settings remembered between launches.
