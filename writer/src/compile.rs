//! Compile: turn the Manuscript into one finished file.
//!
//! First the manuscript is gathered (`gather`): the Manuscript folder's items
//! in binder order, minus anything marked not to be included, with inline
//! notes (`<!-- … -->`) taken out. Each top-level item is a chapter: a folder
//! with everything in it, or a document (with anything under it). Then it's
//! written as Markdown (`to_markdown`, ready for pandoc or anything else) or
//! as a Word file in standard manuscript format (`to_docx`).
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
}

impl Format {
    pub fn extension(self) -> &'static str {
        match self {
            Format::Manuscript => "docx",
            Format::Markdown => "md",
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
    pub format: Format,
    pub chapter_headings: ChapterHeadings,
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
pub fn to_markdown(m: &Manuscript) -> String {
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
                out.push_str("\n* * *\n");
            }
            for line in scene.lines().map(str::trim).filter(|l| !l.is_empty()) {
                out.push('\n');
                // A lone `#` would read as an empty heading: use the same
                // break as between scenes.
                out.push_str(if is_scene_break(line) { "* * *" } else { line });
                out.push('\n');
            }
        }
    }
    out
}

// ----- Word manuscript -----------------------------------------------------------

/// Twentieths of a point, Word's unit for page geometry.
const INCH: i32 = 1440;
/// Letter paper.
const PAGE_W: u32 = 12_240;
const PAGE_H: u32 = 15_840;

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

/// Standard manuscript format: Times New Roman 12 pt, double spaced, 1 inch
/// margins, a title page, each chapter on a new page a third of the way
/// down, `#` between scenes, and "Surname / Title / page" at the top right
/// of every page after the first.
pub fn to_docx(m: &Manuscript) -> Result<Vec<u8>, String> {
    let surname = m.author.split_whitespace().last().unwrap_or("").to_owned();
    let header_text = if surname.is_empty() { format!("{} / ", m.title) } else { format!("{surname} / {} / ", m.title) };
    let header = Header::new().add_paragraph(
        Paragraph::new().align(AlignmentType::Right).add_run(Run::new().add_text(header_text)).add_page_num(PageNum::new()),
    );

    let mut doc = Docx::new()
        .page_size(PAGE_W, PAGE_H)
        .page_margin(PageMargin::new().top(INCH).bottom(INCH).left(INCH).right(INCH).header(720))
        .default_fonts(RunFonts::new().ascii("Times New Roman").hi_ansi("Times New Roman").cs("Times New Roman"))
        .default_size(24) // half-points: 12 pt
        .default_line_spacing(LineSpacing::new().line_rule(LineSpacingType::Auto).line(480).after(0))
        .header(header)
        // No header on the title page.
        .first_header(Header::new())
        .title_pg();

    // Title page: name top left, word count top right, title and byline
    // centred halfway down.
    let single = || LineSpacing::new().line_rule(LineSpacingType::Auto).line(240).after(0);
    let text_width = PAGE_W as usize - 2 * INCH as usize;
    doc = doc.add_paragraph(
        Paragraph::new()
            .line_spacing(single())
            .add_tab(Tab::new().val(TabValueType::Right).pos(text_width))
            .add_run(Run::new().add_text(&m.author).add_tab().add_text(rounded_words(m.words()))),
    );
    doc = doc.add_paragraph(
        Paragraph::new()
            .align(AlignmentType::Center)
            .line_spacing(LineSpacing::new().before(4 * INCH as u32).line(480))
            .add_run(Run::new().add_text(m.title.to_uppercase())),
    );
    if !m.author.is_empty() {
        doc = doc.add_paragraph(body().align(AlignmentType::Center).add_run(Run::new().add_text(format!("by {}", m.author))));
    }

    // Spacing and page breaks are set on each paragraph rather than left to
    // document defaults: some readers (Quick Look, TextEdit) ignore those.
    for chapter in &m.chapters {
        doc = doc.add_paragraph(
            Paragraph::new()
                .align(AlignmentType::Center)
                .line_spacing(LineSpacing::new().before(2 * INCH as u32).after(480).line(480))
                .add_run(page_break())
                .add_run(Run::new().add_text(&chapter.heading)),
        );
        for (i, scene) in chapter.scenes.iter().enumerate() {
            if i > 0 {
                doc = doc.add_paragraph(scene_break());
            }
            for block in blocks(scene) {
                doc = doc.add_paragraph(match block {
                    Block::Paragraph(spans) => runs(body().indent(None, Some(SpecialIndentType::FirstLine(INCH / 2)), None, None), spans),
                    Block::Quote(spans) => runs(body().indent(Some(INCH / 2), None, Some(INCH / 2), None), spans),
                    Block::Heading(spans) => runs(body().align(AlignmentType::Center), spans),
                    Block::SceneBreak => scene_break(),
                });
            }
        }
    }

    let mut bytes = std::io::Cursor::new(Vec::new());
    doc.build().pack(&mut bytes).map_err(|e| e.to_string())?;
    Ok(bytes.into_inner())
}

/// A double-spaced paragraph.
fn body() -> Paragraph {
    Paragraph::new().line_spacing(LineSpacing::new().line_rule(LineSpacingType::Auto).line(480).after(0))
}

fn scene_break() -> Paragraph {
    body().align(AlignmentType::Center).add_run(Run::new().add_text("#"))
}

fn runs(mut paragraph: Paragraph, spans: Vec<Span>) -> Paragraph {
    for span in spans {
        let mut run = Run::new().add_text(span.text);
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
        let md = to_markdown(&m);
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
        assert!(to_markdown(&m).contains("\na\n\n* * *\n\nb\n"));
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
        let bytes = to_docx(&m).unwrap();
        // Read it back and look at what's inside.
        let json = docx_rs::read_docx(&bytes).unwrap().json();
        for text in ["THE LONG WINTER", "by Ann Ellison", "about 100 words", "Arrival", "Mara was ", "late", "Come home."] {
            assert!(json.contains(text), "missing {text}");
        }
        assert!(json.contains("\"italic\": true") && json.contains("\"bold\": true"));
        assert!(!json.contains("check tides"));
    }
}
