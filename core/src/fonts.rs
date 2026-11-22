//! The editor fonts: listing the installed fixed-width fonts, and installing
//! the chosen one (plus the tab width) into egui. Also the "prose" family,
//! a reading face for editors showing prose (Sloth Writer), kept separate
//! so the rest of the interface keeps its usual font.

use std::sync::{Arc, OnceLock};

use eframe::egui::{self, FontData, FontDefinitions, FontFamily};
use fontique::{Collection, CollectionOptions, FontStyle, FontWeight, FontWidth};
use read_fonts::{FontRef, TableProvider};

/// Installed fixed-width font families, sorted. Filled in by a background
/// scan started at launch; empty until then.
static FAMILIES: OnceLock<Vec<String>> = OnceLock::new();
/// Every installed font family (for prose), sorted; filled in by the same scan.
static ALL_FAMILIES: OnceLock<Vec<String>> = OnceLock::new();

const EDITOR_FONT_KEY: &str = "sloth-editor-font";
const PROSE_FONT_KEY: &str = "sloth-prose-font";

/// The font family a prose editor draws with. Always defined once [`apply`]
/// has run: the chosen reading face, falling back to egui's own fonts.
pub fn prose_family() -> FontFamily {
    FontFamily::Name("prose".into())
}

/// The prose family's bold, italic or bold italic face, if the prose font
/// has a true one installed (see [`apply`]). Without it, editors draw
/// italics slanted and bold as regular.
pub fn prose_variant(bold: bool, italic: bool) -> Option<FontFamily> {
    let name = match (bold, italic) {
        (false, false) => return Some(prose_family()),
        (true, false) => "prose-bold",
        (false, true) => "prose-italic",
        (true, true) => "prose-bold-italic",
    };
    Some(FontFamily::Name(name.into()))
}

/// Reading faces that come with macOS, best first: the default prose font
/// is the first one installed.
pub const PROSE_DEFAULTS: [&str; 4] = ["Iowan Old Style", "Charter", "Georgia", "Palatino"];

pub fn default_prose_font() -> Option<&'static str> {
    PROSE_DEFAULTS.into_iter().find(|f| load_regular(f).is_some())
}

pub fn families() -> &'static [String] {
    FAMILIES.get().map(Vec::as_slice).unwrap_or(&[])
}

/// Every installed font family, fixed-width or not, for choosing a prose
/// font. Empty until the scan started by [`start_scan`] finishes.
pub fn all_families() -> &'static [String] {
    ALL_FAMILIES.get().map(Vec::as_slice).unwrap_or(&[])
}

/// Scan installed fonts in the background (it takes a moment), then wake
/// the UI so the View → Font menu can be filled in.
pub fn start_scan(ctx: &egui::Context) {
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let (mono, all) = scan();
        let _ = FAMILIES.set(mono);
        let _ = ALL_FAMILIES.set(all);
        ctx.request_repaint();
    });
}

/// Run the font scan now (tests can't wait for the background thread).
/// Not `#[cfg(test)]`: the app's UI tests, in another crate, call it too.
#[doc(hidden)]
pub fn scan_for_tests() -> &'static [String] {
    let (mono, all) = scan();
    let _ = FAMILIES.set(mono);
    let _ = ALL_FAMILIES.set(all);
    families()
}

/// The installed families: (fixed-width ones, all of them), each sorted.
fn scan() -> (Vec<String>, Vec<String>) {
    let mut collection = Collection::new(CollectionOptions::default());
    let mut all: Vec<String> = collection
        .family_names()
        .map(str::to_owned)
        // Hidden system fonts (".SF NS Mono" etc.) aren't meant to be picked.
        .filter(|name| !name.starts_with('.'))
        .collect();
    all.sort_by_key(|n| n.to_lowercase());
    all.dedup();
    let mono = all.iter().filter(|name| is_fixed_width(&mut collection, name)).cloned().collect();
    (mono, all)
}

fn is_fixed_width(collection: &mut Collection, family: &str) -> bool {
    let Some(info) = collection.family_by_name(family) else {
        return false;
    };
    let Some(font) = info.default_font() else {
        return false;
    };
    let Some(blob) = font.load(None) else {
        return false;
    };
    FontRef::from_index(blob.as_ref(), font.index())
        .ok()
        .and_then(|f| f.post().ok())
        .is_some_and(|post| post.is_fixed_pitch() != 0)
}

/// Install the editor font and the prose font (`None` for egui's built-in
/// ones), and the tab width.
pub fn apply(ctx: &egui::Context, family: Option<&str>, prose: Option<&str>, tab_width: usize) {
    let mut fonts = FontDefinitions::default();

    // The prose family: the reading face, then egui's fonts for any
    // characters it lacks.
    let fallback = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let mut prose_fonts = fallback.clone();
    if let Some((blob, index)) = prose.and_then(load_regular) {
        let data = FontData::from_blob(Arc::new(blob), index);
        fonts.font_data.insert(PROSE_FONT_KEY.to_owned(), Arc::new(data));
        prose_fonts.insert(0, PROSE_FONT_KEY.to_owned());
    }
    fonts.families.insert(prose_family(), prose_fonts);
    // Its bold and italic faces, each only if the font really has it.
    if let Some(family) = prose {
        for (bold, italic) in [(true, false), (false, true), (true, true)] {
            let Some((blob, index)) = load_face(family, bold, italic) else { continue };
            let key = format!("{PROSE_FONT_KEY}-{bold}-{italic}");
            fonts.font_data.insert(key.clone(), Arc::new(FontData::from_blob(Arc::new(blob), index)));
            let mut list = fallback.clone();
            list.insert(0, key);
            fonts.families.insert(prose_variant(bold, italic).expect("a variant"), list);
        }
    }

    if let Some((blob, index)) = family.and_then(load_regular) {
        let data = FontData::from_blob(Arc::new(blob), index);
        fonts.font_data.insert(EDITOR_FONT_KEY.to_owned(), Arc::new(data));
        if let Some(mono) = fonts.families.get_mut(&FontFamily::Monospace) {
            mono.insert(0, EDITOR_FONT_KEY.to_owned());
        }
    }

    // Tab characters are drawn `tab_size` spaces wide.
    for data in fonts.font_data.values_mut() {
        Arc::make_mut(data).tweak.tab_size = tab_width as f32;
    }
    ctx.set_fonts(fonts);
}

/// A family's bold and/or italic face, only if it truly is one (a family
/// without an italic would otherwise hand back its upright face).
fn load_face(family: &str, bold: bool, italic: bool) -> Option<(fontique::Blob<u8>, u32)> {
    let mut collection = Collection::new(CollectionOptions::default());
    let info = collection.family_by_name(family)?;
    let weight = if bold { FontWeight::BOLD } else { FontWeight::NORMAL };
    let style = if italic { FontStyle::Italic } else { FontStyle::Normal };
    let font = info.match_font(FontWidth::NORMAL, style, weight, false)?;
    let is_italic = !matches!(font.style(), FontStyle::Normal);
    let is_bold = font.weight().value() >= 600.0;
    if is_italic != italic || is_bold != bold {
        return None;
    }
    Some((font.load(None)?, font.index()))
}

/// The regular (upright, normal weight) face of a family.
fn load_regular(family: &str) -> Option<(fontique::Blob<u8>, u32)> {
    let mut collection = Collection::new(CollectionOptions::default());
    let info = collection.family_by_name(family)?;
    let font = info
        .match_font(FontWidth::NORMAL, FontStyle::Normal, FontWeight::NORMAL, false)
        .or_else(|| info.default_font())?;
    Some((font.load(None)?, font.index()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_menlo_and_skips_proportional_fonts() {
        let (families, all) = scan();
        assert!(all.iter().any(|f| f == "Helvetica") && all.iter().any(|f| f == "Menlo"), "every family is listed");
        // Menlo ships with every Mac since 10.6; Helvetica isn't fixed width.
        assert!(families.iter().any(|f| f == "Menlo"), "{families:?}");
        assert!(!families.iter().any(|f| f == "Helvetica"));
        assert!(load_regular("Menlo").is_some());
    }

    #[test]
    fn finds_true_bold_and_italic_faces() {
        // Georgia ships with macOS with all four faces.
        assert!(load_face("Georgia", true, false).is_some());
        assert!(load_face("Georgia", false, true).is_some());
        assert!(load_face("Georgia", true, true).is_some());
    }

    #[test]
    fn a_reading_face_is_installed() {
        // Georgia and Palatino have shipped with macOS for decades.
        assert!(default_prose_font().is_some());
    }
}
