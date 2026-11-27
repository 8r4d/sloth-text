//! Build a Plot: story structures to walk through, beat by beat, and the
//! document the answers become (see plot_ui.rs for the walk).
//!
//! The document is plain Markdown in Notes: a `#` heading per beat (or a
//! `#` per act and a `##` per beat), each beat's guidance as a note, which
//! compiling and word counts leave out, and the answer under it, to write
//! on from. Break Up at headings makes each beat (or act) a document of its
//! own when the plot's ready to become the draft.
//!
//! ```markdown
//! <!-- Story Circle: eight steps, out into the unknown and back changed. -->
//!
//! # You
//! <!-- Someone in their comfort zone. Who are they, and what's their everyday? -->
//! Mara, the ferryman's daughter, who has never crossed.
//! ```

/// One step of a structure.
pub struct Beat {
    pub name: &'static str,
    /// What happens here, in a sentence or two.
    pub about: &'static str,
    /// What to ask yourself.
    pub question: &'static str,
    /// Roughly where it falls, as a share of the way through (0.10 is a
    /// tenth of the way in), for the structures that pace by length.
    pub share: Option<f32>,
    /// Each line of the answer (`Name: their story`) becomes a character
    /// profile.
    pub characters: bool,
}

/// A run of beats under an act's heading (or none).
pub struct Act {
    pub name: Option<&'static str>,
    pub beats: &'static [Beat],
}

/// The headings structures are listed under.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Short,
    Long,
    Genre,
    Method,
}

impl Group {
    pub const ALL: [Group; 4] = [Group::Short, Group::Long, Group::Genre, Group::Method];

    pub fn name(self) -> &'static str {
        match self {
            Group::Short => "Short stories",
            Group::Long => "Novels",
            Group::Genre => "By genre",
            Group::Method => "A way of working",
        }
    }
}

/// A story structure.
pub struct Structure {
    pub name: &'static str,
    /// Where it's listed.
    pub group: Group,
    /// Whose it is, or where it's from.
    pub from: &'static str,
    /// What it's good for: "Short stories", "Novels"…
    pub suits: &'static str,
    pub about: &'static str,
    pub acts: &'static [Act],
}

impl Structure {
    /// Every beat, in order, with its act's name.
    pub fn beats(&self) -> Vec<(Option<&'static str>, &'static Beat)> {
        self.acts.iter().flat_map(|a| a.beats.iter().map(move |b| (a.name, b))).collect()
    }

    pub fn beat_count(&self) -> usize {
        self.acts.iter().map(|a| a.beats.len()).sum()
    }

    /// The document: `answers` and `headings` (your own name for a beat,
    /// or empty for its own) line up with `beats()`; missing ones are
    /// empty. With `book_words` (the Manuscript's target), a beat that
    /// paces by length says near which word it falls.
    pub fn document(&self, answers: &[String], headings: &[String], book_words: Option<u32>) -> String {
        let mut out = format!("<!-- {}, {}: {} Break Up at headings to make each part a document of its own. -->\n", self.name, self.from, self.about);
        let acts = self.acts.iter().any(|a| a.name.is_some());
        let mut i = 0;
        for act in self.acts {
            if let Some(name) = act.name {
                out.push_str(&format!("\n# {name}\n"));
            }
            for beat in act.beats {
                let hashes = if acts { "##" } else { "#" };
                let mut note = format!("{} \u{2014} {} {}", beat.name, beat.about, beat.question);
                match (beat.share, book_words.filter(|w| *w > 0)) {
                    (Some(share), _) if share <= 0.0 => note.push_str(" At the very start."),
                    (Some(share), Some(words)) => {
                        note.push_str(&format!(" About {} of the way in, near word {}.", percent(share), round_words(share * words as f32)));
                    }
                    (Some(share), None) => note.push_str(&format!(" About {} of the way in.", percent(share))),
                    (None, _) => {}
                }
                let heading = headings.get(i).map(|h| h.trim()).filter(|h| !h.is_empty()).unwrap_or(beat.name);
                out.push_str(&format!("\n{hashes} {heading}\n{BEAT_NOTE}{note} -->\n"));
                let answer = answers.get(i).map(|a| a.trim()).unwrap_or_default();
                if !answer.is_empty() {
                    out.push_str(answer);
                    out.push('\n');
                }
                i += 1;
            }
        }
        out
    }
}

fn percent(share: f32) -> String {
    format!("{}%", (share * 100.0).round() as u32)
}

/// A friendly round number: to the nearest 100 (or 10, under a thousand).
fn round_words(words: f32) -> String {
    let step = if words < 1000.0 { 10.0 } else { 100.0 };
    let n = ((words / step).round() * step) as u32;
    if n >= 1000 { format!("{},{:03}", n / 1000, n % 1000) } else { n.to_string() }
}

const fn beat(name: &'static str, about: &'static str, question: &'static str) -> Beat {
    Beat { name, about, question, share: None, characters: false }
}

const fn paced(name: &'static str, share: f32, about: &'static str, question: &'static str) -> Beat {
    Beat { name, about, question, share: Some(share), characters: false }
}

/// A beat whose answer lists characters, one per line, each to become a
/// profile.
const fn characters(name: &'static str, about: &'static str, question: &'static str) -> Beat {
    Beat { name, about, question, share: None, characters: true }
}

/// What starts a beat's note, so Break Up can tell it from notes of your
/// own and make it the new document's synopsis.
const BEAT_NOTE: &str = "<!-- Beat: ";

/// A piece of a broken-up plot: if it opens (after its heading, if it kept
/// one) with a beat's note, the note's words, for a synopsis, and the text
/// without it.
pub fn beat_note(text: &str) -> Option<(String, String)> {
    let (heading, body) = match text.split_once('\n') {
        Some((first, rest)) if first.trim_start().starts_with('#') => (format!("{first}\n"), rest),
        _ => (String::new(), text),
    };
    let inner = body.trim_start().strip_prefix(BEAT_NOTE)?;
    let (note, rest) = inner.split_once("-->")?;
    let rest = rest.strip_prefix('\n').unwrap_or(rest);
    Some((note.trim().to_owned(), format!("{heading}{rest}")))
}

/// The characters an answer lists, one per line: `Mara: wants to cross`
/// (or a dash for the colon, with a bullet in front if you like). The
/// name, and the rest. A line that doesn't look like one is left alone.
pub fn character_lines(answer: &str) -> Vec<(String, String)> {
    answer.lines().filter_map(character_line).map(|(_, name, rest)| (name, rest)).collect()
}

/// One line as a character: what came before the name (a bullet), the
/// name, and the rest.
fn character_line(line: &str) -> Option<(String, String, String)> {
    let body = line.trim_start().trim_start_matches(['-', '*', '\u{2022}']).trim_start();
    let lead = line[..line.len() - body.len()].to_owned();
    let (name, rest) = [":", " \u{2014} ", " \u{2013} ", " - "].iter().find_map(|sep| body.split_once(sep))?;
    let name = name.trim().trim_matches(|c| c == '[' || c == ']' || c == '*').trim();
    let short = !name.is_empty() && name.split_whitespace().count() <= 4 && name.len() <= 40;
    short.then(|| (lead, name.to_owned(), rest.trim().to_owned()))
}

/// The answer with each character's name as a reference to their profile:
/// `[[Mara]]: wants to cross`.
pub fn link_characters(answer: &str) -> String {
    answer
        .lines()
        .map(|line| match character_line(line) {
            Some((lead, name, rest)) if rest.is_empty() => format!("{lead}[[{name}]]"),
            Some((lead, name, rest)) => format!("{lead}[[{name}]]: {rest}"),
            None => line.to_owned(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The structures offered, in their groups.
pub const STRUCTURES: &[Structure] = &[
    Structure {
        name: "Story Spine",
        group: Group::Short,
        from: "Kenn Adams",
        suits: "Short stories, fables, a first sketch",
        about: "Seven sentence starters that make a story by finishing them.",
        acts: &[Act {
            name: None,
            beats: &[
                beat("Once upon a time\u{2026}", "The world and who's in it, before anything changes.", "Who is this about, and where?"),
                beat("Every day\u{2026}", "Their routine: what's normal for them.", "What do they do, day in, day out?"),
                beat("Until one day\u{2026}", "Something breaks the routine.", "What happens that's never happened before?"),
                beat("Because of that\u{2026}", "The first consequence.", "What do they do about it, and what does that lead to?"),
                beat("Because of that\u{2026}", "The consequences pile up, each causing the next.", "And then what does that lead to?"),
                beat("Until finally\u{2026}", "The climax: it comes to a head.", "What finally settles it?"),
                beat("Ever since then\u{2026}", "The new normal.", "How is the world, or they, different now?"),
            ],
        }],
    },
    Structure {
        name: "Story Circle",
        group: Group::Short,
        from: "Dan Harmon",
        suits: "Short stories, episodes, a chapter or a subplot",
        about: "Eight steps around a circle: out into the unknown, a price paid, and home again, changed.",
        acts: &[Act {
            name: None,
            beats: &[
                beat("You", "Someone in their comfort zone.", "Who are they, and what's their everyday?"),
                beat("Need", "They want something, or something's missing.", "What do they want, and what do they really need?"),
                beat("Go", "They cross into an unfamiliar situation.", "What pulls or pushes them out of the familiar?"),
                beat("Search", "They adapt: trials, allies, the rules of the new place.", "What do they try, and what goes wrong?"),
                beat("Find", "They get what they wanted.", "What do they find, and is it what they expected?"),
                beat("Take", "It costs them.", "What price do they pay for it?"),
                beat("Return", "They head back to where they began.", "How do they get back, and what comes with them?"),
                beat("Change", "They're different now, and so is home.", "How have they changed, and what does it show?"),
            ],
        }],
    },
    Structure {
        name: "Kish\u{14d}tenketsu",
        group: Group::Short,
        from: "Chinese and Japanese storytelling",
        suits: "Short stories, literary and quiet fiction",
        about: "Four parts with no need for conflict: the story turns on a twist that makes you see what came before anew.",
        acts: &[Act {
            name: None,
            beats: &[
                beat("Ki: Introduction", "The characters and their world, plainly set out.", "Who and what are we looking at?"),
                beat("Sh\u{14d}: Development", "More of the same, deepened; nothing breaks yet.", "What do we come to know and care about?"),
                beat("Ten: Twist", "Something unexpected, often from elsewhere entirely, enters.", "What arrives that seems unrelated, or changes the angle?"),
                beat("Ketsu: Reconciliation", "The twist and what came before are brought together.", "How do the two fit, and what does that reveal?"),
            ],
        }],
    },
    Structure {
        name: "Freytag's Pyramid",
        group: Group::Short,
        from: "Gustav Freytag",
        suits: "Short stories, plays, tragedy",
        about: "Five parts up to a peak in the middle and down the other side.",
        acts: &[Act {
            name: None,
            beats: &[
                beat("Exposition", "The situation, the people, what's at stake, and the spark that sets it off.", "What do we need to know, and what lights the fuse?"),
                beat("Rising Action", "Complications build toward the peak.", "What makes it harder, step by step?"),
                beat("Climax", "The turning point, at the top of the pyramid.", "What's the moment after which nothing can go back?"),
                beat("Falling Action", "The consequences unwind.", "What follows from the turn, and how does it close in?"),
                beat("Resolution", "The end: a catastrophe in tragedy, a settling in comedy.", "Where does everyone end up?"),
            ],
        }],
    },
    Structure {
        name: "MICE Quotient",
        group: Group::Short,
        from: "Orson Scott Card, as Mary Robinette Kowal teaches it",
        suits: "Short stories, and checking a novel's threads",
        about: "Every story thread opens a question and closes it: Milieu, Inquiry, Character or Event. Threads close in the reverse order they opened, like nested brackets.",
        acts: &[Act {
            name: None,
            beats: &[
                beat("The Threads", "Milieu: arrive somewhere, leave it. Inquiry: a question asked, answered. Character: unhappy with who they are, then changed or accepting. Event: the world's order broken, then restored.", "Which thread is your story's main one, and which others run inside it?"),
                beat("Open the Outer Thread", "The story begins where its main thread opens.", "What's the arrival, the question, the unease or the disruption at the start?"),
                beat("Open the Inner Threads", "Further threads open inside the first.", "What other questions open, and in what order?"),
                beat("Try and Fail", "Attempts that go \u{201c}yes, but\u{201d} or \u{201c}no, and\u{201d}, raising the stakes.", "What do they try, and how does each try make things worse?"),
                beat("Close the Inner Threads", "The inner threads close, the last opened first.", "How does each resolve, in reverse order?"),
                beat("Close the Outer Thread", "The story ends when its first thread closes, the way it opened.", "How does the main thread close, in its own kind (a departure, an answer, a change, order restored)?"),
            ],
        }],
    },
    Structure {
        name: "Three Acts",
        group: Group::Long,
        from: "the classic shape",
        suits: "Novels, screenplays, any length",
        about: "A beginning, a middle twice as long, and an end, turned by a few key moments.",
        acts: &[
            Act {
                name: Some("Act One"),
                beats: &[
                    paced("Setup", 0.0, "The world, the main character and what they want, before it starts.", "What do we need to know, and care about, first?"),
                    paced("Inciting Incident", 0.12, "The event that starts the story.", "What happens that can't be ignored?"),
                    paced("First Plot Point", 0.25, "They commit, and there's no going back.", "What do they decide, and what door closes behind them?"),
                ],
            },
            Act {
                name: Some("Act Two"),
                beats: &[
                    paced("Rising Action", 0.37, "Trying, failing, learning; the stakes climb.", "What stands in their way, and how do they push on?"),
                    paced("Midpoint", 0.5, "A turn in the middle: a revelation, a false victory or a defeat.", "What changes how they see the problem?"),
                    paced("Complications", 0.62, "Things get harder; the opposition closes in.", "What goes wrong now that they know more?"),
                    paced("Second Plot Point", 0.75, "The lowest point, and the push into the end.", "What do they lose, and what gets them up again?"),
                ],
            },
            Act {
                name: Some("Act Three"),
                beats: &[
                    paced("Climax", 0.88, "The final confrontation.", "How do they face it, and what decides it?"),
                    paced("Resolution", 0.98, "What's left after.", "What's the new normal, and what does it mean?"),
                ],
            },
        ],
    },
    Structure {
        name: "Seven Points",
        group: Group::Long,
        from: "Dan Wells",
        suits: "Novels, especially plotted genre fiction",
        about: "Seven points from a starting state to its opposite. Many write the Resolution first and work back to the Hook.",
        acts: &[Act {
            name: None,
            beats: &[
                paced("Hook", 0.0, "Where they start: the opposite of where they'll end.", "Who are they at the start, so the ending is a change?"),
                paced("First Plot Turn", 0.15, "Something sets the story going: a call, a discovery.", "What moves them from the Hook toward the conflict?"),
                paced("First Pinch", 0.3, "The opposition shows its strength; pressure's applied.", "What forces them to act?"),
                paced("Midpoint", 0.5, "They stop reacting and start acting.", "What makes them take charge?"),
                paced("Second Pinch", 0.65, "More pressure, and a real loss: the plan fails, a mentor falls.", "What do they lose, and how bad can it get?"),
                paced("Second Plot Turn", 0.8, "They find the last thing they need.", "What do they learn or get that lets them win?"),
                paced("Resolution", 0.95, "The climax, and the change made good.", "How does it end, the opposite of the Hook?"),
            ],
        }],
    },
    Structure {
        name: "Fichtean Curve",
        group: Group::Long,
        from: "after John Gardner",
        suits: "Thrillers, mysteries, fast openings",
        about: "Straight into trouble: little setup, a run of crises each worse than the last, a climax and a short fall.",
        acts: &[Act {
            name: None,
            beats: &[
                paced("First Crisis", 0.05, "Open in trouble; explain the world as you go.", "What's going wrong on the first page?"),
                paced("Second Crisis", 0.25, "It gets worse, and we learn more about who they are.", "What new crisis comes from dealing with the first?"),
                paced("Third Crisis", 0.45, "Higher stakes; a secret or a backstory comes out.", "What does this crisis reveal?"),
                paced("Fourth Crisis", 0.65, "The worst yet; everything's on the line.", "What's the crisis that pushes them to the brink?"),
                paced("Climax", 0.85, "The peak, everything tested.", "How is it decided?"),
                paced("Falling Action", 0.95, "A short way down: what it cost, what's changed.", "What's left, and who are they now?"),
            ],
        }],
    },
    Structure {
        name: "Six Stages",
        group: Group::Long,
        from: "Michael Hauge",
        suits: "Novels and screenplays with a strong inner journey",
        about: "Six stages and five turning points, an outer journey toward a visible goal and an inner one from the identity the hero hides behind to their true self.",
        acts: &[
            Act {
                name: Some("Act One"),
                beats: &[
                    paced("The Setup", 0.0, "The hero's everyday life, and the wound and armour (their identity) they live in.", "Who are they, and what are they protecting themselves from?"),
                    paced("Opportunity", 0.1, "Something new comes along.", "What opportunity starts them moving?"),
                    paced("New Situation", 0.15, "They adjust to the new circumstances.", "What's new, and how do they get the lie of it?"),
                    paced("Change of Plans", 0.25, "A specific, visible goal appears: the outer journey.", "What exactly do they now set out to do?"),
                ],
            },
            Act {
                name: Some("Act Two"),
                beats: &[
                    paced("Progress", 0.35, "The plan seems to work; they glimpse their true self.", "What goes right, and where does their essence show through?"),
                    paced("Point of No Return", 0.5, "They commit fully; there's no going back.", "What do they risk that they can't take back?"),
                    paced("Complications and Higher Stakes", 0.6, "Harder, with more to lose; the identity pulls back.", "What gets in the way, and how do they retreat into their armour?"),
                    paced("Major Setback", 0.75, "Everything seems lost.", "What disaster leaves the goal out of reach?"),
                ],
            },
            Act {
                name: Some("Act Three"),
                beats: &[
                    paced("Final Push", 0.8, "Everything on the line, living as their true self.", "What do they risk, and what armour do they drop?"),
                    paced("Climax", 0.9, "The outer goal is won or lost; the inner journey completes.", "How does it resolve, inside and out?"),
                    paced("Aftermath", 0.98, "The new life.", "What does living as their true self look like?"),
                ],
            },
        ],
    },
    Structure {
        name: "The Hero's Journey",
        group: Group::Long,
        from: "after Joseph Campbell and Christopher Vogler",
        suits: "Novels, adventure and fantasy, quests",
        about: "Twelve stages of a journey into a special world and back with something to give.",
        acts: &[
            Act {
                name: Some("Departure"),
                beats: &[
                    paced("The Ordinary World", 0.0, "The hero's everyday, and what's lacking in it.", "Who are they, and what's missing?"),
                    paced("The Call to Adventure", 0.08, "A problem, a challenge or an offer.", "What calls them away?"),
                    paced("Refusal of the Call", 0.12, "Doubt, fear, reluctance.", "Why don't they want to go?"),
                    paced("Meeting the Mentor", 0.17, "Someone or something gives advice, a gift, courage.", "Who helps them over their doubt, and with what?"),
                    paced("Crossing the Threshold", 0.25, "They commit and enter the special world.", "What's the moment they really leave?"),
                ],
            },
            Act {
                name: Some("Initiation"),
                beats: &[
                    paced("Tests, Allies, Enemies", 0.35, "Learning the special world's rules; finding friends and foes.", "Who do they meet, and what do they learn?"),
                    paced("Approach to the Inmost Cave", 0.45, "Preparing for the central ordeal.", "What's the danger ahead, and how do they get ready?"),
                    paced("The Ordeal", 0.5, "The greatest challenge yet: a death and rebirth, literal or not.", "What do they face, and what dies in them?"),
                    paced("The Reward", 0.6, "What they win from surviving.", "What do they seize: a treasure, a truth, a bond?"),
                ],
            },
            Act {
                name: Some("Return"),
                beats: &[
                    paced("The Road Back", 0.75, "Leaving with the reward; consequences follow.", "What chases them, or calls them home?"),
                    paced("The Resurrection", 0.88, "A final test where everything they've learned is used.", "How are they tested one last time, and how are they reborn?"),
                    paced("Return with the Elixir", 0.97, "Home, changed, with something for the ordinary world.", "What do they bring back, and who's it for?"),
                ],
            },
        ],
    },
    Structure {
        name: "Save the Cat!",
        group: Group::Long,
        from: "Blake Snyder, as Jessica Brody adapts it for novels",
        suits: "Novels and screenplays, commercial fiction",
        about: "Fifteen beats, each at a set point in the book, so a draft's pacing can be checked against them.",
        acts: &[
            Act {
                name: Some("Act One"),
                beats: &[
                    paced("Opening Image", 0.0, "A snapshot of the hero's life before.", "What one picture shows who they are and what's wrong?"),
                    paced("Theme Stated", 0.05, "Someone hints at what the hero must learn; the hero doesn't get it yet.", "What's the lesson, and who says it?"),
                    paced("Setup", 0.05, "The hero's world, their flaws and the people in it.", "What needs fixing in their life?"),
                    paced("Catalyst", 0.1, "Something happens to them that changes everything.", "What's the life-changing event?"),
                    paced("Debate", 0.15, "They hesitate: what should they do?", "What question do they wrestle with?"),
                ],
            },
            Act {
                name: Some("Act Two"),
                beats: &[
                    paced("Break into Two", 0.2, "They choose, and step into a new world or a new way.", "What's the decision that starts the journey?"),
                    paced("B Story", 0.22, "A new character or relationship that carries the theme.", "Who helps them learn the lesson?"),
                    paced("Fun and Games", 0.3, "The promise of the premise: the hero in the new world, succeeding or failing.", "What's the fun of this story, the part on the poster?"),
                    paced("Midpoint", 0.5, "A false victory or a false defeat; the stakes rise.", "What raises the stakes, and how does it look like winning or losing?"),
                    paced("Bad Guys Close In", 0.55, "Inner flaws and outer enemies tighten.", "What goes wrong, from outside and from within?"),
                    paced("All Is Lost", 0.75, "The lowest point; often something or someone dies.", "What's lost, and what's the whiff of death?"),
                    paced("Dark Night of the Soul", 0.77, "They wallow, then begin to understand.", "What do they finally see about themselves?"),
                ],
            },
            Act {
                name: Some("Act Three"),
                beats: &[
                    paced("Break into Three", 0.8, "The idea that fixes things, from the lesson learned.", "What's the solution, and where does it come from?"),
                    paced("Finale", 0.8, "They put the lesson to work and set the world right.", "How do they prove they've changed?"),
                    paced("Final Image", 0.99, "The opposite of the opening image.", "What picture shows how far they've come?"),
                ],
            },
        ],
    },
    Structure {
        name: "Romancing the Beat",
        group: Group::Genre,
        from: "Gwen Hayes",
        suits: "Romance, and any love story",
        about: "Four phases of a love story, each written from both leads: setting up, falling in love, retreating from it and fighting for it.",
        acts: &[
            Act {
                name: Some("Setup"),
                beats: &[
                    beat("Introduce the First Lead", "One lead in their world, missing something they don't yet see.", "Who are they, and what's their wound or lack?"),
                    beat("Introduce the Second Lead", "The other lead, likewise.", "Who are they, and why would they be wrong for the first, on paper?"),
                    beat("Meet Cute", "They meet, and there's a spark, or friction.", "How do they meet, and what's charged about it?"),
                    beat("No Way", "Reasons this could never work.", "Why does each think it's impossible?"),
                    beat("Adhesion", "Something keeps them together anyway.", "What forces them to stay close?"),
                ],
            },
            Act {
                name: Some("Falling in Love"),
                beats: &[
                    beat("Inkling of Desire", "A first flicker of wanting.", "When does one of them first notice?"),
                    beat("Deepening Desire", "The pull grows, through what they share.", "What draws them closer?"),
                    beat("Maybe This Could Work", "They allow themselves to hope.", "What makes it seem possible?"),
                    beat("Midpoint of Love", "A big moment of intimacy: a kiss, a confession, a night.", "What's the moment they're truly together?"),
                ],
            },
            Act {
                name: Some("Retreating from Love"),
                beats: &[
                    beat("Inkling of Doubt", "Fear creeps back in.", "What reminds them why this is dangerous?"),
                    beat("Deepening Doubt", "The wound and the lack come back into play.", "What old fear grows louder?"),
                    beat("Retreat", "They pull back to protect themselves.", "How does each step away?"),
                    beat("Breakup", "It falls apart.", "What breaks them up, and whose fault does it feel like?"),
                ],
            },
            Act {
                name: Some("Fighting for Love"),
                beats: &[
                    beat("Dark Night", "Apart, each faces what they've lost.", "How bad is it without the other?"),
                    beat("Wake Up", "They see what they've done, and what they need.", "What do they finally understand about themselves?"),
                    beat("Grand Gesture", "Someone risks everything to show it.", "What do they do, and what does it cost?"),
                    beat("Whole-Hearted", "Together, healed, the lack filled.", "What does their love look like now?"),
                ],
            },
        ],
    },
    Structure {
        name: "The Snowflake Method",
        group: Group::Method,
        from: "Randy Ingermanson",
        suits: "Novels, for planners who like to start small",
        about: "Start with one sentence and grow it: a paragraph, the characters, a page, then a list of scenes. After this, write each scene from the list.",
        acts: &[Act {
            name: None,
            beats: &[
                beat("One Sentence", "The whole novel in a sentence of fifteen words or so: who, and what they want or face.", "What's the story, in one line?"),
                beat("One Paragraph", "Five sentences: the setup, three disasters, and the ending.", "What sets it up, what are the three disasters, and how does it end?"),
                characters("The Characters", "For each main character: their name, what they want, what they're after in the abstract, what's in their way, and how they change.", "Who are the main characters? One to a line: a name, a colon, and their story in a line."),
                beat("One Page", "Each sentence of the paragraph grows into a paragraph of its own.", "What happens at each step, at more length?"),
                beat("Character Stories", "The story told from each main character's point of view.", "How does each of them see it?"),
                beat("Scene List", "Every scene in order, a line each: whose point of view and what happens.", "What are the scenes, one per line?"),
            ],
        }],
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::breakup::{BreakAt, break_up};

    #[test]
    fn every_structure_has_beats_and_sensible_shares() {
        for s in STRUCTURES {
            assert!(s.beat_count() >= 4, "{}", s.name);
            let shares: Vec<f32> = s.beats().iter().filter_map(|(_, b)| b.share).collect();
            assert!(shares.windows(2).all(|w| w[0] <= w[1]), "{} in order", s.name);
            assert!(shares.iter().all(|s| (0.0..=1.0).contains(s)));
            assert!(!s.about.ends_with(' ') && s.about.ends_with('.'), "{}", s.name);
        }
    }

    #[test]
    fn the_document_has_a_heading_per_beat_with_its_answer() {
        let circle = STRUCTURES.iter().find(|s| s.name == "Story Circle").unwrap();
        let answers = vec!["Mara, who has never crossed.".to_owned(), String::new(), "  The ferry leaves without her father.  ".to_owned()];
        let headings = vec![String::new(), String::new(), " The Ferry Leaves ".to_owned()];
        let text = circle.document(&answers, &headings, None);
        assert!(text.starts_with("<!-- Story Circle, Dan Harmon:"));
        assert!(text.contains("\n# You\n<!-- Beat: You \u{2014} Someone in their comfort zone. Who are they, and what's their everyday? -->\nMara, who has never crossed.\n"));
        // Your own heading, the beat named in its note.
        assert!(text.contains("\n# The Ferry Leaves\n<!-- Beat: Go \u{2014} "));
        assert!(text.contains("-->\nThe ferry leaves without her father.\n"));
        // Broken up at headings: a document per beat, named for it.
        let broken = break_up(&text, BreakAt::Headings(1), false);
        let titles: Vec<&str> = broken.pieces.iter().map(|p| p.title.as_str()).collect();
        assert_eq!(titles, ["You", "Need", "The Ferry Leaves", "Search", "Find", "Take", "Return", "Change"]);
        // Each piece's note, ready to be its synopsis.
        let (synopsis, rest) = beat_note(&broken.pieces[0].text).unwrap();
        assert_eq!(synopsis, "You \u{2014} Someone in their comfort zone. Who are they, and what's their everyday?");
        assert_eq!(rest.trim_end(), "Mara, who has never crossed.");
        let kept = break_up(&text, BreakAt::Headings(1), true);
        let (_, rest) = beat_note(&kept.pieces[2].text).unwrap();
        assert_eq!(rest.trim_end(), "# The Ferry Leaves\nThe ferry leaves without her father.");
    }

    #[test]
    fn a_note_of_your_own_isnt_a_beat() {
        assert_eq!(beat_note("<!-- remember the tide -->\nText."), None);
        assert_eq!(beat_note("Text first.\n<!-- Beat: Go \u{2014} … -->"), None);
    }

    #[test]
    fn acts_are_headings_above_their_beats_with_word_shares() {
        let cat = STRUCTURES.iter().find(|s| s.name == "Save the Cat!").unwrap();
        let text = cat.document(&[], &[], Some(90_000));
        assert!(text.contains("\n# Act One\n\n## Opening Image\n"));
        assert!(text.contains("## Catalyst\n<!-- Beat: Catalyst \u{2014} Something happens to them that changes everything. What's the life-changing event? About 10% of the way in, near word 9,000. -->"));
        let acts = break_up(&text, BreakAt::Headings(1), true);
        assert_eq!(acts.pieces.len(), 3);
        assert_eq!(break_up(&text, BreakAt::Headings(2), false).pieces.len(), 3 + 15);
        // Without a book target, just the share.
        assert!(cat.document(&[], &[], None).contains("About 50% of the way in. -->"));
    }

    #[test]
    fn characters_one_to_a_line() {
        let answer = "Mara: wants to cross the river\n- Old Tam \u{2014} the ferryman\nAnd a crowd of others, who aren't anyone in particular: yet.\nNessa";
        assert_eq!(
            character_lines(answer),
            vec![("Mara".to_owned(), "wants to cross the river".to_owned()), ("Old Tam".to_owned(), "the ferryman".to_owned())]
        );
        assert_eq!(
            link_characters(answer),
            "[[Mara]]: wants to cross the river\n- [[Old Tam]]: the ferryman\nAnd a crowd of others, who aren't anyone in particular: yet.\nNessa"
        );
        let snowflake = STRUCTURES.iter().find(|s| s.name == "The Snowflake Method").unwrap();
        assert_eq!(snowflake.beats().iter().filter(|(_, b)| b.characters).count(), 1);
    }

    #[test]
    fn round_numbers() {
        assert_eq!(round_words(9_000.0), "9,000");
        assert_eq!(round_words(4_449.0), "4,400");
        assert_eq!(round_words(87.0), "90");
        assert_eq!(round_words(0.0), "0");
        let cat = STRUCTURES.iter().find(|s| s.name == "Save the Cat!").unwrap();
        assert!(cat.document(&[], &[], Some(90_000)).contains("Opening Image \u{2014} A snapshot of the hero's life before. What one picture shows who they are and what's wrong? At the very start. -->"));
    }
}
