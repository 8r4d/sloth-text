//! Spell checking with the macOS spell checker (the one TextEdit and Pages
//! use, with the person's languages and learned words).
//!
//! Text is checked a paragraph (line) at a time and the results are kept,
//! so typing only rechecks the line being typed in. Words inside notes
//! (`<!-- … -->`) aren't checked. Elsewhere than macOS nothing is flagged.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Mutex;

/// Misspellings by line text, as byte ranges within the line.
static CHECKED: Mutex<Option<HashMap<String, Vec<Range<usize>>>>> = Mutex::new(None);

/// Above this many remembered lines, start again (old drafts' lines).
const MAX_REMEMBERED: usize = 20_000;

/// Whether there's a spell checker to use.
pub fn available() -> bool {
    cfg!(target_os = "macos")
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
mod platform {
    use std::ops::Range;
    pub fn check(_: &str) -> Vec<Range<usize>> {
        Vec::new()
    }
    pub fn guesses(_: &str) -> Vec<String> {
        Vec::new()
    }
    pub fn learn(_: &str) {}
    pub fn ignore(_: &str) {}
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
