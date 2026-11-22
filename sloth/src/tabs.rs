//! Open documents, shown as tabs.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use eframe::egui::{self, Color32, CornerRadius, Frame, Id, Margin, RichText, Sense};

use crate::document::Document;
use crate::syntax::HighlightCache;

/// Stable identity for a tab (indices shift as tabs close; ids don't).
/// Unique across all windows, since editor state is keyed by it.
pub type TabId = u64;

fn next_tab_id() -> TabId {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

pub struct Tab {
    pub id: TabId,
    pub doc: Document,
    /// The user chose "Don't Save" for this tab while quitting.
    pub discard_on_quit: bool,
    /// Shown above the editor when another program changed or removed the file.
    pub disk_notice: Option<DiskNotice>,
    pub highlight_cache: HighlightCache,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiskNotice {
    /// Changed on disk while this tab has unsaved edits.
    Changed,
    /// Deleted or moved.
    Missing,
}

pub struct Tabs {
    tabs: Vec<Tab>,
    active: usize,
}

impl Tabs {
    /// Starts with one empty, untitled tab.
    pub fn new() -> Self {
        let mut tabs = Self {
            tabs: Vec::new(),
            active: 0,
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

    /// A tab list holding just this tab (moved from another window).
    pub fn with_tab(tab: Tab) -> Self {
        Self { tabs: vec![tab], active: 0 }
    }

    /// Take a tab out (to move it to another window). There is always at
    /// least one tab left.
    pub fn take(&mut self, id: TabId) -> Option<Tab> {
        let i = self.index_of(id)?;
        let tab = self.tabs.remove(i);
        if self.tabs.is_empty() {
            self.add(Document::new());
        } else if self.active > i || self.active == self.tabs.len() {
            self.active = self.active.saturating_sub(1);
        }
        Some(tab)
    }

    /// True if this is just the blank tab every window starts with.
    pub fn is_pristine(&self) -> bool {
        self.tabs.len() == 1 && {
            let doc = &self.tabs[0].doc;
            doc.path.is_none() && doc.text.is_empty() && !doc.is_dirty()
        }
    }

    pub fn get_mut(&mut self, id: TabId) -> Option<&mut Tab> {
        self.tabs.iter_mut().find(|t| t.id == id)
    }

    pub fn get(&self, id: TabId) -> Option<&Tab> {
        self.tabs.iter().find(|t| t.id == id)
    }

    /// Add a new tab after the active one and switch to it.
    pub fn add(&mut self, doc: Document) -> TabId {
        let id = next_tab_id();
        let tab = Tab {
            id,
            doc,
            discard_on_quit: false,
            disk_notice: None,
            highlight_cache: HighlightCache::default(),
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
            let tab = self.active_mut();
            tab.doc = doc;
            tab.disk_notice = None;
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

    /// Move the tab at `from` to position `to`, keeping the same tab active.
    pub fn move_tab(&mut self, from: usize, to: usize) {
        if from == to || from >= self.tabs.len() || to >= self.tabs.len() {
            return;
        }
        let active_id = self.active().id;
        let tab = self.tabs.remove(from);
        self.tabs.insert(to, tab);
        self.activate(active_id);
    }

    pub fn index_of(&self, id: TabId) -> Option<usize> {
        self.tabs.iter().position(|t| t.id == id)
    }

    /// Every tab except `id`.
    pub fn others(&self, id: TabId) -> Vec<TabId> {
        self.tabs.iter().map(|t| t.id).filter(|&t| t != id).collect()
    }

    /// The tabs to the right of `id`.
    pub fn to_the_right_of(&self, id: TabId) -> Vec<TabId> {
        let start = self.index_of(id).map_or(self.tabs.len(), |i| i + 1);
        self.tabs[start..].iter().map(|t| t.id).collect()
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
    CloseOthers(TabId),
    CloseToTheRight(TabId),
    /// Drag to reorder: move the tab at this index to that one.
    Move { from: usize, to: usize },
}

pub fn tab_bar(ui: &mut egui::Ui, tabs: &Tabs) -> Option<TabBarAction> {
    let mut action = None;
    let active_id = tabs.active().id;
    let active_fill = ui.visuals().extreme_bg_color;
    let count = tabs.iter().count();

    egui::ScrollArea::horizontal()
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                let mut rects = Vec::with_capacity(count);
                let mut dragging = None;

                for (index, tab) in tabs.iter().enumerate() {
                    {
                        let is_active = tab.id == active_id;
                        let frame = Frame::NONE
                            .fill(if is_active { active_fill } else { Color32::TRANSPARENT })
                            .inner_margin(Margin::symmetric(10, 5))
                            .corner_radius(CornerRadius { nw: 6, ne: 6, sw: 0, se: 0 });

                        let inner = frame.show(ui, |ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            let mut name = tab.doc.display_name();
                            if tab.doc.is_dirty() {
                                name = format!("\u{2022} {name}");
                            }
                            let text = if is_active {
                                RichText::new(name).strong()
                            } else {
                                RichText::new(name)
                            };
                            let label = ui.add(egui::Label::new(text).selectable(false));
                            // An id tied to the tab (not its position), so a
                            // drag keeps following the tab as it moves.
                            let label = ui.interact(
                                label.rect,
                                Id::unique(("sloth-tab", tab.id)),
                                Sense::click_and_drag(),
                            );
                            let close = ui
                                .add(egui::Button::new(RichText::new("\u{d7}").weak()).frame(false).small())
                                .on_hover_text("Close Tab (\u{2318}W)");
                            (label, close)
                        });
                        rects.push(inner.response.rect);
                        let (label, close) = inner.inner;

                        if label.dragged() {
                            dragging = Some(index);
                        }
                        label.context_menu(|ui| {
                            if ui.button("Close Tab").clicked() {
                                action = Some(TabBarAction::Close(tab.id));
                            }
                            if ui.add_enabled(count > 1, egui::Button::new("Close Other Tabs")).clicked() {
                                action = Some(TabBarAction::CloseOthers(tab.id));
                            }
                            let any_right = index + 1 < count;
                            if ui.add_enabled(any_right, egui::Button::new("Close Tabs to the Right")).clicked() {
                                action = Some(TabBarAction::CloseToTheRight(tab.id));
                            }
                        });
                        let label = match &tab.doc.path {
                            Some(path) => label.on_hover_text(path.display().to_string()),
                            None => label,
                        };
                        if close.clicked() || label.middle_clicked() {
                            action = Some(TabBarAction::Close(tab.id));
                        } else if label.clicked() || label.drag_started() {
                            action = Some(TabBarAction::Activate(tab.id));
                        }
                    }
                }

                // While dragging, move the tab to wherever the pointer is.
                if let (Some(from), Some(pointer)) = (dragging, ui.ctx().pointer_latest_pos()) {
                    // Insertion point among the tabs (past the last = the end)…
                    let insert_at = rects
                        .iter()
                        .position(|r| pointer.x < r.center().x)
                        .unwrap_or(count);
                    // …as an index once the dragged tab is taken out.
                    let to = if insert_at > from { insert_at - 1 } else { insert_at };
                    if to != from {
                        action = Some(TabBarAction::Move { from, to });
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
    fn moving_tabs_keeps_the_active_one() {
        let mut tabs = Tabs::new();
        let a = tabs.active().id;
        let b = tabs.add(Document::new());
        let c = tabs.add(Document::new());
        tabs.activate(b);
        tabs.move_tab(0, 2);
        let order: Vec<_> = tabs.iter().map(|t| t.id).collect();
        assert_eq!(order, vec![b, c, a]);
        assert_eq!(tabs.active().id, b);
        assert_eq!(tabs.to_the_right_of(b), vec![c, a]);
        assert_eq!(tabs.others(c), vec![b, a]);
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
