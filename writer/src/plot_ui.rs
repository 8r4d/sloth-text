//! Build a Plot… in the app: pick a structure, answer its beats one at a
//! time, and the answers become a document in Notes (see plot.rs). Closing
//! the window part way keeps the answers until Writer quits, so it opens
//! again where you were.

use eframe::egui::{self, RichText};

use super::WriterApp;
use crate::plot::{self, Group, STRUCTURES, Structure};
use crate::project::{CHARACTERS, Place, Project, Role, profile_presets};

/// A plot being built.
pub(super) struct PlotBuilder {
    /// Shown (closing it keeps the rest).
    pub open: bool,
    /// The structure picked (an index into `STRUCTURES`); `None` while
    /// choosing.
    pub structure: Option<usize>,
    /// Highlighted in the list while choosing.
    pub picked: usize,
    /// The beat on show.
    pub step: usize,
    pub answers: Vec<String>,
    /// Your own heading for each beat (empty: the beat's name).
    pub headings: Vec<String>,
    /// The structure the answers are for.
    answers_for: Option<usize>,
    /// The document's title.
    pub title: String,
    /// Put the cursor in the answer on the next frame.
    focus: bool,
}

impl PlotBuilder {
    fn new() -> Self {
        // Story Circle: the one asked for first.
        let picked = STRUCTURES.iter().position(|s| s.name == "Story Circle").unwrap_or(0);
        PlotBuilder { open: true, structure: None, picked, step: 0, answers: Vec::new(), headings: Vec::new(), answers_for: None, title: String::new(), focus: false }
    }

    fn structure(&self) -> Option<&'static Structure> {
        self.structure.and_then(|i| STRUCTURES.get(i))
    }
}

/// What the window asked for this frame.
enum Do {
    Nothing,
    Make,
    Close,
}

impl WriterApp {
    /// Build a Plot…: the window, as it was left if there's a plot under way.
    pub(super) fn start_plot(&mut self) {
        match &mut self.plot {
            Some(builder) => builder.open = true,
            None => self.plot = Some(PlotBuilder::new()),
        }
    }

    pub(super) fn plot_window(&mut self, ctx: &egui::Context) {
        let Some(builder) = self.plot.as_mut().filter(|b| b.open) else { return };
        let mut open = true;
        let mut todo = Do::Nothing;
        egui::Window::new("Build a Plot")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(460.0)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .show(ctx, |ui| {
                ui.set_width(460.0);
                todo = match builder.structure() {
                    None => choose(ui, builder),
                    Some(structure) => walk(ui, builder, structure),
                };
            });
        if !open || matches!(todo, Do::Close) {
            builder.open = false;
        }
        if matches!(todo, Do::Make) {
            self.make_plot(ctx.input(|i| i.time));
        }
    }

    /// The answers as a document in Notes, opened.
    fn make_plot(&mut self, now: f64) {
        let Some(builder) = self.plot.take() else { return };
        let Some(structure) = builder.structure() else { return };
        let Some(project) = self.project.as_mut() else { return };
        let book = project.node(Role::Manuscript.folder()).and_then(|n| n.meta.target_words);
        // Characters listed where a beat asks for them: profiles, and the
        // names linked to them.
        let mut answers = builder.answers.clone();
        let mut profiles = 0;
        for ((_, beat), answer) in structure.beats().into_iter().zip(answers.iter_mut()) {
            if !beat.characters || plot::character_lines(answer).is_empty() {
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
        let text = structure.document(&answers, &builder.headings, book);
        let title = match builder.title.trim() {
            "" => format!("Plot: {}", structure.name),
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
            }
            Err(e) => {
                self.error = Some(format!("Couldn't make the plot: {e}"));
                // Keep the answers, to try again.
                self.plot = Some(PlotBuilder { open: false, ..builder });
            }
        }
    }
}

/// A character profile for each character an answer lists that doesn't
/// have one, the rest of the line its description. The Characters folder
/// comes back if the project had put it away. Returns how many were made.
fn make_characters(project: &mut Project, answer: &str) -> std::io::Result<usize> {
    let mut made = 0;
    for (name, story) in plot::character_lines(answer) {
        let taken = project.profile_items(CHARACTERS).iter().any(|n| n.title.trim().eq_ignore_ascii_case(&name));
        if taken {
            continue;
        }
        if project.profile_kind(CHARACTERS).is_none() {
            let preset = profile_presets().into_iter().map(|(k, _)| k).find(|k| k.id == CHARACTERS);
            project.add_profile_folder(preset.ok_or_else(|| std::io::Error::other("no Characters preset"))?)?;
        }
        let id = project.add_document(&Place::Into(CHARACTERS.to_owned()), &name)?;
        if !story.is_empty() {
            let path = project.file_path(&id).ok_or_else(|| std::io::Error::other("no file"))?;
            sloth_core::document::write_atomically(&path, format!("{story}\n").as_bytes())?;
        }
        made += 1;
    }
    Ok(made)
}

/// The first page: which structure.
fn choose(ui: &mut egui::Ui, builder: &mut PlotBuilder) -> Do {
    ui.label("Walk through a story structure a beat at a time. Your answers become a document in Notes, a heading for each beat, to write on from.");
    ui.add_space(6.0);
    egui::ScrollArea::vertical().max_height(400.0).show(ui, |ui| {
        for group in Group::ALL {
            ui.add_space(4.0);
            ui.label(RichText::new(group.name()).small().weak());
            for (i, s) in STRUCTURES.iter().enumerate().filter(|(_, s)| s.group == group) {
                structure_row(ui, builder, i, s);
            }
        }
    });
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label("Title");
        let hint = format!("Plot: {}", STRUCTURES[builder.picked].name);
        ui.add(egui::TextEdit::singleline(&mut builder.title).hint_text(hint).desired_width(f32::INFINITY));
    });
    ui.add_space(6.0);
    let mut todo = Do::Nothing;
    ui.horizontal(|ui| {
        if ui.button("Begin").clicked() || ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter)) {
            begin(builder);
        }
        if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            todo = Do::Close;
        }
    });
    todo
}

/// A structure in the list: click to pick it, double-click to begin.
fn structure_row(ui: &mut egui::Ui, builder: &mut PlotBuilder, i: usize, s: &Structure) {
    let selected = builder.picked == i;
    let frame = egui::Frame::group(ui.style())
        .fill(if selected { ui.visuals().selection.bg_fill.gamma_multiply(0.35) } else { egui::Color32::TRANSPARENT })
        .stroke(if selected { ui.visuals().selection.stroke } else { ui.visuals().widgets.noninteractive.bg_stroke });
    let response = frame
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new(s.name).strong());
                ui.weak(format!("{} beats", s.beat_count()));
            });
            ui.label(RichText::new(format!("{}. For {}.", capitalise(s.from), s.suits.to_lowercase())).small().weak());
            ui.label(s.about);
        })
        .response
        .interact(egui::Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    if response.clicked() {
        builder.picked = i;
    }
    if response.double_clicked() {
        begin(builder);
    }
    ui.add_space(2.0);
}

/// Into the beats of the structure picked. Answers are kept if it's the
/// one they were written for.
fn begin(builder: &mut PlotBuilder) {
    let structure = &STRUCTURES[builder.picked];
    if builder.answers_for != Some(builder.picked) {
        builder.answers = vec![String::new(); structure.beat_count()];
        builder.headings = vec![String::new(); structure.beat_count()];
        builder.answers_for = Some(builder.picked);
        builder.step = 0;
    }
    builder.structure = Some(builder.picked);
    builder.focus = true;
}

/// A beat's page.
fn walk(ui: &mut egui::Ui, builder: &mut PlotBuilder, structure: &'static Structure) -> Do {
    let beats = structure.beats();
    let last = beats.len() - 1;
    builder.step = builder.step.min(last);
    let (act, beat) = beats[builder.step];

    // Where you are: a dot per beat, the answered ones filled; click one to go there.
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for (i, (_, each)) in beats.iter().enumerate() {
            let answered = !builder.answers[i].trim().is_empty();
            let (rect, response) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::click());
            let color = if i == builder.step { ui.visuals().selection.stroke.color } else { ui.visuals().weak_text_color() };
            let radius = if i == builder.step { 5.0 } else { 4.0 };
            if answered {
                ui.painter().circle_filled(rect.center(), radius, color);
            } else {
                ui.painter().circle_stroke(rect.center(), radius - 0.5, egui::Stroke::new(1.2, color));
            }
            let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
            if response.on_hover_text(each.name).clicked() {
                builder.step = i;
                builder.focus = true;
            }
        }
    });
    let place = match act {
        Some(act) => format!("{} \u{00b7} {act} \u{00b7} {} of {}", structure.name, builder.step + 1, beats.len()),
        None => format!("{} \u{00b7} {} of {}", structure.name, builder.step + 1, beats.len()),
    };
    ui.weak(place);
    ui.add_space(4.0);
    ui.label(RichText::new(beat.name).size(20.0).strong());
    ui.label(beat.about);
    ui.add_space(2.0);
    ui.label(RichText::new(beat.question).italics());
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label("Heading").on_hover_text("What this beat is called in the document, and the document's title if you Break Up. Empty, it's the beat's name");
        ui.add(egui::TextEdit::singleline(&mut builder.headings[builder.step]).hint_text(beat.name).desired_width(f32::INFINITY));
    });
    ui.add_space(4.0);
    let hint = if beat.characters {
        "Mara: wants to cross the river, though her father forbids it"
    } else {
        "Your answer: a sentence or a page. Leave it empty to come back to."
    };
    let field = ui.add(
        egui::TextEdit::multiline(&mut builder.answers[builder.step])
            .hint_text(hint)
            .desired_rows(6)
            .desired_width(f32::INFINITY),
    );
    if builder.focus {
        field.request_focus();
        builder.focus = false;
    }
    if beat.characters {
        ui.weak("Each becomes a character profile (unless there's one already), linked from the plot.");
    }
    ui.add_space(6.0);

    let mut todo = Do::Nothing;
    let command_enter = ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter));
    ui.horizontal(|ui| {
        if builder.step < last {
            if ui.button("Next").on_hover_text(sloth_core::keys::shown("\u{2318}\u{21a9}")).clicked() || command_enter {
                builder.step += 1;
                builder.focus = true;
            }
        } else if ui.button("Make the Document").on_hover_text(sloth_core::keys::shown("\u{2318}\u{21a9}")).clicked() || command_enter {
            todo = Do::Make;
        }
        if ui.add_enabled(builder.step > 0, egui::Button::new("Back")).clicked() {
            builder.step -= 1;
            builder.focus = true;
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if builder.step < last && ui.button("Make It Now").on_hover_text("The document with what you've answered so far; the rest are headings to fill in").clicked() {
                todo = Do::Make;
            }
            if ui.button("Change Structure").on_hover_text("Back to the list. Your answers stay with this one unless you begin another").clicked() {
                builder.structure = None;
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
