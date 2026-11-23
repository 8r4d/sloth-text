//! UI tests: run Writer's real UI in memory with `egui_kittest` (no window)
//! against a project in a scratch folder.

use std::fs;
use std::path::PathBuf;

use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use super::{Direction, WriterApp};
use crate::binder::BinderAction;
use crate::project::{Place, Project};

/// A scratch folder for one test's project, removed when dropped.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("sloth-writer-ui-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A project whose first document (`manuscript/untitled.md`) holds `text`.
fn project(dir: &TempDir, text: &str) -> Project {
    let project = Project::create(&dir.0, "Book").unwrap();
    fs::write(dir.0.join("manuscript/untitled.md"), text).unwrap();
    project
}

fn harness(project: Project) -> Harness<'static, WriterApp> {
    let mut harness = Harness::builder()
        .with_size([1000.0, 700.0])
        // Real frame times (the default is a quarter second a step, which
        // is too slow for a double-click to count as one).
        .with_step_dt(1.0 / 60.0)
        // Icons like 📄 come from fonts tests don't load; draw a box instead.
        .allow_missing_glyphs()
        .build_eframe(|cc| WriterApp::new(cc, None));
    harness.state_mut().set_project(project);
    // Autosave asks for repaints, so step a fixed number of frames.
    harness.run_steps(3);
    harness
}

fn current_text(harness: &mut Harness<'static, WriterApp>) -> Option<String> {
    harness.state_mut().current().map(|t| t.doc.text.clone())
}

#[test]
fn opens_the_first_document_and_autosaves_edits() {
    let dir = TempDir::new("autosave");
    let mut harness = harness(project(&dir, "Once upon"));

    harness.get_by_label("📄 Untitled");
    assert_eq!(current_text(&mut harness).as_deref(), Some("Once upon"));

    harness.state_mut().current().unwrap().doc.text.push_str(" a time");
    // Pretend the last autosave was long ago.
    harness.state_mut().last_autosave = -100.0;
    harness.run_steps(1);
    assert_eq!(fs::read_to_string(dir.0.join("manuscript/untitled.md")).unwrap(), "Once upon a time");
}

#[test]
fn clicking_a_document_shows_it() {
    let dir = TempDir::new("click");
    let mut p = project(&dir, "one");
    p.add_document(&Place::Into("manuscript".into()), "Second").unwrap();
    fs::write(dir.0.join("manuscript/second.md"), "two").unwrap();
    let mut harness = harness(p);

    harness.get_by_label("📄 Second").click();
    harness.run_steps(3);
    assert_eq!(current_text(&mut harness).as_deref(), Some("two"));

    // Back again: the first document reopens where it was.
    harness.get_by_label("📄 Untitled").click();
    harness.run_steps(3);
    assert_eq!(current_text(&mut harness).as_deref(), Some("one"));
}

#[test]
fn a_new_document_is_named_by_typing() {
    let dir = TempDir::new("new");
    let mut harness = harness(project(&dir, ""));

    let selected = harness.state().selected.clone();
    harness.state_mut().new_item(selected, true);
    harness.run_steps(3);
    // The title field has focus with "Untitled" selected: typing replaces it.
    harness.event(egui::Event::Text("The Harbour".into()));
    harness.key_press(egui::Key::Enter);
    harness.run_steps(3);

    harness.get_by_label("📄 The Harbour");
    assert!(dir.0.join("manuscript/the-harbour.md").is_file());
    let app = harness.state();
    let titles: Vec<&str> = app.project.as_ref().unwrap().manifest.binder[0]
        .children
        .iter()
        .map(|n| n.title.as_str())
        .collect();
    assert_eq!(titles, ["Untitled", "The Harbour"]);
}

#[test]
fn renaming_an_open_document_keeps_its_unsaved_words() {
    let dir = TempDir::new("rename");
    let mut harness = harness(project(&dir, "saved"));
    let id = harness.state().selected.clone().unwrap();

    harness.state_mut().current().unwrap().doc.text = "not saved yet".into();
    harness.state_mut().apply(BinderAction::FinishRename { id, title: "Chapter One".into() });
    harness.state_mut().save_all();

    assert!(!dir.0.join("manuscript/untitled.md").exists());
    assert_eq!(fs::read_to_string(dir.0.join("manuscript/chapter-one.md")).unwrap(), "not saved yet");
}

#[test]
fn trash_then_empty_closes_the_document() {
    let dir = TempDir::new("trash");
    let mut harness = harness(project(&dir, "cut me"));
    let id = harness.state().selected.clone().unwrap();

    harness.state_mut().apply(BinderAction::Trash(id.clone()));
    harness.run_steps(3);
    assert!(dir.0.join("trash/untitled.md").is_file());
    // Still selected and readable while in the Trash.
    assert_eq!(current_text(&mut harness).as_deref(), Some("cut me"));

    harness.state_mut().apply(BinderAction::EmptyTrash);
    harness.run_steps(3);
    harness.get_by_label("Empty Trash").click();
    harness.run_steps(3);

    assert!(!dir.0.join("trash/untitled.md").exists());
    let app = harness.state_mut();
    assert!(app.project.as_ref().unwrap().node(&id).is_none());
    assert_eq!(app.selected, None);
    assert!(app.current().is_none());
}

/// Drag one binder row onto another with the mouse. `fraction` is how far
/// down the target row to let go: near the top means before it, near the
/// bottom after it, the middle inside it.
fn drag(harness: &mut Harness<'static, WriterApp>, from: &str, to: &str, fraction: f32) {
    let start = harness.get_by_label(from).rect().center();
    let target = harness.get_by_label(to).rect();
    let end = egui::pos2(target.center().x, target.top() + target.height() * fraction);
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    harness.event(egui::Event::PointerMoved(start));
    harness.run_steps(1);
    harness.event(button(start, true));
    harness.run_steps(1);
    for step in 1..=6 {
        harness.event(egui::Event::PointerMoved(start.lerp(end, step as f32 / 6.0)));
        harness.run_steps(1);
    }
    harness.event(button(end, false));
    harness.run_steps(3);
}

fn manuscript_titles(harness: &Harness<'static, WriterApp>) -> Vec<String> {
    harness.state().project.as_ref().unwrap().manifest.binder[0]
        .children
        .iter()
        .map(|n| n.title.clone())
        .collect()
}

#[test]
fn dragging_reorders_and_moves_between_folders() {
    let dir = TempDir::new("drag");
    let mut p = project(&dir, "");
    p.add_document(&Place::Into("manuscript".into()), "Second").unwrap();
    let mut harness = harness(p);

    // Onto the top edge of the first row: before it.
    drag(&mut harness, "📄 Second", "📄 Untitled", 0.1);
    assert_eq!(manuscript_titles(&harness), ["Second", "Untitled"]);

    // Onto the middle of a role folder: inside it, and the file follows.
    drag(&mut harness, "📄 Untitled", "📚 Notes", 0.5);
    assert_eq!(manuscript_titles(&harness), ["Second"]);
    assert!(dir.0.join("notes/untitled.md").is_file());
    assert!(!dir.0.join("manuscript/untitled.md").exists());
    assert_eq!(harness.state().error, None);
}

#[test]
fn an_item_cant_be_dropped_inside_itself() {
    let dir = TempDir::new("drag-self");
    let mut p = project(&dir, "");
    let chapter = p.add_document(&Place::Into("manuscript".into()), "Chapter").unwrap();
    p.add_document(&Place::Into(chapter), "Scene").unwrap();
    let mut harness = harness(p);

    drag(&mut harness, "📄 Chapter", "📄 Scene", 0.5);
    assert_eq!(manuscript_titles(&harness), ["Untitled", "Chapter"]);
    // Refused quietly while dragging, rather than with an error afterwards.
    assert_eq!(harness.state().error, None);
}

// ----- changes made by other programs -----------------------------------------

/// Run one frame in which the outside-change check is due.
fn check_disk(harness: &mut Harness<'static, WriterApp>) {
    harness.state_mut().last_disk_check = -100.0;
    harness.run_steps(1);
}

/// Edit the open document in the app, then change its file "in another
/// program" (different length, so the change shows even within the file
/// system's timestamp resolution).
fn edit_both_sides(harness: &mut Harness<'static, WriterApp>, dir: &TempDir) -> PathBuf {
    let file = dir.0.join("manuscript/untitled.md");
    harness.state_mut().current().unwrap().doc.text = "my edit".into();
    fs::write(&file, "their longer edit").unwrap();
    file
}

#[test]
fn an_outside_change_reloads_an_unedited_document() {
    let dir = TempDir::new("outside-clean");
    let mut harness = harness(project(&dir, "old"));
    fs::write(dir.0.join("manuscript/untitled.md"), "new words").unwrap();

    check_disk(&mut harness);
    assert_eq!(current_text(&mut harness).as_deref(), Some("new words"));
    let status = harness.state().status.clone().unwrap().0;
    assert!(status.starts_with("Reloaded untitled.md"), "{status}");
}

#[test]
fn autosave_never_writes_over_an_outside_change() {
    let dir = TempDir::new("outside-autosave");
    let mut harness = harness(project(&dir, "old"));
    let file = edit_both_sides(&mut harness, &dir);

    // An autosave is due but the periodic check isn't: the save itself
    // must notice the change.
    harness.state_mut().last_disk_check = 1e9;
    harness.state_mut().last_autosave = -100.0;
    harness.run_steps(2);
    assert_eq!(fs::read_to_string(&file).unwrap(), "their longer edit");

    // The person chooses theirs: it's saved straight away.
    harness.get_by_label("Keep My Version").click();
    harness.run_steps(2);
    assert_eq!(fs::read_to_string(&file).unwrap(), "my edit");
    assert!(harness.query_by_label("Keep My Version").is_none());
}

#[test]
fn reload_from_disk_takes_the_other_version() {
    let dir = TempDir::new("outside-reload");
    let mut harness = harness(project(&dir, "old"));
    let file = edit_both_sides(&mut harness, &dir);

    check_disk(&mut harness);
    harness.get_by_label("Reload from Disk").click();
    harness.run_steps(2);
    assert_eq!(current_text(&mut harness).as_deref(), Some("their longer edit"));
    assert_eq!(fs::read_to_string(&file).unwrap(), "their longer edit");
}

#[test]
fn a_deleted_file_is_written_back() {
    let dir = TempDir::new("outside-deleted");
    let mut harness = harness(project(&dir, "keep these words"));
    let file = dir.0.join("manuscript/untitled.md");
    fs::remove_file(&file).unwrap();

    check_disk(&mut harness);
    assert_eq!(fs::read_to_string(&file).unwrap(), "keep these words");
    let status = harness.state().status.clone().unwrap().0;
    assert!(status.contains("again"), "{status}");
}

#[test]
fn quitting_waits_for_a_version_choice() {
    let dir = TempDir::new("outside-quit");
    let mut p = project(&dir, "old");
    p.add_document(&Place::Into("manuscript".into()), "Other").unwrap();
    let mut harness = harness(p);
    let conflicted = harness.state().selected.clone();
    edit_both_sides(&mut harness, &dir);
    check_disk(&mut harness);

    // Looking at another document when quitting: the conflict is shown.
    harness.get_by_label("📄 Other").click();
    harness.run_steps(2);
    assert!(!harness.state_mut().before_close());
    assert_eq!(harness.state().selected, conflicted);
    assert!(harness.state().error.is_some());

    harness.run_steps(1);
    harness.get_by_label("Keep My Version").click();
    harness.run_steps(2);
    assert!(harness.state_mut().before_close());
}

#[test]
fn typing_goes_straight_into_a_newly_opened_document() {
    let dir = TempDir::new("focus");
    let mut p = project(&dir, "");
    p.add_document(&Place::Into("manuscript".into()), "Second").unwrap();
    let mut harness = harness(p);

    harness.get_by_label("📄 Second").click();
    harness.run_steps(2);
    harness.event(egui::Event::Text("Hello".into()));
    harness.run_steps(2);
    assert_eq!(current_text(&mut harness).as_deref(), Some("Hello"));
}

// ----- find --------------------------------------------------------------------

/// Type into whatever has focus (the find field, once it's open).
fn type_text(harness: &mut Harness<'static, WriterApp>, text: &str) {
    harness.run_steps(2);
    harness.event(egui::Event::Text(text.into()));
    harness.run_steps(3);
}

#[test]
fn find_steps_through_the_open_document() {
    let dir = TempDir::new("find");
    let mut harness = harness(project(&dir, "the cat and the hat"));

    harness.state_mut().open_find(false);
    type_text(&mut harness, "the");
    assert_eq!(harness.state().find.current, Some(0..3));
    harness.get_by_label("1 of 2");

    let ctx = harness.ctx.clone();
    harness.state_mut().find_step(&ctx, Direction::Next);
    harness.run_steps(2);
    assert_eq!(harness.state().find.current, Some(12..15));
    harness.get_by_label("2 of 2");

    // Next again wraps round to the first.
    harness.state_mut().find_step(&ctx, Direction::Next);
    assert_eq!(harness.state().find.current, Some(0..3));
}

#[test]
fn project_search_lists_every_document_and_jumps_to_a_match() {
    let dir = TempDir::new("find-project");
    let mut p = project(&dir, "a whale");
    let second = p.add_document(&Place::Into("manuscript".into()), "Second").unwrap();
    fs::write(dir.0.join("manuscript/second.md"), "notes\nthe whale returns").unwrap();
    let cut = p.add_document(&Place::Into("manuscript".into()), "Cut").unwrap();
    fs::write(dir.0.join("manuscript/cut.md"), "a whale in the bin").unwrap();
    p.trash(&cut).unwrap();
    let mut harness = harness(p);

    harness.state_mut().open_find(true);
    type_text(&mut harness, "whale");
    // The trashed document isn't searched.
    harness.get_by_label("2 matches in 2 documents");
    harness.get_by_label("Untitled  (1)");
    harness.get_by_label("Second  (1)");

    // Line 2 of Second, as the results panel shows it.
    harness.get_by_label("     2  the whale returns").click();
    harness.run_steps(3);
    assert_eq!(harness.state().selected.as_deref(), Some(second.as_str()));
    assert_eq!(harness.state().find.current, Some(10..15));
    // The match is highlighted as the current one, with the cursor just
    // after it (egui collapses a selection to its end when it redraws).
    let tab = harness.state_mut().current().unwrap().id;
    assert_eq!(sloth_core::editor::selection(&harness.ctx, tab).map(|r| r.end), Some(15));
}

#[test]
fn project_search_sees_unsaved_edits() {
    let dir = TempDir::new("find-unsaved");
    let mut harness = harness(project(&dir, "nothing here"));
    harness.state_mut().current().unwrap().doc.text = "a fresh idea".into();

    harness.state_mut().open_find(true);
    type_text(&mut harness, "fresh");
    harness.get_by_label("1 match in 1 document");
}

#[test]
fn escape_closes_find() {
    let dir = TempDir::new("find-escape");
    let mut harness = harness(project(&dir, "text"));
    harness.state_mut().open_find(true);
    type_text(&mut harness, "te");
    harness.key_press(egui::Key::Escape);
    harness.run_steps(2);
    assert!(!harness.state().find.visible);
    assert!(!harness.state().show_results);
}

// ----- typing --------------------------------------------------------------------

#[test]
fn two_dashes_become_an_em_dash() {
    let dir = TempDir::new("dash");
    let mut harness = harness(project(&dir, ""));
    harness.run_steps(2); // the editor takes focus

    // One keystroke per frame, as when typing.
    for t in ["Wait", "-", "-", "what"] {
        harness.event(egui::Event::Text(t.into()));
        harness.run_steps(2);
    }
    assert_eq!(current_text(&mut harness).as_deref(), Some("Wait\u{2014}what"));

    // Both dashes arriving in one frame works too.
    harness.event(egui::Event::Text("-".into()));
    harness.event(egui::Event::Text("-".into()));
    harness.run_steps(2);
    assert_eq!(current_text(&mut harness).as_deref(), Some("Wait\u{2014}what\u{2014}"));

    // Fast typing: every key in a single frame.
    for c in " more--ish".chars() {
        harness.event(egui::Event::Text(c.to_string()));
    }
    harness.run_steps(2);
    assert_eq!(current_text(&mut harness).as_deref(), Some("Wait\u{2014}what\u{2014} more\u{2014}ish"));

    // An HTML comment's opener is left alone.
    for t in [" <!", "-", "-"] {
        harness.event(egui::Event::Text(t.into()));
        harness.run_steps(2);
    }
    assert_eq!(current_text(&mut harness).as_deref(), Some("Wait\u{2014}what\u{2014} more\u{2014}ish <!--"));
}

#[test]
fn the_right_click_menu_and_shortcuts_format_the_selection() {
    let dir = TempDir::new("format");
    let mut harness = harness(project(&dir, "a word here"));
    harness.run_steps(2); // the editor takes focus
    let tab = harness.state_mut().current().unwrap().id;
    let ctx = harness.ctx.clone();
    sloth_core::editor::set_selection(&ctx, tab, 2..6);

    harness.get_by_label("Document text").click_secondary();
    harness.run_steps(2);
    harness.get_by_label("Bold \u{2318}B").click();
    harness.run_steps(3);
    assert_eq!(current_text(&mut harness).as_deref(), Some("a **word** here"));
    assert_eq!(sloth_core::editor::selection(&ctx, tab), Some(4..8), "still the word");

    // ⌘I on the same selection, ⌘B again to take the bold off.
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::I);
    harness.run_steps(2);
    assert_eq!(current_text(&mut harness).as_deref(), Some("a ***word*** here"));
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::I);
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::B);
    harness.run_steps(2);
    assert_eq!(current_text(&mut harness).as_deref(), Some("a word here"));

    harness.get_by_label("Document text").click_secondary();
    harness.run_steps(2);
    harness.get_by_label("Heading").click();
    harness.run_steps(3);
    assert_eq!(current_text(&mut harness).as_deref(), Some("# a word here"));
}

// ----- inspector ---------------------------------------------------------------

#[test]
fn inspector_edits_are_saved_with_autosave() {
    let dir = TempDir::new("inspector");
    let mut harness = harness(project(&dir, ""));

    // Status: the first "None" is the status menu, the second the label's.
    harness.get_all_by_value("None").next().unwrap().click();
    harness.run_steps(2);
    harness.get_by_label("First draft").click();
    harness.run_steps(2);
    assert!(harness.state().manifest_dirty);
    let manifest = || fs::read_to_string(dir.0.join("project.ron")).unwrap();
    assert!(!manifest().contains("status: Some"), "not written on every change");

    harness.state_mut().last_autosave = -100.0;
    harness.run_steps(1);
    assert!(manifest().contains("status: Some(2)"), "{}", manifest());
    assert!(!harness.state().manifest_dirty);
}

#[test]
fn renaming_in_the_inspector_renames_the_file() {
    let dir = TempDir::new("inspector-title");
    let mut harness = harness(project(&dir, ""));

    // The field, then the text inside it: click the field.
    harness.get_all_by_value("Untitled").next().unwrap().click();
    harness.run_steps(2);
    // Replace the title: select all, type, leave the field.
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    harness.event(egui::Event::Text("Opening".into()));
    harness.run_steps(1);
    harness.key_press(egui::Key::Enter);
    harness.run_steps(3);

    assert!(dir.0.join("manuscript/opening.md").is_file());
    harness.get_by_label("📄 Opening");
}

#[test]
fn the_manuscript_counts_every_document_in_it() {
    let dir = TempDir::new("inspector-words");
    let mut p = project(&dir, "one two three");
    p.add_document(&Place::Into("manuscript".into()), "Second").unwrap();
    fs::write(dir.0.join("manuscript/second.md"), "four five").unwrap();
    // Research doesn't count towards the manuscript.
    p.add_document(&Place::Into("research".into()), "Notes").unwrap();
    fs::write(dir.0.join("research/notes.md"), "six seven eight").unwrap();
    let mut harness = harness(p);

    harness.get_by_label("📚 Manuscript").click();
    harness.run_steps(3);
    harness.get_by_label("5 words in the manuscript");
    assert_eq!(harness.state_mut().words_under("manuscript"), 5);

    // Typing in an open document counts straight away.
    let first = harness.state().project.as_ref().unwrap().manifest.binder[0].children[0].id.clone();
    harness.state_mut().select(first);
    harness.state_mut().current().unwrap().doc.text.push_str(" and more");
    assert_eq!(harness.state_mut().words_under("manuscript"), 7);
}

#[test]
fn labels_can_be_added_and_show_in_the_binder() {
    let dir = TempDir::new("inspector-labels");
    let mut harness = harness(project(&dir, ""));
    let starter = harness.state().project.as_ref().unwrap().manifest.labels.len();

    harness.state_mut().editing_labels = true;
    harness.run_steps(2);
    harness.get_by_label("Add Label").click();
    harness.run_steps(2);
    assert_eq!(harness.state().project.as_ref().unwrap().manifest.labels.len(), starter + 1);
    assert!(harness.state().manifest_dirty);

    // Give the document a label: its binder row gains a dot.
    let id = harness.state().selected.clone().unwrap();
    let meta = crate::project::Meta { label: Some(0), ..Default::default() };
    harness.state_mut().project.as_mut().unwrap().set_meta(&id, meta);
    harness.run_steps(2);
    harness.get_by_label("📄 Untitled \u{25cf}");
}

// ----- corkboard and outline -----------------------------------------------------

/// A project with a chapter folder holding three scenes.
fn chapter_project(dir: &TempDir) -> (Project, String, Vec<String>) {
    let mut p = project(dir, "");
    let chapter = p.add_folder(&Place::Into("manuscript".into()), "Chapter").unwrap();
    let scenes = ["Arrival", "Letter", "Harbour"]
        .map(|t| p.add_document(&Place::Into(chapter.clone()), t).unwrap())
        .to_vec();
    (p, chapter, scenes)
}

fn chapter_titles(harness: &Harness<'static, WriterApp>, chapter: &str) -> Vec<String> {
    let project = harness.state().project.as_ref().unwrap();
    project.node(chapter).unwrap().children.iter().map(|n| n.title.clone()).collect()
}

/// A card or outline row's title (the binder has a row of the same name;
/// the view's comes after it).
fn in_view<'a>(harness: &'a Harness<'static, WriterApp>, label: &'a str) -> egui_kittest::Node<'a> {
    harness.get_all_by_label(label).last().unwrap()
}

/// Press at one point and let go at another, a few frames apart.
fn drag_points(harness: &mut Harness<'static, WriterApp>, start: egui::Pos2, end: egui::Pos2) {
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    harness.event(egui::Event::PointerMoved(start));
    harness.run_steps(1);
    harness.event(button(start, true));
    harness.run_steps(1);
    for step in 1..=6 {
        harness.event(egui::Event::PointerMoved(start.lerp(end, step as f32 / 6.0)));
        harness.run_steps(1);
    }
    harness.event(button(end, false));
    harness.run_steps(3);
}

#[test]
fn a_folder_shows_its_items_as_cards() {
    let dir = TempDir::new("cards");
    let (p, chapter, scenes) = chapter_project(&dir);
    let mut harness = harness(p);
    // Out of the way: it shows the folder's own synopsis field.
    harness.state_mut().show_inspector = false;
    harness.state_mut().select(chapter.clone());
    harness.run_steps(3);

    harness.get_by_label("Corkboard");
    let titles: Vec<_> = ["📄 Arrival", "📄 Letter", "📄 Harbour"].map(|t| harness.get_all_by_label(t).count()).to_vec();
    assert_eq!(titles, [2, 2, 2], "each in the binder and on a card");

    // A synopsis typed on a card is the item's synopsis.
    let field = harness.get_all_by_value("").find(|n| n.rect().width() > 150.0 && n.rect().left() > 200.0).unwrap();
    field.focus();
    field.type_text("She arrives.");
    harness.run_steps(2);
    let project = harness.state().project.as_ref().unwrap();
    assert_eq!(project.node(&scenes[0]).unwrap().meta.synopsis, "She arrives.");
    assert!(harness.state().manifest_dirty);

    // Double-clicking a card opens it.
    let pos = in_view(&harness, "📄 Letter").rect().center();
    harness.event(egui::Event::PointerMoved(pos));
    harness.run_steps(1);
    for _ in 0..2 {
        for pressed in [true, false] {
            harness.event(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
            harness.run_steps(1);
        }
    }
    harness.run_steps(2);
    assert_eq!(harness.state().selected.as_deref(), Some(scenes[1].as_str()));
}

#[test]
fn dragging_a_card_reorders_the_folder() {
    let dir = TempDir::new("cards-drag");
    let (p, chapter, _) = chapter_project(&dir);
    let mut harness = harness(p);
    // Out of the way: it shows the folder's own synopsis field.
    harness.state_mut().show_inspector = false;
    harness.state_mut().select(chapter.clone());
    harness.run_steps(3);

    // Harbour onto the left half of Arrival's card: before it.
    let start = in_view(&harness, "📄 Harbour").rect().center();
    let target = in_view(&harness, "📄 Arrival").rect();
    drag_points(&mut harness, start, target.left_center() + egui::vec2(4.0, 20.0));
    assert_eq!(chapter_titles(&harness, &chapter), ["Harbour", "Arrival", "Letter"]);
}

#[test]
fn the_outline_edits_status_and_nests_by_dragging() {
    let dir = TempDir::new("outline");
    let (p, chapter, scenes) = chapter_project(&dir);
    let mut harness = harness(p);
    // Out of the way: it shows the folder's own synopsis field.
    harness.state_mut().show_inspector = false;
    harness.state_mut().select(chapter.clone());
    harness.run_steps(3);
    harness.get_by_label("Outline").click();
    harness.run_steps(3);

    // Status from the first row's menu.
    harness.get_all_by_value("\u{2014}").next().unwrap().click();
    harness.run_steps(2);
    harness.get_by_label("Revised").click();
    harness.run_steps(2);
    let project = harness.state().project.as_ref().unwrap();
    assert_eq!(project.node(&scenes[0]).unwrap().meta.status, Some(3));

    // Letter onto the middle of Arrival's row: inside it.
    let start = in_view(&harness, "📄 Letter").rect().center();
    let target = in_view(&harness, "📄 Arrival").rect();
    drag_points(&mut harness, start, target.center() + egui::vec2(0.0, 8.0));
    assert_eq!(chapter_titles(&harness, &chapter), ["Arrival", "Harbour"]);
    let project = harness.state().project.as_ref().unwrap();
    assert_eq!(project.node(&scenes[0]).unwrap().children[0].title, "Letter");
    // Nested rows show in the outline too.
    assert_eq!(harness.get_all_by_label("📄 Letter").count(), 2);
}

// ----- compile -------------------------------------------------------------------

#[test]
fn compile_summarises_the_manuscript_and_writes_it() {
    let dir = TempDir::new("compile");
    let mut harness = harness(project(&dir, "Saved words here."));
    // Unsaved edits are compiled too.
    harness.state_mut().current().unwrap().doc.text = "Typed *but* not saved yet.".into();

    harness.state_mut().compile_open = true;
    // A few frames: the window sizes itself, then settles in the middle.
    harness.run_steps(6);
    harness.get_by_label_contains("1 chapter, 5 words");

    // Choices are the project's, saved with it.
    harness.get_by_label("Markdown (.md)").click();
    harness.run_steps(2);
    let project = harness.state().project.as_ref().unwrap();
    assert_eq!(project.manifest.compile.format, crate::compile::Format::Markdown);
    assert!(harness.state().manifest_dirty);

    // What Compile… writes once a place is chosen.
    let manuscript = harness.state().gather_manuscript().unwrap();
    let out = dir.0.join("Book.md");
    let markdown = crate::compile::CompileSettings { format: crate::compile::Format::Markdown, ..Default::default() };
    super::compile_window::write_compiled(&manuscript, &markdown, &out).unwrap();
    let md = fs::read_to_string(&out).unwrap();
    assert!(md.contains("# Untitled\n\nTyped *but* not saved yet.\n"), "{md}");

    let docx = dir.0.join("Book.docx");
    super::compile_window::write_compiled(&manuscript, &crate::compile::CompileSettings::default(), &docx).unwrap();
    assert!(docx_rs::read_docx(&fs::read(&docx).unwrap()).unwrap().json().contains("not saved yet"));
}

// ----- snapshots -----------------------------------------------------------------

fn snapshot_list(harness: &Harness<'static, WriterApp>) -> Vec<crate::snapshots::Snapshot> {
    let app = harness.state();
    crate::snapshots::list(&app.project.as_ref().unwrap().root, app.selected.as_deref().unwrap())
}

#[test]
fn snapshots_are_taken_compared_restored_and_deleted() {
    let dir = TempDir::new("snapshots");
    let mut harness = harness(project(&dir, "The ferry had gone."));

    // Named, from the inspector.
    harness.state_mut().snapshot_name = "first draft".into();
    harness.get_by_label("Take").click();
    harness.run_steps(2);
    // Unnamed, with ⌘5, of the text as it is in the editor (unsaved).
    harness.state_mut().current().unwrap().doc.text = "The last ferry was gone.".into();
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Num5);
    harness.run_steps(2);
    let list = snapshot_list(&harness);
    assert_eq!(list.len(), 2);
    assert_eq!(list.iter().find(|s| s.name == "first draft").unwrap().text().unwrap(), "The ferry had gone.");

    // Compare the named one with the current text, then restore it.
    harness.get_by_label_contains("first draft").click();
    harness.run_steps(3);
    harness.get_by_label_contains("added since");
    harness.get_by_label("Restore This Version").click();
    harness.run_steps(3);
    assert_eq!(current_text(&mut harness).as_deref(), Some("The ferry had gone."));
    // What was there is kept, so the restore can be undone.
    let list = snapshot_list(&harness);
    let kept = list.iter().find(|s| s.name.starts_with("before restoring")).unwrap();
    assert_eq!(kept.text().unwrap(), "The last ferry was gone.");

    // Delete one, after confirming.
    harness.get_by_label_contains("first draft").click();
    harness.run_steps(3);
    harness.get_by_label("Delete Snapshot\u{2026}").click();
    harness.run_steps(2);
    harness.get_by_label("Delete").click();
    harness.run_steps(2);
    assert!(snapshot_list(&harness).iter().all(|s| s.name != "first draft"));
    assert_eq!(snapshot_list(&harness).len(), 2);
}

// ----- recent projects and renaming ----------------------------------------------

#[test]
fn recent_projects_are_listed_and_reopen() {
    let dir = TempDir::new("recent");
    let alpha = Project::create(&dir.0.join("Alpha"), "Alpha").unwrap();
    let beta = Project::create(&dir.0.join("Beta"), "Beta").unwrap();
    let mut harness = harness(alpha);
    harness.state_mut().set_project(beta);
    harness.state_mut().recent.push(dir.0.join("Gone"));
    harness.state_mut().close_project();
    harness.run_steps(3);

    // Newest first.
    let recent = harness.state().recent.clone();
    assert_eq!(recent[..2], [dir.0.join("Beta"), dir.0.join("Alpha")]);

    // A project that isn't there any more can be taken off the list.
    harness.get_by_label_contains("(not found)");
    harness.get_by_label("Remove").click();
    harness.run_steps(2);
    assert_eq!(harness.state().recent.len(), 2);

    harness.get_by_label_contains("Alpha").click();
    harness.run_steps(3);
    assert_eq!(harness.state().project.as_ref().unwrap().title(), "Alpha");
    assert_eq!(harness.state().recent[0], dir.0.join("Alpha"));
}

#[test]
fn renaming_the_project_renames_its_folder_too() {
    let dir = TempDir::new("rename-project");
    let old_root = dir.0.join("Working Title");
    let p = Project::create(&old_root, "Working Title").unwrap();
    fs::write(old_root.join("manuscript/untitled.md"), "words").unwrap();
    let mut harness = harness(p);
    harness.state_mut().current().unwrap().doc.text = "words, unsaved".into();

    // The title in the toolbar opens the window.
    harness.get_by_label("Working Title").click();
    harness.run_steps(3);
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    harness.event(egui::Event::Text("The Long Winter".into()));
    harness.run_steps(1);
    harness.get_by_label("Rename").click();
    harness.run_steps(3);

    let new_root = dir.0.join("The Long Winter");
    assert!(!old_root.exists());
    let app = harness.state_mut();
    assert_eq!(app.project.as_ref().unwrap().title(), "The Long Winter");
    assert_eq!(app.project.as_ref().unwrap().root, new_root);
    assert_eq!(app.recent, std::slice::from_ref(&new_root));
    // Unsaved words were saved before the move, and editing carries on.
    assert_eq!(app.current().unwrap().doc.text, "words, unsaved");
    assert_eq!(fs::read_to_string(new_root.join("manuscript/untitled.md")).unwrap(), "words, unsaved");
}

#[test]
fn a_folder_with_another_name_keeps_it() {
    let dir = TempDir::new("rename-title-only");
    let p = Project::create(&dir.0.join("my-novel"), "Working Title").unwrap();
    let mut harness = harness(p);
    harness.state_mut().start_renaming_project();
    harness.run_steps(2);
    // The folder isn't named after the title, so it's left alone by default.
    harness.get_by_label("Rename").click();
    harness.run_steps(2);
    harness.state_mut().rename_project("Snowbound", false);
    assert!(dir.0.join("my-novel/project.ron").is_file());
    assert_eq!(harness.state().project.as_ref().unwrap().title(), "Snowbound");
}

// ----- characters ------------------------------------------------------------------

#[test]
fn a_character_is_a_profile_with_characteristics() {
    let dir = TempDir::new("character");
    let mut p = project(&dir, "");
    let mara = p.add_document(&Place::Into("characters".into()), "Mara").unwrap();
    fs::write(p.file_path(&mara).unwrap(), "Eldest daughter.\n").unwrap();
    let mut harness = harness(p);
    harness.get_by_label("👤 Mara").click();
    harness.run_steps(3);

    // The form shows the name and the description from the file.
    harness.get_all_by_value("Mara").next().unwrap();
    harness.get_all_by_value("Eldest daughter.").next().unwrap();

    // + Characteristic adds a section from the project's list.
    harness.get_by_label("+ Characteristic").click();
    harness.run_steps(2);
    harness.get_by_label("Motivations").click();
    harness.run_steps(2);
    assert_eq!(current_text(&mut harness).as_deref(), Some("Eldest daughter.\n\n## Motivations\n\n\n"));
    // Already there, so it isn't offered again.
    harness.get_by_label("+ Characteristic").click();
    harness.run_steps(2);
    assert!(harness.query_by_label("Motivations").is_none() || harness.get_all_by_label("Motivations").count() == 1);

    // Saved like any document.
    harness.state_mut().save_all();
    assert!(fs::read_to_string(dir.0.join("characters/mara.md")).unwrap().contains("## Motivations"));
}

#[test]
fn renaming_a_characteristic_renames_it_in_every_profile() {
    let dir = TempDir::new("characteristic-rename");
    let mut p = project(&dir, "");
    let a = p.add_document(&Place::Into("characters".into()), "Mara").unwrap();
    let b = p.add_document(&Place::Into("characters".into()), "Jonah").unwrap();
    fs::write(p.file_path(&a).unwrap(), "## Motivations\n\nTo leave.\n").unwrap();
    fs::write(p.file_path(&b).unwrap(), "## Motivations\n\nTo stay.\n").unwrap();
    let mut harness = harness(p);
    harness.state_mut().select(a.clone()); // one open, one not

    harness.state_mut().rename_section("characters", "Motivations", "Wants");
    let app = harness.state_mut();
    assert!(app.project.as_ref().unwrap().profile_kind("characters").unwrap().sections.contains(&"Wants".to_owned()));
    assert_eq!(app.current().unwrap().doc.text, "## Wants\n\nTo leave.\n");
    assert_eq!(fs::read_to_string(dir.0.join("characters/jonah.md")).unwrap(), "## Wants\n\nTo stay.\n");
}

#[test]
fn scenes_list_their_characters_and_characters_their_scenes() {
    let dir = TempDir::new("appearances");
    let mut p = project(&dir, "scene text");
    let scene = p.manifest.binder[0].children[0].id.clone();
    let mara = p.add_document(&Place::Into("characters".into()), "Mara").unwrap();
    let mut harness = harness(p);

    // In the scene's inspector: + Character, then Mara.
    harness.get_by_label("+ Character").click();
    harness.run_steps(2);
    harness.get_all_by_label("Mara").last().unwrap().click();
    harness.run_steps(2);
    let linked = |h: &Harness<'static, WriterApp>| h.state().project.as_ref().unwrap().node(&scene).unwrap().meta.links.get("characters").cloned().unwrap_or_default();
    assert_eq!(linked(&harness), std::slice::from_ref(&mara));

    // Mara's profile lists the scene; clicking it opens the scene.
    harness.state_mut().select(mara.clone());
    harness.run_steps(3);
    harness.get_by_label_contains("Appears in");
    harness.get_all_by_label("Untitled").last().unwrap().click();
    harness.run_steps(3);
    assert_eq!(harness.state().selected.as_deref(), Some(scene.as_str()));

    // Taking her out of the scene.
    harness.get_by_label("Mara  \u{d7}").click();
    harness.run_steps(2);
    assert!(linked(&harness).is_empty());
}

// ----- settings ----------------------------------------------------------------------

#[test]
fn settings_change_the_editor_and_are_remembered() {
    let dir = TempDir::new("settings");
    let mut harness = harness(project(&dir, "text"));
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Comma);
    harness.run_steps(6);
    assert!(harness.state().settings_open);
    harness.get_by_label("Restore Defaults");

    // Smart dashes off: the editor follows straight away.
    harness.get_by_label_contains("make an em dash").click();
    harness.run_steps(2);
    assert!(!harness.state().settings.smart_dashes);
    assert!(!harness.state().editor.smart_dashes);

    // Full width.
    harness.get_by_label("Keep lines to a comfortable length").click();
    harness.run_steps(2);
    assert_eq!(harness.state().editor.max_text_width, None);

    // Restore Defaults puts them back.
    harness.get_by_label("Restore Defaults").click();
    harness.run_steps(2);
    assert_eq!(harness.state().settings, crate::app::settings_ui::WriterSettings::default());
    assert!(harness.state().editor.smart_dashes);
}

#[test]
fn settings_open_from_the_start_screen() {
    let dir = TempDir::new("settings-start");
    let mut harness = harness(project(&dir, ""));
    harness.state_mut().close_project();
    harness.run_steps(2);
    harness.get_by_label("Settings\u{2026}").click();
    harness.run_steps(6);
    harness.get_by_label("Restore Defaults");
}

#[test]
fn spaces_typed_in_a_profile_are_kept() {
    let dir = TempDir::new("character-spaces");
    let mut p = project(&dir, "");
    let mara = p.add_document(&Place::Into("characters".into()), "Mara").unwrap();
    fs::write(p.file_path(&mara).unwrap(), "Eldest\n\n## Voice\n\nDry\n").unwrap();
    let mut harness = harness(p);
    harness.get_by_label("👤 Mara").click();
    harness.run_steps(3);

    // Click at the end of the description and type a word, its space, and
    // the next word, each in its own frame, as a person types.
    let field = harness.get_all_by_value("Eldest").next().unwrap().rect();
    harness.event(egui::Event::PointerMoved(field.right_center()));
    harness.run_steps(1);
    for pressed in [true, false] {
        harness.event(egui::Event::PointerButton {
            pos: field.right_center() - egui::vec2(4.0, 0.0),
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        harness.run_steps(1);
    }
    for typed in [" ", "daughter", ".", " ", "Left", " ", "first."] {
        harness.event(egui::Event::Text(typed.into()));
        harness.run_steps(1);
    }
    assert_eq!(
        current_text(&mut harness).as_deref(),
        Some("Eldest daughter. Left first.\n\n## Voice\n\nDry\n")
    );
}

// ----- syncing with the iPad helper ---------------------------------------------------

use crate::sync::{Action, Outcome, SyncState};

/// Hand the app a finished sync, as the background thread would.
fn finish(harness: &mut Harness<'static, WriterApp>, actions: Vec<Action>) {
    let root = harness.state().project.as_ref().unwrap().root.clone();
    let outcome = Outcome { actions, ..Outcome::default() };
    harness.state_mut().finish_sync(&root, Ok(outcome), 0.0);
    harness.run_steps(2);
}

#[test]
fn ipad_edits_come_into_open_and_closed_scenes() {
    let dir = TempDir::new("sync-in");
    let mut p = project(&dir, "Mac words");
    let other = p.add_document(&Place::Into("manuscript".into()), "Other").unwrap();
    fs::write(dir.0.join("manuscript/other.md"), "closed words").unwrap();
    let mut harness = harness(p);
    let open = harness.state().selected.clone().unwrap();

    finish(&mut harness, vec![
        Action::Replace { id: open.clone(), was: "Mac words".into(), text: "iPad words".into() },
        Action::Replace { id: other.clone(), was: "closed words".into(), text: "closed, from the iPad".into() },
    ]);
    assert_eq!(current_text(&mut harness).as_deref(), Some("iPad words"));
    assert_eq!(fs::read_to_string(dir.0.join("manuscript/untitled.md")).unwrap(), "iPad words", "saved straight away");
    assert_eq!(fs::read_to_string(dir.0.join("manuscript/other.md")).unwrap(), "closed, from the iPad");
}

#[test]
fn typing_during_a_sync_keeps_the_macs_words() {
    let dir = TempDir::new("sync-typing");
    let mut harness = harness(project(&dir, "Mac words"));
    let open = harness.state().selected.clone().unwrap();
    // Typed after the sync read the scene.
    harness.state_mut().current().unwrap().doc.text = "Mac words, and more".into();

    finish(&mut harness, vec![Action::Replace { id: open.clone(), was: "Mac words".into(), text: "iPad words".into() }]);
    assert_eq!(current_text(&mut harness).as_deref(), Some("Mac words, and more"));
    let kept = crate::snapshots::list(&dir.0, &open);
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].text().unwrap(), "iPad words");
}

#[test]
fn clashes_are_snapshotted_and_reported() {
    let dir = TempDir::new("sync-clash");
    let mut harness = harness(project(&dir, "Mac words"));
    let open = harness.state().selected.clone().unwrap();
    finish(&mut harness, vec![Action::Snapshot { id: open.clone(), text: "iPad words".into(), name: "from the iPad (both changed)".into() }]);
    assert_eq!(crate::snapshots::list(&dir.0, &open)[0].name, "from the iPad (both changed)");
    let status = harness.state().status.clone().unwrap().0;
    assert!(status.contains("\u{201c}Untitled\u{201d} changed on both"), "{status}");
}

#[test]
fn scenes_written_on_the_ipad_land_in_the_manuscript() {
    let dir = TempDir::new("sync-new");
    let mut harness = harness(project(&dir, ""));
    finish(&mut harness, vec![Action::NewScene { inbox_id: 7, title: "On the train".into(), text: "An idea.".into() }]);

    harness.get_by_label("📄 On the train");
    let app = harness.state();
    let project = app.project.as_ref().unwrap();
    let node = project.manifest.binder[0].children.iter().find(|n| n.title == "On the train").unwrap();
    assert!(node.meta.on_ipad, "it stays on the iPad");
    assert_eq!(fs::read_to_string(project.file_path(&node.id).unwrap()).unwrap(), "An idea.");
    assert!(SyncState::load(&dir.0).imported.contains(&7), "recorded, to be collected next time");
}

#[test]
fn notes_written_on_the_ipad_come_in_and_both_sides_are_kept_in_a_clash() {
    let dir = TempDir::new("sync-notes");
    let mut harness = harness(project(&dir, "Words."));
    let id = harness.state().selected.clone().unwrap();
    let notes = |h: &Harness<'static, WriterApp>| h.state().project.as_ref().unwrap().node(&id).unwrap().meta.notes.clone();
    let notes_id = crate::sync::notes_id(&id);

    // Changed only on the iPad: taken as they are.
    finish(&mut harness, vec![Action::Replace { id: notes_id.clone(), was: String::new(), text: "Check the ferry times.".into() }]);
    assert_eq!(notes(&harness), "Check the ferry times.");
    assert!(harness.state().manifest_dirty);
    assert!(!dir.0.join("snapshots").exists() || fs::read_dir(dir.0.join("snapshots")).unwrap().count() == 0, "no snapshot for notes");

    // Changed on both: the Mac's, then the iPad's.
    finish(&mut harness, vec![Action::Snapshot { id: notes_id, text: "Rain, not snow?".into(), name: "x".into() }]);
    assert_eq!(notes(&harness), "Check the ferry times.\n\nFrom the iPad:\nRain, not snow?");
}

#[test]
fn the_inspector_marks_scenes_for_the_ipad() {
    let dir = TempDir::new("sync-mark");
    let mut harness = harness(project(&dir, ""));
    let id = harness.state().selected.clone().unwrap();
    harness.get_by_label("Available on iPad").click();
    harness.run_steps(2);
    assert!(harness.state().project.as_ref().unwrap().on_ipad(&id));
    // Without a helper set up, nothing tries to sync, and it says so.
    assert!(!harness.state().sync_ready());
    harness.get_by_label_contains("Set up the iPad helper");
}

/// The whole app against the real helper (run by helper/tests/run.sh, which
/// sets HELPER_URL and HELPER_TOKEN).
#[test]
#[ignore]
fn app_syncs_with_the_real_helper() {
    let dir = TempDir::new("sync-real");
    let mut harness = harness(project(&dir, "Written on the Mac."));
    let id = harness.state().selected.clone().unwrap();
    {
        let app = harness.state_mut();
        app.settings.helper_url = std::env::var("HELPER_URL").unwrap();
        app.helper_token = std::env::var("HELPER_TOKEN").unwrap();
        let project = app.project.as_mut().unwrap();
        project.set_meta(&id, crate::project::Meta { on_ipad: true, notes: "Check the dates.".into(), ..Default::default() });
    }
    // Run frames until the background sync has come back.
    for _ in 0..600 {
        harness.run_steps(1);
        if harness.state().last_sync.is_some() && !harness.state().sync_running() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(harness.state().sync_message.starts_with("Synced"), "{}", harness.state().sync_message);
    let state = SyncState::load(&dir.0);
    assert_eq!(state.scenes[&id].text, "Written on the Mac.");
    assert_eq!(state.scenes[&crate::sync::notes_id(&id)].text, "Check the dates.", "its notes go too");
    assert!(harness.state().project.as_ref().unwrap().manifest.sync_id.is_some());
}

// ----- profile folders -------------------------------------------------------------

#[test]
fn a_locations_folder_works_like_characters() {
    let dir = TempDir::new("locations");
    let mut harness = harness(project(&dir, "scene text"));
    let scene = harness.state().selected.clone().unwrap();

    // Right-click a top-level folder: New Profile Folder → Locations.
    harness.get_by_label("📚 Notes").click_secondary();
    harness.run_steps(2);
    harness.get_by_label("New Profile Folder").hover();
    harness.run_steps(2);
    harness.get_by_label("Locations").click();
    harness.run_steps(3);
    harness.get_by_label("🗂 Locations");
    assert!(dir.0.join("locations").is_dir());

    // A location, with its form and its own sections.
    let harbour = harness
        .state_mut()
        .project
        .as_mut()
        .unwrap()
        .add_document(&Place::Into("locations".into()), "The Harbour")
        .unwrap();
    harness.state_mut().select(harbour.clone());
    harness.run_steps(3);
    harness.get_by_label("Location");
    harness.get_by_label("+ Section").click();
    harness.run_steps(2);
    harness.get_by_label("Sights and sounds").click();
    harness.run_steps(2);
    assert!(current_text(&mut harness).unwrap().contains("## Sights and sounds"));

    // The scene's inspector links it: + Location.
    harness.state_mut().select(scene.clone());
    harness.run_steps(3);
    harness.get_by_label("+ Location").click();
    harness.run_steps(2);
    harness.get_all_by_label("The Harbour").last().unwrap().click();
    harness.run_steps(2);
    let links = harness.state().project.as_ref().unwrap().node(&scene).unwrap().meta.links.clone();
    assert_eq!(links["locations"], [harbour]);
}

// ----- split view --------------------------------------------------------------------

/// Click at a point (press and release in their own frames).
fn click_at(harness: &mut Harness<'static, WriterApp>, pos: egui::Pos2) {
    harness.event(egui::Event::PointerMoved(pos));
    harness.run_steps(1);
    for pressed in [true, false] {
        harness.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
        harness.run_steps(1);
    }
}

#[test]
fn option_click_opens_beside_the_main_editor() {
    let dir = TempDir::new("split");
    let mut p = project(&dir, "main scene");
    let other = p.add_document(&Place::Into("notes".into()), "Research note").unwrap();
    fs::write(p.file_path(&other).unwrap(), "a note").unwrap();
    let mut harness = harness(p);
    let main = harness.state().selected.clone();

    harness.get_by_label("📄 Research note").click_modifiers(egui::Modifiers::ALT);
    harness.run_steps(3);
    let app = harness.state();
    assert!(app.split_open);
    assert_eq!(app.split.as_deref(), Some(other.as_str()));
    assert_eq!(app.selected, main, "the main editor keeps its scene");
    harness.get_all_by_value("a note").next().unwrap();
    harness.get_all_by_value("main scene").next().unwrap();

    // Typing in the split pane edits that document.
    let field = harness.get_all_by_value("a note").next().unwrap().rect();
    click_at(&mut harness, field.right_center() - egui::vec2(4.0, 0.0));
    harness.event(egui::Event::Text(" and more".into()));
    harness.run_steps(2);
    harness.state_mut().save_all();
    assert_eq!(fs::read_to_string(dir.0.join("notes/research-note.md")).unwrap(), "a note and more");
    assert_eq!(fs::read_to_string(dir.0.join("manuscript/untitled.md")).unwrap(), "main scene");

    // ⌘\\ closes it.
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Backslash);
    harness.run_steps(2);
    assert!(!harness.state().split_open);
}

#[test]
fn two_profiles_side_by_side_keep_their_typing() {
    let dir = TempDir::new("split-profiles");
    let mut p = project(&dir, "");
    let mara = p.add_document(&Place::Into("characters".into()), "Mara").unwrap();
    let jonah = p.add_document(&Place::Into("characters".into()), "Jonah").unwrap();
    fs::write(p.file_path(&mara).unwrap(), "Eldest\n").unwrap();
    fs::write(p.file_path(&jonah).unwrap(), "Youngest\n").unwrap();
    let mut harness = harness(p);
    harness.state_mut().show_inspector = false;
    harness.state_mut().select(mara.clone());
    harness.state_mut().open_in_split(jonah.clone());
    harness.run_steps(3);

    // Type with spaces into the split pane's profile, word by word.
    let field = harness.get_all_by_value("Youngest").next().unwrap().rect();
    click_at(&mut harness, field.right_center() - egui::vec2(4.0, 0.0));
    for typed in [" ", "son", ",", " ", "stayed", " ", "home."] {
        harness.event(egui::Event::Text(typed.into()));
        harness.run_steps(1);
    }
    let text = |h: &mut Harness<'static, WriterApp>, id: &str| h.state_mut().tab_of(id).unwrap().doc.text.clone();
    assert_eq!(text(&mut harness, &jonah), "Youngest son, stayed home.\n");
    assert_eq!(text(&mut harness, &mara), "Eldest\n");
}

#[test]
fn the_split_pane_picks_from_its_menu_and_wont_double_up() {
    let dir = TempDir::new("split-menu");
    let mut p = project(&dir, "main");
    p.add_document(&Place::Into("notes".into()), "Note").unwrap();
    let mut harness = harness(p);
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Backslash);
    harness.run_steps(3);
    harness.get_by_label_contains("Choose what to show here");

    harness.get_by_label("Show\u{2026}").click();
    harness.run_steps(2);
    harness.get_all_by_label("Untitled").last().unwrap().click();
    harness.run_steps(2);
    // The scene that's already in the main editor.
    harness.get_by_label("This is open in the main editor.");
}

#[test]
fn compile_presets_in_the_window() {
    let dir = TempDir::new("compile-presets");
    let mut harness = harness(project(&dir, "words"));
    harness.state_mut().compile_open = true;
    harness.run_steps(6);

    // E-book, saved as "Beta readers".
    harness.get_by_label("E-book (.epub)").click();
    harness.run_steps(2);
    harness.get_by_label("Save as Preset\u{2026}").click();
    harness.run_steps(2);
    harness.event(egui::Event::Text("Beta readers".into()));
    harness.run_steps(1);
    harness.get_by_label("Save").click();
    harness.run_steps(2);
    let compile = |h: &Harness<'static, WriterApp>| h.state().project.as_ref().unwrap().manifest.compile.clone();
    assert_eq!(compile(&harness).presets.len(), 1);
    assert_eq!(compile(&harness).presets[0].format, crate::compile::Format::Epub);

    // Back to Word: the preset no longer matches ("Custom"); picking it restores.
    harness.get_by_label("Word manuscript (.docx)").click();
    harness.run_steps(2);
    harness.get_all_by_value("Custom").next().unwrap().click();
    harness.run_steps(2);
    harness.get_by_label("Beta readers").click();
    harness.run_steps(2);
    assert_eq!(compile(&harness).format, crate::compile::Format::Epub);
    assert!(harness.state().manifest_dirty, "kept with the project");
}

// ----- about and help ----------------------------------------------------------

#[test]
fn help_shows_the_shortcuts_the_release_notes_and_the_version() {
    let dir = TempDir::new("help");
    let mut harness = harness(project(&dir, ""));
    harness.get_by_label("Help").click();
    harness.run_steps(2);
    harness.get_by_label("Find in Project");
    harness.get_by_label("Release Notes").click();
    harness.run_steps(2);
    harness.get_by_label("1.6.0");
    harness.get_by_label("About").click();
    harness.run_steps(2);
    harness.get_by_label(&format!("Version {}", env!("CARGO_PKG_VERSION")));
    
}

// ----- writing statistics ------------------------------------------------------

/// Today's words as the log has them.
fn logged_today(dir: &TempDir) -> i64 {
    let stats = crate::stats::Stats::load(&dir.0);
    stats.days.get(&crate::stats::key(crate::stats::today())).copied().unwrap_or(0)
}

#[test]
fn words_written_in_the_manuscript_count_towards_today() {
    let dir = TempDir::new("stats");
    let mut p = project(&dir, "Already here before today.");
    let notes_id = p.add_document(&Place::Into("notes".into()), "Idea").unwrap();
    let mut harness = harness(p);
    harness.run_steps(2); // the editor takes focus
    harness.get_by_label("Today: 0 words");

    // Typing shows straight away, and is logged when saved.
    harness.event(egui::Event::Text(" Then the snow came.".into()));
    harness.run_steps(2);
    harness.get_by_label("Today: 4 words");
    harness.state_mut().save_all();
    assert_eq!(logged_today(&dir), 4);

    // Notes aren't the manuscript.
    let notes = harness.state_mut().ensure_open(&notes_id).unwrap();
    harness.state_mut().docs.get_mut(notes).unwrap().doc.text = "lots of words for later on".into();
    harness.state_mut().save_all();
    assert_eq!(logged_today(&dir), 4);

    // Trashing a scene isn't writing (or unwriting) it.
    let id = harness.state().selected.clone().unwrap();
    harness.state_mut().trash(&id);
    harness.state_mut().save_all();
    harness.run_steps(2);
    assert_eq!(logged_today(&dir), 4);
    harness.get_by_label("Today: 4 words");
}

#[test]
fn goals_set_in_the_statistics_window_show_in_the_status_bar() {
    let dir = TempDir::new("goals");
    let mut harness = harness(project(&dir, "One two three."));
    harness.get_by_label("Today: 0 words").click();
    harness.run_steps(2);
    harness.get_by_label("Manuscript length");
    harness.state_mut().project.as_mut().unwrap().manifest.goals.daily = Some(1000);
    harness.run_steps(2);
    harness.get_by_label("Today: 0 / 1,000");
    harness.get_by_label("words today, of 1,000");
}

// ----- spelling ----------------------------------------------------------------

#[test]
fn right_clicking_a_misspelled_word_offers_its_spelling() {
    let dir = TempDir::new("spelling");
    let mut harness = harness(project(&dir, "Teh snow came early."));
    harness.run_steps(2);
    // Right-click on the first word, at the text's top left.
    let rect = harness.get_by_label("Document text").rect();
    let pos = rect.min + egui::vec2(14.0, 12.0);
    harness.event(egui::Event::PointerMoved(pos));
    for pressed in [true, false] {
        harness.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Secondary, pressed, modifiers: egui::Modifiers::NONE });
        harness.run_steps(1);
    }
    harness.run_steps(2);
    harness.get_by_label("Learn Spelling");
    harness.get_by_label("Ignore Spelling");
    harness.get_by_label("The").click();
    harness.run_steps(3);
    assert_eq!(current_text(&mut harness).as_deref(), Some("The snow came early."));
}

// ----- splitting, selection counts, the binder, case ------------------------------------

#[test]
fn split_at_cursor_moves_the_rest_into_a_new_document() {
    let dir = TempDir::new("split-doc");
    let mut harness = harness(project(&dir, "The ferry had gone.\n\nNext morning she walked."));
    harness.run_steps(2);
    let first = harness.state().selected.clone().unwrap();
    let tab = harness.state_mut().current().unwrap().id;
    let ctx = harness.ctx.clone();
    sloth_core::editor::set_selection(&ctx, tab, 19..19);
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::K);
    harness.run_steps(3);

    let app = harness.state();
    let project = app.project.as_ref().unwrap();
    let titles = manuscript_titles(&harness);
    assert_eq!(titles, ["Untitled", "Untitled (continued)"]);
    let second = harness.state().selected.clone().unwrap();
    assert_ne!(second, first, "the new one is selected");
    let read = |id: &str| fs::read_to_string(project.file_path(id).unwrap()).unwrap();
    assert_eq!(read(&first), "The ferry had gone.");
    assert_eq!(read(&second), "Next morning she walked.");
    assert!(harness.state().renaming.is_some(), "ready to name it");
    // Moved, not written.
    harness.get_by_label("Today: 0 words");
}

#[test]
fn the_status_bar_counts_the_selection_too() {
    let dir = TempDir::new("selection-count");
    let mut harness = harness(project(&dir, "One two three"));
    harness.run_steps(2);
    assert!(harness.query_all_by_label("3 words").count() >= 1, "in the status bar (and the inspector)");
    let tab = harness.state_mut().current().unwrap().id;
    let ctx = harness.ctx.clone();
    sloth_core::editor::set_selection(&ctx, tab, 0..7);
    harness.run_steps(2);
    harness.get_by_label("2 of 3 words");
}

#[test]
fn the_binder_can_be_hidden() {
    let dir = TempDir::new("hide-binder");
    let mut harness = harness(project(&dir, ""));
    assert_eq!(harness.query_all_by_label("📚 Manuscript").count(), 1);
    harness.key_press_modifiers(egui::Modifiers::COMMAND | egui::Modifiers::ALT, egui::Key::B);
    harness.run_steps(2);
    assert_eq!(harness.query_all_by_label("📚 Manuscript").count(), 0);
    harness.get_by_label("Binder").click();
    harness.run_steps(2);
    assert_eq!(harness.query_all_by_label("📚 Manuscript").count(), 1);
}

#[test]
fn change_case_from_the_right_click_menu() {
    let dir = TempDir::new("case");
    let mut harness = harness(project(&dir, "the long winter"));
    harness.run_steps(2);
    let tab = harness.state_mut().current().unwrap().id;
    let ctx = harness.ctx.clone();
    sloth_core::editor::set_selection(&ctx, tab, 0..15);
    harness.get_by_label("Document text").click_secondary();
    harness.run_steps(2);
    harness.get_by_label("Change Case").hover();
    harness.run_steps(2);
    harness.get_by_label("Title Case").click();
    harness.run_steps(3);
    assert_eq!(current_text(&mut harness).as_deref(), Some("The Long Winter"));
}

#[test]
fn back_up_saves_first_and_remembers_the_folder() {
    let dir = TempDir::new("backup");
    let out = TempDir::new("backup-out");
    fs::create_dir_all(&out.0).unwrap();
    let mut harness = harness(project(&dir, "Saved words."));
    harness.state_mut().current().unwrap().doc.text.push_str(" Unsaved ones.");
    harness.state_mut().back_up_to(&out.0, 0.0);
    harness.run_steps(2);

    let zips: Vec<_> = fs::read_dir(&out.0).unwrap().map(|e| e.unwrap().path()).collect();
    assert_eq!(zips.len(), 1);
    assert!(zips[0].file_name().unwrap().to_string_lossy().starts_with("Book 20"));
    let mut zip = zip::ZipArchive::new(fs::File::open(&zips[0]).unwrap()).unwrap();
    let top = dir.0.file_name().unwrap().to_string_lossy().into_owned();
    let mut text = String::new();
    std::io::Read::read_to_string(&mut zip.by_name(&format!("{top}/manuscript/untitled.md")).unwrap(), &mut text).unwrap();
    assert_eq!(text, "Saved words. Unsaved ones.");
    assert_eq!(harness.state().settings.backup_folder.as_deref(), Some(out.0.to_string_lossy().as_ref()));
    assert!(harness.state().status.as_ref().is_some_and(|(m, _)| m.starts_with("Backed up to")));
}

#[test]
fn a_documents_notes_are_above_its_text() {
    let dir = TempDir::new("doc-notes");
    let mut harness = harness(project(&dir, "The ferry had gone."));
    harness.run_steps(2);
    // Folded while there are none; open it and write some.
    harness.get_by_label("Notes").click();
    harness.run_steps(2);
    let field = harness.get_all_by_value("").find(|n| n.rect().left() > 240.0 && n.rect().right() < 730.0).unwrap();
    field.focus();
    field.type_text("Check the ferry times.");
    harness.run_steps(2);
    let id = harness.state().selected.clone().unwrap();
    let project = harness.state().project.as_ref().unwrap();
    assert_eq!(project.node(&id).unwrap().meta.notes, "Check the ferry times.");
    assert!(harness.state().manifest_dirty);
    assert_eq!(current_text(&mut harness).as_deref(), Some("The ferry had gone."), "not in the text");
}

#[test]
fn the_notes_panel_is_resized_by_its_bottom_edge() {
    let dir = TempDir::new("notes-resize");
    let mut p = project(&dir, "Text.");
    let scene = p.manifest.binder[0].children[0].id.clone();
    p.set_meta(&scene, crate::project::Meta { notes: "A note.".into(), ..Default::default() });
    let mut harness = harness(p);
    harness.run_steps(3);
    let before = harness.state().settings.notes_height;
    let handle = harness.get_by_label("Drag to make the notes taller or shorter").rect().center();
    drag_points(&mut harness, handle, handle + egui::vec2(0.0, 120.0));
    let after = harness.state().settings.notes_height;
    assert!(after > before + 80.0, "{before} → {after}");
}

#[test]
fn undo_takes_back_a_formatting_edit_on_its_own() {
    let dir = TempDir::new("undo-format");
    let mut harness = harness(project(&dir, "a word here"));
    harness.run_steps(2);
    // Typed, then made bold straight away (well inside a second).
    harness.event(egui::Event::Text("!".into()));
    harness.run_steps(3);
    let tab = harness.state_mut().current().unwrap().id;
    let ctx = harness.ctx.clone();
    sloth_core::editor::set_selection(&ctx, tab, 2..6);
    harness.run_steps(2);
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::B);
    harness.run_steps(3);
    assert_eq!(current_text(&mut harness).as_deref(), Some("a **word** here!"));
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    harness.run_steps(3);
    assert_eq!(current_text(&mut harness).as_deref(), Some("a word here!"), "the bold goes, the typing stays");
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    harness.run_steps(3);
    assert_eq!(current_text(&mut harness).as_deref(), Some("a word here"));
    // And redo brings them back in turn.
    harness.key_press_modifiers(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::Z);
    harness.run_steps(3);
    assert_eq!(current_text(&mut harness).as_deref(), Some("a word here!"));
}

#[test]
fn closing_a_project_backs_it_up_automatically() {
    let dir = TempDir::new("auto-backup");
    let out = TempDir::new("auto-backup-out");
    let mut harness = harness(project(&dir, "Words."));
    {
        let app = harness.state_mut();
        app.settings.auto_backup = true;
        app.settings.auto_backup_folder = Some(out.0.to_string_lossy().into_owned());
        app.current().unwrap().doc.text.push_str(" More.");
    }
    harness.state_mut().close_project();
    let folder = crate::backup::auto_folder(&out.0, &dir.0);
    let (zip_path, _) = crate::backup::newest(&folder).expect("a backup was made");
    let mut zip = zip::ZipArchive::new(fs::File::open(zip_path).unwrap()).unwrap();
    let top = dir.0.file_name().unwrap().to_string_lossy().into_owned();
    let mut text = String::new();
    std::io::Read::read_to_string(&mut zip.by_name(&format!("{top}/manuscript/untitled.md")).unwrap(), &mut text).unwrap();
    assert_eq!(text, "Words. More.", "saved first, then backed up");
}

// ----- one copy of Writer per project ------------------------------------------------

/// Mark a project as open in another copy of Writer, on "Studio iMac".
fn opened_elsewhere(root: &std::path::Path) {
    let now = chrono::Utc::now().timestamp();
    let lock = crate::lock::Lock { owner: "another-copy".into(), host: "Studio iMac".into(), since: now, beat: now };
    fs::create_dir_all(root.join(".sloth")).unwrap();
    fs::write(root.join(".sloth").join(crate::lock::FILE_NAME), ron::to_string(&lock).unwrap()).unwrap();
}

#[test]
fn opening_a_project_open_elsewhere_asks_first() {
    let dir = TempDir::new("lock-open");
    let other = TempDir::new("lock-open-other");
    let mut harness = harness(project(&dir, "Here."));
    drop(Project::create(&other.0, "Other").unwrap());
    opened_elsewhere(&other.0);

    harness.state_mut().open_project(&other.0);
    harness.run_steps(2);
    harness.get_by_label_contains("is open on Studio iMac");
    assert_eq!(harness.state().project.as_ref().unwrap().root, dir.0, "not opened yet");
    harness.get_by_label("Open Anyway").click();
    harness.run_steps(2);
    assert_eq!(harness.state().project.as_ref().unwrap().root, other.0);
    assert_eq!(crate::lock::holder(&other.0), crate::lock::Holder::Us);
    assert_eq!(crate::lock::holder(&dir.0), crate::lock::Holder::Nobody, "the first one was let go");
}

#[test]
fn when_another_copy_takes_the_project_nothing_is_saved_over_it() {
    let dir = TempDir::new("lock-lost");
    let mut harness = harness(project(&dir, "Words."));
    let id = harness.state().selected.clone().unwrap();
    opened_elsewhere(&dir.0);
    harness.state_mut().last_lock_beat = -100.0;
    harness.run_steps(2);
    harness.get_by_label_contains("This project was opened on Studio iMac");

    // Typing here isn't saved over the other copy's file…
    harness.state_mut().current().unwrap().doc.text.push_str(" Written here.");
    harness.state_mut().last_autosave = -100.0;
    harness.run_steps(2);
    assert_eq!(fs::read_to_string(dir.0.join("manuscript/untitled.md")).unwrap(), "Words.");
    // …and closing keeps it as a snapshot instead.
    harness.get_by_label("Close It Here").click();
    harness.run_steps(2);
    let kept = crate::snapshots::list(&dir.0, &id);
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].text().unwrap(), "Words. Written here.");
    assert!(kept[0].name.contains("Studio iMac"));
    assert!(matches!(crate::lock::holder(&dir.0), crate::lock::Holder::Other(_)), "their lock is left alone");
}

// ----- importing from Scrivener ------------------------------------------------------

#[test]
fn a_scrivener_project_is_imported_and_opened() {
    let dir = TempDir::new("import-here");
    let src = TempDir::new("import-src");
    fs::create_dir_all(&src.0).unwrap();
    let scriv = crate::scrivener::tests::make_scriv(&src.0);
    let mut harness = harness(project(&dir, "Before."));
    let dest = src.0.join("The Long Winter");
    harness.state_mut().import_scrivener(&scriv, &dest, 0.0);
    harness.run_steps(3);
    assert_eq!(harness.state().project.as_ref().unwrap().root, dest);
    // The first document in the Manuscript is open: the chapter's own text.
    let selected = harness.state().selected.clone().unwrap();
    let project = harness.state().project.as_ref().unwrap();
    assert_eq!(project.node(&selected).unwrap().title, "Chapter One");
    assert_eq!(current_text(&mut harness).as_deref(), Some("Words in the chapter folder itself."));
    assert!(harness.state().status.as_ref().is_some_and(|(m, _)| m.starts_with("Imported 7 documents")));

    // A failed import leaves nothing behind.
    let bad = src.0.join("Empty.scriv");
    fs::create_dir_all(&bad).unwrap();
    let nowhere = src.0.join("Nothing");
    harness.state_mut().import_scrivener(&bad, &nowhere, 0.0);
    assert!(harness.state().error.as_ref().is_some_and(|e| e.contains("isn't a Scrivener project")));
    assert!(!nowhere.exists());
}
