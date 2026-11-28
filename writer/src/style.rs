//! Style: things worth a second look in a text, found by word lists and
//! simple rules (no AI): clichés, wordy phrases, redundancies, hedges, long
//! sentences, sentences starting alike, passive voice, adverbs, fancy
//! dialogue tags, and a slip out of the text's tense. Hints, never fixes:
//! each says what it found and, where there is one, a shorter way.
//!
//! Notes, comments, references, headings and scene breaks aren't prose and
//! aren't looked at. What's inside quotation marks is dialogue: people
//! talk in clichés and change tense, so most checks leave it alone.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::LazyLock;

/// A kind of thing to look for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    /// Harper's grammar checks (see grammar.rs), not found here.
    Grammar,
    Cliche,
    Wordy,
    Redundant,
    Hedge,
    Long,
    Openers,
    Tense,
    Passive,
    Adverb,
    Tags,
}

impl Category {
    pub const ALL: [Category; 11] = [
        Category::Grammar,
        Category::Cliche,
        Category::Wordy,
        Category::Redundant,
        Category::Hedge,
        Category::Long,
        Category::Openers,
        Category::Tense,
        Category::Passive,
        Category::Adverb,
        Category::Tags,
    ];

    /// Its name in settings (kept as written, so don't change these).
    pub fn id(self) -> &'static str {
        match self {
            Category::Grammar => "grammar",
            Category::Cliche => "cliches",
            Category::Wordy => "wordy",
            Category::Redundant => "redundant",
            Category::Hedge => "hedges",
            Category::Long => "long",
            Category::Openers => "openers",
            Category::Tense => "tense",
            Category::Passive => "passive",
            Category::Adverb => "adverbs",
            Category::Tags => "tags",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Category::Grammar => "Grammar",
            Category::Cliche => "Clichés",
            Category::Wordy => "Wordy phrases",
            Category::Redundant => "Redundancies",
            Category::Hedge => "Hedges",
            Category::Long => "Long sentences",
            Category::Openers => "Same openings",
            Category::Tense => "Tense slips",
            Category::Passive => "Passive voice",
            Category::Adverb => "Adverbs",
            Category::Tags => "Dialogue tags",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Category::Grammar => "Grammar and word choice, by Harper, on this computer: \u{201c}should of\u{201d} \u{2192} \u{201c}should have\u{201d}, \u{201c}their was\u{201d}, \u{201c}buyed\u{201d}. Right-click a mark to change it.",
            Category::Cliche => "Phrases worn smooth: \u{201c}at the end of the day\u{201d}, \u{201c}a shiver down her spine\u{201d}. Say it your way.",
            Category::Wordy => "Long ways of saying short things, with the short one: \u{201c}in order to\u{201d} \u{2192} \u{201c}to\u{201d}.",
            Category::Redundant => "Words that say the same thing twice: \u{201c}free gift\u{201d}, \u{201c}nodded his head\u{201d}.",
            Category::Hedge => "Softeners: \u{201c}sort of\u{201d}, \u{201c}perhaps\u{201d}, \u{201c}I think\u{201d}. One can be honest; several sound unsure.",
            Category::Long => "Sentences over 30 words, shaded. Some earn it; a run of them tires the reader.",
            Category::Openers => "Three or more sentences in a row starting with the same word.",
            Category::Tense => "A sentence in present tense where most of the text is past, or the other way. Dialogue doesn't count.",
            Category::Passive => "\u{201c}Was taken\u{201d}, \u{201c}is being built\u{201d}: who did it? Sometimes that's the point. (Off at first.)",
            Category::Adverb => "Words ending in -ly. A stronger verb often does the work. (Off at first.)",
            Category::Tags => "Dialogue tags other than said and asked (hissed, exclaimed), and said with an adverb. (Off at first.)",
        }
    }

    /// Whether it's on before the writer chooses: the noisier ones aren't.
    pub fn on_at_first(self) -> bool {
        !matches!(self, Category::Passive | Category::Adverb | Category::Tags)
    }
}

/// Something found: where, what kind, and what to say about it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub range: Range<usize>,
    pub category: Category,
    pub note: String,
}

/// More words than this and a sentence is long.
pub const LONG: usize = 30;

// ----- the lists -------------------------------------------------------------

const CLICHES: &[&str] = &[
    "a chill ran down", "a perfect storm", "all hell broke loose", "all walks of life", "as luck would have it", "at a loss for words",
    "at the end of the day", "at the speed of light", "avoid like the plague", "bated breath", "better late than never", "blink of an eye",
    "blood ran cold", "by the skin of", "can of worms", "champing at the bit", "chomping at the bit", "cold as ice", "crystal clear",
    "dark and stormy night", "dead as a doornail", "dead of night", "deep dive", "didn't know she was holding", "didn't know he was holding",
    "didn't know i was holding", "easier said than done", "elephant in the room", "every cloud has a silver lining", "eyes like saucers",
    "few and far between", "fit as a fiddle", "game changer", "head over heels", "heart of gold", "heat of the moment", "hit the ground running",
    "in the nick of time", "in this day and age", "it goes without saying", "last but not least", "level playing field",
    "light at the end of the tunnel", "low-hanging fruit", "low hanging fruit", "long story short", "lost track of time", "method to the madness",
    "method to his madness", "method to her madness", "moment of truth", "more than meets the eye", "move the needle", "needle in a haystack",
    "nerves of steel", "not a moment too soon", "on the same page", "once in a blue moon", "only time will tell", "out of the blue",
    "paint the town red", "pale as a ghost", "par for the course", "paradigm shift", "plenty of fish in the sea", "push the envelope",
    "pushing the envelope", "quiet as a mouse", "raining cats and dogs", "read between the lines", "rule of thumb", "scared to death",
    "shadow of a doubt", "shiver down her spine", "shiver down his spine", "shiver down my spine", "shivers down her spine",
    "shivers down his spine", "shivers down my spine", "sigh of relief", "silver lining", "skipped a beat", "slept like a log",
    "sleep like a log", "smart as a whip", "so far so good", "sore thumb", "stood the test of time", "stand the test of time",
    "take the bull by the horns", "the bottom line", "the last straw", "the whole nine yards", "think outside the box", "threw caution to the wind",
    "throw caution to the wind", "time stood still", "tip of the iceberg", "tried and true", "twist of fate", "under the weather",
    "until the cows come home", "when all is said and done", "when push comes to shove", "without further ado", "worth its weight in gold",
    "writing on the wall",
];

const WORDY: &[(&str, &str)] = &[
    ("a large number of", "many"), ("a majority of", "most"), ("the majority of", "most"), ("a number of", "some, or several"),
    ("all of a sudden", "suddenly, or cut it"), ("along the lines of", "like"), ("are able to", "can"), ("is able to", "can"),
    ("as a matter of fact", "in fact"), ("at all times", "always"), ("at that point in time", "then"), ("at the present time", "now"),
    ("at this point in time", "now"), ("came to the conclusion", "concluded"), ("come to the conclusion", "conclude"),
    ("despite the fact that", "although"), ("due to the fact that", "because"), ("during the course of", "during"), ("each and every", "each"),
    ("first and foremost", "first"), ("for all intents and purposes", "cut it"), ("for the purpose of", "to, or for"),
    ("give consideration to", "consider"), ("has the ability to", "can"), ("have the ability to", "can"), ("in addition to", "besides"),
    ("in close proximity to", "near"), ("in light of the fact that", "because"), ("in order to", "to"), ("in regard to", "about"),
    ("in relation to", "about"), ("in spite of the fact that", "although"), ("in the course of", "during"), ("in the event that", "if"),
    ("in the near future", "soon"), ("in the process of", "cut it"), ("in terms of", "rephrase, or cut it"), ("is indicative of", "shows"),
    ("it is important to note that", "cut it"), ("it should be noted that", "cut it"), ("made a decision", "decided"), ("make a decision", "decide"),
    ("on a daily basis", "daily"), ("on a regular basis", "regularly"), ("on the grounds that", "because"), ("owing to the fact that", "because"),
    ("prior to", "before"), ("subsequent to", "after"), ("take into consideration", "consider"), ("until such time as", "until"),
    ("whether or not", "whether"), ("with regard to", "about"), ("with respect to", "about"),
];

const REDUNDANT: &[(&str, &str)] = &[
    ("added bonus", "bonus"), ("advance planning", "planning"), ("advance warning", "warning"), ("armed gunman", "gunman"),
    ("basic fundamentals", "fundamentals"), ("blinked her eyes", "blinked"), ("blinked his eyes", "blinked"), ("blinked my eyes", "blinked"),
    ("close proximity", "proximity, or near"), ("collaborate together", "collaborate"), ("completely finished", "finished"),
    ("current status", "status"), ("descended down", "descended"), ("each individual", "each"), ("end result", "result"), ("exact same", "same"),
    ("final outcome", "outcome"), ("first began", "began"), ("first started", "started"), ("free gift", "gift"), ("future plans", "plans"),
    ("general consensus", "consensus"), ("join together", "join"), ("joined together", "joined"), ("most unique", "unique"),
    ("new innovation", "innovation"), ("nodded her head", "nodded"), ("nodded his head", "nodded"), ("nodded my head", "nodded"),
    ("nodded their heads", "nodded"), ("past experience", "experience"), ("past history", "history"), ("personal opinion", "opinion"),
    ("reason why", "reason"), ("repeat again", "repeat"), ("revert back", "revert"), ("shrugged her shoulders", "shrugged"),
    ("shrugged his shoulders", "shrugged"), ("shrugged my shoulders", "shrugged"), ("shrugged their shoulders", "shrugged"),
    ("still remains", "remains"), ("sum total", "total"), ("thought to herself", "thought"), ("thought to himself", "thought"),
    ("thought to myself", "thought"), ("true fact", "fact"), ("unexpected surprise", "surprise"), ("very unique", "unique"),
    ("whispered quietly", "whispered"),
];

const HEDGES: &[&str] = &[
    "a little bit", "apparently", "arguably", "basically", "essentially", "i believe", "i feel", "i think", "in a sense", "in my opinion",
    "it seems", "kind of", "maybe", "more or less", "perhaps", "possibly", "presumably", "seems to", "so to speak", "somewhat", "sort of",
    "tend to", "tends to", "to some extent", "virtually",
];

/// Tags other than said and asked.
const FANCY_TAGS: &[&str] = &[
    "admonished", "announced", "barked", "bellowed", "breathed", "chortled", "chuckled", "cooed", "declared", "exclaimed", "gasped",
    "giggled", "grinned", "groaned", "growled", "hissed", "interjected", "intoned", "laughed", "moaned", "mused", "opined", "proclaimed",
    "purred", "quipped", "retorted", "roared", "smiled", "smirked", "snapped", "snarled", "sneered", "spat", "sighed", "whimpered",
];

/// Words ending in -ly that aren't adverbs (or aren't worth a look).
const NOT_ADVERBS: &[&str] = &[
    "ally", "anomaly", "apply", "assembly", "belly", "billy", "bully", "butterfly", "chilly", "costly", "curly", "daily", "deadly", "early",
    "elderly", "emily", "family", "firefly", "fly", "folly", "friendly", "holly", "holy", "hourly", "italy", "jelly", "jolly", "july",
    "kelly", "likely", "lily", "lively", "lonely", "lovely", "lowly", "melancholy", "molly", "monthly", "oily", "only", "orderly", "rally",
    "reply", "rely", "sally", "silly", "sly", "supply", "tally", "ugly", "weekly", "wily", "woolly", "yearly",
];

const BE: &[&str] = &["am", "is", "are", "was", "were", "be", "been", "being"];

/// Past participles that don't end in -ed.
const PARTICIPLES: &[&str] = &[
    "beaten", "begun", "bitten", "blown", "born", "bought", "broken", "brought", "built", "caught", "chosen", "done", "drawn", "driven",
    "eaten", "fallen", "felt", "flown", "forgiven", "forgotten", "found", "frozen", "given", "grown", "heard", "held", "hidden", "hung",
    "hurt", "kept", "known", "laid", "led", "left", "lost", "made", "meant", "met", "paid", "ridden", "seen", "sent", "shaken", "shot",
    "shown", "sold", "spoken", "stolen", "struck", "sung", "sunk", "sworn", "taken", "taught", "thrown", "told", "torn", "understood",
    "woken", "won", "worn", "written",
];

/// -ed words that are usually adjectives after "was" ("she was tired"),
/// or aren't verbs at all.
const NOT_PASSIVE: &[&str] = &[
    "ashamed", "bed", "bored", "concerned", "convinced", "determined", "excited", "frightened", "hundred", "interested", "married",
    "need", "pleased", "prepared", "red", "scared", "shed", "speed", "supposed", "surprised", "tired", "used", "worried",
];

/// Past forms that don't end in -ed.
const PAST: &[&str] = &[
    "ate", "became", "began", "bit", "blew", "bought", "broke", "brought", "built", "came", "caught", "chose", "crept", "did", "didn't",
    "drank", "drew", "drove", "fell", "felt", "fled", "flew", "forgot", "fought", "found", "froze", "gave", "got", "grew", "had", "hadn't",
    "heard", "held", "hid", "hung", "kept", "knew", "laid", "led", "left", "lost", "made", "meant", "met", "paid", "ran", "rang", "rode",
    "rose", "said", "sang", "sank", "sat", "saw", "sent", "shook", "shot", "slept", "slid", "sold", "spoke", "spent", "stood", "stole",
    "struck", "swam", "swept", "swore", "taught", "thought", "threw", "told", "took", "tore", "understood", "was", "wasn't", "went", "wept",
    "were", "weren't", "woke", "won", "wore", "wrote",
];

const PRESENT: &[&str] = &[
    "am", "are", "aren't", "can", "can't", "do", "does", "doesn't", "don't", "has", "hasn't", "have", "haven't", "is", "isn't", "will",
    "won't",
];

/// -ed words that aren't past tense.
const NOT_PAST: &[&str] = &["bed", "bleed", "breed", "feed", "heed", "need", "red", "seed", "shed", "speed", "weed"];

const PRONOUNS: &[&str] = &["i", "he", "she", "it", "we", "they", "you"];

/// Words between a subject and its verb that don't change its tense.
const BETWEEN: &[&str] = &["already", "also", "always", "even", "just", "never", "only", "soon", "still", "then"];

const ABBREVIATIONS: &[&str] = &["dr", "e.g", "etc", "i.e", "jr", "mr", "mrs", "ms", "mt", "no", "sr", "st", "vs"];

// ----- reading the text --------------------------------------------------------

/// A word: where it is, lower case (curly apostrophes made straight), and
/// whether it's inside quotation marks.
#[derive(Clone, Debug)]
struct Word {
    range: Range<usize>,
    text: String,
    dialogue: bool,
}

/// A sentence: where it is, and its words.
#[derive(Clone, Debug)]
pub struct Sentence {
    pub range: Range<usize>,
    words: Vec<Word>,
}

impl Sentence {
    pub fn word_count(&self) -> usize {
        self.words.len()
    }
}

/// The text's sentences, in order: its prose only.
pub fn sentences(text: &str) -> Vec<Sentence> {
    let asides = crate::repetition::asides(text);
    let aside = |at: usize| asides.iter().any(|r| r.contains(&at));
    let mut out = Vec::new();
    let mut line_start = 0;
    for line in text.split_inclusive('\n') {
        let start = line_start;
        line_start += line.len();
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || crate::compile::is_scene_break(trimmed) {
            continue;
        }
        // Words, and where sentences end.
        let mut words: Vec<Word> = Vec::new();
        let mut ends: Vec<usize> = Vec::new();
        let mut in_quote = false;
        let chars: Vec<(usize, char)> = line.char_indices().map(|(i, c)| (i + start, c)).collect();
        let mut word_start: Option<usize> = None;
        let mut n = 0;
        while n < chars.len() {
            let (i, c) = chars[n];
            let next = chars.get(n + 1).map(|&(_, c)| c);
            let inner = (c == '\'' || c == '\u{2019}') && word_start.is_some() && next.is_some_and(char::is_alphabetic);
            let wordy = c.is_alphabetic() || inner || (c == '-' && word_start.is_some() && next.is_some_and(char::is_alphabetic));
            if wordy && !aside(i) {
                word_start.get_or_insert(i);
            } else {
                if let Some(s) = word_start.take() {
                    words.push(Word { range: s..i, text: text[s..i].to_lowercase().replace('\u{2019}', "'"), dialogue: in_quote });
                }
                if aside(i) {
                    n += 1;
                    continue;
                }
                match c {
                    '"' => in_quote = !in_quote,
                    '\u{201c}' => in_quote = true,
                    '\u{201d}' => in_quote = false,
                    '.' | '!' | '?' | '\u{2026}' => {
                        // The end of a sentence, unless it's an abbreviation or an initial.
                        let before = words.last().filter(|w| w.range.end == i).map(|w| w.text.as_str()).unwrap_or("");
                        let initial = before.chars().count() == 1 && c == '.' && text[i - 1..i].chars().all(char::is_uppercase);
                        let followed = next.is_none_or(|c| c.is_whitespace() || matches!(c, '"' | '\u{201d}' | '\u{2019}' | '\'' | ')'));
                        if followed && !(c == '.' && (ABBREVIATIONS.contains(&before) || initial)) {
                            // Past any closing quotes and brackets.
                            let mut end = n + 1;
                            let mut closed = false;
                            while let Some(&(_, q)) = chars.get(end) {
                                if matches!(q, '"' | '\u{201d}' | '\u{2019}' | '\'' | ')' | '.' | '!' | '?') {
                                    if q == '"' || q == '\u{201d}' {
                                        in_quote = false;
                                        closed = true;
                                    }
                                    end += 1;
                                } else {
                                    break;
                                }
                            }
                            // \u{201c}Go!\u{201d} she said: the sentence goes on after the quote.
                            let goes_on = closed && chars[end..].iter().find(|(_, c)| !c.is_whitespace()).is_some_and(|(_, c)| c.is_lowercase());
                            if !goes_on {
                                ends.push(chars.get(end).map_or(start + line.len(), |&(b, _)| b));
                            }
                            n = end;
                            continue;
                        }
                    }
                    _ => {}
                }
            }
            n += 1;
        }
        if let Some(s) = word_start.take() {
            let e = start + line.trim_end().len();
            words.push(Word { range: s..e.max(s), text: text[s..e.max(s)].to_lowercase().replace('\u{2019}', "'"), dialogue: in_quote });
        }
        // Split the words at the ends.
        let mut from = 0;
        let mut sentence_start = start + (line.len() - line.trim_start().len());
        for end in ends.into_iter().chain(std::iter::once(start + line.trim_end().len())) {
            let take: Vec<Word> = words[from..].iter().take_while(|w| w.range.end <= end).cloned().collect();
            from += take.len();
            if !take.is_empty() {
                let s = sentence_start.min(take[0].range.start);
                let e = text[..end].trim_end().len().max(take.last().map_or(s, |w| w.range.end));
                out.push(Sentence { range: s..e, words: take });
            }
            sentence_start = end;
            while sentence_start < text.len() && text[sentence_start..].starts_with(char::is_whitespace) {
                sentence_start += text[sentence_start..].chars().next().map_or(1, char::len_utf8);
            }
        }
    }
    out
}

/// A list of phrases, by first word: each phrase's words and its
/// suggestion. Made once.
type Index = HashMap<&'static str, Vec<(Vec<&'static str>, &'static str)>>;

fn index(list: impl Iterator<Item = (&'static str, &'static str)>) -> Index {
    let mut out: Index = HashMap::new();
    for (phrase, better) in list {
        let words: Vec<&str> = phrase.split_whitespace().collect();
        out.entry(words[0]).or_default().push((words, better));
    }
    // Longest first, so "in order to" wins over a shorter phrase it starts with.
    for v in out.values_mut() {
        v.sort_by_key(|(w, _)| std::cmp::Reverse(w.len()));
    }
    out
}

static CLICHE_INDEX: LazyLock<Index> = LazyLock::new(|| index(CLICHES.iter().map(|p| (*p, ""))));
static WORDY_INDEX: LazyLock<Index> = LazyLock::new(|| index(WORDY.iter().copied()));
static REDUNDANT_INDEX: LazyLock<Index> = LazyLock::new(|| index(REDUNDANT.iter().copied()));
static HEDGE_INDEX: LazyLock<Index> = LazyLock::new(|| index(HEDGES.iter().map(|p| (*p, ""))));

/// Phrases from a list in a sentence's prose (not its dialogue): where
/// each is, with its suggestion.
fn phrases(sentence: &Sentence, list: &Index) -> Vec<(Range<usize>, &'static str)> {
    let words = &sentence.words;
    let mut out = Vec::new();
    let mut i = 0;
    while i < words.len() {
        let found = list.get(words[i].text.as_str()).and_then(|candidates| {
            candidates.iter().find(|(ws, _)| i + ws.len() <= words.len() && words[i..i + ws.len()].iter().zip(ws).all(|(w, p)| w.text == *p))
        });
        match found {
            Some((ws, better)) if !words[i].dialogue => {
                out.push((words[i].range.start..words[i + ws.len() - 1].range.end, *better));
                i += ws.len();
            }
            _ => i += 1,
        }
    }
    out
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Tense {
    Past,
    Present,
}

/// The verbs in a sentence's prose that show its tense: after I, he,
/// she… (and their contractions: she's, I'm).
fn tense_markers(sentence: &Sentence) -> Vec<(Range<usize>, Tense)> {
    let words: Vec<&Word> = sentence.words.iter().filter(|w| !w.dialogue).collect();
    let mut out = Vec::new();
    for (i, w) in words.iter().enumerate() {
        // Contractions: she's, I'm, they're, we've are present.
        if let Some((subject, rest)) = w.text.split_once('\'')
            && PRONOUNS.contains(&subject)
        {
            if matches!(rest, "s" | "m" | "re" | "ve") {
                out.push((w.range.clone(), Tense::Present));
            }
            continue;
        }
        if !PRONOUNS.contains(&w.text.as_str()) {
            continue;
        }
        // The verb: the next word, past a never or a quickly.
        let mut j = i + 1;
        while j < words.len() && j <= i + 2 && (BETWEEN.contains(&words[j].text.as_str()) || words[j].text.ends_with("ly")) {
            j += 1;
        }
        let Some(v) = words.get(j) else { continue };
        let v_text = v.text.as_str();
        let third = matches!(w.text.as_str(), "he" | "she" | "it");
        let tense = if PAST.contains(&v_text) || (v_text.len() > 3 && v_text.ends_with("ed") && !NOT_PAST.contains(&v_text)) {
            Some(Tense::Past)
        } else if PRESENT.contains(&v_text)
            || (third && v_text.len() > 2 && v_text.ends_with('s') && !v_text.ends_with("ss") && !v_text.ends_with("us"))
        {
            Some(Tense::Present)
        } else {
            None
        };
        if let Some(t) = tense {
            out.push((v.range.clone(), t));
        }
    }
    out
}

/// Each sentence's tense, where it's clear (all its markers agree).
fn sentence_tenses(sentences: &[Sentence]) -> Vec<(Option<Tense>, Vec<Range<usize>>)> {
    sentences
        .iter()
        .map(|s| {
            let markers = tense_markers(s);
            let past = markers.iter().all(|(_, t)| *t == Tense::Past);
            let present = markers.iter().all(|(_, t)| *t == Tense::Present);
            let tense = match (markers.is_empty(), past, present) {
                (true, ..) => None,
                (false, true, _) => Some(Tense::Past),
                (false, _, true) => Some(Tense::Present),
                _ => None,
            };
            (tense, markers.into_iter().map(|(r, _)| r).collect())
        })
        .collect()
}

/// The text's tense, if most of it (seven in ten sentences that show one,
/// and at least six of them) agree.
fn main_tense(tenses: &[(Option<Tense>, Vec<Range<usize>>)]) -> (Option<Tense>, usize, usize) {
    let past = tenses.iter().filter(|(t, _)| *t == Some(Tense::Past)).count();
    let present = tenses.iter().filter(|(t, _)| *t == Some(Tense::Present)).count();
    let total = past + present;
    let main = if total < 6 {
        None
    } else if past * 10 >= total * 7 {
        Some(Tense::Past)
    } else if present * 10 >= total * 7 {
        Some(Tense::Present)
    } else {
        None
    };
    (main, past, present)
}

/// What's worth a look in `text`, for the categories that are `on`, in
/// order.
pub fn check(text: &str, on: &[Category]) -> Vec<Finding> {
    let sentences = sentences(text);
    let mut out = Vec::new();
    let has = |c: Category| on.contains(&c);
    for s in &sentences {
        if has(Category::Cliche) {
            for (r, _) in phrases(s, &CLICHE_INDEX) {
                out.push(Finding { range: r, category: Category::Cliche, note: "A clich\u{e9}: say it your way".to_owned() });
            }
        }
        if has(Category::Wordy) {
            for (r, better) in phrases(s, &WORDY_INDEX) {
                out.push(Finding { range: r, category: Category::Wordy, note: format!("Wordy \u{2192} {better}") });
            }
        }
        if has(Category::Redundant) {
            for (r, better) in phrases(s, &REDUNDANT_INDEX) {
                out.push(Finding { range: r, category: Category::Redundant, note: format!("Says it twice \u{2192} {better}") });
            }
        }
        if has(Category::Hedge) {
            for (r, _) in phrases(s, &HEDGE_INDEX) {
                out.push(Finding { range: r, category: Category::Hedge, note: "A hedge: do you need it?".to_owned() });
            }
        }
        if has(Category::Long) && s.word_count() > LONG {
            out.push(Finding { range: s.range.clone(), category: Category::Long, note: format!("A long sentence: {} words", s.word_count()) });
        }
        let words = &s.words;
        if has(Category::Adverb) {
            for w in words.iter().filter(|w| !w.dialogue && w.text.len() > 4 && w.text.ends_with("ly") && !NOT_ADVERBS.contains(&w.text.as_str())) {
                out.push(Finding { range: w.range.clone(), category: Category::Adverb, note: "An adverb: would a stronger verb do?".to_owned() });
            }
        }
        if has(Category::Passive) {
            for i in 0..words.len() {
                if words[i].dialogue || !BE.contains(&words[i].text.as_str()) {
                    continue;
                }
                let mut j = i + 1;
                if words.get(j).is_some_and(|w| w.text.ends_with("ly")) {
                    j += 1;
                }
                let Some(p) = words.get(j) else { continue };
                let participle = PARTICIPLES.contains(&p.text.as_str())
                    || (p.text.len() > 4 && p.text.ends_with("ed") && !NOT_PASSIVE.contains(&p.text.as_str()));
                if participle {
                    out.push(Finding { range: words[i].range.start..p.range.end, category: Category::Passive, note: "Passive: who did it?".to_owned() });
                }
            }
        }
        if has(Category::Tags) {
            for i in 0..words.len() {
                let w = &words[i];
                if w.dialogue {
                    continue;
                }
                // Next to the dialogue: just after it, or just before it.
                let after = words[..i].iter().rev().take(3).any(|p| p.dialogue);
                let before = words[i + 1..].iter().take(3).any(|n| n.dialogue);
                if !(after || before) {
                    continue;
                }
                if FANCY_TAGS.contains(&w.text.as_str()) {
                    out.push(Finding { range: w.range.clone(), category: Category::Tags, note: format!("\u{201c}{}\u{201d}: said usually does it", w.text) });
                } else if matches!(w.text.as_str(), "said" | "asked" | "replied")
                    && let Some(adverb) = words.get(i + 1).filter(|n| !n.dialogue && n.text.len() > 4 && n.text.ends_with("ly") && !NOT_ADVERBS.contains(&n.text.as_str()))
                {
                    out.push(Finding { range: adverb.range.clone(), category: Category::Tags, note: format!("{} {}: let the line carry it", w.text, adverb.text) });
                }
            }
        }
    }
    // Sentences in a row starting with the same word.
    if has(Category::Openers) {
        let firsts: Vec<Option<&Word>> = sentences.iter().map(|s| s.words.first().filter(|w| !w.dialogue)).collect();
        let mut i = 0;
        while i < firsts.len() {
            let Some(first) = firsts[i] else {
                i += 1;
                continue;
            };
            let run = firsts[i..].iter().take_while(|f| f.is_some_and(|w| w.text == first.text)).count();
            if run >= 3 {
                for f in firsts[i..i + run].iter().flatten() {
                    let shown = &text[f.range.clone()];
                    out.push(Finding { range: f.range.clone(), category: Category::Openers, note: format!("{run} sentences in a row start with \u{201c}{shown}\u{201d}") });
                }
            }
            i += run.max(1);
        }
    }
    // Sentences out of the text's tense.
    if has(Category::Tense) {
        let tenses = sentence_tenses(&sentences);
        if let (Some(main), ..) = main_tense(&tenses) {
            let note = match main {
                Tense::Past => "Present tense, where most of this is past",
                Tense::Present => "Past tense, where most of this is present",
            };
            for (tense, markers) in &tenses {
                if tense.is_some_and(|t| t != main) {
                    for r in markers {
                        out.push(Finding { range: r.clone(), category: Category::Tense, note: note.to_owned() });
                    }
                }
            }
        }
    }
    out.sort_by_key(|f| (f.range.start, std::cmp::Reverse(f.range.end)));
    out
}

// ----- the report ---------------------------------------------------------------

/// A text's figures, for the Style window.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Report {
    pub words: usize,
    /// Each sentence: where it is, and its words.
    pub sentences: Vec<(Range<usize>, usize)>,
    /// Flesch–Kincaid grade, if there's enough to say.
    pub grade: Option<f32>,
    /// Sentences that show past tense, and present.
    pub past: usize,
    pub present: usize,
}

impl Report {
    pub fn average(&self) -> f32 {
        if self.sentences.is_empty() { 0.0 } else { self.words as f32 / self.sentences.len() as f32 }
    }

    pub fn longest(&self) -> usize {
        self.sentences.iter().map(|(_, n)| *n).max().unwrap_or(0)
    }

    /// "Mostly past (92%)", or "Mixed: 12 past, 9 present".
    pub fn tense(&self) -> Option<String> {
        let total = self.past + self.present;
        if total < 3 {
            return None;
        }
        let pct = |n: usize| (n * 100 + total / 2) / total;
        Some(if self.past * 10 >= total * 7 {
            format!("Past ({}% of sentences that show one)", pct(self.past))
        } else if self.present * 10 >= total * 7 {
            format!("Present ({}% of sentences that show one)", pct(self.present))
        } else {
            format!("Mixed: {} past, {} present", self.past, self.present)
        })
    }
}

/// Syllables in a word, near enough: groups of vowels, less a silent e.
fn syllables(word: &str) -> usize {
    let mut count = 0;
    let mut vowel = false;
    for c in word.chars() {
        let v = matches!(c, 'a' | 'e' | 'i' | 'o' | 'u' | 'y');
        if v && !vowel {
            count += 1;
        }
        vowel = v;
    }
    if word.ends_with('e') && !word.ends_with("le") && count > 1 {
        count -= 1;
    }
    count.max(1)
}

pub fn report(text: &str) -> Report {
    let sentences = sentences(text);
    let words: usize = sentences.iter().map(Sentence::word_count).sum();
    let syllable_count: usize = sentences.iter().flat_map(|s| &s.words).map(|w| syllables(&w.text)).sum();
    let grade = (words >= 100 && !sentences.is_empty())
        .then(|| (0.39 * words as f32 / sentences.len() as f32 + 11.8 * syllable_count as f32 / words as f32 - 15.59).max(0.0));
    let tenses = sentence_tenses(&sentences);
    let (_, past, present) = main_tense(&tenses);
    Report { words, sentences: sentences.iter().map(|s| (s.range.clone(), s.word_count())).collect(), grade, past, present }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(text: &str, c: Category) -> Vec<(String, String)> {
        check(text, &[c]).into_iter().map(|f| (text[f.range].to_owned(), f.note)).collect()
    }

    #[test]
    fn sentences_split_at_their_ends_not_at_abbreviations() {
        let text = "Mr. Tellin waited. \u{201c}Go!\u{201d} she said. Was it J. R. Hale? Yes\u{2026} it was.\n\n# A heading\n\n* * *\nLast one";
        let s: Vec<&str> = sentences(text).iter().map(|s| &text[s.range.clone()]).collect();
        assert_eq!(s, ["Mr. Tellin waited.", "\u{201c}Go!\u{201d} she said.", "Was it J. R. Hale?", "Yes\u{2026}", "it was.", "Last one"]);
    }

    #[test]
    fn phrases_with_their_shorter_ways() {
        assert_eq!(found("In order to win, she nodded her head.", Category::Wordy), [("In order to".to_owned(), "Wordy \u{2192} to".to_owned())]);
        assert_eq!(found("She nodded her head.", Category::Redundant)[0].0, "nodded her head");
        assert_eq!(found("At the end of the day, it was over.", Category::Cliche)[0].0, "At the end of the day");
        assert_eq!(found("It\u{2019}s sort of blue, I think.", Category::Hedge).len(), 2);
        // Dialogue is left alone.
        assert!(found("\u{201c}At the end of the day,\u{201d} she said.", Category::Cliche).is_empty());
        // Notes too.
        assert!(found("<!-- in order to fix --> Fine.", Category::Wordy).is_empty());
    }

    #[test]
    fn long_sentences_and_same_openings() {
        let long = format!("{}.", vec!["word"; 31].join(" "));
        assert_eq!(check(&long, &[Category::Long]).len(), 1);
        assert!(check("Short one.", &[Category::Long]).is_empty());
        let same = "He ran. He fell. He rose. She watched.";
        assert_eq!(found(same, Category::Openers).len(), 3);
        assert!(found("He ran. He fell. She rose.", Category::Openers).is_empty());
    }

    #[test]
    fn passive_adverbs_and_tags() {
        assert_eq!(found("The ferry was sunk by night. She was tired.", Category::Passive)[0].0, "was sunk");
        assert_eq!(found("The ferry was sunk by night. She was tired.", Category::Passive).len(), 1);
        assert_eq!(found("She ran quickly. Only the early bird.", Category::Adverb)[0].0, "quickly");
        assert_eq!(found("She ran quickly. Only the early bird.", Category::Adverb).len(), 1);
        let tags = "\u{201c}Go,\u{201d} she hissed. \u{201c}Now,\u{201d} he said softly. She hissed at the cat.";
        let f = found(tags, Category::Tags);
        assert_eq!(f.iter().map(|(t, _)| t.as_str()).collect::<Vec<_>>(), ["hissed", "softly"], "not the cat's hiss, away from dialogue");
    }

    #[test]
    fn a_slip_out_of_the_texts_tense() {
        let past = "She walked to the quay. He waited there. They talked for an hour. It rained. She left at noon. He stayed. She walks home now. \u{201c}I am fine,\u{201d} she said.";
        let f = found(past, Category::Tense);
        assert_eq!(f, [("walks".to_owned(), "Present tense, where most of this is past".to_owned())], "not the dialogue's \u{201c}I am\u{201d}");
        // Too little to say, or too mixed: nothing.
        assert!(found("She walks. He walked.", Category::Tense).is_empty());
        let r = report(past);
        assert_eq!((r.past, r.present), (7, 1), "she said, after the dialogue, is past");
        assert!(r.tense().unwrap().starts_with("Past (88%"));
    }

    #[test]
    fn a_report_of_sentences_and_grade() {
        let text = "The cat sat on the mat. It was a good day for a cat to sit on a mat in the sun, and so it did, for hours.";
        let r = report(text);
        assert_eq!(r.sentences.len(), 2);
        assert_eq!(r.words, 6 + 22);
        assert_eq!(r.longest(), 22);
        assert_eq!(r.grade, None, "too little to say");
        let r = report(&format!("{text} ").repeat(4));
        assert!(r.grade.is_some_and(|g| g > 1.0 && g < 8.0), "{:?}", r.grade);
        assert_eq!(syllables("ferry"), 2);
        assert_eq!(syllables("table"), 2);
        assert_eq!(syllables("make"), 1);
    }

    #[test]
    fn quick_enough_to_run_as_you_type() {
        let para = "In order to leave, she walked to the quay at the end of the day. He waited there, sort of. They talked for an hour and she said softly that it was taken. ";
        let text = para.repeat(400); // about 13,000 words
        let started = std::time::Instant::now();
        let found = check(&text, &Category::ALL);
        assert!(!found.is_empty());
        // Generous, for a debug build: it runs again at each keystroke.
        assert!(started.elapsed().as_millis() < 1500, "{:?}", started.elapsed());
    }
}
