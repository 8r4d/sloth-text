//! Focus mode's progress: a bar across the top with the document's title and
//! how far the manuscript has come, and a bin per chapter along the bottom,
//! each filling towards its share. When a bin first reaches its share its
//! lid lifts and settles, once (remembered in the project's statistics),
//! and a tick stays. See SEVERANCE-UI.md, part 1.
//!
//! A bin is a top-level item in the Manuscript (usually a chapter or a
//! part). Its share is its own target, or else its documents' targets added
//! up, or else the manuscript's target shared evenly.

use eframe::egui::{self, FontId, Rect, RichText, Stroke, pos2, vec2};
use sloth_core::motion;

use super::WriterApp;
use crate::project::Role;
use crate::stats::thousands;

/// One chapter's bin.
pub(super) struct Bin {
    id: String,
    title: String,
    words: u32,
    share: Option<u32>,
    /// The open document is in it.
    current: bool,
}

/// How long a lid takes to lift and settle (seconds).
const LID_SECONDS: f64 = 0.8;
const BIN: egui::Vec2 = vec2(132.0, 40.0);

impl WriterApp {
    /// The bins, the manuscript's words, and its target (if any).
    pub(super) fn bins(&self) -> (Vec<Bin>, u32, Option<u32>) {
        let Some(project) = &self.project else { return (Vec::new(), 0, None) };
        let Some(top) = project.manifest.binder.iter().find(|n| n.role == Some(Role::Manuscript)) else { return (Vec::new(), 0, None) };
        let target = project.manifest.goals.total.or(top.meta.target_words).filter(|&t| t > 0);
        let even = target.map(|t| t / top.children.len().max(1) as u32);
        let selected = self.selected.as_deref();
        let bins: Vec<Bin> = top
            .children
            .iter()
            .map(|child| {
                let docs = project.documents_under(&child.id);
                let words = docs.iter().map(|d| self.live_words(&d.id)).sum();
                let own: u32 = docs.iter().filter_map(|d| d.meta.target_words).sum();
                let share = child.meta.target_words.filter(|&t| t > 0).or((own > 0).then_some(own)).or(even);
                let current = selected.is_some_and(|s| docs.iter().any(|d| d.id == s) || child.id == s);
                Bin { id: child.id.clone(), title: child.title.clone(), words, share, current }
            })
            .collect();
        let total = bins.iter().map(|b| b.words).sum();
        (bins, total, target)
    }

    /// Bins newly past their share are marked done (once, ever), and the
    /// first of them gets its lid lifted.
    fn mark_full_bins(&mut self, bins: &[Bin], now: f64) {
        for bin in bins {
            let full = bin.share.is_some_and(|s| bin.words >= s);
            if full && !self.stats.bins_done.contains(&bin.id) {
                self.stats.bins_done.push(bin.id.clone());
                self.stats_dirty = true;
                if self.bin_lid.is_none() {
                    self.bin_lid = Some((bin.id.clone(), now));
                }
            }
        }
    }

    /// Focus mode's top bar: the document's title, and the manuscript's
    /// progress (its percentage, or its words with no target).
    pub(super) fn focus_top_bar(&mut self, ui: &mut egui::Ui, fill: egui::Color32) {
        if !self.settings.show_bins {
            return;
        }
        let (_, total, target) = self.bins();
        let title = self.selected.as_deref().and_then(|id| self.project.as_ref()?.node(id)).map(|n| n.title.clone()).unwrap_or_default();
        let accent = ui.visuals().hyperlink_color;
        let weak = ui.visuals().weak_text_color();
        egui::Panel::top(egui::Id::unique("focus-progress")).frame(egui::Frame::NONE.fill(fill).inner_margin(egui::Margin::symmetric(24, 8))).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(title).color(weak));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| match target {
                    Some(t) => {
                        let percent = (total as u64 * 100 / t as u64).min(100);
                        ui.label(RichText::new(format!("{percent}% complete")).color(accent).strong());
                    }
                    None => {
                        ui.label(RichText::new(format!("{} words in the manuscript", thousands(total))).color(weak));
                    }
                });
            });
            let rect = ui.max_rect();
            ui.painter().line_segment([pos2(rect.left(), rect.bottom() + 8.0), pos2(rect.right(), rect.bottom() + 8.0)], Stroke::new(1.0, accent.gamma_multiply(0.35)));
        });
    }

    /// Focus mode's bins, above its status line.
    pub(super) fn focus_bins(&mut self, ui: &mut egui::Ui, fill: egui::Color32) {
        if !self.settings.show_bins {
            return;
        }
        let (bins, _, _) = self.bins();
        if bins.len() < 2 {
            return;
        }
        let now = ui.input(|i| i.time);
        self.mark_full_bins(&bins, now);
        let lid = self.bin_lid.clone().filter(|(_, start)| now - start < LID_SECONDS && motion::animate());
        if lid.is_none() {
            self.bin_lid = None;
        } else {
            ui.ctx().request_repaint();
        }
        let done = self.stats.bins_done.clone();
        egui::Panel::bottom(egui::Id::unique("focus-bins")).frame(egui::Frame::NONE.fill(fill).inner_margin(egui::Margin { left: 24, right: 24, top: 18, bottom: 6 })).show(ui, |ui| {
            egui::ScrollArea::horizontal().show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    for (n, bin) in bins.iter().enumerate() {
                        let lift = lid.as_ref().filter(|(id, _)| *id == bin.id).map(|(_, start)| ((now - start) / LID_SECONDS) as f32);
                        bin_ui(ui, n + 1, bin, done.contains(&bin.id), lift);
                    }
                });
            });
        });
    }
}

/// One bin: its number and title, a bar towards its share, and its lid.
/// `lift` is how far through its lifting the lid is (0 to 1), if it is.
fn bin_ui(ui: &mut egui::Ui, number: usize, bin: &Bin, done: bool, lift: Option<f32>) {
    let (rect, response) = ui.allocate_exact_size(BIN, egui::Sense::hover());
    let visuals = ui.visuals();
    let accent = visuals.hyperlink_color;
    let line = if bin.current { accent } else { visuals.widgets.noninteractive.bg_stroke.color };
    let weak = visuals.weak_text_color();
    let painter = ui.painter();

    // The box, open at the top: its lid is drawn separately.
    let stroke = Stroke::new(if bin.current { 1.5 } else { 1.0 }, line);
    painter.line_segment([rect.left_top(), rect.left_bottom()], stroke);
    painter.line_segment([rect.left_bottom(), rect.right_bottom()], stroke);
    painter.line_segment([rect.right_bottom(), rect.right_top()], stroke);
    // The lid: hinged at the left, lifting its right end and settling.
    let raise = lift.map_or(0.0, |t| (t * std::f32::consts::PI).sin() * 14.0);
    painter.line_segment([rect.left_top(), rect.right_top() - vec2(0.0, raise)], Stroke::new(stroke.width + 0.5, line));

    let inner = rect.shrink2(vec2(8.0, 6.0));
    let label = format!("{number:02}  {}", bin.title);
    let mut job = egui::text::LayoutJob::simple_singleline(label, FontId::proportional(11.0), if bin.current { accent } else { weak });
    job.wrap = egui::text::TextWrapping::truncate_at_width(inner.width() - if done { 12.0 } else { 0.0 });
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    ui.painter().galley(inner.left_top(), galley, weak);
    if done {
        // A tick, drawn (not every font has one).
        let at = inner.right_top() + vec2(-8.0, 3.0);
        let tick = Stroke::new(1.5, accent);
        ui.painter().line_segment([at + vec2(0.0, 3.5), at + vec2(2.5, 6.0)], tick);
        ui.painter().line_segment([at + vec2(2.5, 6.0), at + vec2(8.0, 0.0)], tick);
    }

    // Towards its share.
    let bar = Rect::from_min_size(pos2(inner.left(), inner.bottom() - 4.0), vec2(inner.width(), 3.0));
    ui.painter().rect_filled(bar, 1.5, accent.gamma_multiply(0.15));
    let part = match bin.share {
        Some(s) if s > 0 => (bin.words as f32 / s as f32).clamp(0.0, 1.0),
        _ => 0.0,
    };
    ui.painter().rect_filled(Rect::from_min_size(bar.min, vec2(bar.width() * part, bar.height())), 1.5, accent);

    let tip = match bin.share {
        Some(s) => format!("{}: {} of {} words", bin.title, thousands(bin.words), thousands(s)),
        None => format!("{}: {} words", bin.title, thousands(bin.words)),
    };
    response.on_hover_text(tip);
}
