//! Grammar: Harper's checks (https://writewithharper.com), on this
//! computer, no AI: "should of" → "should have", "their was", "buyed".
//! Spelling is left to the Mac's spell checker, so Harper's is off.
//!
//! A text is checked a paragraph at a time and each paragraph's findings
//! remembered, so as you type only the paragraph you're in is checked
//! again. Notes, comments and references aren't prose and are left out;
//! headings and scene breaks too.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::Range;
use std::sync::Arc;
use std::time::{Duration, Instant};

use harper_core::linting::{LintGroup, Suggestion};
use harper_core::parsers::Markdown;
use harper_core::spell::FstDictionary;
use harper_core::{Dialect, Document};

/// The kinds of English Harper knows, by name, the first the usual one.
pub const DIALECTS: [&str; 5] = ["Canadian", "American", "British", "Australian", "Indian"];

fn dialect(name: &str) -> Dialect {
    match name {
        "American" => Dialect::American,
        "British" => Dialect::British,
        "Australian" => Dialect::Australian,
        "Indian" => Dialect::Indian,
        _ => Dialect::Canadian,
    }
}

/// Harper's rules that are always off: spelling, which the Mac's spell
/// checker already does.
const ALWAYS_OFF: [&str; 2] = ["SpellCheck", "Misspell"];

/// Something Harper found: where (bytes in the text), which rule, what it
/// says, and what could go in its place ("" to remove it).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    pub range: Range<usize>,
    pub rule: String,
    pub message: String,
    pub options: Vec<String>,
}

/// Harper, set up: its dictionary, its rules, and what it's found in each
/// paragraph (by the paragraph's text).
pub struct Checker {
    dict: Arc<FstDictionary>,
    linter: LintGroup,
    dialect: String,
    ignored: Vec<String>,
    /// Each paragraph's findings, by a hash of its text, places counted
    /// from the paragraph's start.
    seen: HashMap<u64, Vec<Found>>,
}

/// Paragraphs remembered before the oldest are let go.
const REMEMBER: usize = 20_000;

impl Checker {
    /// Loading the dictionary takes a moment: do it on a thread.
    pub fn new(dialect_name: &str, ignored: &[String]) -> Self {
        let dict = FstDictionary::curated();
        let linter = linter(&dict, dialect_name, ignored);
        Checker { dict, linter, dialect: dialect_name.to_owned(), ignored: ignored.to_vec(), seen: HashMap::new() }
    }

    pub fn dialect(&self) -> &str {
        &self.dialect
    }

    /// Rules to leave out (Ignore Suggestions Like This): changing them
    /// checks everything again.
    pub fn set_ignored(&mut self, ignored: &[String]) {
        if self.ignored != ignored {
            self.linter = linter(&self.dict, &self.dialect, ignored);
            self.ignored = ignored.to_vec();
            self.seen.clear();
        }
    }

    /// What Harper finds in `text`, in order. Paragraphs it hasn't seen are
    /// checked until `budget` runs out; the second value says whether
    /// it got to them all (if not, ask again).
    pub fn check(&mut self, text: &str, budget: Duration) -> (Vec<Found>, bool) {
        let started = Instant::now();
        let asides = crate::repetition::asides(text);
        let mut out = Vec::new();
        let mut done = true;
        let mut line_start = 0;
        if self.seen.len() > REMEMBER {
            self.seen.clear();
        }
        for line in text.split_inclusive('\n') {
            let start = line_start;
            line_start += line.len();
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') || crate::compile::is_scene_break(trimmed) {
                continue;
            }
            let line = line.trim_end_matches(['\n', '\r']);
            // Notes and references blanked out, a space for each character,
            // so Harper's places still line up.
            let prose: String = line.char_indices().map(|(i, c)| if asides.iter().any(|r| r.contains(&(start + i))) { ' ' } else { c }).collect();
            if prose.trim().is_empty() {
                continue;
            }
            let mut hasher = DefaultHasher::new();
            prose.hash(&mut hasher);
            let key = hasher.finish();
            if !self.seen.contains_key(&key) {
                if started.elapsed() > budget {
                    done = false;
                    continue;
                }
                let found = self.check_paragraph(&prose);
                self.seen.insert(key, found);
            }
            out.extend(self.seen[&key].iter().map(|f| Found { range: f.range.start + start..f.range.end + start, ..f.clone() }));
        }
        (out, done)
    }

    fn check_paragraph(&mut self, prose: &str) -> Vec<Found> {
        let document = Document::new(prose, &Markdown::default(), &self.dict);
        // Characters to bytes.
        let bytes: Vec<usize> = prose.char_indices().map(|(b, _)| b).chain(std::iter::once(prose.len())).collect();
        let to_byte = |c: usize| bytes.get(c).copied().unwrap_or(prose.len());
        let mut found: Vec<Found> = Vec::new();
        for (rule, lints) in self.linter.organized_lints(&document) {
            for lint in lints {
                let range = to_byte(lint.span.start)..to_byte(lint.span.end);
                if range.is_empty() {
                    continue;
                }
                let was = &prose[range.clone()];
                let mut options: Vec<String> = Vec::new();
                for s in &lint.suggestions {
                    let option = match s {
                        Suggestion::ReplaceWith(chars) => chars.iter().collect(),
                        Suggestion::InsertAfter(chars) => format!("{was}{}", chars.iter().collect::<String>()),
                        Suggestion::Remove => String::new(),
                    };
                    if option != was && !options.contains(&option) {
                        options.push(option);
                    }
                }
                // The same words found twice (by two rules): one finding, both rules' choices.
                if let Some(same) = found.iter_mut().find(|f| f.range == range) {
                    for o in options {
                        if !same.options.contains(&o) {
                            same.options.push(o);
                        }
                    }
                    continue;
                }
                found.push(Found { range, rule: rule.clone(), message: quoted(&lint.message), options });
            }
        }
        found.sort_by_key(|f| f.range.start);
        found
    }
}

fn linter(dict: &Arc<FstDictionary>, dialect_name: &str, ignored: &[String]) -> LintGroup {
    let mut linter = LintGroup::new_curated(dict.clone(), dialect(dialect_name));
    for rule in ALWAYS_OFF.iter().map(|r| r.to_string()).chain(ignored.iter().cloned()) {
        linter.config.set_rule_enabled(rule, false);
    }
    linter
}

/// Harper's messages put words in `backticks`: curly quotes instead.
fn quoted(message: &str) -> String {
    let mut out = String::new();
    let mut open = true;
    for c in message.chars() {
        if c == '`' {
            out.push(if open { '\u{201c}' } else { '\u{201d}' });
            open = !open;
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checker() -> Checker {
        Checker::new("Canadian", &[])
    }

    #[test]
    fn finds_grammar_not_spelling_with_its_fixes() {
        let mut c = checker();
        let text = "Mara waited. She should of known better.\n\nHe buyed bread. The colour was grey and she realised it.\n";
        let (found, done) = c.check(text, Duration::from_secs(10));
        assert!(done);
        let at = |words: &str| found.iter().find(|f| &text[f.range.clone()] == words).unwrap_or_else(|| panic!("{words}: {found:?}"));
        assert!(at("should of").options.contains(&"should have".to_owned()));
        assert_eq!(at("buyed").options, ["bought"], "one finding for two rules");
        // Spelling is the Mac's: realised and colour are fine here.
        assert!(!found.iter().any(|f| ["realised", "colour", "grey"].contains(&&text[f.range.clone()])), "{found:?}");
        assert!(!at("should of").message.contains('`'));
    }

    #[test]
    fn notes_headings_and_breaks_are_left_out() {
        let mut c = checker();
        let text = "# She should of known\n\n<!-- she should of known --> Fine.\n\n* * *\n";
        let (found, _) = c.check(text, Duration::from_secs(10));
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn a_paragraph_once_checked_is_remembered() {
        let mut c = checker();
        let para = "She should of known. ";
        let text = format!("{}\n", para.repeat(5)).repeat(50);
        let (first, done) = c.check(&text, Duration::from_secs(30));
        assert!(done);
        assert_eq!(first.len(), 250);
        // Nothing new to check: no time needed at all.
        let (again, done) = c.check(&text, Duration::ZERO);
        assert!(done);
        assert_eq!(again, first);
        // A new paragraph with no time left: the rest are still there.
        let more = format!("{text}He buyed bread.\n");
        let (partial, done) = c.check(&more, Duration::ZERO);
        assert!(!done);
        assert_eq!(partial.len(), 250);
    }

    #[test]
    fn ignored_rules_stay_quiet() {
        let mut c = checker();
        let text = "She should of known.\n";
        let (found, _) = c.check(text, Duration::from_secs(10));
        let rule = found[0].rule.clone();
        c.set_ignored(&[rule]);
        let (found, _) = c.check(text, Duration::from_secs(10));
        assert!(found.is_empty(), "{found:?}");
    }
}
