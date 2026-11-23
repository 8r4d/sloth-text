# Changelog

Sloth Writer uses semantic versioning (major.minor.patch).

## 1.4.0

- A document's notes panel can be made taller or shorter: drag its bottom edge.
- The iPad helper gets each scene's notes, editable there and synced back (if both sides changed them, both are kept), and today's words against your goal, with the manuscript's length and target.
- The iPad's scene list is grouped by chapter (chapters fold away), with word counts against targets, a Continue link to where you were, and Find. Writing a scene shows its words and today's goal as you type, and links to the scenes before and after.
- Fixed: on the iPad, the page no longer jumps back to the top while you type in a long scene.

## 1.3.0

- Split at Cursor (⌘K, or right-click): the text after the cursor moves into a new document just after this one, ready to name.
- Change Case in the right-click menu: UPPERCASE, lowercase, Title Case and Sentence case.
- A document's notes are now in a panel above its text (folded away when empty). They're still never compiled.
- The inspector is tidier: the title with its words and target at the top, then Synopsis, Details, In This Scene and Snapshots, each of which folds away. The synopsis box starts small and grows with what you write.
- Settings are grouped: Text (with the sample right under it), Writing, Appearance and iPad Helper.
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
- Words are counted as you type in the Manuscript, net: cutting counts against the day, and moving or trashing documents doesn't count. Words from the iPad count on the day they sync.

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

On the iPad
- Mark scenes to write on the iPad, and sync them with your own Sloth Writer helper on a PHP web host. Edits from either side are kept, never overwritten, and new scenes written on the iPad arrive in the Inbox.
