//! Build an Essay: shapes for an essay or a post, and the document the
//! answers become, made where you asked for it.
//!
//! Every shape starts with the piece in a sentence (its synopsis) and ends
//! with its title, length and tags. In between, each part's guidance is a
//! note (left out when compiling or copying) and your answer is the
//! paragraph under it, so the draft reads as a post from the start:
//!
//! ```markdown
//! <!-- Part: Hook — Open with something that makes them want to read on. … -->
//! Every ferry timetable lies.
//!
//! <!-- Part: Claim — … -->
//! ```
//!
//! With section headings asked for, each part also gets a `##` heading.

use crate::guide::{Fill, Guide, NOTE_STARTS, Part, Step, filled, step};

/// Asked before every shape.
const BEFORE: &[Step] = &[filled(
    Fill::Synopsis,
    "In a Sentence",
    "What the piece says, in one sentence. It becomes the synopsis, and makes a good excerpt when you post.",
    "What's it saying?",
)];

/// Asked after every shape.
const AFTER: &[Step] = &[
    filled(Fill::Title, "Title", "Now you know what it says: a few possible titles, one to a line. Pick one below.", "What could it be called?"),
    filled(Fill::Target, "Length", "About how many words, for a target.", "How long should it be?"),
    filled(Fill::Tags, "Tags", "Keywords, separated by commas: handy as tags when you post, and to find it again.", "What's it about?"),
];

/// Every step of an essay in this shape, the shared ones included.
pub fn steps(guide: &Guide) -> Vec<(Option<&'static str>, &'static Step)> {
    let before = BEFORE.iter().map(|s| (None, s));
    let after = AFTER.iter().map(|s| (None, s));
    before.chain(guide.steps()).chain(after).collect()
}

/// The possible titles an answer lists, one to a line.
pub fn titles(answer: &str) -> Vec<String> {
    answer.lines().map(|l| l.trim().trim_start_matches(['-', '*', '\u{2022}']).trim().to_owned()).filter(|l| !l.is_empty()).collect()
}

/// A word count written any reasonable way: 1200, 1,200, about 1200 words.
pub fn target(answer: &str) -> Option<u32> {
    let digits: String = answer.split_whitespace().find(|w| w.chars().next().is_some_and(|c| c.is_ascii_digit()))?.chars().filter(char::is_ascii_digit).collect();
    digits.parse().ok().filter(|n| *n > 0)
}

pub fn tags(answer: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for tag in answer.split([',', '\n']).map(str::trim).filter(|t| !t.is_empty()) {
        if !out.iter().any(|t| t.eq_ignore_ascii_case(tag)) {
            out.push(tag.to_owned());
        }
    }
    out
}

/// What an essay's answers make.
#[derive(Debug, Default, PartialEq)]
pub struct Made {
    pub title: Option<String>,
    pub synopsis: String,
    pub target: Option<u32>,
    pub keywords: Vec<String>,
    pub text: String,
}

/// The essay: `answers` and `headings` line up with `steps(guide)`;
/// `pick` is the title chosen from the Title step's lines.
pub fn make(guide: &Guide, answers: &[String], headings: &[String], pick: usize, with_headings: bool) -> Made {
    let mut made = Made::default();
    let mut parts = Vec::new();
    for (i, (_, step)) in steps(guide).into_iter().enumerate() {
        let answer = answers.get(i).map(|a| a.trim()).unwrap_or_default();
        match step.fill {
            Fill::Synopsis => made.synopsis = answer.to_owned(),
            Fill::Title => {
                let titles = titles(answer);
                made.title = titles.get(pick).or(titles.first()).cloned();
            }
            Fill::Target => made.target = target(answer),
            Fill::Tags => made.keywords = tags(answer),
            Fill::Text | Fill::Characters => {
                let mut part = String::new();
                if with_headings {
                    let heading = headings.get(i).map(|h| h.trim()).filter(|h| !h.is_empty()).unwrap_or(step.name);
                    part.push_str(&format!("## {heading}\n"));
                }
                part.push_str(&format!("{}{} \u{2014} {} {} -->\n", NOTE_STARTS[1], step.name, step.about, step.question));
                if !answer.is_empty() {
                    part.push_str(answer);
                    part.push('\n');
                }
                parts.push(part);
            }
        }
    }
    made.text = parts.join("\n");
    made
}

/// The shapes, in their groups.
pub const GUIDES: &[Guide] = &[
    Guide {
        name: "Argument",
        group: "Arguing",
        from: "the classic shape",
        suits: "Opinion pieces, persuasive essays",
        about: "Make a claim, back it with reasons, meet the best objection, and say why it matters.",
        parts: &[Part {
            name: None,
            steps: &[
                step("Hook", "Open with something that makes them want to read on: a scene, a fact, a question.", "What's the way in?"),
                step("Claim", "What you're arguing, plainly.", "What do you want them to believe?"),
                step("First Reason", "Your strongest reason, and what shows it.", "Why, and what's the evidence?"),
                step("Second Reason", "Another reason, another kind of evidence.", "What else supports it?"),
                step("Third Reason", "One more, or a deeper version of the first.", "What clinches it?"),
                step("The Objection", "The best case against you, put fairly.", "What would a smart opponent say?"),
                step("Your Answer", "Why the claim stands anyway.", "How do you answer them?"),
                step("So What", "Why it matters beyond this piece.", "What should change if you're right?"),
            ],
        }],
    },
    Guide {
        name: "They Say / I Say",
        group: "Arguing",
        from: "after Gerald Graff and Cathy Birkenstein",
        suits: "Responses, reviews, joining a conversation",
        about: "Start from what others are saying, then add your own turn on it.",
        parts: &[Part {
            name: None,
            steps: &[
                step("They Say", "The view you're responding to, fairly summed up.", "What's the usual take, and who holds it?"),
                step("I Say", "Where you agree, disagree, or both.", "What's your take?"),
                step("Because", "Your reasons and evidence.", "Why do you think so?"),
                step("The Pushback", "What they'd say back.", "Where would they push back?"),
                step("Your Reply", "Why your view still holds.", "What do you say to that?"),
                step("Why It Matters", "What's at stake in getting it right.", "Who should care, and why?"),
            ],
        }],
    },
    Guide {
        name: "Explainer",
        group: "Explaining",
        from: "the classic shape",
        suits: "Explaining an idea, a thing, a process",
        about: "What it is, why it matters, how it works, an example, and the common misunderstanding.",
        parts: &[Part {
            name: None,
            steps: &[
                step("What It Is", "A plain definition, for someone who's never heard of it.", "What is it, simply?"),
                step("Why It Matters", "Why the reader should care.", "What difference does it make?"),
                step("How It Works", "The mechanism, step by step.", "How does it actually work?"),
                step("An Example", "One concrete case that makes it click.", "Where can we see it?"),
                step("The Common Misunderstanding", "What people usually get wrong.", "What do people think that isn't so?"),
            ],
        }],
    },
    Guide {
        name: "How-To",
        group: "Explaining",
        from: "the classic shape",
        suits: "Tutorials, guides, recipes",
        about: "Promise a result, list what's needed, walk through the steps, and warn about what goes wrong.",
        parts: &[Part {
            name: None,
            steps: &[
                step("The Promise", "What they'll be able to do by the end.", "What will they get?"),
                step("What You Need", "Tools, materials, what to know first.", "What do they need before starting?"),
                step("The Steps", "In order, one at a time.", "What are the steps?"),
                step("What Can Go Wrong", "The usual mistakes, and how to fix them.", "Where do people go wrong?"),
                step("The Result", "What it looks like done, and what next.", "What does success look like?"),
            ],
        }],
    },
    Guide {
        name: "Problem and Solution",
        group: "Explaining",
        from: "the classic shape",
        suits: "Proposals, advice, blog posts",
        about: "Name a problem, show why the usual fixes fail, and offer yours.",
        parts: &[Part {
            name: None,
            steps: &[
                step("The Problem", "What's wrong, made vivid.", "What's the problem?"),
                step("Who Has It", "Who it hurts, and how much.", "Who's it a problem for?"),
                step("Why the Usual Fixes Fail", "What people try, and why it doesn't work.", "What's been tried?"),
                step("The Fix", "Your solution.", "What would actually work?"),
                step("What Changes", "Life after the fix.", "What's different once it's solved?"),
                step("What to Do Next", "A first step the reader can take.", "What should they do now?"),
            ],
        }],
    },
    Guide {
        name: "Personal Essay",
        group: "Telling",
        from: "the classic shape",
        suits: "Memoir, reflection, columns",
        about: "A moment from your life, what you made of it then, the turn, and what you understand now.",
        parts: &[Part {
            name: None,
            steps: &[
                step("The Moment", "Open in a scene: a place, a time, what happened.", "What happened, as if we were there?"),
                step("What I Thought Then", "How you saw it at the time.", "What did you believe, or feel, back then?"),
                step("The Turn", "When that changed.", "What made you see it differently?"),
                step("What I Understand Now", "Your view from here.", "What do you know now that you didn't?"),
                step("The Wider Truth", "What it says beyond you.", "What might it mean for the reader?"),
            ],
        }],
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::breakup::{BreakAt, break_up};
    use crate::guide::step_note;

    fn argument() -> &'static Guide {
        GUIDES.iter().find(|g| g.name == "Argument").unwrap()
    }

    #[test]
    fn every_shape_starts_with_a_sentence_and_ends_with_title_length_tags() {
        for g in GUIDES {
            let all = steps(g);
            assert_eq!(all.len(), g.step_count() + 4, "{}", g.name);
            assert_eq!(all[0].1.fill, Fill::Synopsis);
            assert_eq!(all.iter().rev().take(3).map(|(_, s)| s.fill).collect::<Vec<_>>(), [Fill::Tags, Fill::Target, Fill::Title]);
        }
    }

    #[test]
    fn an_essay_reads_as_a_post_with_its_notes() {
        let mut answers = vec![String::new(); steps(argument()).len()];
        answers[0] = "Timetables are promises nobody keeps.".into();
        answers[1] = "Every ferry timetable lies.".into();
        answers[2] = "They should say so.".into();
        let n = answers.len();
        answers[n - 3] = "The Timetable Lies\n- Nobody Keeps Time".into();
        answers[n - 2] = "about 1,200 words".into();
        answers[n - 1] = "ferries, time, Ferries".into();
        let made = make(argument(), &answers, &[], 1, false);
        assert_eq!(made.title.as_deref(), Some("Nobody Keeps Time"));
        assert_eq!(made.synopsis, "Timetables are promises nobody keeps.");
        assert_eq!(made.target, Some(1200));
        assert_eq!(made.keywords, ["ferries", "time"]);
        assert!(made.text.starts_with("<!-- Part: Hook \u{2014} Open with something"), "{}", made.text);
        assert!(made.text.contains("-->\nEvery ferry timetable lies.\n\n<!-- Part: Claim"));
        assert!(!made.text.contains("##"));
        assert!(!made.text.contains("In a Sentence"));
        assert_eq!(crate::compile::strip_notes(&made.text).split_whitespace().collect::<Vec<_>>().join(" "), "Every ferry timetable lies. They should say so.");
    }

    #[test]
    fn with_headings_it_breaks_up_with_synopses() {
        let n = steps(argument()).len();
        let mut headings = vec![String::new(); n];
        headings[1] = "Why Timetables Lie".into();
        let made = make(argument(), &vec![String::new(); n], &headings, 0, true);
        assert!(made.text.starts_with("## Why Timetables Lie\n<!-- Part: Hook"));
        let broken = break_up(&made.text, BreakAt::Headings(2), false);
        assert_eq!(broken.pieces.len(), 8);
        assert!(step_note(&broken.pieces[1].text).unwrap().0.starts_with("Claim \u{2014} "));
        assert_eq!(made.title, None);
    }

    #[test]
    fn reading_lengths() {
        assert_eq!(target("1200"), Some(1200));
        assert_eq!(target("about 1,500 words"), Some(1500));
        assert_eq!(target("short"), None);
        assert_eq!(target("0"), None);
    }
}
