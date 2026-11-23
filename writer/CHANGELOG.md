# Changelog

Sloth Writer uses semantic versioning (major.minor.patch).

## 1.15.0

Writing on the web, with paragraphs as Sloth Writer shows them.

- A new writing area for the web editor: each paragraph spaced and indented as in Settings → Text (paragraph spacing and first-line indent come over with each sync). It looks and saves as before: plain Markdown, one line a paragraph, with em dashes, drafts, notes and clashes all working the same.
- It's switched on one device at a time: Account → Writing on this device → Paragraphs as in Sloth Writer. Turn it off for the plain writing box.
- Pasting brings plain text (no fonts or bold from elsewhere), and the formatting keys do nothing, so what's saved is always what you typed.

## 1.14.0

Mobile Notes: a notepad for jotting on your phone when you're out and about.

- On the web, every project in the list has a pen button beside it that goes straight to its notes, and Mobile Notes is first in the project. New Entry adds the date and time as a heading, with the cursor under it; it opens at the end, where the newest notes are. Notes written there don't count as the day's writing.
- On the Mac, More… → Mobile Notes opens it beside what you're writing, making it in Notes the first time. Or make any document the notepad: tick Mobile notepad in its inspector (one per project). It's on the web either way, and shows 📝 in the binder.
- Start it on the phone before the Mac has one, and the Mac makes it at the next sync.
- Added to on both the Mac and the phone between syncs: both sets of notes are kept, the Mac's then the phone's, rather than one going to a snapshot.

## 1.13.0

The Canvas: a place to think about the project as a whole, apart from the order it's in. Click Canvas in the toolbar (⌥⌘C).

- Every document is a card on one surface you can pan around and zoom (pinch, or ⌘-scroll). Drag cards wherever you like: the binder's order doesn't change. Show… chooses which folders' documents are on it.
- Documents are strung to the profiles they refer to, by `[[references]]` and by links made in the inspector; the more references, the heavier the string. Hover a string to see what it stands for.
- Click a card to pick it (and show it in the inspector): its strings and neighbours stand out and the rest fades. Double-click to open it in the split pane.
- Notes of your own: double-click the empty canvas, or New Note. Right-click a note to make it a document in Notes, its first line the title.
- Strings end at the cards' edges, so they never cross the text. Tie strings by hand: drag from the pin on a card's top edge to another card (again to untie). Right-click a string to say what it means; click it and press Delete to untie it.
- Where cards are, notes and strings are kept in the project's `.sloth` folder (and its backups), not the project file.
- The corkboard has a faint grid of dots behind its cards.

## 1.12.1

- Remove Blank Lines has a shortcut: ⇧⌘L.

## 1.12.0

References: point a passage at a profile — a source, concept, theme, character, place — by writing `[[its name]]` where it comes up. Like a note in the margin, a reference isn't part of the prose.

- References show small and blue in the text. Hover one to see what it refers to; ⌘-click to open it in the split pane. `[[Concept: attention]]` picks the kind when two share a name, and `[[Ways of Seeing|p. 47]]` adds a page.
- Typing `[[` suggests profiles' names as you type (↑↓ to choose, Return to put one in, Esc to close), and offers to make a new one if nothing matches. ⌘-clicking a reference to a profile that doesn't exist yet offers the same.
- Each profile lists where it's referenced, by document and section, with the line it's on; click to go there. A section starts at a heading, or at a separator line (`#`, `##`, `***`, `+++`) with its title on the next line.
- In This Scene shows the profiles a document refers to, alongside the ones linked by hand.
- Renaming a profile renames its references.
- Compile leaves references out, as it does notes, and they don't count as words. Or, under Sources: references to profiles with a Citation section (Sources have one) become footnotes in Word, notes at the end of each chapter in an e-book, or pandoc footnotes in Markdown, and the works cited can be listed at the end.
- Themes can be added as a profile folder in non-fiction projects too.

## 1.11.0

- Recent projects on the start screen are tiles: each with an icon and colour for the kind of writing it is — Novel, Short Fiction, Essay, Nonfiction, Blog, Podcast Script, Screenplay, Stage Play, Poetry, Journal, Personal, Letters, Children's, Fantasy, Mystery, Science Fiction, Horror, or just Writing. Right-click a tile and choose Icon to change it, or click the project's title in the toolbar while it's open. The kind is kept with the project.

## 1.10.0

- Hide the Markdown marks (Settings → Writing): the `**`, `*`, `#` and `>` disappear and the text closes up, except in the paragraph you're typing in, where they show faintly so you can still change them. Scene breaks always show.
- Copy as formatted text (Settings → Writing): Copy and Cut give other apps real bold, italics, headings and quotes rather than `**` and `*`; notes are left out, and plain-text apps get the words without the marks. Pasting back into Writer keeps the Markdown.

## 1.9.0

- First-line indent (Settings → Text): each paragraph's first line starts a little way in, from none to 4 em (1 em is the font size). Like paragraph spacing, it's only how the text looks; no spaces are added.
- Remove Blank Lines in the right-click menu: takes the empty lines out of the selected lines (or the cursor's line). What's left stays selected.

## 1.8.0

- Paragraph spacing (Settings → Text → Paragraphs): extra space after each paragraph, from none to two lines, so a Return looks like a paragraph break. Blank lines you type get none, so they don't double up. The text itself doesn't change.

## 1.7.0

- Focus mode (⌥⌘F, or Focus in the toolbar): full screen, just the text in its column, with a faint line of today's figures at the bottom. Esc leaves it.
- Typewriter scrolling keeps the line you're typing in the middle of the screen: in Focus mode, always, or never (Settings → Writing).
- Statuses can be edited like labels: Edit Statuses… at the foot of the Status menu renames, reorders, adds and removes them. Every document keeps its status through a reorder.
- The toolbar fits again: Back Up…, Settings, Help and Close Project are in its More… menu (⌘, and ⌘? still work).
- Settings scroll on a small screen, with Restore Defaults always in view.

## 1.6.0

- Import from Scrivener… (on the start screen): makes a Writer project from a Scrivener 3 project, which isn't changed. The Draft becomes the Manuscript, chapters and all; Research, Trash, and Characters and Places folders keep their places; the rest goes into Notes. Text keeps its bold, italics, headings, quotes and lists; synopses, notes, labels, statuses, keywords, include-in-compile, targets and snapshots come across; comments, annotations and footnotes become notes in the text. PDFs, pictures and sound files are copied into the project folder. An import report in Notes says what came across and what didn't.

## 1.5.0

Keeping your work safe.

- Automatic backups: when a project closes (or Writer quits), and once a day while it's open, if anything changed. The newest ten are kept, in ~/Library/Application Support/Sloth Writer/Backups unless you choose another folder (Settings → Backups). Back Up… still makes one whenever you like, and those are never deleted.
- One copy at a time: a project open in another copy of Writer (on another Mac sharing it, say) asks before opening it here too. If it's opened elsewhere anyway, this copy stops saving and says so, keeping anything typed since as snapshots.
- Emptying the Trash keeps what it removes (each document under its title, with its snapshots) as one zip in your Mac's Trash, so it can still be got back.
- Fixed: undo after Bold, Italic, Change Case or a spelling fix took back the typing just before it too. Each is now an undo step of its own.

## 1.4.0

- A document's notes panel can be made taller or shorter: drag its bottom edge.
- The web helper gets each scene's notes, editable there and synced back (if both sides changed them, both are kept), and today's words against your goal, with the manuscript's length and target.
- The web editor's scene list is grouped by chapter (chapters fold away), with word counts against targets, a Continue link to where you were, and Find. Writing a scene shows its words and today's goal as you type, and links to the scenes before and after.
- Fixed: in the web editor, the page no longer jumps back to the top while you type in a long scene.

## 1.3.0

- Split at Cursor (⌘K, or right-click): the text after the cursor moves into a new document just after this one, ready to name.
- Change Case in the right-click menu: UPPERCASE, lowercase, Title Case and Sentence case.
- A document's notes are now in a panel above its text (folded away when empty). They're still never compiled.
- The inspector is tidier: the title with its words and target at the top, then Synopsis, Details, In This Scene and Snapshots, each of which folds away. The synopsis box starts small and grows with what you write.
- Settings are grouped: Text (with the sample right under it), Writing, Appearance and Web Helper.
- Hide the binder with the Binder button or ⌥⌘B.
- With text selected, the status bar shows its words too: "12 of 340 words".
- Back Up…: the whole project as a dated zip file in a folder of your choice.
- Non-fiction profile folders: Sources, People, Concepts, Arguments, Events and Glossary, beside the fiction ones.
- The Characters folder can be removed once it's empty, and added back from New Profile Folder.
- "Ongoing" status, after To do.
- Fixed: a long synopsis no longer runs out of its card on the corkboard.

## 1.2.0

- Spell checking as you type, with your Mac's spelling language and learned words: misspelled words get a red wavy underline (not the word you're still typing, and not in notes). Right-click one for suggestions, Learn Spelling or Ignore Spelling. Turn it off in Settings.

## 1.1.0

- Writing statistics: the status bar shows the words written today, against your goal if you set one. Click it for the Statistics window: today's progress, your streak, the last 30 days as a chart, the manuscript's length, your average, and words per chapter.
- Goals: words a day, a length for the manuscript, and a date to finish by. With a deadline and no daily goal, the day's goal is what the deadline needs.
- Words are counted as you type in the Manuscript, net: cutting counts against the day, and moving or trashing documents doesn't count. Words from the web count on the day they sync.

## 1.0.0

The first full release: a writing app for long projects, built on Sloth.

Projects and the binder
- A project is a folder of plain Markdown files you can open in any other program, arranged in a binder: Manuscript, Characters, Research, Notes and the Trash.
- Drag documents and folders to reorder them, nest them or move them between folders. New documents (⌘N) and folders (⌥⌘N) are named by typing.
- Recent projects on the start screen, and the project can be renamed from the toolbar.
- Everything saves itself every few seconds and on quitting. If another program changes a file, Writer never writes over it: it reloads unedited documents and asks about edited ones.

Writing
- Prose in a reading face, in a centred column, with your choice of font, size, line spacing and width (Settings, ⌘,).
- Two hyphens make an em dash (not inside notes).
- Markdown shown as you type: real *italics*, **bold** and headings, with the markers dimmed.
- Right-click for Bold, Italic, Heading, Quote, Note and Scene Break, or use ⌘B and ⌘I.
- Notes to yourself (`<!-- like this -->`) are left out of the word count and the compiled book.
- Split view (⌘\): a second pane beside the editor for another scene, a note or a character.
- Find in the document (⌘F) and across the whole project (⇧⌘F).

Planning
- The inspector (⌥⌘I): synopsis, status, label, keywords, a word target and notes for each document.
- Corkboard and outline views for any folder.
- The character creator: a profile built from blocks such as Motivations or Physical Appearance, linked to the scenes a character appears in.
- More profile folders in the same style: Locations, Objects, Factions, Worldbuilding, Themes, or your own.

Versions and output
- Snapshots (⌘5): keep versions of a document, compare them word by word, and restore one.
- Compile (⇧⌘E) the manuscript to a standard Word manuscript, an ePub or Markdown, with your contact details, Letter or A4 paper, Times or Courier, chapter headings, scene break styles and saved presets.

On the web
- Mark scenes to write on the web, and sync them with your own Sloth Writer helper on a PHP web host. Edits from either side are kept, never overwritten, and new scenes written on the web arrive in the Inbox.
