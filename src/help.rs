//! Pages shown in the Help window.

use eframe::egui::{self, RichText};

use crate::commands::{self, Command, MenuEntry};
use crate::version;

/// Shortcut reference, generated from the menus so it never goes stale.
pub fn shortcuts_page(ui: &mut egui::Ui) {
    fn collect(entries: &[MenuEntry], out: &mut Vec<Command>) {
        for entry in entries {
            match entry {
                MenuEntry::Item(cmd) if cmd.shortcut().is_some() => out.push(*cmd),
                MenuEntry::Submenu(_, children) => collect(children, out),
                _ => {}
            }
        }
    }
    let ctx = ui.ctx().clone();
    // One grid for every menu so the shortcut column lines up.
    egui::Grid::new("shortcuts")
        .num_columns(2)
        .spacing([40.0, 4.0])
        .show(ui, |ui| {
            for (title, entries) in commands::MENUS {
                let mut cmds = Vec::new();
                collect(entries, &mut cmds);
                if cmds.is_empty() {
                    continue;
                }
                ui.add_space(6.0);
                ui.end_row();
                ui.strong(*title);
                ui.end_row();
                for cmd in cmds {
                    ui.label(cmd.label().trim_end_matches('…'));
                    let sc = cmd.shortcut().expect("filtered above");
                    ui.monospace(ctx.format_shortcut(&sc));
                    ui.end_row();
                }
            }
        });
    ui.add_space(10.0);
    ui.strong("Tips");
    for tip in [
        "Drag files onto the window to open them.",
        "Middle-click a tab to close it.",
        "In the find bar, Enter finds the next match and Shift-Enter the previous one.",
        "Theme, line numbers, word wrap and text size are remembered between launches.",
    ] {
        ui.label(format!("•  {tip}"));
    }
}

/// CHANGELOG.md, rendered simply.
pub fn release_notes_page(ui: &mut egui::Ui) {
    for line in version::CHANGELOG.lines() {
        if line.starts_with("# ") {
            continue; // the document title
        } else if let Some(heading) = line.strip_prefix("## ") {
            ui.add_space(8.0);
            ui.heading(heading);
        } else if let Some(item) = line.strip_prefix("- ") {
            ui.label(format!("•  {item}"));
        } else if !line.trim().is_empty() {
            ui.label(RichText::new(line).weak());
        }
    }
}
