//! The day's goal, made visible: a ring in the status bar that fills as
//! the words come, and, once a day when the goal's reached, a small moment:
//! the ring pulses and the sloth says so, then goes. (Settings → Writing →
//! Celebrate reaching the day's goal; animation follows Reduce Motion.)

use std::f32::consts::{PI, TAU};

use eframe::egui::{self, Color32, Pos2, RichText, Sense, Stroke, Vec2};
use sloth_core::motion;

use super::WriterApp;
use crate::stats::{self, thousands};

/// How long the moment lasts, in seconds: in, held, out.
const FADE_IN: f64 = 0.35;
const HOLD: f64 = 4.0;
const FADE_OUT: f64 = 0.8;
/// How long the ring pulses.
const PULSE: f64 = 0.9;

/// The day's goal reached, being celebrated: since when, and the goal.
pub(super) struct Celebration {
    start: f64,
    goal: u32,
    /// Where the ring is on screen, for the moment to rise from.
    ring: Option<egui::Rect>,
}

impl WriterApp {
    /// Today's words (to the last keystroke) and today's goal, if there is one.
    pub(super) fn today_goal(&self) -> (u32, Option<u32>) {
        let (written, total) = self.today_and_total();
        let goals = self.project.as_ref().map(|p| p.manifest.goals.clone()).unwrap_or_default();
        (written, goals.today(total, written, stats::today()).filter(|&g| g > 0))
    }

    /// The ring and "Today: 412 / 1,000", in the status bar. Clicking
    /// either opens the Statistics window.
    pub(super) fn goal_ring(&mut self, ui: &mut egui::Ui, summary: &str, written: u32, goal: Option<u32>) {
        let now = ui.input(|i| i.time);
        let size = 14.0;
        let (rect, ring) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
        if let Some(c) = &mut self.celebration {
            c.ring = Some(rect);
        }
        let fraction = goal.map_or(0.0, |g| (written as f32 / g as f32).min(1.0));
        // A pulse while celebrating: the ring swells and settles.
        let pulse = match &self.celebration {
            Some(c) if motion::animate() && now - c.start < PULSE => ((now - c.start) / PULSE) as f32,
            _ => 1.0,
        };
        let swell = 1.0 + 0.35 * (pulse * PI).sin();
        let visuals = ui.visuals();
        let accent = visuals.hyperlink_color;
        let track = visuals.weak_text_color().gamma_multiply(0.35);
        let painter = ui.painter();
        let radius = size / 2.0 - 1.5;
        if pulse < 1.0 {
            painter.circle_filled(rect.center(), radius * swell + 2.0, accent.gamma_multiply(0.25 * (1.0 - pulse)));
        }
        ring_shape(painter, rect.center(), radius * swell, fraction, accent, track, 2.0);
        if goal.is_some() && fraction >= 1.0 {
            // Done: a small dot in the middle.
            painter.circle_filled(rect.center(), 1.8 * swell, accent);
        }
        if pulse < 1.0 {
            ui.ctx().request_repaint();
        }
        let label = ui.add(egui::Button::new(summary).frame(false));
        let hint = match goal {
            Some(g) if written >= g => "Today's goal reached. Statistics and goals".to_owned(),
            Some(g) => format!("{} to go today. Statistics and goals", thousands(g - written)),
            None => "Writing statistics and goals".to_owned(),
        };
        if (ring | label).on_hover_text(hint).clicked() {
            self.open_stats();
        }
    }

    /// Once a day, when the goal's reached (and celebrating is on), start
    /// the moment. Remembered in the statistics, so it's not repeated.
    pub(super) fn check_goal(&mut self, now: f64) {
        if !self.settings.celebrate_goals || self.project.is_none() {
            return;
        }
        let (written, goal) = self.today_goal();
        let Some(goal) = goal.filter(|&g| written >= g) else { return };
        let today = stats::key(stats::today());
        if self.stats.celebrated.as_deref() == Some(today.as_str()) {
            return;
        }
        self.stats.celebrated = Some(today);
        self.stats_dirty = true;
        self.celebration = Some(Celebration { start: now, goal, ring: None });
    }

    /// The moment itself: the sloth and a line, just above the ring (or the
    /// window's bottom left, in Focus mode), fading in and out.
    pub(super) fn celebration_ui(&mut self, ctx: &egui::Context) {
        let Some(c) = &self.celebration else { return };
        let now = ctx.input(|i| i.time);
        let elapsed = now - c.start;
        if elapsed > FADE_IN + HOLD + FADE_OUT {
            self.celebration = None;
            return;
        }
        let animate = motion::animate();
        let opacity = if !animate {
            1.0
        } else if elapsed < FADE_IN {
            (elapsed / FADE_IN) as f32
        } else if elapsed > FADE_IN + HOLD {
            1.0 - ((elapsed - FADE_IN - HOLD) / FADE_OUT) as f32
        } else {
            1.0
        };
        // Rises a little as it comes in.
        let rise = if animate { (1.0 - motion::ease(now, c.start, FADE_IN)) * 10.0 } else { 0.0 };
        let goal = c.goal;
        let window = ctx.content_rect();
        let anchor = c.ring.map_or(Pos2::new(window.left() + 12.0, window.bottom() - 36.0), |r| Pos2::new(r.left() - 8.0, r.top() - 10.0));
        let icon = self.sloth(ctx, super::sloth_ui::Pose::Cheering);
        egui::Area::new(egui::Id::unique("goal-celebration"))
            .order(egui::Order::Foreground)
            .interactable(false)
            .pivot(egui::Align2::LEFT_BOTTOM)
            .fixed_pos(anchor + Vec2::new(0.0, rise))
            .show(ctx, |ui| {
                ui.set_opacity(opacity.clamp(0.0, 1.0));
                egui::Frame::popup(ui.style()).inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.add(egui::Image::new(&icon).fit_to_exact_size(Vec2::splat(44.0)));
                        ui.vertical(|ui| {
                            ui.label(RichText::new("Today's goal reached").strong());
                            ui.label(RichText::new(format!("{} words. Well done.", thousands(goal))).weak());
                        });
                    });
                });
            });
        ctx.request_repaint();
    }
}

/// A ring `fraction` of the way round (from the top, clockwise) over its
/// faint track.
pub(super) fn ring_shape(painter: &egui::Painter, center: Pos2, radius: f32, fraction: f32, color: Color32, track: Color32, width: f32) {
    painter.circle_stroke(center, radius, Stroke::new(width, track));
    if fraction <= 0.0 {
        return;
    }
    let steps = ((fraction * 48.0).ceil() as usize).max(2);
    let points: Vec<Pos2> = (0..=steps)
        .map(|i| {
            let angle = -PI / 2.0 + TAU * fraction * i as f32 / steps as f32;
            center + Vec2::new(angle.cos(), angle.sin()) * radius
        })
        .collect();
    painter.add(egui::Shape::line(points, Stroke::new(width, color)));
}
