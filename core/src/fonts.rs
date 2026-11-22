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

const EDITOR_FONT_KEY: &str = "sloth-editor-font";
const PROSE_FONT_KEY: &str = "sloth-prose-font";

/// The font family a prose editor draws with. Always defined once [`apply`]
/// has run: the chosen reading face, falling back to egui's own fonts.
pub fn prose_family() -> FontFamily {
    FontFamily::Name("prose".into())
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

/// Scan installed fonts in the background (it takes a moment), then wake
/// the UI so the View → Font menu can be filled in.
pub fn start_scan(ctx: &egui::Context) {
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let _ = FAMILIES.set(scan());
        ctx.request_repaint();
    });
}

/// Run the font scan now (tests can't wait for the background thread).
/// Not `#[cfg(test)]`: the app's UI tests, in another crate, call it too.
#[doc(hidden)]
pub fn scan_for_tests() -> &'static [String] {
    let _ = FAMILIES.set(scan());
    families()
}

fn scan() -> Vec<String> {
    let mut collection = Collection::new(CollectionOptions::default());
    let names: Vec<String> = collection.family_names().map(str::to_owned).collect();
    let mut mono: Vec<String> = names
        .into_iter()
        // Hidden system fonts (".SF NS Mono" etc.) aren't meant to be picked.
        .filter(|name| !name.starts_with('.'))
        .filter(|name| is_fixed_width(&mut collection, name))
        .collect();
    mono.sort_by_key(|n| n.to_lowercase());
    mono.dedup();
    mono
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
    let mut prose_fonts = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    if let Some((blob, index)) = prose.and_then(load_regular) {
        let data = FontData::from_blob(Arc::new(blob), index);
        fonts.font_data.insert(PROSE_FONT_KEY.to_owned(), Arc::new(data));
        prose_fonts.insert(0, PROSE_FONT_KEY.to_owned());
    }
    fonts.families.insert(prose_family(), prose_fonts);

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
        let families = scan();
        // Menlo ships with every Mac since 10.6; Helvetica isn't fixed width.
        assert!(families.iter().any(|f| f == "Menlo"), "{families:?}");
        assert!(!families.iter().any(|f| f == "Helvetica"));
        assert!(load_regular("Menlo").is_some());
    }

    #[test]
    fn a_reading_face_is_installed() {
        // Georgia and Palatino have shipped with macOS for decades.
        assert!(default_prose_font().is_some());
    }
}
