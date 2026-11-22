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
