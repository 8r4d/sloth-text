//! UI tests: run the real app UI in memory with `egui_kittest` (no window,
//! no menu bar, nothing on screen) and check what it shows and does.

use std::fs;
use std::path::{Path, PathBuf};

use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use super::SlothApp;
use crate::commands::Command;
use crate::editor;
use crate::settings::Settings;

/// A scratch folder for one test's files, removed when dropped.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("sloth-ui-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn file(&self, name: &str, contents: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, contents).unwrap();
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn harness(settings: Settings, files: Vec<PathBuf>) -> Harness<'static, SlothApp> {
    let mut harness = Harness::builder()
        .with_size([900.0, 600.0])
        .build_eframe(move |cc| SlothApp::with_settings(&cc.egui_ctx, settings, files));
    // The app asks to repaint every couple of seconds (to notice outside
    // file changes), so step a fixed number of frames rather than `run()`.
    harness.run_steps(3);
    harness
}

fn execute(harness: &mut Harness<'static, SlothApp>, cmd: Command) {
    let ctx = harness.ctx.clone();
    harness.state_mut().execute(&ctx, cmd);
    harness.run_steps(3);
}

fn active_text(harness: &Harness<'static, SlothApp>) -> String {
    harness.state().tabs.active().doc.text.clone()
}

#[test]
fn shows_a_tab_per_file() {
    let dir = TempDir::new("tabs");
    let a = dir.file("a.txt", "first");
    let b = dir.file("b.md", "second");
    let harness = harness(Settings::default(), vec![a, b]);

    harness.get_by_label("a.txt");
    harness.get_by_label("b.md");
    assert_eq!(active_text(&harness), "second");
}

#[test]
fn find_highlights_matches_and_counts_them() {
    let dir = TempDir::new("find");
    let file = dir.file("cats.txt", "cat dog cat");
    let mut harness = harness(Settings::default(), vec![file]);

    execute(&mut harness, Command::Find);
    harness.event(egui::Event::Text("cat".into())); // typed into the focused find field
    harness.run_steps(3);
    harness.get_by_label("1 of 2");

    execute(&mut harness, Command::FindNext);
    harness.get_by_label("2 of 2");
}

#[test]
fn replace_all_replaces_every_match() {
    let dir = TempDir::new("replace");
    let file = dir.file("cats.txt", "cat dog cat");
    let mut harness = harness(Settings::default(), vec![file]);

    execute(&mut harness, Command::FindAndReplace);
    harness.state_mut().find.query = "cat".into();
    harness.state_mut().find.replacement = "bird".into();
    harness.run_steps(3);
    harness.get_by_label("Replace All").click();
    harness.run_steps(3);

    assert_eq!(active_text(&harness), "bird dog bird");
    harness.get_by_label("Replaced 2 occurrences");
}

#[test]
fn closing_an_unsaved_tab_asks_first() {
    let dir = TempDir::new("close");
    let file = dir.file("notes.txt", "hello");
    let mut harness = harness(Settings::default(), vec![file]);

    harness.state_mut().tabs.active_mut().doc.text.push_str(" world");
    execute(&mut harness, Command::CloseTab);
    harness.get_by_label_contains("Save changes to");

    harness.get_by_label("Don't Save").click();
    harness.run_steps(3);
    // The only tab was closed, leaving a fresh untitled one.
    assert!(harness.state().tabs.active().doc.path.is_none());
}

#[test]
fn reloads_clean_files_changed_on_disk() {
    let dir = TempDir::new("reload");
    let file = dir.file("live.txt", "version 1");
    let mut harness = harness(Settings::default(), vec![file.clone()]);

    fs::write(&file, "version two").unwrap();
    harness.state_mut().check_disk();
    harness.run_steps(3);
    assert_eq!(active_text(&harness), "version two");
}

#[test]
fn asks_before_overwriting_edits_when_file_changes_on_disk() {
    let dir = TempDir::new("conflict");
    let file = dir.file("live.txt", "version 1");
    let mut harness = harness(Settings::default(), vec![file.clone()]);

    harness.state_mut().tabs.active_mut().doc.text.push_str(" + my edit");
    fs::write(&file, "version two").unwrap();
    harness.state_mut().check_disk();
    harness.run_steps(3);

    // Edits are kept until the user chooses.
    assert_eq!(active_text(&harness), "version 1 + my edit");
    harness.get_by_label("Reload from Disk").click();
    harness.run_steps(3);
    assert_eq!(active_text(&harness), "version two");
}

#[test]
fn go_to_line_moves_the_cursor() {
    let dir = TempDir::new("goto");
    let file = dir.file("lines.txt", "one\ntwo\nthree");
    let mut harness = harness(Settings::default(), vec![file]);

    execute(&mut harness, Command::GoToLine);
    harness.event(egui::Event::Text("3:2".into()));
    harness.run_steps(2);
    harness.key_press(egui::Key::Enter);
    harness.run_steps(3);

    let tab = harness.state().tabs.active().id;
    // Line 3, column 2 is just after the "t" of "three".
    assert_eq!(editor::selection(&harness.ctx, tab), Some(9..9));
    assert!(harness.state().go_to_line.is_none());
}

#[test]
fn restores_last_session_and_remembers_recent_files() {
    let dir = TempDir::new("session");
    let a = dir.file("a.txt", "A");
    let b = dir.file("b.txt", "B");

    let mut settings = Settings::default();
    settings.session.files = vec![a.clone(), dir.0.join("deleted.txt"), b.clone()];
    settings.session.active = 0;
    let harness = harness(settings, vec![]);

    // The missing file is skipped, and the previously active tab is active.
    let paths: Vec<_> = harness.state().tabs.iter().filter_map(|t| t.doc.path.clone()).collect();
    assert_eq!(paths, vec![a.clone(), b.clone()]);
    assert_eq!(active_text(&harness), "A");
    assert_eq!(harness.state().session().files, vec![a.clone(), b.clone()]);

    // Opened files go to the top of Open Recent.
    let recent: Vec<&Path> = harness.state().settings.recent_files.iter().map(|p| p.as_path()).collect();
    assert_eq!(recent, vec![b.as_path(), a.as_path()]);
}

// ----- 0.4: file formats and recovery ----------------------------------------

#[test]
fn keeps_a_windows_file_in_its_encoding_and_line_endings() {
    let dir = TempDir::new("win1252");
    let path = dir.0.join("old.txt");
    fs::write(&path, b"caf\xE9 cr\xE8me \xE0 la plage\r\n").unwrap();
    let mut harness = harness(Settings::default(), vec![path.clone()]);

    harness.get_by_label("Windows Latin 1 · CRLF");
    harness.state_mut().tabs.active_mut().doc.text.push_str("d\u{e9}j\u{e0} vu\n");
    execute(&mut harness, Command::Save);
    assert_eq!(
        fs::read(&path).unwrap(),
        b"caf\xE9 cr\xE8me \xE0 la plage\r\nd\xE9j\xE0 vu\r\n"
    );
}

#[test]
fn offers_utf8_when_characters_dont_fit_the_encoding() {
    let dir = TempDir::new("unmappable");
    let path = dir.0.join("old.txt");
    fs::write(&path, b"caf\xE9 cr\xE8me \xE0 la plage").unwrap();
    let mut harness = harness(Settings::default(), vec![path.clone()]);

    harness.state_mut().tabs.active_mut().doc.text.push_str(" \u{142}");
    execute(&mut harness, Command::Save);
    harness.get_by_label_contains("can't be stored in windows-1252");
    harness.get_by_label("Save as UTF-8").click();
    harness.run_steps(3);

    assert_eq!(fs::read_to_string(&path).unwrap(), "café crème à la plage \u{142}");
    harness.get_by_label("UTF-8 · LF");
}

#[test]
fn reopen_with_encoding_rereads_the_file() {
    let dir = TempDir::new("reopen");
    let path = dir.file("utf8.txt", "é");
    let mut harness = harness(Settings::default(), vec![path]);

    execute(
        &mut harness,
        Command::ReopenWithEncoding(crate::text_format::TextEncoding::Windows1252),
    );
    assert_eq!(active_text(&harness), "Ã©");
    assert!(!harness.state().tabs.active().doc.is_dirty());
}

#[test]
fn restores_documents_after_a_crash() {
    let dir = TempDir::new("crash");
    let recovery_dir = dir.0.join("recovery");
    let original = dir.file("notes.txt", "saved text");

    // A previous session (no longer running) left recovery copies behind.
    let crashed = recovery_dir.join("999999999");
    fs::create_dir_all(&crashed).unwrap();
    let records = [
        crate::recovery::Record {
            path: Some(original.clone()),
            text: "saved text + unsaved edit".into(),
            encoding: "UTF-8".into(),
            bom: false,
            crlf: false,
        },
        crate::recovery::Record {
            path: None,
            text: "an untitled draft".into(),
            encoding: "UTF-8".into(),
            bom: false,
            crlf: false,
        },
    ];
    for (i, r) in records.iter().enumerate() {
        fs::write(crashed.join(format!("tab-{i}.ron")), ron::to_string(r).unwrap()).unwrap();
    }

    let mut harness = harness(Settings::default(), vec![]);
    harness.state_mut().enable_recovery(recovery_dir.clone());
    harness.run_steps(3);
    harness.get_by_label("Restore unsaved documents?");
    harness.get_by_label_contains("notes.txt");
    harness.get_by_label("Restore").click();
    harness.run_steps(3);

    let texts: Vec<_> = harness.state().tabs.iter().map(|t| t.doc.text.clone()).collect();
    assert_eq!(texts, vec!["saved text + unsaved edit", "an untitled draft"]);
    assert!(harness.state().tabs.iter().all(|t| t.doc.is_dirty()));
    // The old session's folder is gone; the restored edits are protected
    // again under this session.
    assert!(!crashed.exists());
    let own = recovery_dir.join(std::process::id().to_string());
    assert_eq!(fs::read_dir(own).unwrap().count(), 2);
}

#[test]
fn writes_recovery_copies_only_for_unsaved_documents() {
    let dir = TempDir::new("autosave");
    let recovery_dir = dir.0.join("recovery");
    let a = dir.file("a.txt", "A");
    let b = dir.file("b.txt", "B");
    let mut harness = harness(Settings::default(), vec![a, b]);
    harness.state_mut().enable_recovery(recovery_dir.clone());

    harness.state_mut().tabs.active_mut().doc.text.push_str(" edited");
    harness.state_mut().save_recovery_copies();
    let own = recovery_dir.join(std::process::id().to_string());
    assert_eq!(fs::read_dir(&own).unwrap().count(), 1);

    // Saving the document removes its copy.
    execute(&mut harness, Command::Save);
    harness.state_mut().save_recovery_copies();
    assert_eq!(fs::read_dir(&own).unwrap().count(), 0);
}
