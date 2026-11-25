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
    /// The reading pose in shades of white, to tint as the emblem.
    emblem: Option<egui::TextureHandle>,
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

    /// The Severance and Fallout themes' start screen crest: the reading sloth in one
    /// colour (its light and dark kept, in shades of the theme's accent),
    /// inside a thin ring.
    pub(super) fn sloth_emblem(&mut self, ui: &mut egui::Ui) {
        let texture = self
            .sloth_art
            .emblem
            .get_or_insert_with(|| {
                let png = eframe::icon_data::from_png_bytes(Pose::Reading.png()).expect("the sloth is a valid PNG");
                let rgba: Vec<u8> = png
                    .rgba
                    .as_chunks::<4>().0.iter()
                    .flat_map(|p| {
                        // Brightness, lifted so the darkest parts still show.
                        let l = (0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32) / 255.0;
                        let v = ((0.35 + 0.65 * l) * 255.0) as u8;
                        [v, v, v, p[3]]
                    })
                    .collect();
                let image = egui::ColorImage::from_rgba_unmultiplied([png.width as usize, png.height as usize], &rgba);
                ui.ctx().load_texture("sloth-emblem", image, egui::TextureOptions::LINEAR)
            })
            .clone();
        let accent = ui.visuals().hyperlink_color;
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(120.0), egui::Sense::hover());
        let ring = rect.width() / 2.0 - 2.0;
        ui.painter().circle_stroke(rect.center(), ring, egui::Stroke::new(1.5, accent));
        ui.painter().circle_stroke(rect.center(), ring - 5.0, egui::Stroke::new(0.75, accent.gamma_multiply(0.6)));
        let inner = egui::Rect::from_center_size(rect.center(), Vec2::splat(rect.width() * 0.62));
        egui::Image::new(&texture).tint(accent).paint_at(ui, inner);
    }

    /// The day's fact about your work, under the greeting. Click it for
    /// another.
    pub(super) fn fact_of_the_day(&mut self, ui: &mut egui::Ui) {
        if !self.settings.show_fact {
            return;
        }
        let facts = self.start_facts.get_or_insert_with(|| {
            let projects: Vec<crate::facts::ProjectFacts> = self
                .recent
                .iter()
                .filter_map(|root| {
                    let (title, _) = super::projects_ui::title_of(root)?;
                    let facts = super::projects_ui::facts_of(root)?;
                    Some(crate::facts::ProjectFacts { title, words: facts.words, target: facts.target, stats: crate::stats::Stats::load(root) })
                })
                .collect();
            crate::facts::all(&projects, crate::stats::today())
        });
        let Some(fact) = crate::facts::for_today(facts, crate::stats::today(), self.fact_skip) else { return };
        let fact = fact.to_owned();
        // A terminal's theme types it out, a letter at a time, with a cursor.
        let now = ui.input(|i| i.time);
        let shown = if self.settings.theme.types_out() && sloth_core::motion::animate() {
            let since = match &self.fact_typed {
                Some((typed, at)) if *typed == fact => *at,
                _ => {
                    self.fact_typed = Some((fact.clone(), now));
                    now
                }
            };
            let letters = ((now - since) * 40.0) as usize;
            let count = fact.chars().count();
            if letters < count {
                ui.ctx().request_repaint();
                format!("{}_", fact.chars().take(letters).collect::<String>())
            } else {
                fact.clone()
            }
        } else {
            fact.clone()
        };
        let facts_len = facts.len();
        ui.add_space(6.0);
        let label = egui::Label::new(RichText::new(shown).italics().weak()).sense(egui::Sense::click());
        let more = facts_len > 1;
        let response = ui.add(label);
        if more && response.on_hover_text("Another fact").clicked() {
            self.fact_skip += 1;
        }
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
