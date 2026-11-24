//! Overused words: echoes (a word used again within a short stretch of
//! text) and a watch list of words and phrases the writer leans on, for the
//! editor to mark; and the manuscript's most used words, for Statistics.
//! Common little words don't count, and nor do notes (`<!-- -->`) or
//! references (`[[…]]`).

use std::collections::{HashMap, HashSet};
use std::ops::Range;

/// How close (in words) a word has to come again to be an echo.
pub const ECHO_WINDOW: usize = 100;
/// Shorter words are never echoes (too many of them are ordinary).
const MIN_LETTERS: usize = 4;

/// Words too ordinary to count as echoes or in the most used.
const COMMON: &[&str] = &[
    "about", "above", "after", "again", "against", "also", "although", "always", "among", "another", "anything", "around",
    "back", "because", "been", "before", "being", "below", "between", "both", "came", "come", "could", "didn't", "does",
    "doesn't", "doing", "done", "down", "during", "each", "even", "ever", "every", "from", "further", "gave", "give",
    "going", "gone", "have", "having", "he'd", "he's", "hers", "herself", "here", "himself", "into", "it's", "itself",
    "just", "know", "knew", "like", "made", "make", "many", "more", "most", "much", "must", "myself", "never", "next",
    "nothing", "once", "only", "other", "ought", "ours", "ourselves", "over", "same", "said", "says", "she'd", "she's",
    "should", "since", "some", "something", "still", "such", "than", "that", "that's", "their", "theirs", "them",
    "themselves", "then", "there", "there's", "these", "they", "they'd", "they're", "thing", "things", "this", "those",
    "though", "through", "time", "told", "took", "toward", "towards", "under", "until", "upon", "very", "want", "wanted",
    "wasn't", "we're", "well", "went", "were", "weren't", "what", "what's", "when", "where", "which", "while", "will",
    "with", "without", "won't", "would", "wouldn't", "you'd", "you're", "your", "yours", "yourself",
];

/// The watch list a project starts with.
pub fn default_watch_list() -> Vec<String> {
    ["just", "really", "very", "suddenly", "actually", "quite", "rather", "somehow", "seemed to", "began to", "started to", "a bit"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

/// What's marked in a text: echoes, and watch-list words (byte ranges).
#[derive(Debug, Default, PartialEq)]
pub struct Found {
    pub echoes: Vec<Range<usize>>,
    pub watched: Vec<Range<usize>>,
}

/// The parts of a text that aren't prose: notes and references.
fn asides(text: &str) -> Vec<Range<usize>> {
    let mut out: Vec<Range<usize>> = sloth_core::markdown::references(text).into_iter().map(|(r, _)| r).collect();
    // A comment's words (its passage is prose).
    out.extend(sloth_core::markdown::comments(text).into_iter().map(|c| c.words.start - 3..c.words.end + 3));
    let mut from = 0;
    while let Some(start) = text[from..].find("<!--").map(|i| from + i) {
        let end = text[start..].find("-->").map_or(text.len(), |i| start + i + 3);
        out.push(start..end);
        from = end;
    }
    out
}

/// The words of a text, in order, lower case, with where each is. Inside
/// notes and references don't count.
fn words(text: &str) -> Vec<(Range<usize>, String)> {
    let skip = asides(text);
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    let is_word = |c: char| c.is_alphabetic();
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    for (n, &(i, c)) in chars.iter().enumerate() {
        // An apostrophe inside a word ("didn't", "she’d") belongs to it.
        let inner = (c == '\'' || c == '\u{2019}')
            && start.is_some()
            && chars.get(n + 1).is_some_and(|&(_, next)| is_word(next));
        if is_word(c) || inner {
            start.get_or_insert(i);
        } else if let Some(s) = start.take() {
            out.push(s..i);
        }
    }
    if let Some(s) = start {
        out.push(s..text.len());
    }
    out.into_iter()
        .filter(|r| !skip.iter().any(|a| a.start <= r.start && r.end <= a.end))
        .map(|r| {
            let word = text[r.clone()].to_lowercase().replace('\u{2019}', "'");
            (r, word)
        })
        .collect()
}

/// Common words: the built-in ones, and the writer's own (any case, and
/// either apostrophe).
pub struct Common(HashSet<String>);

impl Common {
    pub fn new(own: &[String]) -> Self {
        Self(own.iter().map(|w| w.trim().to_lowercase().replace('\u{2019}', "'")).filter(|w| !w.is_empty()).collect())
    }

    /// Whether a word (lower case) counts: long enough, and not common.
    fn counts(&self, word: &str) -> bool {
        word.chars().count() >= MIN_LETTERS && !COMMON.contains(&word) && !self.0.contains(word)
    }
}

/// Echoes and watched words in `text`. `watch`: words or phrases, matched
/// whole and without regard to case. Common words are never echoes.
pub fn find(text: &str, watch: &[String], common: &Common) -> Found {
    let words = words(text);
    let mut found = Found::default();
    // Echoes: both the word and its return within the window.
    let mut last: HashMap<&str, usize> = HashMap::new();
    let mut echo = vec![false; words.len()];
    for (i, (_, word)) in words.iter().enumerate() {
        if !common.counts(word) {
            continue;
        }
        if let Some(&before) = last.get(word.as_str())
            && i - before <= ECHO_WINDOW
        {
            echo[before] = true;
            echo[i] = true;
        }
        last.insert(word, i);
    }
    found.echoes = words.iter().zip(&echo).filter(|(_, e)| **e).map(|((r, _), _)| r.clone()).collect();
    // The watch list, a phrase at a time.
    let phrases: Vec<Vec<String>> = watch
        .iter()
        .map(|p| p.split_whitespace().map(|w| w.to_lowercase().replace('\u{2019}', "'")).collect::<Vec<_>>())
        .filter(|p| !p.is_empty())
        .collect();
    for i in 0..words.len() {
        for phrase in &phrases {
            let n = phrase.len();
            if i + n <= words.len() && words[i..i + n].iter().zip(phrase).all(|((_, w), p)| w == p) {
                found.watched.push(words[i].0.start..words[i + n - 1].0.end);
            }
        }
    }
    found
}

/// The most used words across some texts (common ones left out), most
/// first, at most `n`.
pub fn most_used<'a>(texts: impl IntoIterator<Item = &'a str>, n: usize, common: &Common) -> Vec<(String, usize)> {
    let mut tally: HashMap<String, usize> = HashMap::new();
    for text in texts {
        for (_, word) in words(text) {
            if common.counts(&word) {
                *tally.entry(word).or_default() += 1;
            }
        }
    }
    let mut list: Vec<(String, usize)> = tally.into_iter().filter(|(_, c)| *c > 1).collect();
    list.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    list.truncate(n);
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn your_own_common_words_are_left_out_too() {
        let text = "People said the people here were people of the harbour. The harbour, people!";
        let none = Common::new(&[]);
        assert_eq!(most_used([text], 5, &none), [("people".to_owned(), 4), ("harbour".to_owned(), 2)]);
        let mine = Common::new(&["  PEOPLE ".to_owned(), String::new()]);
        assert_eq!(most_used([text], 5, &mine), [("harbour".to_owned(), 2)]);
        // Nor are they echoes.
        let echoes = find(text, &[], &mine).echoes;
        assert!(echoes.iter().all(|r| text[r.clone()].to_lowercase() != "people"));
        assert!(!echoes.is_empty(), "harbour still is");
        // A curly apostrophe matches a straight one.
        let text = "Mara’s boat. Mara’s oars.";
        assert!(most_used([text], 5, &Common::new(&["mara's".to_owned()])).is_empty());
    }

    fn marked<'a>(text: &'a str, ranges: &[Range<usize>]) -> Vec<&'a str> {
        ranges.iter().map(|r| &text[r.clone()]).collect()
    }

    #[test]
    fn echoes_are_words_back_too_soon() {
        let text = "The harbour froze. Nobody came to the Harbour, and the snow fell on the harbour wall.";
        let found = find(text, &[], &Common::new(&[]));
        assert_eq!(marked(text, &found.echoes), ["harbour", "Harbour", "harbour"]);
        // Common and short words never are.
        assert!(!marked(text, &found.echoes).contains(&"the"));

        // Too far apart to notice.
        let far = format!("lantern {} lantern", "the ".repeat(ECHO_WINDOW + 5));
        assert!(find(&far, &[], &Common::new(&[])).echoes.is_empty());
    }

    #[test]
    fn the_watch_list_finds_words_and_phrases_whole() {
        let text = "She just began to run. Justice was suddenly Suddenly over. It began.";
        let watch: Vec<String> = ["just", "suddenly", "began to"].iter().map(|s| s.to_string()).collect();
        let found = find(text, &watch, &Common::new(&[]));
        assert_eq!(marked(text, &found.watched), ["just", "began to", "suddenly", "Suddenly"]);
    }

    #[test]
    fn notes_references_and_apostrophes() {
        let text = "Mara's lantern. <!-- lantern lantern --> [[Lantern]] The lantern didn’t didn't.";
        let found = find(text, &[], &Common::new(&[]));
        assert_eq!(marked(text, &found.echoes), ["lantern", "lantern"], "not in the note or the reference");
        assert_eq!(most_used([text], 5, &Common::new(&[])), [("lantern".to_owned(), 2)]);
    }

    #[test]
    fn most_used_counts_across_texts() {
        let a = "The ferry left. The ferry came back. Snow, snow, snow.";
        let b = "A ferry. Lanterns.";
        assert_eq!(most_used([a, b], 1, &Common::new(&[])), [("ferry".to_owned(), 3)]);
        assert_eq!(most_used([a, b], 5, &Common::new(&[])), [("ferry".to_owned(), 3), ("snow".to_owned(), 3)]);
    }
}
