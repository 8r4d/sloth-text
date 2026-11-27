//! Build a Character: guides to a character (a sketch, an arc, an
//! antagonist), and how the answers go into their profile.
//!
//! Each answer goes in the profile section its step names, as a paragraph
//! of its own that starts with the step's name in bold:
//!
//! ```markdown
//! ## Arc
//!
//! Something you wrote yourself stays as it is.
//!
//! **The Lie:** That being needed is the same as being loved.
//! ```
//!
//! So the builder can find its answers again (to revise them) and change
//! only those paragraphs. A section the profile hasn't got is added.

use crate::character::{Section, Sheet};
use crate::guide::{Guide, Part, Step, in_section};

const LOOKS: &str = "Physical appearance";
const PERSONALITY: &str = "Personality";
const MOTIVATIONS: &str = "Motivations";
const BACKSTORY: &str = "Backstory";
const VOICE: &str = "Voice";
const RELATIONSHIPS: &str = "Relationships";
const ARC: &str = "Arc";

/// A paragraph that starts with this label, if this one does: the rest.
fn labelled<'a>(paragraph: &'a str, label: &str) -> Option<&'a str> {
    let rest = paragraph.trim_start().strip_prefix("**")?.strip_prefix(label)?.strip_prefix(":**")?;
    Some(rest.trim())
}

fn paragraphs(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            if !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
        } else {
            if !current.is_empty() {
                current.push('\n');
            }
            current.push_str(line);
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

fn section<'a>(sheet: &'a Sheet, heading: &str) -> Option<&'a Section> {
    sheet.sections.iter().find(|s| s.heading.trim().eq_ignore_ascii_case(heading))
}

/// The answers a profile holds already for this guide's steps (empty where
/// it has none), to revise.
pub fn read(sheet: &Sheet, guide: &Guide) -> Vec<String> {
    guide
        .steps()
        .iter()
        .map(|(_, step)| {
            section(sheet, step.section)
                .and_then(|s| paragraphs(&s.text).iter().find_map(|p| labelled(p, step.name).map(str::to_owned)))
                .unwrap_or_default()
        })
        .collect()
}

/// The profile with these answers in it: each replaces the step's paragraph
/// (or goes at the end of its section), an empty one takes it out, and
/// everything else stays as it was.
pub fn apply(sheet: &Sheet, guide: &Guide, answers: &[String]) -> Sheet {
    let mut sheet = sheet.clone();
    for (i, (_, step)) in guide.steps().into_iter().enumerate() {
        let answer = answers.get(i).map(|a| tidy(a)).unwrap_or_default();
        let at = match sheet.sections.iter().position(|s| s.heading.trim().eq_ignore_ascii_case(step.section)) {
            Some(at) => at,
            None if answer.is_empty() => continue,
            None => {
                sheet.sections.push(Section { heading: step.section.to_owned(), text: String::new() });
                sheet.sections.len() - 1
            }
        };
        let mut paras = paragraphs(&sheet.sections[at].text);
        let found = paras.iter().position(|p| labelled(p, step.name).is_some());
        let line = format!("**{}:** {answer}", step.name);
        match (found, answer.is_empty()) {
            (Some(f), true) => {
                paras.remove(f);
            }
            (Some(f), false) => paras[f] = line,
            (None, false) => paras.push(line),
            (None, true) => {}
        }
        sheet.sections[at].text = paras.join("\n\n");
    }
    sheet
}

/// An answer as one paragraph: its lines kept, blank lines between them
/// dropped (a blank line would end it).
fn tidy(answer: &str) -> String {
    answer.lines().map(str::trim_end).filter(|l| !l.trim().is_empty()).collect::<Vec<_>>().join("\n").trim().to_owned()
}

const fn wants(about: &'static str, question: &'static str) -> Step {
    in_section(MOTIVATIONS, "Wants", about, question)
}

const fn needs(about: &'static str, question: &'static str) -> Step {
    in_section(MOTIVATIONS, "Needs", about, question)
}

const fn wound(about: &'static str, question: &'static str) -> Step {
    in_section(BACKSTORY, "The Wound", about, question)
}

const fn lie(about: &'static str, question: &'static str) -> Step {
    in_section(ARC, "The Lie", about, question)
}

/// The character guides, in their groups.
pub const GUIDES: &[Guide] = &[
    Guide {
        name: "Quick Sketch",
        group: "Anyone",
        from: "the essentials",
        suits: "Every character, and a first pass at the main ones",
        about: "Five things that make someone feel real: what they want, what they need, what gets in their way, how they sound and how we'll remember them.",
        parts: &[Part {
            name: None,
            steps: &[
                wants("What they're after, the thing they'd tell you.", "What do they want?"),
                needs("What would actually make them whole, often not what they want.", "What do they need, whether they know it or not?"),
                in_section(PERSONALITY, "Flaw", "The habit or blind spot that keeps tripping them up.", "What's their flaw, and when does it show?"),
                in_section(VOICE, "Voice", "How they sound: the words they reach for, what they never say.", "What would they say, and how?"),
                in_section(LOOKS, "Telling Detail", "One thing a reader would remember them by.", "What do we notice first, or never forget?"),
            ],
        }],
    },
    Guide {
        name: "Positive Change Arc",
        group: "Arcs",
        from: "after K. M. Weiland",
        suits: "Leads who grow",
        about: "They believe a Lie, want what it tells them to want, and learn, at a cost, the Truth they need.",
        parts: &[Part {
            name: None,
            steps: &[
                lie("The false belief about themselves or the world they live by.", "What do they believe that isn't true?"),
                wound("What happened that taught them the Lie.", "Where did the Lie come from?"),
                wants("What the Lie tells them will fix things.", "What do they want, because of the Lie?"),
                needs("The Truth that would set them free.", "What do they need to learn?"),
                in_section(ARC, "First Seen", "The moment we meet them, the Lie at work.", "What do we first see them do, that shows the Lie?"),
                in_section(ARC, "Glimpse of the Truth", "Around the middle, they see the Truth, but don't yet act on it.", "When do they first see it, and why don't they change yet?"),
                in_section(ARC, "Choosing the Truth", "At the climax they give up the Lie, and it costs them.", "What do they choose, and what does it cost?"),
                in_section(ARC, "Afterwards", "Who they are, living by the Truth.", "How are they different at the end?"),
            ],
        }],
    },
    Guide {
        name: "Flat Arc",
        group: "Arcs",
        from: "after K. M. Weiland",
        suits: "Steady heroes, mentors, detectives",
        about: "They don't change: they hold to a Truth in a world that believes a Lie, and the world changes around them.",
        parts: &[Part {
            name: None,
            steps: &[
                in_section(ARC, "The Truth", "What they know, and hold to.", "What do they believe that's true?"),
                in_section(ARC, "The World's Lie", "What the people around them believe instead.", "What's the world getting wrong?"),
                wants("What they're after in the story.", "What do they want?"),
                in_section(ARC, "Tested", "When holding to the Truth is hardest.", "What tempts or forces them to doubt it?"),
                in_section(RELATIONSHIPS, "Who Changes", "The people who come round to the Truth because of them.", "Who changes, and how?"),
            ],
        }],
    },
    Guide {
        name: "Negative Arc",
        group: "Arcs",
        from: "after K. M. Weiland",
        suits: "Tragic leads, villains' origins, falls",
        about: "They end worse than they began: disillusioned by a hard Truth, or falling deeper into a Lie, or trading the Truth for one.",
        parts: &[Part {
            name: None,
            steps: &[
                in_section(ARC, "The Fall", "Disillusionment: they find the Truth, and it's bitter. A fall: they reject the Truth for a worse Lie. Corruption: they had the Truth and trade it for a Lie.", "Which kind of fall is it?"),
                lie("The belief that leads them down.", "What do they believe that will ruin them?"),
                wound("What made the Lie seem true.", "Where does it come from?"),
                wants("What they're after, and why it's dangerous.", "What do they want?"),
                in_section(ARC, "The Turning", "The moment they could choose the Truth, and don't.", "Where could they have turned back?"),
                in_section(ARC, "What It Costs", "What they lose, and who else pays.", "What does the fall cost?"),
                in_section(ARC, "At the End", "Who they've become.", "Where does it leave them?"),
            ],
        }],
    },
    Guide {
        name: "Antagonist",
        group: "Opposition",
        from: "the essentials",
        suits: "Villains, rivals, anyone in the hero's way",
        about: "A worthy opponent: someone with good reasons, a mirror of the hero, and a line the hero won't cross.",
        parts: &[Part {
            name: None,
            steps: &[
                wants("What they're after, the thing the hero stands in the way of.", "What do they want?"),
                in_section(MOTIVATIONS, "Why They're Right", "In their own story, they're the hero.", "Why are they sure they're right?"),
                wound("What made them this way.", "What happened to them?"),
                in_section(RELATIONSHIPS, "Mirror", "How they reflect the hero: the same wound, another choice.", "What do they share with the hero, and where do they part?"),
                in_section(ARC, "The Line", "What they'll do that the hero never would.", "What line will they cross?"),
                in_section(PERSONALITY, "Weakness", "What could bring them down.", "Where are they vulnerable?"),
            ],
        }],
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    fn guide(name: &str) -> &'static Guide {
        GUIDES.iter().find(|g| g.name == name).unwrap()
    }

    #[test]
    fn every_step_has_a_section() {
        for g in GUIDES {
            assert!(g.steps().iter().all(|(_, s)| !s.section.is_empty()), "{}", g.name);
        }
    }

    #[test]
    fn answers_go_in_their_sections_and_come_back_out() {
        let sheet = Sheet::parse("The eldest.\n\n## Motivations\n\nShe wrote this herself.\n\n## Voice\n\nDry.\n");
        let sketch = guide("Quick Sketch");
        let answers = vec!["To cross the river".to_owned(), "To be let go\n\nby her father".to_owned(), String::new(), "Short words.".to_owned(), String::new()];
        let made = apply(&sheet, sketch, &answers);
        assert_eq!(made.description, "The eldest.");
        assert_eq!(section(&made, MOTIVATIONS).unwrap().text, "She wrote this herself.\n\n**Wants:** To cross the river\n\n**Needs:** To be let go\nby her father");
        assert_eq!(section(&made, VOICE).unwrap().text, "Dry.\n\n**Voice:** Short words.");
        // Nothing to add, no section added.
        assert!(section(&made, PERSONALITY).is_none());
        // Read back, through the file.
        let again = Sheet::parse(&made.to_text());
        assert_eq!(read(&again, sketch), ["To cross the river", "To be let go\nby her father", "", "Short words.", ""]);
        // Another guide sees the same Wants.
        assert_eq!(read(&again, guide("Positive Change Arc"))[2], "To cross the river");
    }

    #[test]
    fn revising_changes_only_the_answers() {
        let sketch = guide("Quick Sketch");
        let once = apply(&Sheet::default(), sketch, &["Gold".into(), "Rest".into(), String::new(), String::new(), "A scar".into()]);
        assert_eq!(once.sections.iter().map(|s| s.heading.as_str()).collect::<Vec<_>>(), [MOTIVATIONS, LOOKS]);
        let mut edited = once.clone();
        edited.sections[0].text.push_str("\n\nAnd a note of mine.");
        let twice = apply(&edited, sketch, &["More gold".into(), String::new(), String::new(), String::new(), "A scar".into()]);
        assert_eq!(section(&twice, MOTIVATIONS).unwrap().text, "**Wants:** More gold\n\nAnd a note of mine.");
    }
}
