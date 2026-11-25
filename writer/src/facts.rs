//! Facts about your work, for the start screen: one calm, true sentence a
//! day, made from the recent projects' statistics ("You wrote 1,204 words
//! on Tuesday."). See SEVERANCE-UI.md, part 3.
//!
//! Each fact is a small function that returns a sentence, or nothing when
//! its numbers aren't there (no target, too few days written). The day's
//! fact is picked by the date, so it stays the same all day.

use std::collections::BTreeMap;

use chrono::{Datelike, NaiveDate, Weekday};

use crate::stats::{self, Stats, thousands};

/// What's known about one recent project.
pub struct ProjectFacts {
    pub title: String,
    /// Words in the manuscript, and its target if it has one.
    pub words: u32,
    pub target: Option<u32>,
    pub stats: Stats,
}

/// Every fact that holds today, in a fixed order.
pub fn all(projects: &[ProjectFacts], today: NaiveDate) -> Vec<String> {
    // Words by day, all projects together.
    let mut days: BTreeMap<NaiveDate, i64> = BTreeMap::new();
    for p in projects {
        for (day, words) in &p.stats.days {
            if let Some(d) = stats::parse_date(day) {
                *days.entry(d).or_default() += words;
            }
        }
    }
    let written: Vec<(NaiveDate, u32)> = days.iter().filter(|(_, w)| **w > 0).map(|(d, w)| (*d, *w as u32)).collect();

    let mut out = Vec::new();
    // The last writing day this past week.
    if let Some((day, words)) = written.iter().rev().find(|(d, _)| *d < today && (today - *d).num_days() <= 6) {
        let when = if (today - *day).num_days() == 1 { "yesterday".to_owned() } else { format!("on {}", weekday(day.weekday())) };
        out.push(format!("You wrote {} {} {when}.", thousands(*words), plural(*words, "word")));
    }
    // How far each project with a target has come.
    for p in projects {
        if let Some(target) = p.target.filter(|&t| t > 0 && p.words > 0) {
            let percent = (p.words as u64 * 100 / target as u64).min(100);
            if percent >= 100 {
                out.push(format!("{} has reached its target of {} words.", p.title, thousands(target)));
            } else {
                out.push(format!("{} is {percent}% of the way to its target.", p.title));
            }
        }
    }
    // The longest run of writing days.
    let longest = longest_run(&written);
    if longest >= 3 {
        out.push(format!("Your longest run of writing days is {longest}."));
    }
    // The day of the week written most on (with enough days to say).
    if written.len() >= 10 {
        let mut by_weekday = [0u64; 7];
        for (d, w) in &written {
            by_weekday[d.weekday().num_days_from_monday() as usize] += *w as u64;
        }
        if let Some((i, _)) = by_weekday.iter().enumerate().max_by_key(|(_, w)| **w) {
            let day = Weekday::try_from(i as u8).unwrap_or(Weekday::Mon);
            out.push(format!("You write most on {}s.", weekday(day)));
        }
    }
    // Days written this year.
    let this_year = written.iter().filter(|(d, _)| d.year() == today.year()).count();
    if this_year >= 2 {
        out.push(format!("You have written on {this_year} days this year."));
    }
    // A project left a while. It is still there.
    for p in projects {
        let last = p.stats.days.iter().rev().find(|(_, w)| **w > 0).and_then(|(d, _)| stats::parse_date(d));
        if let Some(last) = last.filter(|d| (today - *d).num_days() >= 14) {
            out.push(format!("{} hasn't been written in since {}. It is still there.", p.title, last.format("%-d %B")));
        }
    }
    // All the words, between them.
    let total: u64 = projects.iter().map(|p| p.words as u64).sum();
    if projects.len() >= 2 && total > 0 {
        out.push(format!("Your projects hold {} words between them.", thousands(total.min(u32::MAX as u64) as u32)));
    }
    out
}

/// The fact for `today`, or the one `skip` places after it (clicked for
/// another). `None` when there are none.
pub fn for_today(facts: &[String], today: NaiveDate, skip: usize) -> Option<&str> {
    if facts.is_empty() {
        return None;
    }
    let day = today.num_days_from_ce().unsigned_abs() as usize;
    facts.get((day + skip) % facts.len()).map(String::as_str)
}

fn longest_run(written: &[(NaiveDate, u32)]) -> u32 {
    let (mut best, mut run, mut prev) = (0, 0, None::<NaiveDate>);
    for (d, _) in written {
        run = if prev.is_some_and(|p| (*d - p).num_days() == 1) { run + 1 } else { 1 };
        best = best.max(run);
        prev = Some(*d);
    }
    best
}

fn weekday(day: Weekday) -> &'static str {
    match day {
        Weekday::Mon => "Monday",
        Weekday::Tue => "Tuesday",
        Weekday::Wed => "Wednesday",
        Weekday::Thu => "Thursday",
        Weekday::Fri => "Friday",
        Weekday::Sat => "Saturday",
        Weekday::Sun => "Sunday",
    }
}

fn plural(n: u32, word: &str) -> String {
    if n == 1 { word.to_owned() } else { format!("{word}s") }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(s: &str) -> NaiveDate {
        stats::parse_date(s).unwrap()
    }

    fn project(title: &str, words: u32, target: Option<u32>, days: &[(&str, i64)]) -> ProjectFacts {
        let stats = Stats { days: days.iter().map(|(d, w)| ((*d).to_owned(), *w)).collect(), ..Default::default() };
        ProjectFacts { title: title.to_owned(), words, target, stats }
    }

    #[test]
    fn facts_come_only_from_numbers_that_are_there() {
        let today = day("2026-11-26"); // a Thursday
        let projects = [
            project("Bellingham", 19_000, Some(50_000), &[("2026-11-23", 800), ("2026-11-24", 1204), ("2026-11-22", 300)]),
            project("Morning Pages", 4_000, None, &[("2026-10-01", 500)]),
        ];
        let facts = all(&projects, today);
        assert!(facts.contains(&"You wrote 1,204 words on Tuesday.".to_owned()), "{facts:?}");
        assert!(facts.contains(&"Bellingham is 38% of the way to its target.".to_owned()));
        assert!(facts.contains(&"Your longest run of writing days is 3.".to_owned()));
        assert!(facts.contains(&"Morning Pages hasn't been written in since 1 October. It is still there.".to_owned()));
        assert!(facts.contains(&"Your projects hold 23,000 words between them.".to_owned()));
        // Too few days to say which weekday's best; no target for Morning Pages.
        assert!(!facts.iter().any(|f| f.starts_with("You write most")));
        assert!(!facts.iter().any(|f| f.starts_with("Morning Pages is")));

        // Nothing written, nothing to say.
        assert!(all(&[project("New", 0, None, &[])], today).is_empty());
    }

    #[test]
    fn the_same_day_gives_the_same_fact() {
        let facts = vec!["a".to_owned(), "b".to_owned(), "c".to_owned()];
        let today = day("2026-11-26");
        assert_eq!(for_today(&facts, today, 0), for_today(&facts, today, 0));
        assert_ne!(for_today(&facts, today, 0), for_today(&facts, today.succ_opt().unwrap(), 0));
        assert_ne!(for_today(&facts, today, 0), for_today(&facts, today, 1));
        assert_eq!(for_today(&[], today, 0), None);
    }
}
