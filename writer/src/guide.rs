//! Guides: what the builders walk you through a step at a time (Build a
//! Plot, Build a Character, Build an Essay; see guide_ui.rs), and what
//! they share. Each builder's own guides, and what it makes of the
//! answers, are in plot.rs, cast.rs and essay.rs.

/// What a step's answer is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fill {
    /// Words for the text.
    Text,
    /// One character to a line, `Name: their story`, each to become a
    /// character profile.
    Characters,
    /// Possible titles, one to a line, one of them picked.
    Title,
    /// A sentence for the synopsis.
    Synopsis,
    /// A word count, for the target.
    Target,
    /// Keywords, separated by commas.
    Tags,
}

/// One step of a guide.
pub struct Step {
    pub name: &'static str,
    /// What happens here, in a sentence or two.
    pub about: &'static str,
    /// What to ask yourself.
    pub question: &'static str,
    /// Roughly where it falls, as a share of the way through (0.10 is a
    /// tenth of the way in), for the plots that pace by length.
    pub share: Option<f32>,
    pub fill: Fill,
    /// The profile section a character's answer goes in ("" elsewhere).
    pub section: &'static str,
}

/// A run of steps under a heading (an act), or none.
pub struct Part {
    pub name: Option<&'static str>,
    pub steps: &'static [Step],
}

/// A guide: a story structure, a character arc, an essay's shape.
pub struct Guide {
    pub name: &'static str,
    /// The heading it's listed under.
    pub group: &'static str,
    /// Whose it is, or where it's from.
    pub from: &'static str,
    /// What it's good for.
    pub suits: &'static str,
    pub about: &'static str,
    pub parts: &'static [Part],
}

impl Guide {
    /// Every step, in order, with its part's name.
    pub fn steps(&self) -> Vec<(Option<&'static str>, &'static Step)> {
        self.parts.iter().flat_map(|a| a.steps.iter().map(move |b| (a.name, b))).collect()
    }

    #[cfg(test)]
    pub fn step_count(&self) -> usize {
        self.parts.iter().map(|a| a.steps.len()).sum()
    }
}

/// The headings a list of guides goes under, in order.
pub fn groups(guides: &[Guide]) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for g in guides {
        if !out.contains(&g.group) {
            out.push(g.group);
        }
    }
    out
}

pub const fn step(name: &'static str, about: &'static str, question: &'static str) -> Step {
    Step { name, about, question, share: None, fill: Fill::Text, section: "" }
}

pub const fn paced(name: &'static str, share: f32, about: &'static str, question: &'static str) -> Step {
    Step { name, about, question, share: Some(share), fill: Fill::Text, section: "" }
}

/// A step whose answer lists characters, one to a line, each to become a
/// profile.
pub const fn characters_step(name: &'static str, about: &'static str, question: &'static str) -> Step {
    filled(Fill::Characters, name, about, question)
}

/// A step whose answer is something other than text (see `Fill`).
pub const fn filled(fill: Fill, name: &'static str, about: &'static str, question: &'static str) -> Step {
    Step { name, about, question, share: None, fill, section: "" }
}

/// A character's step, its answer for this section of the profile.
pub const fn in_section(section: &'static str, name: &'static str, about: &'static str, question: &'static str) -> Step {
    Step { name, about, question, share: None, fill: Fill::Text, section }
}

/// How a step's note starts in a plot (`Beat`) or an essay (`Part`), so
/// Break Up can tell it from notes of your own and make it the synopsis.
pub const NOTE_STARTS: [&str; 2] = ["<!-- Beat: ", "<!-- Part: "];

/// A piece of a broken-up plot or essay: if it opens (after its heading,
/// if it kept one) with a step's note, the note's words, for a synopsis,
/// and the text without it.
pub fn step_note(text: &str) -> Option<(String, String)> {
    let (heading, body) = match text.split_once('\n') {
        Some((first, rest)) if first.trim_start().starts_with('#') => (format!("{first}\n"), rest),
        _ => (String::new(), text),
    };
    let body = body.trim_start();
    let inner = NOTE_STARTS.iter().find_map(|start| body.strip_prefix(start))?;
    let (note, rest) = inner.split_once("-->")?;
    let rest = rest.strip_prefix('\n').unwrap_or(rest);
    Some((note.trim().to_owned(), format!("{heading}{rest}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_note_of_your_own_isnt_a_step() {
        assert_eq!(step_note("<!-- remember the tide -->\nText."), None);
        assert_eq!(step_note("Text first.\n<!-- Beat: Go \u{2014} … -->"), None);
        assert_eq!(step_note("<!-- Part: Hook \u{2014} Open strong. -->\nIt began.").unwrap(), ("Hook \u{2014} Open strong.".to_owned(), "It began.".to_owned()));
    }
}
