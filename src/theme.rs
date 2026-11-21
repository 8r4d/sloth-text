//! Colour themes.
//!
//! Each theme is a small `Palette`; `Palette::visuals` turns it into egui's
//! styling. To add a theme: add a variant to `ThemeChoice`, list it in `ALL`,
//! give it a label and a palette. It then shows up in View → Theme.

use eframe::egui::{self, Color32, SystemTheme, Theme, ThemePreference, ViewportCommand, Visuals};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ThemeChoice {
    /// Light or Dark, following the macOS appearance setting.
    #[default]
    System,
    Light,
    Dark,
    SolarizedLight,
    SolarizedDark,
    Sepia,
    Midnight,
}

pub const ALL: &[ThemeChoice] = &[
    ThemeChoice::System,
    ThemeChoice::Light,
    ThemeChoice::Dark,
    ThemeChoice::SolarizedLight,
    ThemeChoice::SolarizedDark,
    ThemeChoice::Sepia,
    ThemeChoice::Midnight,
];

impl ThemeChoice {
    pub fn label(self) -> &'static str {
        match self {
            ThemeChoice::System => "Match System",
            ThemeChoice::Light => "Light",
            ThemeChoice::Dark => "Dark",
            ThemeChoice::SolarizedLight => "Solarized Light",
            ThemeChoice::SolarizedDark => "Solarized Dark",
            ThemeChoice::Sepia => "Sepia",
            ThemeChoice::Midnight => "Midnight",
        }
    }

    /// Stable name used when saving the setting.
    pub fn key(self) -> String {
        format!("{self:?}")
    }

    pub fn from_key(key: &str) -> Option<Self> {
        ALL.iter().copied().find(|t| t.key() == key)
    }

    /// `None` for `System`, which picks Light or Dark at runtime.
    fn palette(self) -> Option<Palette> {
        Some(match self {
            ThemeChoice::System => return None,
            ThemeChoice::Light => LIGHT,
            ThemeChoice::Dark => DARK,
            ThemeChoice::SolarizedLight => Palette {
                dark: false,
                editor_bg: hex(0xfdf6e3),
                chrome_bg: hex(0xeee8d5),
                text: hex(0x586e75),
                muted: hex(0x93a1a1),
                border: hex(0xd9d2bd),
                accent: hex(0x268bd2),
                selection: hex(0xe4dcc3),
            },
            ThemeChoice::SolarizedDark => Palette {
                dark: true,
                editor_bg: hex(0x002b36),
                chrome_bg: hex(0x073642),
                text: hex(0x93a1a1),
                muted: hex(0x586e75),
                border: hex(0x0e4654),
                accent: hex(0x268bd2),
                selection: hex(0x114b5c),
            },
            ThemeChoice::Sepia => Palette {
                dark: false,
                editor_bg: hex(0xf4ecd8),
                chrome_bg: hex(0xeadfc6),
                text: hex(0x433422),
                muted: hex(0xa08f74),
                border: hex(0xd6c8a8),
                accent: hex(0xb5651d),
                selection: hex(0xe2cfa5),
            },
            ThemeChoice::Midnight => Palette {
                dark: true,
                editor_bg: hex(0x0f1420),
                chrome_bg: hex(0x151b29),
                text: hex(0xc8d3f5),
                muted: hex(0x4b5675),
                border: hex(0x232c3f),
                accent: hex(0x82aaff),
                selection: hex(0x2a3a5e),
            },
        })
    }

    /// Apply this theme to the whole app, including the window title bar.
    pub fn apply(self, ctx: &egui::Context) {
        match self.palette() {
            None => {
                ctx.set_visuals_of(Theme::Light, LIGHT.visuals());
                ctx.set_visuals_of(Theme::Dark, DARK.visuals());
                ctx.set_theme(ThemePreference::System);
                ctx.send_viewport_cmd(ViewportCommand::SetTheme(SystemTheme::SystemDefault));
            }
            Some(p) => {
                // Use the same visuals whichever mode macOS is in.
                ctx.set_visuals_of(Theme::Light, p.visuals());
                ctx.set_visuals_of(Theme::Dark, p.visuals());
                let (pref, system) = if p.dark {
                    (ThemePreference::Dark, SystemTheme::Dark)
                } else {
                    (ThemePreference::Light, SystemTheme::Light)
                };
                ctx.set_theme(pref);
                ctx.send_viewport_cmd(ViewportCommand::SetTheme(system));
            }
        }
    }
}

/// The handful of colours a theme needs to define.
#[derive(Clone, Copy)]
struct Palette {
    dark: bool,
    /// Background of the text area.
    editor_bg: Color32,
    /// Background of the gutter, status bar, menus and dialogs.
    chrome_bg: Color32,
    text: Color32,
    /// Line numbers and secondary text.
    muted: Color32,
    border: Color32,
    /// Text cursor and links.
    accent: Color32,
    selection: Color32,
}

const LIGHT: Palette = Palette {
    dark: false,
    editor_bg: hex(0xffffff),
    chrome_bg: hex(0xf3f3f3),
    text: hex(0x1f1f1f),
    muted: hex(0x9a9a9a),
    border: hex(0xdadada),
    accent: hex(0x2f6feb),
    selection: hex(0xcfe0ff),
};

const DARK: Palette = Palette {
    dark: true,
    editor_bg: hex(0x1e1e1e),
    chrome_bg: hex(0x252526),
    text: hex(0xd4d4d4),
    muted: hex(0x6e6e6e),
    border: hex(0x3a3a3a),
    accent: hex(0x569cd6),
    selection: hex(0x264f78),
};

impl Palette {
    fn visuals(&self) -> Visuals {
        let mut v = if self.dark {
            Visuals::dark()
        } else {
            Visuals::light()
        };

        // Surfaces. The editor paints with `extreme_bg_color` and the
        // gutter with `faint_bg_color` (see editor.rs).
        v.extreme_bg_color = self.editor_bg;
        v.text_edit_bg_color = Some(self.editor_bg);
        v.faint_bg_color = self.chrome_bg;
        v.panel_fill = self.chrome_bg;
        v.window_fill = self.chrome_bg;
        v.window_stroke.color = self.border;

        // Text.
        v.override_text_color = Some(self.text);
        v.weak_text_color = Some(self.muted);
        v.hyperlink_color = self.accent;

        // Selection and cursor.
        v.selection.bg_fill = self.selection;
        v.selection.stroke.color = self.text;
        v.text_cursor.stroke.color = self.accent;

        // Widgets (buttons, menu items): tints of the text colour over the
        // chrome background so they suit every theme.
        let tint = |t: f32| self.chrome_bg.lerp_to_gamma(self.text, t);
        v.widgets.noninteractive.bg_fill = self.chrome_bg;
        v.widgets.noninteractive.weak_bg_fill = self.chrome_bg;
        v.widgets.noninteractive.bg_stroke.color = self.border;
        v.widgets.noninteractive.fg_stroke.color = self.text;
        for (w, t) in [
            (&mut v.widgets.inactive, 0.08),
            (&mut v.widgets.hovered, 0.16),
            (&mut v.widgets.active, 0.24),
            (&mut v.widgets.open, 0.16),
        ] {
            w.bg_fill = tint(t);
            w.weak_bg_fill = tint(t);
            w.fg_stroke.color = self.text;
        }
        v.widgets.hovered.bg_stroke.color = self.border;

        v
    }
}

const fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}
