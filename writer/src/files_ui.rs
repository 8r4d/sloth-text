//! Research files: PDFs, pictures and anything else kept in the project's
//! Research Files folder, each an item in the binder (see `Node::file`).
//!
//! Selecting one shows it where the editor is: a picture fitted to the
//! space, a PDF's pages one under another (drawn by hayro on a thread of
//! its own, just the pages in view), anything else with a button to open
//! it in its own app. The file stays on each computer: on another, until
//! it's copied there, the item says so and offers to find it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};

use eframe::egui::{self, Color32, RichText};

use super::WriterApp;
use crate::project::Place;

/// What kind of file it is, by its extension.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileKind {
    Picture,
    Pdf,
    Other,
}

pub fn kind_of(path: &str) -> FileKind {
    let ext = Path::new(path).extension().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "tif" | "tiff" => FileKind::Picture,
        "pdf" => FileKind::Pdf,
        _ => FileKind::Other,
    }
}

/// The binder's icon for a research file.
pub fn icon(path: &str) -> &'static str {
    match kind_of(path) {
        FileKind::Picture => crate::icons::PICTURE,
        _ => crate::icons::ATTACHMENT,
    }
}

/// A file being shown, kept while it's open so it isn't read again.
pub(super) struct Shown {
    path: PathBuf,
    /// When the file was last changed, to notice a new copy.
    modified: Option<std::time::SystemTime>,
    content: Content,
}

enum Content {
    Missing,
    /// Couldn't be read or shown: why.
    Failed(String),
    Picture(egui::TextureHandle),
    Pdf(PdfView),
    Other,
}

/// A PDF: its pages' sizes, the pages drawn so far, and the thread
/// drawing them.
struct PdfView {
    /// Each page's size in points.
    sizes: Vec<egui::Vec2>,
    /// Drawn pages: the texture and the width in pixels it was drawn at.
    pages: HashMap<usize, (egui::TextureHandle, u32)>,
    /// Pages asked for and not back yet, with the width asked for.
    asked: HashMap<usize, u32>,
    to_drawer: Sender<(usize, u32)>,
    from_drawer: Receiver<Drawn>,
}

enum Drawn {
    /// The pages' sizes, once the file's read.
    Sizes(Vec<egui::Vec2>),
    Page { index: usize, width: u32, image: egui::ColorImage },
    Failed(String),
}

impl PdfView {
    fn open(data: Vec<u8>, ctx: egui::Context) -> PdfView {
        let (to_drawer, requests) = channel::<(usize, u32)>();
        let (results, from_drawer) = channel::<Drawn>();
        std::thread::spawn(move || draw_pages(data, requests, results, ctx));
        PdfView { sizes: Vec::new(), pages: HashMap::new(), asked: HashMap::new(), to_drawer, from_drawer }
    }
}

/// The PDF drawer's thread: read the file, say how big its pages are,
/// then draw each page asked for at the width asked for.
fn draw_pages(data: Vec<u8>, requests: Receiver<(usize, u32)>, results: Sender<Drawn>, ctx: egui::Context) {
    use hayro::hayro_interpret::InterpreterSettings;
    use hayro::hayro_syntax::Pdf;
    use hayro::vello_cpu::color::palette::css::WHITE;
    use hayro::{PixmapSettings, RenderCache, RenderSettings};

    let pdf = match Pdf::new(data) {
        Ok(pdf) => pdf,
        Err(e) => {
            let _ = results.send(Drawn::Failed(format!("It couldn't be read as a PDF ({e:?}).")));
            ctx.request_repaint();
            return;
        }
    };
    let pages = pdf.pages();
    let sizes = pages
        .iter()
        .map(|p| {
            let (w, h) = p.render_dimensions();
            egui::vec2(w, h)
        })
        .collect();
    if results.send(Drawn::Sizes(sizes)).is_err() {
        return;
    }
    ctx.request_repaint();
    let cache = RenderCache::new();
    let settings = InterpreterSettings::default();
    let render = RenderSettings::default();
    while let Ok((mut index, mut width)) = requests.recv() {
        // Only the latest request for each page matters; the newest first.
        let mut latest: Vec<(usize, u32)> = vec![(index, width)];
        while let Ok(more) = requests.try_recv() {
            latest.retain(|(i, _)| *i != more.0);
            latest.push(more);
        }
        while let Some(next) = latest.pop() {
            (index, width) = next;
            let Some(page) = pages.get(index) else { continue };
            let (w, _) = page.render_dimensions();
            let scale = width as f32 / w.max(1.0);
            let pixmap = hayro::render(page, &cache, &settings, &render, &PixmapSettings { x_scale: scale, y_scale: scale, bg_color: WHITE });
            let size = [pixmap.width() as usize, pixmap.height() as usize];
            let image = egui::ColorImage::from_rgba_premultiplied(size, pixmap.data_as_u8_slice());
            if results.send(Drawn::Page { index, width, image }).is_err() {
                return;
            }
            ctx.request_repaint();
        }
    }
}

impl Shown {
    fn load(path: &Path, ctx: &egui::Context) -> Shown {
        let modified = std::fs::metadata(path).and_then(|m| m.modified()).ok();
        let content = if !path.is_file() {
            Content::Missing
        } else {
            match kind_of(&path.to_string_lossy()) {
                FileKind::Picture => match picture(path) {
                    Ok(image) => Content::Picture(ctx.load_texture(path.to_string_lossy(), image, egui::TextureOptions::LINEAR)),
                    Err(e) => Content::Failed(e),
                },
                FileKind::Pdf => match std::fs::read(path) {
                    Ok(data) => Content::Pdf(PdfView::open(data, ctx.clone())),
                    Err(e) => Content::Failed(e.to_string()),
                },
                FileKind::Other => Content::Other,
            }
        };
        Shown { path: path.to_owned(), modified, content }
    }
}

/// A picture file as an image egui can show.
fn picture(path: &Path) -> Result<egui::ColorImage, String> {
    let image = image::ImageReader::open(path)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .decode()
        .map_err(|e| format!("Writer can't show this picture ({e})."))?;
    // No bigger than a graphics card's textures go (a camera's photo can
    // be), and still more than any screen shows.
    let image = if image.width().max(image.height()) > LARGEST { image.resize(LARGEST, LARGEST, image::imageops::FilterType::Triangle) } else { image };
    let image = image.to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    Ok(egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw()))
}

/// The longest side a picture is shown at, in pixels.
const LARGEST: u32 = 4096;

/// How much room is left for a page, side to side.
const MARGIN: f32 = 24.0;
/// The widest a PDF's page is shown, in points.
const MAX_PAGE: f32 = 900.0;

impl WriterApp {
    /// A research file, shown where the editor would be.
    pub(super) fn file_view(&mut self, ui: &mut egui::Ui, id: &str) {
        let Some(project) = &self.project else { return };
        let Some(node) = project.node(id) else { return };
        let (title, rel) = (node.title.clone(), node.file.clone().unwrap_or_default());
        let Some(path) = project.research_file(id) else { return };
        let ctx = ui.ctx().clone();
        // Read it once, and again if it's changed (or turned up).
        let modified = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        let stale = self.shown_files.get(id).is_none_or(|s| s.path != path || s.modified != modified);
        if stale {
            self.shown_files.insert(id.to_owned(), Shown::load(&path, &ctx));
        }

        // A bar along the top: the title, the file, and opening it elsewhere.
        let mut find = false;
        // The buttons first, from the right; the names get what's left.
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(8.0);
                if path.is_file() {
                    if ui.button(super::settings_ui::SHOW_FOLDER).clicked() {
                        let _ = opener::open(path.parent().unwrap_or(&path));
                    }
                    if ui.button("Open in Its Own App").clicked() {
                        let _ = opener::open(&path);
                    }
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.add_space(8.0);
                    ui.add(egui::Label::new(RichText::new(&title).strong()).truncate());
                    ui.add(egui::Label::new(RichText::new(&rel).weak()).truncate()).on_hover_text(&rel);
                });
            });
        });
        ui.separator();

        let Some(shown) = self.shown_files.get_mut(id) else { return };
        match &mut shown.content {
            Content::Missing => {
                ui.add_space(40.0);
                ui.vertical_centered(|ui| {
                    ui.heading("Not on this computer");
                    ui.add_space(6.0);
                    ui.label(format!("The file goes in the project folder, as \u{201c}{rel}\u{201d}."));
                    ui.weak("Research files stay on the computer they were added on; syncing brings the item, not the file. Copy it into that folder, or find it here:");
                    ui.add_space(8.0);
                    if ui.button("Find the File\u{2026}").clicked() {
                        find = true;
                    }
                });
            }
            Content::Failed(why) => {
                ui.add_space(40.0);
                ui.vertical_centered(|ui| {
                    ui.label(why.as_str());
                    if ui.button("Open in Its Own App").clicked() {
                        let _ = opener::open(&path);
                    }
                });
            }
            Content::Other => {
                ui.add_space(40.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new(crate::icons::ATTACHMENT).size(48.0).weak());
                    ui.label("Writer shows PDFs and pictures; this one opens in its own app.");
                    ui.add_space(6.0);
                    if ui.button("Open in Its Own App").clicked() {
                        let _ = opener::open(&path);
                    }
                });
            }
            Content::Picture(texture) => {
                egui::ScrollArea::both().id_salt(("picture", id)).auto_shrink([false, false]).show(ui, |ui| {
                    let room = (ui.available_size() - egui::vec2(2.0 * MARGIN, 2.0 * MARGIN)).max(egui::vec2(40.0, 40.0));
                    let size = texture.size_vec2() / ui.ctx().pixels_per_point();
                    // Fitted to the space, never made bigger than it is.
                    let scale = (room.x / size.x).min(room.y / size.y).min(1.0);
                    ui.add_space(MARGIN);
                    ui.vertical_centered(|ui| {
                        ui.add(egui::Image::new(&*texture).fit_to_exact_size(size * scale));
                    });
                });
            }
            Content::Pdf(pdf) => pdf_pages(ui, id, pdf),
        }
        if find {
            self.find_research_file(id);
        }
    }

    /// The file for an item that hasn't got it on this computer: pick it,
    /// and a copy goes where the item expects it.
    fn find_research_file(&mut self, id: &str) {
        let Some(path) = self.project.as_ref().and_then(|p| p.research_file(id)) else { return };
        let Some(source) = rfd::FileDialog::new().set_title("Find the File").pick_file() else { return };
        let copied = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::copy(&source, &path).map(|_| ()));
        if let Err(e) = copied {
            self.error = Some(format!("Couldn't copy it into the project: {e}"));
        }
        self.shown_files.remove(id);
    }

    /// Add File…: pick files, each added as a research file near `near`
    /// (or in Research).
    pub(super) fn pick_research_files(&mut self, near: Option<String>) {
        let Some(paths) = rfd::FileDialog::new().set_title("Add Files to the Project").pick_files() else { return };
        self.add_research_files(near, paths);
    }

    /// Add these files as research files, near `near` (or in Research),
    /// one after another, and select the last.
    pub(super) fn add_research_files(&mut self, near: Option<String>, paths: Vec<PathBuf>) {
        let mut place = match near {
            Some(near) => self.place_near(Some(&near)),
            None => Place::Into(crate::project::Role::Research.folder().to_owned()),
        };
        let Some(project) = self.project.as_mut() else { return };
        let mut last = None;
        for path in paths.into_iter().filter(|p| p.is_file()) {
            match project.add_file(&place, &path, "") {
                Ok(id) => {
                    place = Place::After(id.clone());
                    last = Some(id);
                }
                Err(e) => {
                    self.error = Some(format!("Couldn't add {}: {e}", path.display()));
                    break;
                }
            }
        }
        if let Some(id) = last {
            self.select(id);
        }
    }
}

/// A PDF's pages, one under another, each fitted to the width. Only the
/// pages in view (and one either side) are drawn; others are let go.
fn pdf_pages(ui: &mut egui::Ui, id: &str, pdf: &mut PdfView) {
    let ctx = ui.ctx().clone();
    while let Ok(drawn) = pdf.from_drawer.try_recv() {
        match drawn {
            Drawn::Sizes(sizes) => pdf.sizes = sizes,
            Drawn::Page { index, width, image } => {
                pdf.asked.remove(&index);
                let texture = ctx.load_texture(format!("pdf-{id}-{index}"), image, egui::TextureOptions::LINEAR);
                pdf.pages.insert(index, (texture, width));
            }
            Drawn::Failed(why) => {
                ui.label(why);
                return;
            }
        }
    }
    if pdf.sizes.is_empty() {
        ui.add_space(40.0);
        ui.vertical_centered(|ui| ui.spinner());
        return;
    }
    let width = (ui.available_width() - 2.0 * MARGIN).clamp(100.0, MAX_PAGE);
    let gap = 16.0;
    let heights: Vec<f32> = pdf.sizes.iter().map(|s| s.y * width / s.x.max(1.0)).collect();
    let pixels = (width * ctx.pixels_per_point()).round() as u32;
    let count = heights.len();
    let total: f32 = heights.iter().sum::<f32>() + gap * (count as f32 + 1.0);
    egui::ScrollArea::vertical().id_salt(("pdf", id)).auto_shrink([false, false]).show_viewport(ui, |ui, viewport| {
        let full = ui.available_width();
        let (rect, _) = ui.allocate_exact_size(egui::vec2(full, total), egui::Sense::hover());
        let left = rect.left() + (full - width) / 2.0;
        let mut top = rect.top() + gap;
        let mut wanted = Vec::new();
        for (index, height) in heights.iter().enumerate() {
            let page = egui::Rect::from_min_size(egui::pos2(left, top), egui::vec2(width, *height));
            let local = page.translate(-rect.min.to_vec2());
            let near = local.bottom() >= viewport.top() - viewport.height() && local.top() <= viewport.bottom() + viewport.height();
            if near {
                wanted.push(index);
                let painter = ui.painter();
                painter.rect_filled(page, 0.0, Color32::WHITE);
                match pdf.pages.get(&index) {
                    Some((texture, _)) => {
                        painter.image(texture.id(), page, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
                    }
                    None => {
                        painter.text(page.center(), egui::Align2::CENTER_CENTER, format!("Page {}", index + 1), egui::FontId::proportional(14.0), Color32::GRAY);
                    }
                }
                painter.rect_stroke(page, 0.0, ui.visuals().widgets.noninteractive.bg_stroke, egui::StrokeKind::Outside);
                // Drawn again if the width it was drawn at is well off.
                let drawn_at = pdf.pages.get(&index).map(|(_, w)| *w);
                let off = drawn_at.is_none_or(|w| (w as f32 - pixels as f32).abs() > pixels as f32 * 0.2);
                if off && pdf.asked.get(&index) != Some(&pixels) {
                    pdf.asked.insert(index, pixels);
                    let _ = pdf.to_drawer.send((index, pixels));
                }
            }
            top += height + gap;
        }
        // Let go of pages well out of view.
        pdf.pages.retain(|i, _| wanted.contains(i));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_by_extension() {
        assert_eq!(kind_of("Research Files/Map.JPG"), FileKind::Picture);
        assert_eq!(kind_of("Research Files/Timetable.pdf"), FileKind::Pdf);
        assert_eq!(kind_of("Research Files/Interview.m4a"), FileKind::Other);
        assert_eq!(kind_of("Research Files/README"), FileKind::Other);
        assert_eq!(icon("a.png"), crate::icons::PICTURE);
        assert_eq!(icon("a.pdf"), crate::icons::ATTACHMENT);
    }

    #[test]
    fn a_huge_picture_is_scaled_down_to_show() {
        let dir = std::env::temp_dir().join(format!("sloth-writer-huge-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("huge.png");
        image::RgbaImage::new(9000, 300).save(&path).unwrap();
        let shown = picture(&path).unwrap();
        assert_eq!(shown.size, [LARGEST as usize, 137], "the long side fitted, the shape kept");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn draws_a_pdfs_pages_on_its_own_thread() {
        // A PDF made by compile, so there's one to read.
        let Ok((bytes, _)) = crate::compile_pdf::to_pdf(
            &crate::compile::Manuscript { title: "T".into(), chapters: vec![crate::compile::Chapter { heading: "One".into(), scenes: vec!["Words.".into()] }], ..Default::default() },
            &crate::compile::CompileSettings::default(),
        ) else {
            return; // no font to make one with
        };
        let ctx = egui::Context::default();
        let pdf = PdfView::open(bytes, ctx);
        let Ok(Drawn::Sizes(sizes)) = pdf.from_drawer.recv() else { panic!("sizes first") };
        assert_eq!(sizes.len(), 1, "the chapter's page");
        assert!((sizes[0].x - 612.0).abs() < 1.0, "US Letter");
        pdf.to_drawer.send((0, 300)).unwrap();
        let Ok(Drawn::Page { index, width, image }) = pdf.from_drawer.recv() else { panic!("a page") };
        assert_eq!((index, width), (0, 300));
        assert_eq!(image.size[0], 300);
        // Mostly white paper, with some ink.
        let dark = image.pixels.iter().filter(|p| p.r() < 128).count();
        assert!(dark > 0 && dark < image.pixels.len() / 4, "{dark} dark pixels");
    }
}
