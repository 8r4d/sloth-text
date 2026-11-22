//! Sloth Writer (working name): a long-form writing app built on sloth-core.
//!
//! For now this is only a placeholder window: an empty binder panel on the
//! left and Sloth's editor on the right, editing one untitled document. It
//! proves the workspace split works. The design doc's Phase 1 (projects and
//! the binder) starts here.

mod project;

use eframe::egui;
use sloth_core::editor::{self, EditorSettings, Highlights};
use sloth_core::tabs::Tabs;
use sloth_core::theme::ThemeChoice;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Sloth Writer")
            .with_inner_size([1000.0, 700.0])
            .with_min_inner_size([400.0, 300.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Sloth Writer",
        options,
        Box::new(|cc| {
            ThemeChoice::default().apply(&cc.egui_ctx);
            Ok(Box::new(WriterApp::default()))
        }),
    )
}

#[derive(Default)]
struct WriterApp {
    tabs: Tabs,
    editor: EditorSettings,
}

impl eframe::App for WriterApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::left(egui::Id::unique("binder"))
            .resizable(true)
            .default_size(220.0)
            .show(ui, |ui| {
                ui.add_space(8.0);
                ui.heading("Binder");
                ui.weak("Projects and scenes arrive in Phase 1.");
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(ui.visuals().extreme_bg_color))
            .show(ui, |ui| {
                let tab = self.tabs.active_mut();
                editor::show(
                    ui,
                    tab.id,
                    &mut tab.doc.text,
                    &self.editor,
                    Highlights::default(),
                    None,
                    None,
                );
            });
    }
}
