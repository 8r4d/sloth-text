mod app;
mod commands;
mod document;
mod editor;
mod theme;
#[cfg(target_os = "macos")]
mod native_menu;

use std::path::PathBuf;

fn main() -> eframe::Result {
    // `sloth path/to/file.txt` opens that file.
    let initial_file = std::env::args_os().nth(1).map(PathBuf::from);

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
    }

    eframe::run_native(
        "Sloth",
        options,
        Box::new(|cc| Ok(Box::new(app::SlothApp::new(cc, initial_file)))),
    )
}
