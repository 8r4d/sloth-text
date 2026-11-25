//! Get Project from the Helper…: a project synced whole from another
//! computer (see sync.rs), brought to this one. The helper's projects are
//! listed; the one chosen is fetched into a new folder, named after it, in
//! a folder you pick, and opened. The network is only ever used off the
//! main thread.

use std::sync::mpsc;

use eframe::egui::{self, RichText};

use super::WriterApp;
use crate::project::Project;
use crate::sync::{self, Http, RemoteProject, SyncError, SyncState};

/// Where the window's at.
pub(super) enum GetProject {
    Listing(mpsc::Receiver<Result<Vec<RemoteProject>, SyncError>>),
    List(Vec<RemoteProject>),
    Fetching(String, mpsc::Receiver<Result<(Project, SyncState), String>>),
    Failed(String),
}

impl WriterApp {
    pub(super) fn start_get_project(&mut self) {
        if self.settings.helper_url.trim().is_empty() || self.helper_token.trim().is_empty() {
            self.get_project = Some(GetProject::Failed("Set up the web helper in App Settings first: its address, and a token from its Account page.".into()));
            return;
        }
        let (url, token) = (self.settings.helper_url.clone(), self.helper_token.clone());
        let (send, receive) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = send.send(Http::new(&url, &token).projects());
        });
        self.get_project = Some(GetProject::Listing(receive));
    }

    pub(super) fn get_project_window(&mut self, ctx: &egui::Context) {
        // Results that have come in.
        match &self.get_project {
            Some(GetProject::Listing(r)) => {
                if let Ok(result) = r.try_recv() {
                    self.get_project = Some(match result {
                        Ok(list) => GetProject::List(list),
                        Err(e) => GetProject::Failed(format!("Couldn't get the list: {e}")),
                    });
                }
            }
            Some(GetProject::Fetching(_, r)) => {
                if let Ok(result) = r.try_recv() {
                    match result {
                        Ok((project, state)) => {
                            let root = project.root.clone();
                            if let Err(e) = state.save(&root, &self.settings.device_id) {
                                self.error = Some(format!("Couldn't save the sync record: {e}"));
                            }
                            self.get_project = None;
                            self.open_project_anyway(&root);
                            return;
                        }
                        Err(e) => self.get_project = Some(GetProject::Failed(e)),
                    }
                }
            }
            _ => {}
        }
        let Some(state) = &self.get_project else { return };
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
        let mut chosen = None;
        let mut close = false;
        egui::Modal::new(egui::Id::unique("get-project")).show(ctx, |ui| {
            ui.set_width(420.0);
            ui.heading("Get a Project from the Helper");
            ui.add_space(6.0);
            match state {
                GetProject::Listing(_) => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Asking the helper\u{2026}");
                    });
                }
                GetProject::List(list) if list.is_empty() => {
                    ui.label("There are no projects on the helper yet.");
                }
                GetProject::List(list) => {
                    ui.weak("A project synced whole from another computer (Project Settings \u{2192} Web helper) comes with everything. Choose one, then where to put it.");
                    ui.add_space(6.0);
                    egui::ScrollArea::vertical().max_height(280.0).show(ui, |ui| {
                        for p in list {
                            let what = if p.documents == 1 { "document" } else { "documents" };
                            let row = ui.add(egui::Button::new(format!("{}   ", p.title)).right_text(RichText::new(format!("{} {what}", p.documents)).weak()).min_size(egui::vec2(400.0, 0.0)));
                            if row.clicked() {
                                chosen = Some(p.clone());
                            }
                        }
                    });
                }
                GetProject::Fetching(title, _) => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(format!("Bringing \u{201c}{title}\u{201d} over\u{2026}"));
                    });
                }
                GetProject::Failed(message) => {
                    ui.label(message);
                }
            }
            ui.add_space(8.0);
            if !matches!(state, GetProject::Fetching(..)) && ui.button("Cancel").clicked() {
                close = true;
            }
        });
        if close {
            self.get_project = None;
        }
        if let Some(remote) = chosen
            && let Some(parent) = rfd::FileDialog::new().set_title("Put the Project In…").pick_folder()
        {
            let root = parent.join(crate::project::folder_name(&remote.title));
            let (url, token) = (self.settings.helper_url.clone(), self.helper_token.clone());
            let (send, receive) = mpsc::channel();
            let id = remote.id.clone();
            std::thread::spawn(move || {
                let mut helper = Http::new(&url, &token);
                let result = match sync::fetch(&mut helper, &id) {
                    Ok(Some((binder, texts, state, tallies))) => Project::from_shared(&root, binder.manifest, &texts)
                        .and_then(|p| {
                            // The other computers' words, for streaks and goals here.
                            let mut stats = crate::stats::Stats::load(&p.root);
                            for (device, tally) in tallies {
                                stats.set_other(&device, tally);
                            }
                            stats.save(&p.root).map(|()| p)
                        })
                        .map(|p| (p, state))
                        .map_err(|e| format!("Couldn't write the project to {}: {e}", root.display())),
                    Ok(None) => Err("That project isn't synced whole yet: on the computer that has it, turn on Project Settings \u{2192} Web helper \u{2192} Sync the whole project, and sync.".into()),
                    Err(e) => Err(format!("Couldn't bring it over: {e}")),
                };
                let _ = send.send(result);
            });
            self.get_project = Some(GetProject::Fetching(remote.title.clone(), receive));
        }
    }
}
