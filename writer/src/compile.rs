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
}

impl Format {
    pub fn extension(self) -> &'static str {
        match self {
            Format::Manuscript => "docx",
            Format::Markdown => "md",
            Format::Epub => "epub",
        }
    }
}

/// The Word manuscript's paper.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Paper {
    #[default]
    Letter,
    A4,
}

impl Paper {
    /// Width and height in twentieths of a point.
    fn size(self) -> (u32, u32) {
        match self {
            Paper::Letter => (12_240, 15_840),
            Paper::A4 => (11_906, 16_838),
        }
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

/// Compile choices, kept in the project so they're there next time.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CompileSettings {
    /// The author's name as it should appear: "Mara Ellison".
    pub author: String,
    /// Address, email, phone, agent…: one per line, on the title page.
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
    /// Saved setups ("Agent submission", "Beta readers"…).
    pub presets: Vec<Preset>,
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
    }

    /// Whether the settings are as this setup has them.
    pub fn matches(&self, s: &CompileSettings) -> bool {
        *self == Preset::from_settings(&self.name, s)
    }
}

/// The gathered manuscript.
#[derive(Debug, PartialEq)]
pub struct Manuscript {
    pub title: String,
    pub author: String,
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

fn note_mark(n: usize) -> String {
    format!("{NOTE_OPEN}{n}{NOTE_CLOSE}")
}

/// A piece of text: words, or where a note goes.
#[derive(Debug, PartialEq)]
enum Piece<'a> {
    Text(&'a str),
    Note(usize),
}

/// Text split at its note marks.
fn pieces(text: &str) -> Vec<Piece<'_>> {
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

#[derive(Debug, PartialEq)]
pub struct Chapter {
    pub heading: String,
    /// Each scene's text, notes removed.
    pub scenes: Vec<String>,
}

impl Manuscript {
    pub fn words(&self) -> usize {
        self.chapters.iter().flat_map(|c| &c.scenes).map(|s| without_marks(s).split_whitespace().count()).sum()
    }
}

/// Gather the Manuscript folder. `text` gives each document's text: from
/// the editor if it's open, else from its file.
pub fn gather(project: &Project, settings: &CompileSettings, text: impl Fn(&Node) -> String) -> Manuscript {
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
    cited.sort_by_key(|c| c.to_lowercase());
    Manuscript { title: project.title().to_owned(), author: settings.author.trim().to_owned(), chapters, notes, works_cited: cited }
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
fn break_before(i: usize, scene: &str) -> bool {
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
enum Block {
    Paragraph(Vec<Span>),
    /// A heading inside a scene.
    Heading(Vec<Span>),
    /// `> quoted` lines.
    Quote(Vec<Span>),
    SceneBreak,
}

/// A run of text with its emphasis.
#[derive(Debug, PartialEq)]
struct Span {
    text: String,
    italic: bool,
    bold: bool,
}

fn blocks(scene: &str) -> Vec<Block> {
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

fn is_scene_break(line: &str) -> bool {
    let marks: String = line.chars().filter(|c| !c.is_whitespace()).collect();
    marks == "#" || (marks.len() >= 3 && (marks.chars().all(|c| c == '*') || marks.chars().all(|c| c == '-')))
}

/// One line's text and emphasis. Markdown's block syntax (lists and so on)
/// is switched off by escaping, so a line like "1. She ran" stays as typed.
fn spans(line: &str) -> Vec<Span> {
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
    out.push_str("---\n");
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
    if !m.works_cited.is_empty() {
        out.push_str("\n# Works Cited\n");
        for entry in &m.works_cited {
            out.push_str(&format!("\n{entry}\n"));
        }
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
            .add_run(run(face).add_text(&m.author).add_tab().add_text(rounded_words(m.words()))),
    );
    // Contact details under the name, single spaced.
    for line in settings.contact.lines().map(str::trim).filter(|l| !l.is_empty()) {
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
    for chapter in &m.chapters {
        doc = doc.add_paragraph(
            Paragraph::new()
                .align(AlignmentType::Center)
                .line_spacing(LineSpacing::new().before(2 * INCH as u32).after(480).line(480))
                .add_run(page_break())
                .add_run(run(face).add_text(&chapter.heading)),
        );
        for (i, scene) in chapter.scenes.iter().enumerate() {
            if break_before(i, scene) {
                doc = doc.add_paragraph(scene_break(mark, face));
            }
            for block in blocks(scene) {
                doc = doc.add_paragraph(match block {
                    Block::Paragraph(spans) => runs(body().indent(None, Some(SpecialIndentType::FirstLine(INCH / 2)), None, None), spans, face, &m.notes),
                    Block::Quote(spans) => runs(body().indent(Some(INCH / 2), None, Some(INCH / 2), None), spans, face, &m.notes),
                    Block::Heading(spans) => runs(body().align(AlignmentType::Center), spans, face, &m.notes),
                    Block::SceneBreak => scene_break(mark, face),
                });
            }
        }
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
    let mut pages: Vec<(String, String, String)> = Vec::new(); // file, title, xhtml
    let byline = if m.author.is_empty() { String::new() } else { format!("<p>{}</p>", xml(&m.author)) };
    pages.push((
        "title.xhtml".into(),
        m.title.clone(),
        xhtml_page(&m.title, &format!("<div class=\"title\"><h1>{}</h1>{byline}</div>", xml(&m.title))),
    ));
    for (n, chapter) in m.chapters.iter().enumerate() {
        let mut body = format!("<h1>{}</h1>\n", xml(&chapter.heading));
        for (i, scene) in chapter.scenes.iter().enumerate() {
            if break_before(i, scene) {
                body.push_str(&format!("<p class=\"break\">{}</p>\n", xml(mark)));
            }
            for block in blocks(scene) {
                body.push_str(&match block {
                    Block::Paragraph(spans) => format!("<p>{}</p>\n", xhtml_spans(&spans)),
                    Block::Quote(spans) => format!("<blockquote><p>{}</p></blockquote>\n", xhtml_spans(&spans)),
                    Block::Heading(spans) => format!("<h2>{}</h2>\n", xhtml_spans(&spans)),
                    Block::SceneBreak => format!("<p class=\"break\">{}</p>\n", xml(mark)),
                });
            }
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
        pages.push((format!("chapter-{}.xhtml", n + 1), chapter.heading.clone(), xhtml_page(&chapter.heading, &body)));
    }
    if !m.works_cited.is_empty() {
        let entries: String = m.works_cited.iter().map(|e| format!("<p>{}</p>\n", xhtml_spans(&spans(e)))).collect();
        let body = format!("<h1>Works Cited</h1>\n<div class=\"cited\">\n{entries}</div>");
        pages.push(("works-cited.xhtml".into(), "Works Cited".into(), xhtml_page("Works Cited", &body)));
    }

    let toc: String = pages
        .iter()
        .skip(1)
        .map(|(file, title, _)| format!("<li><a href=\"{file}\">{}</a></li>", xml(title)))
        .collect();
    let nav = xhtml_page(
        "Contents",
        &format!("<nav epub:type=\"toc\" id=\"toc\"><h1>Contents</h1><ol>{toc}</ol></nav>"),
    );
    let manifest: String = pages
        .iter()
        .enumerate()
        .map(|(i, (file, _, _))| format!("<item id=\"p{i}\" href=\"{file}\" media-type=\"application/xhtml+xml\"/>"))
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
    for (file, _, page) in &pages {
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

    #[test]
    fn gathers_chapters_in_order_without_notes_or_excluded_items() {
        let dir = TempDir::new("gather");
        let p = book(&dir);
        let settings = CompileSettings { author: " Ann Ellison ".into(), ..Default::default() };
        let m = gather(&p, &settings, read(&p));
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
        let m = gather(&p, &numbered, read(&p));
        assert_eq!(m.chapters[1].heading, "Chapter 2: The Harbour");
    }

    #[test]
    fn documents_can_be_headed_with_their_titles() {
        let dir = TempDir::new("doc-titles");
        let p = book(&dir);
        let settings = CompileSettings { document_titles: true, ..Default::default() };
        let m = gather(&p, &settings, read(&p));
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
        let m = gather(&p, &CompileSettings { author: "Ann Ellison".into(), ..Default::default() }, read(&p));
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
            notes: Vec::new(),
            works_cited: Vec::new(),
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
        let m = gather(&p, &CompileSettings { author: "Ann Ellison".into(), ..Default::default() }, read(&p));
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
        let m = gather(&p, &settings, read(&p));
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
        let m = gather(&p, &settings, read(&p));
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
        let plain = gather(&p, &CompileSettings::default(), read(&p));
        assert_eq!(plain.chapters[1].scenes, ["The harbour froze and stayed.\nSo it goes.\nAgain."]);
        assert!(plain.notes.is_empty() && plain.works_cited.is_empty());

        let settings = CompileSettings { citations: Citations::Notes, works_cited: true, ..Default::default() };
        let m = gather(&p, &settings, read(&p));
        assert_eq!(m.chapters[1].scenes, ["The harbour froze\u{e000}1\u{e001} and stayed.\nSo it goes\u{e000}2\u{e001}.\nAgain\u{e000}3\u{e001}."]);
        assert_eq!(m.notes, ["Berger, John. *Ways of Seeing*. Penguin, 1972, p. 8.", "Debord", "Berger, John. *Ways of Seeing*. Penguin, 1972."]);
        assert_eq!(m.works_cited, ["Berger, John. *Ways of Seeing*. Penguin, 1972.", "Debord"], "each once, alphabetically");
        assert_eq!(m.words(), plain.words(), "notes aren't words");

        // Works Cited alone, without notes.
        let cited_only = gather(&p, &CompileSettings { works_cited: true, ..Default::default() }, read(&p));
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
