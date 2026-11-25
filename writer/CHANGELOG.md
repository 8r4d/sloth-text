# Changelog

Sloth Writer uses semantic versioning (major.minor.patch).

## 1.53.0

Your web helper as the hub between computers.

- Project Settings → Web helper → "Sync the whole project": every document and the binder go to your web helper, not only the ones marked for the web, and each computer that has the project keeps its own copy in step through it. No Dropbox or iCloud needed.
- On another computer, Get Project from the Helper… (the start screen's fourth icon) lists your helper's projects and brings one over, everything in it, into a folder you choose.
- When both computers changed the project while apart, the two are merged: each side's new documents are kept, renames and moves and inspector changes come through, a document trashed on one computer goes to the Trash on the other (unless it was changed there meanwhile), and where both changed the same thing, this computer's is kept and the other's is noted in the item's Notes or kept as a snapshot. The status bar says what came from which computer.

Upload the helper's new api.php to your host.

## 1.52.0

Writing on two computers through a synced folder (Dropbox, OneDrive, iCloud, Syncthing).

- When two computers change the same document while out of touch, the sync service keeps both, the second under a changed name, and Writer used to ignore it. Now a bar says what it found ("a copy of “Walking” from Writing PC"), and Bring Them In puts each copy just after its original in the binder, to compare and keep what you want. A copy of the project file itself is pointed out too, since binder changes made on the other computer won't be showing.
- A project open on another computer is recognised even when the two computers' clocks differ by minutes, or the synced folder is slow to bring the news, so it still asks before opening it here.

## 1.51.1

- Documents written on Windows (with its line endings, or a byte-order mark, as Notepad saves them) read the same everywhere in Writer: in compiling, syncing, searching, statistics, profiles and snapshots, not only in the editor.
- Checked: a project kept deep in folders, past Windows' old 260-character limit for a path, works.

## 1.51.0

- Writer has its own icons: clean line icons in place of emoji everywhere (the binder, the toolbar, the start screen, each kind of project, the rewards). They take the theme's colours, so they suit every theme, the Severance ones especially, and look the same on Mac, Windows and Linux. Drawn from Lucide (lucide.dev, ISC licence).
- The finale's falling leaves are drawn in the theme's colour, and spread more evenly.

## 1.50.0

- Rewards that grow. On top of the day's cheer: writing 3, 7, 14 and 30 days in a row, and the manuscript reaching a quarter, half and three quarters of its target, each earn a small prize, once. Reaching the target itself is a party, with leaves falling across the window. What you've earned is on a shelf in Statistics. (App Settings → Writing → "Celebrate the day's goal, streaks and milestones".)
- The elevator: opening a project, a panel carrying its name slides up and away as it arrives (and back to the start screen the same way). Not with reduced motion; turn it off in App Settings → Appearance.
- With a Severance theme, an "Old screen glow" in App Settings → Appearance: faint scanlines and darker edges, like an old monitor. Off unless you turn it on.

## 1.49.0

- Focus mode shows progress. Across the top, the document's title and how far the manuscript has come ("53% complete", or its words with no target). Along the bottom, a bin for each chapter (each top-level item in the Manuscript), filling towards its share: its own target, its documents' targets, or the manuscript's target shared evenly. The chapter you're in is outlined. When a bin first fills, its lid lifts and settles, once, and a tick stays. Turn it off in App Settings → Writing.
- The start screen's New, Open and Import buttons are a row of three icons, each named when you point at it.

## 1.48.0

- The start screen has a fact about your work under the greeting: one calm, true sentence a day from your writing statistics ("You wrote 1,204 words on Tuesday." "Bellingham is 38% of the way to its target."). Click it for another. Turn it off in App Settings → Appearance.
- With a Severance theme, the start screen's sloth is a one-colour emblem in the theme's colour, inside a ring.

## 1.47.0

- Severance Light: the daytime office to go with Severance, deep teal on cool white. In App Settings → Appearance, with the light themes.
- Fixed: the card file now flips with the scroll wheel anywhere over it, the front card included (it only did between the cards).

## 1.46.0

- A new dark theme, Severance: pale cyan on blue-black, after the office terminals in the TV show. In App Settings → Appearance (and in Sloth).

## 1.45.0

The card file.

- Recent projects on the start screen can be a card file instead of tiles (Card File, beside Recent projects; remembered). Your projects stand on a rotary file, A to Z, one in front with the next few behind it, each with a tab showing its letter. Flip with the arrow keys or the scroll wheel, type a letter to jump to it, and press Return or click the front card to open it; click a card behind to bring it forward. The front card shows its kind, words, when it was last written in, and its target. Right-click it to change its icon or take it off the list.

## 1.44.0

Windows and Linux, nearly at home. Nothing changes on the Mac.

- Spell checking on Windows and Linux, with English dictionaries (US and UK) built in: a word is right if either has it. Learn Spelling keeps your words; Ignore Spelling lasts until you quit. Words with numbers in them, and web and email addresses, aren't checked.
- The web helper's token is kept in Windows' Credential Manager (on Linux, in a file only you can read), so web sync works there.
- Empty Trash moves what's emptied to the Recycle Bin (or the desktop's trash on Linux).
- The little animations follow Windows' "Animation effects" setting, and GNOME's on Linux.
- The default prose font is Georgia on Windows, and Noto Serif or DejaVu Serif on Linux.
- Downloads: each release on GitHub has a Windows zip (Sloth Writer.exe, with its icon and no console window) and a Linux download with an install script.

## 1.43.0

Getting ready for Windows and Linux: shortcuts, and checks on every system.

- On Windows and Linux, shortcuts are shown as those systems write them (Ctrl+K, Ctrl+Shift+F) in tooltips, menus and Help, rather than with the Mac's symbols. Nothing changes on the Mac.
- The Mac's ⌥⌘ shortcuts are Ctrl+Shift on Windows and Linux, not Ctrl+Alt, which many Windows keyboards use to type characters such as @ and €. Focus mode is F11 there, since Ctrl+Shift+F is Find in Project.
- Every change is now built and tested on macOS, Windows and Linux.

## 1.42.3

- On Windows, automatic backups are kept in your own app data folder (%LOCALAPPDATA%\Sloth Writer\Backups), not the temporary folder, which Windows may clear. Any made there already are moved over the first time. On Linux they're in ~/.local/share/sloth-writer. Nothing changes on the Mac.
- On Windows and Linux, if Writer stops on an error, what went wrong is now written to a crash log, as on the Mac: in %LOCALAPPDATA%\Sloth Writer\Logs, or ~/.local/state/sloth-writer.

## 1.42.2

More getting ready for Windows and Linux. Nothing changes on the Mac.

- Web addresses and the backups folder open the way each system does. The backups folder's button says Show in Explorer on Windows and Open Folder on Linux.
- When a project is open on another computer, it's named on Windows and Linux too, or called "another computer" when there's no name to show.

## 1.42.1

Getting ready for Windows and Linux: names that work everywhere.

- New file and folder names avoid what Windows doesn't allow, so a project can go to a Windows computer (through a synced folder, say). The characters < > : / \ | ? * become a hyphen and quotes are left out, a name doesn't end in a dot or space, and names Windows keeps for itself (Con, Aux, Nul, Com1…) get a hyphen. This is for document files, the project folder when it's renamed, backups, snapshots, and research files from Scrivener. Existing files keep their names.
- A new document's file is never named like one already there except for capitals (walking.md beside Walking.md), which Windows and the Mac count as the same file.

## 1.42.0

Project Settings.

- A Project Settings window (More… → Project Settings…, or ⌥⌘,) gathers what belongs to the open project: its title and icon, where new items go, word goals, statuses, labels, and each kind of profile's sections. Statuses, labels and sections open their own editors from here, as they still do from where they're used.
- New: each project can add new documents and folders at the top of their folder, newest first, for a journal or blog. Off (as before), a new item goes after the selected document, or at the bottom of the selected folder, for writing a story start to finish. In Project Settings → Binder.
- Settings is now App Settings (⌘,): what's the same for every project.

## 1.41.1

- Fixed: on the web, a new document sent just as the connection dropped (so the reply never came) could arrive twice when sent again, two copies in your Mac's inbox. Each is now sent with the page's own name for it, and the helper keeps one. The same goes for your words saved aside by "Use the Mac's" in a clash.

Upload the helper's new api.php and app.js to your host; its database is brought up to date by itself on the first request.

## 1.41.0

The web editor, offline.

- Each project you open on the web is kept on that device: its list of documents, and their text and notes, updated each time with just what's changed. Without a connection (on a plane, say), you can still open the app, go through your projects and folders, and read and write any of their documents, not only ones you'd started; a note at the top says you're looking at what's kept on the device, and from when. What you write is sent when you're back online, and if your Mac changed the same document meanwhile you choose whose words to keep, as ever.
- A connection that hangs (a weak signal) no longer leaves the page saying "Saving…" with Back and the trail waiting on it: after 15 seconds it counts as offline, the words kept on the device and sent later.
- Fixed: if the connection dropped just as a save reached the helper, so the reply was lost, the next try said your Mac had changed the document. Now the helper having your very words counts as saved.

Upload the helper's new app.js, style.css and sw.js to your host.

## 1.40.2

- Nested rows in the outline step in as a whole, handle and all, so what's inside what reads at a glance, as in the binder. The Status, Label and Words columns stay lined up.

## 1.40.1

- Outline rows have more room above the title: the title and synopsis sit in the middle of the row, clear of its top edge.

## 1.40.0

The web editor's project page, redone for bigger projects.

- Documents are shown in the binder's folders as they are, nested: Manuscript, then a part, then its chapters, each set in with a faint line down its side, with how many documents and words are in it, subfolders and all. Before, only the folder a document was directly in came across, so parts and nesting were lost (and two folders of the same name were muddled).
- A chapter that's a document with documents inside (after Break Up) is a folder, its own text first in it.
- Open All and Close All beside Find. Folding is remembered folder by folder.
- Find lists every match together, each with where it is (Manuscript › Part One › Chapter 1), so two of the same name are told apart.
- Writing a document, where it is shows above its title, each folder a link back to it in the project page (opened and brought into view). Back goes to its folder, and the next or previous document says which folder it's in when it's another.
- A document's label shows as a thin bar of its colour.
- The page says documents, not scenes.

Writer now sends each document's folders, from its top-level folder down, and its label, at each sync. (Upload the helper's new app.js and style.css to your host too.)

## 1.39.0

- Eight more label colours, sixteen in all: lime, sky, indigo, lavender, burgundy, brown, olive and slate join red, orange, yellow, green, teal, blue, purple and pink. Labels you have keep their colours.
- In the Labels window, click a label's dot to pick its colour from swatches (named when you point at them), rather than stepping through them one by one.

## 1.38.1

- The outline's handle is now an end cap on the row: its left end in the label's colour, rounded on the outside and wide enough to take hold of, with thin lines of the same colour running on along the row's top and bottom and fading out a quarter of the way across.

## 1.38.0

- The outline, redesigned: rows sit on the usual background again, and each has a handle at its left, a rounded bar in its label's colour (a quiet grey without one), with grip dots on it to drag the row by. Status and Label are back to their usual pulldowns. This replaces 1.36.0's tinted rows.

## 1.37.0

- Outline rows have a grip on the left (six dots, a hand under the pointer) to drag them by, to reorder them or drop one into another, as the binder does. (The whole row always dragged; the grip says so.) Not while filtered by keyword.
- The outline breathes more: a little space between rows, and the title and synopsis off the edge of the tint.
- Status and Label in the outline no longer sit in grey boxes on the label's tint: just the choice and its arrow, until the pointer's on them.

## 1.36.0

- Outline rows are tinted with a light wash of their label's colour (a little deeper under the pointer), so labels stand out down the list. Rows without a label are as before.

## 1.35.0

- Filter a folder's corkboard or outline by keyword: the menu beside Outline lists the keywords used in the folder, with how many have each. Pick one to see everything that has it, from anywhere in the folder (on the Manuscript, across every chapter), in binder order; All keywords goes back. Cards can't be dragged to reorder while filtered.
- A globe (🌐) beside the title of a card or outline row with a web address: click it to open where it's published.

## 1.34.0

- Copy, beside a document's word count in the inspector: its text, with its bold, italics, headings and quotes, on the clipboard to paste into WordPress (or anywhere). Notes, comments and references are left out, and the title, which WordPress has a field of its own for.
- Break Up and Combine sync first when what they'd change is on the web, so anything written there since the last sync comes in before the documents change. They wait for a sync begun after they opened; if it can't sync, they say why and can go ahead anyway.

## 1.33.0

- Set Status for All Inside and Set Label for All Inside, in a folder's right-click menu in the binder: what's directly in the folder gets the status or label you pick. Tick Include subfolders (at the top of either menu) to reach into its subfolders too; that stays as you left it while Writer's open. The choice they all have already is highlighted, and the status bar says how many changed.

## 1.32.2

- In Slothenings, each document's title in the rule above it is bigger (the interface's usual size), easier to read and to click.
- Fixed: the split view's × didn't close it (the click was undone the moment it was made). ⌘\ and the Split button always worked.

## 1.32.1

- The inspector's Links: what a document links to is the same size and look as where it's linked from.

## 1.32.0

- Common words of your own: in Statistics, right-click one of the most used words and choose Leave Out. It's listed under Left out, where a click brings it back, and all of them are in Settings → Writing → Common words. Like the little words Writer already leaves out, they're never tinted as repeats either. They're yours, so they apply to every project.

## 1.31.0

- A Web address for each document (and folder), in the inspector's Details: where it's published, say. Open beside it takes you there in your browser. `https://` can be left off; only web addresses open.
- Openings in the outline and on the Canvas too: a document with no synopsis shows its first words, faint and in italics, in its outline row (cut short to fit) and on its Canvas card (two lines). On the Canvas they're gathered every few seconds, with the strings.

## 1.30.0

- A corkboard card for a document with no synopsis shows how the document opens: its first words, faint and in italics, as they read (no Markdown, notes or headings). Click in to write a synopsis, which takes their place.
- Fixed: Break Up counted the headings it turns into titles as words cut, so a day of breaking up chapters started below zero, and Today (which never shows less than 0) stayed at 0 while you wrote. Now every word Break Up and Combine move counts as moved, headings and titles included.
- Fixed: a sprint on a day with more cut than written stayed at 0 words. It now counts from where it started, whatever the day's total.

## 1.29.2

- If Writer ever closes on an error, what went wrong is kept in ~/Library/Logs/Sloth Writer/crash.log (when, the version, the message and where in the code), so it can be fixed without having to make it happen again.

## 1.29.1

- Fixed: moving a profile's pill on the Canvas could close Writer. Working out where a string meets the pill's rounded end could trip over a rounding error a hair wide, depending on exactly where the pill was while carried.

## 1.29.0

A tidier Canvas.

- Depth, above the Canvas: 1, 2, 3 or All. At 1, what's straight inside each top-level folder gets one card (a chapter, say), standing for everything in it, and the strings from what's inside are gathered onto it, thicker for more references. Hover a string to see the count. At 2, it's the next level down, and so on.
- A folder's card has a tab and tells how many documents are in it; a document holding others shows "+ 12 inside". Each one first sits where the first thing inside it already is. Double-click it to see the folder in the split pane.
- Folders (ticked to start with): each folder's cards are outlined, with its title, the outer folders' outlines wider.
- Strings tied by hand to something inside a card follow it onto that card.
- New cards arrive a folder at a time, each folder's on rows of its own, rather than mixed together.
- Nothing moves when the depth changes: every card keeps its place for when it's shown again.

## 1.28.0

Break Up and Combine.

- Break Up… (right-click a document in the binder): each part becomes a document of its own inside it, named from its heading. Break at `#` headings, `#` and `##`, deeper, any heading, or at scene breaks (a piece after a break is named from its first words). The window lists what it'll make before anything changes.
- Text before the first break stays in the document; with none, it becomes a folder. A snapshot of the whole text is kept first, and each new document keeps the status, label and keywords. Headings come out of the text unless you keep them.
- Combine into One Document… (right-click a folder) puts everything in it back into one document, each under its title as a heading (`#`, `##` or `###`, a level deeper for subfolders), with a scene break between each, or just one after another. Combine with Next… does the same for a document and the one after it. What's taken in goes to the Trash, so it can still be got back.
- Words moved either way don't count as written today.
- Compile: Documents → Their titles as headings, for essays and posts: each document in a chapter under its own title. A document (or scene) that opens with a heading gets no break mark before it.

## 1.27.0

Writing on the web.

- The paragraph editor is now the one every device gets (Account → Writing on this device still has the plain box, for a device that wants it).
- ⌘B and ⌘I, and an iPad keyboard's B and I buttons, wrap the selection in `**` or `*` (and take them off again), in either editor.
- The web editor shows what its Markdown does, as Writer does: bold, italics, headings, quotes and scene breaks, the marks faint, notes and comments quieter. A paragraph is restyled once you've moved out of it, so nothing shifts while you type, dictate or autocorrect.
- Each project shows the day's goal as a ring, as in Writer's status bar, and your streak ("5 days in a row"), sent with each sync.

## 1.26.1

- Fixed: with a folder as Slothenings, opening one of its documents in the split pane (or the folder in the split pane beside one of its documents) made two editors of the same text, which clashed. The split pane now says it's open in the main editor, as it does for the open document.

## 1.26.0

Slothenings.

- Any folder can be read and written as one text: Slothenings, beside Corkboard and Outline (⌘1; ⌘2 and ⌘3 for the other two). Every document in it, subfolders and all, follows the one before, each under a faint rule with its title. From a document, ⌘1 shows the folder it's in with the cursor where it was.
- Each part is still its own document, with its own file and undo. The arrow keys run on across the joins: ↓ or → at the end of one goes into the next, ↑ or ← at the start back into the one before. Click a title to open that document on its own.
- With a folder showing, the status bar counts all its words.

## 1.25.0

Comments on passages.

- Select some words and press ⌥⌘K (or right-click → Add Comment): the words get a soft amber tint, and a card in a column beside the text, level with them, to write the comment in. With nothing selected, the comment goes at the cursor.
- Each card quotes its passage (click to go there) and has Resolve, which takes the comment away and leaves the words.
- In the text, comments are CriticMarkup, `{==the words==}{>>the comment<<}`, so they stay with their words through edits, splits and syncing. Their marks are kept out of sight except in the paragraph you're writing.
- Compile, word counts, copying as formatted text and the web editor's Preview all leave comments out.

## 1.24.0

Overused words.

- Highlight Repeated Words (⌥⌘R, or More…): a word used again within about 100 words is tinted, both times, so echoes stand out; common little words don't count, nor do notes or references.
- Words you lean on are underlined wherever they come: a watch list in Settings → Writing, starting with just, really, very, suddenly, actually, began to and the like. Phrases work too.
- Statistics lists the manuscript's most used words; click one to find it through the project.

## 1.23.0

Writing sprints.

- Sprint, in the status bar beside the goal ring: write for 10, 15, 25, 45 or 60 minutes. The time left (as a ring emptying) and the sprint's words show while it's on, in Focus mode too; click it to stop early.
- When the time's up, the sloth says how it went ("412 words in 25 minutes"), and the sprint is kept: Statistics shows this week's sprints, their words, and the best.

## 1.22.2

- Strings stay attached to a card while it's lifted (under the pointer, or being carried).

## 1.22.1

- Fixed: strings on the Canvas met a profile's pill (or a card's rounded corner) short of its edge, where the line left through a rounded end; they now meet the curve.

## 1.22.0

Canvas and corkboard polish.

- Cards on the Canvas sit on a soft shadow, lift a little under the pointer, and lift more while you carry one.
- Strings swing for a moment and settle when a card is set down or a string is tied.
- Picking a card eases the rest into the background, and back, rather than switching.
- Statuses show as a small stamp in a card's corner, on the corkboard and the Canvas, coloured from warm (just begun) through blue to green (done).
- All of it follows Reduce motion.

## 1.21.0

Focus, and softer motion.

- Paragraph focus: in Focus mode (⌥⌘F), every paragraph but the one you're writing fades back, and the light eases across as the cursor moves. Turn it off in Settings → Writing.
- The inspector and the split pane slide in and out, the main area fades in when it changes (another document, the corkboard or outline, the Canvas), and scrolling to the cursor glides.
- All of it follows your Mac's Reduce motion setting: with it on, everything changes at once.

## 1.20.0

The sloth comes out to keep you company.

- The start screen greets you for the time of day, with the sloth reading (or, late at night, asleep). Each recent project's tile shows its words, when you last wrote in it, your streak, and a bar towards its target.
- Empty places say so kindly, with the sloth: nothing open, an empty folder, the empty Trash, a blank Canvas, and a document with no snapshots yet.
- Reaching the day's goal: the sloth cheers.

## 1.19.0

Goals you can see.

- A ring in the status bar fills as you write towards the day's goal, beside today's words (click it for Statistics).
- Reaching the goal is celebrated, once a day: the ring pulses, and the sloth says so for a moment above it. Turn it off in Settings → Writing → Celebrate reaching the day's goal.
- Statistics shows the whole year as a calendar: a square a day, shaded by the words written against the goal; hover one for its words. Under it, the longest run of days meeting the goal (or writing, without one).
- Animations follow your Mac's Reduce motion setting (System Settings → Accessibility → Display).

## 1.18.0

- Replace: Replace… in the find bar (⌘F) replaces the selected match and moves to the next, or Replace All in the document. Regular expressions can use `$1` in the replacement, as in Sloth.
- Replace in the whole project: with the project's matches showing (⇧⌘F, or Whole Project in the find bar), Replace All replaces in every document outside the Trash. It asks first, saying how many it'll change, and takes a snapshot of each document before changing it, so any can be put back from its Snapshots.

## 1.17.0

- Twenty more colour themes, 26 in all (Settings → Appearance → Theme), grouped into light and dark. For writing: Paper (a soft white page) and Lamplight (warm and low-glare, for night). Also GitHub, One, Gruvbox, Catppuccin, Rosé Pine and Everforest in light and dark, Nord, Dracula, Tokyo Night and Monokai, and a High Contrast pair.
- The web editor wears the same theme: its colours come over with each sync, down to the phone's status bar. With Match System, or with Account → "Colours from Sloth Writer's theme" turned off on a device, it follows that device's light or dark mode as before.
- Saving keeps a file's Finder tags, other extended attributes and its creation date.

## 1.16.0

Links between documents.

- Write `[[A document's title]]` to link to it, the way references point at profiles: typing `[[` offers documents too (by the folder they're in), hovering shows its synopsis, and ⌘-click opens it beside. A pinpoint works as before: `[[The Letter|second page]]`.
- A document's inspector has Links: the documents it links to (click to open one beside), and the places that link to it, by section (click to go there).
- Renaming a document renames the links to it.
- Linked documents are strung together on the Canvas.
- A profile with the same name as a document wins; name the profile's kind to be sure (`[[Character: Mara]]`). Links are left out of word counts and the compiled book.

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
