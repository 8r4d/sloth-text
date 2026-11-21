# Changelog

Sloth uses semantic versioning (major.minor.patch).

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
