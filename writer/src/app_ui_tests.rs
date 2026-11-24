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
fn replace_one_at_a_time_or_all_in_the_open_document() {
    let dir = TempDir::new("replace");
    let mut harness = harness(project(&dir, "the cat and the hat and the bat"));
    harness.state_mut().open_find(false);
    type_text(&mut harness, "the");
    harness.get_by_label("Replace…").click();
    harness.run_steps(2);
    harness.state_mut().find.replacement = "a".into();

    // Replace: the selected match, then on to the next.
    harness.get_by_label("Replace").click();
    harness.run_steps(2);
    assert_eq!(current_text(&mut harness).as_deref(), Some("a cat and the hat and the bat"));
    assert_eq!(harness.state().find.current, Some(10..13), "on to the next");

    harness.get_by_label("Replace All").click();
    harness.run_steps(2);
    assert_eq!(current_text(&mut harness).as_deref(), Some("a cat and a hat and a bat"));
    assert_eq!(harness.state().status.as_ref().map(|(m, _)| m.as_str()), Some("Replaced 2 occurrences"));
}

#[test]
fn replace_all_in_the_project_asks_and_snapshots_first() {
    let dir = TempDir::new("replace-project");
    let mut p = project(&dir, "The ferry left. The ferry came back.");
    let first = p.documents().into_iter().find(|n| n.title == "Untitled").unwrap().id.clone();
    let second = p.add_document(&Place::Into("notes".into()), "Timetable").unwrap();
    fs::write(p.file_path(&second).unwrap(), "ferry at noon").unwrap();
    let cut = p.add_document(&Place::Into("manuscript".into()), "Cut").unwrap();
    fs::write(p.file_path(&cut).unwrap(), "a ferry in the bin").unwrap();
    p.trash(&cut).unwrap();
    let mut harness = harness(p);

    harness.state_mut().open_find(true);
    type_text(&mut harness, "ferry");
    harness.get_by_label("Replace…").click();
    harness.run_steps(2);
    harness.state_mut().find.replacement = "boat".into();
    harness.get_by_label("Replace All").click();
    harness.run_steps(2);

    // It asks first, with what it'll do.
    assert_eq!(harness.state().confirm_project_replace, Some((3, 2)));
    harness.get_by_label("Replace 3 occurrences in 2 documents?");
    harness.get_all_by_label("Replace All").last().unwrap().click();
    harness.run_steps(3);
    assert_eq!(harness.state().confirm_project_replace, None);

    assert_eq!(current_text(&mut harness).as_deref(), Some("The boat left. The boat came back."));
    let project = harness.state().project.as_ref().unwrap();
    assert_eq!(fs::read_to_string(project.file_path(&second).unwrap()).unwrap(), "boat at noon");
    assert_eq!(fs::read_to_string(project.file_path(&cut).unwrap()).unwrap(), "a ferry in the bin", "not in the Trash");
    // Each changed document has a snapshot of how it was.
    for (id, was) in [(&first, "The ferry left. The ferry came back."), (&second, "ferry at noon")] {
        let snaps = crate::snapshots::list(&dir.0, id);
        assert_eq!(snaps.len(), 1);
        assert!(snaps[0].name.contains("Before replacing"), "{:?}", snaps[0].name);
        assert_eq!(fs::read_to_string(&snaps[0].path).unwrap(), was);
    }
    assert!(harness.state().status.as_ref().is_some_and(|(m, _)| m.starts_with("Replaced 3 occurrences in 2 documents")));
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
fn a_card_with_no_synopsis_shows_how_the_document_opens() {
    let dir = TempDir::new("cards-opening");
    let (mut p, chapter, scenes) = chapter_project(&dir);
    fs::write(p.file_path(&scenes[0]).unwrap(), "# Arrival\n\nMara was *late*, and the ferry had gone.").unwrap();
    fs::write(p.file_path(&scenes[1]).unwrap(), "Dear Mara,").unwrap();
    p.set_meta(&scenes[1], crate::project::Meta { synopsis: "The letter.".into(), ..Default::default() });
    let mut harness = harness(p);
    harness.state_mut().show_inspector = false;
    harness.state_mut().select(chapter.clone());
    harness.run_steps(3);
    let items = harness.state_mut().view_items(&chapter, false);
    let openings: Vec<&str> = items.iter().map(|i| i.opening.as_str()).collect();
    assert_eq!(openings, ["Mara was late, and the ferry had gone.", "", ""], "a synopsis wins; an empty document has none");
    if let Ok(dir) = std::env::var("SHOT_DIR") {
        harness.render().unwrap().save(format!("{dir}/corkboard.png")).unwrap();
    }
}

#[test]
fn outline_rows_and_canvas_cards_show_openings_too() {
    let dir = TempDir::new("openings-everywhere");
    let (p, chapter, scenes) = chapter_project(&dir);
    fs::write(p.file_path(&scenes[0]).unwrap(), "# Arrival\n\nMara was *late*, and the ferry had gone.").unwrap();
    let mut harness = harness(p);
    harness.state_mut().show_inspector = false;
    harness.state_mut().select(chapter.clone());
    harness.run_steps(3);
    // The outline: everything under the folder, nested.
    let rows = harness.state_mut().view_items(&chapter, true);
    assert_eq!(rows[0].opening, "Mara was late, and the ferry had gone.");
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Num3);
    harness.run_steps(3);
    if let Ok(dir) = std::env::var("SHOT_DIR") {
        harness.render().unwrap().save(format!("{dir}/outline.png")).unwrap();
    }

    harness.get_by_label("Canvas").click();
    harness.run_steps(3);
    let canvas = harness.state().canvas.as_ref().unwrap();
    assert_eq!(canvas.openings().get(&scenes[0]).map(String::as_str), Some("Mara was late, and the ferry had gone."));
    assert!(!canvas.openings().contains_key(&scenes[1]), "nothing written, nothing to show");
    if let Ok(dir) = std::env::var("SHOT_DIR") {
        harness.render().unwrap().save(format!("{dir}/canvas.png")).unwrap();
    }
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
fn a_projects_icon_is_chosen_from_its_tile_or_its_title() {
    let dir = TempDir::new("kind-tile");
    let p = Project::create(&dir.0.join("Snow"), "Snow").unwrap();
    let mut harness = harness(p);

    // In the project: click its title, pick an icon.
    harness.get_by_label("Snow").click();
    harness.run_steps(3);
    harness.get_by_label_contains("Short Fiction").click();
    harness.run_steps(2);
    assert_eq!(harness.state_mut().project.as_ref().unwrap().kind(), crate::project::Kind::ShortFiction);
    harness.get_by_label("Cancel").click();
    harness.run_steps(2);

    // On the start screen: the tile shows it; right-click to change it.
    harness.state_mut().close_project();
    harness.run_steps(3);
    harness.get_by_label_contains("Snow, Short Fiction").click_secondary();
    harness.run_steps(2);
    harness.get_by_label("Icon").hover();
    harness.run_steps(2);
    harness.get_by_label_contains("Podcast Script").click();
    harness.run_steps(3);
    harness.get_by_label_contains("Snow, Podcast Script");
    let reopened = Project::open(&dir.0.join("Snow")).unwrap();
    assert_eq!(reopened.kind(), crate::project::Kind::Podcast);
}

#[test]
fn a_profile_lists_where_it_is_referenced() {
    let dir = TempDir::new("references");
    let essay = "## On images\nThe gaze [[Ways of Seeing|p. 47]] is learned.\n\n## On ads\nNothing here.";
    let mut p = project(&dir, essay);
    let sources = crate::project::profile_presets().into_iter().map(|(k, _)| k).find(|k| k.id == "sources").unwrap();
    p.add_profile_folder(sources).unwrap();
    let berger = p.add_document(&Place::Into("sources".into()), "Ways of Seeing").unwrap();
    fs::write(p.file_path(&berger).unwrap(), "John Berger, 1972.").unwrap();
    let doc = p.documents()[0].id.clone();
    let mut harness = harness(p);

    harness.state_mut().select(berger);
    harness.run_steps(3);
    harness.get_by_label("Referenced in");
    harness.get_by_label("The gaze is learned.");
    harness.get_by_label("On images \u{b7} p. 47").click();
    harness.run_steps(3);

    // The essay opens with the reference selected.
    assert_eq!(harness.state().selected.as_deref(), Some(doc.as_str()));
    let tab = harness.state_mut().current().unwrap().id;
    let start = essay.find("[[").unwrap();
    let end = essay.find("]]").unwrap() + 2;
    assert_eq!(sloth_core::editor::selection(&harness.ctx, tab), Some(start..end));
    // References aren't words.
    let without = essay.replace(" [[Ways of Seeing|p. 47]]", "");
    assert_eq!(crate::app::inspector::count_words(essay), crate::app::inspector::count_words(&without));
}

/// A project whose one document says `text`, with a Sources folder holding
/// "Ways of Seeing" and a Concepts folder: the document is selected, and
/// typing goes at its end.
fn essay_with_sources(dir: &TempDir, text: &str) -> (Harness<'static, WriterApp>, String, String) {
    let mut p = project(dir, text);
    for id in ["sources", "concepts"] {
        let kind = crate::project::profile_presets().into_iter().map(|(k, _)| k).find(|k| k.id == id).unwrap();
        p.add_profile_folder(kind).unwrap();
    }
    let berger = p.add_document(&Place::Into("sources".into()), "Ways of Seeing").unwrap();
    let doc = p.documents()[0].id.clone();
    let mut harness = harness(p);
    harness.state_mut().select(doc.clone());
    harness.run_steps(2);
    harness.get_by_label("Document text").click();
    harness.run_steps(2);
    let tab = harness.state_mut().current().unwrap().id;
    let end = text.chars().count();
    sloth_core::editor::set_selection(&harness.ctx.clone(), tab, end..end);
    harness.run_steps(1);
    (harness, doc, berger)
}

#[test]
fn typing_double_brackets_suggests_profiles() {
    let dir = TempDir::new("suggest-ui");
    let (mut harness, ..) = essay_with_sources(&dir, "The gaze ");
    harness.event(egui::Event::Text("[[way".into()));
    harness.run_steps(3);
    harness.get_by_label("Ways of Seeing   Source");
    harness.key_press(egui::Key::Enter);
    harness.run_steps(3);
    assert_eq!(current_text(&mut harness).as_deref(), Some("The gaze [[Ways of Seeing]]"));
    assert!(harness.state().suggesting.is_none());
    // Typing on carries on after the reference.
    harness.event(egui::Event::Text(" applies.".into()));
    harness.run_steps(2);
    assert_eq!(current_text(&mut harness).as_deref(), Some("The gaze [[Ways of Seeing]] applies."));
}

#[test]
fn a_new_profile_can_be_made_from_a_reference() {
    let dir = TempDir::new("suggest-new");
    let (mut harness, ..) = essay_with_sources(&dir, "It sells ");
    harness.event(egui::Event::Text("[[envy".into()));
    harness.run_steps(3);
    harness.get_by_label_contains("New Concept").click();
    harness.run_steps(3);
    assert_eq!(current_text(&mut harness).as_deref(), Some("It sells [[envy]]"));
    let app = harness.state();
    let project = app.project.as_ref().unwrap();
    let envy = project.profile_items("concepts");
    assert_eq!(envy.iter().map(|n| n.title.as_str()).collect::<Vec<_>>(), ["envy"]);
    // Esc puts the suggestions away.
    harness.event(egui::Event::Text(" [[w".into()));
    harness.run_steps(3);
    assert!(harness.state().suggesting.is_some());
    harness.key_press(egui::Key::Escape);
    harness.run_steps(2);
    assert!(harness.state().suggesting.is_none());
}

#[test]
fn renaming_a_profile_renames_references_to_it() {
    let dir = TempDir::new("rename-refs");
    let (mut harness, doc, berger) = essay_with_sources(&dir, "The gaze [[Ways of Seeing|p. 8]].");
    // In This Scene shows it, from the text.
    harness.get_by_label("[[Ways of Seeing]]");
    harness.state_mut().apply(BinderAction::FinishRename { id: berger, title: "Berger 1972".into() });
    harness.run_steps(2);
    let app = harness.state_mut();
    assert_eq!(app.tab_of(&doc).unwrap().doc.text, "The gaze [[Berger 1972|p. 8]].");
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

// ----- syncing with the web helper ---------------------------------------------------

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
        Action::Replace { id: open.clone(), was: "Mac words".into(), text: "web words".into() },
        Action::Replace { id: other.clone(), was: "closed words".into(), text: "closed, from the web".into() },
    ]);
    assert_eq!(current_text(&mut harness).as_deref(), Some("web words"));
    assert_eq!(fs::read_to_string(dir.0.join("manuscript/untitled.md")).unwrap(), "web words", "saved straight away");
    assert_eq!(fs::read_to_string(dir.0.join("manuscript/other.md")).unwrap(), "closed, from the web");
}

#[test]
fn typing_during_a_sync_keeps_the_macs_words() {
    let dir = TempDir::new("sync-typing");
    let mut harness = harness(project(&dir, "Mac words"));
    let open = harness.state().selected.clone().unwrap();
    // Typed after the sync read the scene.
    harness.state_mut().current().unwrap().doc.text = "Mac words, and more".into();

    finish(&mut harness, vec![Action::Replace { id: open.clone(), was: "Mac words".into(), text: "web words".into() }]);
    assert_eq!(current_text(&mut harness).as_deref(), Some("Mac words, and more"));
    let kept = crate::snapshots::list(&dir.0, &open);
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].text().unwrap(), "web words");
}

#[test]
fn clashes_are_snapshotted_and_reported() {
    let dir = TempDir::new("sync-clash");
    let mut harness = harness(project(&dir, "Mac words"));
    let open = harness.state().selected.clone().unwrap();
    finish(&mut harness, vec![Action::Snapshot { id: open.clone(), text: "web words".into(), name: "from the web (both changed)".into() }]);
    assert_eq!(crate::snapshots::list(&dir.0, &open)[0].name, "from the web (both changed)");
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
    assert!(node.meta.on_ipad, "it stays on the web");
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

    // Changed only on the web: taken as they are.
    finish(&mut harness, vec![Action::Replace { id: notes_id.clone(), was: String::new(), text: "Check the ferry times.".into() }]);
    assert_eq!(notes(&harness), "Check the ferry times.");
    assert!(harness.state().manifest_dirty);
    assert!(!dir.0.join("snapshots").exists() || fs::read_dir(dir.0.join("snapshots")).unwrap().count() == 0, "no snapshot for notes");

    // Changed on both: the Mac's, then the web editor's.
    finish(&mut harness, vec![Action::Snapshot { id: notes_id, text: "Rain, not snow?".into(), name: "x".into() }]);
    assert_eq!(notes(&harness), "Check the ferry times.\n\nFrom the web:\nRain, not snow?");
}

#[test]
fn the_inspector_marks_scenes_for_the_ipad() {
    let dir = TempDir::new("sync-mark");
    let mut harness = harness(project(&dir, ""));
    let id = harness.state().selected.clone().unwrap();
    harness.get_by_label("Available on the web").click();
    harness.run_steps(2);
    assert!(harness.state().project.as_ref().unwrap().on_ipad(&id));
    // Without a helper set up, nothing tries to sync, and it says so.
    assert!(!harness.state().sync_ready());
    harness.get_by_label_contains("Set up the web helper");
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
    // In the toolbar's More… menu.
    harness.get_by_label("More\u{2026}").click();
    harness.run_steps(2);
    harness.get_by_label("Help \u{2318}?").click();
    harness.run_steps(2);
    harness.get_by_label("Find in Project");
    harness.get_by_label("Release Notes").click();
    harness.run_steps(2);
    harness.get_by_label("1.12.1");
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

#[test]
fn reaching_the_days_goal_is_celebrated_once() {
    let dir = TempDir::new("celebrate");
    let mut harness = harness(project(&dir, "Already here."));
    harness.state_mut().project.as_mut().unwrap().manifest.goals.daily = Some(4);
    harness.run_steps(2);
    assert!(harness.state().celebration.is_none());

    harness.event(egui::Event::Text(" Then the snow came.".into()));
    harness.run_steps(2);
    assert!(harness.state().celebration.is_some(), "the goal's reached");
    harness.get_by_label("Today's goal reached");
    harness.get_by_label("4 words. Well done.");
    let today = crate::stats::key(crate::stats::today());
    assert_eq!(harness.state().stats.celebrated.as_deref(), Some(today.as_str()));

    // Not again today, even after more writing (or reopening: it's saved).
    harness.state_mut().celebration = None;
    harness.event(egui::Event::Text(" And more.".into()));
    harness.run_steps(2);
    assert!(harness.state().celebration.is_none());
    harness.state_mut().save_all();
    assert_eq!(crate::stats::Stats::load(&dir.0).celebrated.as_deref(), Some(today.as_str()));
}

#[test]
fn celebrating_can_be_turned_off() {
    let dir = TempDir::new("celebrate-off");
    let mut harness = harness(project(&dir, ""));
    harness.state_mut().settings.celebrate_goals = false;
    harness.state_mut().project.as_mut().unwrap().manifest.goals.daily = Some(1);
    harness.run_steps(2);
    harness.event(egui::Event::Text("Snow.".into()));
    harness.run_steps(2);
    assert!(harness.state().celebration.is_none());
    harness.get_by_label("Today: 1 / 1");
}

#[test]
fn statistics_show_a_year_of_days() {
    let dir = TempDir::new("calendar");
    let mut harness = harness(project(&dir, "One two three."));
    let day = |back: u64| crate::stats::today() - chrono::Days::new(back);
    for back in [1, 2, 3, 10] {
        harness.state_mut().stats.add(day(back), 300);
    }
    harness.state_mut().open_stats();
    harness.run_steps(3);
    harness.get_by_label("This year");
    harness.get_by_label("Longest run writing: 3 days");
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
fn remove_blank_lines_from_the_right_click_menu() {
    let dir = TempDir::new("blank-lines");
    let mut harness = harness(project(&dir, "One.\n\nTwo.\n   \nThree.\n\nFour."));
    harness.run_steps(2);
    let tab = harness.state_mut().current().unwrap().id;
    let ctx = harness.ctx.clone();
    sloth_core::editor::set_selection(&ctx, tab, 0..20);
    harness.get_by_label("Document text").click_secondary();
    harness.run_steps(2);
    harness.get_by_label("Remove Blank Lines \u{21e7}\u{2318}L").click();
    harness.run_steps(3);
    assert_eq!(current_text(&mut harness).as_deref(), Some("One.\nTwo.\nThree.\n\nFour."));
}

#[test]
fn remove_blank_lines_has_a_shortcut() {
    let dir = TempDir::new("blank-lines-key");
    let mut harness = harness(project(&dir, "One.\n\nTwo.\n\nThree."));
    harness.run_steps(2);
    harness.get_by_label("Document text").click();
    harness.run_steps(2);
    let tab = harness.state_mut().current().unwrap().id;
    sloth_core::editor::set_selection(&harness.ctx.clone(), tab, 0..9);
    harness.key_press_modifiers(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::L);
    harness.run_steps(3);
    assert_eq!(current_text(&mut harness).as_deref(), Some("One.\nTwo.\n\nThree."));
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

#[test]
fn statuses_are_edited_from_the_status_menu() {
    let dir = TempDir::new("edit-statuses-ui");
    let mut harness = harness(project(&dir, ""));
    let id = harness.state().selected.clone().unwrap();
    // The document is "First draft".
    harness.state_mut().project.as_mut().unwrap().set_meta(&id, crate::project::Meta { status: Some(2), ..Default::default() });
    harness.run_steps(2);

    // Status menu → Edit Statuses…
    harness.get_by_value("First draft").click();
    harness.run_steps(2);
    harness.get_by_label("Edit Statuses\u{2026}").click();
    harness.run_steps(2);
    harness.get_by_label("Add Status").click();
    harness.run_steps(2);
    let statuses = || harness.state().project.as_ref().unwrap().manifest.statuses.clone();
    assert_eq!(statuses().last().map(String::as_str), Some("New status"));

    // Move "First draft" up above "Ongoing": the document is still First draft.
    harness.get_all_by_label("\u{2191}").nth(2).unwrap().click();
    harness.run_steps(2);
    let app = harness.state();
    let project = app.project.as_ref().unwrap();
    assert_eq!(project.manifest.statuses[1..3], ["First draft", "Ongoing"]);
    let status = project.node(&id).unwrap().meta.status.unwrap();
    assert_eq!(project.manifest.statuses[status], "First draft");
    assert!(app.manifest_dirty);
}

// ----- focus mode --------------------------------------------------------------------

#[test]
fn focus_mode_is_just_the_text_until_escape() {
    let dir = TempDir::new("focus-mode");
    let mut harness = harness(project(&dir, "The ferry had gone."));
    harness.run_steps(2);
    harness.key_press_modifiers(egui::Modifiers::COMMAND | egui::Modifiers::ALT, egui::Key::F);
    harness.run_steps(3);
    assert!(harness.state().focus_mode);
    assert_eq!(harness.query_all_by_label("Inspector").count(), 0, "no toolbar");
    assert_eq!(harness.query_all_by_label("📚 Manuscript").count(), 0, "no binder");
    harness.get_by_label("Esc to leave Focus");
    harness.get_by_label("4 words");

    // Typing goes into the document.
    harness.event(egui::Event::Text(" Again.".into()));
    harness.run_steps(2);
    assert_eq!(current_text(&mut harness).as_deref(), Some("The ferry had gone. Again."));

    harness.key_press(egui::Key::Escape);
    harness.run_steps(2);
    assert!(!harness.state().focus_mode);
    harness.get_by_label("Inspector");
}

#[test]
fn focus_mode_needs_a_document() {
    let dir = TempDir::new("focus-folder");
    let (p, chapter, _) = chapter_project(&dir);
    let mut harness = harness(p);
    harness.state_mut().select(chapter);
    harness.run_steps(2);
    harness.get_by_label("Focus").click();
    harness.run_steps(2);
    assert!(!harness.state().focus_mode);
    assert!(harness.state().status.as_ref().is_some_and(|(m, _)| m.contains("Choose a document")));
}

// ----- canvas --------------------------------------------------------------------------

/// A project with a scene that refers to Alice, a second scene, and the
/// canvas open with the binder and inspector out of the way.
fn canvas_harness(dir: &TempDir) -> (Harness<'static, WriterApp>, String, String, String) {
    let mut p = project(dir, "Then [[Alice]] arrived.");
    let first = p.documents().into_iter().find(|n| n.title == "Untitled").unwrap().id.clone();
    let alice = p.add_document(&Place::Into("characters".into()), "Alice").unwrap();
    let second = p.add_document(&Place::Into("manuscript".into()), "Harbour").unwrap();
    let mut harness = harness(p);
    harness.state_mut().show_inspector = false;
    harness.state_mut().show_binder = false;
    harness.run_steps(2);
    harness.get_by_label("Canvas").click();
    harness.run_steps(3);
    (harness, first, alice, second)
}

/// Where a point on the canvas is on screen.
fn on_screen(harness: &Harness<'static, WriterApp>, at: [f32; 2]) -> egui::Pos2 {
    harness.state().canvas.as_ref().unwrap().to_screen * egui::pos2(at[0], at[1])
}

fn canvas_place(harness: &Harness<'static, WriterApp>, id: &str) -> [f32; 2] {
    harness.state().canvas.as_ref().unwrap().data().places[id]
}

#[test]
fn the_canvas_shows_every_document_strung_to_what_it_refers_to() {
    let dir = TempDir::new("canvas");
    let (mut harness, first, alice, _) = canvas_harness(&dir);
    assert!(harness.state().canvas_open);
    for title in ["Untitled", "Harbour", "Alice"] {
        harness.get_by_label(title);
    }
    let edges = harness.state().canvas.as_ref().unwrap().edges().to_vec();
    assert_eq!(edges.len(), 1);
    assert_eq!((edges[0].from.as_str(), edges[0].to.as_str(), edges[0].references), (first.as_str(), alice.as_str(), 1));

    // ⌥⌘C puts it away, and the editor's back.
    harness.key_press_modifiers(egui::Modifiers::COMMAND | egui::Modifiers::ALT, egui::Key::C);
    harness.run_steps(2);
    assert!(!harness.state().canvas_open);
    assert_eq!(harness.query_all_by_label("Harbour").count(), 0);
}

#[test]
fn the_canvas_can_show_each_chapter_as_one_card() {
    let dir = TempDir::new("canvas-depth");
    let mut p = project(&dir, "");
    let alice = p.add_document(&Place::Into("characters".into()), "Alice").unwrap();
    let chapter = p.add_folder(&Place::Into("manuscript".into()), "Chapter One").unwrap();
    for (title, text) in [("Walking", "With [[Alice]]."), ("The Long Way", "[[Alice]] again, [[Alice]].")] {
        let essay = p.add_document(&Place::Into(chapter.clone()), title).unwrap();
        fs::write(p.file_path(&essay).unwrap(), text).unwrap();
    }
    let mut harness = harness(p);
    harness.state_mut().show_inspector = false;
    harness.state_mut().show_binder = false;
    harness.run_steps(2);
    harness.get_by_label("Canvas").click();
    harness.run_steps(3);
    // All levels: each essay, outlined with its chapter's title.
    harness.get_by_label("Walking");
    harness.get_by_label("The Long Way");
    harness.get_by_label("Chapter One");

    harness.get_by_label("1").click();
    harness.run_steps(3);
    assert_eq!(harness.query_all_by_label("Walking").count(), 0, "inside the chapter's card");
    harness.get_by_label("Chapter One");
    let state = harness.state().canvas.as_ref().unwrap();
    assert_eq!(state.data().depth, Some(1));
    let drawn: Vec<_> = state.drawn_edges().iter().map(|e| (e.from.clone(), e.to.clone(), e.references)).collect();
    assert_eq!(drawn, [(chapter.clone(), alice.clone(), 3)], "both essays' strings, as one");
    // It sits where its first essay was.
    let first = harness.state().project.as_ref().unwrap().node(&chapter).unwrap().children[0].id.clone();
    assert_eq!(canvas_place(&harness, &chapter), canvas_place(&harness, &first));
}

#[test]
fn cards_move_freely_without_changing_the_binder() {
    let dir = TempDir::new("canvas-drag");
    let (mut harness, first, _, second) = canvas_harness(&dir);
    let order = |h: &Harness<'static, WriterApp>| h.state().project.as_ref().unwrap().manifest.binder[0].children.iter().map(|n| n.id.clone()).collect::<Vec<_>>();
    let before = order(&harness);
    let [x, y] = canvas_place(&harness, &first);

    let start = on_screen(&harness, [x + 100.0, y + 50.0]);
    drag_points(&mut harness, start, start + egui::vec2(0.0, 150.0));
    let [nx, ny] = canvas_place(&harness, &first);
    let scale = harness.state().canvas.as_ref().unwrap().to_screen.scaling;
    assert_eq!(nx, x);
    assert!((ny - (y + 150.0 / scale)).abs() <= 12.0, "moved down, snapped to the dots: {y} → {ny}");
    assert_eq!(ny % 24.0, 0.0);
    assert_eq!(order(&harness), before, "the binder's order is untouched");

    // Kept with the project.
    harness.state_mut().save_all();
    let saved = crate::canvas::Canvas::load(&dir.0);
    assert_eq!(saved.places[&first], [nx, ny]);
    assert!(saved.places.contains_key(&second));
}

#[test]
fn a_pin_dragged_to_another_card_ties_a_string() {
    let dir = TempDir::new("canvas-tie");
    let (mut harness, first, _, second) = canvas_harness(&dir);
    let [x, y] = canvas_place(&harness, &first);
    let [x2, y2] = canvas_place(&harness, &second);
    // The pin is at the middle of a card's top edge.
    let pin = on_screen(&harness, [x + 100.0, y]);
    let target = on_screen(&harness, [x2 + 100.0, y2 + 40.0]);
    drag_points(&mut harness, pin, target);
    let strings = harness.state().canvas.as_ref().unwrap().data().strings.clone();
    assert_eq!(strings.len(), 1);
    assert_eq!((strings[0].a.as_str(), strings[0].b.as_str()), (first.as_str(), second.as_str()));
    assert_eq!(canvas_place(&harness, &first), [x, y], "the card itself stayed put");

    // Tying it again unties it.
    drag_points(&mut harness, pin, target);
    assert!(harness.state().canvas.as_ref().unwrap().data().strings.is_empty());
}

#[test]
fn double_clicking_the_canvas_makes_a_note_that_can_become_a_document() {
    let dir = TempDir::new("canvas-note");
    let (mut harness, ..) = canvas_harness(&dir);
    // Long enough after clicking Canvas that this isn't a triple-click.
    harness.run_steps(30);
    // Well away from the cards: the bottom right of the canvas.
    let pos = egui::pos2(940.0, 620.0);
    harness.event(egui::Event::PointerMoved(pos));
    harness.run_steps(1);
    for _ in 0..2 {
        for pressed in [true, false] {
            harness.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
            harness.run_steps(1);
        }
    }
    harness.run_steps(2);
    let notes = harness.state().canvas.as_ref().unwrap().data().notes.clone();
    assert_eq!(notes.len(), 1, "one note");
    let note = notes[0].id.clone();

    // It's ready to type in.
    harness.event(egui::Event::Text("The ferry\nShe never sent the letter.".into()));
    harness.run_steps(2);
    assert_eq!(harness.state().canvas.as_ref().unwrap().data().notes[0].text, "The ferry\nShe never sent the letter.");
    let place = canvas_place(&harness, &note);

    harness.state_mut().note_to_document(&note);
    harness.run_steps(2);
    let app = harness.state();
    let project = app.project.as_ref().unwrap();
    let doc = project.node("notes").unwrap().children.last().unwrap();
    assert_eq!(doc.title, "The ferry");
    assert_eq!(fs::read_to_string(project.file_path(&doc.id).unwrap()).unwrap(), "She never sent the letter.");
    let canvas = app.canvas.as_ref().unwrap().data();
    assert!(canvas.notes.is_empty());
    assert_eq!(canvas.places[&doc.id], place, "it stays where the note was");
}

// ----- the mobile notepad ------------------------------------------------------------

#[test]
fn mobile_notes_is_made_in_notes_once_and_opens_beside() {
    let dir = TempDir::new("notepad");
    let mut harness = harness(project(&dir, "The ferry had gone."));
    harness.get_by_label("More…").click();
    harness.run_steps(2);
    harness.get_by_label("📝 Mobile Notes").click();
    harness.run_steps(3);
    let app = harness.state();
    let project = app.project.as_ref().unwrap();
    let pad = project.notepad().unwrap();
    assert_eq!(pad.title, "Mobile Notes");
    assert_eq!(project.role_of(&pad.id), Some(crate::project::Role::Notes));
    assert_eq!(app.split.as_deref(), Some(pad.id.as_str()), "open beside what's being written");
    assert!(app.split_open);
    let pad = pad.id.clone();
    harness.get_by_label("📝 Mobile Notes");

    // Asking again opens the same one.
    harness.state_mut().open_notepad();
    let project = harness.state().project.as_ref().unwrap();
    assert_eq!(project.notepad().unwrap().id, pad);
    assert_eq!(project.node("notes").unwrap().children.len(), 1);
}

#[test]
fn the_notepad_from_the_web_lands_in_its_document() {
    use crate::sync::{Action, NOTEPAD, Outcome};
    let dir = TempDir::new("notepad-sync");
    let mut harness = harness(project(&dir, ""));
    let root = dir.0.clone();

    // Started on the web, with none here: one is made, holding it.
    let outcome = Outcome { actions: vec![Action::NewNotepad { text: "Idea on the bus.".into() }], ..Default::default() };
    harness.state_mut().finish_sync(&root, Ok(outcome), 0.0);
    let pad = harness.state().project.as_ref().unwrap().notepad().unwrap().id.clone();
    let path = harness.state().project.as_ref().unwrap().file_path(&pad).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "Idea on the bus.");

    // Edits to `notepad` on the helper go to whichever document it is.
    let outcome = Outcome {
        actions: vec![Action::Replace { id: NOTEPAD.into(), was: "Idea on the bus.".into(), text: "Idea on the bus.\n\nAnother.".into() }],
        ..Default::default()
    };
    harness.state_mut().finish_sync(&root, Ok(outcome), 0.0);
    assert_eq!(fs::read_to_string(&path).unwrap(), "Idea on the bus.\n\nAnother.");
}

// ----- links between documents --------------------------------------------------------

/// A project whose first document links to a second, "The Letter".
fn linked_project(dir: &TempDir) -> (Project, String, String) {
    let mut p = project(dir, "She read [[the letter|again]] twice.");
    let first = p.documents().into_iter().find(|n| n.title == "Untitled").unwrap().id.clone();
    let letter = p.add_document(&Place::Into("manuscript".into()), "The Letter").unwrap();
    fs::write(p.file_path(&letter).unwrap(), "Dear Mara,").unwrap();
    (p, first, letter)
}

#[test]
fn a_documents_links_show_both_ways_in_the_inspector() {
    let dir = TempDir::new("doc-links");
    let (p, first, letter) = linked_project(&dir);
    let mut harness = harness(p);
    harness.run_steps(2);
    harness.get_by_label("Links");
    harness.get_by_label("[[The Letter]]").click();
    harness.run_steps(3);
    assert_eq!(harness.state().split.as_deref(), Some(letter.as_str()), "opened beside");

    // From the letter: where it's linked from, which goes there.
    harness.state_mut().select(letter.clone());
    harness.run_steps(3);
    harness.get_by_label("Linked from");
    harness.get_by_label("Untitled").click();
    harness.run_steps(3);
    assert_eq!(harness.state().selected.as_deref(), Some(first.as_str()));

    // And on the Canvas, a string between them.
    harness.state_mut().canvas_open = true;
    harness.run_steps(3);
    let edges = harness.state().canvas.as_ref().unwrap().edges().to_vec();
    assert!(edges.iter().any(|e| e.from == first && e.to == letter && e.references == 1));
}

#[test]
fn renaming_a_document_renames_the_links_to_it() {
    let dir = TempDir::new("doc-links-rename");
    let (p, first, letter) = linked_project(&dir);
    let mut harness = harness(p);
    harness.state_mut().apply(BinderAction::FinishRename { id: letter, title: "The Unsent Letter".into() });
    harness.run_steps(2);
    assert_eq!(current_text(&mut harness).as_deref(), Some("She read [[The Unsent Letter|again]] twice."));
    harness.state_mut().save_all();
    let path = harness.state().project.as_ref().unwrap().file_path(&first).unwrap();
    assert_eq!(fs::read_to_string(path).unwrap(), "She read [[The Unsent Letter|again]] twice.");
}

// ----- the sloth ----------------------------------------------------------------------

#[test]
fn empty_places_say_so_kindly() {
    let dir = TempDir::new("empty-states");
    let mut p = project(&dir, "");
    let folder = p.add_folder(&Place::Into("manuscript".into()), "Part Two").unwrap();
    let mut harness = harness(p);
    harness.state_mut().select(folder);
    harness.run_steps(3);
    harness.get_by_label("This folder is empty");

    harness.state_mut().select("trash".into());
    harness.run_steps(3);
    harness.get_by_label("The Trash is empty");

    harness.state_mut().selected = None;
    harness.run_steps(3);
    harness.get_by_label("Nothing open");

    // A document with no snapshots yet.
    let id = harness.state().project.as_ref().unwrap().documents()[0].id.clone();
    harness.state_mut().select(id);
    harness.run_steps(3);
    harness.get_by_label("None yet");
}

#[test]
fn the_start_screen_greets_and_tiles_show_progress() {
    let dir = TempDir::new("start-screen");
    let mut p = project(&dir, &"word ".repeat(250));
    p.manifest.goals.total = Some(1000);
    p.save().unwrap();
    let mut log = crate::stats::Stats::default();
    for back in 0..3u64 {
        log.add(crate::stats::today() - chrono::Days::new(back), 100);
    }
    log.save(&dir.0).unwrap();
    let mut harness = harness(p);
    harness.state_mut().close_project();
    harness.run_steps(3);

    let hour = chrono::Timelike::hour(&chrono::Local::now());
    harness.get_by_label(super::sloth_ui::greeting(hour).0);
    let facts = harness.state().tile_facts.get(&dir.0).cloned().flatten().unwrap();
    assert_eq!(facts, super::projects_ui::TileFacts::for_test(250, Some(1000), Some(crate::stats::today()), 3));
}

// ----- writing sprints -------------------------------------------------------------------

#[test]
fn a_sprint_counts_its_words_and_says_how_it_went() {
    let dir = TempDir::new("sprint");
    let mut harness = harness(project(&dir, "Already here."));
    harness.run_steps(2);
    harness.get_by_label("Sprint").click();
    harness.run_steps(2);
    harness.get_by_label("25 minutes").click();
    harness.run_steps(2);
    assert!(harness.state().sprint.is_some());
    harness.get_by_label_contains("\u{b7} 0 words");

    harness.event(egui::Event::Text(" Then the snow came.".into()));
    harness.run_steps(2);
    assert_eq!(harness.state().sprint_words(), 4);
    harness.get_by_label_contains("\u{b7} 4 words");

    // Time's up.
    let now = harness.ctx.input(|i| i.time);
    harness.state_mut().sprint.as_mut().unwrap().start = now - 25.0 * 60.0 - 1.0;
    harness.run_steps(2);
    assert!(harness.state().sprint.is_none());
    harness.get_by_label("Sprint done");
    harness.get_by_label("4 words in 25 minutes.");
    let kept = &harness.state().stats.sprints;
    assert_eq!((kept.len(), kept[0].minutes, kept[0].words), (1, 25, 4));
}

#[test]
fn a_sprint_counts_even_on_a_day_with_more_cut_than_written() {
    let dir = TempDir::new("sprint-negative");
    let mut harness = harness(project(&dir, "Already here."));
    harness.run_steps(2);
    harness.state_mut().stats.add(crate::stats::today(), -239);
    let now = harness.ctx.input(|i| i.time);
    harness.state_mut().start_sprint(now, 25);
    harness.run_steps(2);
    harness.event(egui::Event::Text(" Then the snow came.".into()));
    harness.run_steps(2);
    assert_eq!(harness.state().sprint_words(), 4);
    harness.get_by_label("Today: 0 words");
}

#[test]
fn a_sprint_stopped_early_isnt_kept() {
    let dir = TempDir::new("sprint-stop");
    let mut harness = harness(project(&dir, ""));
    let now = harness.ctx.input(|i| i.time);
    harness.state_mut().start_sprint(now, 10);
    harness.run_steps(2);
    harness.get_by_label_contains("\u{b7} 0 words").click();
    harness.run_steps(2);
    harness.get_by_label("Stop Sprint").click();
    harness.run_steps(2);
    assert!(harness.state().sprint.is_none());
    assert!(harness.state().stats.sprints.is_empty());
    assert!(harness.state().celebration.is_none());
}

// ----- overused words ---------------------------------------------------------------------

#[test]
fn repeated_words_are_marked_and_the_most_used_listed() {
    let dir = TempDir::new("repeats");
    let mut harness = harness(project(&dir, "The lantern swung. She suddenly saw the lantern go out. Lantern light."));
    harness.run_steps(2);
    assert!(harness.query_by_label("Repeats shown").is_none());

    harness.key_press_modifiers(egui::Modifiers::COMMAND | egui::Modifiers::ALT, egui::Key::R);
    harness.run_steps(2);
    harness.get_by_label("Repeats shown");
    let visuals = harness.ctx.global_style().visuals.clone();
    let marks = harness.state_mut().repetition_marks(&visuals);
    let text = current_text(&mut harness).unwrap();
    let marked: Vec<&str> = marks.iter().map(|m| &text[m.range.clone()]).collect();
    assert_eq!(marked, ["lantern", "suddenly", "lantern", "Lantern"]);
    assert!(marks[1].underline != egui::Color32::TRANSPARENT, "watched");

    // Statistics lists the most used; clicking one finds it in the project.
    harness.state_mut().open_stats();
    harness.run_steps(3);
    harness.get_by_label("lantern  3").click();
    harness.run_steps(3);
    assert_eq!(harness.state().find.query, "lantern");
    assert!(harness.state().show_results);

    // Click the reminder to put them away.
    harness.state_mut().stats_open = false;
    harness.run_steps(2);
    harness.get_by_label("Repeats shown").click();
    harness.run_steps(2);
    assert!(!harness.state().settings.show_repetition);
}

// ----- comments ------------------------------------------------------------------------------

#[test]
fn a_comment_on_a_passage_is_written_in_the_margin_and_resolved() {
    let dir = TempDir::new("comments");
    let mut harness = harness(project(&dir, "She left the lamp on."));
    harness.run_steps(2);
    let tab = harness.state_mut().current().unwrap().id;
    // "left the lamp", then ⌥⌘K.
    sloth_core::editor::set_selection(&harness.ctx, tab, 4..17);
    harness.key_press_modifiers(egui::Modifiers::COMMAND | egui::Modifiers::ALT, egui::Key::K);
    harness.run_steps(3);
    assert_eq!(current_text(&mut harness).as_deref(), Some("She {==left the lamp==}{>><<} on."));

    // Its card has the keyboard: type the comment.
    harness.event(egui::Event::Text("Why would she?".into()));
    harness.run_steps(3);
    assert_eq!(current_text(&mut harness).as_deref(), Some("She {==left the lamp==}{>>Why would she?<<} on."));
    harness.get_by_label("\u{201c}left the lamp\u{201d}");
    // Not words of the document.
    assert_eq!(crate::app::inspector::count_words(&current_text(&mut harness).unwrap()), 5);

    harness.get_by_label("Resolve").click();
    harness.run_steps(3);
    assert_eq!(current_text(&mut harness).as_deref(), Some("She left the lamp on."));
}

// ----- slothenings ------------------------------------------------------------------------

/// A chapter of three scenes with words in, and a subfolder with a fourth.
fn slothenings_project(dir: &TempDir) -> (Project, String, Vec<String>) {
    let (mut p, chapter, mut scenes) = chapter_project(dir);
    for (id, text) in scenes.iter().zip(["She arrived too late.", "The letter was never sent.", "The harbour froze."]) {
        fs::write(p.file_path(id).unwrap(), text).unwrap();
    }
    let part = p.add_folder(&Place::Into(chapter.clone()), "Later").unwrap();
    let last = p.add_document(&Place::Into(part), "Spring").unwrap();
    fs::write(p.file_path(&last).unwrap(), "By spring she had gone.").unwrap();
    scenes.push(last);
    (p, chapter, scenes)
}

#[test]
fn slothenings_shows_a_folders_documents_as_one_text() {
    let dir = TempDir::new("slothenings");
    let (p, chapter, scenes) = slothenings_project(&dir);
    let mut harness = harness(p);
    harness.state_mut().show_inspector = false;
    harness.state_mut().select(chapter);
    harness.run_steps(2);
    harness.get_by_label("Slothenings").click();
    harness.run_steps(3);
    // Each document under its title, the subfolder's too, in order.
    for title in ["Arrival", "Letter", "Harbour", "Spring"] {
        harness.get_all_by_label(title).last().unwrap();
    }
    let texts: Vec<String> = harness.get_all_by_label("Document text").map(|n| n.value().unwrap_or_default()).collect();
    assert_eq!(texts, ["She arrived too late.", "The letter was never sent.", "The harbour froze.", "By spring she had gone."]);

    // Typing goes into the document it's in, and saves to its own file.
    let letter = harness.state().open[&scenes[1]];
    let len = "The letter was never sent.".chars().count();
    harness.ctx.memory_mut(|m| m.request_focus(sloth_core::editor::editor_id(letter)));
    sloth_core::editor::set_selection(&harness.ctx, letter, len..len);
    harness.run_steps(2);
    harness.event(egui::Event::Text(" Not once.".into()));
    harness.run_steps(2);
    harness.state_mut().save_all();
    let path = harness.state().project.as_ref().unwrap().file_path(&scenes[1]).unwrap();
    assert_eq!(fs::read_to_string(path).unwrap(), "The letter was never sent. Not once.");

    // At its end, ↓ goes on into the next; at that one's start, ↑ comes back.
    harness.key_press(egui::Key::ArrowDown);
    harness.run_steps(3);
    let harbour = harness.state().open[&scenes[2]];
    assert!(harness.ctx.memory(|m| m.has_focus(sloth_core::editor::editor_id(harbour))), "into the next");
    assert_eq!(sloth_core::editor::selection(&harness.ctx, harbour), Some(0..0));
    harness.key_press(egui::Key::ArrowUp);
    harness.run_steps(3);
    assert!(harness.ctx.memory(|m| m.has_focus(sloth_core::editor::editor_id(letter))), "back again");
    let end = "The letter was never sent. Not once.".chars().count();
    assert_eq!(sloth_core::editor::selection(&harness.ctx, letter), Some(end..end));

    // A title opens its document on its own.
    harness.get_all_by_label("Harbour").last().unwrap().click();
    harness.run_steps(3);
    assert_eq!(harness.state().selected.as_deref(), Some(scenes[2].as_str()));
}

#[test]
fn command_1_shows_a_documents_folder_as_slothenings_where_it_was() {
    let dir = TempDir::new("slothenings-cmd1");
    let (p, chapter, scenes) = slothenings_project(&dir);
    let mut harness = harness(p);
    harness.state_mut().select(scenes[1].clone());
    harness.run_steps(2);
    let letter = harness.state().open[&scenes[1]];
    sloth_core::editor::set_selection(&harness.ctx, letter, 4..4);
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Num1);
    harness.run_steps(4);
    assert_eq!(harness.state().selected.as_deref(), Some(chapter.as_str()));
    assert_eq!(harness.state().folder_views.get(&chapter), Some(&super::views::FolderView::Slothenings));
    assert!(harness.ctx.memory(|m| m.has_focus(sloth_core::editor::editor_id(letter))));
    assert_eq!(sloth_core::editor::selection(&harness.ctx, letter), Some(4..4));
    // ⌘2: the corkboard.
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Num2);
    harness.run_steps(2);
    assert_eq!(harness.state().folder_views.get(&chapter), Some(&super::views::FolderView::Corkboard));
}

#[test]
fn the_split_pane_never_opens_a_second_editor_on_a_document() {
    let dir = TempDir::new("slothenings-split");
    let (p, chapter, scenes) = slothenings_project(&dir);
    let mut harness = harness(p);
    harness.state_mut().show_inspector = false;
    let editors = |h: &Harness<'static, WriterApp>| h.get_all_by_label("Document text").count();

    // The chapter as Slothenings, and one of its scenes in the split pane.
    harness.state_mut().folder_views.insert(chapter.clone(), super::views::FolderView::Slothenings);
    harness.state_mut().select(chapter.clone());
    harness.state_mut().open_in_split(scenes[1].clone());
    harness.run_steps(3);
    harness.get_by_label("This is open in the main editor.");
    assert_eq!(editors(&harness), 4, "one each, in the main area");

    // A scene open in the main editor, and its chapter as Slothenings beside it.
    harness.state_mut().select(scenes[0].clone());
    harness.state_mut().open_in_split(chapter);
    harness.run_steps(3);
    harness.get_by_label("Open in the main editor.");
    assert_eq!(editors(&harness), 4, "the main one, and the other three beside it");
}

// ----- Break Up and Combine ------------------------------------------------------------

const CHAPTER: &str = "Why these essays.\n\n# On Walking\n\nI walk.\n\n## A detour\n\nStill walking.\n\n# The Long Way\n\nIt was long.";

#[test]
fn break_up_makes_a_document_of_each_essay() {
    let dir = TempDir::new("break-up");
    let mut harness = harness(project(&dir, CHAPTER));
    let id = harness.state().selected.clone().unwrap();
    harness.state_mut().apply(BinderAction::BreakUp(id.clone()));
    harness.run_steps(2);
    // It starts at the top headings, and says what it'll make.
    harness.get_by_label("2 documents:");
    harness.get_by_label("📄 On Walking");
    harness.get_by_label("The 3 words before the first break stay in “Untitled” itself.");
    harness.get_by_label("Break Up").click();
    harness.run_steps(3);

    let app = harness.state();
    let project = app.project.as_ref().unwrap();
    let node = project.node(&id).unwrap();
    let children: Vec<_> = node.children.iter().map(|n| n.title.as_str()).collect();
    assert_eq!(children, ["On Walking", "The Long Way"]);
    let read = |id: &str| fs::read_to_string(project.file_path(id).unwrap()).unwrap();
    assert_eq!(read(&id), "Why these essays.");
    assert_eq!(read(&node.children[0].id), "I walk.\n\n## A detour\n\nStill walking.");
    assert_eq!(read(&node.children[1].id), "It was long.");
    let snapshots = crate::snapshots::list(&project.root, &id);
    assert_eq!(snapshots[0].text().unwrap(), CHAPTER, "the whole text is kept");
    // Moved, not written: the headings that became titles too.
    assert_eq!(harness.state().today_net_and_total().0, 0);
}

#[test]
fn breaking_up_with_nothing_before_the_first_heading_leaves_a_folder() {
    let dir = TempDir::new("break-up-folder");
    let mut harness = harness(project(&dir, "## One\n\na\n\n## Two\n\nb"));
    let id = harness.state().selected.clone().unwrap();
    harness.state_mut().apply(BinderAction::BreakUp(id.clone()));
    harness.run_steps(2);
    harness.get_by_label("“Untitled” becomes a folder holding them.");
    harness.get_by_label("Break Up").click();
    harness.run_steps(3);
    let project = harness.state().project.as_ref().unwrap();
    let node = project.node(&id).unwrap();
    assert!(!node.is_document());
    assert!(!dir.0.join("manuscript/untitled.md").exists());
    assert_eq!(node.children.len(), 2);
}

#[test]
fn break_up_can_go_by_scene_breaks_instead() {
    let dir = TempDir::new("break-up-scenes");
    let mut harness = harness(project(&dir, "First bit.\n\n* * *\n\nSecond bit."));
    let id = harness.state().selected.clone().unwrap();
    harness.state_mut().apply(BinderAction::BreakUp(id.clone()));
    harness.run_steps(2);
    // No headings: scene breaks are chosen already.
    harness.get_by_label("1 document:");
    harness.get_by_label("Break Up").click();
    harness.run_steps(3);
    let project = harness.state().project.as_ref().unwrap();
    assert_eq!(project.node(&id).unwrap().children[0].title, "Second bit.");
}

#[test]
fn combine_puts_a_folder_back_together_under_headings() {
    let dir = TempDir::new("combine");
    let mut harness = harness(project(&dir, CHAPTER));
    let id = harness.state().selected.clone().unwrap();
    harness.state_mut().apply(BinderAction::BreakUp(id.clone()));
    harness.run_steps(2);
    harness.get_by_label("Break Up").click();
    harness.run_steps(3);

    harness.state_mut().apply(BinderAction::Combine(id.clone()));
    harness.run_steps(2);
    harness.get_by_label("2 documents:");
    harness.get_by_label("Combine").click();
    harness.run_steps(3);

    assert_eq!(current_text(&mut harness).as_deref(), Some(CHAPTER), "back as it was");
    let project = harness.state().project.as_ref().unwrap();
    assert!(project.node(&id).unwrap().children.is_empty());
    let trash = project.manifest.binder.iter().find(|n| n.role == Some(crate::project::Role::Trash)).unwrap();
    assert_eq!(trash.children.len(), 2, "the essays are in the Trash");
    // Moved both ways: nothing written, nothing cut.
    assert_eq!(harness.state().today_net_and_total().0, 0);
}

#[test]
fn combine_makes_a_folder_a_document() {
    let dir = TempDir::new("combine-folder");
    let mut p = project(&dir, "");
    let folder = p.add_folder(&Place::Into("manuscript".into()), "Chapter").unwrap();
    for (title, text) in [("One", "a"), ("Two", "b")] {
        let doc = p.add_document(&Place::Into(folder.clone()), title).unwrap();
        fs::write(p.file_path(&doc).unwrap(), text).unwrap();
    }
    let mut harness = harness(p);
    harness.state_mut().apply(BinderAction::Combine(folder.clone()));
    harness.run_steps(2);
    harness.get_by_label("With a scene break between each").click();
    harness.run_steps(1);
    harness.get_by_label("Combine").click();
    harness.run_steps(3);
    let project = harness.state().project.as_ref().unwrap();
    assert!(project.node(&folder).unwrap().is_document());
    assert_eq!(fs::read_to_string(project.file_path(&folder).unwrap()).unwrap(), "a\n\n#\n\nb");
}

#[test]
fn combine_with_next_joins_two_documents() {
    let dir = TempDir::new("combine-next");
    let mut p = project(&dir, "The ferry had gone.");
    let second = p.add_document(&Place::Into("manuscript".into()), "Later").unwrap();
    fs::write(p.file_path(&second).unwrap(), "Next morning she walked.").unwrap();
    let mut harness = harness(p);
    let first = harness.state().selected.clone().unwrap();
    harness.state_mut().apply(BinderAction::CombineWithNext(first.clone()));
    harness.run_steps(2);
    harness.get_by_label("Combine").click();
    harness.run_steps(3);
    assert_eq!(current_text(&mut harness).as_deref(), Some("The ferry had gone.\n\nNext morning she walked."));
    assert_eq!(manuscript_titles(&harness), ["Untitled"]);
}

#[test]
fn a_pill_moves_at_depth_two() {
    let dir = TempDir::new("canvas-depth-drag");
    let (mut harness, _, alice, _) = canvas_harness(&dir);
    harness.get_by_label("2").click();
    harness.run_steps(3);
    let [x, y] = canvas_place(&harness, &alice);
    let start = on_screen(&harness, [x + 85.0, y + 30.0]);
    drag_points(&mut harness, start, start + egui::vec2(0.0, 120.0));
    harness.run_steps(3);
    assert_ne!(canvas_place(&harness, &alice), [x, y]);
}

// ----- web addresses -----------------------------------------------------------------------

#[test]
fn a_document_has_a_web_address_to_open() {
    use egui_kittest::kittest::NodeT;
    let dir = TempDir::new("link");
    let mut harness = harness(project(&dir, "An essay."));
    let id = harness.state().selected.clone().unwrap();
    // No address yet: nothing to open.
    assert!(harness.get_by_label("Open").accesskit_node().is_disabled());

    let field = harness.get_all_by_value("").find(|n| n.rect().top() > harness.get_by_label("Web address").rect().top() - 4.0).unwrap();
    field.focus();
    field.type_text("blog.example.com/walking");
    harness.run_steps(3);
    let meta = harness.state().project.as_ref().unwrap().node(&id).unwrap().meta.clone();
    assert_eq!(meta.url, "blog.example.com/walking");
    let open = harness.get_by_label("Open");
    assert!(!open.accesskit_node().is_disabled());
    open.click();
    harness.run_steps(2);
    if let Ok(dir) = std::env::var("SHOT_DIR") {
        harness.render().unwrap().save(format!("{dir}/link.png")).unwrap();
    }
}
