//! The native macOS menu bar, built from `commands::MENUS`.
//!
//! Selecting a menu item (or pressing its shortcut, which macOS routes through
//! the menu) sends the matching `Command` down a channel that the app drains
//! every frame.

use std::collections::HashMap;
use std::sync::mpsc::{Receiver, channel};

use eframe::egui::{self, Key, KeyboardShortcut};
use muda::accelerator::{Accelerator, Code, Modifiers};
use muda::{
    AboutMetadata, CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu,
};

use crate::commands::{self, Command, MenuEntry};

pub struct NativeMenu {
    /// Must stay alive for the menu to keep working.
    _menu: Menu,
    events: Receiver<Command>,
    checkable: HashMap<Command, CheckMenuItem>,
}

impl NativeMenu {
    pub fn install(ctx: &egui::Context) -> Self {
        let menu = Menu::new();
        let mut checkable = HashMap::new();

        // Application menu (titled with the app name by macOS).
        let app_menu = Submenu::new("Sloth", true);
        append(
            &app_menu,
            &[
                &PredefinedMenuItem::about(
                    Some("About Sloth"),
                    Some(AboutMetadata {
                        name: Some("Sloth".into()),
                        version: Some(env!("CARGO_PKG_VERSION").into()),
                        ..Default::default()
                    }),
                ),
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

        for (title, entries) in commands::MENUS {
            let submenu = Submenu::new(*title, true);
            fill(&submenu, entries, &mut checkable);
            menu.append(&submenu).ok();
        }

        let window_menu = Submenu::new("Window", true);
        append(
            &window_menu,
            &[
                &PredefinedMenuItem::minimize(None),
                &PredefinedMenuItem::maximize(None),
                &PredefinedMenuItem::fullscreen(None),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::bring_all_to_front(None),
            ],
        );
        menu.append(&window_menu).ok();

        menu.init_for_nsapp();
        window_menu.set_as_windows_menu_for_nsapp();

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
            checkable,
        }
    }

    /// Commands chosen from the menu since the last call.
    pub fn drain(&self) -> Vec<Command> {
        self.events.try_iter().collect()
    }

    /// Keep check marks in sync with app state.
    pub fn set_checked(&self, cmd: Command, checked: bool) {
        if let Some(item) = self.checkable.get(&cmd)
            && item.is_checked() != checked
        {
            item.set_checked(checked);
        }
    }
}

/// Add `entries` to `submenu`, recursing into nested submenus.
fn fill(
    submenu: &Submenu,
    entries: &[MenuEntry],
    checkable: &mut HashMap<Command, CheckMenuItem>,
) {
    // Quit lives in the app menu on macOS; drop it (and any separator left
    // dangling at the end) from the other menus.
    let mut entries: Vec<_> = entries
        .iter()
        .filter(|e| !matches!(e, MenuEntry::Item(Command::Quit)))
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
                fill(&child, children, checkable);
                submenu.append(&child).ok();
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
                checkable.insert(*cmd, check);
            }
            MenuEntry::Item(cmd) => {
                submenu.append(&command_item(*cmd)).ok();
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
        _ => return None, // add more keys here as new shortcuts need them
    };
    Some(Accelerator::new(mods, code))
}
