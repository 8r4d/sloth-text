//! Importing a Scrivener 3 project (a `.scriv` folder) as a new Writer
//! project. The `.scriv` is only read, never changed.
//!
//! What a `.scriv` holds (Scrivener 3):
//! - `<name>.scrivx`: XML with the binder (titles, nesting, types, labels,
//!   statuses, keywords, include-in-compile), the label, status and keyword
//!   lists, and the project's targets;
//! - `Files/Data/<id>/`: each item's `content.rtf` (or `.pdf`, `.jpg`…),
//!   `notes.rtf`, `synopsis.txt`, `content.comments` (comments, as RTF in
//!   XML) and `content.styles` (the paragraph styles its text uses);
//! - `Files/styles.xml`: the project's styles, by id;
//! - `Snapshots/<id>.snapshots/`: each snapshot as RTF, with `index.xml`
//!   naming them.
//!
//! Where things go: the Draft becomes the Manuscript, Research and Trash
//! stay themselves, a top-level folder called Characters becomes the
//! Characters folder (and one called Places, Locations or Settings, a
//! Locations folder), and anything else at the top goes into Notes. Text
//! is converted by `rtf`. What can't come across (pictures, PDFs and other
//! files, tables…) is listed in an import report, a document in Notes.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::project::{CHARACTERS, Label, Meta, Place, Project, Role, profile_presets};
use crate::rtf::{self, Context};

/// What an import brought across, and what it couldn't.
#[derive(Debug, Default, PartialEq)]
pub struct Report {
    pub documents: usize,
    pub folders: usize,
    pub snapshots: usize,
    pub comments: usize,
    pub footnotes: usize,
    pub pictures: usize,
    pub tables: usize,
    /// Files that aren't text (PDFs, pictures, sound…): where they went.
    pub files: Vec<String>,
    /// Folders in Scrivener that had text of their own (Writer's folders
    /// don't): it's in a document at the folder's top.
    pub folder_texts: usize,
}

/// One item in Scrivener's binder.
struct Item {
    id: String,
    kind: String,
    title: String,
    label: Option<i64>,
    status: Option<i64>,
    keywords: Vec<i64>,
    include: bool,
    target: Option<u32>,
    children: Vec<Item>,
}

/// The project's lists, by Scrivener's ids.
#[derive(Default)]
struct Lists {
    labels: HashMap<i64, usize>,
    statuses: HashMap<i64, usize>,
    keywords: HashMap<i64, String>,
    styles: HashMap<String, String>,
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// Import the `.scriv` at `scriv` as a new project at `dest` (a new or
/// empty folder).
pub fn import(scriv: &Path, dest: &Path) -> io::Result<(Project, Report)> {
    let scrivx = fs::read_dir(scriv)?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|x| x == "scrivx"))
        .ok_or_else(|| invalid("That isn't a Scrivener project (it has no .scrivx file)."))?;
    if !scriv.join("Files/Data").is_dir() {
        return Err(invalid(
            "This looks like a Scrivener 2 project. Open it in Scrivener 3 once (it offers to update it), then import it.",
        ));
    }
    let xml = fs::read_to_string(&scrivx)?;
    let doc = roxmltree::Document::parse(&xml).map_err(|e| invalid(format!("The project file couldn't be read: {e}")))?;
    let root = doc.root_element();

    let title = scriv.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Imported".into());
    let mut project = Project::create_empty(dest, &title)?;
    let mut lists = Lists::default();

    // Labels and statuses replace Writer's starting ones.
    let mut labels = Vec::new();
    for label in descendants(root, "Label") {
        let Some(id) = label.attribute("ID").and_then(|i| i.parse::<i64>().ok()).filter(|&i| i >= 0) else { continue };
        let name = label.text().unwrap_or("").trim().to_owned();
        let color = label.attribute("Color").map_or(0, nearest_color);
        lists.labels.insert(id, labels.len());
        labels.push(Label { name, color });
    }
    project.manifest.labels = labels;
    let mut statuses = Vec::new();
    for status in descendants(root, "Status") {
        let Some(id) = status.attribute("ID").and_then(|i| i.parse::<i64>().ok()).filter(|&i| i >= 0) else { continue };
        lists.statuses.insert(id, statuses.len());
        statuses.push(status.text().unwrap_or("").trim().to_owned());
    }
    if !statuses.is_empty() {
        project.manifest.statuses = statuses;
    }
    if let Some(keywords) = root.children().find(|n| n.has_tag_name("Keywords")) {
        for keyword in keywords.descendants().filter(|n| n.has_tag_name("Keyword")) {
            if let (Some(id), Some(title)) = (keyword.attribute("ID").and_then(|i| i.parse().ok()), child_text(keyword, "Title")) {
                lists.keywords.insert(id, title);
            }
        }
    }
    if let Ok(styles) = fs::read_to_string(scriv.join("Files/styles.xml"))
        && let Ok(styles) = roxmltree::Document::parse(&styles)
    {
        for style in descendants(styles.root_element(), "Style") {
            if let (Some(id), Some(name)) = (style.attribute("ID"), style.attribute("Name")) {
                lists.styles.insert(id.to_owned(), name.to_owned());
            }
        }
    }

    // Targets: the draft's length, its deadline, words a session.
    if let Some(targets) = root.children().find(|n| n.has_tag_name("ProjectTargets")) {
        if let Some(draft) = targets.children().find(|n| n.has_tag_name("DraftTarget")) {
            project.manifest.goals.total = draft.text().and_then(|t| t.trim().parse().ok()).filter(|&n: &u32| n > 0);
            if draft.attribute("IgnoreDeadline") == Some("No") {
                project.manifest.goals.deadline = draft.attribute("Deadline").and_then(|d| d.get(..10)).map(str::to_owned);
            }
        }
        if let Some(session) = targets.children().find(|n| n.has_tag_name("SessionTarget")) {
            project.manifest.goals.daily = session.text().and_then(|t| t.trim().parse().ok()).filter(|&n: &u32| n > 0);
        }
    }

    let binder = root.children().find(|n| n.has_tag_name("Binder")).ok_or_else(|| invalid("The project has no binder."))?;
    let items: Vec<Item> = binder.children().filter(|n| n.has_tag_name("BinderItem")).map(read_item).collect();

    let mut importer = Importer { scriv, project, lists, report: Report::default() };
    for item in &items {
        let folder = |name: &str| item.title.trim().eq_ignore_ascii_case(name);
        match item.kind.as_str() {
            "DraftFolder" => importer.children_into(item, Role::Manuscript.folder())?,
            "ResearchFolder" => importer.children_into(item, Role::Research.folder())?,
            "TrashFolder" => importer.children_into(item, Role::Trash.folder())?,
            "Folder" if folder("Characters") && !importer.project.manifest.no_characters => {
                importer.children_into(item, CHARACTERS)?
            }
            "Folder" if folder("Places") || folder("Locations") || folder("Settings") => {
                let id = importer.locations_folder()?;
                importer.children_into(item, &id)?;
            }
            // Collections, searches and the like aren't binder items.
            _ => importer.item(item, Role::Notes.folder())?,
        }
    }
    importer.write_report(&title)?;
    importer.project.save()?;
    Ok((importer.project, importer.report))
}

fn descendants<'a, 'i>(node: roxmltree::Node<'a, 'i>, tag: &'static str) -> impl Iterator<Item = roxmltree::Node<'a, 'i>> {
    node.descendants().filter(move |n| n.has_tag_name(tag))
}

fn child_text(node: roxmltree::Node, tag: &str) -> Option<String> {
    node.children().find(|n| n.has_tag_name(tag)).and_then(|n| n.text()).map(|t| t.trim().to_owned())
}

fn read_item(node: roxmltree::Node) -> Item {
    let meta = node.children().find(|n| n.has_tag_name("MetaData"));
    let meta_text = |tag: &str| meta.and_then(|m| child_text(m, tag));
    let number = |tag: &str| meta_text(tag).and_then(|t| t.parse::<i64>().ok()).filter(|&n| n >= 0);
    let keywords = node
        .children()
        .find(|n| n.has_tag_name("Keywords"))
        .map(|k| k.children().filter(|n| n.has_tag_name("KeywordID")).filter_map(|n| n.text()?.trim().parse().ok()).collect())
        .unwrap_or_default();
    let children = node
        .children()
        .find(|n| n.has_tag_name("Children"))
        .map(|c| c.children().filter(|n| n.has_tag_name("BinderItem")).map(read_item).collect())
        .unwrap_or_default();
    Item {
        id: node.attribute("UUID").unwrap_or("").to_owned(),
        kind: node.attribute("Type").unwrap_or("Text").to_owned(),
        title: child_text(node, "Title").filter(|t| !t.is_empty()).unwrap_or_else(|| "Untitled".into()),
        label: number("LabelID"),
        status: number("StatusID"),
        keywords,
        include: meta_text("IncludeInCompile").is_none_or(|v| v != "No"),
        target: meta_text("Target").and_then(|t| t.parse().ok()).filter(|&n| n > 0),
        children,
    }
}

/// The nearest of Writer's label colours to Scrivener's "r g b" (0–1),
/// by hue: Scrivener's are pale, Writer's strong.
fn nearest_color(rgb: &str) -> u8 {
    let parts: Vec<f32> = rgb.split_whitespace().filter_map(|p| p.parse().ok()).collect();
    let [r, g, b] = parts[..] else { return 0 };
    let hue = |r: f32, g: f32, b: f32| {
        let (max, min) = (r.max(g).max(b), r.min(g).min(b));
        if max - min < 1e-4 {
            return 0.0;
        }
        let h = if max == r {
            ((g - b) / (max - min)).rem_euclid(6.0)
        } else if max == g {
            (b - r) / (max - min) + 2.0
        } else {
            (r - g) / (max - min) + 4.0
        };
        h * 60.0
    };
    let wanted = hue(r, g, b);
    let distance = |h: f32| {
        let d = (h - wanted).abs();
        d.min(360.0 - d)
    };
    crate::app::inspector::LABEL_COLORS
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            let ha = hue(a.r() as f32 / 255.0, a.g() as f32 / 255.0, a.b() as f32 / 255.0);
            let hb = hue(b.r() as f32 / 255.0, b.g() as f32 / 255.0, b.b() as f32 / 255.0);
            distance(ha).total_cmp(&distance(hb))
        })
        .map_or(0, |(i, _)| i as u8)
}

struct Importer<'a> {
    scriv: &'a Path,
    project: Project,
    lists: Lists,
    report: Report,
}

impl Importer<'_> {
    fn data(&self, id: &str) -> PathBuf {
        self.scriv.join("Files/Data").join(id)
    }

    fn children_into(&mut self, item: &Item, parent: &str) -> io::Result<()> {
        for child in &item.children {
            self.item(child, parent)?;
        }
        Ok(())
    }

    /// A Locations profile folder, made if the project hasn't one.
    fn locations_folder(&mut self) -> io::Result<String> {
        if self.project.profile_kind("locations").is_some() {
            return Ok("locations".into());
        }
        let preset = profile_presets().into_iter().map(|(k, _)| k).find(|k| k.id == "locations").expect("a preset");
        self.project.add_profile_folder(preset)
    }

    /// An item and everything in it, into `parent`.
    fn item(&mut self, item: &Item, parent: &str) -> io::Result<()> {
        let place = Place::Into(parent.to_owned());
        let dir = self.data(&item.id);
        let content = fs::read_dir(&dir)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .map(|e| e.path())
            .find(|p| p.file_stem().is_some_and(|s| s == "content") && !matches!(p.extension().and_then(|x| x.to_str()), Some("styles" | "comments")));
        let is_text = content.as_ref().is_none_or(|p| p.extension().is_some_and(|x| x == "rtf"));
        let text = if is_text { self.text(&item.id)? } else { String::new() };

        let id = if item.kind == "Folder" {
            let id = self.project.add_folder(&place, &item.title)?;
            self.report.folders += 1;
            // Writer's folders have no text: Scrivener's goes in a document at its top.
            if !text.trim().is_empty() {
                let inner = self.project.add_document(&Place::Into(id.clone()), &item.title)?;
                self.write(&inner, &text)?;
                self.report.folder_texts += 1;
                self.report.documents += 1;
            }
            id
        } else {
            let id = self.project.add_document(&place, &item.title)?;
            self.report.documents += 1;
            match &content {
                Some(file) if !is_text => {
                    let note = self.copy_file(file, &item.title)?;
                    self.write(&id, &note)?;
                }
                _ => self.write(&id, &text)?,
            }
            self.snapshots(&item.id, &id)?;
            id
        };
        self.project.set_meta(&id, self.meta(item));
        for child in &item.children {
            self.item(child, &id)?;
        }
        Ok(())
    }

    /// A document's text as Markdown, with its comments in it.
    fn text(&mut self, scrivener_id: &str) -> io::Result<String> {
        let dir = self.data(scrivener_id);
        let Ok(rtf) = fs::read_to_string(dir.join("content.rtf")) else { return Ok(String::new()) };
        let comments = self.comments(&dir);
        let styles: Vec<String> = fs::read_to_string(dir.join("content.styles"))
            .unwrap_or_default()
            .split(',')
            .map(|id| self.lists.styles.get(id.trim()).cloned().unwrap_or_default())
            .collect();
        let context = Context { comments: comments.clone(), styles: &styles };
        let converted = rtf::to_markdown(&rtf, &context);
        self.report.pictures += converted.pictures;
        self.report.tables += converted.tables;
        self.report.footnotes += converted.footnotes;
        self.report.comments += converted.comments;
        let mut text = converted.text;
        // Comments not anchored in the text (their text was deleted): at the end.
        for (id, comment) in &comments {
            if !rtf.contains(&format!("scrivcmt://{id}")) {
                text.push_str(&format!("\n<!-- {} -->", comment.trim()));
                self.report.comments += 1;
            }
        }
        Ok(text)
    }

    /// A document's comments, by id, as text.
    fn comments(&self, dir: &Path) -> HashMap<String, String> {
        let Ok(xml) = fs::read_to_string(dir.join("content.comments")) else { return HashMap::new() };
        let Ok(doc) = roxmltree::Document::parse(&xml) else { return HashMap::new() };
        descendants(doc.root_element(), "Comment")
            .filter_map(|c| {
                let text = rtf::to_plain(c.text().unwrap_or(""));
                // Scrivener's footnotes in the inspector are comments marked as such.
                let text = if c.attribute("Footnote") == Some("Yes") { format!("Footnote: {text}") } else { text };
                Some((c.attribute("ID")?.to_owned(), text))
            })
            .collect()
    }

    fn write(&mut self, id: &str, text: &str) -> io::Result<()> {
        let path = self.project.file_path(id).ok_or_else(|| invalid("A new document has no file."))?;
        sloth_core::document::write_atomically(&path, text.as_bytes())
    }

    /// A PDF, picture or sound file: copied into the project's "Research
    /// Files" folder; the document says where it is.
    fn copy_file(&mut self, file: &Path, title: &str) -> io::Result<String> {
        let folder = self.project.root.join("Research Files");
        fs::create_dir_all(&folder)?;
        let ext = file.extension().map(|x| x.to_string_lossy().into_owned()).unwrap_or_default();
        let safe: String = title.chars().map(|c| if matches!(c, '/' | ':' | '\\') { '-' } else { c }).collect();
        let name = (1..)
            .map(|n| if n == 1 { format!("{safe}.{ext}") } else { format!("{safe} ({n}).{ext}") })
            .find(|n| !folder.join(n).exists())
            .expect("a free name");
        fs::copy(file, folder.join(&name))?;
        let kind = match ext.to_lowercase().as_str() {
            "pdf" => "a PDF",
            "jpg" | "jpeg" | "png" | "gif" | "tiff" | "tif" | "heic" => "a picture",
            "wav" | "mp3" | "m4a" | "aiff" | "aif" => "a sound file",
            "mov" | "mp4" | "m4v" => "a video",
            "webarchive" | "html" => "a web page",
            _ => "a file",
        };
        self.report.files.push(format!("Research Files/{name}"));
        Ok(format!(
            "<!-- This was {kind} in Scrivener. It's in the project folder, as \"Research Files/{name}\"; Writer can't show it yet. -->"
        ))
    }

    /// Snapshots, with the dates and names Scrivener gave them.
    fn snapshots(&mut self, scrivener_id: &str, id: &str) -> io::Result<()> {
        let dir = self.scriv.join("Snapshots").join(format!("{scrivener_id}.snapshots"));
        let Ok(entries) = fs::read_dir(&dir) else { return Ok(()) };
        // index.xml: each snapshot's title, by its date.
        let mut titles: BTreeMap<String, String> = BTreeMap::new();
        if let Ok(xml) = fs::read_to_string(dir.join("index.xml"))
            && let Ok(index) = roxmltree::Document::parse(&xml)
        {
            for snapshot in descendants(index.root_element(), "Snapshot") {
                if let (Some(date), Some(title)) = (child_text(snapshot, "Date"), child_text(snapshot, "Title")) {
                    titles.insert(snapshot_stem(&date), title);
                }
            }
        }
        let root = self.project.root.clone();
        for file in entries.filter_map(Result::ok).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "rtf")) {
            let stem = file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            // "2006-08-23-19-33-44+0100": the time, ignoring the zone.
            let Ok(taken) = chrono::NaiveDateTime::parse_from_str(stem.get(..19).unwrap_or(""), "%Y-%m-%d-%H-%M-%S") else { continue };
            let rtf = fs::read_to_string(&file)?;
            let text = rtf::to_markdown(&rtf, &Context::default()).text;
            let name = titles.get(&stem).cloned().unwrap_or_default();
            crate::snapshots::take_at(&root, id, &text, &name, taken)?;
            self.report.snapshots += 1;
        }
        Ok(())
    }

    fn meta(&self, item: &Item) -> Meta {
        let dir = self.data(&item.id);
        let synopsis = fs::read_to_string(dir.join("synopsis.txt")).unwrap_or_default().trim().to_owned();
        let notes = fs::read_to_string(dir.join("notes.rtf"))
            .map(|rtf| rtf::to_markdown(&rtf, &Context::default()).text)
            .unwrap_or_default();
        Meta {
            synopsis,
            notes,
            status: item.status.and_then(|s| self.lists.statuses.get(&s).copied()),
            label: item.label.and_then(|l| self.lists.labels.get(&l).copied()),
            keywords: item.keywords.iter().filter_map(|k| self.lists.keywords.get(k).cloned()).collect(),
            exclude_from_compile: !item.include,
            target_words: item.target,
            ..Meta::default()
        }
    }

    /// "Imported from Scrivener", in Notes: what came across, and what didn't.
    fn write_report(&mut self, title: &str) -> io::Result<()> {
        let r = &self.report;
        let mut lines = vec![
            "# Imported from Scrivener".to_owned(),
            format!("\u{201c}{title}\u{201d} was imported on {}.", chrono::Local::now().format("%-d %B %Y at %H:%M")),
            String::new(),
            format!("{} documents and {} folders came across, with their synopses, notes, labels, statuses, keywords and targets.", r.documents, r.folders),
        ];
        let mut said = |line: String| lines.push(format!("- {line}"));
        let n = |count: usize, one: &str, many: &str| format!("{count} {}", if count == 1 { one } else { many });
        if r.snapshots > 0 {
            said(n(r.snapshots, "snapshot, with its date and name.", "snapshots, with their dates and names."));
        }
        if r.comments > 0 {
            said(format!("{} now notes in the text, like <!-- this -->.", n(r.comments, "comment or annotation is", "comments and annotations are")));
        }
        if r.footnotes > 0 {
            said(format!("{} now notes in the text, starting \u{201c}Footnote:\u{201d}. Writer has no footnotes yet.", n(r.footnotes, "footnote is", "footnotes are")));
        }
        if r.folder_texts > 0 {
            said(format!("{} text of its own: it's in a document of the same name, first in the folder.", n(r.folder_texts, "folder had", "folders had")));
        }
        if r.pictures > 0 {
            said(format!("{} in the text couldn't come across. Each one's place is marked with a note.", n(r.pictures, "picture", "pictures")));
        }
        if r.tables > 0 {
            said(format!("{} now lines with their cells separated by \u{201c} | \u{201d}.", n(r.tables, "table is", "tables are")));
        }
        if !r.files.is_empty() {
            said(format!(
                "{} (PDFs, pictures, sound…) {} copied into the project's \u{201c}Research Files\u{201d} folder; {} where. Writer can't show {} yet:",
                n(r.files.len(), "file that isn't text", "files that aren't text"),
                if r.files.len() == 1 { "was" } else { "were" },
                if r.files.len() == 1 { "its document says" } else { "their documents say" },
                if r.files.len() == 1 { "it" } else { "them" }
            ));
            for file in &r.files {
                lines.push(format!("    - {file}"));
            }
        }
        lines.push(String::new());
        lines.push(
            "Not brought across: compile settings, collections, custom metadata, links between documents (their text is kept), and formatting beyond bold, italic, headings and quotes."
                .into(),
        );
        lines.push("The Scrivener project itself wasn't changed.".into());
        let id = self.project.add_document(&Place::Into(Role::Notes.folder().into()), "Imported from Scrivener")?;
        self.write(&id, &lines.join("\n"))
    }
}

/// A snapshot's date as index.xml has it ("2006-08-23 19:33:44 +0100") in
/// the form its file is named ("2006-08-23-19-33-44+0100").
fn snapshot_stem(date: &str) -> String {
    date.replacen(' ', "-", 1).replace(':', "-").replace(" +", "+").replace(" -", "-")
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    struct TempDir(PathBuf);
    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("sloth-writer-scriv-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn rtf(body: &str) -> String {
        format!("{{\\rtf1\\ansi\\ansicpg1252\\cocoartf2819{{\\fonttbl\\f0\\froman Palatino;}}\\pard\\f0\\fs28 {body}}}")
    }

    /// A small Scrivener 3 project, as Scrivener lays it out.
    pub(crate) fn make_scriv(dir: &Path) -> PathBuf {
        let scriv = dir.join("The Long Winter.scriv");
        let item = |id: &str, kind: &str, title: &str, meta: &str, children: &str| {
            let children = if children.is_empty() { String::new() } else { format!("<Children>{children}</Children>") };
            format!(r#"<BinderItem UUID="{id}" Type="{kind}"><Title>{title}</Title><MetaData>{meta}</MetaData>{children}</BinderItem>"#)
        };
        let scene1 = item(
            "S1",
            "Text",
            "Arrival",
            "<IncludeInCompile>Yes</IncludeInCompile><LabelID>8</LabelID><StatusID>5</StatusID>",
            "",
        )
        .replace("</MetaData>", "</MetaData><Keywords><KeywordID>2</KeywordID></Keywords>");
        let scene2 = item("S2", "Text", "Cut", "<IncludeInCompile>No</IncludeInCompile>", "");
        let chapter = item("C1", "Folder", "Chapter One", "", &(scene1 + &scene2));
        let draft = item("D", "DraftFolder", "Draft", "", &chapter);
        let mara = item("M", "Text", "Mara", "", "");
        let characters = item("CH", "Folder", "Characters", "", &mara);
        let harbour = item("H", "Text", "The Harbour", "", "");
        let places = item("PL", "Folder", "Places", "", &harbour);
        let map = item("P", "PDF", "Ferry Timetable", "", "");
        let research = item("R", "ResearchFolder", "Research", "", &map);
        let trash = item("T", "TrashFolder", "Trash", "", "");
        let loose = item("L", "Text", "Ideas", "", "");
        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ScrivenerProject Version="2.0"><Binder>{draft}{characters}{places}{research}{trash}{loose}</Binder>
<LabelSettings><Labels><Label ID="-1">No Label</Label><Label ID="8" Color="0.99 0.70 0.73">Main plot</Label><Label ID="12" Color="0.70 0.88 0.97">Subplot</Label></Labels></LabelSettings>
<StatusSettings><StatusItems><Status ID="-1">No Status</Status><Status ID="4">To Do</Status><Status ID="5">First Draft</Status></StatusItems></StatusSettings>
<Keywords><Keyword ID="1"><Title>People</Title><Children><Keyword ID="2"><Title>Mara</Title></Keyword></Children></Keyword></Keywords>
<ProjectTargets><DraftTarget Type="Words" Deadline="2027-03-01 12:00:00 -0600" IgnoreDeadline="No">90000</DraftTarget><SessionTarget Type="Words">1000</SessionTarget></ProjectTargets>
</ScrivenerProject>"#
        );
        fs::create_dir_all(scriv.join("Files/Data")).unwrap();
        fs::write(scriv.join("The Long Winter.scrivx"), xml).unwrap();
        fs::write(scriv.join("Files/styles.xml"), r#"<Styles><Style Name="Block Quote" ID="Q"/></Styles>"#).unwrap();
        let data = |id: &str, file: &str, contents: &str| {
            let dir = scriv.join("Files/Data").join(id);
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join(file), contents).unwrap();
        };
        data(
            "S1",
            "content.rtf",
            &rtf(r#"Mara was \i late\i0 . {\field{\*\fldinst{HYPERLINK "scrivcmt://K1"}}{\fldrslt The ferry}} had gone.\
<$Scr_Ps::0>A quotation.<!$Scr_Ps::0>"#),
        );
        data("S1", "content.styles", "Q");
        data("S1", "content.comments", r#"<Comments><Comment ID="K1"><![CDATA[{\rtf1\ansi Check the times.}]]></Comment></Comments>"#);
        data("S1", "synopsis.txt", "She arrives too late.\n");
        data("S1", "notes.rtf", &rtf("Rain, not snow?"));
        data("S2", "content.rtf", &rtf("An old opening."));
        data("C1", "content.rtf", &rtf("Words in the chapter folder itself."));
        data("M", "content.rtf", &rtf("Tall, impatient."));
        data("H", "content.rtf", &rtf("Grey stone."));
        data("L", "content.rtf", &rtf("A loose idea."));
        data("P", "content.pdf", "%PDF-1.4 pretend");
        let snaps = scriv.join("Snapshots/S1.snapshots");
        fs::create_dir_all(&snaps).unwrap();
        fs::write(snaps.join("2026-10-01-09-30-00-0600.rtf"), rtf("Mara was early.")).unwrap();
        fs::write(
            snaps.join("index.xml"),
            "<Snapshots><Snapshot><Title>Before the rewrite</Title><Date>2026-10-01 09:30:00 -0600</Date></Snapshot></Snapshots>",
        )
        .unwrap();
        scriv
    }

    #[test]
    fn a_scrivener_project_comes_across() {
        let tmp = TempDir::new("import");
        let scriv = make_scriv(&tmp.0);
        let before = fs::read_to_string(scriv.join("The Long Winter.scrivx")).unwrap();
        let (p, report) = import(&scriv, &tmp.0.join("The Long Winter")).unwrap();
        assert_eq!(fs::read_to_string(scriv.join("The Long Winter.scrivx")).unwrap(), before, "the .scriv is only read");
        let read = |id: &str| fs::read_to_string(p.file_path(id).unwrap()).unwrap();
        let find = |title: &str| p.documents().into_iter().chain(p.manifest.binder.iter().flat_map(|n| n.subtree())).find(|n| n.title == title).unwrap().clone();

        // The Draft is the Manuscript, chapters and all.
        let manuscript = &p.manifest.binder.iter().find(|n| n.role == Some(Role::Manuscript)).unwrap().children;
        assert_eq!(manuscript.iter().map(|n| n.title.as_str()).collect::<Vec<_>>(), ["Chapter One"]);
        let chapter = &manuscript[0];
        assert_eq!(chapter.children.iter().map(|n| n.title.as_str()).collect::<Vec<_>>(), ["Chapter One", "Arrival", "Cut"]);
        assert_eq!(read(&chapter.children[0].id), "Words in the chapter folder itself.", "the folder's own text");

        // Text, comments and styles.
        let arrival = find("Arrival");
        assert_eq!(read(&arrival.id), "Mara was *late*. The ferry <!-- Check the times. --> had gone.\n> A quotation.");
        // Its metadata.
        assert_eq!(arrival.meta.synopsis, "She arrives too late.");
        assert_eq!(arrival.meta.notes, "Rain, not snow?");
        assert_eq!(p.manifest.statuses, ["To Do", "First Draft"]);
        assert_eq!(arrival.meta.status, Some(1));
        assert_eq!(p.manifest.labels.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(), ["Main plot", "Subplot"]);
        assert_eq!(arrival.meta.label, Some(0));
        assert_eq!(p.manifest.labels[0].color, 0, "pink-red is red");
        assert_eq!(p.manifest.labels[1].color, 5, "pale blue is blue");
        assert_eq!(arrival.meta.keywords, ["Mara"]);
        assert!(!arrival.meta.exclude_from_compile);
        assert!(find("Cut").meta.exclude_from_compile);
        // Its snapshot, with its date and name.
        let snaps = crate::snapshots::list(&p.root, &arrival.id);
        assert_eq!(snaps.len(), 1);
        assert_eq!(snaps[0].name, "Before the rewrite");
        assert_eq!(snaps[0].taken.format("%Y-%m-%d %H:%M").to_string(), "2026-10-01 09:30");
        assert_eq!(snaps[0].text().unwrap(), "Mara was early.");

        // Characters and Places are profile folders; the rest has a home.
        assert_eq!(p.role_of(&find("Mara").id), Some(Role::Characters));
        assert_eq!(p.profile_of(&find("The Harbour").id).map(|k| k.id.as_str()), Some("locations"));
        assert_eq!(p.role_of(&find("Ideas").id), Some(Role::Notes));
        // A PDF: copied, and its document says where.
        let timetable = find("Ferry Timetable");
        assert_eq!(p.role_of(&timetable.id), Some(Role::Research));
        assert!(read(&timetable.id).contains("Research Files/Ferry Timetable.pdf"));
        assert_eq!(fs::read_to_string(p.root.join("Research Files/Ferry Timetable.pdf")).unwrap(), "%PDF-1.4 pretend");

        // Goals.
        assert_eq!(p.manifest.goals.total, Some(90_000));
        assert_eq!(p.manifest.goals.daily, Some(1000));
        assert_eq!(p.manifest.goals.deadline.as_deref(), Some("2027-03-01"));

        // The report.
        assert_eq!((report.documents, report.comments, report.snapshots, report.folder_texts), (7, 1, 1, 1));
        let report_doc = find("Imported from Scrivener");
        assert!(read(&report_doc.id).contains("Research Files/Ferry Timetable.pdf"));
        // And it opens as any project does.
        assert_eq!(Project::open(&p.root).unwrap().manifest, p.manifest);
    }

    #[test]
    fn not_a_scrivener_project_or_too_old() {
        let tmp = TempDir::new("not-scriv");
        fs::create_dir_all(tmp.0.join("x.scriv")).unwrap();
        let error = |out: &str| import(&tmp.0.join("x.scriv"), &tmp.0.join(out)).err().expect("refused").to_string();
        assert!(error("out1").contains("no .scrivx"));
        fs::write(tmp.0.join("x.scriv/x.scrivx"), "<ScrivenerProject/>").unwrap();
        assert!(error("out2").contains("Scrivener 2"));
    }

    #[test]
    fn snapshot_dates_match_their_files() {
        assert_eq!(snapshot_stem("2006-08-23 19:33:44 +0100"), "2006-08-23-19-33-44+0100");
        assert_eq!(snapshot_stem("2026-10-01 09:30:00 -0600"), "2026-10-01-09-30-00-0600");
    }

    /// Scrivener's own tutorial project, if there's a copy beside the code
    /// (it isn't ours to keep in the repository): `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn scriveners_tutorial_imports_cleanly() {
        let scriv = Path::new(env!("CARGO_MANIFEST_DIR")).join("../Tutorial.scriv");
        if !scriv.is_dir() {
            eprintln!("No Tutorial.scriv beside the code; skipped.");
            return;
        }
        let tmp = TempDir::new("tutorial");
        let (p, report) = import(&scriv, &tmp.0.join("Tutorial")).unwrap();
        assert!(report.documents > 70 && report.folders > 10, "{report:?}");
        assert_eq!(report.files.len(), 3);
        // Nothing of RTF or Scrivener's markers left in any document.
        for node in p.manifest.binder.iter().flat_map(|n| n.subtree()).filter(|n| n.is_document()) {
            let text = fs::read_to_string(p.file_path(&node.id).unwrap()).unwrap();
            for line in text.lines() {
                assert!(!line.contains('\\') && !line.contains("Scr_") && !line.contains("{\\"), "{}: {line}", node.title);
            }
        }
    }
}
