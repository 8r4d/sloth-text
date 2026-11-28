//! Short stories: what length a story counts as (flash, short story,
//! novelette…), and its bookends, the first and last paragraphs, to read
//! side by side.

use crate::stats::thousands;

/// The lengths as most magazines and awards count them: each one's name
/// and the words it goes up to (not including).
const LENGTHS: [(&str, usize); 5] = [
    ("Flash fiction", 1_000),
    ("Short story", 7_500),
    ("Novelette", 17_500),
    ("Novella", 40_000),
    ("Novel", usize::MAX),
];

/// What a story of this many words counts as.
pub fn length(words: usize) -> &'static str {
    LENGTHS.iter().find(|(_, up_to)| words < *up_to).map_or("Novel", |(name, _)| name)
}

/// How far it is to the next length up, for the hover: "1,240 words
/// before it's a novelette".
pub fn to_next(words: usize) -> Option<String> {
    let i = LENGTHS.iter().position(|(_, up_to)| words < *up_to)?;
    let (next, _) = LENGTHS.get(i + 1)?;
    let left = LENGTHS[i].1 - words;
    let a = if next.starts_with(['A', 'E', 'I', 'O', 'U']) { "an" } else { "a" };
    Some(format!("{} word{} before it's {a} {}", thousands(left as u32), if left == 1 { "" } else { "s" }, next.to_lowercase()))
}

/// The lengths, for the hover.
pub const LENGTHS_HINT: &str = "As most magazines and awards count them: flash fiction under 1,000 words, a short story up to 7,500, a novelette up to 17,500, a novella up to 40,000. Many magazines take stories up to 5,000 words; flash markets often stop at 1,000, some at 500.";

/// The story's paragraphs, as read: no notes, headings or scene breaks.
pub fn paragraphs(text: &str) -> Vec<String> {
    crate::compile::strip_notes(text)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#') && !crate::compile::is_scene_break(l))
        .map(str::to_owned)
        .collect()
}

/// Its first and last paragraphs (the last only if there are two or more).
pub fn bookends(text: &str) -> Option<(String, Option<String>)> {
    let mut paragraphs = paragraphs(text);
    let last = if paragraphs.len() > 1 { paragraphs.pop() } else { None };
    let first = paragraphs.into_iter().next()?;
    Some((first, last))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lengths_as_magazines_count_them() {
        assert_eq!(length(0), "Flash fiction");
        assert_eq!(length(999), "Flash fiction");
        assert_eq!(length(1_000), "Short story");
        assert_eq!(length(7_499), "Short story");
        assert_eq!(length(7_500), "Novelette");
        assert_eq!(length(39_999), "Novella");
        assert_eq!(length(40_000), "Novel");
        assert_eq!(to_next(6_260).as_deref(), Some("1,240 words before it's a novelette"));
        assert_eq!(to_next(999).as_deref(), Some("1 word before it's a short story"));
        assert_eq!(to_next(50_000), None);
    }

    #[test]
    fn bookends_skip_notes_headings_and_breaks() {
        let text = "# The Ferry\n\n<!-- Mara's story -->\nThe quay was empty.\n\nRain.\n\n* * *\n\nShe laughed, at last.\n<!-- end -->\n";
        assert_eq!(bookends(text), Some(("The quay was empty.".to_owned(), Some("She laughed, at last.".to_owned()))));
        assert_eq!(bookends("Only this.\n"), Some(("Only this.".to_owned(), None)));
        assert_eq!(bookends("# Title\n\n"), None);
    }
}
