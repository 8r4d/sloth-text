mod app;
mod commands;
mod document;
mod editing;
mod editor;
mod file_types;
mod find;
mod fonts;
mod help;
#[cfg(target_os = "macos")]
mod macos_open;
#[cfg(target_os = "macos")]
mod macos_print;
#[cfg(target_os = "macos")]
mod native_menu;
mod recovery;
mod search_results;
mod settings;
mod syntax;
mod tabs;
mod text_format;
mod theme;
mod version;

use std::path::PathBuf;

fn main() -> eframe::Result {
    // `sloth a.txt b.md` opens each file in its own tab.
    let files: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();

    let mut options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Sloth")
            .with_inner_size([900.0, 650.0])
            .with_min_inner_size([300.0, 200.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };

    #[cfg(target_os = "macos")]
    {
        // winit's default macOS menu binds ⌘Q to an immediate terminate,
        // which would skip our "unsaved changes" prompt. Turn it off so
        // ⌘Q reaches the app like any other shortcut.
        use winit::platform::macos::EventLoopBuilderExtMacOS;
        options.event_loop_builder = Some(Box::new(|builder| {
            builder.with_default_menu(false);
        }));

        // Sloth has its own tab bar; stop macOS adding window-tab items
        // ("Show Tab Bar", "Show All Tabs", …) to the menus.
        if let Some(mtm) = objc2::MainThreadMarker::new() {
            objc2_app_kit::NSWindow::setAllowsAutomaticWindowTabbing(false, mtm);
        }

        // Receive files opened from Finder (double-click, Open With, Dock).
        macos_open::install();
    }

    eframe::run_native(
        "Sloth",
        options,
        Box::new(|cc| Ok(Box::new(app::SlothApp::new(cc, files)))),
    )
}
