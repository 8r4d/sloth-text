//! Sloth Writer: a long-form writing app built on sloth-core.
//! A project is a folder of Markdown documents arranged in a binder; see
//! `project.rs` for the format and `app.rs` for the window.

// On Windows, a release build is a windowed app: no console window opens
// beside it. (Debug builds keep one, for what's printed while developing.)
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod backup;
mod binder;
mod breakup;
mod canvas;
mod cast;
mod character;
mod compile;
mod compile_pdf;
mod glossary;
mod grammar;
mod guide;
mod crash_log;
mod essay;
mod facts;
mod icons;
mod keychain;
mod lock;
mod merge;
mod places;
mod presence;
mod plot;
mod project;
mod references;
mod rewards;
mod repetition;
mod rtf;
mod scrivener;
mod snapshots;
mod story;
mod style;
mod strays;
mod stats;
mod sync;
mod version;

use eframe::egui;

fn main() -> eframe::Result {
    // Before anything else, so any crash is recorded (see crash_log.rs).
    crash_log::install();
    // Windows and Linux: words learned for spelling are kept here, and the
    // dictionaries load while the window opens (the Mac has its own).
    if let Some(data) = places::data_dir() {
        sloth_core::spell::set_personal_words(data.join("words.txt"));
    }
    sloth_core::spell::warm_up();
    // `sloth-writer path/to/project` opens that project.
    let open = std::env::args_os().nth(1).map(std::path::PathBuf::from);
    #[allow(unused_mut)] // Only changed on macOS.
    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Sloth Writer")
            // Linux matches the window to its menu entry and icon by this.
            .with_app_id("sloth-writer")
            .with_inner_size([1100.0, 750.0])
            .with_min_inner_size([500.0, 300.0])
            .with_icon(std::sync::Arc::new(
                eframe::icon_data::from_png_bytes(include_bytes!("../icons/icon-512.png"))
                    .expect("icon is a valid PNG"),
            )),
        ..Default::default()
    };
    #[cfg(target_os = "macos")]
    {
        // winit's default macOS menu makes ⌘Q end the app at once, before
        // the last edits are saved. Without it, ⌘Q reaches the app, which
        // closes the window the normal way (saving first).
        use winit::platform::macos::EventLoopBuilderExtMacOS;
        options.event_loop_builder = Some(Box::new(|builder| {
            builder.with_default_menu(false);
        }));
    }
    eframe::run_native(
        "Sloth Writer",
        options,
        Box::new(|cc| Ok(Box::new(app::WriterApp::new(cc, open)))),
    )
}
