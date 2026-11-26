#!/usr/bin/env python3
"""Make Sloth Writer's icon font: the Lucide icons Writer uses, cut out of
Lucide's own font, and a Rust file naming each one.

    python3 -m venv /tmp/icons && /tmp/icons/bin/pip install fonttools
    /tmp/icons/bin/python scripts/make-writer-icons.py path/to/lucide-static

`lucide-static` is the npm package (or its folder from unpkg.com): this reads
its font/lucide.ttf and font/lucide.css. Writes writer/icons/writer-icons.ttf
and writer/src/icons.rs. To add an icon, add it to ICONS and run this again.

Lucide (https://lucide.dev) is under the ISC licence; see
writer/icons/LUCIDE-LICENSE.
"""

import re
import sys
from pathlib import Path

from fontTools import subset
from fontTools.ttLib import TTFont

# (constant, Lucide name, what it's for). Grouped as icons.rs is.
ICONS = [
    # The interface.
    ("DOCUMENT", "file-text", "A document"),
    ("FOLDER", "folder", "A folder"),
    ("OPEN", "folder-open", "Open a project"),
    ("NEW", "plus", "Start something new"),
    ("IMPORT", "import", "Import"),
    ("FROM_HELPER", "cloud-download", "Get a project from the web helper"),
    ("TRASH", "trash-2", "The Trash"),
    ("NOTEPAD", "notebook-pen", "The mobile notepad"),
    ("PEOPLE", "users", "The Characters folder"),
    ("PERSON", "user", "A character"),
    ("PROFILES", "folders", "A folder of profiles"),
    ("LIBRARY", "library-big", "The Manuscript, Research and Notes folders"),
    ("CARD", "contact", "A profile"),
    ("PICTURE", "image", "A research file that's a picture"),
    ("ATTACHMENT", "paperclip", "A research file: a PDF or any other"),
    ("WEB", "globe", "On the web"),
    ("SETTINGS", "settings", "App settings"),
    ("ABOUT", "info", "About Sloth Writer"),
    # The toolbar.
    ("BINDER", "panel-left", "Show or hide the binder"),
    ("INSPECTOR", "panel-right", "Show or hide the inspector"),
    ("SPLIT", "columns-2", "A second pane beside the editor"),
    ("CANVAS", "waypoints", "The Canvas"),
    ("FOCUS", "focus", "Focus mode"),
    ("COMPILE", "book-up", "Compile"),
    ("SYNC", "refresh-cw", "Sync with the web helper"),
    ("MORE", "ellipsis", "More, in a menu"),
    # Kinds of project.
    ("WRITING", "pencil", "Writing"),
    ("NOVEL", "book-open", "Novel"),
    ("SHORT_FICTION", "scroll-text", "Short fiction"),
    ("ESSAY", "pen-tool", "Essay"),
    ("NONFICTION", "lightbulb", "Nonfiction"),
    ("PODCAST", "headphones", "Podcast script"),
    ("SCREENPLAY", "clapperboard", "Screenplay"),
    ("STAGE_PLAY", "drama", "Stage play"),
    ("POETRY", "moon", "Poetry"),
    ("JOURNAL", "notebook", "Journal"),
    ("PERSONAL", "heart", "Personal"),
    ("LETTERS", "mail-open", "Letters"),
    ("CHILDREN", "balloon", "Children's"),
    ("FANTASY", "castle", "Fantasy"),
    ("MYSTERY", "fingerprint", "Mystery"),
    ("SCIENCE_FICTION", "rocket", "Science fiction"),
    ("HORROR", "ghost", "Horror"),
    ("LORE", "map", "Lore: worldbuilding"),
    # Rewards.
    ("LEAF", "leaf", "Three days running; and the finale's leaves"),
    ("FLOWER", "flower-2", "A week"),
    ("TEA", "coffee", "Two weeks"),
    ("MEDAL", "medal", "Thirty days"),
    ("SEEDLING", "sprout", "A quarter of the manuscript"),
    ("SPRIG", "leafy-green", "Half"),
    ("TREE", "tree-deciduous", "Three quarters"),
    ("PARTY", "party-popper", "The target reached"),
]


def main() -> None:
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    lucide = Path(sys.argv[1])
    root = Path(__file__).resolve().parent.parent
    css = (lucide / "font" / "lucide.css").read_text()
    codes = {name: int(code, 16) for name, code in re.findall(r'\.icon-([a-z0-9-]+)::before\s*\{\s*content:\s*"\\([0-9a-f]+)"', css)}
    missing = [name for _, name, _ in ICONS if name not in codes]
    if missing:
        sys.exit(f"Not in this Lucide: {', '.join(missing)}")

    # The font: only these icons.
    font = TTFont(lucide / "font" / "lucide.ttf")
    # Its ligatures (typing an icon's name, for the web) are malformed, and
    # Writer doesn't need them.
    for table in ("GSUB", "GPOS"):
        if table in font:
            del font[table]
    options = subset.Options()
    options.name_IDs = ["*"]
    options.notdef_outline = True
    subsetter = subset.Subsetter(options)
    subsetter.populate(unicodes=sorted({codes[name] for _, name, _ in ICONS}))
    subsetter.subset(font)
    out = root / "writer" / "icons" / "writer-icons.ttf"
    font.save(out)

    # The names.
    lines = [
        "//! Writer's icons: characters in its own icon font",
        "//! (`icons/writer-icons.ttf`, made from Lucide by",
        "//! scripts/make-writer-icons.py, which also writes this file). They're",
        "//! text, so they go in labels and buttons like any other character and",
        "//! take the text's colour.",
        "",
    ]
    for constant, name, what in ICONS:
        lines.append(f"/// {what} (Lucide: {name}).")
        lines.append(f"pub const {constant}: &str = \"\\u{{{codes[name]:x}}}\";")
    lines += [
        "",
        "/// Every icon, by name, for the icon sheet and tests.",
        "#[cfg_attr(not(test), allow(dead_code))]",
        "pub const ALL: &[(&str, &str)] = &[",
        *[f"    (\"{constant}\", {constant})," for constant, _, _ in ICONS],
        "];",
        "",
    ]
    (root / "writer" / "src" / "icons.rs").write_text("\n".join(lines))
    print(f"{len(ICONS)} icons, {out.stat().st_size:,} bytes: {out.relative_to(root)}")


if __name__ == "__main__":
    main()
