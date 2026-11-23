//! The native macOS menu bar, built from `commands::MENUS`.
//!
//! Selecting a menu item (or pressing its shortcut, which macOS routes through
//! the menu) sends the matching `Command` down a channel that the app drains
//! every frame.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};

use eframe::egui::{self, Key, KeyboardShortcut};
use muda::accelerator::{Accelerator, Code, Modifiers};
use muda::{
    CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu,
};

use crate::commands::{self, Command, MenuEntry};
use crate::settings;

pub struct NativeMenu {
    /// Must stay alive for the menu to keep working.
    _menu: Menu,
    events: Receiver<Command>,
    items: Items,
    /// The recent files currently shown in File → Open Recent.
    shown_recent: Option<Vec<PathBuf>>,
    /// How many installed fonts View → Font lists (filled in once scanned).
    shown_fonts: Option<usize>,
}

/// Handles to menu items we update while the app runs.
#[derive(Default)]
struct Items {
    checkable: HashMap<Command, CheckMenuItem>,
    plain: HashMap<Command, MenuItem>,
    recent: Option<Submenu>,
    fonts: Option<Submenu>,
}

impl NativeMenu {
    pub fn install(ctx: &egui::Context) -> Self {
        let menu = Menu::new();
        let mut items = Items::default();

        // Application menu (titled with the app name by macOS).
        let app_menu = Submenu::new("Sloth", true);
        append(
            &app_menu,
            &[
                &command_item(Command::About),
                &PredefinedMenuItem::separator(),
                &command_item(Command::Settings),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::services(None),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::hide(None),
                &PredefinedMenuItem::hide_others(None),
                &PredefinedMenuItem::show_all(None),
                &PredefinedMenuItem::separator(),
                &command_item(Command::Quit),
            ],
        );
        menu.append(&app_menu).ok();

        let mut window_menu = None;
        let mut help_menu = None;
        for (title, entries) in commands::MENUS {
            let submenu = Submenu::new(*title, true);
            match *title {
                "Window" => {
                    // Standard window items first, then ours, as in other Mac apps.
                    append(
                        &submenu,
                        &[
                            &PredefinedMenuItem::minimize(None),
                            &PredefinedMenuItem::maximize(None),
                            &PredefinedMenuItem::fullscreen(None),
                            &PredefinedMenuItem::separator(),
                        ],
                    );
                    fill(&submenu, entries, &mut items);
                    append(
                        &submenu,
                        &[
                            &PredefinedMenuItem::separator(),
                            &PredefinedMenuItem::bring_all_to_front(None),
                        ],
                    );
                    window_menu = Some(submenu.clone());
                }
                "Help" => {
                    fill(&submenu, entries, &mut items);
                    help_menu = Some(submenu.clone());
                }
                _ => fill(&submenu, entries, &mut items),
            }
            menu.append(&submenu).ok();
        }

        menu.init_for_nsapp();
        if let Some(m) = window_menu {
            m.set_as_windows_menu_for_nsapp();
        }
        if let Some(m) = help_menu {
            // Gives the Help menu its macOS search field.
            m.set_as_help_menu_for_nsapp();
        }

        let (tx, rx) = channel();
        let ctx = ctx.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            if let Some(cmd) = Command::from_id(event.id().as_ref()) {
                let _ = tx.send(cmd);
                // Wake egui so the command runs right away.
                ctx.request_repaint();
            }
        }));

        Self {
            _menu: menu,
            events: rx,
            items,
            shown_recent: None,
            shown_fonts: None,
        }
    }

    /// Commands chosen from the menu since the last call.
    pub fn drain(&self) -> Vec<Command> {
        self.events.try_iter().collect()
    }

    /// Keep check marks in sync with app state.
    pub fn set_checked(&self, cmd: Command, checked: bool) {
        if let Some(item) = self.items.checkable.get(&cmd)
            && item.is_checked() != checked
        {
            item.set_checked(checked);
        }
    }

    /// Grey out (or re-enable) a menu item.
    pub fn set_enabled(&self, cmd: Command, enabled: bool) {
        if let Some(item) = self.items.plain.get(&cmd)
            && item.is_enabled() != enabled
        {
            item.set_enabled(enabled);
        }
    }

    /// Fill in View → Font once the installed fonts are known.
    pub fn set_fonts(&mut self, families: &[String]) {
        if self.shown_fonts == Some(families.len()) {
            return;
        }
        let Some(submenu) = self.items.fonts.clone() else {
            return;
        };
        while submenu.remove_at(0).is_some() {}
        self.items.checkable.retain(|cmd, _| {
            !matches!(cmd, Command::DefaultFont | Command::SetFont(_))
        });

        let mut entries = vec![MenuEntry::Item(Command::DefaultFont)];
        if !families.is_empty() {
            entries.push(MenuEntry::Separator);
            entries.extend((0..families.len() as u16).map(|i| MenuEntry::Item(Command::SetFont(i))));
        }
        fill(&submenu, &entries, &mut self.items);
        self.shown_fonts = Some(families.len());
    }

    /// Rebuild File → Open Recent if the list has changed.
    pub fn set_recent_files(&mut self, paths: &[PathBuf]) {
        if self.shown_recent.as_deref() == Some(paths) {
            return;
        }
        let Some(submenu) = &self.items.recent else {
            return;
        };
        while submenu.remove_at(0).is_some() {}

        let labels = settings::recent_labels(paths);
        for (i, label) in labels.iter().enumerate() {
            let item = MenuItem::with_id(Command::OpenRecent(i as u8).id(), label, true, None);
            submenu.append(&item).ok();
        }
        if !paths.is_empty() {
            submenu.append(&PredefinedMenuItem::separator()).ok();
        }
        let clear = command_item(Command::ClearRecent);
        clear.set_enabled(!paths.is_empty());
        submenu.append(&clear).ok();

        dock_recent(paths);
        self.shown_recent = Some(paths.to_vec());
    }
}

/// The same files in the Dock icon's menu. macOS keeps that list for the
/// app's document controller, so it's replaced with ours: cleared, then
/// filled oldest first so the newest ends up on top. Choosing one opens it
/// the way Finder does (see macos_open.rs).
fn dock_recent(paths: &[PathBuf]) {
    use objc2_app_kit::NSDocumentController;
    use objc2_foundation::{NSString, NSURL};

    let Some(mtm) = objc2::MainThreadMarker::new() else { return };
    let controller = NSDocumentController::sharedDocumentController(mtm);
    // SAFETY: called on the main thread with no sender, as a menu item would.
    unsafe { controller.clearRecentDocuments(None) };
    for path in paths.iter().rev() {
        let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
        controller.noteNewRecentDocumentURL(&url);
    }
}

/// Add `entries` to `submenu`, recursing into nested submenus.
fn fill(submenu: &Submenu, entries: &[MenuEntry], items: &mut Items) {
    // About and Quit live in the app menu on macOS; drop them (and any
    // separator left dangling at the end) from the other menus.
    let mut entries: Vec<_> = entries
        .iter()
        .filter(|e| !matches!(e, MenuEntry::Item(cmd) if cmd.in_app_menu_on_macos()))
        .collect();
    while matches!(entries.last(), Some(MenuEntry::Separator)) {
        entries.pop();
    }

    for entry in entries {
        match entry {
            MenuEntry::Separator => {
                submenu.append(&PredefinedMenuItem::separator()).ok();
            }
            MenuEntry::Submenu(title, children) => {
                let child = Submenu::new(*title, true);
                fill(&child, children, items);
                submenu.append(&child).ok();
            }
            MenuEntry::Generated(list) => {
                let generated: Vec<MenuEntry> = list().into_iter().map(MenuEntry::Item).collect();
                fill(submenu, &generated, items);
            }
            MenuEntry::Fonts => {
                // `submenu` is View → Font itself; filled in by `set_fonts`.
                items.fonts = Some(submenu.clone());
            }
            MenuEntry::RecentFiles => {
                // Filled in by `set_recent_files`.
                let child = Submenu::new("Open Recent", true);
                submenu.append(&child).ok();
                items.recent = Some(child);
            }
            MenuEntry::Item(cmd) if cmd.is_checkable() => {
                let check = CheckMenuItem::with_id(
                    cmd.id(),
                    cmd.label(),
                    true,
                    false,
                    cmd.shortcut().and_then(to_accelerator),
                );
                submenu.append(&check).ok();
                items.checkable.insert(*cmd, check);
            }
            MenuEntry::Item(cmd) => {
                let item = command_item(*cmd);
                submenu.append(&item).ok();
                items.plain.insert(*cmd, item);
            }
        }
    }
}

fn command_item(cmd: Command) -> MenuItem {
    MenuItem::with_id(
        cmd.id(),
        cmd.label(),
        true,
        cmd.shortcut().and_then(to_accelerator),
    )
}

fn append(submenu: &Submenu, items: &[&dyn muda::IsMenuItem]) {
    for item in items {
        submenu.append(*item).ok();
    }
}

fn to_accelerator(sc: KeyboardShortcut) -> Option<Accelerator> {
    let mut mods = Modifiers::empty();
    if sc.modifiers.command || sc.modifiers.mac_cmd {
        mods |= Modifiers::META;
    }
    if sc.modifiers.shift {
        mods |= Modifiers::SHIFT;
    }
    if sc.modifiers.alt {
        mods |= Modifiers::ALT;
    }
    if sc.modifiers.ctrl {
        mods |= Modifiers::CONTROL;
    }
    let code = match sc.logical_key {
        Key::A => Code::KeyA,
        Key::C => Code::KeyC,
        Key::L => Code::KeyL,
        Key::N => Code::KeyN,
        Key::O => Code::KeyO,
        Key::Q => Code::KeyQ,
        Key::S => Code::KeyS,
        Key::V => Code::KeyV,
        Key::W => Code::KeyW,
        Key::X => Code::KeyX,
        Key::Z => Code::KeyZ,
        Key::Equals => Code::Equal,
        Key::Minus => Code::Minus,
        Key::Num0 => Code::Digit0,
        Key::E => Code::KeyE,
        Key::F => Code::KeyF,
        Key::G => Code::KeyG,
        Key::Tab => Code::Tab,
        Key::Slash => Code::Slash,
        Key::OpenBracket => Code::BracketLeft,
        Key::P => Code::KeyP,
        Key::Comma => Code::Comma,
        Key::CloseBracket => Code::BracketRight,
        _ => return None, // add more keys here as new shortcuts need them
    };
    Some(Accelerator::new(mods, code))
}
