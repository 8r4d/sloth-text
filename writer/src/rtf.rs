//! RTF (as Scrivener writes it, via macOS) to Writer's Markdown: one line
//! per paragraph; **bold**, *italic*; headings and block quotes from
//! Scrivener's styles; comments and footnotes as `<!-- notes -->`; links
//! as their text (web links as Markdown links); lists with their numbers
//! or bullets; table cells separated by " | ". Pictures can't come across:
//! each is counted, and marked with a note.
//!
//! It's a small reader for the RTF that Cocoa writes, not all of RTF:
//! groups, control words, `\'hh` and `\uN` characters, fields, and the
//! destinations to skip (font and colour tables, list tables, pictures…).

use std::collections::HashMap;

/// What came out of a document's RTF.
#[derive(Debug, Default, PartialEq)]
pub struct Converted {
    pub text: String,
    pub pictures: usize,
    pub tables: usize,
    pub footnotes: usize,
    pub comments: usize,
}

/// Scrivener's own bits that RTF doesn't carry: the document's comments
/// (by id, already as text) and its paragraph styles (by number, as names).
#[derive(Default)]
pub struct Context<'a> {
    pub comments: HashMap<String, String>,
    pub styles: &'a [String],
}

/// A run of text with one style.
#[derive(Clone, Copy, PartialEq, Default)]
struct Style {
    bold: bool,
    italic: bool,
}

#[derive(Clone)]
struct Group {
    style: Style,
    /// Skipping this group's text (a font table, a picture…).
    skip: bool,
    /// Characters to skip after a \uN.
    uc: usize,
    kind: Kind,
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Normal,
    /// A field's instruction (where a link's address is).
    FieldInstruction,
    /// A list item's number or bullet.
    ListText,
    Footnote,
}

pub fn to_markdown(rtf: &str, context: &Context) -> Converted {
    let mut reader = Reader::new(context);
    reader.read(rtf);
    reader.finish()
}

/// Just the text, no styles (for comments, synopses written as RTF…).
pub fn to_plain(rtf: &str) -> String {
    let context = Context::default();
    let converted = to_markdown(rtf, &context);
    converted.text.replace("**", "").replace('*', "")
}

struct Reader<'c> {
    context: &'c Context<'c>,
    stack: Vec<Group>,
    /// The paragraph being read, as styled runs.
    runs: Vec<(String, Style)>,
    paragraphs: Vec<String>,
    /// Bytes from \'hh, decoded together (cp1252).
    pending_bytes: Vec<u8>,
    /// A field: its instruction, and where its result started in `runs`.
    field_instruction: String,
    list_text: String,
    footnote_text: String,
    /// Cells in the table row being read, and whether the last row counted.
    row_cells: usize,
    in_table: bool,
    out: Converted,
}

impl<'c> Reader<'c> {
    fn new(context: &'c Context<'c>) -> Self {
        Self {
            context,
            stack: vec![Group { style: Style::default(), skip: false, uc: 1, kind: Kind::Normal }],
            runs: Vec::new(),
            paragraphs: Vec::new(),
            pending_bytes: Vec::new(),
            field_instruction: String::new(),
            list_text: String::new(),
            footnote_text: String::new(),
            row_cells: 0,
            in_table: false,
            out: Converted::default(),
        }
    }

    fn top(&mut self) -> &mut Group {
        self.stack.last_mut().expect("never empty")
    }

    fn read(&mut self, rtf: &str) {
        let bytes = rtf.as_bytes();
        let mut i = 0;
        // Characters still to skip after a \uN.
        let mut skip_chars = 0usize;
        while i < bytes.len() {
            match bytes[i] {
                b'{' => {
                    self.flush_bytes();
                    let group = self.top().clone();
                    self.stack.push(group);
                    i += 1;
                }
                b'}' => {
                    self.flush_bytes();
                    let closing = self.stack.pop();
                    if self.stack.is_empty() {
                        self.stack.push(closing.clone().expect("just popped"));
                    }
                    if let Some(group) = closing {
                        self.close_group(&group);
                    }
                    i += 1;
                }
                b'\\' => {
                    i += 1;
                    if i >= bytes.len() {
                        break;
                    }
                    let c = bytes[i];
                    if c.is_ascii_alphabetic() {
                        let start = i;
                        while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                            i += 1;
                        }
                        let word = &rtf[start..i];
                        let num_start = i;
                        if i < bytes.len() && (bytes[i] == b'-' || bytes[i].is_ascii_digit()) {
                            i += 1;
                            while i < bytes.len() && bytes[i].is_ascii_digit() {
                                i += 1;
                            }
                        }
                        let param = rtf[num_start..i].parse::<i64>().ok();
                        // One space after a control word belongs to it.
                        if i < bytes.len() && bytes[i] == b' ' {
                            i += 1;
                        }
                        if word == "u" {
                            if let Some(n) = param {
                                let n = if n < 0 { n + 65536 } else { n } as u32;
                                if let Some(ch) = char::from_u32(n) {
                                    self.flush_bytes();
                                    self.text(&ch.to_string());
                                }
                                skip_chars = self.top().uc;
                            }
                            continue;
                        }
                        self.flush_bytes();
                        self.control(word, param);
                    } else {
                        i += 1;
                        match c {
                            b'\'' => {
                                if i + 2 <= bytes.len()
                                    && let Ok(b) = u8::from_str_radix(&rtf[i..i + 2], 16)
                                {
                                    i += 2;
                                    if skip_chars > 0 {
                                        skip_chars -= 1;
                                    } else if !self.top().skip {
                                        self.pending_bytes.push(b);
                                    }
                                }
                            }
                            // A backslash before a line break: a new paragraph.
                            b'\n' | b'\r' => {
                                self.flush_bytes();
                                self.control("par", None);
                            }
                            b'*' => {
                                // An optional destination: skip what we don't know.
                                let rest = &rtf[i..];
                                let known = ["\\footnote", "\\fldinst", "\\shppict"].iter().any(|k| rest.starts_with(k));
                                if !known {
                                    self.top().skip = true;
                                }
                            }
                            b'~' => self.text("\u{a0}"),
                            b'-' => {}
                            b'_' => self.text("\u{2011}"),
                            other => {
                                self.flush_bytes();
                                self.text(&(other as char).to_string());
                            }
                        }
                    }
                }
                b'\n' | b'\r' => i += 1,
                _ => {
                    // Plain text up to the next special character.
                    let start = i;
                    while i < bytes.len() && !matches!(bytes[i], b'{' | b'}' | b'\\' | b'\n' | b'\r') {
                        i += 1;
                    }
                    let mut chunk = &rtf[start..i];
                    while skip_chars > 0 && !chunk.is_empty() {
                        let mut chars = chunk.chars();
                        chars.next();
                        chunk = chars.as_str();
                        skip_chars -= 1;
                    }
                    self.flush_bytes();
                    self.text(chunk);
                }
            }
        }
        self.flush_bytes();
    }

    fn control(&mut self, word: &str, param: Option<i64>) {
        let on = param != Some(0);
        match word {
            // Destinations whose text isn't the document's.
            // A picture (its stand-in copy, \nonshppict, isn't another one).
            "shppict" | "pict" => {
                if !self.top().skip {
                    self.out.pictures += 1;
                    self.runs.push(("<!-- a picture from Scrivener -->".into(), Style::default()));
                }
                self.top().skip = true;
            }
            "fonttbl" | "colortbl" | "expandedcolortbl" | "stylesheet" | "info" | "listtable" | "listoverridetable"
            | "nonshppict" | "header" | "footer" | "headerl" | "headerr" | "footerl" | "footerr" | "object" | "themedata"
            | "datastore" | "latentstyles" | "generator" | "xmlnstbl" | "rsidtbl" => {
                self.top().skip = true;
            }
            "fldinst" => {
                self.top().kind = Kind::FieldInstruction;
                self.field_instruction.clear();
            }
            "field" => {
                self.field_instruction.clear();
            }
            "listtext" => {
                self.top().kind = Kind::ListText;
                self.list_text.clear();
            }
            "footnote" => {
                self.top().kind = Kind::Footnote;
                self.footnote_text.clear();
                self.out.footnotes += 1;
            }
            "chftn" => {}
            "b" => self.top().style.bold = on,
            "i" => self.top().style.italic = on,
            "plain" => self.top().style = Style::default(),
            "uc" => self.top().uc = param.unwrap_or(1).max(0) as usize,
            "par" | "line" | "sect" | "page" => {
                if self.top().kind == Kind::Footnote {
                    self.footnote_text.push(' ');
                } else {
                    self.end_paragraph();
                    self.in_table = false;
                }
            }
            "tab" => self.text("\t"),
            "cell" | "nestcell" => {
                self.row_cells += 1;
                self.text(" | ");
            }
            "row" | "nestrow" => {
                // The last cell's " | " isn't wanted at the row's end.
                if let Some((last, _)) = self.runs.last_mut()
                    && last.ends_with(" | ")
                {
                    last.truncate(last.len() - 3);
                }
                // One cell a row is a box around text (Scrivener's notes
                // and tips), not a table.
                let table = self.row_cells >= 2;
                if table && !self.in_table {
                    self.out.tables += 1;
                }
                self.end_paragraph();
                self.in_table = table;
            }
            "trowd" => self.row_cells = 0,
            "emdash" => self.text("\u{2014}"),
            "endash" => self.text("\u{2013}"),
            "lquote" => self.text("\u{2018}"),
            "rquote" => self.text("\u{2019}"),
            "ldblquote" => self.text("\u{201c}"),
            "rdblquote" => self.text("\u{201d}"),
            "bullet" => self.text("\u{2022}"),
            _ => {}
        }
    }

    fn close_group(&mut self, group: &Group) {
        match group.kind {
            Kind::ListText if self.top().kind != Kind::ListText => {
                // "\t1.\t" → "1. ", a bullet → "- ".
                let marker = self.list_text.trim().to_owned();
                let marker = if marker.chars().next().is_some_and(|c| c.is_ascii_digit() || c.is_ascii_alphabetic()) {
                    format!("{marker} ")
                } else {
                    "- ".to_owned()
                };
                self.runs.push((marker, Style::default()));
            }
            Kind::Footnote if self.top().kind != Kind::Footnote => {
                let note = self.footnote_text.split_whitespace().collect::<Vec<_>>().join(" ");
                if !note.is_empty() {
                    self.runs.push((format!(" <!-- Footnote: {note} -->"), Style::default()));
                }
            }
            Kind::FieldInstruction if self.top().kind != Kind::FieldInstruction => {}
            _ => {}
        }
        // The end of a field: what its result was, and where it pointed.
        if group.kind != Kind::FieldInstruction && self.top().kind != Kind::FieldInstruction && !self.field_instruction.is_empty() {
            let instruction = std::mem::take(&mut self.field_instruction);
            self.after_field(&instruction);
        }
    }

    /// After a field's text: a comment goes in after it as a note; a web
    /// link becomes a Markdown link; links within the project are just text.
    fn after_field(&mut self, instruction: &str) {
        let Some(url) = instruction.split('"').nth(1) else { return };
        if let Some(id) = url.strip_prefix("scrivcmt://") {
            if let Some(comment) = self.context.comments.get(id) {
                if comment.starts_with("Footnote: ") {
                    self.out.footnotes += 1;
                } else {
                    self.out.comments += 1;
                }
                let comment = comment.split_whitespace().collect::<Vec<_>>().join(" ");
                self.runs.push((format!(" <!-- {comment} -->"), Style::default()));
            }
        } else if url.starts_with("http://") || url.starts_with("https://") || url.starts_with("mailto:") {
            self.runs.push((format!(" ({url})"), Style::default()));
        }
    }

    fn flush_bytes(&mut self) {
        if self.pending_bytes.is_empty() {
            return;
        }
        let bytes = std::mem::take(&mut self.pending_bytes);
        let (text, _, _) = encoding_rs::WINDOWS_1252.decode(&bytes);
        self.text(&text);
    }

    fn text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let group = self.top().clone();
        if group.skip {
            return;
        }
        match group.kind {
            Kind::FieldInstruction => self.field_instruction.push_str(text),
            Kind::ListText => self.list_text.push_str(text),
            Kind::Footnote => self.footnote_text.push_str(text),
            Kind::Normal => match self.runs.last_mut() {
                Some((last, style)) if *style == group.style => last.push_str(text),
                _ => self.runs.push((text.to_owned(), group.style)),
            },
        }
    }

    fn end_paragraph(&mut self) {
        let runs = std::mem::take(&mut self.runs);
        self.paragraphs.push(self.render(runs));
    }

    /// A paragraph's runs as Markdown, with Scrivener's style markers
    /// turned into headings and quotes.
    fn render(&self, runs: Vec<(String, Style)>) -> String {
        let plain: String = runs.iter().map(|(t, _)| t.as_str()).collect();
        let mut prefix = String::new();
        if let Some(level) = marker_number(&plain, "<$Scr_H::") {
            prefix = format!("{} ", "#".repeat(level.clamp(1, 6)));
        } else if let Some(n) = marker_number(&plain, "<$Scr_Ps::") {
            let name = self.context.styles.get(n).map(|s| s.to_lowercase()).unwrap_or_default();
            if name.contains("quote") {
                prefix = "> ".into();
            } else if name == "title" {
                prefix = "# ".into();
            } else if let Some(level) = name.strip_prefix("heading ").and_then(|l| l.trim().parse::<usize>().ok()) {
                prefix = format!("{} ", "#".repeat(level.clamp(1, 6)));
            }
        }
        let mut out = String::new();
        // Merge neighbours of the same style, then emphasis that hugs words.
        // A heading's bold is the heading's, not emphasis.
        let heading = prefix.starts_with('#');
        let mut merged: Vec<(String, Style)> = Vec::new();
        for (text, mut style) in runs {
            if heading {
                style.bold = false;
            }
            let text = strip_markers(&text);
            if text.is_empty() {
                continue;
            }
            match merged.last_mut() {
                Some((last, s)) if *s == style => last.push_str(&text),
                _ => merged.push((text, style)),
            }
        }
        for (text, style) in merged {
            let mark = match (style.bold, style.italic) {
                (true, true) => "***",
                (true, false) => "**",
                (false, true) => "*",
                (false, false) => "",
            };
            if mark.is_empty() || text.trim().is_empty() {
                out.push_str(&text);
                continue;
            }
            let lead = &text[..text.len() - text.trim_start().len()];
            let trail = &text[text.trim_end().len()..];
            out.push_str(&format!("{lead}{mark}{}{mark}{trail}", text.trim()));
        }
        let body = out.trim_matches(|c: char| c == ' ' || c == '\t').to_owned();
        if body.is_empty() { String::new() } else { format!("{prefix}{body}") }
    }

    fn finish(mut self) -> Converted {
        if !self.runs.is_empty() {
            self.end_paragraph();
        }
        // No more than one empty line in a row, none at the ends.
        let mut lines: Vec<String> = Vec::new();
        for p in self.paragraphs {
            if p.is_empty() && lines.last().is_none_or(|l: &String| l.is_empty()) {
                continue;
            }
            lines.push(p);
        }
        while lines.last().is_some_and(String::is_empty) {
            lines.pop();
        }
        let (text, footnotes, annotations) = inline_notes(&lines.join("\n"));
        self.out.text = text;
        self.out.footnotes += footnotes;
        self.out.comments += annotations;
        self.out
    }
}

/// Scrivener's inline footnotes and annotations, which it writes as text:
/// "{\Scrv_fn=…\end_Scrv_fn}" and "{\Scrv_annot \color={…} \text=…\end_Scrv_annot}".
/// They become notes. Returns the text and how many of each.
fn inline_notes(text: &str) -> (String, usize, usize) {
    let mut out = String::with_capacity(text.len());
    let (mut footnotes, mut annotations) = (0, 0);
    let mut rest = text;
    loop {
        let fn_at = rest.find("{\\Scrv_fn=");
        let an_at = rest.find("{\\Scrv_annot");
        let (at, footnote) = match (fn_at, an_at) {
            (Some(f), Some(a)) => if f < a { (f, true) } else { (a, false) },
            (Some(f), None) => (f, true),
            (None, Some(a)) => (a, false),
            (None, None) => break,
        };
        let (body_start, end_mark) = if footnote {
            (at + "{\\Scrv_fn=".len(), "\\end_Scrv_fn}")
        } else {
            match rest[at..].find("\\text=") {
                Some(t) => (at + t + "\\text=".len(), "\\end_Scrv_annot}"),
                None => break,
            }
        };
        let Some(end) = rest[body_start..].find(end_mark) else { break };
        let body = rest[body_start..body_start + end].split_whitespace().collect::<Vec<_>>().join(" ");
        out.push_str(rest[..at].trim_end_matches(' '));
        if footnote {
            footnotes += 1;
            out.push_str(&format!(" <!-- Footnote: {body} -->"));
        } else {
            annotations += 1;
            out.push_str(&format!(" <!-- {body} -->"));
        }
        rest = &rest[body_start + end + end_mark.len()..];
    }
    out.push_str(rest);
    // A note alone on its line: no leading space.
    let out = out.lines().map(|l| l.strip_prefix(" <!--").map_or(l.to_owned(), |r| format!("<!--{r}"))).collect::<Vec<_>>().join("\n");
    (out, footnotes, annotations)
}

/// The number in a marker such as "<$Scr_H::1>", if the text has one.
fn marker_number(text: &str, start: &str) -> Option<usize> {
    let at = text.find(start)? + start.len();
    let end = text[at..].find('>')? + at;
    text[at..end].parse().ok()
}

/// Scrivener's style markers ("<$Scr_Cs::0>", "<!$Scr_Ps::1>") left out.
fn strip_markers(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('<') {
        let tail = &rest[at..];
        let is_marker = tail.starts_with("<$Scr_") || tail.starts_with("<!$Scr_");
        match tail.find('>').filter(|_| is_marker) {
            Some(end) => {
                out.push_str(&rest[..at]);
                rest = &rest[at + end + 1..];
            }
            None => {
                out.push_str(&rest[..=at]);
                rest = &rest[at + 1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn md(rtf: &str) -> String {
        to_markdown(rtf, &Context::default()).text
    }

    const HEAD: &str = r"{\rtf1\ansi\ansicpg1252\cocoartf2819
\cocoatextscaling0\cocoaplatform0{\fonttbl\f0\froman\fcharset0 Palatino-Roman;\f1\fswiss\fcharset0 Helvetica;}
{\colortbl;\red255\green255\blue255;}
{\*\expandedcolortbl;;}
\pard\tx720\sl264\slmult1\sa360\pardirnatural\partightenfactor0

\f0\fs28 \cf0 ";

    #[test]
    fn paragraphs_and_emphasis() {
        let rtf = format!("{HEAD}The ferry had \\i gone\\i0 , and \\b she\\b0  was late.\\\nNext \\b\\i morning\\b0\\i0 .}}");
        assert_eq!(md(&rtf), "The ferry had *gone*, and **she** was late.\nNext ***morning***.");
    }

    #[test]
    fn characters_from_code_pages_and_unicode() {
        let rtf = format!("{HEAD}\\'93Caf\\'e9\\'94 \\'97 na\\uc0\\u239 ve \\uc1\\u8212 ?x}}");
        assert_eq!(md(&rtf), "\u{201c}Caf\u{e9}\u{201d} \u{2014} na\u{ef}ve \u{2014}x");
    }

    #[test]
    fn lists_keep_their_numbers_and_bullets() {
        let rtf = format!(
            "{HEAD}\\ls1\\ilvl0\\cf0 {{\\listtext\t1.\t}}Split the editor.\\\n{{\\listtext\t2.\t}}Click.\\\n{{\\listtext\t\\'95\t}}A bullet.}}"
        );
        assert_eq!(md(&rtf), "1. Split the editor.\n2. Click.\n- A bullet.");
    }

    #[test]
    fn links_comments_and_footnotes() {
        let mut comments = HashMap::new();
        comments.insert("C1".to_owned(), "It wasn't for me.".to_owned());
        let context = Context { comments, styles: &[] };
        let rtf = format!(
            "{HEAD}See {{\\field{{\\*\\fldinst{{HYPERLINK \"scrivlnk://ABC\"}}}}{{\\fldrslt the editor}}}} and {{\\field{{\\*\\fldinst{{HYPERLINK \"scrivcmt://C1\"}}}}{{\\fldrslt the message}}}}. Web: {{\\field{{\\*\\fldinst{{HYPERLINK \"https://example.com\"}}}}{{\\fldrslt here}}}}.{{\\super\\chftn}}{{\\footnote\\pard\\plain\\chftn A source.}}}}"
        );
        let out = to_markdown(&rtf, &context);
        assert_eq!(
            out.text,
            "See the editor and the message <!-- It wasn't for me. -->. Web: here (https://example.com). <!-- Footnote: A source. -->"
        );
        assert_eq!((out.comments, out.footnotes), (1, 1));
    }

    #[test]
    fn headings_and_quotes_from_scrivener_styles() {
        let styles = ["Heading 1".to_owned(), "Block Quote".to_owned()];
        let context = Context { comments: HashMap::new(), styles: &styles };
        let rtf = format!(
            "{HEAD}<$Scr_H::1><$Scr_Ps::0>\\b The Basics\\b0 <!$Scr_H::1><!$Scr_Ps::0>\\\n<$Scr_Ps::1>A quotation.<!$Scr_Ps::1>\\\nUse <$Scr_Cs::0>Navigate > Media<!$Scr_Cs::0> menu.}}"
        );
        assert_eq!(to_markdown(&rtf, &context).text, "# The Basics\n> A quotation.\nUse Navigate > Media menu.");
    }

    #[test]
    fn pictures_are_noted_and_tables_flattened() {
        let rtf = format!(
            "{HEAD}Before {{\\*\\shppict{{\\pict\\pngblip 89504e47}}}}{{\\nonshppict{{\\pict 0000}}}} after.\\\n\\itap1\\trowd\\cellx100 A\\cell B\\cell \\row\\trowd\\cellx100 C\\cell D\\cell \\row\\pard Done.}}"
        );
        let out = to_markdown(&rtf, &Context::default());
        assert_eq!(out.text, "Before <!-- a picture from Scrivener --> after.\nA | B\nC | D\nDone.");
        assert_eq!((out.pictures, out.tables), (1, 1));
    }

    #[test]
    fn scriveners_inline_footnotes_and_annotations_become_notes() {
        let rtf = format!(
            "{HEAD}A footnote after this.\n\\f1 \\{{\\\\Scrv_fn=\n\\f0  A real \\'93footnote\\'94.\n\\f1 \\\\end_Scrv_fn\\}}\n\\f0 \\\n\\{{\\\\Scrv_annot \\\\color=\\{{\\\\R=0.6\\\\G=0.04\\\\B=0.01\\}} \\\\text=\nAn annotation.\n\\\\end_Scrv_annot\\}}\\\nAfter.}}"
        );
        let out = to_markdown(&rtf, &Context::default());
        assert_eq!(out.text, "A footnote after this. <!-- Footnote: A real \u{201c}footnote\u{201d}. -->\n<!-- An annotation. -->\nAfter.");
        assert_eq!((out.footnotes, out.comments), (1, 1));
    }

    #[test]
    fn plain_text_for_comments() {
        let rtf = r"{\rtf1\ansi{\fonttbl\f0\fswiss Helvetica;}\f0\fs24 it was for my wife. Calling \i my\i0  phone!}";
        assert_eq!(to_plain(rtf), "it was for my wife. Calling my phone!");
    }
}
