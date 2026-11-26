//! Compile to PDF: the manuscript set as a book's pages, ready to read or
//! print. No other program is needed: the text is laid out here and the
//! file written with krilla, in a font installed on this computer (its
//! regular, italic, bold and bold italic faces, embedded).
//!
//! The pages: a title page, a copyright page, front matter, contents, then
//! each chapter on a new page a quarter of the way down, then back matter,
//! Works Cited and About the Author, each only if there's something for
//! it. Paragraphs are justified, with the first line indented except after
//! a heading or break. Notes go at the end of their chapter. Pages are
//! numbered from the first chapter, and the contents and the PDF's
//! bookmarks go to each item.

use std::collections::HashMap;
use std::sync::Arc;

use krilla::Document;
use krilla::annotation::{Annotation, LinkAnnotation, Target};
use krilla::destination::{Destination, XyzDestination};
use krilla::geom::{PathBuilder, Point, Rect};
use krilla::metadata::Metadata;
use krilla::outline::{Outline, OutlineNode};
use krilla::page::PageSettings;
use krilla::paint::Stroke;
use krilla::text::{Font, TextDirection};

use crate::compile::{
    ABOUT_HEADING, Block, CompileSettings, GLOSSARY_HEADING, Entry, Manuscript, Matter, Paper, Piece, Span, blocks, break_before, pieces, spans,
};

/// The four faces of a font family. A face the family hasn't got is
/// stood in for by the nearest it has.
struct Faces {
    /// Regular, italic, bold, bold italic.
    faces: [Face; 4],
}

struct Face {
    font: Font,
    data: Arc<Vec<u8>>,
    index: u32,
    /// Widths of words already measured, at 1 pt.
    widths: std::cell::RefCell<HashMap<String, f32>>,
}

impl Face {
    fn load(data: Vec<u8>, index: u32) -> Option<Face> {
        let data = Arc::new(data);
        let font = Font::new(data.clone().into(), index)?;
        rustybuzz::Face::from_slice(&data, index)?;
        Some(Face { font, data, index, widths: Default::default() })
    }

    /// How wide `text` is at `size` points, shaped as krilla will draw it.
    fn width(&self, text: &str, size: f32) -> f32 {
        if let Some(w) = self.widths.borrow().get(text) {
            return w * size;
        }
        let face = rustybuzz::Face::from_slice(&self.data, self.index).expect("checked when loaded");
        let mut buffer = rustybuzz::UnicodeBuffer::new();
        buffer.push_str(text);
        buffer.guess_segment_properties();
        let shaped = rustybuzz::shape(&face, &[], buffer);
        let units: i32 = shaped.glyph_positions().iter().map(|p| p.x_advance).sum();
        let w = units as f32 / face.units_per_em() as f32;
        self.widths.borrow_mut().insert(text.to_owned(), w);
        w * size
    }
}

impl Faces {
    /// The family's faces, if it has a regular one.
    fn load(family: &str) -> Option<Faces> {
        let face = |bold, italic| sloth_core::fonts::face_data(family, bold, italic).and_then(|(d, i)| Face::load(d, i));
        let regular = face(false, false)?;
        let italic = face(false, true);
        let bold = face(true, false);
        let bold_italic = face(true, true);
        // Stand-ins: each missing face from the nearest there is.
        let again = || sloth_core::fonts::face_data(family, false, false).and_then(|(d, i)| Face::load(d, i));
        let italic = italic.or_else(again)?;
        let bold = bold.or_else(again)?;
        let bold_italic = match bold_italic {
            Some(f) => f,
            None => sloth_core::fonts::face_data(family, false, true)
                .or_else(|| sloth_core::fonts::face_data(family, true, false))
                .or_else(|| sloth_core::fonts::face_data(family, false, false))
                .and_then(|(d, i)| Face::load(d, i))?,
        };
        Some(Faces { faces: [regular, italic, bold, bold_italic] })
    }

    fn get(&self, style: Style) -> &Face {
        &self.faces[style.italic as usize + 2 * style.bold as usize]
    }
}

/// The font to set the book in: the one chosen if it's installed, else
/// the default reading face, else any of the usual ones.
fn find_faces(chosen: Option<&str>) -> Result<(String, Faces), String> {
    let candidates = chosen
        .into_iter()
        .chain(sloth_core::fonts::default_prose_font())
        .chain(sloth_core::fonts::PROSE_DEFAULTS)
        .chain(["Times New Roman", "Times", "Cambria", "Book Antiqua"]);
    for family in candidates {
        if let Some(faces) = Faces::load(family) {
            return Ok((family.to_owned(), faces));
        }
    }
    Err("no font to set it in was found on this computer".to_owned())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
struct Style {
    italic: bool,
    bold: bool,
}

// ----- measures ----------------------------------------------------------------

/// The page's geometry and type sizes, in points.
#[derive(Clone, Copy)]
struct Geometry {
    width: f32,
    height: f32,
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
    size: f32,
    leading: f32,
}

impl Geometry {
    fn for_paper(paper: Paper) -> Geometry {
        let (width, height) = paper.points();
        match paper {
            Paper::Trade => Geometry { width, height, left: 54.0, right: 54.0, top: 54.0, bottom: 63.0, size: 11.0, leading: 15.5 },
            _ => Geometry { width, height, left: 90.0, right: 90.0, top: 72.0, bottom: 72.0, size: 12.0, leading: 17.0 },
        }
    }

    fn measure(&self) -> f32 {
        self.width - self.left - self.right
    }

    /// Where the last line's baseline may go.
    fn floor(&self) -> f32 {
        self.height - self.bottom
    }
}

// ----- what's drawn ------------------------------------------------------------

/// Something to draw on a page, top-down coordinates.
#[derive(Debug, Clone, PartialEq)]
enum Draw {
    Text { x: f32, baseline: f32, style: Style, size: f32, text: String },
    Rule { x1: f32, x2: f32, y: f32 },
}

#[derive(Default)]
struct Page {
    draws: Vec<Draw>,
    /// Links on the page: an area and the page it goes to.
    links: Vec<(Rect, usize)>,
}

/// A word, or the part of one in one style, with the gap after it.
#[derive(Debug, Clone)]
struct Run {
    text: String,
    style: Style,
    /// Note numbers are smaller and raised.
    note: bool,
    width: f32,
}

/// A word: its runs, and whether a space follows (none after a dash, where
/// a line may also break).
#[derive(Debug, Clone)]
struct Word {
    runs: Vec<Run>,
    space_after: bool,
}

impl Word {
    fn width(&self) -> f32 {
        self.runs.iter().map(|r| r.width).sum()
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Align {
    Justify,
    Left,
    Center,
}

/// A paragraph to set: its words and how.
struct Para {
    words: Vec<Word>,
    size: f32,
    leading: f32,
    first_indent: f32,
    left: f32,
    right: f32,
    align: Align,
    /// Hanging indent for the lines after the first (Works Cited).
    hanging: f32,
}

// ----- laying out ---------------------------------------------------------------

struct Setter<'a> {
    faces: &'a Faces,
    g: Geometry,
    pages: Vec<Page>,
    /// The next baseline on the current page.
    y: f32,
}

impl<'a> Setter<'a> {
    fn new(faces: &'a Faces, g: Geometry) -> Self {
        Setter { faces, g, pages: Vec::new(), y: 0.0 }
    }

    fn new_page(&mut self) {
        self.pages.push(Page::default());
        self.y = self.g.top + self.g.size;
    }

    fn page(&mut self) -> &mut Page {
        if self.pages.is_empty() {
            self.new_page();
        }
        self.pages.last_mut().unwrap()
    }

    /// Room for `lines` more lines of `leading` on this page?
    fn fits(&self, lines: usize, leading: f32) -> bool {
        !self.pages.is_empty() && self.y + leading * (lines.saturating_sub(1)) as f32 <= self.g.floor()
    }

    /// Start a page if what's on this one has reached the bottom.
    fn ensure(&mut self, lines: usize, leading: f32) {
        if !self.fits(lines, leading) {
            self.new_page();
        }
    }

    /// Words from styled spans, with note numbers as small raised runs.
    fn words(&self, spans: &[Span], size: f32, base: Style) -> Vec<Word> {
        let mut words: Vec<Word> = Vec::new();
        let mut current: Vec<Run> = Vec::new();
        let finish = |current: &mut Vec<Run>, words: &mut Vec<Word>, space: bool| {
            if !current.is_empty() {
                words.push(Word { runs: std::mem::take(current), space_after: space });
            } else if space && let Some(last) = words.last_mut() {
                last.space_after = true;
            }
        };
        for span in spans {
            let style = Style { italic: span.italic != base.italic, bold: span.bold || base.bold };
            for piece in pieces(&span.text) {
                match piece {
                    Piece::Text(t) => {
                        let mut part = String::new();
                        for c in t.chars() {
                            if c.is_whitespace() {
                                if !part.is_empty() {
                                    current.push(self.run(std::mem::take(&mut part), style, size, false));
                                }
                                finish(&mut current, &mut words, true);
                            } else {
                                part.push(c);
                                // A line may break after a dash.
                                if c == '\u{2014}' || c == '\u{2013}' {
                                    current.push(self.run(std::mem::take(&mut part), style, size, false));
                                    finish(&mut current, &mut words, false);
                                }
                            }
                        }
                        if !part.is_empty() {
                            current.push(self.run(part, style, size, false));
                        }
                    }
                    Piece::Note(n) => current.push(self.run(n.to_string(), Style::default(), size * 0.65, true)),
                }
            }
        }
        finish(&mut current, &mut words, false);
        if let Some(last) = words.last_mut() {
            last.space_after = false;
        }
        words
    }

    fn run(&self, text: String, style: Style, size: f32, note: bool) -> Run {
        let width = self.faces.get(style).width(&text, size);
        Run { text, style, note, width }
    }

    fn space(&self, size: f32) -> f32 {
        self.faces.get(Style::default()).width(" ", size)
    }

    /// Break a paragraph into lines: each the words on it, and its width
    /// without the space at the end.
    fn lines(&self, p: &Para) -> Vec<(Vec<Word>, f32)> {
        let space = self.space(p.size);
        let full = self.g.measure() - p.left - p.right;
        let mut lines: Vec<(Vec<Word>, f32)> = Vec::new();
        let mut line: Vec<Word> = Vec::new();
        let mut width = 0.0;
        for word in &p.words {
            let room = full - if lines.is_empty() { p.first_indent } else { p.hanging };
            let gap = match line.last() {
                Some(prev) if prev.space_after => space,
                _ => 0.0,
            };
            if !line.is_empty() && width + gap + word.width() > room {
                lines.push((std::mem::take(&mut line), width));
                width = 0.0;
            } else {
                width += gap;
            }
            width += word.width();
            line.push(word.clone());
        }
        if !line.is_empty() {
            lines.push((line, width));
        }
        lines
    }

    /// Set a paragraph, keeping at least two of its lines together at the
    /// foot and head of a page.
    fn para(&mut self, p: &Para) {
        let lines = self.lines(p);
        let total = lines.len();
        if total == 0 {
            return;
        }
        let space = self.space(p.size);
        let full = self.g.measure() - p.left - p.right;
        let mut i = 0;
        while i < total {
            self.ensure(1, p.leading);
            // How many of the rest fit on this page.
            let room = ((self.g.floor() - self.y) / p.leading).floor() as usize + 1;
            let mut take = room.min(total - i);
            if take < total - i {
                // No lone first line at the foot of a page…
                if i == 0 && take == 1 && total > 1 {
                    self.new_page();
                    continue;
                }
                // …nor a lone last line at the head of the next.
                if total - i - take == 1 && take > 1 {
                    take -= 1;
                }
            }
            for (n, (words, width)) in lines[i..i + take].iter().enumerate() {
                let index = i + n;
                let last = index + 1 == total;
                let indent = if index == 0 { p.first_indent } else { p.hanging };
                let room = full - indent;
                let gaps = words.iter().take(words.len().saturating_sub(1)).filter(|w| w.space_after).count();
                let (mut x, extra) = match p.align {
                    Align::Justify if !last && gaps > 0 => (self.g.left + p.left + indent, (room - width) / gaps as f32),
                    Align::Center => (self.g.left + p.left + (room - width) / 2.0, 0.0),
                    _ => (self.g.left + p.left + indent, 0.0),
                };
                let baseline = self.y;
                for word in words {
                    for run in &word.runs {
                        let (size, rise) = if run.note { (p.size * 0.65, p.size * 0.35) } else { (p.size, 0.0) };
                        self.page().draws.push(Draw::Text { x, baseline: baseline - rise, style: run.style, size, text: run.text.clone() });
                        x += run.width;
                    }
                    if word.space_after {
                        x += space + extra;
                    }
                }
                self.y += p.leading;
            }
            i += take;
            if i < total {
                self.new_page();
            }
        }
    }

    /// Some space, a fraction of a line, unless at the top of a page.
    fn gap(&mut self, lines: f32) {
        if !self.pages.is_empty() && self.y > self.g.top + self.g.size + 0.1 {
            self.y += self.g.leading * lines;
        }
    }

    fn body_para(&self, spans: &[Span], indent: bool) -> Para {
        Para {
            words: self.words(spans, self.g.size, Style::default()),
            size: self.g.size,
            leading: self.g.leading,
            first_indent: if indent { self.g.size * 1.5 } else { 0.0 },
            left: 0.0,
            right: 0.0,
            align: Align::Justify,
            hanging: 0.0,
        }
    }

    fn centered(&self, text: &str, size: f32, style: Style) -> Para {
        let spans = [Span { text: text.to_owned(), italic: style.italic, bold: style.bold }];
        Para {
            words: self.words(&spans, size, Style::default()),
            size,
            leading: size * 1.3,
            first_indent: 0.0,
            left: 0.0,
            right: 0.0,
            align: Align::Center,
            hanging: 0.0,
        }
    }

    /// A chapter's opening: a new page, its heading a quarter of the way
    /// down.
    fn opening(&mut self, heading: &str) {
        self.new_page();
        self.y = self.g.top + (self.g.height - self.g.top - self.g.bottom) * 0.22;
        let p = self.centered(heading, self.g.size * 1.6, Style::default());
        self.para(&p);
        self.y += self.g.leading * 1.5;
    }

    /// A scene's blocks, the first paragraph after a heading or break not
    /// indented. Returns whether the next paragraph should be.
    fn blocks(&mut self, scene: &str, mark: &str, mut indent: bool) -> bool {
        for block in blocks(scene) {
            match block {
                Block::Paragraph(s) => {
                    let p = self.body_para(&s, indent);
                    self.para(&p);
                    indent = true;
                }
                Block::Quote(s) => {
                    let mut p = self.body_para(&s, false);
                    p.words = self.words(&s, self.g.size, Style { italic: true, bold: false });
                    p.left = self.g.size * 2.0;
                    p.right = self.g.size * 2.0;
                    self.gap(0.3);
                    self.para(&p);
                    self.gap(0.3);
                    indent = false;
                }
                Block::Heading(s) => {
                    self.gap(1.0);
                    // Kept with the two lines after it.
                    self.ensure(3, self.g.leading);
                    let mut p = self.body_para(&s, false);
                    p.words = self.words(&s, self.g.size, Style { italic: false, bold: true });
                    p.align = Align::Center;
                    self.para(&p);
                    self.gap(0.5);
                    indent = false;
                }
                Block::SceneBreak => {
                    self.scene_break(mark);
                    indent = false;
                }
            }
        }
        indent
    }

    fn scene_break(&mut self, mark: &str) {
        self.gap(0.5);
        self.ensure(3, self.g.leading);
        let p = self.centered(mark, self.g.size, Style::default());
        self.para(&p);
        self.gap(0.5);
    }

    /// A page of matter: headed like a chapter, or on its own, centred a
    /// third of the way down, as a dedication is.
    fn matter(&mut self, matter: &Matter, mark: &str) {
        match &matter.heading {
            Some(h) => {
                self.opening(h);
                self.blocks(&matter.text, mark, false);
            }
            None => {
                self.new_page();
                self.y = self.g.top + (self.g.height - self.g.top - self.g.bottom) * 0.3;
                for block in blocks(&matter.text) {
                    let (spans, style) = match block {
                        Block::Paragraph(s) | Block::Heading(s) => (s, Style::default()),
                        Block::Quote(s) => (s, Style { italic: true, bold: false }),
                        Block::SceneBreak => (vec![Span { text: mark.to_owned(), italic: false, bold: false }], Style::default()),
                    };
                    let mut p = self.centered("", self.g.size, Style::default());
                    p.words = self.words(&spans, self.g.size, style);
                    p.leading = self.g.leading;
                    self.para(&p);
                }
            }
        }
    }

    /// The notes at the end of a chapter, smaller, under a short rule.
    fn notes(&mut self, numbers: &[usize], notes: &[String]) {
        if numbers.is_empty() {
            return;
        }
        self.gap(1.0);
        self.ensure(3, self.g.leading);
        let (x1, x2, y) = (self.g.left, self.g.left + self.g.measure() * 0.3, self.y - self.g.size * 0.6);
        self.page().draws.push(Draw::Rule { x1, x2, y });
        self.y += self.g.leading * 0.3;
        let size = self.g.size * 0.85;
        for &n in numbers {
            let text = notes.get(n - 1).cloned().unwrap_or_default();
            let mut s = vec![Span { text: format!("{n}. "), italic: false, bold: false }];
            s.extend(spans(&text));
            let p = Para {
                words: self.words(&s, size, Style::default()),
                size,
                leading: size * 1.35,
                first_indent: 0.0,
                left: 0.0,
                right: 0.0,
                align: Align::Left,
                hanging: 0.0,
            };
            self.para(&p);
            self.y += size * 0.3;
        }
    }
}

// ----- the book ------------------------------------------------------------------

/// The book laid out: its pages, and the bookmarks (title, page index).
struct Book {
    pages: Vec<Page>,
    bookmarks: Vec<(String, usize)>,
}

/// The PDF's bytes, and the font it was set in.
pub fn to_pdf(m: &Manuscript, settings: &CompileSettings) -> Result<(Vec<u8>, String), String> {
    let (family, faces) = find_faces(settings.pdf_font.as_deref())?;
    let g = Geometry::for_paper(settings.paper);
    let book = layout(m, settings, &faces);
    Ok((write(&book, &faces, g, m)?, family))
}

fn layout(m: &Manuscript, settings: &CompileSettings, faces: &Faces) -> Book {
    let g = Geometry::for_paper(settings.paper);
    let mark = settings.scene_break.text();

    // Front pages (not numbered): title, copyright, front matter.
    let mut front = Setter::new(faces, g);
    let mut starts: Vec<(String, Entry, usize, bool)> = Vec::new(); // title, entry, page, in front?
    if m.title_page {
        front.new_page();
        front.y = g.top + (g.height - g.top - g.bottom) * 0.3;
        let title = front.centered(&m.title, g.size * 2.2, Style::default());
        front.para(&title);
        if !m.author.is_empty() {
            front.y += g.leading * 1.5;
            let by = front.centered(&m.author, g.size * 1.25, Style::default());
            front.para(&by);
        }
    }
    if !m.copyright.is_empty() {
        front.new_page();
        let size = g.size * 0.85;
        front.y = g.floor() - size * 1.4 * (m.copyright.len() - 1) as f32;
        for line in &m.copyright {
            let mut p = front.centered(line, size, Style::default());
            p.align = Align::Left;
            front.para(&p);
        }
    }
    for (i, matter) in m.front.iter().enumerate() {
        front.matter(matter, mark);
        if let Some(h) = &matter.heading {
            starts.push((h.clone(), Entry::Front(i), front.pages.len() - 1, true));
        }
    }

    // The body: chapters and what follows, numbered from 1.
    let mut body = Setter::new(faces, g);
    for (n, chapter) in m.chapters.iter().enumerate() {
        body.opening(&chapter.heading);
        starts.push((chapter.heading.clone(), Entry::Chapter(n), body.pages.len() - 1, false));
        let mut indent = false;
        for (i, scene) in chapter.scenes.iter().enumerate() {
            if break_before(i, scene) {
                body.scene_break(mark);
                indent = false;
            }
            indent = body.blocks(scene, mark, indent);
        }
        let numbers: Vec<usize> = chapter
            .scenes
            .iter()
            .flat_map(|s| pieces(s).into_iter().filter_map(|p| if let Piece::Note(n) = p { Some(n) } else { None }).collect::<Vec<_>>())
            .collect();
        body.notes(&numbers, &m.notes);
    }
    for (i, matter) in m.back.iter().enumerate() {
        body.matter(matter, mark);
        if let Some(h) = &matter.heading {
            starts.push((h.clone(), Entry::Back(i), body.pages.len() - 1, false));
        }
    }
    if !m.works_cited.is_empty() {
        body.opening("Works Cited");
        starts.push(("Works Cited".into(), Entry::WorksCited, body.pages.len() - 1, false));
        for entry in &m.works_cited {
            let mut p = body.body_para(&spans(entry), false);
            p.align = Align::Left;
            p.hanging = g.size * 2.0;
            body.para(&p);
            body.y += g.size * 0.3;
        }
    }
    if !m.glossary.is_empty() {
        body.opening(GLOSSARY_HEADING);
        starts.push((GLOSSARY_HEADING.into(), Entry::Glossary, body.pages.len() - 1, false));
        for entry in &m.glossary {
            body.gap(0.4);
            // A term kept with the start of its definition.
            body.ensure(2, g.leading);
            let mut term = body.body_para(&[Span { text: entry.term.clone(), italic: false, bold: true }], false);
            term.align = Align::Left;
            body.para(&term);
            for line in entry.definition.lines().map(str::trim).filter(|l| !l.is_empty()) {
                let mut p = body.body_para(&spans(line), false);
                p.left = g.size * 1.5;
                body.para(&p);
            }
        }
    }
    if !m.bio.is_empty() {
        body.opening(ABOUT_HEADING);
        starts.push((ABOUT_HEADING.into(), Entry::About, body.pages.len() - 1, false));
        body.blocks(&m.bio, mark, false);
    }

    // Contents, now the page numbers are known, after the front matter.
    let mut contents = Setter::new(faces, g);
    let mut contents_rows: Vec<(usize, f32, f32, usize)> = Vec::new(); // page, top, bottom, entry index in starts
    if m.contents {
        contents.opening("Contents");
        for (k, (title, _, page, in_front)) in starts.iter().enumerate() {
            let number = if *in_front { String::new() } else { (page + 1).to_string() };
            let num_width = faces.get(Style::default()).width(&number, g.size);
            let mut p = contents.body_para(&[Span { text: title.clone(), italic: false, bold: false }], false);
            p.align = Align::Left;
            p.right = num_width + g.size * 2.0;
            p.hanging = g.size * 1.5;
            contents.ensure(1, g.leading);
            let top = contents.y - g.size;
            let page_index = contents.pages.len() - 1;
            contents.para(&p);
            // The number on the entry's last line, at the right.
            let baseline = contents.y - g.leading;
            let x = g.width - g.right - num_width;
            contents.page().draws.push(Draw::Text { x, baseline, style: Style::default(), size: g.size, text: number });
            if contents.pages.len() - 1 == page_index {
                contents_rows.push((page_index, top, contents.y - g.leading + g.size * 0.3, k));
            }
            contents.y += g.size * 0.35;
        }
    }

    // Put it together: front pages, contents, body.
    let first_body = front.pages.len() + contents.pages.len();
    let mut pages: Vec<Page> = Vec::new();
    pages.extend(front.pages);
    let contents_start = pages.len();
    pages.extend(contents.pages);
    let page_of = |(_, _, page, in_front): &(String, Entry, usize, bool)| if *in_front { *page } else { first_body + page };
    for &(page, top, bottom, k) in &contents_rows {
        if let Some(rect) = Rect::from_ltrb(g.left, top, g.width - g.right, bottom) {
            pages[contents_start + page].links.push((rect, page_of(&starts[k])));
        }
    }
    let body_count = body.pages.len();
    pages.extend(body.pages);

    // Page numbers at the foot of the body's pages.
    let size = g.size * 0.85;
    for n in 0..body_count {
        let text = (n + 1).to_string();
        let w = faces.get(Style::default()).width(&text, size);
        let baseline = g.height - g.bottom / 2.0;
        pages[first_body + n].draws.push(Draw::Text { x: (g.width - w) / 2.0, baseline, style: Style::default(), size, text });
    }
    let bookmarks = starts.iter().map(|start| (start.0.clone(), page_of(start))).collect();
    Book { pages, bookmarks }
}

/// The laid-out book as a PDF.
fn write(book: &Book, faces: &Faces, g: Geometry, m: &Manuscript) -> Result<Vec<u8>, String> {
    let mut document = Document::new();
    for page in &book.pages {
        let size = PageSettings::from_wh(g.width, g.height).ok_or("the paper size isn't one a PDF can have")?;
        let mut out = document.start_page_with(size);
        let mut surface = out.surface();
        for draw in &page.draws {
            match draw {
                Draw::Text { x, baseline, style, size, text } => {
                    let font = faces.get(*style).font.clone();
                    surface.draw_text(Point::from_xy(*x, *baseline), font, *size, text, false, TextDirection::LeftToRight);
                }
                Draw::Rule { x1, x2, y } => {
                    let mut path = PathBuilder::new();
                    path.move_to(*x1, *y);
                    path.line_to(*x2, *y);
                    if let Some(path) = path.finish() {
                        surface.set_stroke(Some(Stroke { width: 0.5, ..Default::default() }));
                        surface.draw_path(&path);
                        surface.set_stroke(None);
                    }
                }
            }
        }
        surface.finish();
        for (rect, to) in &page.links {
            let target = Target::Destination(Destination::Xyz(XyzDestination::new(*to, Point::from_xy(0.0, 0.0))));
            out.add_annotation(Annotation::new_link(LinkAnnotation::new(*rect, target), None));
        }
        out.finish();
    }

    // Bookmarks: each item, as the contents has them.
    let mut outline = Outline::new();
    for (title, page) in &book.bookmarks {
        outline.push_child(OutlineNode::new(title.clone(), XyzDestination::new(*page, Point::from_xy(0.0, 0.0))));
    }
    document.set_outline(outline);
    let mut metadata = Metadata::new().title(m.title.clone()).creator("Sloth Writer".to_owned());
    if !m.author.is_empty() {
        metadata = metadata.authors(vec![m.author.clone()]);
    }
    document.set_metadata(metadata);
    document.finish().map_err(|e| format!("{e:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::Chapter;

    fn manuscript(chapters: usize, paragraphs: usize) -> Manuscript {
        let scene = (0..paragraphs)
            .map(|i| format!("Paragraph {i}: the snow came early that year, *three weeks* before the first frost was due, and the harbour froze."))
            .collect::<Vec<_>>()
            .join("\n");
        Manuscript {
            title: "The Long Winter".into(),
            author: "Ann Ellison".into(),
            name: "Ann Ellison".into(),
            contact: String::new(),
            website: String::new(),
            bio: "Ann lives by the sea.".into(),
            copyright: vec!["Copyright \u{a9} 2026 Ann Ellison".into(), "All rights reserved.".into()],
            title_page: true,
            contents: true,
            front: vec![Matter { heading: None, text: "For M.".into() }],
            back: vec![Matter { heading: Some("Acknowledgements".into()), text: "Thanks.".into() }],
            chapters: (0..chapters).map(|c| Chapter { heading: format!("Chapter {}", c + 1), scenes: vec![scene.clone(), "After.".into()] }).collect(),
            notes: Vec::new(),
            works_cited: Vec::new(),
            glossary: Vec::new(),
        }
    }

    fn faces() -> Option<Faces> {
        find_faces(None).ok().map(|(_, f)| f)
    }

    /// The text drawn on a page, in order.
    fn words(page: &Page) -> String {
        page.draws.iter().filter_map(|d| if let Draw::Text { text, .. } = d { Some(text.as_str()) } else { None }).collect::<Vec<_>>().join(" ")
    }

    #[test]
    fn lays_out_front_matter_contents_chapters_and_back_matter() {
        let Some(faces) = faces() else { return };
        let m = manuscript(3, 30);
        let book = layout(&m, &CompileSettings::default(), &faces);
        let text: Vec<String> = book.pages.iter().map(words).collect();
        assert!(text[0].starts_with("The Long Winter") && text[0].contains("Ann Ellison"), "title page: {}", text[0]);
        assert!(text[1].contains("All rights reserved."), "copyright page");
        assert_eq!(text[2], "For M.", "the dedication, on its own");
        assert!(text[3].starts_with("Contents"), "then the contents: {}", text[3]);
        assert!(text[4].starts_with("Chapter 1"), "then the first chapter");
        // The contents: each item with its page, numbered from the first
        // chapter, and a link to it.
        assert!(text[3].contains("Chapter 1 1"), "{}", text[3]);
        let links = &book.pages[3].links;
        assert_eq!(links.len(), 5, "three chapters, acknowledgements, about");
        assert_eq!(links[0].1, 4, "to the first chapter's page");
        // The last page: About the Author, with its page number.
        let last = text.last().unwrap();
        assert!(last.starts_with(ABOUT_HEADING) && last.ends_with(&(book.pages.len() - 4).to_string()), "{last}");
        // Bookmarks for the same.
        assert_eq!(book.bookmarks.first(), Some(&("Chapter 1".to_owned(), 4)));
        assert_eq!(book.bookmarks.len(), 5);
    }

    #[test]
    fn a_glossary_gets_a_page_and_a_place_in_the_contents() {
        let Some(faces) = faces() else { return };
        let mut m = manuscript(1, 3);
        m.glossary = vec![
            crate::glossary::Entry { term: "Ansible".into(), definition: "Sends at once.".into() },
            crate::glossary::Entry { term: "Zephyr".into(), definition: "A west wind.".into() },
        ];
        let book = layout(&m, &CompileSettings::default(), &faces);
        let titles: Vec<&str> = book.bookmarks.iter().map(|(t, _)| t.as_str()).collect();
        assert_eq!(titles, ["Chapter 1", "Acknowledgements", "Glossary", ABOUT_HEADING], "before About the Author");
        let page = &book.pages[book.bookmarks[2].1];
        assert!(words(page).starts_with("Glossary Ansible Sends at once. Zephyr A west wind."), "{}", words(page));
    }

    #[test]
    fn makes_a_pdf() {
        if faces().is_none() {
            return; // no font installed to test with
        }
        let mut m = manuscript(2, 10);
        // Something of everything: a quote, a heading, a break, a note.
        m.chapters[0].scenes[1] = format!(
            "After the storm.{}\n> The sea does not forgive.\n## Morning\nShe walked to the quay—the long way—and waited.\n***\nThe ferry came.",
            crate::compile::note_mark(1)
        );
        m.notes = vec!["A note on *storms*.".into()];
        let (bytes, family) = to_pdf(&m, &CompileSettings::default()).unwrap();
        assert!(bytes.starts_with(b"%PDF-"), "a PDF");
        assert!(!family.is_empty());
        if let Ok(dir) = std::env::var("SHOT_DIR") {
            std::fs::write(format!("{dir}/book.pdf"), &bytes).unwrap();
        }
    }

    #[test]
    fn lines_fill_the_measure_and_keep_two_lines_together() {
        let Some(faces) = faces() else { return };
        let g = Geometry::for_paper(Paper::Letter);
        let mut s = Setter::new(&faces, g);
        let long = "the snow came early that year ".repeat(40);
        let p = s.body_para(&[Span { text: long, italic: false, bold: false }], true);
        let lines = s.lines(&p);
        assert!(lines.len() > 5);
        for (i, (_, w)) in lines.iter().enumerate() {
            let room = g.measure() - if i == 0 { p.first_indent } else { 0.0 };
            assert!(*w <= room + 0.01, "line {i} is {w} wide, room for {room}");
        }
        // Fill a page to its last line, then a paragraph of three lines:
        // it shouldn't leave one line behind.
        s.new_page();
        s.y = g.floor();
        let three = "word ".repeat(lines[0].0.len() * 2 + 3);
        let p = s.body_para(&[Span { text: three, italic: false, bold: false }], false);
        assert_eq!(s.lines(&p).len(), 3);
        s.para(&p);
        assert_eq!(s.pages.len(), 2, "moved to a new page");
        assert!(s.pages[0].draws.is_empty(), "nothing left at the foot of the first");
    }

    #[test]
    fn note_numbers_are_small_and_raised() {
        let Some(faces) = faces() else { return };
        let g = Geometry::for_paper(Paper::Trade);
        let mut s = Setter::new(&faces, g);
        let text = format!("She left.{}", crate::compile::note_mark(1));
        let p = s.body_para(&spans(&text), false);
        s.para(&p);
        let draws = &s.pages[0].draws;
        let Draw::Text { baseline: word, .. } = &draws[0] else { panic!() };
        let Draw::Text { baseline: note, size, text, .. } = draws.last().unwrap() else { panic!() };
        assert_eq!(text, "1");
        assert!(note < word && *size < g.size);
    }
}
