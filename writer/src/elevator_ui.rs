//! The elevator: opening a project (or going back to the start screen), a
//! panel in the window's colour, carrying where you're going like a floor's
//! name, slides up and away as the new screen arrives beneath it. About
//! 0.45 s; none with reduced motion, or with App Settings → Appearance →
//! "Slide between the start screen and a project" off. See SEVERANCE-UI.md,
//! part 4.
//!
//! It's only drawn over the window, so clicks and keys reach what's
//! underneath straight away.

use eframe::egui::{self, FontId, Stroke, pos2, vec2};
use sloth_core::motion;

use super::WriterApp;

const SECONDS: f64 = 0.45;

/// A ride under way: since when (from its first frame), and the floor's
/// name.
pub(super) struct Ride {
    start: Option<f64>,
    floor: String,
}

impl WriterApp {
    /// Start a ride to `floor` (a project's title, or the start screen).
    pub(super) fn ride_to(&mut self, floor: &str) {
        if self.settings.elevator && motion::animate() {
            self.elevator = Some(Ride { start: None, floor: floor.to_owned() });
        }
    }

    pub(super) fn elevator_ui(&mut self, ctx: &egui::Context) {
        let Some(ride) = &mut self.elevator else { return };
        let now = ctx.input(|i| i.time);
        let start = *ride.start.get_or_insert(now);
        let t = ((now - start) / SECONDS) as f32;
        if t >= 1.0 {
            self.elevator = None;
            return;
        }
        // Holds a moment, then rises, slowing as it goes.
        let rise = ((t - 0.25) / 0.75).clamp(0.0, 1.0);
        let eased = 1.0 - (1.0 - rise).powi(3);
        let window = ctx.content_rect();
        let panel = window.translate(vec2(0.0, -eased * window.height()));
        let visuals = ctx.global_style().visuals.clone();
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("elevator")));
        painter.rect_filled(panel, 0.0, visuals.window_fill);
        // The panel's leading edge, and the floor's name.
        let accent = visuals.hyperlink_color;
        painter.line_segment([pos2(panel.left(), panel.bottom()), pos2(panel.right(), panel.bottom())], Stroke::new(2.0, accent));
        painter.text(panel.center(), egui::Align2::CENTER_CENTER, &ride.floor, FontId::proportional(26.0), visuals.weak_text_color());
        ctx.request_repaint();
    }
}

impl WriterApp {
    /// The old screen's glow, with a Severance theme and App Settings →
    /// Appearance → "Old screen glow" on: faint scanlines across the window
    /// and a soft darkening towards its edges. See SEVERANCE-UI.md, part 5.
    pub(super) fn screen_glow_ui(&self, ctx: &egui::Context) {
        if !self.settings.screen_glow || !self.settings.theme.is_severance() {
            return;
        }
        let window = ctx.content_rect();
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("screen-glow")));
        let dark = ctx.global_style().visuals.dark_mode;
        // Scanlines: one line in three, darkened a few percent.
        let line = egui::Color32::from_black_alpha(if dark { 34 } else { 12 });
        let mut y = window.top() + 1.5;
        while y < window.bottom() {
            painter.line_segment([pos2(window.left(), y), pos2(window.right(), y)], Stroke::new(1.0, line));
            y += 3.0;
        }
        // The vignette: clear in the middle, darker at the edges.
        let edge = egui::Color32::from_black_alpha(if dark { 70 } else { 28 });
        let clear = egui::Color32::TRANSPARENT;
        let inner = window.shrink2(vec2(window.width() * 0.22, window.height() * 0.22));
        let mut mesh = egui::Mesh::default();
        let corners = |r: egui::Rect| [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()];
        for p in corners(window) {
            mesh.colored_vertex(p, edge);
        }
        for p in corners(inner) {
            mesh.colored_vertex(p, clear);
        }
        for i in 0..4u32 {
            let j = (i + 1) % 4;
            mesh.add_triangle(i, j, 4 + i);
            mesh.add_triangle(j, 4 + j, 4 + i);
        }
        painter.add(egui::Shape::mesh(mesh));
    }
}
