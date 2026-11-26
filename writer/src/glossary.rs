//! The glossary: one document of terms, each a `# heading` with its
//! definition under it, kept A to Z.
//!
//! ```markdown
//! # Ansible
//! A device that sends messages instantly across any distance.
//!
//! # Ferryman's Toll
//! The coin paid at the quay, *always* in silver.
//! ```
//!
//! It's plain Markdown, written by hand as much as by Writer: whatever
//! comes before the first heading is kept at the top, and a definition is
//! everything up to the next heading. Writer adds terms in their place
//! (Add to Glossary), sorts them on request, marks them in a document's
//! text while asked to, and compiles them at the end of a book.

use std::ops::Range;

/// A term and what it means.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub term: String,
    pub definition: String,
}

/// The glossary read: anything before the first term, then the terms.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Glossary {
    pub preamble: String,
    pub entries: Vec<Entry>,
}

/// A heading line's term, if the line is one (`# Term`, any level; a lone
/// `#` is a scene break, not a term).
fn heading(line: &str) -> Option<&str> {
    let line = line.trim();
    let term = line.strip_prefix('#')?.trim_start_matches('#').trim();
    (!term.is_empty()).then_some(term)
}

/// How terms are ordered: by letters alone, ignoring case and a leading
/// "The", "A" or "An".
fn sort_key(term: &str) -> String {
    let lower = term.trim().to_lowercase();
    let rest = ["the ", "a ", "an "].iter().find_map(|a| lower.strip_prefix(a)).unwrap_or(&lower);
    rest.chars().filter(|c| c.is_alphanumeric() || c.is_whitespace()).collect()
}

impl Glossary {
    pub fn parse(text: &str) -> Glossary {
        let mut glossary = Glossary::default();
        let mut preamble = Vec::new();
        for line in text.lines() {
            match (heading(line), glossary.entries.last_mut()) {
                (Some(term), _) => glossary.entries.push(Entry { term: term.to_owned(), definition: String::new() }),
                (None, Some(entry)) => {
                    entry.definition.push_str(line);
                    entry.definition.push('\n');
                }
                (None, None) => preamble.push(line),
            }
        }
        glossary.preamble = preamble.join("\n").trim().to_owned();
        for entry in &mut glossary.entries {
            entry.definition = entry.definition.trim().to_owned();
        }
        glossary
    }

    /// Back as text: the preamble, then each term and its definition, with
    /// a blank line between.
    pub fn to_text(&self) -> String {
        let mut parts = Vec::new();
        if !self.preamble.is_empty() {
            parts.push(self.preamble.clone());
        }
        for entry in &self.entries {
            parts.push(if entry.definition.is_empty() { format!("# {}", entry.term) } else { format!("# {}\n{}", entry.term, entry.definition) });
        }
        let mut text = parts.join("\n\n");
        text.push('\n');
        text
    }

    /// The entry for a term, any case.
    pub fn find(&self, term: &str) -> Option<&Entry> {
        let wanted = term.trim().to_lowercase();
        self.entries.iter().find(|e| e.term.to_lowercase() == wanted)
    }

    /// Add a term with its definition in its A–Z place, or change the
    /// definition of one that's there (any case; its spelling as given).
    pub fn set(&mut self, term: &str, definition: &str) {
        let (term, definition) = (term.trim(), definition.trim());
        let wanted = term.to_lowercase();
        if let Some(entry) = self.entries.iter_mut().find(|e| e.term.to_lowercase() == wanted) {
            entry.term = term.to_owned();
            entry.definition = definition.to_owned();
            return;
        }
        let key = sort_key(term);
        // Before the first term that sorts after it (so a hand-ordered
        // glossary isn't reordered by adding one).
        let at = self.entries.iter().position(|e| sort_key(&e.term) > key).unwrap_or(self.entries.len());
        self.entries.insert(at, Entry { term: term.to_owned(), definition: definition.to_owned() });
    }

    /// Put the terms in A–Z order.
    pub fn sort(&mut self) {
        self.entries.sort_by_key(|e| sort_key(&e.term));
    }

    pub fn is_sorted(&self) -> bool {
        self.entries.windows(2).all(|w| sort_key(&w[0].term) <= sort_key(&w[1].term))
    }
}

/// Where the glossary's terms come up in a text, as byte ranges with the
/// entry each is: whole words, any case, a longer term first where two
/// overlap ("Ferryman's Toll" before "Toll"). Notes and headings count.
pub fn occurrences(text: &str, glossary: &Glossary) -> Vec<(Range<usize>, usize)> {
    let lower = text.to_lowercase();
    // Lowercasing can change byte lengths (rarely); then don't mark at all
    // rather than mark the wrong words.
    if lower.len() != text.len() {
        return Vec::new();
    }
    let mut order: Vec<usize> = (0..glossary.entries.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(glossary.entries[i].term.len()));
    let word = |c: char| c.is_alphanumeric() || c == '\'' || c == '\u{2019}';
    let mut found: Vec<(Range<usize>, usize)> = Vec::new();
    for i in order {
        let term = glossary.entries[i].term.to_lowercase();
        if term.is_empty() {
            continue;
        }
        for (at, _) in lower.match_indices(&term) {
            let end = at + term.len();
            let before = lower[..at].chars().next_back();
            let after = lower[end..].chars().next();
            if before.is_some_and(word) || after.is_some_and(word) {
                continue;
            }
            if found.iter().any(|(r, _)| r.start < end && at < r.end) {
                continue;
            }
            found.push((at..end, i));
        }
    }
    found.sort_by_key(|(r, _)| r.start);
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = "Terms from the islands.\n\n# Ansible\nA device that sends messages instantly.\n\n# Ferryman's Toll\nThe coin paid at the quay,\n*always* in silver.\n";

    #[test]
    fn reads_terms_and_definitions_and_writes_them_back() {
        let g = Glossary::parse(TEXT);
        assert_eq!(g.preamble, "Terms from the islands.");
        assert_eq!(g.entries.len(), 2);
        assert_eq!(g.entries[1].term, "Ferryman's Toll");
        assert_eq!(g.entries[1].definition, "The coin paid at the quay,\n*always* in silver.");
        assert_eq!(g.to_text(), TEXT);
        assert_eq!(g.find("ansible").unwrap().definition, "A device that sends messages instantly.");
    }

    #[test]
    fn a_new_term_goes_in_its_place_and_an_old_one_is_changed() {
        let mut g = Glossary::parse(TEXT);
        g.set("Brine Witch", "One who reads the tides.");
        g.set("The Abyss", "Below the shelf.");
        g.set("zephyr", "A west wind.");
        let terms: Vec<&str> = g.entries.iter().map(|e| e.term.as_str()).collect();
        assert_eq!(terms, ["The Abyss", "Ansible", "Brine Witch", "Ferryman's Toll", "zephyr"], "A to Z, a leading The ignored");
        g.set("ANSIBLE", "Faster than light.");
        assert_eq!(g.entries.len(), 5, "changed, not added");
        assert_eq!(g.find("ansible").unwrap().definition, "Faster than light.");
        assert!(g.is_sorted());
    }

    #[test]
    fn sorts_a_glossary_written_out_of_order() {
        let mut g = Glossary::parse("# Zephyr\nWind.\n\n# anchor\nHeavy.\n\n#\n\n# Mast\nTall.");
        assert!(!g.is_sorted());
        g.sort();
        let terms: Vec<&str> = g.entries.iter().map(|e| e.term.as_str()).collect();
        assert_eq!(terms, ["anchor", "Mast", "Zephyr"]);
        assert_eq!(g.find("anchor").unwrap().definition, "Heavy.\n\n#", "a lone # is a scene break, part of the definition");
    }

    #[test]
    fn finds_terms_in_the_text_as_whole_words() {
        let g = Glossary::parse(TEXT);
        let text = "She paid the ferryman's toll; the ansible hummed. Ansibles? No: ansible's.";
        let found: Vec<(&str, usize)> = occurrences(text, &g).into_iter().map(|(r, i)| (&text[r], i)).collect();
        assert_eq!(found, [("ferryman's toll", 1), ("ansible", 0)], "not inside a longer word");
    }
}
