//! Style in the window (see style.rs): what's worth a look marked in the
//! open document (⌥⌘Y), each explained when the pointer's over it, and the
//! Style window: the figures, the sentences' rhythm, and which kinds of
//! thing to look for.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::Range;

use std::thread::JoinHandle;
use std::time::Duration;

use eframe::egui::{self, Color32, RichText, Sense, Stroke, Vec2};
use sloth_core::editor::{CursorInfo, Fix, Mark};

use super::WriterApp;
use crate::grammar::{self, Checker, Found};
use crate::style::{self, Category, Finding, Report};

/// Wording (clichés, wordy, redundant, hedges): underlined in this.
const WORDING: Color32 = Color32::from_rgb(0x7b, 0x6c, 0xd9);
/// Structure (openings, passive, adverbs, tags).
const STRUCTURE: Color32 = Color32::from_rgb(0x2f, 0x9e, 0x8f);
/// Tense slips.
const TENSE: Color32 = Color32::from_rgb(0xd0, 0x4a, 0x5c);
/// Grammar (Harper).
const GRAMMAR: Color32 = Color32::from_rgb(0x4a, 0xa8, 0x4e);
/// Time each frame may spend checking paragraphs' grammar; the rest wait
/// for the next frame.
const GRAMMAR_BUDGET: Duration = Duration::from_millis(8);
/// Seconds the Style window's figures are kept before they're worked out again.
const REFRESH: f64 = 1.0;

fn underline(c: Category) -> Color32 {
    match c {
        Category::Cliche | Category::Wordy | Category::Redundant | Category::Hedge => WORDING,
        Category::Tense => TENSE,
        Category::Grammar => GRAMMAR,
        _ => STRUCTURE,
    }
}

/// The open document's findings, as last worked out: for which text and
/// categories.
#[derive(Default)]
pub(super) struct StyleCache {
    key: u64,
    findings: Vec<Finding>,
    /// The findings marked (style's and grammar's), for the tooltips.
    shown: Vec<Finding>,
    /// Grammar's suggestions, for the editor's right-click menu.
    fixes: Vec<Fix>,
    grammar: GrammarState,
    /// Grammar's findings in the open document: for which text (and
    /// settings), and whether every paragraph's been checked.
    grammar_found: (u64, Vec<Found>, bool),
    /// The Style window's: for what's selected, when.
    window: Option<StyleWindow>,
}

/// Harper: not needed yet, loading its dictionary, or ready.
#[derive(Default)]
pub(super) enum GrammarState {
    #[default]
    Off,
    Loading { dialect: String, handle: JoinHandle<Checker> },
    Ready(Box<Checker>),
}

struct StyleWindow {
    id: String,
    at: f64,
    report: Report,
    /// Every category's findings, on or off, for their counts.
    counts: Vec<(Category, usize)>,
}

impl WriterApp {
    /// The categories switched on.
    pub(super) fn style_categories(&self) -> Vec<Category> {
        Category::ALL.into_iter().filter(|c| !self.settings.style_off.iter().any(|o| o == c.id())).collect()
    }

    pub(super) fn toggle_style(&mut self) {
        self.settings.show_style = !self.settings.show_style;
    }

    /// Grammar's findings in the open document, once Harper's ready (it
    /// loads on a thread the first time); `None` until then, or with
    /// grammar switched off. Paragraphs not checked yet are checked over
    /// the next frames.
    pub(super) fn grammar_found(&mut self, ctx: &egui::Context) -> Option<Vec<Found>> {
        if !self.style_categories().contains(&Category::Grammar) {
            return None;
        }
        let dialect = self.settings.grammar_dialect.clone();
        let ignored = self.settings.grammar_ignored.clone();
        let state = std::mem::take(&mut self.style.grammar);
        self.style.grammar = match state {
            GrammarState::Ready(mut c) if c.dialect() == dialect => {
                c.set_ignored(&ignored);
                GrammarState::Ready(c)
            }
            GrammarState::Loading { dialect: d, handle } if d == dialect => {
                if handle.is_finished() {
                    match handle.join() {
                        Ok(c) => GrammarState::Ready(Box::new(c)),
                        Err(_) => GrammarState::Off,
                    }
                } else {
                    ctx.request_repaint_after(Duration::from_millis(100));
                    GrammarState::Loading { dialect: d, handle }
                }
            }
            _ => {
                let (d, i) = (dialect.clone(), ignored.clone());
                let handle = std::thread::spawn(move || Checker::new(&d, &i));
                ctx.request_repaint_after(Duration::from_millis(100));
                GrammarState::Loading { dialect: dialect.clone(), handle }
            }
        };
        let text = self.docs_current_text();
        let GrammarState::Ready(checker) = &mut self.style.grammar else { return None };
        let mut hasher = DefaultHasher::new();
        (&text, &dialect, &ignored).hash(&mut hasher);
        let key = hasher.finish();
        if self.style.grammar_found.0 == key && self.style.grammar_found.2 {
            return Some(self.style.grammar_found.1.clone());
        }
        let (found, done) = checker.check(&text, GRAMMAR_BUDGET);
        if !done {
            ctx.request_repaint();
        }
        self.style.grammar_found = (key, found.clone(), done);
        Some(found)
    }

    /// The open document's text (empty if none).
    fn docs_current_text(&self) -> String {
        let id = self.open_id();
        id.and_then(|id| self.open.get(&id).and_then(|&t| self.docs.get(t))).map(|t| t.doc.text.clone()).unwrap_or_default()
    }

    /// The selected item, if it's open as a document.
    fn open_id(&self) -> Option<String> {
        self.selected.clone().filter(|id| self.open.contains_key(id))
    }

    /// Grammar's suggestions, for the editor's right-click menu (while
    /// style is shown).
    pub(super) fn style_fixes(&self) -> Vec<Fix> {
        if self.settings.show_style { self.style.fixes.clone() } else { Vec::new() }
    }

    /// Ignore Suggestions Like This: that rule, from now on.
    pub(super) fn ignore_grammar_rule(&mut self, rule: String) {
        if !self.settings.grammar_ignored.contains(&rule) {
            self.settings.grammar_ignored.push(rule);
        }
    }

    /// What's worth a look in the open document, for the categories on.
    fn style_findings(&mut self) -> &[Finding] {
        let on = self.style_categories();
        let text = self.current().map(|t| t.doc.text.clone()).unwrap_or_default();
        let mut hasher = DefaultHasher::new();
        (&text, &on).hash(&mut hasher);
        let key = hasher.finish();
        if self.style.key != key {
            self.style.findings = style::check(&text, &on);
            self.style.key = key;
        }
        &self.style.findings
    }

    /// `marks` (repeats and glossary terms) with the style findings added:
    /// lines under words where nothing's marked yet, and long sentences
    /// shaded around (and under) what's there. Marks mustn't overlap.
    pub(super) fn with_style_marks(&mut self, mut marks: Vec<Mark>, ctx: &egui::Context, visuals: &egui::Visuals) -> Vec<Mark> {
        if !self.settings.show_style {
            self.style.shown.clear();
            self.style.fixes.clear();
            return marks;
        }
        let mut findings = self.style_findings().to_vec();
        let grammar = self.grammar_found(ctx).unwrap_or_default();
        self.style.fixes = grammar.iter().map(|f| Fix { range: f.range.clone(), message: f.message.clone(), options: f.options.clone(), key: f.rule.clone() }).collect();
        findings.extend(grammar.into_iter().map(|f| Finding { range: f.range, category: Category::Grammar, note: f.message }));
        // Grammar first where they overlap: it can be fixed in a click.
        findings.sort_by_key(|f| (f.range.start, f.category != Category::Grammar));
        self.style.shown = findings.clone();
        let overlaps = |a: &Range<usize>, b: &Range<usize>| a.start < b.end && b.start < a.end;
        // A line under it, and a light wash of the same colour behind, to
        // spot them while scanning down a page.
        let wash = if visuals.dark_mode { 0.26 } else { 0.18 };
        for f in findings.iter().filter(|f| f.category != Category::Long) {
            if !marks.iter().any(|m| overlaps(&m.range, &f.range)) {
                let color = underline(f.category);
                marks.push(Mark { range: f.range.clone(), background: color.gamma_multiply(wash), underline: color });
            }
        }
        let tint = visuals.warn_fg_color.gamma_multiply(if visuals.dark_mode { 0.16 } else { 0.12 });
        for long in findings.iter().filter(|f| f.category == Category::Long).map(|f| f.range.clone()) {
            let mut inside: Vec<Range<usize>> = Vec::new();
            for m in marks.iter_mut().filter(|m| overlaps(&m.range, &long)) {
                if m.background == Color32::TRANSPARENT && m.range.start >= long.start && m.range.end <= long.end {
                    m.background = tint;
                }
                inside.push(m.range.clone());
            }
            inside.sort_by_key(|r| r.start);
            let mut at = long.start;
            for r in inside {
                if r.start > at {
                    marks.push(Mark { range: at..r.start, background: tint, underline: Color32::TRANSPARENT });
                }
                at = at.max(r.end);
            }
            if at < long.end {
                marks.push(Mark { range: at..long.end, background: tint, underline: Color32::TRANSPARENT });
            }
        }
        marks.sort_by_key(|m| m.range.start);
        marks
    }

    /// The finding under the pointer, explained.
    pub(super) fn style_tooltip(&self, ui: &egui::Ui, info: &CursorInfo) {
        if !self.settings.show_style {
            return;
        }
        let Some(at) = info.hovered_byte else { return };
        // The word's own before its long sentence's.
        let Some(f) = self.style.shown.iter().filter(|f| f.range.contains(&at)).min_by_key(|f| f.range.len()) else { return };
        let fix = self.style.fixes.iter().find(|x| x.range == f.range && f.category == Category::Grammar);
        egui::Tooltip::always_open(ui.ctx().clone(), ui.layer_id(), egui::Id::unique("style-tooltip"), egui::PopupAnchor::Pointer).show(|ui| {
            ui.set_max_width(320.0);
            ui.label(RichText::new(f.category.name()).small().strong().color(underline(f.category)));
            ui.label(&f.note);
            if let Some(fix) = fix.filter(|x| !x.options.is_empty()) {
                let choices: Vec<String> = fix.options.iter().take(3).map(|o| if o.is_empty() { "remove it".to_owned() } else { format!("\u{201c}{o}\u{201d}") }).collect();
                ui.weak(format!("Right-click to change it: {}", choices.join(", ")));
            }
        });
    }

    /// The Style window, for what's selected (a document, or a folder's
    /// documents together).
    pub(super) fn style_window(&mut self, ctx: &egui::Context) {
        if !self.show_style_window {
            return;
        }
        let now = ctx.input(|i| i.time);
        let selected = self.selected.clone();
        if let Some(id) = &selected {
            let fresh = self.style.window.as_ref().is_some_and(|w| w.id == *id && now - w.at < REFRESH);
            if !fresh && let Some(text) = self.story_text(id) {
                let all = style::check(&text, &Category::ALL);
                let counts = Category::ALL.into_iter().map(|c| (c, all.iter().filter(|f| f.category == c).count())).collect();
                self.style.window = Some(StyleWindow { id: id.clone(), at: now, report: style::report(&text), counts });
            }
        }
        let shown = self.style.window.as_ref().filter(|w| Some(&w.id) == selected.as_ref());
        let report = shown.map(|w| w.report.clone());
        let mut counts: Vec<(Category, Option<usize>)> = shown.map(|w| w.counts.iter().map(|(c, n)| (*c, Some(*n))).collect()).unwrap_or_default();
        // Grammar's count: for the open document, once Harper's ready.
        let is_open = selected.is_some() && selected == self.open_id();
        let grammar = if is_open { self.grammar_found(ctx).map(|f| f.len()) } else { None };
        for (c, n) in &mut counts {
            if *c == Category::Grammar {
                *n = grammar;
            }
        }
        let grammar_loading = matches!(self.style.grammar, GrammarState::Loading { .. });
        let mut dialect = self.settings.grammar_dialect.clone();
        let mut bring_back = false;
        let ignored = self.settings.grammar_ignored.len();
        // Sentences can be jumped to in the open document only.
        let jump_ok = selected.as_ref().is_some_and(|id| self.project.as_ref().and_then(|p| p.node(id)).is_some_and(|n| n.is_document()));
        let title = selected.as_deref().and_then(|id| self.project.as_ref()?.node(id)).map(|n| n.title.clone());
        let mut off = self.settings.style_off.clone();
        let mut show = self.settings.show_style;
        let mut jump = None;
        let mut open = true;
        egui::Window::new("Style")
            .id(egui::Id::unique("style-window"))
            .open(&mut open)
            .default_width(420.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.checkbox(&mut show, sloth_core::keys::shown("Mark them in the text (\u{2325}\u{2318}Y)"));
                if let Some(title) = &title {
                    ui.weak(title);
                }
                ui.add_space(4.0);
                let Some(report) = &report else {
                    ui.weak("Select a document, or a folder, to see its style.");
                    return;
                };
                if report.sentences.is_empty() {
                    ui.weak("No sentences yet.");
                    return;
                }
                egui::Grid::new("style-figures").num_columns(2).spacing([16.0, 4.0]).show(ui, |ui| {
                    ui.label("Sentences");
                    ui.label(format!("{}, {:.0} words on average, the longest {}", report.sentences.len(), report.average(), report.longest()));
                    ui.end_row();
                    if let Some(grade) = report.grade {
                        ui.label("Reading level");
                        ui.label(format!("Grade {grade:.0}")).on_hover_text(
                            "Flesch\u{2013}Kincaid: the US school grade that could read it easily, from words per sentence and syllables per word. Most popular fiction is 4 to 8; essays and journalism 8 to 12. Lower isn't better, only easier.",
                        );
                        ui.end_row();
                    }
                    if let Some(tense) = report.tense() {
                        ui.label("Tense");
                        ui.label(tense);
                        ui.end_row();
                    }
                });
                ui.add_space(8.0);
                ui.label(RichText::new("Sentence lengths, in order").small().weak());
                if let Some(i) = rhythm(ui, report, jump_ok) {
                    jump = Some(report.sentences[i].0.clone());
                }
                ui.add_space(8.0);
                ui.label(RichText::new("Look for").small().weak());
                for (category, count) in &counts {
                    let mut on = !off.iter().any(|o| o == category.id());
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut on, category.name()).on_hover_text(category.hint());
                        let (rect, _) = ui.allocate_exact_size(Vec2::new(14.0, 3.0), Sense::hover());
                        ui.painter().rect_filled(rect, 1.0, if *category == Category::Long { ui.visuals().warn_fg_color } else { underline(*category) });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let shown = match count {
                                Some(n) => n.to_string(),
                                None if *category == Category::Grammar && on && grammar_loading => "loading\u{2026}".to_owned(),
                                None => "\u{2013}".to_owned(),
                            };
                            let n = RichText::new(shown);
                            let r = ui.label(if on { n } else { n.weak() });
                            if *category == Category::Grammar && count.is_none() && !grammar_loading {
                                r.on_hover_text("Counted for the open document");
                            }
                        });
                    });
                    if *category == Category::Grammar && on {
                        ui.indent("grammar-options", |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("English").small().weak());
                                egui::ComboBox::from_id_salt("grammar-dialect").selected_text(dialect.clone()).show_ui(ui, |ui| {
                                    for d in grammar::DIALECTS {
                                        ui.selectable_value(&mut dialect, d.to_owned(), d);
                                    }
                                });
                                if ignored > 0 {
                                    ui.label(RichText::new(format!("{ignored} kind{} ignored", if ignored == 1 { "" } else { "s" })).small().weak());
                                    bring_back = ui.small_button("Bring Back").on_hover_text("Check for every kind again").clicked();
                                }
                            });
                        });
                    }
                    off.retain(|o| o != category.id());
                    if !on {
                        off.push(category.id().to_owned());
                    }
                }
                ui.add_space(4.0);
                ui.add(egui::Label::new(RichText::new("Found by word lists and rules, on this computer. Hints, not mistakes: hover a marked word to see why.").small().weak()).wrap());
            });
        self.settings.style_off = off;
        self.settings.show_style = show;
        self.settings.grammar_dialect = dialect;
        if bring_back {
            self.settings.grammar_ignored.clear();
        }
        if !open {
            self.show_style_window = false;
        }
        if let Some(range) = jump
            && let Some(tab) = self.current()
        {
            let text = &tab.doc.text;
            if range.end <= text.len() {
                let (start, end) = (text[..range.start].chars().count(), text[..range.end].chars().count());
                sloth_core::editor::set_selection(ctx, tab.id, start..end);
                self.reveal = Some(start);
                self.focus_editor = true;
            }
        }
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(REFRESH));
    }
}

/// The sentences' lengths as bars, in order, with the long line marked.
/// Returns the sentence clicked, if any.
fn rhythm(ui: &mut egui::Ui, report: &Report, clickable: bool) -> Option<usize> {
    let n = report.sentences.len();
    let height = 72.0;
    let width = ui.available_width();
    let bar = (width / n as f32).clamp(3.0, 12.0);
    let top = report.longest().max(style::LONG + 8) as f32;
    let mut clicked = None;
    egui::ScrollArea::horizontal().id_salt("style-rhythm").max_height(height + 16.0).show(ui, |ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::new((bar * n as f32).max(width), height), Sense::hover());
        let painter = ui.painter_at(rect.expand(1.0));
        let visuals = ui.visuals().clone();
        let y = |words: f32| rect.bottom() - words / top * height;
        // The long line: dashed, labelled.
        let line_y = y(style::LONG as f32);
        let faint = visuals.weak_text_color().gamma_multiply(0.6);
        painter.add(egui::Shape::dashed_line(&[egui::pos2(rect.left(), line_y), egui::pos2(rect.right(), line_y)], Stroke::new(1.0, faint), 4.0, 3.0));
        painter.text(egui::pos2(rect.right() - 2.0, line_y - 2.0), egui::Align2::RIGHT_BOTTOM, format!("{} words", style::LONG), egui::FontId::proportional(10.0), visuals.weak_text_color());
        painter.hline(rect.x_range(), rect.bottom(), Stroke::new(1.0, faint));
        for (i, (_, words)) in report.sentences.iter().enumerate() {
            let x = rect.left() + i as f32 * bar;
            let cell = egui::Rect::from_min_max(egui::pos2(x, rect.top()), egui::pos2(x + bar, rect.bottom()));
            let response = ui.interact(cell, ui.scope_id().with(("rhythm", i)), if clickable { Sense::click() } else { Sense::hover() });
            let long = *words > style::LONG;
            let mut color = if long { visuals.warn_fg_color } else { visuals.hyperlink_color.gamma_multiply(0.8) };
            if response.hovered() {
                color = visuals.strong_text_color();
            }
            // Thin, with a gap between, rounded at the top.
            let fill = egui::Rect::from_min_max(egui::pos2(x + 0.5, y(*words as f32)), egui::pos2(x + bar - 1.0, rect.bottom()));
            painter.rect_filled(fill, egui::CornerRadius { nw: 2, ne: 2, sw: 0, se: 0 }, color);
            let response = response.on_hover_text(format!(
                "Sentence {}: {} words{}{}",
                i + 1,
                words,
                if long { " (long)" } else { "" },
                if clickable { "\nClick to go to it" } else { "" }
            ));
            if response.clicked() {
                clicked = Some(i);
            }
        }
    });
    clicked
}
