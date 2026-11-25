//! Rewards that grow: a ladder on top of the day's cheer. Writing days in a
//! row (3, 7, 14, 30) and the manuscript's way to its target (25%, 50%, 75%,
//! and reaching it) each earn a small prize, once, remembered in the
//! project's statistics with the day. See SEVERANCE-UI.md, part 2.

use serde::{Deserialize, Serialize};

/// A reward earned, and the day ("YYYY-MM-DD").
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Earned {
    pub key: String,
    pub day: String,
}

/// One rung of the ladder.
pub struct Reward {
    pub key: &'static str,
    /// What it's for, and the prize.
    pub title: &'static str,
    pub prize: &'static str,
    pub line: &'static str,
    /// Days in a row, or percent of the target.
    kind: Kind,
    at: u32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Streak,
    Manuscript,
}

pub const LADDER: &[Reward] = &[
    Reward { key: "streak-3", title: "Three days running", prize: crate::icons::LEAF, line: "A leaf, for keeping at it.", kind: Kind::Streak, at: 3 },
    Reward { key: "streak-7", title: "A week of writing", prize: crate::icons::FLOWER, line: "A hibiscus: seven days in a row.", kind: Kind::Streak, at: 7 },
    Reward { key: "streak-14", title: "Two weeks running", prize: crate::icons::TEA, line: "A cup of tea, and fourteen days.", kind: Kind::Streak, at: 14 },
    Reward { key: "streak-30", title: "Thirty days", prize: crate::icons::MEDAL, line: "A medal. Hang it somewhere slow.", kind: Kind::Streak, at: 30 },
    Reward { key: "manuscript-25", title: "A quarter of the way", prize: crate::icons::SEEDLING, line: "The manuscript is a quarter written.", kind: Kind::Manuscript, at: 25 },
    Reward { key: "manuscript-50", title: "Halfway", prize: crate::icons::SPRIG, line: "The manuscript is half written.", kind: Kind::Manuscript, at: 50 },
    Reward { key: "manuscript-75", title: "Three quarters", prize: crate::icons::TREE, line: "Three quarters written. The end is in sight.", kind: Kind::Manuscript, at: 75 },
    Reward { key: "manuscript-100", title: "The target, reached", prize: crate::icons::PARTY, line: "Every word of it. Take the rest of the day.", kind: Kind::Manuscript, at: 100 },
];

pub fn find(key: &str) -> Option<&'static Reward> {
    LADDER.iter().find(|r| r.key == key)
}

/// The party: reaching the manuscript's target.
pub fn is_finale(key: &str) -> bool {
    key == "manuscript-100"
}

/// What's newly earned with this streak and this percent of the target:
/// the reward to show (the highest new one), and every new key to record.
/// Lower rungs passed at the same time are recorded quietly, so a long
/// streak that was already running earns one card, not several.
pub fn due(streak: u32, percent: Option<u32>, earned: &[Earned]) -> (Option<&'static Reward>, Vec<&'static str>) {
    let has = |key: &str| earned.iter().any(|e| e.key == key);
    let mut new = Vec::new();
    let mut show = None;
    for kind in [Kind::Manuscript, Kind::Streak] {
        let value = match kind {
            Kind::Streak => Some(streak),
            Kind::Manuscript => percent,
        };
        let Some(value) = value else { continue };
        let reached: Vec<&Reward> = LADDER.iter().filter(|r| r.kind == kind && value >= r.at && !has(r.key)).collect();
        if let Some(top) = reached.last() {
            show = show.or(Some(*top));
        }
        new.extend(reached.iter().map(|r| r.key));
    }
    (show, new)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn earned(keys: &[&str]) -> Vec<Earned> {
        keys.iter().map(|k| Earned { key: (*k).to_owned(), day: "2026-11-25".into() }).collect()
    }

    #[test]
    fn each_rung_once_and_the_highest_shown() {
        assert!(due(2, None, &[]).0.is_none());
        let (show, new) = due(3, None, &[]);
        assert_eq!(show.map(|r| r.key), Some("streak-3"));
        assert_eq!(new, ["streak-3"]);
        // Already had: nothing.
        assert!(due(3, None, &earned(&["streak-3"])).1.is_empty());
        // A ten-day streak when rewards arrive: the week's shown, three's kept quietly.
        let (show, new) = due(10, None, &[]);
        assert_eq!(show.map(|r| r.key), Some("streak-7"));
        assert_eq!(new, ["streak-3", "streak-7"]);
    }

    #[test]
    fn the_manuscript_comes_first_and_its_finale_is_a_party() {
        let (show, new) = due(7, Some(100), &earned(&["streak-3", "manuscript-25", "manuscript-50", "manuscript-75"]));
        assert_eq!(show.map(|r| r.key), Some("manuscript-100"));
        assert_eq!(new, ["manuscript-100", "streak-7"]);
        assert!(is_finale("manuscript-100"));
        // No target, no manuscript rewards.
        assert!(due(0, None, &[]).1.is_empty());
    }
}
