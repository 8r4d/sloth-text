//! Writing statistics: words written each day, and the project's goals.
//!
//! Words written are counted net: each time a Manuscript document's text
//! changes, the change in its word count is added to that day's tally, so
//! deleting counts against the day. Moving a document or putting it in the
//! Trash isn't writing and doesn't count; neither does editing outside the
//! Manuscript. Words written on the web count on the day they're synced.
//!
//! The daily log lives in `.sloth/stats.ron` in the project folder, beside
//! the sync record; the goals are in the project file.

use std::collections::{BTreeMap, BTreeSet};
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
///
/// A project synced whole between computers (sync.rs) counts each source
/// once: words typed on this computer (`mine`), words that came from the
/// web editor (`web`), and each other computer's own typing (`others`, as
/// it sent them). `days`, what everything reads, is their sum, with the web
/// editor's taken once however many computers received it (the most any
/// one did). A project only ever used on one computer has just `mine`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Stats {
    /// Net words by day ("YYYY-MM-DD"), all sources. Negative when more was
    /// cut than written.
    pub days: BTreeMap<String, i64>,
    /// Typed on this computer, by day.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub mine: BTreeMap<String, i64>,
    /// From the web editor, as this computer received them, by day.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub web: BTreeMap<String, i64>,
    /// Other computers' tallies, by their device id.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub others: BTreeMap<String, Tally>,
    /// The last day reaching the goal was celebrated, so it's only once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub celebrated: Option<String>,
    /// Focus mode's bins (chapters) that have reached their share, by id:
    /// each lid lifts once.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bins_done: Vec<String>,
    /// Rewards earned (see rewards.rs), oldest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rewards: Vec<crate::rewards::Earned>,
    /// Writing sprints finished, oldest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sprints: Vec<Sprint>,
}

/// One computer's own words by day, as it sends them to the others, and
/// what it's celebrated (so the others don't celebrate it again).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Tally {
    pub typed: BTreeMap<String, i64>,
    pub web: BTreeMap<String, i64>,
    /// The computer's name, and when it last synced (seconds since 1970).
    pub name: String,
    pub synced_at: i64,
    /// Rewards it's earned, and the last day it celebrated the goal.
    pub rewards: Vec<crate::rewards::Earned>,
    pub celebrated: Option<String>,
}

impl Tally {
    pub fn to_text(&self) -> String {
        ron::to_string(self).unwrap_or_default()
    }

    pub fn from_text(text: &str) -> Option<Self> {
        ron::from_str(text).ok()
    }
}

/// A writing sprint, finished: the day, how long it was, and the words.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Sprint {
    pub date: String,
    pub minutes: u32,
    pub words: u32,
}

impl Stats {
    pub fn load(root: &Path) -> Self {
        let mut stats: Stats = std::fs::read_to_string(root.join(STATS_FILE))
            .ok()
            .and_then(|text| ron::from_str(&text).ok())
            .unwrap_or_default();
        // From before the sources were kept apart: all of it was this
        // computer's.
        if stats.mine.is_empty() && stats.web.is_empty() && stats.others.is_empty() {
            stats.mine = stats.days.clone();
        }
        stats
    }

    /// This computer's own tally, to send to the others.
    pub fn tally(&self, name: &str, now: i64) -> Tally {
        Tally {
            typed: self.mine.clone(),
            web: self.web.clone(),
            name: name.to_owned(),
            synced_at: now,
            rewards: self.rewards.clone(),
            celebrated: self.celebrated.clone(),
        }
    }

    /// Another computer's tally, as it sent it. What it's celebrated counts
    /// as celebrated here too, quietly.
    pub fn set_other(&mut self, device: &str, tally: Tally) {
        for earned in &tally.rewards {
            if !self.rewards.iter().any(|e| e.key == earned.key) {
                self.rewards.push(earned.clone());
            }
        }
        if tally.celebrated > self.celebrated {
            self.celebrated = tally.celebrated.clone();
        }
        self.others.insert(device.to_owned(), tally);
        self.recount();
    }

    /// Words that came from the web editor.
    pub fn add_web(&mut self, date: NaiveDate, words: i64) {
        if words != 0 {
            *self.web.entry(key(date)).or_default() += words;
            self.recount_day(&key(date));
        }
    }

    fn recount(&mut self) {
        let days: BTreeSet<String> = self
            .mine
            .keys()
            .chain(self.web.keys())
            .chain(self.others.values().flat_map(|t| t.typed.keys().chain(t.web.keys())))
            .cloned()
            .collect();
        for day in days {
            self.recount_day(&day);
        }
    }

    /// A day's total: everyone's typing, and the web's once.
    fn recount_day(&mut self, day: &str) {
        let get = |m: &BTreeMap<String, i64>| m.get(day).copied().unwrap_or(0);
        let typed: i64 = get(&self.mine) + self.others.values().map(|t| get(&t.typed)).sum::<i64>();
        let web = self.others.values().map(|t| get(&t.web)).fold(get(&self.web), i64::max);
        let total = typed + web;
        if total == 0 {
            self.days.remove(day);
        } else {
            self.days.insert(day.to_owned(), total);
        }
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
            *self.mine.entry(key(date)).or_default() += words;
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

    /// The most days in a row the goal was met (any words at all, without
    /// a goal), ever.
    pub fn longest_streak(&self, goal: Option<u32>) -> u32 {
        let enough = goal.unwrap_or(1).max(1) as i64;
        let mut met: Vec<NaiveDate> =
            self.days.iter().filter(|(_, w)| **w >= enough).filter_map(|(day, _)| parse_date(day)).collect();
        met.sort();
        let (mut best, mut run, mut last): (u32, u32, Option<NaiveDate>) = (0, 0, None);
        for day in met {
            run = if last.and_then(|d| d.succ_opt()) == Some(day) { run + 1 } else { 1 };
            best = best.max(run);
            last = Some(day);
        }
        best
    }

    /// A year of days for the calendar, oldest first: from the Monday 52
    /// weeks before this week's, to today.
    pub fn year(&self, today: NaiveDate) -> Vec<(NaiveDate, u32)> {
        use chrono::Datelike;
        let monday = today - Days::new(today.weekday().num_days_from_monday() as u64);
        let start = monday - Days::new(52 * 7);
        start.iter_days().take_while(|d| *d <= today).map(|d| (d, self.written(d))).collect()
    }

    /// Sprints in the last seven days (today among them): how many, their
    /// words, and the most words in one.
    pub fn sprints_this_week(&self, today: NaiveDate) -> (usize, u32, u32) {
        let since = today - Days::new(6);
        let week: Vec<&Sprint> = self.sprints.iter().filter(|s| parse_date(&s.date).is_some_and(|d| d >= since && d <= today)).collect();
        (week.len(), week.iter().map(|s| s.words).sum(), week.iter().map(|s| s.words).max().unwrap_or(0))
    }

    /// Every word written, net, on every day logged.
    pub fn all_written(&self) -> i64 {
        self.days.values().sum()
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

    #[test]
    fn earlier_statistics_are_this_computers_own() {
        let dir = std::env::temp_dir().join(format!("sloth-writer-stats-old-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".sloth")).unwrap();
        std::fs::write(dir.join(STATS_FILE), "(days: {\"2026-11-20\": 400})").unwrap();
        let mut stats = Stats::load(&dir);
        assert_eq!(stats.mine.get("2026-11-20"), Some(&400));
        // Another computer's words add to the day.
        let other = Tally { typed: [("2026-11-20".to_owned(), 100)].into(), ..Tally::default() };
        stats.set_other("pc", other);
        assert_eq!(stats.days.get("2026-11-20"), Some(&500));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn what_another_computer_celebrated_isnt_celebrated_again() {
        let mut stats = Stats::default();
        let earned = crate::rewards::Earned { key: "streak-7".into(), day: "2026-11-20".into() };
        stats.set_other("pc", Tally { rewards: vec![earned], celebrated: Some("2026-11-25".into()), ..Tally::default() });
        let (show, new) = crate::rewards::due(7, None, &stats.rewards);
        assert!(show.is_none() && !new.contains(&"streak-7"), "a week's already been celebrated, there");
        assert_eq!(stats.celebrated.as_deref(), Some("2026-11-25"));
    }

    #[test]
    fn sprints_this_week() {
        let d = |s: &str| parse_date(s).unwrap();
        let mut st = Stats::default();
        for (date, words) in [("2026-11-10", 900), ("2026-11-17", 300), ("2026-11-20", 612), ("2026-11-23", 292)] {
            st.sprints.push(Sprint { date: date.into(), minutes: 25, words });
        }
        assert_eq!(st.sprints_this_week(d("2026-11-23")), (3, 1204, 612), "the 10th is too long ago");
        assert_eq!(Stats::default().sprints_this_week(d("2026-11-23")), (0, 0, 0));
    }

    #[test]
    fn longest_streak_and_a_year_of_days() {
        let d = |s: &str| parse_date(s).unwrap();
        let mut st = Stats::default();
        for (day, words) in [("2026-01-01", 500), ("2026-01-02", 800), ("2026-01-03", 900), ("2026-01-05", 1200), ("2026-01-06", -40)] {
            st.add(d(day), words);
        }
        assert_eq!(st.longest_streak(None), 3);
        assert_eq!(st.longest_streak(Some(800)), 2, "the goal met on the 2nd and 3rd");
        assert_eq!(Stats::default().longest_streak(None), 0);

        // Wednesday 7 January: from Monday 52 weeks before Monday 5 January.
        let year = st.year(d("2026-01-07"));
        assert_eq!(year.first().unwrap().0, d("2025-01-06"));
        assert_eq!(year.last().unwrap().0, d("2026-01-07"));
        assert_eq!(year.len(), 52 * 7 + 3);
        assert!(year.contains(&(d("2026-01-05"), 1200)));
        assert!(year.contains(&(d("2026-01-06"), 0)), "cutting shows as none");
    }

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
