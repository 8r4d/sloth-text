//! Spell checking: on a Mac with the macOS spell checker (the one TextEdit
//! and Pages use, with the person's languages and learned words); on
//! Windows and Linux with Hunspell dictionaries for English (US and UK)
//! built in, and a word list of the person's own (`set_personal_words`).
//!
//! Text is checked a paragraph (line) at a time and the results are kept,
//! so typing only rechecks the line being typed in. Words inside notes
//! (`<!-- … -->`) aren't checked.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Mutex;

/// Misspellings by line text, as byte ranges within the line.
static CHECKED: Mutex<Option<HashMap<String, Vec<Range<usize>>>>> = Mutex::new(None);

/// Above this many remembered lines, start again (old drafts' lines).
const MAX_REMEMBERED: usize = 20_000;

/// Whether there's a spell checker to use.
pub fn available() -> bool {
    true
}

/// Where learned words are kept, one per line (Windows and Linux; the Mac
/// keeps them itself). Call before checking anything.
pub fn set_personal_words(path: std::path::PathBuf) {
    #[cfg(not(target_os = "macos"))]
    hunspell::set_personal_words(path);
    #[cfg(target_os = "macos")]
    let _ = path;
}

/// Load the dictionaries in the background, so the first check doesn't wait.
pub fn warm_up() {
    #[cfg(not(target_os = "macos"))]
    std::thread::spawn(|| {
        hunspell::dictionaries();
    });
}

/// The misspelled words in `text`, as byte ranges, in order.
pub fn misspelled(text: &str) -> Vec<Range<usize>> {
    if !available() {
        return Vec::new();
    }
    let notes = note_ranges(text);
    let mut guard = CHECKED.lock().unwrap_or_else(|e| e.into_inner());
    let checked = guard.get_or_insert_with(HashMap::new);
    if checked.len() > MAX_REMEMBERED {
        checked.clear();
    }
    let mut out = Vec::new();
    let mut start = 0;
    for line in text.split('\n') {
        if !line.trim().is_empty() {
            let found = checked.entry(line.to_owned()).or_insert_with(|| platform::check(line));
            for r in found.iter() {
                let r = start + r.start..start + r.end;
                if !notes.iter().any(|n| n.start < r.end && r.start < n.end) {
                    out.push(r);
                }
            }
        }
        start += line.len() + 1;
    }
    out
}

/// Suggested spellings for a word, best first.
pub fn suggestions(word: &str) -> Vec<String> {
    if !available() { Vec::new() } else { platform::guesses(word) }
}

/// Add a word to the person's dictionary (shared with other Mac apps).
pub fn learn(word: &str) {
    if available() {
        platform::learn(word);
        forget();
    }
}

/// Stop flagging a word until the app quits.
pub fn ignore(word: &str) {
    if available() {
        platform::ignore(word);
        forget();
    }
}

/// Forget remembered results, so the next check sees learned and ignored words.
fn forget() {
    if let Some(checked) = CHECKED.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        checked.clear();
    }
}

/// Where the notes (`<!-- … -->`) are; an unclosed one runs to the end.
fn note_ranges(text: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(open) = text[from..].find("<!--") {
        let start = from + open;
        let end = text[start + 4..].find("-->").map_or(text.len(), |i| start + 4 + i + 3);
        out.push(start..end);
        from = end;
    }
    out
}

#[cfg(target_os = "macos")]
mod platform {
    use std::ops::Range;
    use std::sync::OnceLock;

    use objc2_app_kit::NSSpellChecker;
    use objc2_foundation::{NSInteger, NSNotFound, NSRange, NSString};

    /// One spell "document" for the whole app, so Ignore lasts until it quits.
    fn tag() -> NSInteger {
        static TAG: OnceLock<NSInteger> = OnceLock::new();
        *TAG.get_or_init(NSSpellChecker::uniqueSpellDocumentTag)
    }

    /// Misspellings in one line, as byte ranges.
    pub fn check(line: &str) -> Vec<Range<usize>> {
        let checker = NSSpellChecker::sharedSpellChecker();
        let language = checker.language();
        let string = NSString::from_str(line);
        let units: usize = line.encode_utf16().count();
        let mut out = Vec::new();
        let mut at: usize = 0;
        while at < units {
            // SAFETY: a null word count is allowed.
            let found = unsafe {
                checker.checkSpellingOfString_startingAt_language_wrap_inSpellDocumentWithTag_wordCount(
                    &string,
                    at as NSInteger,
                    // The Mac's main spelling language. Left to guess, the
                    // checker accepts a word from any language the person
                    // has turned on ("yeer" passes as Dutch, say).
                    Some(&language),
                    false,
                    tag(),
                    std::ptr::null_mut(),
                )
            };
            if found.location == NSNotFound as usize || found.length == 0 || found.location < at {
                break;
            }
            out.push(byte_range(line, found.location..found.location + found.length));
            at = found.location + found.length;
        }
        out
    }

    pub fn guesses(word: &str) -> Vec<String> {
        let checker = NSSpellChecker::sharedSpellChecker();
        let string = NSString::from_str(word);
        let range = NSRange::new(0, word.encode_utf16().count());
        checker
            .guessesForWordRange_inString_language_inSpellDocumentWithTag(range, &string, Some(&checker.language()), tag())
            .map(|list| list.iter().map(|s| s.to_string()).collect())
            .unwrap_or_default()
    }

    pub fn learn(word: &str) {
        NSSpellChecker::sharedSpellChecker().learnWord(&NSString::from_str(word));
    }

    pub fn ignore(word: &str) {
        NSSpellChecker::sharedSpellChecker().ignoreWord_inSpellDocumentWithTag(&NSString::from_str(word), tag());
    }

    /// UTF-16 offsets (what NSString counts in) to byte offsets.
    fn byte_range(text: &str, units: Range<usize>) -> Range<usize> {
        let mut unit = 0;
        let (mut start, mut end) = (text.len(), text.len());
        for (byte, c) in text.char_indices() {
            if unit == units.start {
                start = byte;
            }
            if unit == units.end {
                end = byte;
                break;
            }
            unit += c.len_utf16();
        }
        start..end.max(start)
    }
}

#[cfg(not(target_os = "macos"))]
use hunspell as platform;

/// Hunspell dictionaries, read with spellbook. Built on the Mac only for
/// its tests.
#[cfg(any(not(target_os = "macos"), test))]
mod hunspell {
    use std::collections::HashSet;
    use std::fs;
    use std::ops::Range;
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};

    use spellbook::Dictionary;

    const AFF: &str = include_str!("../dictionaries/en-US/en.aff");
    const US: &str = include_str!("../dictionaries/en-US/en-US.dic");
    const UK: &str = include_str!("../dictionaries/en-GB/en-GB.dic");

    pub fn dictionaries() -> &'static [Dictionary] {
        static DICTS: OnceLock<Vec<Dictionary>> = OnceLock::new();
        DICTS.get_or_init(|| [US, UK].into_iter().filter_map(|dic| Dictionary::new(AFF, dic).ok()).collect())
    }

    /// Words learned (kept in the personal file) and ignored (until quit).
    struct Own {
        file: Option<PathBuf>,
        learned: HashSet<String>,
        ignored: HashSet<String>,
    }

    static OWN: Mutex<Option<Own>> = Mutex::new(None);

    fn with_own<T>(f: impl FnOnce(&mut Own) -> T) -> T {
        let mut guard = OWN.lock().unwrap_or_else(|e| e.into_inner());
        f(guard.get_or_insert_with(|| Own { file: None, learned: HashSet::new(), ignored: HashSet::new() }))
    }

    pub fn set_personal_words(path: PathBuf) {
        let words = fs::read_to_string(&path).unwrap_or_default();
        with_own(|own| {
            own.learned.extend(words.lines().map(str::trim).filter(|w| !w.is_empty()).map(str::to_owned));
            own.file = Some(path);
        });
    }

    fn correct(word: &str) -> bool {
        let word = word.replace('\u{2019}', "'");
        if with_own(|own| own.learned.contains(&word) || own.ignored.contains(&word)) {
            return true;
        }
        dictionaries().iter().any(|d| d.check(&word))
    }

    /// The words in a line, as byte ranges: letters, with apostrophes
    /// inside (don't, Mara’s). Anything with a digit is left alone (3rd,
    /// B12), as are web and email addresses.
    fn words(line: &str) -> Vec<Range<usize>> {
        // Whitespace-separated pieces that are addresses.
        let mut skip = Vec::new();
        let mut at = 0;
        for piece in line.split(char::is_whitespace) {
            if piece.contains("://") || piece.contains('@') || piece.starts_with("www.") {
                skip.push(at..at + piece.len());
            }
            at += piece.len() + 1;
        }
        let apostrophe = |c: char| matches!(c, '\'' | '\u{2019}');
        let mut out = Vec::new();
        let mut start: Option<usize> = None;
        let mut push = |s: usize, e: usize| {
            let word = &line[s..e];
            let lead = word.len() - word.trim_start_matches(apostrophe).len();
            let word = word.trim_matches(apostrophe);
            let r = s + lead..s + lead + word.len();
            if !word.is_empty() && !word.chars().any(char::is_numeric) && !skip.iter().any(|k| k.start <= r.start && r.end <= k.end) {
                out.push(r);
            }
        };
        for (i, c) in line.char_indices() {
            let part = c.is_alphanumeric() || apostrophe(c);
            match (start, part) {
                (None, true) => start = Some(i),
                (Some(s), false) => {
                    push(s, i);
                    start = None;
                }
                _ => {}
            }
        }
        if let Some(s) = start {
            push(s, line.len());
        }
        out
    }

    pub fn check(line: &str) -> Vec<Range<usize>> {
        words(line).into_iter().filter(|r| !correct(&line[r.clone()])).collect()
    }

    pub fn guesses(word: &str) -> Vec<String> {
        let word = word.replace('\u{2019}', "'");
        let mut out: Vec<String> = Vec::new();
        for dict in dictionaries() {
            let mut found = Vec::new();
            dict.suggest(&word, &mut found);
            for g in found {
                if !out.contains(&g) {
                    out.push(g);
                }
            }
        }
        out
    }

    pub fn learn(word: &str) {
        let word = word.replace('\u{2019}', "'");
        let file = with_own(|own| {
            own.learned.insert(word.clone());
            own.file.clone().map(|f| (f, own.learned.iter().cloned().collect::<Vec<_>>()))
        });
        if let Some((path, mut words)) = file {
            words.sort();
            if let Some(dir) = path.parent() {
                let _ = fs::create_dir_all(dir);
            }
            let _ = fs::write(path, words.join("\n") + "\n");
        }
    }

    pub fn ignore(word: &str) {
        let word = word.replace('\u{2019}', "'");
        with_own(|own| own.ignored.insert(word));
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn flagged(line: &str) -> Vec<&str> {
            check(line).into_iter().map(|r| &line[r]).collect()
        }

        #[test]
        fn english_us_and_uk_both_pass() {
            assert_eq!(flagged("Teh snow came early."), ["Teh"]);
            assert!(flagged("The colour of the color, recognised and recognized.").is_empty());
            assert!(flagged("Mara\u{2019}s ferry didn't wait; it's gone.").is_empty());
            assert_eq!(flagged("She was hapy"), ["hapy"]);
        }

        #[test]
        fn numbers_and_addresses_are_left_alone() {
            assert!(flagged("The 3rd of May, room B12.").is_empty());
            assert!(flagged("See https://example.com/xyzzy or ann@qwrtz.com").is_empty());
        }

        #[test]
        fn suggestions_and_learned_words() {
            assert!(guesses("Teh").iter().any(|g| g == "The"), "{:?}", guesses("Teh"));
            ignore("Qwerp");
            assert!(flagged("Qwerp said").is_empty());
            // Learned words are kept in the personal list, and read back.
            let file = std::env::temp_dir().join(format!("sloth-spell-words-{}.txt", std::process::id()));
            fs::write(&file, "Brindlewick\n").unwrap();
            set_personal_words(file.clone());
            assert!(flagged("Brindlewick said").is_empty());
            learn("Zorblat");
            assert!(flagged("Zorblat said").is_empty());
            assert!(fs::read_to_string(&file).unwrap().lines().any(|w| w == "Zorblat"));
            let _ = fs::remove_file(file);
        }
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    fn flagged(text: &str) -> Vec<&str> {
        misspelled(text).into_iter().map(|r| &text[r]).collect()
    }

    #[test]
    fn misspelled_words_are_found_on_every_line() {
        assert_eq!(flagged("This is a tset.\nThe snow came erly that yeer."), vec!["tset", "erly", "yeer"]);
        assert!(flagged("Everything here is spelled correctly.").is_empty());
    }

    #[test]
    fn offsets_are_right_after_accents_and_em_dashes() {
        let text = "Café—naïve\u{2014}wrod";
        assert_eq!(flagged(text), vec!["wrod"]);
    }

    #[test]
    fn notes_and_markdown_markers_are_left_alone() {
        assert_eq!(flagged("**Bold** and *tset* <!-- a nnote here -->"), vec!["tset"]);
        assert!(flagged("<!-- unclosed nnote").is_empty());
    }

    #[test]
    fn suggestions_offer_the_right_spelling() {
        assert!(suggestions("speling").iter().any(|s| s == "spelling"));
    }

    #[test]
    fn notes_are_found() {
        assert_eq!(note_ranges("a <!-- b --> c <!-- d"), vec![2..12, 15..21]);
    }
}
