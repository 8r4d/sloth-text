//! Sloth Writer (working name): a long-form writing app built on sloth-core.
//! A project is a folder of Markdown documents arranged in a binder; see
//! `project.rs` for the format and `app.rs` for the window.

mod app;
mod binder;
mod project;

use eframe::egui;
use sloth_core::theme::ThemeChoice;

fn main() -> eframe::Result {
    // `sloth-writer path/to/project` opens that project.
    let open = std::env::args_os().nth(1).map(std::path::PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Sloth Writer")
            .with_inner_size([1100.0, 750.0])
            .with_min_inner_size([500.0, 300.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Sloth Writer",
        options,
        Box::new(|cc| {
            ThemeChoice::default().apply(&cc.egui_ctx);
            Ok(Box::new(app::WriterApp::new(cc, open)))
        }),
    )
}
