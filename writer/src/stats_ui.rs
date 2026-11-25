//! Keeping the daily word tally, the status bar's "Today" figure, and the
//! Statistics window: today against the goal, the streak, the last 30
//! days, a year of days as a calendar, the manuscript against its target,
//! words per chapter, and the goals themselves. See stats.rs for how words
//! are counted, and goal_ui.rs for the status bar's goal ring.

use chrono::NaiveDate;
use eframe::egui::{self, Pos2, RichText, Sense, Stroke, Vec2};

use super::WriterApp;
use super::inspector::count_words;
use crate::project::Role;
use crate::stats::{self, Goals, thousands};

/// Days in the chart.
const CHART_DAYS: u64 = 30;

impl WriterApp {
    /// Every document's words, as the project opens: the starting point
    /// for counting what's written.
    pub(super) fn count_all_words(&mut self) {
        self.word_counts.clear();
        let Some(project) = &self.project else { return };
        for top in &project.manifest.binder {
            for node in top.subtree() {
                if !node.is_document() {
                    continue;
                }
                let text = project.file_path(&node.id).and_then(|p| crate::project::read_text(&p).ok()).unwrap_or_default();
                self.word_counts.insert(node.id.clone(), count_words(&text) as u32);
            }
        }
    }

    /// A document's text changed: add the change in its words to today,
    /// if it's in the Manuscript. A document not seen before (new) starts
    /// from nothing.
    pub(super) fn note_words(&mut self, id: &str, text: &str) {
        let words = count_words(text) as u32;
        let before = self.word_counts.insert(id.to_owned(), words).unwrap_or(0);
        let in_manuscript = self.project.as_ref().and_then(|p| p.role_of(id)) == Some(Role::Manuscript);
        if in_manuscript && words != before {
            self.stats.add(stats::today(), words as i64 - before as i64);
            self.stats_dirty = true;
        }
    }

    /// A document's words changed in the web editor: counted, as the web's
    /// (so two computers that both receive it don't count it twice).
    pub(super) fn note_web_words(&mut self, id: &str, text: &str) {
        let words = count_words(text) as u32;
        let before = self.word_counts.insert(id.to_owned(), words).unwrap_or(0);
        let in_manuscript = self.project.as_ref().and_then(|p| p.role_of(id)) == Some(Role::Manuscript);
        if in_manuscript && words != before {
            self.stats.add_web(stats::today(), words as i64 - before as i64);
            self.stats_dirty = true;
        }
    }

    /// A document's words changed by moving them (Break Up, Combine), not
    /// by writing: its count follows, and today's doesn't.
    pub(super) fn note_moved(&mut self, id: &str, text: &str) {
        self.word_counts.insert(id.to_owned(), count_words(text) as u32);
    }

    /// The Manuscript's documents.
    fn manuscript_ids(&self) -> Vec<String> {
        let Some(project) = &self.project else { return Vec::new() };
        let Some(top) = project.manifest.binder.iter().find(|n| n.role == Some(Role::Manuscript)) else { return Vec::new() };
        project.documents_under(&top.id).iter().map(|n| n.id.clone()).collect()
    }

    /// A document's words now, counting unsaved typing.
    pub(super) fn live_words(&self, id: &str) -> u32 {
        let open = self.open.get(id).and_then(|&tab| self.docs.get(tab)).filter(|t| t.doc.is_dirty());
        match open {
            Some(tab) => count_words(&tab.doc.text) as u32,
            None => self.word_counts.get(id).copied().unwrap_or(0),
        }
    }

    /// Today's words, and the Manuscript's length, up to the last keystroke.
    /// A day with more cut than written shows as 0.
    pub(super) fn today_and_total(&self) -> (u32, u32) {
        let (net, total) = self.today_net_and_total();
        (net.max(0) as u32, total)
    }

    /// Today's words as they stand, below nothing if more were cut, and the
    /// Manuscript's length.
    pub(super) fn today_net_and_total(&self) -> (i64, u32) {
        let today = stats::today();
        let mut net = self.stats.days.get(&stats::key(today)).copied().unwrap_or(0);
        let mut total = 0;
        for id in self.manuscript_ids() {
            let now = self.live_words(&id);
            net += now as i64 - self.word_counts.get(&id).copied().unwrap_or(0) as i64;
            total += now;
        }
        (net, total)
    }

    /// For the status bar: "Today: 412 / 1,000".
    pub(super) fn today_summary(&self) -> String {
        let (written, total) = self.today_and_total();
        let goals = self.project.as_ref().map(|p| p.manifest.goals.clone()).unwrap_or_default();
        match goals.today(total, written, stats::today()) {
            Some(goal) => format!("Today: {} / {}", thousands(written), thousands(goal)),
            None => format!("Today: {} words", thousands(written)),
        }
    }

    pub(super) fn open_stats(&mut self) {
        self.stats_open = true;
        self.goal_fields = None;
    }

    pub(super) fn stats_window(&mut self, ctx: &egui::Context) {
        if !self.stats_open || self.project.is_none() {
            return;
        }
        let today = stats::today();
        let (written, total) = self.today_and_total();
        let chapters = self.chapters();
        let most_used = self.most_used_words(ctx.input(|i| i.time));
        let mut find_word = None;
        let common = super::repetition_ui::watch_list(&self.settings.common_words);
        let Some(project) = self.project.as_mut() else { return };
        let goals = &mut project.manifest.goals;
        let fields = self.goal_fields.get_or_insert_with(|| {
            [
                goals.daily.map(|n| n.to_string()).unwrap_or_default(),
                goals.total.map(|n| n.to_string()).unwrap_or_default(),
                goals.deadline.clone().unwrap_or_default(),
            ]
        });
        let before = goals.clone();
        let stats = &self.stats;
        let mut open = true;
        egui::Window::new("Statistics")
            .open(&mut open)
            .collapsible(false)
            .default_width(520.0)
            .default_height((ctx.content_rect().height() * 0.85).min(600.0))
            .vscroll(true)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .show(ctx, |ui| {
                {
                    let goal = goals.today(total, written, today);
                    today_section(ui, written, goal, stats.streak(today, goal));
                    ui.add_space(10.0);
                    chart(ui, &stats.recent(today, CHART_DAYS), goal);
                    ui.add_space(14.0);
                    calendar(ui, &stats.year(today), goal, today);
                    let longest = stats.longest_streak(goal);
                    let what = if goal.is_some() { "meeting the goal" } else { "writing" };
                    ui.weak(format!("Longest run {what}: {} {}", thousands(longest), if longest == 1 { "day" } else { "days" }));
                    // The rewards earned, on a shelf.
                    if !stats.rewards.is_empty() {
                        ui.add_space(10.0);
                        ui.strong("Rewards");
                        ui.horizontal_wrapped(|ui| {
                            for earned in &stats.rewards {
                                if let Some(reward) = crate::rewards::find(&earned.key) {
                                    let when = stats::parse_date(&earned.day).map(|d| d.format("%-d %B %Y").to_string()).unwrap_or_default();
                                    ui.label(RichText::new(reward.prize).size(20.0)).on_hover_text(format!("{}\n{when}", reward.title));
                                }
                            }
                        });
                    }
                    ui.add_space(10.0);
                    egui::Grid::new("stats-totals").num_columns(2).spacing([24.0, 4.0]).show(ui, |ui| {
                        ui.label("Manuscript");
                        ui.label(format!("{} words", thousands(total)));
                        ui.end_row();
                        ui.label("Days written");
                        ui.label(thousands(stats.days_written() as u32));
                        ui.end_row();
                        ui.label("Average");
                        ui.label(format!("{} words a writing day", thousands(stats.average())));
                        ui.end_row();
                        let (sprints, words, best) = stats.sprints_this_week(today);
                        if sprints > 0 {
                            ui.label("Sprints this week");
                            let what = if sprints == 1 { "sprint" } else { "sprints" };
                            ui.label(format!("{sprints} {what}, {} words (best {})", thousands(words), thousands(best)));
                            ui.end_row();
                        }
                    });
                    find_word = super::repetition_ui::most_used_section(ui, &most_used, &common);
                    target_section(ui, goals, total, written, today);
                    ui.add_space(8.0);
                    ui.separator();
                    ui.strong("Goals");
                    goals_form(ui, goals, fields);
                    if !chapters.is_empty() {
                        ui.add_space(8.0);
                        ui.separator();
                        ui.strong("Chapters");
                        egui::Grid::new("stats-chapters").num_columns(2).spacing([24.0, 2.0]).striped(true).show(ui, |ui| {
                            for (title, words) in &chapters {
                                ui.label(title);
                                ui.label(thousands(*words));
                                ui.end_row();
                            }
                        });
                    }
                    ui.add_space(6.0);
                    ui.weak("Words written are counted as you type in the Manuscript: cutting counts against the day, and moving or trashing documents doesn't count. Words from the web count on the day they sync.");
                }
            });
        if *goals != before {
            self.manifest_dirty = true;
        }
        match find_word {
            Some(super::repetition_ui::MostUsedAction::Find(word)) => {
                self.find.query = word;
                self.open_find(true);
            }
            Some(super::repetition_ui::MostUsedAction::Common(word, common)) => self.set_common(&word, common),
            None => {}
        }
        if !open {
            self.stats_open = false;
        }
    }

    /// Each top-level item in the Manuscript and its words.
    fn chapters(&self) -> Vec<(String, u32)> {
        let Some(project) = &self.project else { return Vec::new() };
        let Some(top) = project.manifest.binder.iter().find(|n| n.role == Some(Role::Manuscript)) else { return Vec::new() };
        top.children
            .iter()
            .map(|child| {
                let words = project.documents_under(&child.id).iter().map(|d| self.live_words(&d.id)).sum();
                (child.title.clone(), words)
            })
            .collect()
    }
}

fn today_section(ui: &mut egui::Ui, written: u32, goal: Option<u32>, streak: u32) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(thousands(written)).size(30.0).strong());
        ui.vertical(|ui| {
            ui.add_space(4.0);
            match goal {
                Some(goal) => ui.label(format!("words today, of {}", thousands(goal))),
                None => ui.label("words today"),
            };
            let days = if streak == 1 { "day" } else { "days" };
            let what = if goal.is_some() { "meeting the goal" } else { "writing" };
            ui.weak(format!("{streak} {days} in a row {what}"));
        });
    });
    if let Some(goal) = goal.filter(|&g| g > 0) {
        let done = (written as f32 / goal as f32).min(1.0);
        let text = if written >= goal { "Done for today".to_owned() } else { format!("{} to go", thousands(goal - written)) };
        ui.add(egui::ProgressBar::new(done).text(text));
    }
}

/// Size of a day's square in the calendar, and the gap between them.
const CELL: f32 = 7.0;
const GAP: f32 = 2.0;

/// A year of days as a calendar: a column a week (Monday at the top), each
/// day shaded by its words against the goal (or, without one, against a
/// good day's). Hover a day for its words.
fn calendar(ui: &mut egui::Ui, days: &[(NaiveDate, u32)], goal: Option<u32>, today: NaiveDate) {
    use chrono::Datelike;
    ui.strong("This year");
    ui.add_space(2.0);
    // What counts as a full day: the goal, or the busier days' words.
    let full = goal.unwrap_or_else(|| {
        let mut written: Vec<u32> = days.iter().map(|d| d.1).filter(|&w| w > 0).collect();
        written.sort_unstable();
        written.get(written.len() * 3 / 4).copied().unwrap_or(500).max(1)
    });
    let weeks = days.len().div_ceil(7);
    let step = CELL + GAP;
    let left = 14.0; // for the weekday letters
    let top = 14.0; // for the month names
    let size = Vec2::new(left + weeks as f32 * step, top + 7.0 * step);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter_at(rect.expand(2.0));
    let visuals = ui.visuals();
    let accent = visuals.hyperlink_color;
    let empty = visuals.weak_text_color().gamma_multiply(0.18);
    let weak = visuals.weak_text_color();
    let font = egui::FontId::proportional(9.0);
    for (row, letter) in [(0, "M"), (2, "W"), (4, "F")] {
        painter.text(rect.min + Vec2::new(0.0, top + row as f32 * step - 1.0), egui::Align2::LEFT_TOP, letter, font.clone(), weak);
    }
    let cell_at = |i: usize| rect.min + Vec2::new(left + (i / 7) as f32 * step, top + (i % 7) as f32 * step);
    let mut last_month_label = None;
    let mut hovered = None;
    for (i, &(day, words)) in days.iter().enumerate() {
        let at = cell_at(i);
        // A month's name over the week its first day is in.
        if day.day() == 1 || i == 0 {
            let week = i / 7;
            if last_month_label.is_none_or(|w: usize| week >= w + 3) {
                let name = day.format("%b").to_string();
                painter.text(Pos2::new(at.x, rect.top()), egui::Align2::LEFT_TOP, name, font.clone(), weak);
                last_month_label = Some(week);
            }
        }
        let square = egui::Rect::from_min_size(at, Vec2::splat(CELL));
        let fill = if words == 0 {
            empty
        } else {
            accent.gamma_multiply(0.3 + 0.7 * (words as f32 / full as f32).min(1.0))
        };
        painter.rect_filled(square, 1.5, fill);
        if day == today {
            painter.rect_stroke(square.expand(1.0), 2.0, Stroke::new(1.0, visuals.text_color()), egui::StrokeKind::Outside);
        }
        if response.hover_pos().is_some_and(|p| square.expand(GAP / 2.0).contains(p)) {
            hovered = Some((day, words));
        }
    }
    if let Some((day, words)) = hovered {
        let what = if words == 1 { "word".to_owned() } else { "words".to_owned() };
        response.on_hover_text_at_pointer(format!("{}: {} {what}", day.format("%a %-d %b %Y"), thousands(words)));
    }
}

/// The last days as bars, with the goal as a line.
fn chart(ui: &mut egui::Ui, days: &[(NaiveDate, u32)], goal: Option<u32>) {
    let height = 110.0;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height + 16.0), Sense::hover());
    let painter = ui.painter_at(rect);
    let most = days.iter().map(|(_, w)| *w).max().unwrap_or(0).max(goal.unwrap_or(0)).max(1) as f32;
    let slot = rect.width() / days.len().max(1) as f32;
    let base = rect.top() + height;
    let accent = ui.visuals().selection.bg_fill;
    let faint = ui.visuals().weak_text_color();
    painter.line_segment([egui::pos2(rect.left(), base), egui::pos2(rect.right(), base)], Stroke::new(1.0, faint));
    let hovered = response.hover_pos().map(|p| ((p.x - rect.left()) / slot) as usize);
    for (i, (date, words)) in days.iter().enumerate() {
        let x = rect.left() + slot * i as f32;
        let h = height * (*words as f32 / most);
        let met = goal.is_some_and(|g| *words >= g && g > 0);
        let color = if met || goal.is_none() { accent } else { accent.gamma_multiply(0.55) };
        let bar = egui::Rect::from_min_max(egui::pos2(x + slot * 0.15, base - h), egui::pos2(x + slot * 0.85, base));
        painter.rect_filled(bar, 2.0, color);
        if hovered == Some(i) {
            painter.rect_stroke(bar.expand(1.0), 2.0, Stroke::new(1.0, ui.visuals().strong_text_color()), egui::StrokeKind::Outside);
        }
        // A date under the first bar, every seventh, and today (not
        // crowding today's).
        let last = days.len() - 1;
        if i == last || (i % 7 == 1 && last - i >= 4) {
            painter.text(
                // Today's date ends at the chart's edge rather than past it.
                if i == last { egui::pos2(rect.right(), base + 2.0) } else { egui::pos2(x + slot / 2.0, base + 2.0) },
                if i == last { egui::Align2::RIGHT_TOP } else { egui::Align2::CENTER_TOP },
                date.format("%-d %b").to_string(),
                egui::FontId::proportional(10.0),
                faint,
            );
        }
    }
    if let Some(goal) = goal.filter(|&g| g > 0) {
        let y = base - height * (goal as f32 / most);
        let dash = Stroke::new(1.0, faint);
        painter.extend(egui::Shape::dashed_line(&[egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)], dash, 4.0, 4.0));
    }
    if let Some((date, words)) = hovered.and_then(|i| days.get(i)) {
        response.on_hover_text(format!("{}: {} words", date.format("%a %-d %B"), thousands(*words)));
    }
}

fn target_section(ui: &mut egui::Ui, goals: &Goals, total: u32, written: u32, today: NaiveDate) {
    let Some(target) = goals.total.filter(|&t| t > 0) else { return };
    ui.add_space(8.0);
    let done = (total as f32 / target as f32).min(1.0);
    let label = format!("{} of {} words ({:.0}%)", thousands(total), thousands(target), done * 100.0);
    ui.add(egui::ProgressBar::new(done).text(label));
    let deadline = goals.deadline.as_deref().and_then(stats::parse_date);
    if total >= target {
        ui.label("The manuscript has reached its target.");
        return;
    }
    match (deadline, goals.needed_per_day(total, written, today)) {
        (Some(date), Some(per_day)) => {
            let days = (date - today).num_days() + 1;
            let plural = if days == 1 { "day" } else { "days" };
            ui.label(format!(
                "{} words a day to finish by {} ({days} {plural} left, today included)",
                thousands(per_day),
                date.format("%-d %B %Y")
            ));
        }
        (Some(date), None) => {
            ui.label(format!("The deadline, {}, has passed.", date.format("%-d %B %Y")));
        }
        (None, _) => {}
    }
}

/// The goals, typed in. A field that doesn't make sense is left as it was.
pub(super) fn goals_form(ui: &mut egui::Ui, goals: &mut Goals, fields: &mut [String; 3]) {
    egui::Grid::new("stats-goals").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
        ui.label("Words a day");
        if ui.add(egui::TextEdit::singleline(&mut fields[0]).hint_text("e.g. 1000").desired_width(120.0)).changed()
            && let Some(n) = number(&fields[0]) {
                goals.daily = n;
            }
        ui.end_row();
        ui.label("Manuscript length");
        if ui.add(egui::TextEdit::singleline(&mut fields[1]).hint_text("e.g. 90000").desired_width(120.0)).changed()
            && let Some(n) = number(&fields[1]) {
                goals.total = n;
            }
        ui.end_row();
        ui.label("Finish by");
        ui.horizontal(|ui| {
            let field = ui.add(egui::TextEdit::singleline(&mut fields[2]).hint_text("YYYY-MM-DD").desired_width(120.0));
            let text = fields[2].trim();
            if field.changed() {
                if text.is_empty() {
                    goals.deadline = None;
                } else if let Some(date) = stats::parse_date(text) {
                    goals.deadline = Some(stats::key(date));
                }
            }
            if !text.is_empty() && stats::parse_date(text).is_none() {
                ui.weak("as year-month-day");
            }
        });
        ui.end_row();
    });
    if goals.daily.is_none() && goals.total.is_some() && goals.deadline.is_some() {
        ui.weak("With no daily goal, the day's goal is what the deadline needs.");
    }
}

/// A goal as typed: blank for none, or a whole number (commas allowed).
/// `None` when it isn't one.
fn number(text: &str) -> Option<Option<u32>> {
    let digits: String = text.chars().filter(|c| !matches!(c, ',' | ' ' | '_')).collect();
    if digits.is_empty() {
        return Some(None);
    }
    digits.parse().ok().map(|n: u32| (n > 0).then_some(n))
}

#[cfg(test)]
mod tests {
    use super::number;

    #[test]
    fn goals_are_typed_as_numbers() {
        assert_eq!(number("1000"), Some(Some(1000)));
        assert_eq!(number("90,000"), Some(Some(90_000)));
        assert_eq!(number(""), Some(None));
        assert_eq!(number("0"), Some(None));
        assert_eq!(number("lots"), None);
    }
}
