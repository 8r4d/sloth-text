//! Writing statistics: words written each day, and the project's goals.
//!
//! Words written are counted net: each time a Manuscript document's text
//! changes, the change in its word count is added to that day's tally, so
//! deleting counts against the day. Moving a document or putting it in the
//! Trash isn't writing and doesn't count; neither does editing outside the
//! Manuscript. Words written on the iPad count on the day they're synced.
//!
//! The daily log lives in `.sloth/stats.ron` in the project folder, beside
//! the sync record; the goals are in the project file.

use std::collections::BTreeMap;
use std::io;
use std::path::Path;

use chrono::{Days, NaiveDate};
use serde::{Deserialize, Serialize};

const STATS_FILE: &str = ".sloth/stats.ron";

/// The day as the log writes it, e.g. "2026-11-22".
pub fn key(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

pub fn parse_date(text: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(text.trim(), "%Y-%m-%d").ok()
}

pub fn today() -> NaiveDate {
    chrono::Local::now().date_naive()
}

/// The words written each day.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Stats {
    /// Net words by day ("YYYY-MM-DD"). Negative when more was cut than written.
    pub days: BTreeMap<String, i64>,
}

impl Stats {
    pub fn load(root: &Path) -> Self {
        std::fs::read_to_string(root.join(STATS_FILE))
            .ok()
            .and_then(|text| ron::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, root: &Path) -> io::Result<()> {
        let path = root.join(STATS_FILE);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default()).map_err(io::Error::other)?;
        sloth_core::document::write_atomically(&path, text.as_bytes())
    }

    pub fn add(&mut self, date: NaiveDate, words: i64) {
        if words != 0 {
            *self.days.entry(key(date)).or_default() += words;
        }
    }

    /// Words written on a day; a day of cutting shows as none.
    pub fn written(&self, date: NaiveDate) -> u32 {
        self.days.get(&key(date)).copied().unwrap_or(0).max(0) as u32
    }

    /// The last `n` days, oldest first, ending today.
    pub fn recent(&self, today: NaiveDate, n: u64) -> Vec<(NaiveDate, u32)> {
        (0..n).rev().filter_map(|back| today.checked_sub_days(Days::new(back))).map(|d| (d, self.written(d))).collect()
    }

    /// Days in a row the goal was met (any words at all, without a goal),
    /// up to today. Today still counts as part of the run before it's met.
    pub fn streak(&self, today: NaiveDate, goal: Option<u32>) -> u32 {
        let enough = goal.unwrap_or(1).max(1);
        let met = |d: NaiveDate| self.written(d) >= enough;
        let mut day = if met(today) { today } else { today.pred_opt().unwrap_or(today) };
        let mut run = 0;
        while met(day) {
            run += 1;
            match day.pred_opt() {
                Some(d) => day = d,
                None => break,
            }
        }
        run
    }

    /// Days with any words written.
    pub fn days_written(&self) -> usize {
        self.days.values().filter(|&&w| w > 0).count()
    }

    /// Average words on the days with any written.
    pub fn average(&self) -> u32 {
        let days = self.days_written();
        if days == 0 {
            return 0;
        }
        let total: i64 = self.days.values().filter(|&&w| w > 0).sum();
        (total as f64 / days as f64).round() as u32
    }
}

/// What the writer is aiming for. Part of the project file.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Goals {
    /// Words a day.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub daily: Option<u32>,
    /// The manuscript's length when finished.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<u32>,
    /// When it's to be finished ("YYYY-MM-DD").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline: Option<String>,
}

impl Goals {
    pub fn is_empty(&self) -> bool {
        *self == Goals::default()
    }

    /// Words a day to reach the total by the deadline, counting today.
    /// `manuscript` is its length now, `today_written` today's words so
    /// far (so the figure holds steady through the day).
    pub fn needed_per_day(&self, manuscript: u32, today_written: u32, today: NaiveDate) -> Option<u32> {
        let total = self.total?;
        let deadline = parse_date(self.deadline.as_deref()?)?;
        let days_left = (deadline - today).num_days() + 1;
        if days_left <= 0 {
            return None;
        }
        let at_start = manuscript.saturating_sub(today_written);
        let remaining = total.saturating_sub(at_start) as i64;
        Some(((remaining + days_left - 1) / days_left) as u32)
    }

    /// Today's goal: the daily goal, or else what the deadline needs.
    pub fn today(&self, manuscript: u32, today_written: u32, today: NaiveDate) -> Option<u32> {
        self.daily.or_else(|| self.needed_per_day(manuscript, today_written, today))
    }
}

/// A number with thousands separators: 12,345.
pub fn thousands(n: u32) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(text: &str) -> NaiveDate {
        parse_date(text).unwrap()
    }

    #[test]
    fn days_add_up_and_cutting_shows_as_none() {
        let mut stats = Stats::default();
        stats.add(day("2026-11-20"), 300);
        stats.add(day("2026-11-20"), -50);
        stats.add(day("2026-11-21"), -400);
        assert_eq!(stats.written(day("2026-11-20")), 250);
        assert_eq!(stats.written(day("2026-11-21")), 0);
        assert_eq!(stats.written(day("2026-11-22")), 0);
        assert_eq!(stats.days_written(), 1);
        assert_eq!(stats.average(), 250);
        let recent = stats.recent(day("2026-11-22"), 3);
        assert_eq!(recent.iter().map(|(_, w)| *w).collect::<Vec<_>>(), vec![250, 0, 0]);
        assert_eq!(recent[2].0, day("2026-11-22"));
    }

    #[test]
    fn streaks_run_up_to_today_or_yesterday() {
        let mut stats = Stats::default();
        for (d, w) in [("2026-11-18", 600), ("2026-11-19", 1200), ("2026-11-20", 1000), ("2026-11-21", 1500)] {
            stats.add(day(d), w);
        }
        // Today not written yet: the run up to yesterday still stands.
        assert_eq!(stats.streak(day("2026-11-22"), None), 4);
        assert_eq!(stats.streak(day("2026-11-22"), Some(1000)), 3);
        stats.add(day("2026-11-22"), 1000);
        assert_eq!(stats.streak(day("2026-11-22"), Some(1000)), 4);
        // A day missed breaks it.
        assert_eq!(stats.streak(day("2026-11-24"), None), 0);
    }

    #[test]
    fn a_deadline_sets_the_daily_figure() {
        let goals = Goals { total: Some(10_000), deadline: Some("2026-11-30".into()), ..Goals::default() };
        // 4,000 written, 6,000 to go over nine days (today included).
        assert_eq!(goals.needed_per_day(4_000, 0, day("2026-11-22")), Some(667));
        // Writing today doesn't move today's figure.
        assert_eq!(goals.needed_per_day(4_500, 500, day("2026-11-22")), Some(667));
        assert_eq!(goals.needed_per_day(12_000, 0, day("2026-11-22")), Some(0), "already there");
        assert_eq!(goals.needed_per_day(4_000, 0, day("2026-12-01")), None, "past the deadline");
        // A daily goal of its own wins.
        let daily = Goals { daily: Some(500), ..goals.clone() };
        assert_eq!(daily.today(4_000, 0, day("2026-11-22")), Some(500));
        assert_eq!(goals.today(4_000, 0, day("2026-11-22")), Some(667));
        assert_eq!(Goals::default().today(4_000, 0, day("2026-11-22")), None);
    }

    #[test]
    fn the_log_is_saved_and_loaded() {
        let dir = std::env::temp_dir().join(format!("sloth-writer-stats-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut stats = Stats::default();
        stats.add(day("2026-11-22"), 812);
        stats.save(&dir).unwrap();
        assert_eq!(Stats::load(&dir), stats);
        assert_eq!(Stats::load(&dir.join("nowhere")), Stats::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn numbers_get_separators() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1000), "1,000");
        assert_eq!(thousands(1234567), "1,234,567");
    }
}
