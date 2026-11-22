//! Compile: turn the Manuscript into one finished file.
//!
//! First the manuscript is gathered (`gather`): the Manuscript folder's items
//! in binder order, minus anything marked not to be included, with inline
//! notes (`<!-- … -->`) taken out. Each top-level item is a chapter: a folder
//! with everything in it, or a document (with anything under it). Then it's
//! written as Markdown (`to_markdown`, ready for pandoc or anything else) or
//! as a Word file in standard manuscript format (`to_docx`), or as an ePub
//! e-book for reading on an iPad or e-reader (`to_epub`).
//!
//! Text is read the way people write in the editor: each line is a
//! paragraph (blank lines are just spacing), `*italic*` and `**bold**` are
//! kept, and a line of `#`, `***` or `---` is a scene break.

use docx_rs::{
    AlignmentType, BreakType, Docx, Header, LineSpacing, LineSpacingType, PageMargin, PageNum, Paragraph, Run,
    RunFonts, SpecialIndentType, Tab, TabValueType,
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
    /// An e-book, for reading on an iPad or e-reader.
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
    pub paper: Paper,
    pub typeface: Typeface,
    pub scene_break: SceneBreak,
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
    pub paper: Paper,
    pub typeface: Typeface,
    pub scene_break: SceneBreak,
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
            paper: s.paper,
            typeface: s.typeface,
            scene_break: s.scene_break,
        }
    }

    /// Put this setup's choices into the settings.
    pub fn apply(&self, s: &mut CompileSettings) {
        s.format = self.format;
        s.chapter_headings = self.chapter_headings;
        s.paper = self.paper;
        s.typeface = self.typeface;
        s.scene_break = self.scene_break;
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
}

#[derive(Debug, PartialEq)]
pub struct Chapter {
    pub heading: String,
    /// Each scene's text, notes removed.
    pub scenes: Vec<String>,
}

impl Manuscript {
    pub fn words(&self) -> usize {
        self.chapters.iter().flat_map(|c| &c.scenes).map(|s| s.split_whitespace().count()).sum()
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
    for item in manuscript.children.iter().filter(|n| !n.meta.exclude_from_compile) {
        let mut scenes = Vec::new();
        collect_scenes(item, &text, &mut scenes);
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
    Manuscript { title: project.title().to_owned(), author: settings.author.trim().to_owned(), chapters }
}

/// The scenes in an item, in order, skipping anything not included.
fn collect_scenes(node: &Node, text: &impl Fn(&Node) -> String, out: &mut Vec<String>) {
    if node.meta.exclude_from_compile {
        return;
    }
    if node.is_document() {
        let scene = strip_notes(&text(node));
        if !scene.trim().is_empty() {
            out.push(scene.trim().to_owned());
        }
    }
    for child in &node.children {
        collect_scenes(child, text, out);
    }
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
            if i > 0 {
                out.push_str(&format!("\n{separator}\n"));
            }
            for line in scene.lines().map(str::trim).filter(|l| !l.is_empty()) {
                out.push('\n');
                // A break typed in a scene: the same as between scenes.
                out.push_str(if is_scene_break(line) { separator } else { line });
                out.push('\n');
            }
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
            if i > 0 {
                doc = doc.add_paragraph(scene_break(mark, face));
            }
            for block in blocks(scene) {
                doc = doc.add_paragraph(match block {
                    Block::Paragraph(spans) => runs(body().indent(None, Some(SpecialIndentType::FirstLine(INCH / 2)), None, None), spans, face),
                    Block::Quote(spans) => runs(body().indent(Some(INCH / 2), None, Some(INCH / 2), None), spans, face),
                    Block::Heading(spans) => runs(body().align(AlignmentType::Center), spans, face),
                    Block::SceneBreak => scene_break(mark, face),
                });
            }
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

fn runs(mut paragraph: Paragraph, spans: Vec<Span>, face: &str) -> Paragraph {
    for span in spans {
        let mut run = run(face).add_text(span.text);
        if span.italic {
            run = run.italic();
        }
        if span.bold {
            run = run.bold();
        }
        paragraph = paragraph.add_run(run);
    }
    paragraph
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

/// A paragraph's text, with its emphasis, as XHTML.
fn xhtml_spans(spans: &[Span]) -> String {
    spans
        .iter()
        .map(|s| {
            let mut t = xml(&s.text);
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
.title h1 { margin: 0 0 0.5em; font-size: 2em; }\n";

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
            if i > 0 {
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
        pages.push((format!("chapter-{}.xhtml", n + 1), chapter.heading.clone(), xhtml_page(&chapter.heading, &body)));
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

        // Every page is well-formed XML (checked with xmllint where it's installed).
        if std::path::Path::new("/usr/bin/xmllint").exists() {
            for name in ["OEBPS/content.opf", "OEBPS/nav.xhtml", "OEBPS/title.xhtml", "OEBPS/chapter-1.xhtml", "OEBPS/chapter-2.xhtml", "META-INF/container.xml"] {
                let path = dir.0.join(name.replace('/', "_"));
                std::fs::write(&path, file(name)).unwrap();
                let ok = std::process::Command::new("/usr/bin/xmllint").args(["--noout"]).arg(&path).status().unwrap();
                assert!(ok.success(), "{name} isn't well-formed");
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
}
