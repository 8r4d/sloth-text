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
        // The app gets symbols like ⌘ from macOS's system fonts, which tests
        // don't load; draw a placeholder box instead of failing.
        .allow_missing_glyphs()
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

// ----- 0.5: comfortable editing ---------------------------------------------

/// Focus the editor with the cursor at a character index.
fn put_cursor(harness: &mut Harness<'static, SlothApp>, at: usize) {
    let ctx = harness.ctx.clone();
    let tab = harness.state().tabs.active().id;
    harness.state().focus_editor(&ctx);
    editor::set_selection(&ctx, tab, at..at);
    harness.run_steps(2);
}

#[test]
fn enter_keeps_the_lines_indentation() {
    let dir = TempDir::new("autoindent");
    let file = dir.file("code.txt", "    item");
    let mut harness = harness(Settings::default(), vec![file]);
    put_cursor(&mut harness, 8);

    harness.key_press(egui::Key::Enter);
    harness.run_steps(2);
    harness.event(egui::Event::Text("next".into()));
    harness.run_steps(2);
    assert_eq!(active_text(&harness), "    item\n    next");
}

#[test]
fn tab_inserts_spaces_when_set_to() {
    let dir = TempDir::new("spaces");
    let file = dir.file("code.txt", "ab");
    let mut settings = Settings::default();
    settings.editor.indent_with_spaces = true;
    let mut harness = harness(settings, vec![file]);
    put_cursor(&mut harness, 2);

    harness.key_press(egui::Key::Tab);
    harness.run_steps(2);
    assert_eq!(active_text(&harness), "ab  ");
}

#[test]
fn indent_and_outdent_commands_shift_lines() {
    let dir = TempDir::new("indent");
    let file = dir.file("list.txt", "one\ntwo");
    let mut harness = harness(Settings::default(), vec![file]);
    let ctx = harness.ctx.clone();
    let tab = harness.state().tabs.active().id;
    editor::set_selection(&ctx, tab, 0..7);

    execute(&mut harness, Command::Indent);
    assert_eq!(active_text(&harness), "\tone\n\ttwo");
    execute(&mut harness, Command::Outdent);
    assert_eq!(active_text(&harness), "one\ntwo");
}

#[test]
fn status_bar_counts_words_and_selection() {
    let dir = TempDir::new("count");
    let file = dir.file("words.txt", "the quick brown fox");
    let mut harness = harness(Settings::default(), vec![file]);
    harness.get_by_label("4 words");

    put_cursor(&mut harness, 0);
    let ctx = harness.ctx.clone();
    let tab = harness.state().tabs.active().id;
    editor::set_selection(&ctx, tab, 4..9);
    harness.run_steps(2);
    harness.get_by_label("1 word, 5 characters selected");
}

#[test]
fn close_other_tabs_asks_about_unsaved_ones() {
    let dir = TempDir::new("others");
    let a = dir.file("a.txt", "A");
    let b = dir.file("b.txt", "B");
    let c = dir.file("c.txt", "C");
    let mut harness = harness(Settings::default(), vec![a, b, c]);

    // Edit a.txt, then go back to c.txt and close the others.
    let first = harness.state().tabs.iter().next().unwrap().id;
    harness.state_mut().tabs.get_mut(first).unwrap().doc.text.push('!');
    execute(&mut harness, Command::CloseOtherTabs);

    // b.txt is closed straight away; a.txt has edits, so we're asked.
    harness.get_by_label_contains("Save changes to \u{201c}a.txt");
    harness.get_by_label("Don't Save").click();
    harness.run_steps(3);

    let names: Vec<_> = harness.state().tabs.iter().map(|t| t.doc.display_name()).collect();
    assert_eq!(names, vec!["c.txt"]);
}

#[test]
fn switching_to_an_installed_font_and_tab_width() {
    let mut harness = harness(Settings::default(), vec![]);
    let families = crate::fonts::scan_for_tests();
    let menlo = families.iter().position(|f| f == "Menlo").expect("Menlo is installed");

    execute(&mut harness, Command::SetFont(menlo as u16));
    execute(&mut harness, Command::SetTabWidth(8));
    assert_eq!(harness.state().settings.editor.font.as_deref(), Some("Menlo"));
    assert_eq!(harness.state().settings.editor.tab_width, 8);
}

/// Not a check: renders the editor to a PNG so a person can look at it.
/// Run with `SLOTH_PREVIEW_DIR=/some/dir cargo test preview -- --ignored`.
#[test]
#[ignore]
fn preview_editor() {
    let Ok(out) = std::env::var("SLOTH_PREVIEW_DIR") else {
        return;
    };
    let dir = TempDir::new("preview");
    let file = dir.file(
        "sample.rs",
        "fn main() {\n    let list = vec![1, 2, 3];\n    for n in list {\n        println!(\"{n}\");\n    }\n}\n",
    );
    let settings = Settings {
        theme: crate::theme::ThemeChoice::Light,
        ..Settings::default()
    };
    let mut harness = harness(settings, vec![file]);
    harness.set_size(egui::vec2(640.0, 260.0));
    // Cursor just after the "{" that opens the for-loop body.
    put_cursor(&mut harness, 61);
    harness.run_steps(3);
    let image = harness.render().expect("render");
    image.save(Path::new(&out).join("editor.png")).unwrap();

    // Same again in a dark theme with an installed font and tabs.
    let menlo = crate::fonts::scan_for_tests().iter().position(|f| f == "Menlo").unwrap();
    execute(&mut harness, Command::SetTheme(crate::theme::ThemeChoice::Midnight));
    execute(&mut harness, Command::SetFont(menlo as u16));
    harness.state_mut().tabs.active_mut().doc.text.push_str("\tafter a tab\n");
    harness.run_steps(3);
    let image = harness.render().expect("render");
    image.save(Path::new(&out).join("editor-menlo.png")).unwrap();
}

#[test]
fn dragging_a_tab_reorders_tabs() {
    let dir = TempDir::new("drag");
    let a = dir.file("a.txt", "A");
    let b = dir.file("b.txt", "B");
    let c = dir.file("c.txt", "C");
    let mut harness = harness(Settings::default(), vec![a, b, c]);

    let start = harness.get_by_label("a.txt").rect().center();
    let end = harness.get_by_label("c.txt").rect().right_center() + egui::vec2(10.0, 0.0);
    let press = |pressed| egui::Event::PointerButton {
        pos: if pressed { start } else { end },
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    harness.event(egui::Event::PointerMoved(start));
    harness.event(press(true));
    harness.run_steps(2);
    for i in 1..=10 {
        let t = i as f32 / 10.0;
        harness.event(egui::Event::PointerMoved(start.lerp(end, t)));
        harness.run_steps(1);
    }
    harness.event(press(false));
    harness.run_steps(3);

    let names: Vec<_> = harness.state().tabs.iter().map(|t| t.doc.display_name()).collect();
    assert_eq!(names, vec!["b.txt", "c.txt", "a.txt"]);
}
