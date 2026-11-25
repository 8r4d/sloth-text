//! The start screen's card file: recent projects as cards on a rotary file,
//! A to Z, one in front with the next few standing behind it, their tabs
//! showing. Flip with the arrow keys or the scroll wheel, type a letter to
//! jump to it, and press Return (or click the front card) to open it.
//! Clicking a card behind brings it to the front.

use std::path::PathBuf;

use eframe::egui::{self, Color32, FontId, Pos2, Rect, RichText, Stroke, Vec2, pos2, vec2};
use sloth_core::motion;

use super::WriterApp;
use super::projects_ui::{TileChoice, ago, facts_of, kind_color, project_menu, short_place, title_of};
use crate::project::Kind;
use crate::stats;

/// Which card is in front, and scrolling not yet turned into a flip.
#[derive(Default)]
pub(super) struct CardFileState {
    front: usize,
    scrolled: f32,
}

/// One project's card.
struct Card {
    root: PathBuf,
    title: String,
    kind: Kind,
    available: bool,
}

const CARD: Vec2 = vec2(400.0, 184.0);
/// How far each card behind stands above the one in front of it.
const STEP: f32 = 18.0;
/// Cards shown behind the front one.
const BEHIND: usize = 3;
const TAB: Vec2 = vec2(30.0, 20.0);
/// Scrolling this far flips one card.
const SCROLL_PER_CARD: f32 = 40.0;

/// The letter on a card's tab.
fn letter(title: &str) -> char {
    title.chars().find(|c| c.is_alphanumeric()).map_or('#', |c| c.to_uppercase().next().unwrap_or(c))
}

/// Where along the top edge a card's tab sits: by its letter, A at the
/// left and Z at the right, as dividers in a card file are staggered.
fn tab_x(card: Rect, title: &str) -> f32 {
    let c = letter(title);
    let at = if c.is_ascii_alphabetic() { (c as u8 - b'A') as f32 / 25.0 } else { 0.0 };
    card.left() + 18.0 + (card.width() - 36.0 - TAB.x) * at
}

impl WriterApp {
    /// The card file. Returns what was chosen for which project.
    pub(super) fn card_file(&mut self, ui: &mut egui::Ui) -> Option<(PathBuf, TileChoice)> {
        let mut cards: Vec<Card> = self
            .recent
            .iter()
            .map(|root| {
                let found = title_of(root);
                let available = found.is_some();
                let (title, kind) = found.unwrap_or_else(|| (root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), Kind::default()));
                Card { root: root.clone(), title, kind, available }
            })
            .collect();
        cards.sort_by_key(|c| c.title.to_lowercase());
        let count = cards.len();
        let state = &mut self.card_file;
        state.front = state.front.min(count - 1);

        // Keys, unless something's being typed in (App Settings, say).
        let mut open_front = false;
        let typing = ui.memory(|m| m.focused().is_some());
        ui.input(|i| {
            if typing {
                return;
            }
            for event in &i.events {
                match event {
                    egui::Event::Key { key, pressed: true, modifiers, .. } if modifiers.is_none() => match key {
                        egui::Key::ArrowRight | egui::Key::ArrowDown => state.front = (state.front + 1).min(count - 1),
                        egui::Key::ArrowLeft | egui::Key::ArrowUp => state.front = state.front.saturating_sub(1),
                        egui::Key::Home => state.front = 0,
                        egui::Key::End => state.front = count - 1,
                        egui::Key::Enter => open_front = true,
                        _ => {}
                    },
                    // A letter: the first card at or after it.
                    egui::Event::Text(text) => {
                        if let Some(typed) = text.chars().next().filter(|c| c.is_alphanumeric()) {
                            let typed = typed.to_uppercase().next().unwrap_or(typed);
                            state.front = cards.iter().position(|c| letter(&c.title) >= typed).unwrap_or(count - 1);
                        }
                    }
                    _ => {}
                }
            }
        });

        // The space: the stack, with room above for the cards behind and
        // their tabs, and the rotary's knobs either side.
        let height = CARD.y + STEP * BEHIND as f32 + TAB.y + 24.0;
        let (area, response) = ui.allocate_exact_size(vec2(ui.available_width(), height), egui::Sense::hover());
        // Anywhere over the card file, the cards on it included (which take
        // the hover themselves).
        if response.contains_pointer() {
            // Taken, so the page itself doesn't scroll too.
            state.scrolled += ui.input_mut(|i| {
                let d = i.smooth_scroll_delta;
                i.smooth_scroll_delta = Vec2::ZERO;
                d.y + d.x
            });
            while state.scrolled <= -SCROLL_PER_CARD {
                state.scrolled += SCROLL_PER_CARD;
                state.front = (state.front + 1).min(count - 1);
            }
            while state.scrolled >= SCROLL_PER_CARD {
                state.scrolled -= SCROLL_PER_CARD;
                state.front = state.front.saturating_sub(1);
            }
        }
        let front = state.front;
        let shown = if motion::animate() {
            ui.ctx().animate_value_with_time(response.id.with("flip"), front as f32, 0.2)
        } else {
            front as f32
        };

        let visuals = ui.visuals().clone();
        let painter = ui.painter_at(area);
        let front_rect = Rect::from_center_size(pos2(area.center().x, area.bottom() - 12.0 - CARD.y / 2.0), CARD);
        // A card `offset` places back (0 in front, fractions while flipping):
        // higher and a little narrower.
        let place = |offset: f32| {
            let o = offset.max(0.0);
            Rect::from_center_size(front_rect.center() - vec2(0.0, o * STEP), vec2(CARD.x - o * 14.0, CARD.y))
        };

        // The rotary's knobs, their ridges turning as the cards go by.
        let ridge = visuals.weak_text_color().gamma_multiply(0.5);
        for side in [-1.0, 1.0] {
            let x = front_rect.center().x + side * (CARD.x / 2.0 + 26.0);
            let spacing = 7.0;
            let turn = shown.fract() * spacing;
            for k in 0..11 {
                let y = front_rect.center().y - 40.0 + k as f32 * spacing + turn;
                // Shorter towards the ends, as on a round knob.
                let w = 18.0 * (1.0 - ((y - front_rect.center().y + 5.0) / 45.0).powi(2)).max(0.2).sqrt();
                painter.line_segment([pos2(x - w / 2.0, y), pos2(x + w / 2.0, y)], Stroke::new(2.0, ridge));
            }
        }
        // The base the cards stand in.
        let base = Rect::from_min_max(pos2(front_rect.left() - 10.0, front_rect.bottom() - 18.0), pos2(front_rect.right() + 10.0, front_rect.bottom() + 10.0));

        // Back to front: the cards behind, then the one in front, then the
        // one leaving (flipping forward and down) over them.
        let first = (shown.floor() as isize - 1).max(0) as usize;
        let last = ((shown.ceil() as usize) + BEHIND).min(count - 1);
        let mut order: Vec<usize> = (first..=last).collect();
        order.sort_by(|a, b| (*b as f32 - shown).partial_cmp(&(*a as f32 - shown)).unwrap_or(std::cmp::Ordering::Equal));
        let mut clicked_behind = None;
        let mut choice = None;
        for i in order {
            let card = &cards[i];
            let offset = i as f32 - shown;
            if offset > BEHIND as f32 + 0.5 || offset <= -1.0 {
                continue;
            }
            // Leaving: drops forward and fades.
            let (rect, fade) = if offset < 0.0 { (place(0.0).translate(vec2(0.0, -offset * 60.0)), 1.0 + offset) } else { (place(offset), 1.0) };
            let back = offset.max(0.0);
            let fill = visuals.extreme_bg_color.lerp_to_gamma(visuals.panel_fill, (back / BEHIND as f32).min(1.0) * 0.6).gamma_multiply(fade);
            let line = visuals.widgets.noninteractive.bg_stroke.color.gamma_multiply(fade * (1.0 - back * 0.15));
            let accent = kind_color(card.kind).gamma_multiply(fade * if card.available { 1.0 } else { 0.4 });

            // Its tab, with the letter.
            let tab = Rect::from_min_size(pos2(tab_x(rect, &card.title), rect.top() - TAB.y + 1.0), TAB);
            painter.rect(tab, egui::CornerRadius { nw: 5, ne: 5, sw: 0, se: 0 }, fill, Stroke::new(1.0, line), egui::StrokeKind::Inside);
            painter.line_segment([tab.left_top() + vec2(5.0, 2.5), tab.right_top() + vec2(-5.0, 2.5)], Stroke::new(2.0, accent));
            painter.text(tab.center() + vec2(0.0, 1.5), egui::Align2::CENTER_CENTER, letter(&card.title), FontId::proportional(12.0), visuals.strong_text_color().gamma_multiply(fade));
            // The card.
            painter.rect(rect, 6.0, fill, Stroke::new(1.0, line), egui::StrokeKind::Inside);
            // The rail it hangs from: two slots, just above the base.
            for x in [rect.center().x - 50.0, rect.center().x + 50.0] {
                let slot = Rect::from_center_size(pos2(x, rect.bottom() - 30.0), vec2(5.0, 12.0));
                painter.rect_stroke(slot, 2.5, Stroke::new(1.0, line), egui::StrokeKind::Inside);
            }

            if offset.abs() < 0.5 {
                let facts = self.tile_facts.entry(card.root.clone()).or_insert_with(|| facts_of(&card.root)).clone();
                choice = front_card(ui, &painter, rect, response.id.with(("front", i)), card, facts.as_ref(), fade).map(|c| (card.root.clone(), c));
            } else if offset > 0.0 && i <= front + BEHIND {
                // The strip above the card in front of it can be clicked.
                let strip = Rect::from_min_max(tab.left_top(), pos2(rect.right(), rect.top() + STEP));
                let strip = strip.union(Rect::from_min_max(rect.left_top(), pos2(rect.right(), rect.top() + STEP)));
                if ui.interact(strip, response.id.with(("card", i)), egui::Sense::click()).on_hover_text(&card.title).clicked() {
                    clicked_behind = Some(i);
                }
            }
        }
        painter.rect(base, 6.0, visuals.panel_fill, Stroke::new(1.0, visuals.widgets.noninteractive.bg_stroke.color), egui::StrokeKind::Inside);

        if let Some(i) = clicked_behind {
            self.card_file.front = i;
        }
        ui.add_space(6.0);
        ui.label(
            RichText::new(format!("{} of {count}  \u{b7}  \u{2190} \u{2192} or scroll to flip, a letter to jump, Return to open", front + 1))
                .small()
                .weak(),
        );
        if open_front && cards[front].available {
            return Some((cards[front].root.clone(), TileChoice::Open));
        }
        choice
    }
}

/// What's on the front card: icon, title, kind and words, when it was last
/// written in, where it is. Click to open; right-click for its menu.
fn front_card(ui: &mut egui::Ui, painter: &egui::Painter, rect: Rect, id: egui::Id, card: &Card, facts: Option<&super::projects_ui::TileFacts>, fade: f32) -> Option<TileChoice> {
    let visuals = ui.visuals();
    let strong = visuals.strong_text_color().gamma_multiply(fade * if card.available { 1.0 } else { 0.5 });
    let weak = visuals.weak_text_color().gamma_multiply(fade);
    let color = kind_color(card.kind).gamma_multiply(fade);
    let inner = rect.shrink2(vec2(22.0, 16.0));

    let sense = if card.available { egui::Sense::click() } else { egui::Sense::hover() };
    let response = ui.interact(rect, id, sense);
    let place = if card.available { short_place(&card.root) } else { "(not found)".to_owned() };
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, card.available, format!("{}, {}, {place}", card.title, card.kind.name())));
    let response = response.on_hover_text(card.root.display().to_string());
    if response.hovered() && card.available {
        painter.rect_stroke(rect, 6.0, Stroke::new(1.5, color.gamma_multiply(0.8)), egui::StrokeKind::Inside);
    }

    let fit = |text: &str, size: f32, c: Color32| {
        let mut job = egui::text::LayoutJob::simple_singleline(text.to_owned(), FontId::proportional(size), c);
        job.wrap = egui::text::TextWrapping::truncate_at_width(inner.width());
        ui.fonts_mut(|f| f.layout_job(job))
    };
    painter.text(inner.left_top(), egui::Align2::LEFT_TOP, card.kind.icon(), FontId::proportional(22.0), color);
    let title = fit(&card.title, 28.0, strong);
    let title_pos = Pos2::new(inner.center().x - title.size().x / 2.0, inner.top() + 26.0);
    painter.galley(title_pos, title, strong);

    let mut lines = vec![match facts {
        Some(f) if f.words > 0 => format!("{} \u{b7} {} words", card.kind.name(), stats::thousands(f.words)),
        _ => card.kind.name().to_owned(),
    }];
    if let Some(f) = facts {
        let mut when = f.last.map(|d| format!("Last written {}", ago(d, stats::today()))).unwrap_or_default();
        if f.streak >= 2 {
            when.push_str(&format!(" \u{b7} {} days in a row", f.streak));
        }
        if !when.is_empty() {
            lines.push(when);
        }
    }
    lines.push(place);
    let mut y = inner.top() + 70.0;
    for text in lines {
        let galley = fit(&text, 12.5, weak);
        let x = inner.center().x - galley.size().x / 2.0;
        let h = galley.size().y;
        painter.galley(pos2(x, y), galley, weak);
        y += h + 3.0;
    }
    // Towards its target: a thin bar under the lines.
    if let Some((words, target)) = facts.and_then(|f| f.target.map(|t| (f.words, t))) {
        let bar = Rect::from_min_size(pos2(inner.left() + 60.0, y + 4.0), vec2(inner.width() - 120.0, 2.0));
        painter.rect_filled(bar, 1.0, color.gamma_multiply(0.2));
        let done = (words as f32 / target as f32).clamp(0.0, 1.0);
        painter.rect_filled(Rect::from_min_size(bar.min, vec2(bar.width() * done, bar.height())), 1.0, color);
    }

    let mut choice = None;
    if response.clicked() {
        choice = Some(TileChoice::Open);
    }
    response.context_menu(|ui| project_menu(ui, card.available, card.kind, &mut choice));
    choice
}
