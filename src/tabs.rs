//! Open documents, shown as tabs.

use std::path::Path;

use eframe::egui::{self, Color32, CornerRadius, Frame, Margin, RichText, Sense};

use crate::document::Document;

/// Stable identity for a tab (indices shift as tabs close; ids don't).
pub type TabId = u64;

pub struct Tab {
    pub id: TabId,
    pub doc: Document,
    /// The user chose "Don't Save" for this tab while quitting.
    pub discard_on_quit: bool,
}

pub struct Tabs {
    tabs: Vec<Tab>,
    active: usize,
    next_id: TabId,
}

impl Tabs {
    /// Starts with one empty, untitled tab.
    pub fn new() -> Self {
        let mut tabs = Self {
            tabs: Vec::new(),
            active: 0,
            next_id: 0,
        };
        tabs.add(Document::new());
        tabs
    }

    pub fn active(&self) -> &Tab {
        &self.tabs[self.active]
    }

    pub fn active_mut(&mut self) -> &mut Tab {
        &mut self.tabs[self.active]
    }

    pub fn iter(&self) -> impl Iterator<Item = &Tab> {
        self.tabs.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Tab> {
        self.tabs.iter_mut()
    }

    pub fn get_mut(&mut self, id: TabId) -> Option<&mut Tab> {
        self.tabs.iter_mut().find(|t| t.id == id)
    }

    pub fn get(&self, id: TabId) -> Option<&Tab> {
        self.tabs.iter().find(|t| t.id == id)
    }

    /// Add a new tab after the active one and switch to it.
    pub fn add(&mut self, doc: Document) -> TabId {
        let id = self.next_id;
        self.next_id += 1;
        let tab = Tab {
            id,
            doc,
            discard_on_quit: false,
        };
        let index = if self.tabs.is_empty() { 0 } else { self.active + 1 };
        self.tabs.insert(index, tab);
        self.active = index;
        id
    }

    /// Open a document, reusing the active tab if it's an untouched blank one.
    pub fn open(&mut self, doc: Document) -> TabId {
        let active = self.active();
        if active.doc.path.is_none() && active.doc.text.is_empty() && !active.doc.is_dirty() {
            let id = active.id;
            self.active_mut().doc = doc;
            id
        } else {
            self.add(doc)
        }
    }

    pub fn find_by_path(&self, path: &Path) -> Option<TabId> {
        self.tabs
            .iter()
            .find(|t| t.doc.path.as_deref() == Some(path))
            .map(|t| t.id)
    }

    pub fn activate(&mut self, id: TabId) {
        if let Some(i) = self.tabs.iter().position(|t| t.id == id) {
            self.active = i;
        }
    }

    /// Close a tab without asking. There is always at least one tab.
    pub fn close(&mut self, id: TabId) {
        let Some(i) = self.tabs.iter().position(|t| t.id == id) else {
            return;
        };
        self.tabs.remove(i);
        if self.tabs.is_empty() {
            self.add(Document::new());
        } else if self.active > i || self.active == self.tabs.len() {
            self.active = self.active.saturating_sub(1);
        }
    }

    pub fn select_next(&mut self) {
        self.active = (self.active + 1) % self.tabs.len();
    }

    pub fn select_previous(&mut self) {
        self.active = (self.active + self.tabs.len() - 1) % self.tabs.len();
    }

    /// A tab with unsaved changes the user hasn't already chosen to discard.
    pub fn first_unsaved(&self) -> Option<TabId> {
        self.tabs
            .iter()
            .find(|t| t.doc.is_dirty() && !t.discard_on_quit)
            .map(|t| t.id)
    }
}

pub enum TabBarAction {
    Activate(TabId),
    Close(TabId),
}

pub fn tab_bar(ui: &mut egui::Ui, tabs: &Tabs) -> Option<TabBarAction> {
    let mut action = None;
    let active_id = tabs.active().id;
    let active_fill = ui.visuals().extreme_bg_color;

    egui::ScrollArea::horizontal()
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for tab in tabs.iter() {
                    let is_active = tab.id == active_id;
                    let frame = Frame::NONE
                        .fill(if is_active { active_fill } else { Color32::TRANSPARENT })
                        .inner_margin(Margin::symmetric(10, 5))
                        .corner_radius(CornerRadius { nw: 6, ne: 6, sw: 0, se: 0 });

                    let inner = frame.show(ui, |ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        let mut name = tab.doc.display_name();
                        if tab.doc.is_dirty() {
                            name = format!("• {name}");
                        }
                        let text = if is_active {
                            RichText::new(name).strong()
                        } else {
                            RichText::new(name)
                        };
                        let label = ui.add(egui::Label::new(text).sense(Sense::click()).selectable(false));
                        let close = ui
                            .add(egui::Button::new(RichText::new("×").weak()).frame(false).small())
                            .on_hover_text("Close Tab (⌘W)");
                        (label, close)
                    });
                    let (label, close) = inner.inner;

                    let label = match &tab.doc.path {
                        Some(path) => label.on_hover_text(path.display().to_string()),
                        None => label,
                    };
                    if close.clicked() || label.middle_clicked() {
                        action = Some(TabBarAction::Close(tab.id));
                    } else if label.clicked() {
                        action = Some(TabBarAction::Activate(tab.id));
                    }
                }
            });
        });

    action
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(name: &str) -> Document {
        let mut d = Document::new();
        d.path = Some(name.into());
        d
    }

    #[test]
    fn open_reuses_blank_tab_then_adds() {
        let mut tabs = Tabs::new();
        let first = tabs.active().id;
        assert_eq!(tabs.open(named("a.txt")), first);
        let second = tabs.open(named("b.txt"));
        assert_ne!(second, first);
        assert_eq!(tabs.active().id, second);
        assert_eq!(tabs.find_by_path(Path::new("a.txt")), Some(first));
    }

    #[test]
    fn closing_keeps_a_sensible_active_tab() {
        let mut tabs = Tabs::new();
        tabs.open(named("a"));
        let b = tabs.add(named("b"));
        let c = tabs.add(named("c"));
        tabs.close(c);
        assert_eq!(tabs.active().id, b);
        tabs.close(b);
        tabs.close(tabs.active().id);
        // Closing the last tab leaves a fresh blank one.
        assert_eq!(tabs.iter().count(), 1);
        assert!(tabs.active().doc.path.is_none());
    }

    #[test]
    fn next_and_previous_wrap() {
        let mut tabs = Tabs::new();
        let a = tabs.active().id;
        let b = tabs.add(Document::new());
        tabs.select_next();
        assert_eq!(tabs.active().id, a);
        tabs.select_previous();
        assert_eq!(tabs.active().id, b);
    }
}
