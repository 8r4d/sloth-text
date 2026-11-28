//! The Tracker: who (or what) is where across a folder's scenes, and how
//! the story's threads open and close. Only the working out is here; the
//! grid is drawn by tracker_ui.rs.
//!
//! A profile is *in* a scene when the scene's In This Scene has it. It's
//! *mentioned* when its name turns up in the text without that: a hint,
//! never a change. A thread in a scene opens there, closes there, or (with
//! no mark) moves along.

use std::collections::BTreeMap;

use crate::project::{ThreadMark, TrackRules};

/// A scene, as the tracker needs it.
pub struct Scene {
    pub words: usize,
    /// Its text, notes taken out.
    pub text: String,
    /// The profiles of the kind tracked that its In This Scene has.
    pub links: Vec<String>,
    pub marks: BTreeMap<String, ThreadMark>,
    /// The profiles it has a Tracker note for.
    pub noted: Vec<String>,
}

/// What a cell of the grid shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cell {
    Empty,
    Mentioned,
    In,
    Opens,
    Moves,
    Closes,
}

impl Cell {
    pub fn present(self) -> bool {
        self != Cell::Empty
    }
}

/// The names to look for in the text: the whole name, each word of it
/// four letters or longer that starts with a capital ("Mara" and "Tellin"
/// from Mara Tellin, not "Tam" from Old Tam), and whatever the profile's
/// Also known as says, separated by commas or lines.
pub fn names(title: &str, aliases: &str) -> Vec<String> {
    let mut out = vec![title.trim().to_owned()];
    for word in title.split_whitespace() {
        let word = word.trim_matches(|c: char| !c.is_alphanumeric());
        if word.chars().count() >= 4 && word.chars().next().is_some_and(char::is_uppercase) {
            out.push(word.to_owned());
        }
    }
    out.extend(aliases.split([',', ';', '\n']).map(|a| a.trim().trim_end_matches('.').trim().to_owned()));
    let mut seen: Vec<String> = Vec::new();
    for name in out {
        if !name.is_empty() && !seen.contains(&name) {
            seen.push(name);
        }
    }
    seen
}

/// Whether any of the names is in the text as a whole word (so "Mara" in
/// "Mara's" but not in "Maraschino"). Case counts for a name with a
/// capital; one written in lower case ("the ferryman") is a description,
/// found however it's capitalised (at the start of a sentence, say).
pub fn mentions(text: &str, names: &[String]) -> bool {
    let lower = text.to_ascii_lowercase();
    names.iter().any(|name| {
        let described = name.chars().next().is_some_and(char::is_lowercase);
        let (haystack, needle) = if described { (lower.as_str(), name.to_ascii_lowercase()) } else { (text, name.clone()) };
        haystack.match_indices(needle.as_str()).any(|(at, _)| {
            let before = text[..at].chars().next_back();
            let after = text[at + name.len()..].chars().next();
            !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
        })
    })
}

/// A profile's cell in a scene. `names` empty: no looking for mentions
/// (threads are questions, not names).
pub fn cell(scene: &Scene, id: &str, names: &[String]) -> Cell {
    if scene.links.iter().any(|l| l == id) {
        return match scene.marks.get(id) {
            Some(ThreadMark::Opens) => Cell::Opens,
            Some(ThreadMark::Closes) => Cell::Closes,
            None if names.is_empty() => Cell::Moves,
            None => Cell::In,
        };
    }
    if !names.is_empty() && mentions(&scene.text, names) { Cell::Mentioned } else { Cell::Empty }
}

/// A stretch of scenes without a profile: between scene `after` and scene
/// `before` (indices), how many scenes and words that is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gap {
    pub after: usize,
    pub before: usize,
    pub scenes: usize,
    pub words: usize,
}

/// A row's figures.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    /// Scenes it's in or mentioned in.
    pub scenes: usize,
    pub first: Option<usize>,
    pub last: Option<usize>,
    /// The longest time away between appearances.
    pub gap: Option<Gap>,
}

/// The longest run of scenes between two cells that count as `touched`
/// (by scenes, then words).
fn longest_gap(cells: &[Cell], scenes: &[Scene], touched: impl Fn(Cell) -> bool) -> Option<Gap> {
    let at: Vec<usize> = cells.iter().enumerate().filter(|(_, c)| touched(**c)).map(|(i, _)| i).collect();
    at.windows(2)
        .filter(|w| w[1] > w[0] + 1)
        .map(|w| Gap { after: w[0], before: w[1], scenes: w[1] - w[0] - 1, words: scenes[w[0] + 1..w[1]].iter().map(|s| s.words).sum() })
        .max_by_key(|g| (g.scenes, g.words))
}

pub fn summary(cells: &[Cell], scenes: &[Scene]) -> Summary {
    let present: Vec<usize> = cells.iter().enumerate().filter(|(_, c)| c.present()).map(|(i, _)| i).collect();
    Summary { scenes: present.len(), first: present.first().copied(), last: present.last().copied(), gap: longest_gap(cells, scenes, Cell::present) }
}

/// Scenes in a row an open thread can go untouched before it's called
/// quiet.
pub const QUIET: usize = 8;

/// Something to look at in a row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Issue {
    /// Away for longer than its rules allow (not a thread: see `Quiet`).
    Away(Gap),
    /// Written scenes (by index) with a note for it that don't have it in
    /// In This Scene.
    NotIn(Vec<usize>),
    /// It comes up, but no scene opens it.
    NeverOpened,
    /// A scene closes it before the scene that opens it.
    ClosedBeforeOpened,
    /// Opened, and still open at the end.
    NeverClosed,
    /// Open, and untouched for this long.
    Quiet(Gap),
    /// It closes before this thread, which opened after it: threads
    /// usually close in the reverse of the order they opened.
    OutOfOrder(String),
}

/// Where a thread opens and (after that) closes.
fn span(cells: &[Cell]) -> (Option<usize>, Option<usize>) {
    let opens = cells.iter().position(|c| *c == Cell::Opens);
    let closes = match opens {
        Some(o) => cells[o..].iter().position(|c| *c == Cell::Closes).map(|i| i + o),
        None => None,
    };
    (opens, closes)
}

/// A row, as the checks need it.
pub struct Track<'a> {
    pub id: &'a str,
    pub title: &'a str,
    pub cells: &'a [Cell],
    pub rules: &'a TrackRules,
}

/// What to look at in each row. `threads`: the rows are threads, which
/// open and close; otherwise they come and go.
pub fn issues(rows: &[Track], scenes: &[Scene], threads: bool) -> Vec<Vec<Issue>> {
    let mut all = if threads { thread_issues(rows, scenes) } else { rows.iter().map(|r| away(r, scenes)).collect() };
    for (row, issues) in rows.iter().zip(&mut all) {
        if row.rules.ignore_notes {
            continue;
        }
        // Noted in a written scene that hasn't got it. (In a scene with no
        // words yet, the note is the plan.)
        let missing: Vec<usize> = scenes
            .iter()
            .enumerate()
            .filter(|(i, s)| s.words > 0 && s.noted.iter().any(|n| n == row.id) && !s.links.iter().any(|l| l == row.id) && row.cells.get(*i).is_some())
            .map(|(i, _)| i)
            .collect();
        if !missing.is_empty() {
            issues.push(Issue::NotIn(missing));
        }
    }
    all
}

/// A row away for as long as its rules allow, or longer.
fn away(row: &Track, scenes: &[Scene]) -> Vec<Issue> {
    let Some(limit) = row.rules.away.filter(|n| *n > 0) else { return Vec::new() };
    longest_gap(row.cells, scenes, Cell::present).filter(|g| g.scenes >= limit).map(Issue::Away).into_iter().collect()
}

fn thread_issues(rows: &[Track], scenes: &[Scene]) -> Vec<Vec<Issue>> {
    let spans: Vec<(Option<usize>, Option<usize>)> = rows.iter().map(|r| span(r.cells)).collect();
    rows.iter()
        .zip(&spans)
        .map(|(row, &(opens, closes))| {
            let cells = row.cells;
            let mut issues = Vec::new();
            let first_close = cells.iter().position(|c| *c == Cell::Closes);
            match opens {
                None if cells.iter().any(|c| c.present()) => issues.push(Issue::NeverOpened),
                None => {}
                Some(o) => {
                    if first_close.is_some_and(|c| c < o) {
                        issues.push(Issue::ClosedBeforeOpened);
                    }
                    if closes.is_none() && !row.rules.open_on_purpose {
                        issues.push(Issue::NeverClosed);
                    }
                    // While it's open: from where it opens to where it closes (or the end).
                    let end = closes.unwrap_or(cells.len().saturating_sub(1));
                    let mut open_cells: Vec<Cell> = cells[o..=end.max(o)].to_vec();
                    if closes.is_none() {
                        // The end counts as a touch, so a thread left hanging is quiet up to it.
                        if let Some(last) = open_cells.last_mut() {
                            *last = Cell::Moves;
                        }
                    }
                    let quiet = row.rules.away.unwrap_or(QUIET);
                    if quiet > 0
                        && let Some(gap) = longest_gap(&open_cells, &scenes[o..], Cell::present).filter(|g| g.scenes >= quiet)
                    {
                        issues.push(Issue::Quiet(Gap { after: gap.after + o, before: gap.before + o, ..gap }));
                    }
                }
            }
            issues
        })
        .enumerate()
        .map(|(i, mut issues)| {
            // Nesting: this one opened first, another opened inside it,
            // and this one closes before the other does.
            if let (Some(oa), Some(ca)) = spans[i]
                && !rows[i].rules.any_order
            {
                for (j, &(ob, cb)) in spans.iter().enumerate() {
                    if let (Some(ob), Some(cb)) = (ob, cb)
                        && j != i
                        && oa < ob
                        && ob < ca
                        && ca < cb
                        && !rows[j].rules.any_order
                    {
                        issues.push(Issue::OutOfOrder(rows[j].title.to_owned()));
                    }
                }
            }
            issues
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene(words: usize, text: &str, links: &[&str], marks: &[(&str, ThreadMark)]) -> Scene {
        Scene {
            words,
            text: text.to_owned(),
            links: links.iter().map(|s| s.to_string()).collect(),
            marks: marks.iter().map(|(k, m)| (k.to_string(), *m)).collect(),
            noted: Vec::new(),
        }
    }

    /// The checks on rows named by their titles (their ids too).
    fn check(rows: &[(&str, Vec<Cell>, TrackRules)], scenes: &[Scene], threads: bool) -> Vec<Vec<Issue>> {
        let tracks: Vec<Track> = rows.iter().map(|(t, cells, rules)| Track { id: t, title: t, cells, rules }).collect();
        issues(&tracks, scenes, threads)
    }

    fn plain(rows: Vec<(&str, Vec<Cell>)>) -> Vec<(&str, Vec<Cell>, TrackRules)> {
        rows.into_iter().map(|(t, c)| (t, c, TrackRules::default())).collect()
    }

    #[test]
    fn names_from_the_title_and_also_known_as() {
        assert_eq!(names("Mara Tellin", "the ferryman's daughter, Mar."), ["Mara Tellin", "Mara", "Tellin", "the ferryman's daughter", "Mar"]);
        assert_eq!(names("Old Tam", ""), ["Old Tam"], "no short words alone");
        assert_eq!(names("Will", ""), ["Will"]);
    }

    #[test]
    fn mentions_are_whole_words_with_capitals() {
        let n = names("Mara Tellin", "");
        assert!(mentions("It was Mara's coat.", &n));
        assert!(mentions("Ask [[Mara Tellin]].", &n));
        assert!(!mentions("A maraschino cherry, and Marat.", &n));
        assert!(!mentions("mara, in lower case", &n));
        let tam = names("Old Tam", "the ferryman");
        assert!(mentions("The ferryman laughed.", &tam));
        assert!(!mentions("The ferrymen laughed.", &tam));
    }

    #[test]
    fn cells_in_mentioned_and_empty() {
        let n = names("Mara", "");
        assert_eq!(cell(&scene(10, "", &["m"], &[]), "m", &n), Cell::In);
        assert_eq!(cell(&scene(10, "Mara ran.", &[], &[]), "m", &n), Cell::Mentioned);
        assert_eq!(cell(&scene(10, "Nobody.", &[], &[]), "m", &n), Cell::Empty);
        // Threads: marks, or moving along.
        assert_eq!(cell(&scene(10, "", &["t"], &[("t", ThreadMark::Opens)]), "t", &[]), Cell::Opens);
        assert_eq!(cell(&scene(10, "", &["t"], &[]), "t", &[]), Cell::Moves);
    }

    #[test]
    fn the_longest_absence() {
        let scenes: Vec<Scene> = [100, 200, 300, 400, 500, 600].iter().map(|w| scene(*w, "", &[], &[])).collect();
        use Cell::*;
        let cells = [Empty, In, Empty, Empty, In, Mentioned];
        let s = summary(&cells, &scenes);
        assert_eq!((s.scenes, s.first, s.last), (3, Some(1), Some(5)));
        assert_eq!(s.gap, Some(Gap { after: 1, before: 4, scenes: 2, words: 700 }));
        assert_eq!(summary(&[In, In], &scenes[..2]).gap, None);
    }

    #[test]
    fn threads_that_dont_open_or_close() {
        let scenes: Vec<Scene> = (0..12).map(|_| scene(1000, "", &[], &[])).collect();
        use Cell::*;
        let mut hanging = vec![Empty; 12];
        hanging[1] = Opens;
        let mut never_opened = vec![Empty; 12];
        never_opened[2] = Moves;
        let mut backwards = vec![Empty; 12];
        backwards[3] = Closes;
        backwards[4] = Opens;
        backwards[5] = Closes;
        let rows = plain(vec![("Hanging", hanging.clone()), ("Never opened", never_opened), ("Backwards", backwards)]);
        let issues = check(&rows, &scenes, true);
        assert_eq!(issues[0], [Issue::NeverClosed, Issue::Quiet(Gap { after: 1, before: 11, scenes: 9, words: 9000 })]);
        assert_eq!(issues[1], [Issue::NeverOpened]);
        assert_eq!(issues[2], [Issue::ClosedBeforeOpened]);

        // Left open on purpose, and allowed to be quiet longer: nothing to say.
        let rules = TrackRules { open_on_purpose: true, away: Some(10), ..TrackRules::default() };
        assert!(check(&[("Hanging", hanging.clone(), rules)], &scenes, true)[0].is_empty());
        // Quiet never counts.
        let rules = TrackRules { away: Some(0), ..TrackRules::default() };
        assert_eq!(check(&[("Hanging", hanging, rules)], &scenes, true)[0], [Issue::NeverClosed]);
    }

    #[test]
    fn threads_close_in_reverse_order() {
        let scenes: Vec<Scene> = (0..6).map(|_| scene(100, "", &[], &[])).collect();
        use Cell::*;
        // Outer opens 0, inner opens 1; outer closes 3, inner closes 4: crossed.
        let outer = vec![Opens, Empty, Moves, Closes, Empty, Empty];
        let inner = vec![Empty, Opens, Empty, Empty, Closes, Empty];
        let rows = plain(vec![("Outer", outer.clone()), ("Inner", inner.clone())]);
        let issues = check(&rows, &scenes, true);
        assert_eq!(issues[0], [Issue::OutOfOrder("Inner".to_owned())]);
        assert!(issues[1].is_empty());
        // Nested properly: nothing to say.
        let nested = vec![Empty, Opens, Closes, Empty, Empty, Empty];
        let issues = check(&plain(vec![("Outer", outer.clone()), ("Inner", nested)]), &scenes, true);
        assert!(issues.iter().all(Vec::is_empty), "{issues:?}");
        // Either may close in any order: nothing to say.
        let any = TrackRules { any_order: true, ..TrackRules::default() };
        let issues = check(&[("Outer", outer, TrackRules::default()), ("Inner", inner, any)], &scenes, true);
        assert!(issues.iter().all(Vec::is_empty), "{issues:?}");
    }

    #[test]
    fn away_too_long_only_when_asked() {
        let scenes: Vec<Scene> = (0..8).map(|_| scene(100, "", &[], &[])).collect();
        use Cell::*;
        let mara = vec![In, Empty, Empty, Empty, Empty, In, Empty, Empty];
        assert!(check(&plain(vec![("Mara", mara.clone())]), &scenes, false)[0].is_empty(), "no limit: no warning");
        let rules = TrackRules { away: Some(4), ..TrackRules::default() };
        assert_eq!(check(&[("Mara", mara.clone(), rules)], &scenes, false)[0], [Issue::Away(Gap { after: 0, before: 5, scenes: 4, words: 400 })]);
        let rules = TrackRules { away: Some(5), ..TrackRules::default() };
        assert!(check(&[("Mara", mara, rules)], &scenes, false)[0].is_empty());
    }

    #[test]
    fn notes_in_written_scenes_that_dont_have_it() {
        let mut scenes = vec![scene(100, "", &["Mara"], &[]), scene(100, "", &[], &[]), scene(0, "", &[], &[])];
        for s in &mut scenes {
            s.noted = vec!["Mara".to_owned()];
        }
        use Cell::*;
        let mara = vec![In, Empty, Empty];
        // Noted in all three: in the first, missing from the second; the third's only planned.
        assert_eq!(check(&plain(vec![("Mara", mara.clone())]), &scenes, false)[0], [Issue::NotIn(vec![1])]);
        let rules = TrackRules { ignore_notes: true, ..TrackRules::default() };
        assert!(check(&[("Mara", mara, rules)], &scenes, false)[0].is_empty());
    }
}
