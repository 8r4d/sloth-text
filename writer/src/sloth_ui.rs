//! The sloth from the app icon, in three poses (drawn by
//! scripts/make-writer-sloth.swift): reading, asleep and cheering. It keeps
//! the start screen company, fills the empty places (an empty folder, the
//! Trash, a blank Canvas, nothing open), and cheers when the day's goal is
//! reached.

use std::collections::HashMap;

use eframe::egui::{self, RichText, Vec2};

use super::WriterApp;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Pose {
    Reading,
    Asleep,
    Cheering,
}

impl Pose {
    fn png(self) -> &'static [u8] {
        match self {
            Pose::Reading => include_bytes!("../icons/sloth-reading.png"),
            Pose::Asleep => include_bytes!("../icons/sloth-asleep.png"),
            Pose::Cheering => include_bytes!("../icons/sloth-cheering.png"),
        }
    }
}

/// The poses as textures, loaded when first shown.
#[derive(Default)]
pub(super) struct SlothArt {
    textures: HashMap<Pose, egui::TextureHandle>,
}

impl WriterApp {
    /// The sloth in a pose, ready to draw.
    pub(super) fn sloth(&mut self, ctx: &egui::Context, pose: Pose) -> egui::TextureHandle {
        self.sloth_art
            .textures
            .entry(pose)
            .or_insert_with(|| {
                let png = eframe::icon_data::from_png_bytes(pose.png()).expect("the sloth is a valid PNG");
                let image = egui::ColorImage::from_rgba_unmultiplied([png.width as usize, png.height as usize], &png.rgba);
                ctx.load_texture(format!("sloth-{pose:?}"), image, egui::TextureOptions::LINEAR)
            })
            .clone()
    }

    /// An empty place, kindly: the sloth, a line saying what's (not) here,
    /// and a hint at what to do, centred in what's left of `ui`.
    pub(super) fn empty_state(&mut self, ui: &mut egui::Ui, pose: Pose, title: &str, hint: &str) {
        let sloth = self.sloth(ui.ctx(), pose);
        let top = ((ui.available_height() - 200.0) * 0.4).max(12.0);
        ui.vertical_centered(|ui| {
            ui.add_space(top);
            ui.add(egui::Image::new(&sloth).fit_to_exact_size(Vec2::splat(112.0)));
            ui.add_space(6.0);
            ui.label(RichText::new(title).size(16.0).strong());
            ui.add_space(2.0);
            ui.weak(hint);
        });
    }
}

/// A greeting for the hour of the day (0 to 23), and the sloth to go with it.
pub(super) fn greeting(hour: u32) -> (&'static str, Pose) {
    match hour {
        5..=11 => ("Good morning", Pose::Reading),
        12..=16 => ("Good afternoon", Pose::Reading),
        17..=21 => ("Good evening", Pose::Reading),
        _ => ("Writing late?", Pose::Asleep),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_greeting_for_every_hour() {
        assert_eq!(greeting(7), ("Good morning", Pose::Reading));
        assert_eq!(greeting(12), ("Good afternoon", Pose::Reading));
        assert_eq!(greeting(21), ("Good evening", Pose::Reading));
        assert_eq!(greeting(23), ("Writing late?", Pose::Asleep));
        assert_eq!(greeting(3), ("Writing late?", Pose::Asleep));
        for pose in [Pose::Reading, Pose::Asleep, Pose::Cheering] {
            assert!(eframe::icon_data::from_png_bytes(pose.png()).is_ok(), "{pose:?} is a valid picture");
        }
    }
}
