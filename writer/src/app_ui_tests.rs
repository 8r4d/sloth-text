//! UI tests: run Writer's real UI in memory with `egui_kittest` (no window)
//! against a project in a scratch folder.

use std::fs;
use std::path::PathBuf;

use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use super::WriterApp;
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
