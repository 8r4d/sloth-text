//! The builders in the app: Build a Plot, Build a Character and Build an
//! Essay. Each picks a guide, walks its steps one at a time, and makes
//! something of the answers: a plot document in Notes (plot.rs), a
//! character's profile (cast.rs), or an essay where you asked for it
//! (essay.rs). Closing the window part way keeps the answers until Writer
//! quits, so it opens again where you were.

use eframe::egui::{self, RichText};

use super::WriterApp;
use crate::character::Sheet;
use crate::guide::{Fill, Guide, Step, groups};
use crate::project::{CHARACTERS, Meta, Place, Project, Role, profile_presets};
use crate::{cast, essay, plot};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Builder {
    Plot,
    Character,
    Essay,
}

impl Builder {
    fn title(self) -> &'static str {
        match self {
            Builder::Plot => "Build a Plot",
            Builder::Character => "Build a Character",
            Builder::Essay => "Build an Essay",
        }
    }

    fn intro(self) -> &'static str {
        match self {
            Builder::Plot => "Walk through a story structure a beat at a time. Your answers become a document in Notes, a heading for each beat, to write on from.",
            Builder::Character => "Walk through a character a question at a time. Each answer goes in their profile, in the section it belongs to; anything you wrote there yourself stays.",
            Builder::Essay => "Walk through an essay's shape a part at a time. Your answers become the draft, each part's guidance a note above it that copying and compiling leave out.",
        }
    }

    pub(super) fn guides(self) -> &'static [Guide] {
        match self {
            Builder::Plot => plot::GUIDES,
            Builder::Character => cast::GUIDES,
            Builder::Essay => essay::GUIDES,
        }
    }

    /// The guide picked to begin with.
    fn first(self) -> usize {
        let name = match self {
            Builder::Plot => "Story Circle",
            Builder::Character => "Quick Sketch",
            Builder::Essay => "Argument",
        };
        self.guides().iter().position(|g| g.name == name).unwrap_or(0)
    }

    fn steps(self, guide: &'static Guide) -> Vec<(Option<&'static str>, &'static Step)> {
        match self {
            Builder::Essay => essay::steps(guide),
            _ => guide.steps(),
        }
    }

    /// What a step's on: a plot's beat, a character's question, an
    /// essay's part.
    fn unit(self) -> &'static str {
        match self {
            Builder::Plot => "beat",
            Builder::Character => "question",
            Builder::Essay => "part",
        }
    }
}

/// A walk under way.
pub(super) struct Walk {
    pub builder: Builder,
    /// Shown (closing it keeps the rest).
    pub open: bool,
    /// The guide picked (an index into the builder's guides); `None`
    /// while choosing.
    pub guide: Option<usize>,
    /// Highlighted in the list while choosing.
    pub picked: usize,
    /// The step on show.
    pub step: usize,
    pub answers: Vec<String>,
    /// Your own heading for each step (empty: the step's name).
    pub headings: Vec<String>,
    /// The guide the answers are for.
    answers_for: Option<usize>,
    /// A plot's title, or a new character's name.
    pub title: String,
    /// The character's profile (`None`: a new one, called `title`).
    pub profile: Option<String>,
    /// Where an essay goes: near this item.
    pub near: Option<String>,
    /// An essay with a `##` heading for each part.
    pub with_headings: bool,
    /// The title picked from an essay's Title step.
    pub title_pick: usize,
    /// Put the cursor in the answer on the next frame.
    focus: bool,
}

impl Walk {
    fn new(builder: Builder) -> Self {
        Walk {
            builder,
            open: true,
            guide: None,
            picked: builder.first(),
            step: 0,
            answers: Vec::new(),
            headings: Vec::new(),
            answers_for: None,
            title: String::new(),
            profile: None,
            near: None,
            with_headings: false,
            title_pick: 0,
            focus: false,
        }
    }

    fn guide(&self) -> Option<&'static Guide> {
        self.guide.and_then(|i| self.builder.guides().get(i))
    }

    fn steps(&self) -> Vec<(Option<&'static str>, &'static Step)> {
        self.guide().map(|g| self.builder.steps(g)).unwrap_or_default()
    }

    /// Whether a step gets a heading of its own.
    fn shows_headings(&self) -> bool {
        match self.builder {
            Builder::Plot => true,
            Builder::Character => false,
            Builder::Essay => self.with_headings,
        }
    }
}

/// What the window asked for this frame.
enum Do {
    Nothing,
    Begin,
    Make,
    Close,
}

impl WriterApp {
    /// Build a Plot…
    pub(super) fn start_plot(&mut self) {
        self.start_walk(Builder::Plot, |_| true, |_| {});
    }

    /// Build a Character… for this profile, or a new one.
    pub(super) fn start_character(&mut self, profile: Option<String>) {
        let wanted = profile.clone();
        self.start_walk(Builder::Character, |w| w.profile == wanted, |w| w.profile = profile);
    }

    /// Build an Essay…, to go near this item.
    pub(super) fn start_essay(&mut self, near: Option<String>) {
        self.start_walk(Builder::Essay, |_| true, |w| w.near = near);
    }

    /// Open the builder's window: as it was left if there's a walk under
    /// way that `same` says is this one, or a new one set up by `set`.
    /// Any other walk's window closes (keeping its answers).
    fn start_walk(&mut self, builder: Builder, same: impl Fn(&Walk) -> bool, set: impl FnOnce(&mut Walk)) {
        for walk in &mut self.walks {
            walk.open = false;
        }
        match self.walks.iter_mut().find(|w| w.builder == builder && same(w)) {
            Some(walk) => walk.open = true,
            None => {
                let mut walk = Walk::new(builder);
                set(&mut walk);
                self.walks.push(walk);
            }
        }
    }

    /// The walk on show, if any.
    #[cfg(test)]
    pub(super) fn open_walk(&mut self) -> Option<&mut Walk> {
        self.walks.iter_mut().find(|w| w.open)
    }

    pub(super) fn walk_window(&mut self, ctx: &egui::Context) {
        let Some(at) = self.walks.iter().position(|w| w.open) else { return };
        let profile_name = self.walks[at].profile.as_ref().and_then(|id| self.project.as_ref()?.node(id)).map(|n| n.title.clone());
        let walk = &mut self.walks[at];
        let mut open = true;
        let mut todo = Do::Nothing;
        egui::Window::new(walk.builder.title())
            .id(egui::Id::unique("guide-walk"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(460.0)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .show(ctx, |ui| {
                ui.set_width(460.0);
                todo = match walk.guide() {
                    None => choose(ui, walk, profile_name.as_deref()),
                    Some(guide) => step_page(ui, walk, guide),
                };
            });
        if !open || matches!(todo, Do::Close) {
            walk.open = false;
        }
        let now = ctx.input(|i| i.time);
        match todo {
            Do::Begin => self.begin_walk(at),
            Do::Make => {
                let walk = self.walks.remove(at);
                let kept = match walk.builder {
                    Builder::Plot => self.make_plot(&walk, now),
                    Builder::Character => self.make_character(&walk, now),
                    Builder::Essay => self.make_essay(&walk, now),
                };
                // It didn't work: keep the answers, to try again.
                if !kept {
                    self.walks.push(Walk { open: false, ..walk });
                }
            }
            Do::Nothing | Do::Close => {}
        }
    }

    /// Into the steps of the guide picked. The answers are kept if they're
    /// for this guide; a character's start from their profile.
    fn begin_walk(&mut self, at: usize) {
        // A new character with the name of one there already: that one.
        if self.walks[at].builder == Builder::Character && self.walks[at].profile.is_none() {
            let name = self.walks[at].title.trim().to_owned();
            let existing = self.project.as_ref().and_then(|p| {
                p.profile_items(CHARACTERS).into_iter().find(|n| n.title.trim().eq_ignore_ascii_case(&name)).map(|n| n.id.clone())
            });
            self.walks[at].profile = existing;
        }
        let sheet = self.walks[at].profile.as_ref().and_then(|id| self.document_text(id)).map(|t| Sheet::parse(&t));
        let walk = &mut self.walks[at];
        let guide = &walk.builder.guides()[walk.picked];
        if walk.answers_for != Some(walk.picked) {
            let count = walk.builder.steps(guide).len();
            walk.answers = match (&sheet, walk.builder) {
                (Some(sheet), Builder::Character) => cast::read(sheet, guide),
                _ => vec![String::new(); count],
            };
            walk.headings = vec![String::new(); count];
            walk.answers_for = Some(walk.picked);
            walk.step = 0;
        }
        walk.guide = Some(walk.picked);
        walk.focus = true;
    }

    /// The answers as a plot document in Notes, opened. Returns whether it
    /// was made.
    fn make_plot(&mut self, walk: &Walk, now: f64) -> bool {
        let Some(guide) = walk.guide() else { return true };
        let Some(project) = self.project.as_mut() else { return false };
        let book = project.node(Role::Manuscript.folder()).and_then(|n| n.meta.target_words);
        // Characters listed where a beat asks for them: profiles, and the
        // names linked to them.
        let mut answers = walk.answers.clone();
        let mut profiles = 0;
        for ((_, step), answer) in guide.steps().into_iter().zip(answers.iter_mut()) {
            if step.fill != Fill::Characters || plot::character_lines(answer).is_empty() {
                continue;
            }
            match make_characters(project, answer) {
                Ok(made) => {
                    profiles += made;
                    *answer = plot::link_characters(answer);
                }
                Err(e) => self.error = Some(format!("Couldn't make the character profiles: {e}")),
            }
        }
        let text = plot::document(guide, &answers, &walk.headings, book);
        let title = match walk.title.trim() {
            "" => format!("Plot: {}", guide.name),
            title => title.to_owned(),
        };
        let made = project.add_document(&Place::Into(Role::Notes.folder().to_owned()), &title).and_then(|id| {
            let path = project.file_path(&id).ok_or_else(|| std::io::Error::other("no file"))?;
            sloth_core::document::write_atomically(&path, text.as_bytes())?;
            Ok(id)
        });
        match made {
            Ok(id) => {
                self.manifest_dirty = true;
                self.select(id);
                let made = match profiles {
                    0 => String::new(),
                    1 => ", with a new character profile".to_owned(),
                    n => format!(", with {n} new character profiles"),
                };
                self.set_status(format!("{title} is in Notes{made}: Break Up\u{2026} in its right-click menu makes each beat a document"), now);
                true
            }
            Err(e) => {
                self.error = Some(format!("Couldn't make the plot: {e}"));
                false
            }
        }
    }

    /// The answers into the character's profile (made if it's new), opened.
    fn make_character(&mut self, walk: &Walk, now: f64) -> bool {
        let Some(guide) = walk.guide() else { return true };
        let id = match &walk.profile {
            Some(id) => id.clone(),
            None => {
                let Some(project) = self.project.as_mut() else { return false };
                let name = match walk.title.trim() {
                    "" => "Unnamed",
                    name => name,
                };
                let made = ensure_characters(project).and_then(|()| project.add_document(&Place::Into(CHARACTERS.to_owned()), name));
                match made {
                    Ok(id) => {
                        self.manifest_dirty = true;
                        id
                    }
                    Err(e) => {
                        self.error = Some(format!("Couldn't make the profile: {e}"));
                        return false;
                    }
                }
            }
        };
        let sheet = Sheet::parse(&self.document_text(&id).unwrap_or_default());
        let sheet = cast::apply(&sheet, guide, &walk.answers);
        self.apply_rewrites(vec![(id.clone(), sheet.to_text())]);
        let name = self.project.as_ref().and_then(|p| p.node(&id)).map(|n| n.title.clone()).unwrap_or_default();
        self.select(id);
        self.set_status(format!("Saved to {name}\u{2019}s profile"), now);
        true
    }

    /// The essay, as a new document where it was asked for, opened.
    fn make_essay(&mut self, walk: &Walk, now: f64) -> bool {
        let Some(guide) = walk.guide() else { return true };
        let made = essay::make(guide, &walk.answers, &walk.headings, walk.title_pick, walk.with_headings);
        let place = self.place_near(walk.near.as_deref());
        let Some(project) = self.project.as_mut() else { return false };
        let title = made.title.clone().unwrap_or_else(|| "Untitled".to_owned());
        let result = project.add_document(&place, &title).and_then(|id| {
            let path = project.file_path(&id).ok_or_else(|| std::io::Error::other("no file"))?;
            sloth_core::document::write_atomically(&path, made.text.as_bytes())?;
            let old = project.node(&id).map(|n| n.meta.clone()).unwrap_or_default();
            let meta = Meta {
                synopsis: made.synopsis.clone(),
                keywords: made.keywords.clone(),
                target_words: made.target.or(old.target_words),
                ..old
            };
            project.set_meta(&id, meta);
            Ok(id)
        });
        match result {
            Ok(id) => {
                self.manifest_dirty = true;
                self.select(id);
                self.set_status(format!("{title}: drafted from {}", guide.name), now);
                true
            }
            Err(e) => {
                self.error = Some(format!("Couldn't make the essay: {e}"));
                false
            }
        }
    }
}

/// The Characters folder, back if the project had put it away.
fn ensure_characters(project: &mut Project) -> std::io::Result<()> {
    if project.profile_kind(CHARACTERS).is_none() {
        let preset = profile_presets().into_iter().map(|(k, _)| k).find(|k| k.id == CHARACTERS);
        project.add_profile_folder(preset.ok_or_else(|| std::io::Error::other("no Characters preset"))?)?;
    }
    Ok(())
}

/// A character profile for each character an answer lists that doesn't
/// have one, the rest of the line its description. Returns how many were
/// made.
fn make_characters(project: &mut Project, answer: &str) -> std::io::Result<usize> {
    let mut made = 0;
    for (name, story) in plot::character_lines(answer) {
        let taken = project.profile_items(CHARACTERS).iter().any(|n| n.title.trim().eq_ignore_ascii_case(&name));
        if taken {
            continue;
        }
        ensure_characters(project)?;
        let id = project.add_document(&Place::Into(CHARACTERS.to_owned()), &name)?;
        if !story.is_empty() {
            let path = project.file_path(&id).ok_or_else(|| std::io::Error::other("no file"))?;
            sloth_core::document::write_atomically(&path, format!("{story}\n").as_bytes())?;
        }
        made += 1;
    }
    Ok(made)
}

/// The first page: which guide (and the title, the character's name, or
/// whether an essay has headings).
fn choose(ui: &mut egui::Ui, walk: &mut Walk, profile_name: Option<&str>) -> Do {
    ui.label(walk.builder.intro());
    ui.add_space(6.0);
    let guides = walk.builder.guides();
    let mut begin = false;
    egui::ScrollArea::vertical().max_height(400.0).show(ui, |ui| {
        for group in groups(guides) {
            ui.add_space(4.0);
            ui.label(RichText::new(group).small().weak());
            for (i, g) in guides.iter().enumerate().filter(|(_, g)| g.group == group) {
                begin |= guide_row(ui, walk, i, g);
            }
        }
    });
    ui.add_space(6.0);
    let mut ready = true;
    match walk.builder {
        Builder::Plot => {
            ui.horizontal(|ui| {
                ui.label("Title");
                let hint = format!("Plot: {}", guides[walk.picked].name);
                ui.add(egui::TextEdit::singleline(&mut walk.title).hint_text(hint).desired_width(f32::INFINITY));
            });
        }
        Builder::Character => match profile_name {
            Some(name) => {
                ui.label(format!("For {name}"));
            }
            None => {
                ui.horizontal(|ui| {
                    ui.label("Name");
                    ui.add(egui::TextEdit::singleline(&mut walk.title).hint_text("Who is it?").desired_width(f32::INFINITY));
                });
                ready = !walk.title.trim().is_empty();
            }
        },
        Builder::Essay => {
            ui.checkbox(&mut walk.with_headings, "Section headings").on_hover_text("A ## heading over each part, for a longer piece, or to Break Up later");
        }
    }
    ui.add_space(6.0);
    let mut todo = Do::Nothing;
    ui.horizontal(|ui| {
        let command_enter = ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter));
        let button = ui.add_enabled(ready, egui::Button::new("Begin"));
        if ready && (button.clicked() || command_enter || begin) {
            todo = Do::Begin;
        }
        if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            todo = Do::Close;
        }
    });
    todo
}

/// A guide in the list: click to pick it. Returns whether it was
/// double-clicked (to begin).
fn guide_row(ui: &mut egui::Ui, walk: &mut Walk, i: usize, g: &'static Guide) -> bool {
    let selected = walk.picked == i;
    let frame = egui::Frame::group(ui.style())
        .fill(if selected { ui.visuals().selection.bg_fill.gamma_multiply(0.35) } else { egui::Color32::TRANSPARENT })
        .stroke(if selected { ui.visuals().selection.stroke } else { ui.visuals().widgets.noninteractive.bg_stroke });
    let count = walk.builder.steps(g).len();
    let unit = walk.builder.unit();
    let response = frame
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new(g.name).strong());
                ui.weak(format!("{count} {unit}s"));
            });
            ui.label(RichText::new(format!("{}. For {}.", capitalise(g.from), g.suits.to_lowercase())).small().weak());
            ui.label(g.about);
        })
        .response
        .interact(egui::Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    if response.clicked() {
        walk.picked = i;
    }
    ui.add_space(2.0);
    response.double_clicked()
}

/// A step's page.
fn step_page(ui: &mut egui::Ui, walk: &mut Walk, guide: &'static Guide) -> Do {
    let steps = walk.steps();
    let last = steps.len() - 1;
    walk.step = walk.step.min(last);
    let (part, step) = steps[walk.step];

    // Where you are: a dot per step, the answered ones filled; click one to go there.
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for (i, (_, each)) in steps.iter().enumerate() {
            let answered = !walk.answers[i].trim().is_empty();
            let (rect, response) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::click());
            let color = if i == walk.step { ui.visuals().selection.stroke.color } else { ui.visuals().weak_text_color() };
            let radius = if i == walk.step { 5.0 } else { 4.0 };
            if answered {
                ui.painter().circle_filled(rect.center(), radius, color);
            } else {
                ui.painter().circle_stroke(rect.center(), radius - 0.5, egui::Stroke::new(1.2, color));
            }
            let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
            if response.on_hover_text(each.name).clicked() {
                walk.step = i;
                walk.focus = true;
            }
        }
    });
    let place = match part {
        Some(part) => format!("{} \u{00b7} {part} \u{00b7} {} of {}", guide.name, walk.step + 1, steps.len()),
        None => format!("{} \u{00b7} {} of {}", guide.name, walk.step + 1, steps.len()),
    };
    ui.weak(place);
    ui.add_space(4.0);
    ui.label(RichText::new(step.name).size(20.0).strong());
    ui.label(step.about);
    ui.add_space(2.0);
    ui.label(RichText::new(step.question).italics());
    ui.add_space(6.0);
    let writes_text = matches!(step.fill, Fill::Text | Fill::Characters);
    if writes_text && walk.shows_headings() {
        ui.horizontal(|ui| {
            ui.label("Heading").on_hover_text("What this is called in the document, and its title if you Break Up. Empty, it's the name above");
            ui.add(egui::TextEdit::singleline(&mut walk.headings[walk.step]).hint_text(step.name).desired_width(f32::INFINITY));
        });
        ui.add_space(4.0);
    }
    let answer = &mut walk.answers[walk.step];
    let field = match step.fill {
        Fill::Target => ui.add(egui::TextEdit::singleline(answer).hint_text("1,200").desired_width(120.0)),
        Fill::Tags => ui.add(egui::TextEdit::singleline(answer).hint_text("ferries, time, travel").desired_width(f32::INFINITY)),
        fill => {
            let (hint, rows) = match fill {
                Fill::Characters => ("Mara: wants to cross the river, though her father forbids it", 6),
                Fill::Title => ("One to a line", 4),
                Fill::Synopsis => ("One sentence", 2),
                _ => ("Your answer: a sentence or a page. Leave it empty to come back to.", 6),
            };
            ui.add(egui::TextEdit::multiline(answer).hint_text(hint).desired_rows(rows).desired_width(f32::INFINITY))
        }
    };
    if walk.focus {
        field.request_focus();
        walk.focus = false;
    }
    match step.fill {
        Fill::Characters => {
            ui.weak("Each becomes a character profile (unless there's one already), linked from the plot.");
        }
        Fill::Title => {
            let titles = essay::titles(&walk.answers[walk.step]);
            if titles.len() > 1 {
                walk.title_pick = walk.title_pick.min(titles.len() - 1);
                for (i, title) in titles.iter().enumerate() {
                    ui.radio_value(&mut walk.title_pick, i, title.as_str());
                }
            }
        }
        _ => {}
    }
    ui.add_space(6.0);

    let (finish, early) = match walk.builder {
        Builder::Plot => ("Make the Document", "Make It Now"),
        Builder::Character => ("Save to Profile", "Save Now"),
        Builder::Essay => ("Make the Essay", "Make It Now"),
    };
    let mut todo = Do::Nothing;
    let command_enter = ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter));
    ui.horizontal(|ui| {
        if walk.step < last {
            if ui.button("Next").on_hover_text(sloth_core::keys::shown("\u{2318}\u{21a9}")).clicked() || command_enter {
                walk.step += 1;
                walk.focus = true;
            }
        } else if ui.button(finish).on_hover_text(sloth_core::keys::shown("\u{2318}\u{21a9}")).clicked() || command_enter {
            todo = Do::Make;
        }
        if ui.add_enabled(walk.step > 0, egui::Button::new("Back")).clicked() {
            walk.step -= 1;
            walk.focus = true;
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if walk.step < last && ui.button(early).on_hover_text("With what you've answered so far; the rest can wait").clicked() {
                todo = Do::Make;
            }
            let hint = "Back to the list. Your answers stay with this one unless you begin another";
            if ui.button("Change").on_hover_text(hint).clicked() {
                walk.guide = None;
            }
        });
    });
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        todo = Do::Close;
    }
    todo
}

fn capitalise(s: &str) -> String {
    let mut chars = s.chars();
    chars.next().map_or_else(String::new, |c| c.to_uppercase().chain(chars).collect())
}
