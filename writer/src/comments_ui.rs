//! Comments on passages, in the margin. In the text a comment is
//! CriticMarkup: `{==the passage==}{>>the comment<<}` (or `{>>just a
//! comment<<}` at a point), so it stays with its words through edits,
//! splits and syncing, and compile leaves it out. The editor tints the
//! passage and keeps the marks out of sight except in the paragraph you're
//! writing; here each comment gets a card in a column beside the text,
//! level with its passage: the passage quoted (click to go to it), the
//! comment to read and write, and Resolve, which takes the comment away
//! and leaves the words.

use eframe::egui::{self, Color32, Pos2, Rect, RichText, Sense, Stroke, UiBuilder, Vec2};
use sloth_core::editor;
use sloth_core::markdown::{self, Comment};

use super::WriterApp;

/// The comments column's width, and the gap before it.
pub(super) const COLUMN: f32 = 230.0;
pub(super) const GAP: f32 = 16.0;
/// The comments' colour (as the editor draws them).
const AMBER: Color32 = Color32::from_rgb(0xc9, 0x8a, 0x22);
/// Longest a passage is quoted on its card, in characters.
const QUOTE: usize = 70;

/// What was done on a card.
enum CardAction {
    Edit(usize, String),
    Resolve(usize),
    Show(usize),
}

impl WriterApp {
    /// ⌥⌘K: a comment on the selected words (or at the cursor) in the open
    /// document, its card ready to write in.
    pub(super) fn add_comment(&mut self, ctx: &egui::Context) {
        let Some(tab) = self.current() else { return };
        let id = tab.id;
        editor::add_comment(ctx, id, &mut tab.doc.text);
    }

    /// The cards, in `column`, each level with its passage (`tops`, as the
    /// editor placed them), pushed down where they'd overlap.
    pub(super) fn comment_cards(&mut self, ui: &mut egui::Ui, column: Rect, tops: &[f32]) {
        let ctx = ui.ctx().clone();
        let Some(tab) = self.current() else { return };
        let (tab_id, text) = (tab.id, tab.doc.text.clone());
        // A comment just added: its card takes the keyboard.
        if let Some(at) = editor::take_added_comment(&ctx, tab_id) {
            self.comment_focus = Some(at);
        }
        let list = markdown::comments(&text);
        // Typed into this frame: line up again next frame.
        if list.len() != tops.len() {
            ctx.request_repaint();
            return;
        }
        let mut action = None;
        let mut below = column.top();
        let clip = column.intersect(ui.clip_rect());
        for (i, c) in list.iter().enumerate() {
            let y = tops[i].max(below);
            if y > column.bottom() {
                break;
            }
            let mut card = ui.new_child(
                UiBuilder::new()
                    .max_rect(Rect::from_min_size(Pos2::new(column.left(), y), Vec2::new(COLUMN, 2000.0)))
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            card.set_clip_rect(clip);
            let focus = self.comment_focus == Some(c.words.start);
            if focus {
                self.comment_focus = None;
            }
            let shown = comment_card(&mut card, &text, c, i, focus, &mut action);
            below = shown.bottom() + 8.0;
        }
        let Some(action) = action else { return };
        let Some(tab) = self.current() else { return };
        match action {
            CardAction::Edit(i, words) => {
                let c = &list[i];
                // The comment's words can't hold its closing mark.
                tab.doc.text.replace_range(c.words.clone(), &words.replace("<<}", "<< }"));
            }
            CardAction::Resolve(i) => {
                let c = &list[i];
                let keep = c.passage.as_ref().map_or("", |p| &text[p.clone()]);
                tab.doc.text.replace_range(c.whole.clone(), keep);
            }
            CardAction::Show(i) => {
                let p = list[i].passage.clone().unwrap_or(list[i].whole.clone());
                let (start, end) = (text[..p.start].chars().count(), text[..p.end].chars().count());
                editor::set_selection(&ctx, tab_id, start..end);
                self.reveal = Some(start);
                self.focus_editor = true;
            }
        }
    }
}

/// One card. Returns where it was drawn.
fn comment_card(ui: &mut egui::Ui, text: &str, c: &Comment, i: usize, focus: bool, action: &mut Option<CardAction>) -> Rect {
    let visuals = ui.visuals().clone();
    let fill = visuals.extreme_bg_color.lerp_to_gamma(AMBER, if visuals.dark_mode { 0.08 } else { 0.06 });
    egui::Frame::new()
        .fill(fill)
        .stroke(Stroke::new(1.0, AMBER.gamma_multiply(0.45)))
        .corner_radius(6)
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(COLUMN - 20.0);
            if let Some(p) = &c.passage {
                let quoted: String = text[p.clone()].chars().take(QUOTE).collect();
                let more = if text[p.clone()].chars().count() > QUOTE { "\u{2026}" } else { "" };
                let quote = egui::Label::new(RichText::new(format!("\u{201c}{quoted}{more}\u{201d}")).small().italics().color(AMBER)).sense(Sense::click());
                if ui.add(quote).on_hover_text("Show it in the text").clicked() {
                    *action = Some(CardAction::Show(i));
                }
            }
            let mut words = text[c.words.clone()].to_owned();
            let field = ui.add(
                egui::TextEdit::multiline(&mut words)
                    .id(egui::Id::unique(("comment-card", i)))
                    .hint_text("Comment\u{2026}")
                    .frame(egui::Frame::NONE)
                    .desired_rows(1)
                    .desired_width(f32::INFINITY),
            );
            if focus {
                field.request_focus();
            }
            if field.changed() {
                *action = Some(CardAction::Edit(i, words));
            }
            ui.horizontal(|ui| {
                let resolve = egui::Button::new(RichText::new("Resolve").small()).frame(false);
                if ui.add(resolve).on_hover_text("Take the comment away (the words stay)").clicked() {
                    *action = Some(CardAction::Resolve(i));
                }
                if c.passage.is_none() {
                    let go = egui::Button::new(RichText::new("Show").small()).frame(false);
                    if ui.add(go).on_hover_text("Show where it is in the text").clicked() {
                        *action = Some(CardAction::Show(i));
                    }
                }
            });
        })
        .response
        .rect
}
