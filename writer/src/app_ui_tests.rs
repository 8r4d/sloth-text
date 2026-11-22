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
    assert!(manifest().contains("status: Some(1)"), "{}", manifest());
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
    harness.get_by_label("Words in the manuscript");
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
    assert_eq!(project.node(&scenes[0]).unwrap().meta.status, Some(2));

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
