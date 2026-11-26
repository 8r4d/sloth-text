//! Compile: turn the Manuscript into one finished file.
//!
//! First the manuscript is gathered (`gather`): the Manuscript folder's items
//! in binder order, minus anything marked not to be included, with inline
//! notes (`<!-- … -->`) taken out. Each top-level item is a chapter: a folder
//! with everything in it, or a document (with anything under it). Then it's
//! written as Markdown (`to_markdown`, ready for pandoc or anything else) or
//! as a Word file in standard manuscript format (`to_docx`), or as an ePub
//! e-book for reading in a web browser or e-reader (`to_epub`).
//!
//! Text is read the way people write in the editor: each line is a
//! paragraph (blank lines are just spacing), `*italic*` and `**bold**` are
//! kept, and a line of `#`, `***` or `---` is a scene break.
//!
//! `[[References]]` are margin notes, left out like the rest. Except, if
//! asked: a reference to a profile with a Citation section (a Source) can
//! become a note (a footnote in Word, a note at the chapter's end in an
//! e-book, a pandoc footnote in Markdown), and the sources cited can be
//! listed at the end as Works Cited.

use docx_rs::{
    AlignmentType, BreakType, Docx, Footnote, Header, LineSpacing, LineSpacingType, PageMargin, PageNum, Paragraph,
    Run, RunFonts, SpecialIndentType, Style, StyleType, Tab, TabValueType, VertAlignType,
};
use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};

use crate::project::{Node, Project, Role};

/// What to make.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Format {
    /// Standard manuscript format, for submitting to agents and editors.
    #[default]
    Manuscript,
    /// The stitched text, as Markdown.
    Markdown,
    /// An e-book, for reading in a web browser or e-reader.
    Epub,
    /// Pages ready to read or print, set like a book.
    Pdf,
}

impl Format {
    pub fn extension(self) -> &'static str {
        match self {
            Format::Manuscript => "docx",
            Format::Markdown => "md",
            Format::Epub => "epub",
            Format::Pdf => "pdf",
        }
    }
}

/// The paper, for a Word manuscript or a PDF.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Paper {
    #[default]
    Letter,
    A4,
    /// A trade paperback's page, 6 × 9 inches.
    Trade,
}

impl Paper {
    /// Width and height in twentieths of a point.
    fn size(self) -> (u32, u32) {
        match self {
            Paper::Letter => (12_240, 15_840),
            Paper::A4 => (11_906, 16_838),
            Paper::Trade => (8_640, 12_960),
        }
    }

    /// Width and height in points.
    pub fn points(self) -> (f32, f32) {
        let (w, h) = self.size();
        (w as f32 / 20.0, h as f32 / 20.0)
    }
}

/// The Word manuscript's font: the two standard manuscript faces.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Typeface {
    #[default]
    Times,
    Courier,
}

impl Typeface {
    pub fn name(self) -> &'static str {
        match self {
            Typeface::Times => "Times New Roman",
            Typeface::Courier => "Courier New",
        }
    }
}

/// What marks a break between scenes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SceneBreak {
    #[default]
    Hash,
    Stars,
    Asterism,
}

impl SceneBreak {
    pub fn text(self) -> &'static str {
        match self {
            SceneBreak::Hash => "#",
            SceneBreak::Stars => "* * *",
            SceneBreak::Asterism => "\u{2042}",
        }
    }
}

/// How chapters are headed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChapterHeadings {
    /// The folder or document title: "The Harbour".
    #[default]
    Titles,
    /// "Chapter 1", "Chapter 2"…
    Numbers,
    /// "Chapter 1: The Harbour".
    NumbersAndTitles,
}

/// What becomes of references to sources.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Citations {
    /// Left out, like every other reference.
    #[default]
    LeaveOut,
    /// A numbered note each: footnotes in Word, notes at the end of each
    /// chapter in an e-book, pandoc footnotes in Markdown.
    Notes,
}

/// Who you are, for every project: kept in App Settings, used when
/// compiling unless a project gives its own author or contact details.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AuthorProfile {
    /// Your name, for the contact details on a manuscript's title page.
    pub name: String,
    /// The name you write under, if it isn't your own: "by …".
    pub byline: String,
    /// Address, email, phone, agent…: one per line.
    pub contact: String,
    pub website: String,
    /// A few lines about you, for About the Author at the end of a book.
    pub bio: String,
}

impl AuthorProfile {
    /// The name on the cover: the byline, else your name.
    pub fn shown_name(&self) -> &str {
        if self.byline.trim().is_empty() { self.name.trim() } else { self.byline.trim() }
    }
}

/// Compile choices, kept in the project so they're there next time.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CompileSettings {
    /// The author's name as it should appear: "Mara Ellison". Empty to use
    /// the profile's (in App Settings).
    pub author: String,
    /// Address, email, phone, agent…: one per line, on the title page.
    /// Empty to use the profile's.
    pub contact: String,
    pub format: Format,
    pub chapter_headings: ChapterHeadings,
    /// Each document inside a chapter headed with its title (for essays
    /// and posts, rather than scenes).
    pub document_titles: bool,
    pub paper: Paper,
    pub typeface: Typeface,
    pub scene_break: SceneBreak,
    /// What becomes of references to sources.
    pub citations: Citations,
    /// List the sources cited, at the end.
    pub works_cited: bool,
    /// A page with the title and byline (a Word manuscript always has one).
    pub title_page: bool,
    /// A copyright page: © year, the author, All rights reserved.
    pub copyright_page: bool,
    /// A contents page (not for a Word manuscript).
    pub contents: bool,
    /// A folder whose documents come before the manuscript, a page each
    /// (a dedication, an epigraph, a foreword…): its binder id.
    pub front_matter: Option<String>,
    /// A folder whose documents come after it (acknowledgements…).
    pub back_matter: Option<String>,
    /// About the Author, from the profile's bio, at the end.
    pub about_author: bool,
    /// The PDF's font; `None` for the default reading face.
    pub pdf_font: Option<String>,
    /// Saved setups ("Agent submission", "Beta readers"…).
    pub presets: Vec<Preset>,
}

impl Default for CompileSettings {
    fn default() -> Self {
        CompileSettings {
            author: String::new(),
            contact: String::new(),
            format: Format::default(),
            chapter_headings: ChapterHeadings::default(),
            document_titles: false,
            paper: Paper::default(),
            typeface: Typeface::default(),
            scene_break: SceneBreak::default(),
            citations: Citations::default(),
            works_cited: false,
            title_page: true,
            copyright_page: false,
            contents: false,
            front_matter: None,
            back_matter: None,
            about_author: false,
            pdf_font: None,
            presets: Vec::new(),
        }
    }
}

/// A saved compile setup: everything but who you are.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preset {
    pub name: String,
    pub format: Format,
    pub chapter_headings: ChapterHeadings,
    pub document_titles: bool,
    pub paper: Paper,
    pub typeface: Typeface,
    pub scene_break: SceneBreak,
    pub citations: Citations,
    pub works_cited: bool,
    pub title_page: bool,
    pub copyright_page: bool,
    pub contents: bool,
    pub front_matter: Option<String>,
    pub back_matter: Option<String>,
    pub about_author: bool,
    pub pdf_font: Option<String>,
}

impl Default for Preset {
    fn default() -> Self {
        Preset::from_settings("", &CompileSettings::default())
    }
}

impl Preset {
    pub fn from_settings(name: &str, s: &CompileSettings) -> Self {
        Preset {
            name: name.to_owned(),
            format: s.format,
            chapter_headings: s.chapter_headings,
            document_titles: s.document_titles,
            paper: s.paper,
            typeface: s.typeface,
            scene_break: s.scene_break,
            citations: s.citations,
            works_cited: s.works_cited,
            title_page: s.title_page,
            copyright_page: s.copyright_page,
            contents: s.contents,
            front_matter: s.front_matter.clone(),
            back_matter: s.back_matter.clone(),
            about_author: s.about_author,
            pdf_font: s.pdf_font.clone(),
        }
    }

    /// Put this setup's choices into the settings.
    pub fn apply(&self, s: &mut CompileSettings) {
        s.format = self.format;
        s.chapter_headings = self.chapter_headings;
        s.document_titles = self.document_titles;
        s.paper = self.paper;
        s.typeface = self.typeface;
        s.scene_break = self.scene_break;
        s.citations = self.citations;
        s.works_cited = self.works_cited;
        s.title_page = self.title_page;
        s.copyright_page = self.copyright_page;
        s.contents = self.contents;
        s.front_matter = self.front_matter.clone();
        s.back_matter = self.back_matter.clone();
        s.about_author = self.about_author;
        s.pdf_font = self.pdf_font.clone();
    }

    /// Whether the settings are as this setup has them.
    pub fn matches(&self, s: &CompileSettings) -> bool {
        *self == Preset::from_settings(&self.name, s)
    }
}

/// The gathered manuscript.
#[derive(Debug, Default, PartialEq)]
pub struct Manuscript {
    pub title: String,
    /// The name it's by, as it should appear.
    pub author: String,
    /// Your own name, for the contact details (the author's, if not given).
    pub name: String,
    /// Contact details, one per line.
    pub contact: String,
    pub website: String,
    /// About the Author (empty if not asked for).
    pub bio: String,
    /// The copyright page's lines (empty if not asked for).
    pub copyright: Vec<String>,
    /// Whether to have a title page and a contents page.
    pub title_page: bool,
    pub contents: bool,
    /// Pages before and after the chapters.
    pub front: Vec<Matter>,
    pub back: Vec<Matter>,
    pub chapters: Vec<Chapter>,
    /// The notes, in order: note 1 first. Where each goes in the text is
    /// marked by `note_mark`.
    pub notes: Vec<String>,
    /// The sources cited, for Works Cited (empty if not asked for).
    pub works_cited: Vec<String>,
}

/// Where note `n` (from 1) goes in a scene's text: characters no one types
/// (Unicode's private use area) around its number.
const NOTE_OPEN: char = '\u{e000}';
const NOTE_CLOSE: char = '\u{e001}';

pub(crate) fn note_mark(n: usize) -> String {
    format!("{NOTE_OPEN}{n}{NOTE_CLOSE}")
}

/// A piece of text: words, or where a note goes.
#[derive(Debug, PartialEq)]
pub(crate) enum Piece<'a> {
    Text(&'a str),
    Note(usize),
}

/// Text split at its note marks.
pub(crate) fn pieces(text: &str) -> Vec<Piece<'_>> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find(NOTE_OPEN) {
        let after = &rest[open + NOTE_OPEN.len_utf8()..];
        let Some(close) = after.find(NOTE_CLOSE) else { break };
        let Ok(n) = after[..close].parse() else { break };
        if open > 0 {
            out.push(Piece::Text(&rest[..open]));
        }
        out.push(Piece::Note(n));
        rest = &after[close + NOTE_CLOSE.len_utf8()..];
    }
    if !rest.is_empty() {
        out.push(Piece::Text(rest));
    }
    out
}

/// Text with its note marks taken out (for counting words).
fn without_marks(text: &str) -> String {
    pieces(text).into_iter().filter_map(|p| if let Piece::Text(t) = p { Some(t) } else { None }).collect()
}

/// A citation as a reference makes it: the source's Citation section
/// (its name, if that's empty), for the note and for Works Cited.
struct Cited {
    note: String,
    entry: String,
}

/// What a reference cites, if it's to a profile whose kind has a Citation
/// section (a Source). Any pinpoint is added to the note: "…, p. 47."
fn citation(project: &Project, inner: &str, text: &impl Fn(&Node) -> String) -> Option<Cited> {
    let target = crate::references::resolve(project, inner)?;
    let kind = project.profile_kind(&target.kind)?;
    let heading = kind.sections.iter().find(|s| s.eq_ignore_ascii_case("citation"))?;
    let node = project.node(&target.id)?;
    let sheet = crate::character::Sheet::parse(&text(node));
    let written = sheet.sections.iter().find(|s| s.heading.eq_ignore_ascii_case(heading)).map(|s| s.text.split_whitespace().collect::<Vec<_>>().join(" "));
    let entry = written.filter(|c| !c.is_empty()).unwrap_or_else(|| node.title.clone());
    let note = match crate::references::split(inner).1 {
        Some(pin) => format!("{}, {pin}.", entry.trim_end_matches('.')),
        None => entry.clone(),
    };
    Some(Cited { note, entry })
}

/// A scene's references to sources as note marks (joined to the word
/// before them, as note numbers are), the note texts added to `notes`.
/// Other references are left for `strip_references`.
fn mark_citations(scene: &str, notes: &mut Vec<String>, cite: &mut impl FnMut(&str) -> Option<Cited>) -> String {
    let mut out = String::with_capacity(scene.len());
    let mut from = 0;
    for (range, inner) in sloth_core::markdown::references(scene) {
        let Some(cited) = cite(inner) else { continue };
        out.push_str(scene[from..range.start].trim_end_matches([' ', '\t']));
        notes.push(cited.note);
        out.push_str(&note_mark(notes.len()));
        from = range.end;
    }
    out.push_str(&scene[from..]);
    out
}

/// A page of front or back matter: a document as written. If it starts
/// with a `# heading`, that's its heading (and its place in the contents);
/// if not, it's set as a dedication or epigraph is, on its own.
#[derive(Debug, PartialEq)]
pub struct Matter {
    pub heading: Option<String>,
    pub text: String,
}

/// A document's text as a page of matter: its opening heading taken off.
fn matter(text: &str) -> Matter {
    let first = text.lines().map(str::trim).find(|l| !l.is_empty());
    let heading = first
        .filter(|l| l.starts_with('#') && !is_scene_break(l))
        .map(|l| l.trim_start_matches('#').trim().to_owned())
        .filter(|h| !h.is_empty());
    let body = match &heading {
        Some(_) => text.trim_start().split_once('\n').map_or("", |(_, rest)| rest).trim().to_owned(),
        None => text.trim().to_owned(),
    };
    Matter { heading, text: body }
}

#[derive(Debug, PartialEq)]
pub struct Chapter {
    pub heading: String,
    /// Each scene's text, notes removed.
    pub scenes: Vec<String>,
}

impl Manuscript {
    /// The chapters' words (front and back matter aren't counted).
    pub fn words(&self) -> usize {
        self.chapters.iter().flat_map(|c| &c.scenes).map(|s| without_marks(s).split_whitespace().count()).sum()
    }
}

pub const ABOUT_HEADING: &str = "About the Author";

/// Where an item of the contents is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Entry {
    Front(usize),
    Chapter(usize),
    Back(usize),
    WorksCited,
    About,
}

/// Gather the Manuscript folder. `text` gives each document's text: from
/// the editor if it's open, else from its file.
pub fn gather(project: &Project, settings: &CompileSettings, profile: &AuthorProfile, text: impl Fn(&Node) -> String) -> Manuscript {
    let manuscript = project
        .manifest
        .binder
        .iter()
        .find(|n| n.role == Some(Role::Manuscript))
        .expect("every project has a Manuscript");
    let mut chapters = Vec::new();
    let mut notes = Vec::new();
    let mut cited: Vec<String> = Vec::new();
    // Each scene: references to sources as notes (if asked), everything
    // else in the margin taken out.
    let mut prepare = |raw: &str| {
        let mut cite = |inner: &str| {
            let c = citation(project, inner, &text)?;
            if settings.works_cited && !cited.contains(&c.entry) {
                cited.push(c.entry.clone());
            }
            (settings.citations == Citations::Notes).then_some(c)
        };
        // Comments go first (a commented passage stays).
        let raw = &sloth_core::markdown::strip_comments(raw);
        let marked = mark_citations(raw, &mut notes, &mut cite);
        strip_notes(&sloth_core::markdown::strip_references(&marked))
    };
    for item in manuscript.children.iter().filter(|n| !n.meta.exclude_from_compile) {
        let mut scenes = Vec::new();
        collect_scenes(item, &text, &mut prepare, settings.document_titles.then_some(0), &mut scenes);
        if scenes.is_empty() && !item.is_document() && item.children.is_empty() {
            continue; // an empty folder
        }
        let number = chapters.len() + 1;
        let heading = match settings.chapter_headings {
            ChapterHeadings::Titles => item.title.clone(),
            ChapterHeadings::Numbers => format!("Chapter {number}"),
            ChapterHeadings::NumbersAndTitles => format!("Chapter {number}: {}", item.title),
        };
        chapters.push(Chapter { heading, scenes });
    }
    // Front and back matter: each document in the chosen folder, as written
    // (inline notes and references taken out, but no citations made).
    let matter_in = |id: &Option<String>| -> Vec<Matter> {
        let Some(folder) = id.as_deref().and_then(|id| project.node(id)) else { return Vec::new() };
        let mut texts = Vec::new();
        let mut plain = |raw: &str| strip_notes(&sloth_core::markdown::strip_references(&sloth_core::markdown::strip_comments(raw)));
        for child in &folder.children {
            collect_scenes(child, &text, &mut plain, None, &mut texts);
        }
        texts.iter().map(|t| matter(t)).collect()
    };
    let (front, back) = (matter_in(&settings.front_matter), matter_in(&settings.back_matter));
    cited.sort_by_key(|c| c.to_lowercase());
    let author = match settings.author.trim() {
        "" => profile.shown_name().to_owned(),
        own => own.to_owned(),
    };
    let name = match profile.name.trim() {
        "" => author.clone(),
        own => own.to_owned(),
    };
    let contact = match settings.contact.trim() {
        "" => profile.contact.trim().to_owned(),
        own => own.to_owned(),
    };
    let website = profile.website.trim().to_owned();
    let copyright = if settings.copyright_page {
        let year = chrono::Local::now().format("%Y");
        let mut lines = vec![if author.is_empty() { format!("Copyright \u{a9} {year}") } else { format!("Copyright \u{a9} {year} {author}") }];
        lines.push("All rights reserved.".to_owned());
        if !website.is_empty() {
            lines.push(website.clone());
        }
        lines
    } else {
        Vec::new()
    };
    Manuscript {
        title: project.title().to_owned(),
        author,
        name,
        contact,
        website,
        bio: if settings.about_author { profile.bio.trim().to_owned() } else { String::new() },
        copyright,
        title_page: settings.title_page,
        contents: settings.contents,
        front,
        back,
        chapters,
        notes,
        works_cited: cited,
    }
}

/// The scenes in an item, in order, skipping anything not included.
/// `titled`: how deep this item is in its chapter, when documents are
/// headed with their titles (the chapter's own title is its heading).
fn collect_scenes(
    node: &Node,
    text: &impl Fn(&Node) -> String,
    prepare: &mut impl FnMut(&str) -> String,
    titled: Option<usize>,
    out: &mut Vec<String>,
) {
    if node.meta.exclude_from_compile {
        return;
    }
    if node.is_document() {
        let scene = prepare(&text(node));
        let scene = scene.trim();
        if !scene.is_empty() {
            out.push(match titled {
                Some(depth) if depth > 0 => format!("## {}\n\n{scene}", node.title.trim()),
                _ => scene.to_owned(),
            });
        }
    }
    for child in &node.children {
        collect_scenes(child, text, prepare, titled.map(|d| d + 1), out);
    }
}

/// A scene that opens with a heading needs no break mark before it.
pub(crate) fn break_before(i: usize, scene: &str) -> bool {
    i > 0 && !scene.lines().map(str::trim).find(|l| !l.is_empty()).is_some_and(|l| l.starts_with('#') && !is_scene_break(l))
}

/// Remove inline notes: everything from `<!--` to `-->` (an unclosed one
/// runs to the end).
pub fn strip_notes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("<!--") {
        out.push_str(&rest[..start]);
        rest = match rest[start + 4..].find("-->") {
            Some(end) => &rest[start + 4 + end + 3..],
            None => "",
        };
    }
    out.push_str(rest);
    out
}

// ----- reading the text --------------------------------------------------------

/// A block of a scene, as compile sees it.
#[derive(Debug, PartialEq)]
pub(crate) enum Block {
    Paragraph(Vec<Span>),
    /// A heading inside a scene.
    Heading(Vec<Span>),
    /// `> quoted` lines.
    Quote(Vec<Span>),
    SceneBreak,
}

/// A run of text with its emphasis.
#[derive(Debug, PartialEq)]
pub(crate) struct Span {
    pub text: String,
    pub italic: bool,
    pub bold: bool,
}

pub(crate) fn blocks(scene: &str) -> Vec<Block> {
    scene
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| {
            if is_scene_break(line) {
                Block::SceneBreak
            } else if let Some(heading) = line.strip_prefix('#').map(|h| h.trim_start_matches('#').trim()) {
                Block::Heading(spans(heading))
            } else if let Some(quote) = line.strip_prefix('>') {
                Block::Quote(spans(quote.trim()))
            } else {
                Block::Paragraph(spans(line))
            }
        })
        .collect()
}

pub(crate) fn is_scene_break(line: &str) -> bool {
    let marks: String = line.chars().filter(|c| !c.is_whitespace()).collect();
    marks == "#" || (marks.len() >= 3 && (marks.chars().all(|c| c == '*') || marks.chars().all(|c| c == '-')))
}

/// One line's text and emphasis. Markdown's block syntax (lists and so on)
/// is switched off by escaping, so a line like "1. She ran" stays as typed.
pub(crate) fn spans(line: &str) -> Vec<Span> {
    let escaped = escape_block_markers(line);
    let (mut italic, mut bold) = (0, 0);
    let mut out: Vec<Span> = Vec::new();
    for event in Parser::new(&escaped) {
        match event {
            Event::Start(Tag::Emphasis) => italic += 1,
            Event::End(TagEnd::Emphasis) => italic -= 1,
            Event::Start(Tag::Strong) => bold += 1,
            Event::End(TagEnd::Strong) => bold -= 1,
            Event::Text(t) | Event::Code(t) => {
                let (i, b) = (italic > 0, bold > 0);
                match out.last_mut() {
                    Some(last) if last.italic == i && last.bold == b => last.text.push_str(&t),
                    _ => out.push(Span { text: t.into_string(), italic: i, bold: b }),
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if let Some(last) = out.last_mut() {
                    last.text.push(' ');
                }
            }
            _ => {}
        }
    }
    out
}

/// Escape what would make a line a list or code block rather than text.
fn escape_block_markers(line: &str) -> String {
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    let after = &line[digits..];
    if digits > 0 && (after.starts_with(". ") || after.starts_with(") ")) {
        return format!("{}\\{}", &line[..digits], after);
    }
    if ["- ", "+ ", "* "].iter().any(|m| line.starts_with(m)) {
        return format!("\\{line}");
    }
    line.to_owned()
}

// ----- Markdown ----------------------------------------------------------------

/// The manuscript as Markdown, with a title block pandoc understands, a
/// level-one heading per chapter and `* * *` between scenes. Each paragraph
/// is separated by a blank line, so any Markdown reader sees them as such.
pub fn to_markdown(m: &Manuscript, settings: &CompileSettings) -> String {
    let separator = match settings.scene_break {
        // A lone # would read as an empty heading in Markdown.
        SceneBreak::Hash => "* * *",
        other => other.text(),
    };
    let mut out = String::new();
    out.push_str(&format!("---\ntitle: \"{}\"\n", m.title.replace('"', "\\\"")));
    if !m.author.is_empty() {
        out.push_str(&format!("author: \"{}\"\n", m.author.replace('"', "\\\"")));
    }
    if let Some(rights) = m.copyright.first() {
        out.push_str(&format!("rights: \"{}\"\n", rights.replace('"', "\\\"")));
    }
    if m.contents {
        out.push_str("toc: true\n");
    }
    out.push_str("---\n");
    let matter_md = |out: &mut String, matter: &Matter| {
        if let Some(h) = &matter.heading {
            out.push_str(&format!("\n# {h}\n"));
        }
        for line in matter.text.lines().map(str::trim).filter(|l| !l.is_empty()) {
            out.push_str(&format!("\n{}\n", if is_scene_break(line) { separator } else { line }));
        }
    };
    for matter in &m.front {
        matter_md(&mut out, matter);
    }
    for chapter in &m.chapters {
        out.push_str(&format!("\n# {}\n", chapter.heading));
        for (i, scene) in chapter.scenes.iter().enumerate() {
            if break_before(i, scene) {
                out.push_str(&format!("\n{separator}\n"));
            }
            for line in scene.lines().map(str::trim).filter(|l| !l.is_empty()) {
                out.push('\n');
                // A break typed in a scene: the same as between scenes.
                if is_scene_break(line) {
                    out.push_str(separator);
                } else {
                    for piece in pieces(line) {
                        match piece {
                            Piece::Text(t) => out.push_str(t),
                            Piece::Note(n) => out.push_str(&format!("[^{n}]")),
                        }
                    }
                }
                out.push('\n');
            }
        }
    }
    // Pandoc footnotes: put where they're numbered, wherever these go.
    if !m.notes.is_empty() {
        out.push('\n');
        for (i, note) in m.notes.iter().enumerate() {
            out.push_str(&format!("[^{}]: {note}\n", i + 1));
        }
    }
    for matter in &m.back {
        matter_md(&mut out, matter);
    }
    if !m.works_cited.is_empty() {
        out.push_str("\n# Works Cited\n");
        for entry in &m.works_cited {
            out.push_str(&format!("\n{entry}\n"));
        }
    }
    if !m.bio.is_empty() {
        matter_md(&mut out, &Matter { heading: Some(ABOUT_HEADING.to_owned()), text: m.bio.clone() });
    }
    out
}

// ----- Word manuscript -----------------------------------------------------------

/// Twentieths of a point, Word's unit for page geometry.
const INCH: i32 = 1440;

/// "about 85,000 words": rounded as manuscripts give it (to the nearest
/// thousand for a novel, else the nearest hundred).
pub fn rounded_words(words: usize) -> String {
    let step = if words >= 20_000 { 1000 } else { 100 };
    let rounded = ((words + step / 2) / step * step).max(step);
    let digits = rounded.to_string();
    let mut grouped = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(c);
    }
    format!("about {grouped} words")
}

/// Standard manuscript format: Times New Roman (or Courier) 12 pt, double
/// spaced, 1 inch margins on Letter or A4, a title page (name and contact
/// details top left, word count top right), each chapter on a new page a
/// third of the way down, a break mark between scenes, and
/// "Surname / Title / page" at the top right of every page after the first.
pub fn to_docx(m: &Manuscript, settings: &CompileSettings) -> Result<Vec<u8>, String> {
    let (page_w, page_h) = settings.paper.size();
    let face = settings.typeface.name();
    let mark = settings.scene_break.text();
    let surname = m.author.split_whitespace().last().unwrap_or("").to_owned();
    let header_text = if surname.is_empty() { format!("{} / ", m.title) } else { format!("{surname} / {} / ", m.title) };
    let header = Header::new().add_paragraph(
        Paragraph::new().align(AlignmentType::Right).add_run(run(face).add_text(header_text)).add_page_num(PageNum::new()),
    );

    let mut doc = Docx::new()
        .page_size(page_w, page_h)
        .page_margin(PageMargin::new().top(INCH).bottom(INCH).left(INCH).right(INCH).header(720))
        .default_fonts(RunFonts::new().ascii(face).hi_ansi(face).cs(face))
        .default_size(24) // half-points: 12 pt
        .default_line_spacing(LineSpacing::new().line_rule(LineSpacingType::Auto).line(480).after(0))
        .header(header)
        // No header on the title page.
        .first_header(Header::new())
        .title_pg();
    // Note numbers in the text, raised.
    let mut note_style = Style::new("FootnoteReference", StyleType::Character).name("footnote reference");
    note_style.run_property = note_style.run_property.vert_align(VertAlignType::SuperScript);
    doc = doc.add_style(note_style);

    // Title page: name top left, word count top right, title and byline
    // centred halfway down.
    let single = || LineSpacing::new().line_rule(LineSpacingType::Auto).line(240).after(0);
    let text_width = page_w as usize - 2 * INCH as usize;
    doc = doc.add_paragraph(
        Paragraph::new()
            .line_spacing(single())
            .add_tab(Tab::new().val(TabValueType::Right).pos(text_width))
            .add_run(run(face).add_text(&m.name).add_tab().add_text(rounded_words(m.words()))),
    );
    // Contact details under the name, single spaced.
    for line in m.contact.lines().chain(std::iter::once(m.website.as_str())).map(str::trim).filter(|l| !l.is_empty()) {
        doc = doc.add_paragraph(Paragraph::new().line_spacing(single()).add_run(run(face).add_text(line)));
    }
    doc = doc.add_paragraph(
        Paragraph::new()
            .align(AlignmentType::Center)
            .line_spacing(LineSpacing::new().before(4 * INCH as u32).line(480))
            .add_run(run(face).add_text(m.title.to_uppercase())),
    );
    if !m.author.is_empty() {
        doc = doc.add_paragraph(body().align(AlignmentType::Center).add_run(run(face).add_text(format!("by {}", m.author))));
    }

    // Spacing and page breaks are set on each paragraph rather than left to
    // document defaults: some readers (Quick Look, TextEdit) ignore those.
    let new_page = |heading: &str| {
        Paragraph::new()
            .align(AlignmentType::Center)
            .line_spacing(LineSpacing::new().before(2 * INCH as u32).after(480).line(480))
            .add_run(page_break())
            .add_run(run(face).add_text(heading))
    };
    let add_blocks = |mut doc: Docx, scene: &str| {
        for block in blocks(scene) {
            doc = doc.add_paragraph(match block {
                Block::Paragraph(spans) => runs(body().indent(None, Some(SpecialIndentType::FirstLine(INCH / 2)), None, None), spans, face, &m.notes),
                Block::Quote(spans) => runs(body().indent(Some(INCH / 2), None, Some(INCH / 2), None), spans, face, &m.notes),
                Block::Heading(spans) => runs(body().align(AlignmentType::Center), spans, face, &m.notes),
                Block::SceneBreak => scene_break(mark, face),
            });
        }
        doc
    };
    // A page of front or back matter: headed like a chapter, or, without a
    // heading, centred a third of the way down, as a dedication is.
    let add_matter = |mut doc: Docx, matter: &Matter| {
        match &matter.heading {
            Some(heading) => {
                doc = doc.add_paragraph(new_page(heading));
                add_blocks(doc, &matter.text)
            }
            None => {
                let mut first = true;
                for block in blocks(&matter.text) {
                    let spans = match block {
                        Block::Paragraph(s) | Block::Quote(s) | Block::Heading(s) => s,
                        Block::SceneBreak => vec![Span { text: mark.to_owned(), italic: false, bold: false }],
                    };
                    let mut p = body().align(AlignmentType::Center);
                    if first {
                        p = p.line_spacing(LineSpacing::new().line_rule(LineSpacingType::Auto).before(3 * INCH as u32).line(480).after(0)).add_run(page_break());
                        first = false;
                    }
                    doc = doc.add_paragraph(runs(p, spans, face, &m.notes));
                }
                doc
            }
        }
    };
    if !m.copyright.is_empty() {
        let mut first = true;
        for line in &m.copyright {
            let mut p = Paragraph::new().line_spacing(single());
            if first {
                p = p.line_spacing(LineSpacing::new().line_rule(LineSpacingType::Auto).before(6 * INCH as u32).line(240).after(0)).add_run(page_break());
                first = false;
            }
            doc = doc.add_paragraph(p.add_run(run(face).add_text(line)));
        }
    }
    for matter in &m.front {
        doc = add_matter(doc, matter);
    }
    for chapter in &m.chapters {
        doc = doc.add_paragraph(new_page(&chapter.heading));
        for (i, scene) in chapter.scenes.iter().enumerate() {
            if break_before(i, scene) {
                doc = doc.add_paragraph(scene_break(mark, face));
            }
            doc = add_blocks(doc, scene);
        }
    }
    for matter in &m.back {
        doc = add_matter(doc, matter);
    }

    // Works Cited: on a page of its own, each entry with a hanging indent.
    if !m.works_cited.is_empty() {
        doc = doc.add_paragraph(
            Paragraph::new()
                .align(AlignmentType::Center)
                .line_spacing(LineSpacing::new().after(480).line(480))
                .add_run(page_break())
                .add_run(run(face).add_text("Works Cited")),
        );
        for entry in &m.works_cited {
            let hanging = body().indent(Some(INCH / 2), Some(SpecialIndentType::Hanging(INCH / 2)), None, None);
            doc = doc.add_paragraph(runs(hanging, spans(entry), face, &[]));
        }
    }
    if !m.bio.is_empty() {
        doc = doc.add_paragraph(new_page(ABOUT_HEADING));
        doc = add_blocks(doc, &m.bio);
    }

    let mut bytes = std::io::Cursor::new(Vec::new());
    doc.build().pack(&mut bytes).map_err(|e| e.to_string())?;
    Ok(bytes.into_inner())
}

/// Text in the manuscript's font. Set on every run, not only as the
/// document's default: simpler readers (Quick Look, TextEdit) ignore that.
fn run(face: &str) -> Run {
    Run::new().fonts(RunFonts::new().ascii(face).hi_ansi(face).cs(face))
}

/// A double-spaced paragraph.
fn body() -> Paragraph {
    Paragraph::new().line_spacing(LineSpacing::new().line_rule(LineSpacingType::Auto).line(480).after(0))
}

fn scene_break(mark: &str, face: &str) -> Paragraph {
    body().align(AlignmentType::Center).add_run(run(face).add_text(mark))
}

fn runs(mut paragraph: Paragraph, spans: Vec<Span>, face: &str, notes: &[String]) -> Paragraph {
    for span in spans {
        for piece in pieces(&span.text) {
            match piece {
                Piece::Text(t) => {
                    let mut run = run(face).add_text(t);
                    if span.italic {
                        run = run.italic();
                    }
                    if span.bold {
                        run = run.bold();
                    }
                    paragraph = paragraph.add_run(run);
                }
                Piece::Note(n) => {
                    let text = notes.get(n - 1).cloned().unwrap_or_default();
                    paragraph = paragraph.add_run(Run::new().add_footnote_reference(footnote(n, &text, face)));
                }
            }
        }
    }
    paragraph
}

/// Note `n`'s text at the foot of the page: 10 pt, single spaced, with its
/// number raised in front (Word numbers the notes in the text itself).
fn footnote(n: usize, text: &str, face: &str) -> Footnote {
    let mut number = run(face).size(20).add_text(n.to_string());
    number.run_property = number.run_property.vert_align(VertAlignType::SuperScript);
    let mut paragraph = Paragraph::new().line_spacing(LineSpacing::new().line_rule(LineSpacingType::Auto).line(240).after(0)).add_run(number).add_run(run(face).size(20).add_text(" "));
    for span in spans(text) {
        let mut piece = run(face).size(20).add_text(span.text);
        if span.italic {
            piece = piece.italic();
        }
        if span.bold {
            piece = piece.bold();
        }
        paragraph = paragraph.add_run(piece);
    }
    let mut note = Footnote::new();
    note.add_content(paragraph);
    note
}

/// Starts a new page (more widely understood than "page break before").
fn page_break() -> Run {
    Run::new().add_break(BreakType::Page)
}

// ----- ePub ------------------------------------------------------------------------

/// XML-escape text.
fn xml(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// A paragraph's text, with its emphasis and note numbers, as XHTML.
fn xhtml_spans(spans: &[Span]) -> String {
    spans
        .iter()
        .map(|s| {
            let mut t: String = pieces(&s.text)
                .into_iter()
                .map(|p| match p {
                    Piece::Text(t) => xml(t),
                    Piece::Note(n) => format!("<a epub:type=\"noteref\" href=\"#note-{n}\" id=\"ref-{n}\"><sup>{n}</sup></a>"),
                })
                .collect();
            if s.italic {
                t = format!("<em>{t}</em>");
            }
            if s.bold {
                t = format!("<strong>{t}</strong>");
            }
            t
        })
        .collect()
}

fn xhtml_page(title: &str, body: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<!DOCTYPE html>\n\
         <html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\" lang=\"en\" xml:lang=\"en\">\n\
         <head><meta charset=\"utf-8\"/><title>{}</title><link rel=\"stylesheet\" href=\"style.css\"/></head>\n\
         <body>\n{body}\n</body>\n</html>\n",
        xml(title)
    )
}

const EPUB_CSS: &str = "body { font-family: serif; line-height: 1.5; }\n\
h1 { text-align: center; margin: 3em 0 2em; font-weight: normal; }\n\
h2 { text-align: center; font-size: 1em; }\n\
p { margin: 0; text-indent: 1.5em; }\n\
h1 + p, .break + p, h2 + p, blockquote + p { text-indent: 0; }\n\
.break { text-align: center; text-indent: 0; margin: 1em 0; }\n\
blockquote { margin: 0.5em 2em; font-style: italic; }\n\
.title { text-align: center; margin-top: 30%; }\n\
.title h1 { margin: 0 0 0.5em; font-size: 2em; }\n\
.dedication { text-align: center; margin-top: 30%; }\n\
.dedication p, .copyright p, .contents p { text-indent: 0; }\n\
.copyright { margin-top: 60%; font-size: 0.85em; }\n\
.contents ol { list-style: none; padding: 0; }\n\
.contents li { margin: 0.4em 0; }\n\
.notes { margin-top: 2em; border-top: 1px solid #999; font-size: 0.85em; }\n\
.notes p, .cited p { text-indent: 0; margin: 0.3em 0; }\n\
.cited p { padding-left: 2em; text-indent: -2em; }\n\
sup { line-height: 0; }\n";

/// An EPUB 3 e-book: a title page, then a page per chapter, with a table of
/// contents. Readable in Apple Books, Kindle (via Send to Kindle) and others.
pub fn to_epub(m: &Manuscript, settings: &CompileSettings) -> Result<Vec<u8>, String> {
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    let mark = settings.scene_break.text();
    let blocks_html = |scene: &str| -> String {
        blocks(scene)
            .into_iter()
            .map(|block| match block {
                Block::Paragraph(spans) => format!("<p>{}</p>\n", xhtml_spans(&spans)),
                Block::Quote(spans) => format!("<blockquote><p>{}</p></blockquote>\n", xhtml_spans(&spans)),
                Block::Heading(spans) => format!("<h2>{}</h2>\n", xhtml_spans(&spans)),
                Block::SceneBreak => format!("<p class=\"break\">{}</p>\n", xml(mark)),
            })
            .collect()
    };
    // A page of front or back matter: headed, or set on its own as a
    // dedication is.
    let matter_page = |file: String, matter: &Matter| -> (String, String, String, Option<Entry>) {
        match &matter.heading {
            Some(h) => (file, h.clone(), xhtml_page(h, &format!("<h1>{}</h1>\n{}", xml(h), blocks_html(&matter.text))), None),
            None => (file, String::new(), xhtml_page(&m.title, &format!("<div class=\"dedication\">\n{}</div>", blocks_html(&matter.text))), None),
        }
    };
    // file, title (empty to leave it out of the contents), xhtml, and
    // which item of the contents it is.
    let mut pages: Vec<(String, String, String, Option<Entry>)> = Vec::new();
    if m.title_page {
        let byline = if m.author.is_empty() { String::new() } else { format!("<p>{}</p>", xml(&m.author)) };
        pages.push((
            "title.xhtml".into(),
            String::new(),
            xhtml_page(&m.title, &format!("<div class=\"title\"><h1>{}</h1>{byline}</div>", xml(&m.title))),
            None,
        ));
    }
    if !m.copyright.is_empty() {
        let lines: String = m.copyright.iter().map(|l| format!("<p>{}</p>", xml(l))).collect();
        pages.push(("copyright.xhtml".into(), String::new(), xhtml_page("Copyright", &format!("<div class=\"copyright\">{lines}</div>")), None));
    }
    for (i, matter) in m.front.iter().enumerate() {
        let mut page = matter_page(format!("front-{}.xhtml", i + 1), matter);
        page.3 = Some(Entry::Front(i));
        pages.push(page);
    }
    let contents_at = pages.len();
    for (n, chapter) in m.chapters.iter().enumerate() {
        let mut body = format!("<h1>{}</h1>\n", xml(&chapter.heading));
        for (i, scene) in chapter.scenes.iter().enumerate() {
            if break_before(i, scene) {
                body.push_str(&format!("<p class=\"break\">{}</p>\n", xml(mark)));
            }
            body.push_str(&blocks_html(scene));
        }
        // The chapter's notes, at its end, each linked back to its number.
        let numbers: Vec<usize> = chapter
            .scenes
            .iter()
            .flat_map(|s| pieces(s).into_iter().filter_map(|p| if let Piece::Note(n) = p { Some(n) } else { None }).collect::<Vec<_>>())
            .collect();
        if !numbers.is_empty() {
            body.push_str("<section class=\"notes\" epub:type=\"footnotes\">\n");
            for n in numbers {
                let text = m.notes.get(n - 1).map(|t| xhtml_spans(&spans(t))).unwrap_or_default();
                body.push_str(&format!("<aside epub:type=\"footnote\" id=\"note-{n}\"><p><a href=\"#ref-{n}\">{n}</a>. {text}</p></aside>\n"));
            }
            body.push_str("</section>\n");
        }
        pages.push((format!("chapter-{}.xhtml", n + 1), chapter.heading.clone(), xhtml_page(&chapter.heading, &body), Some(Entry::Chapter(n))));
    }
    for (i, matter) in m.back.iter().enumerate() {
        let mut page = matter_page(format!("back-{}.xhtml", i + 1), matter);
        page.3 = Some(Entry::Back(i));
        pages.push(page);
    }
    if !m.works_cited.is_empty() {
        let entries: String = m.works_cited.iter().map(|e| format!("<p>{}</p>\n", xhtml_spans(&spans(e)))).collect();
        let body = format!("<h1>Works Cited</h1>\n<div class=\"cited\">\n{entries}</div>");
        pages.push(("works-cited.xhtml".into(), "Works Cited".into(), xhtml_page("Works Cited", &body), Some(Entry::WorksCited)));
    }
    if !m.bio.is_empty() {
        let body = format!("<h1>{ABOUT_HEADING}</h1>\n{}", blocks_html(&m.bio));
        pages.push(("about.xhtml".into(), ABOUT_HEADING.into(), xhtml_page(ABOUT_HEADING, &body), Some(Entry::About)));
    }
    // A contents page in the book itself, after the front matter, linking
    // to each item (the reader's own contents is the nav, below).
    let links = |pages: &[(String, String, String, Option<Entry>)]| -> String {
        pages
            .iter()
            .filter(|(_, title, _, _)| !title.is_empty())
            .map(|(file, title, _, _)| format!("<li><a href=\"{file}\">{}</a></li>", xml(title)))
            .collect()
    };
    if m.contents {
        let list = links(&pages);
        let page = xhtml_page("Contents", &format!("<div class=\"contents\"><h1>Contents</h1>\n<ol>{list}</ol></div>"));
        pages.insert(contents_at, ("contents.xhtml".into(), String::new(), page, None));
    }

    let toc = links(&pages);
    let nav = xhtml_page(
        "Contents",
        &format!("<nav epub:type=\"toc\" id=\"toc\"><h1>Contents</h1><ol>{toc}</ol></nav>"),
    );
    let manifest: String = pages
        .iter()
        .enumerate()
        .map(|(i, (file, _, _, _))| format!("<item id=\"p{i}\" href=\"{file}\" media-type=\"application/xhtml+xml\"/>"))
        .collect();
    let spine: String = (0..pages.len()).map(|i| format!("<itemref idref=\"p{i}\"/>")).collect();
    let id = format!("urn:sloth-writer:{}", crate::project::random_id(16));
    let modified = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
    let creator = if m.author.is_empty() { String::new() } else { format!("<dc:creator>{}</dc:creator>", xml(&m.author)) };
    let opf = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\" unique-identifier=\"book-id\">\n\
         <metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\
         <dc:identifier id=\"book-id\">{id}</dc:identifier><dc:title>{}</dc:title>{creator}<dc:language>en</dc:language>\
         <meta property=\"dcterms:modified\">{modified}</meta></metadata>\n\
         <manifest><item id=\"nav\" href=\"nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/>\
         <item id=\"css\" href=\"style.css\" media-type=\"text/css\"/>{manifest}</manifest>\n\
         <spine>{spine}</spine>\n</package>\n",
        xml(&m.title)
    );
    let container = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
        <container version=\"1.0\" xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\">\
        <rootfiles><rootfile full-path=\"OEBPS/content.opf\" media-type=\"application/oebps-package+xml\"/></rootfiles></container>\n";

    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let err = |e: zip::result::ZipError| e.to_string();
    let io = |e: std::io::Error| e.to_string();
    // The mimetype comes first, uncompressed, as e-readers expect.
    zip.start_file("mimetype", SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored)).map_err(err)?;
    zip.write_all(b"application/epub+zip").map_err(io)?;
    let deflated = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut add = |name: &str, content: &str| -> Result<(), String> {
        zip.start_file(name, deflated).map_err(err)?;
        zip.write_all(content.as_bytes()).map_err(io)
    };
    add("META-INF/container.xml", container)?;
    add("OEBPS/content.opf", &opf)?;
    add("OEBPS/nav.xhtml", &nav)?;
    add("OEBPS/style.css", EPUB_CSS)?;
    for (file, _, page, _) in &pages {
        add(&format!("OEBPS/{file}"), page)?;
    }
    Ok(zip.finish().map_err(err)?.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{Meta, Place};
    use std::fs;
    use std::path::PathBuf;

    struct TempDir(PathBuf);
    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("sloth-writer-compile-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            Self(dir)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// Manuscript: a chapter folder with two scenes (one with a note), an
    /// excluded scene, and a chapter that's a single document.
    fn book(dir: &TempDir) -> Project {
        let mut p = Project::create(&dir.0, "The Long Winter").unwrap();
        let untitled = p.manifest.binder[0].children[0].id.clone();
        p.trash(&untitled).unwrap();
        let m = Place::Into("manuscript".into());
        let one = p.add_folder(&m, "Arrival").unwrap();
        for (title, text) in [
            ("Quay", "Mara was *late*.\nThe ferry had gone.<!-- check tides -->"),
            ("Cut", "This scene is cut."),
            ("Letter", "Dear Mara,\n\n**Come home.**"),
        ] {
            let id = p.add_document(&Place::Into(one.clone()), title).unwrap();
            fs::write(p.file_path(&id).unwrap(), text).unwrap();
            if title == "Cut" {
                p.set_meta(&id, Meta { exclude_from_compile: true, ..Meta::default() });
            }
        }
        let two = p.add_document(&m, "The Harbour").unwrap();
        fs::write(p.file_path(&two).unwrap(), "The harbour froze.").unwrap();
        p
    }

    fn read(p: &Project) -> impl Fn(&Node) -> String + '_ {
        |n: &Node| fs::read_to_string(p.file_path(&n.id).unwrap()).unwrap()
    }

    /// The book with front matter (a dedication and a foreword) and back
    /// matter (acknowledgements) in folders under Notes.
    fn book_with_matter(dir: &TempDir) -> (Project, CompileSettings, AuthorProfile) {
        let mut p = book(dir);
        let notes = Place::Into("notes".into());
        let front = p.add_folder(&notes, "Front").unwrap();
        let back = p.add_folder(&notes, "Back").unwrap();
        for (folder, title, text) in [
            (&front, "Dedication", "For M.<!-- who knows -->"),
            (&front, "Foreword", "# Foreword\n\nA word *first*."),
            (&front, "Draft", "Not this one."),
            (&back, "Thanks", "# Acknowledgements\n\nThanks to the ferry."),
        ] {
            let id = p.add_document(&Place::Into(folder.clone()), title).unwrap();
            fs::write(p.file_path(&id).unwrap(), text).unwrap();
            if title == "Draft" {
                p.set_meta(&id, Meta { exclude_from_compile: true, ..Meta::default() });
            }
        }
        let settings = CompileSettings {
            copyright_page: true,
            contents: true,
            about_author: true,
            front_matter: Some(front),
            back_matter: Some(back),
            ..Default::default()
        };
        let profile = AuthorProfile {
            name: "Ann Smith".into(),
            byline: "Ann Ellison".into(),
            contact: "1 Quay Street\nann@example.com".into(),
            website: "annellison.example".into(),
            bio: "Ann lives by the sea.".into(),
        };
        (p, settings, profile)
    }

    #[test]
    fn the_profile_fills_in_who_you_are_unless_the_project_says() {
        let dir = TempDir::new("profile");
        let (p, settings, profile) = book_with_matter(&dir);
        let m = gather(&p, &settings, &profile, read(&p));
        assert_eq!((m.author.as_str(), m.name.as_str()), ("Ann Ellison", "Ann Smith"), "the byline, and your own name");
        assert_eq!(m.contact, "1 Quay Street\nann@example.com");
        assert_eq!(m.bio, "Ann lives by the sea.");
        assert_eq!(m.copyright.len(), 3);
        assert!(m.copyright[0].starts_with("Copyright \u{a9} 20") && m.copyright[0].ends_with(" Ann Ellison"));
        assert_eq!(m.copyright[2], "annellison.example");
        // A project's own author and contact win.
        let own = CompileSettings { author: "A. E. Smith".into(), contact: "Agent: J. Bloggs".into(), ..settings.clone() };
        let m = gather(&p, &own, &profile, read(&p));
        assert_eq!((m.author.as_str(), m.contact.as_str()), ("A. E. Smith", "Agent: J. Bloggs"));
        // No byline: your name. Not asked for: no bio, no copyright.
        let plain = AuthorProfile { byline: String::new(), ..profile.clone() };
        let m = gather(&p, &CompileSettings::default(), &plain, read(&p));
        assert_eq!(m.author, "Ann Smith");
        assert!(m.bio.is_empty() && m.copyright.is_empty() && m.front.is_empty());
    }

    #[test]
    fn front_and_back_matter_come_from_their_folders() {
        let dir = TempDir::new("matter");
        let (p, settings, profile) = book_with_matter(&dir);
        let m = gather(&p, &settings, &profile, read(&p));
        assert_eq!(
            m.front,
            vec![
                Matter { heading: None, text: "For M.".into() },
                Matter { heading: Some("Foreword".into()), text: "A word *first*.".into() },
            ],
            "notes out, the heading taken off, what's not included left out"
        );
        assert_eq!(m.back, vec![Matter { heading: Some("Acknowledgements".into()), text: "Thanks to the ferry.".into() }]);
        assert_eq!(m.chapters.len(), 2, "the manuscript as ever");
        assert_eq!(m.words(), gather(&p, &CompileSettings::default(), &profile, read(&p)).words(), "matter isn't counted");

        // Markdown: pandoc's contents, the matter around the chapters.
        let md = to_markdown(&m, &settings);
        assert!(md.contains("toc: true\n") && md.contains("rights: \"Copyright"));
        let at = |t: &str| md.find(t).unwrap_or_else(|| panic!("no {t} in {md}"));
        assert!(at("For M.") < at("# Foreword") && at("# Foreword") < at("# Arrival"));
        assert!(at("# The Harbour") < at("# Acknowledgements") && at("# Acknowledgements") < at("# About the Author"));

        // Word: the copyright page, the matter, About the Author; your own
        // name and contact on the title page.
        let json = docx_rs::read_docx(&to_docx(&m, &settings).unwrap()).unwrap().json();
        for text in ["Ann Smith", "1 Quay Street", "annellison.example", "by Ann Ellison", "All rights reserved.", "For M.", "Foreword", "Acknowledgements", "About the Author", "Ann lives by the sea."] {
            assert!(json.contains(text), "missing {text}");
        }
    }

    #[test]
    fn an_ebook_has_its_contents_and_matter_as_pages() {
        use std::io::Read;
        let dir = TempDir::new("epub-matter");
        let (p, settings, profile) = book_with_matter(&dir);
        let m = gather(&p, &settings, &profile, read(&p));
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(to_epub(&m, &settings).unwrap())).unwrap();
        let mut file = |name: &str| {
            let mut text = String::new();
            zip.by_name(name).unwrap_or_else(|_| panic!("no {name}")).read_to_string(&mut text).unwrap();
            text
        };
        let opf = file("OEBPS/content.opf");
        let order: Vec<&str> = ["title", "copyright", "front-1", "front-2", "contents", "chapter-1", "chapter-2", "back-1", "about"]
            .into_iter()
            .collect();
        let spots: Vec<usize> = order.iter().map(|f| opf.find(&format!("href=\"{f}.xhtml\"")).unwrap_or_else(|| panic!("no {f}"))).collect();
        assert!(spots.windows(2).all(|w| w[0] < w[1]), "in order: {order:?}");
        assert!(file("OEBPS/front-1.xhtml").contains("class=\"dedication\""), "set on its own");
        let contents = file("OEBPS/contents.xhtml");
        for item in ["front-2.xhtml\">Foreword", "chapter-1.xhtml\">Arrival", "back-1.xhtml\">Acknowledgements", "about.xhtml\">About the Author"] {
            assert!(contents.contains(item), "{item} in {contents}");
        }
        assert!(!contents.contains("front-1.xhtml"), "a dedication has no place in the contents");
        assert!(file("OEBPS/copyright.xhtml").contains("All rights reserved."));
        // No title page if not asked for.
        let untitled = CompileSettings { title_page: false, ..settings };
        let m = gather(&p, &untitled, &profile, read(&p));
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(to_epub(&m, &untitled).unwrap())).unwrap();
        assert!(zip.by_name("OEBPS/title.xhtml").is_err());
    }

    #[test]
    fn gathers_chapters_in_order_without_notes_or_excluded_items() {
        let dir = TempDir::new("gather");
        let p = book(&dir);
        let settings = CompileSettings { author: " Ann Ellison ".into(), ..Default::default() };
        let m = gather(&p, &settings, &AuthorProfile::default(), read(&p));
        assert_eq!(m.author, "Ann Ellison");
        assert_eq!(
            m.chapters,
            [
                Chapter {
                    heading: "Arrival".into(),
                    scenes: vec!["Mara was *late*.\nThe ferry had gone.".into(), "Dear Mara,\n\n**Come home.**".into()],
                },
                Chapter { heading: "The Harbour".into(), scenes: vec!["The harbour froze.".into()] },
            ]
        );
        assert_eq!(m.words(), 14);

        let numbered = CompileSettings { chapter_headings: ChapterHeadings::NumbersAndTitles, ..Default::default() };
        let m = gather(&p, &numbered, &AuthorProfile::default(), read(&p));
        assert_eq!(m.chapters[1].heading, "Chapter 2: The Harbour");
    }

    #[test]
    fn documents_can_be_headed_with_their_titles() {
        let dir = TempDir::new("doc-titles");
        let p = book(&dir);
        let settings = CompileSettings { document_titles: true, ..Default::default() };
        let m = gather(&p, &settings, &AuthorProfile::default(), read(&p));
        assert_eq!(m.chapters[0].scenes, ["## Quay\n\nMara was *late*.\nThe ferry had gone.", "## Letter\n\nDear Mara,\n\n**Come home.**"]);
        // A chapter that's one document: its title is the chapter's heading.
        assert_eq!(m.chapters[1].scenes, ["The harbour froze."]);
        // Each opens with its heading, so no break mark between them.
        let md = to_markdown(&m, &settings);
        assert!(md.contains("# Arrival\n\n## Quay\n\nMara was *late*.\n\nThe ferry had gone.\n\n## Letter\n"), "{md}");
        assert!(!md.contains("* * *"), "{md}");
    }

    #[test]
    fn notes_come_out() {
        assert_eq!(strip_notes("a <!-- x --> b <!-- y -->c"), "a  b c");
        assert_eq!(strip_notes("a <!-- never closed"), "a ");
        assert_eq!(strip_notes("plain"), "plain");
    }

    #[test]
    fn reads_lines_as_paragraphs_with_emphasis() {
        let b = blocks("She was *very* late.\n\n# Part Two\n> a letter\n* * *\n1. She ran\n- Yes, she said.");
        let plain = |t: &str| Span { text: t.into(), italic: false, bold: false };
        assert_eq!(
            b,
            [
                Block::Paragraph(vec![plain("She was "), Span { text: "very".into(), italic: true, bold: false }, plain(" late.")]),
                Block::Heading(vec![plain("Part Two")]),
                Block::Quote(vec![plain("a letter")]),
                Block::SceneBreak,
                Block::Paragraph(vec![plain("1. She ran")]),
                Block::Paragraph(vec![plain("- Yes, she said.")]),
            ]
        );
    }

    #[test]
    fn markdown_has_a_title_block_chapters_and_scene_breaks() {
        let dir = TempDir::new("markdown");
        let p = book(&dir);
        let m = gather(&p, &CompileSettings { author: "Ann Ellison".into(), ..Default::default() }, &AuthorProfile::default(), read(&p));
        let md = to_markdown(&m, &CompileSettings::default());
        assert!(md.starts_with("---\ntitle: \"The Long Winter\"\nauthor: \"Ann Ellison\"\n---\n"), "{md}");
        assert!(md.contains("\n# Arrival\n\nMara was *late*.\n\nThe ferry had gone.\n\n* * *\n\nDear Mara,\n"), "{md}");
        assert!(md.contains("\n# The Harbour\n\nThe harbour froze.\n"), "{md}");
        assert!(!md.contains("check tides") && !md.contains("cut"));
        // A scene break typed in a scene matches the ones between scenes.
        let m = Manuscript {
            title: "T".into(),
            author: String::new(),
            chapters: vec![Chapter { heading: "One".into(), scenes: vec!["a\n#\nb".into()] }],
            ..Default::default()
        };
        assert!(to_markdown(&m, &CompileSettings::default()).contains("\na\n\n* * *\n\nb\n"));
        let asterism = CompileSettings { scene_break: SceneBreak::Asterism, ..Default::default() };
        assert!(to_markdown(&m, &asterism).contains("\na\n\n\u{2042}\n\nb\n"));
    }

    #[test]
    fn word_counts_round_like_a_manuscript() {
        assert_eq!(rounded_words(13), "about 100 words");
        assert_eq!(rounded_words(4_349), "about 4,300 words");
        assert_eq!(rounded_words(84_612), "about 85,000 words");
        assert_eq!(rounded_words(123_456), "about 123,000 words");
    }

    #[test]
    fn makes_a_word_file_with_the_text_and_italics() {
        let dir = TempDir::new("docx");
        let p = book(&dir);
        let m = gather(&p, &CompileSettings { author: "Ann Ellison".into(), ..Default::default() }, &AuthorProfile::default(), read(&p));
        let bytes = to_docx(&m, &CompileSettings::default()).unwrap();
        // Read it back and look at what's inside.
        let json = docx_rs::read_docx(&bytes).unwrap().json();
        for text in ["THE LONG WINTER", "by Ann Ellison", "about 100 words", "Arrival", "Mara was ", "late", "Come home."] {
            assert!(json.contains(text), "missing {text}");
        }
        assert!(json.contains("\"italic\": true") && json.contains("\"bold\": true"));
        assert!(!json.contains("check tides"));
    }

    #[test]
    fn word_paper_font_contact_and_breaks() {
        let dir = TempDir::new("docx-options");
        let p = book(&dir);
        let settings = CompileSettings {
            author: "Ann Ellison".into(),
            contact: "12 Harbour Road\nann@example.com\n".into(),
            paper: Paper::A4,
            typeface: Typeface::Courier,
            scene_break: SceneBreak::Stars,
            ..Default::default()
        };
        let m = gather(&p, &settings, &AuthorProfile::default(), read(&p));
        let json = docx_rs::read_docx(&to_docx(&m, &settings).unwrap()).unwrap().json();
        for text in ["12 Harbour Road", "ann@example.com", "Courier New", "* * *"] {
            assert!(json.contains(text), "missing {text}");
        }
        // A4 is 11906 by 16838 twentieths of a point.
        assert!(json.contains("11906") && json.contains("16838"));
        assert!(!json.contains("Times New Roman"));
    }

    #[test]
    fn makes_an_epub() {
        use std::io::Read;
        let dir = TempDir::new("epub");
        let p = book(&dir);
        let settings = CompileSettings { author: "Ann Ellison".into(), ..Default::default() };
        let m = gather(&p, &settings, &AuthorProfile::default(), read(&p));
        let bytes = to_epub(&m, &settings).unwrap();
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();

        // The mimetype first and uncompressed, as e-readers require.
        {
            let mut first = zip.by_index(0).unwrap();
            assert_eq!(first.name(), "mimetype");
            assert_eq!(first.compression(), zip::CompressionMethod::Stored);
            let mut text = String::new();
            first.read_to_string(&mut text).unwrap();
            assert_eq!(text, "application/epub+zip");
        }
        let mut file = |name: &str| {
            let mut text = String::new();
            zip.by_name(name).unwrap().read_to_string(&mut text).unwrap();
            text
        };
        let opf = file("OEBPS/content.opf");
        assert!(opf.contains("<dc:title>The Long Winter</dc:title>") && opf.contains("<dc:creator>Ann Ellison</dc:creator>"));
        assert!(opf.contains("chapter-2.xhtml") && opf.contains("properties=\"nav\""));
        let nav = file("OEBPS/nav.xhtml");
        assert!(nav.contains(">Arrival</a>") && nav.contains(">The Harbour</a>"));
        let one = file("OEBPS/chapter-1.xhtml");
        assert!(one.contains("<h1>Arrival</h1>"));
        assert!(one.contains("<p>Mara was <em>late</em>.</p>"), "{one}");
        assert!(one.contains("<strong>Come home.</strong>"));
        assert!(one.contains("<p class=\"break\">#</p>"));
        assert!(!one.contains("check tides"));
        file("META-INF/container.xml");
        file("OEBPS/style.css");

        // Every page is well-formed XML.
        for name in ["OEBPS/content.opf", "OEBPS/nav.xhtml", "OEBPS/title.xhtml", "OEBPS/chapter-1.xhtml", "OEBPS/chapter-2.xhtml", "META-INF/container.xml"] {
            let text = file(name);
            let options = roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() };
            if let Err(e) = roxmltree::Document::parse_with_options(&text, options) {
                panic!("{name} isn't well-formed: {e}");
            }
        }
    }

    #[test]
    fn presets_save_and_restore_choices() {
        let mut s = CompileSettings { author: "Ann".into(), format: Format::Epub, scene_break: SceneBreak::Stars, ..Default::default() };
        let beta = Preset::from_settings("Beta readers", &s);
        assert!(beta.matches(&s));
        s.format = Format::Manuscript;
        s.paper = Paper::A4;
        assert!(!beta.matches(&s));
        beta.apply(&mut s);
        assert_eq!((s.format, s.paper, s.scene_break), (Format::Epub, Paper::Letter, SceneBreak::Stars));
        assert_eq!(s.author, "Ann", "who you are isn't part of a preset");
    }

    #[test]
    fn older_compile_settings_still_load() {
        let old: CompileSettings = ron::from_str(r#"(author: "Ann", format: Markdown, chapter_headings: Numbers)"#).unwrap();
        assert_eq!((old.format, old.chapter_headings), (Format::Markdown, ChapterHeadings::Numbers));
        assert_eq!((old.paper, old.typeface, old.scene_break), (Paper::Letter, Typeface::Times, SceneBreak::Hash));
        assert!(old.presets.is_empty() && old.contact.is_empty());
    }

    /// A book with a Sources folder (one source with a citation, one
    /// without) and a Concepts folder, and two chapters that refer to them.
    fn essays(dir: &TempDir) -> Project {
        let mut p = book(dir);
        for id in ["sources", "concepts"] {
            let kind = crate::project::profile_presets().into_iter().map(|(k, _)| k).find(|k| k.id == id).unwrap();
            p.add_profile_folder(kind).unwrap();
        }
        let berger = p.add_document(&Place::Into("sources".into()), "Ways of Seeing").unwrap();
        fs::write(p.file_path(&berger).unwrap(), "A classic.\n\n## Citation\n\nBerger, John. *Ways of Seeing*.\nPenguin, 1972.\n").unwrap();
        p.add_document(&Place::Into("sources".into()), "Debord").unwrap();
        p.add_document(&Place::Into("concepts".into()), "The gaze").unwrap();
        let harbour = p.documents().into_iter().find(|n| n.title == "The Harbour").unwrap().id.clone();
        fs::write(
            p.file_path(&harbour).unwrap(),
            "The harbour froze [[Ways of Seeing|p. 8]] and [[The gaze]] stayed.\nSo it goes [[Debord]].\nAgain [[Ways of Seeing]].",
        )
        .unwrap();
        p
    }

    #[test]
    fn references_to_sources_become_notes_and_works_cited() {
        let dir = TempDir::new("cite");
        let p = essays(&dir);

        // Left out (the default): no notes, and no words from references.
        let plain = gather(&p, &CompileSettings::default(), &AuthorProfile::default(), read(&p));
        assert_eq!(plain.chapters[1].scenes, ["The harbour froze and stayed.\nSo it goes.\nAgain."]);
        assert!(plain.notes.is_empty() && plain.works_cited.is_empty());

        let settings = CompileSettings { citations: Citations::Notes, works_cited: true, ..Default::default() };
        let m = gather(&p, &settings, &AuthorProfile::default(), read(&p));
        assert_eq!(m.chapters[1].scenes, ["The harbour froze\u{e000}1\u{e001} and stayed.\nSo it goes\u{e000}2\u{e001}.\nAgain\u{e000}3\u{e001}."]);
        assert_eq!(m.notes, ["Berger, John. *Ways of Seeing*. Penguin, 1972, p. 8.", "Debord", "Berger, John. *Ways of Seeing*. Penguin, 1972."]);
        assert_eq!(m.works_cited, ["Berger, John. *Ways of Seeing*. Penguin, 1972.", "Debord"], "each once, alphabetically");
        assert_eq!(m.words(), plain.words(), "notes aren't words");

        // Works Cited alone, without notes.
        let cited_only = gather(&p, &CompileSettings { works_cited: true, ..Default::default() }, &AuthorProfile::default(), read(&p));
        assert!(cited_only.notes.is_empty() && cited_only.works_cited.len() == 2);

        // Markdown: pandoc footnotes, and the list.
        let md = to_markdown(&m, &settings);
        assert!(md.contains("\nThe harbour froze[^1] and stayed.\n"), "{md}");
        assert!(md.contains("\n[^1]: Berger, John. *Ways of Seeing*. Penguin, 1972, p. 8.\n[^2]: Debord\n"), "{md}");
        assert!(md.ends_with("\n# Works Cited\n\nBerger, John. *Ways of Seeing*. Penguin, 1972.\n\nDebord\n"), "{md}");

        // Word: real footnotes, and Works Cited at the end.
        use std::io::Read;
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(to_docx(&m, &settings).unwrap())).unwrap();
        let mut file = |name: &str| {
            let mut text = String::new();
            zip.by_name(name).unwrap().read_to_string(&mut text).unwrap();
            text
        };
        let document = file("word/document.xml");
        assert_eq!(document.matches("<w:footnoteReference").count(), 3);
        assert!(document.contains("Works Cited"));
        let footnotes = file("word/footnotes.xml");
        assert!(footnotes.contains("Penguin, 1972, p. 8."), "{footnotes}");
        assert!(file("word/styles.xml").contains("FootnoteReference"));

        // ePub: linked notes at the chapter's end, and a Works Cited page.
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(to_epub(&m, &settings).unwrap())).unwrap();
        let mut file = |name: &str| {
            let mut text = String::new();
            zip.by_name(name).unwrap().read_to_string(&mut text).unwrap();
            text
        };
        let two = file("OEBPS/chapter-2.xhtml");
        assert!(two.contains("froze<a epub:type=\"noteref\" href=\"#note-1\" id=\"ref-1\"><sup>1</sup></a> and"), "{two}");
        assert!(two.contains("<aside epub:type=\"footnote\" id=\"note-1\"><p><a href=\"#ref-1\">1</a>. Berger, John. <em>Ways of Seeing</em>. Penguin, 1972, p. 8.</p></aside>"), "{two}");
        assert!(file("OEBPS/works-cited.xhtml").contains("<p>Debord</p>"));
        assert!(file("OEBPS/content.opf").contains("works-cited.xhtml"));
    }
}
