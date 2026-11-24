//! Writing sprints: a set time (10 to 60 minutes) to write in, with the
//! time left and the words so far in the status bar (and Focus mode's
//! footer). When it's up, the sloth says how it went, and it's kept in the
//! project's statistics. Words count as they do for the day: net, in the
//! Manuscript.

use eframe::egui::{self, Sense, Vec2};

use super::WriterApp;
use crate::stats::{self, thousands};

/// The lengths offered, in minutes.
const LENGTHS: [u32; 5] = [10, 15, 25, 45, 60];

/// A sprint under way: when it started (egui's clock), how long it is, and
/// the words written up to then.
pub(super) struct Sprint {
    pub(super) start: f64,
    minutes: u32,
    written_before: i64,
}

impl WriterApp {
    /// Every word written so far, to the last keystroke: the log, with
    /// today's as it stands now.
    fn written_ever(&self) -> i64 {
        let today = self.stats.days.get(&stats::key(stats::today())).copied().unwrap_or(0);
        // Not today's as shown (never below 0): a day that started with
        // more cut than written would hold the sprint at 0 too.
        self.stats.all_written() - today + self.today_net_and_total().0
    }

    pub(super) fn start_sprint(&mut self, now: f64, minutes: u32) {
        self.sprint = Some(Sprint { start: now, minutes, written_before: self.written_ever() });
        // Straight back to the page.
        self.focus_editor = true;
    }

    /// Words written in the sprint so far.
    pub(super) fn sprint_words(&self) -> u32 {
        self.sprint.as_ref().map_or(0, |s| (self.written_ever() - s.written_before).max(0) as u32)
    }

    /// Seconds left in the sprint, and the fraction of it gone.
    fn sprint_left(&self, now: f64) -> Option<(f64, f32)> {
        let s = self.sprint.as_ref()?;
        let length = s.minutes as f64 * 60.0;
        let gone = (now - s.start).clamp(0.0, length);
        Some((length - gone, (gone / length) as f32))
    }

    /// Stop the sprint. If it ran its course, keep it and say how it went.
    pub(super) fn stop_sprint(&mut self, now: f64, finished: bool) {
        let words = self.sprint_words();
        let Some(sprint) = self.sprint.take() else { return };
        if !finished {
            return;
        }
        self.stats.sprints.push(stats::Sprint { date: stats::key(stats::today()), minutes: sprint.minutes, words });
        self.stats_dirty = true;
        let detail = match words {
            0 => format!("{} minutes. The page waits.", sprint.minutes),
            1 => format!("1 word in {} minutes.", sprint.minutes),
            n => format!("{} words in {} minutes.", thousands(n), sprint.minutes),
        };
        self.celebrate(now, "Sprint done", &detail);
    }

    /// Each frame: end the sprint when its time's up, and keep the clock
    /// ticking meanwhile.
    pub(super) fn sprint_tick(&mut self, ctx: &egui::Context, now: f64) {
        match self.sprint_left(now) {
            Some((left, _)) if left <= 0.0 => self.stop_sprint(now, true),
            Some(_) => ctx.request_repaint_after(std::time::Duration::from_millis(500)),
            None => {}
        }
    }

    /// "18:42 · 312 words", while a sprint's on.
    pub(super) fn sprint_summary(&self, now: f64) -> Option<String> {
        let (left, _) = self.sprint_left(now)?;
        let secs = left.ceil() as u64;
        let words = self.sprint_words();
        Some(format!("{}:{:02} \u{b7} {} {}", secs / 60, secs % 60, thousands(words), if words == 1 { "word" } else { "words" }))
    }

    /// In the status bar: Sprint, to start one; while one's on, its clock
    /// (a ring emptying as the time goes) and words, to stop it.
    pub(super) fn sprint_button(&mut self, ui: &mut egui::Ui) {
        let now = ui.input(|i| i.time);
        match (self.sprint_left(now), self.sprint_summary(now)) {
            (Some((_, gone)), Some(summary)) => {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
                let visuals = ui.visuals();
                let (accent, track) = (visuals.hyperlink_color, visuals.weak_text_color().gamma_multiply(0.35));
                super::goal_ui::ring_shape(ui.painter(), rect.center(), 5.5, 1.0 - gone, accent, track, 2.0);
                ui.menu_button(summary, |ui| {
                    if ui.button("Stop Sprint").clicked() {
                        self.stop_sprint(now, false);
                        ui.close();
                    }
                })
                .response
                .on_hover_text("A writing sprint: the time left, and the words so far");
            }
            _ => {
                ui.menu_button("Sprint", |ui| {
                    ui.label(egui::RichText::new("Write for").small().weak());
                    for minutes in LENGTHS {
                        if ui.button(format!("{minutes} minutes")).clicked() {
                            self.start_sprint(now, minutes);
                            ui.close();
                        }
                    }
                })
                .response
                .on_hover_text("A writing sprint: a set time to write, and how many words you got");
            }
        }
    }
}
