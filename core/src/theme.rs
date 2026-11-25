//! Colour themes, shared by Sloth and Sloth Writer.
//!
//! Each theme is a small `Palette`; `Palette::visuals` turns it into egui's
//! styling. To add a theme: add a variant to `ThemeChoice`, list it in `ALL`
//! (with the light or the dark ones), and give it a label and a palette. It
//! then shows up in both apps' Settings (see `picker`); in Sloth's View →
//! Theme menu too, once it's added to that list (sloth/src/commands.rs).
//! The tests check every palette's text can be read on its background.
//!
//! Besides Sloth's own (Light, Dark, Paper, Sepia, Midnight, Severance and Fallout (dark and light), Newsroom, Nostromo, Pastel Hotel, Lamplight and
//! the High Contrast pair), the palettes follow well-known open themes:
//! Solarized, GitHub, One, Gruvbox, Catppuccin, Rosé Pine, Everforest,
//! Tokyo Night, Nord, Dracula and Monokai.

use eframe::egui::{self, Color32, SystemTheme, Theme, ThemePreference, ViewportCommand, Visuals};

use crate::syntax::SyntaxColors;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
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
    /// After the office terminals in the TV show: pale cyan on blue-black.
    Severance,
    /// Its daytime office: deep teal on cool white.
    SeveranceLight,
    /// A wrist computer's terminal: phosphor green on near black.
    Fallout,
    /// The vault's own colours: vault blue on warm cream, sunny yellow.
    FalloutLight,
    /// A 1960s newsroom: typewriter ink on cream paper, a red pencil.
    Newsroom,
    /// A ship's computer: amber phosphor on black.
    Nostromo,
    /// A pastel hotel: plum ink on dusty pink, lilac and mustard.
    PastelHotel,
    Paper,
    GithubLight,
    OneLight,
    GruvboxLight,
    CatppuccinLatte,
    RosePineDawn,
    EverforestLight,
    HighContrastLight,
    Lamplight,
    GithubDark,
    OneDark,
    Nord,
    Dracula,
    GruvboxDark,
    TokyoNight,
    CatppuccinMocha,
    RosePine,
    EverforestDark,
    Monokai,
    HighContrastDark,
}

/// Every theme: Match System, then the light ones, then the dark ones.
pub const ALL: &[ThemeChoice] = &[
    ThemeChoice::System,
    ThemeChoice::Light,
    ThemeChoice::Paper,
    ThemeChoice::Sepia,
    ThemeChoice::SolarizedLight,
    ThemeChoice::GithubLight,
    ThemeChoice::OneLight,
    ThemeChoice::GruvboxLight,
    ThemeChoice::CatppuccinLatte,
    ThemeChoice::RosePineDawn,
    ThemeChoice::EverforestLight,
    ThemeChoice::HighContrastLight,
    ThemeChoice::SeveranceLight,
    ThemeChoice::FalloutLight,
    ThemeChoice::Newsroom,
    ThemeChoice::PastelHotel,
    ThemeChoice::Dark,
    ThemeChoice::Midnight,
    ThemeChoice::Severance,
    ThemeChoice::Fallout,
    ThemeChoice::Nostromo,
    ThemeChoice::SolarizedDark,
    ThemeChoice::Lamplight,
    ThemeChoice::GithubDark,
    ThemeChoice::OneDark,
    ThemeChoice::Nord,
    ThemeChoice::Dracula,
    ThemeChoice::GruvboxDark,
    ThemeChoice::TokyoNight,
    ThemeChoice::CatppuccinMocha,
    ThemeChoice::RosePine,
    ThemeChoice::EverforestDark,
    ThemeChoice::Monokai,
    ThemeChoice::HighContrastDark,
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
            ThemeChoice::Severance => "Severance",
            ThemeChoice::SeveranceLight => "Severance Light",
            ThemeChoice::Fallout => "Fallout",
            ThemeChoice::FalloutLight => "Fallout Light",
            ThemeChoice::Newsroom => "Newsroom",
            ThemeChoice::Nostromo => "Nostromo",
            ThemeChoice::PastelHotel => "Pastel Hotel",
            ThemeChoice::Paper => "Paper",
            ThemeChoice::GithubLight => "GitHub Light",
            ThemeChoice::OneLight => "One Light",
            ThemeChoice::GruvboxLight => "Gruvbox Light",
            ThemeChoice::CatppuccinLatte => "Catppuccin Latte",
            ThemeChoice::RosePineDawn => "Rosé Pine Dawn",
            ThemeChoice::EverforestLight => "Everforest Light",
            ThemeChoice::HighContrastLight => "High Contrast Light",
            ThemeChoice::Lamplight => "Lamplight",
            ThemeChoice::GithubDark => "GitHub Dark",
            ThemeChoice::OneDark => "One Dark",
            ThemeChoice::Nord => "Nord",
            ThemeChoice::Dracula => "Dracula",
            ThemeChoice::GruvboxDark => "Gruvbox Dark",
            ThemeChoice::TokyoNight => "Tokyo Night",
            ThemeChoice::CatppuccinMocha => "Catppuccin Mocha",
            ThemeChoice::RosePine => "Rosé Pine",
            ThemeChoice::EverforestDark => "Everforest Dark",
            ThemeChoice::Monokai => "Monokai",
            ThemeChoice::HighContrastDark => "High Contrast Dark",
        }
    }

    /// One of the two Severance themes, which bring the office touches
    /// that are only looks (the start screen's emblem, say).
    pub fn is_severance(self) -> bool {
        matches!(self, ThemeChoice::Severance | ThemeChoice::SeveranceLight)
    }

    /// A theme from an old screen, which can wear its glow (scanlines and
    /// darker edges): the Severance and Fallout ones.
    pub fn has_glow(self) -> bool {
        self.is_severance() || matches!(self, ThemeChoice::Fallout | ThemeChoice::FalloutLight | ThemeChoice::Nostromo)
    }

    /// The start screen shows the sloth as a one-colour emblem: the old
    /// screens' themes, and the Pastel Hotel's crest.
    pub fn has_emblem(self) -> bool {
        self.has_glow() || self == ThemeChoice::PastelHotel
    }

    /// The start screen's fact types itself out, like a terminal's.
    pub fn types_out(self) -> bool {
        matches!(self, ThemeChoice::Severance | ThemeChoice::Fallout | ThemeChoice::Nostromo)
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
                syntax: SyntaxColors {
                    comment: hex(0x93a1a1),
                    keyword: hex(0x859900),
                    string: hex(0x2aa198),
                    number: hex(0xd33682),
                    function: hex(0x268bd2),
                    type_: hex(0xb58900),
                },
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
                syntax: SyntaxColors {
                    comment: hex(0x586e75),
                    keyword: hex(0x859900),
                    string: hex(0x2aa198),
                    number: hex(0xd33682),
                    function: hex(0x268bd2),
                    type_: hex(0xb58900),
                },
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
                syntax: SyntaxColors {
                    comment: hex(0xa08f74),
                    keyword: hex(0xa0522d),
                    string: hex(0x4d7a3b),
                    number: hex(0x8b3a8b),
                    function: hex(0x2f5f8a),
                    type_: hex(0x8a6d1f),
                },
            },
            ThemeChoice::Fallout => Palette {
                dark: true,
                editor_bg: hex(0x08110a),
                chrome_bg: hex(0x0c170e),
                text: hex(0x9cf0ae),
                muted: hex(0x4f8a5c),
                border: hex(0x1a3320),
                accent: hex(0x3dfc78),
                selection: hex(0x1b4727),
                syntax: SyntaxColors {
                    comment: hex(0x4f8a5c),
                    keyword: hex(0x3dfc78),
                    string: hex(0xd8f27a),
                    number: hex(0xf2c25a),
                    function: hex(0x7ff2d0),
                    type_: hex(0xc8ffd4),
                },
            },
            ThemeChoice::Newsroom => Palette {
                dark: false,
                editor_bg: hex(0xf4efe3),
                chrome_bg: hex(0xe8e0cc),
                text: hex(0x1f1d1a),
                muted: hex(0x77706a),
                border: hex(0xd5cbb4),
                accent: hex(0xb3261e),
                selection: hex(0xe9d9b0),
                syntax: SyntaxColors {
                    comment: hex(0x77706a),
                    keyword: hex(0xb3261e),
                    string: hex(0x3f5a2a),
                    number: hex(0x8a4b16),
                    function: hex(0x2a3f66),
                    type_: hex(0x1f1d1a),
                },
            },
            ThemeChoice::Nostromo => Palette {
                dark: true,
                editor_bg: hex(0x0c0904),
                chrome_bg: hex(0x140f07),
                text: hex(0xf2bd68),
                muted: hex(0x8f6c35),
                border: hex(0x33270f),
                accent: hex(0xffa529),
                selection: hex(0x4a3512),
                syntax: SyntaxColors {
                    comment: hex(0x8f6c35),
                    keyword: hex(0xffa529),
                    string: hex(0xe8d27a),
                    number: hex(0xff7f50),
                    function: hex(0xffd08a),
                    type_: hex(0xfce3b8),
                },
            },
            ThemeChoice::PastelHotel => Palette {
                dark: false,
                editor_bg: hex(0xfbeef0),
                chrome_bg: hex(0xf2dce2),
                text: hex(0x4a2a3c),
                muted: hex(0x94727f),
                border: hex(0xe5c4ce),
                accent: hex(0x86509f),
                selection: hex(0xf2d98f),
                syntax: SyntaxColors {
                    comment: hex(0x94727f),
                    keyword: hex(0x86509f),
                    string: hex(0x5e7a3a),
                    number: hex(0xa65a2a),
                    function: hex(0x3f5c8c),
                    type_: hex(0x4a2a3c),
                },
            },
            ThemeChoice::FalloutLight => Palette {
                dark: false,
                editor_bg: hex(0xf5f0df),
                chrome_bg: hex(0xe9e1c6),
                text: hex(0x14304d),
                muted: hex(0x6a7b8c),
                border: hex(0xd3c8a3),
                accent: hex(0x1f5fa6),
                selection: hex(0xf6dd73),
                syntax: SyntaxColors {
                    comment: hex(0x6a7b8c),
                    keyword: hex(0x1f5fa6),
                    string: hex(0x5a7a1e),
                    number: hex(0xa8541a),
                    function: hex(0x2b4f8f),
                    type_: hex(0x14304d),
                },
            },
            ThemeChoice::SeveranceLight => Palette {
                dark: false,
                editor_bg: hex(0xf3f7f6),
                chrome_bg: hex(0xe2ebea),
                text: hex(0x0d3a44),
                muted: hex(0x6a8c92),
                border: hex(0xc7d7d7),
                accent: hex(0x0b8a9e),
                selection: hex(0xc4e6eb),
                syntax: SyntaxColors {
                    comment: hex(0x6a8c92),
                    keyword: hex(0x0b7a8c),
                    string: hex(0x2e7d5b),
                    number: hex(0x9a6a12),
                    function: hex(0x1f5fa8),
                    type_: hex(0x0d3a44),
                },
            },
            ThemeChoice::Severance => Palette {
                dark: true,
                editor_bg: hex(0x061219),
                chrome_bg: hex(0x0a1a22),
                text: hex(0xb4ebf2),
                muted: hex(0x4d7c87),
                border: hex(0x16343f),
                accent: hex(0x3fd0e4),
                selection: hex(0x11414d),
                syntax: SyntaxColors {
                    comment: hex(0x4d7c87),
                    keyword: hex(0x5fdcee),
                    string: hex(0x8fe6cc),
                    number: hex(0xe8cc72),
                    function: hex(0x82c4ff),
                    type_: hex(0xc4f4fa),
                },
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
                syntax: SyntaxColors {
                    comment: hex(0x636da6),
                    keyword: hex(0xc099ff),
                    string: hex(0xc3e88d),
                    number: hex(0xff966c),
                    function: hex(0x82aaff),
                    type_: hex(0xffc777),
                },
            },
            ThemeChoice::Paper => Palette {
                dark: false,
                editor_bg: hex(0xfbfaf7),
                chrome_bg: hex(0xf1efe9),
                text: hex(0x2b2b2b),
                muted: hex(0x9b978e),
                border: hex(0xdedad0),
                accent: hex(0x3b6ea5),
                selection: hex(0xdce6f2),
                syntax: SyntaxColors {
                    comment: hex(0x8a857a),
                    keyword: hex(0xa23b72),
                    string: hex(0x3d7a3a),
                    number: hex(0xb5541c),
                    function: hex(0x2b5f9e),
                    type_: hex(0x7a5c00),
                },
            },
            ThemeChoice::GithubLight => Palette {
                dark: false,
                editor_bg: hex(0xffffff),
                chrome_bg: hex(0xf6f8fa),
                text: hex(0x1f2328),
                muted: hex(0x8c959f),
                border: hex(0xd0d7de),
                accent: hex(0x0969da),
                selection: hex(0xb6e3ff),
                syntax: SyntaxColors {
                    comment: hex(0x6e7781),
                    keyword: hex(0xcf222e),
                    string: hex(0x0a3069),
                    number: hex(0x0550ae),
                    function: hex(0x8250df),
                    type_: hex(0x953800),
                },
            },
            ThemeChoice::OneLight => Palette {
                dark: false,
                editor_bg: hex(0xfafafa),
                chrome_bg: hex(0xf0f0f1),
                text: hex(0x383a42),
                muted: hex(0xa0a1a7),
                border: hex(0xdbdbdc),
                accent: hex(0x4078f2),
                selection: hex(0xd7e0f5),
                syntax: SyntaxColors {
                    comment: hex(0xa0a1a7),
                    keyword: hex(0xa626a4),
                    string: hex(0x50a14f),
                    number: hex(0x986801),
                    function: hex(0x4078f2),
                    type_: hex(0xc18401),
                },
            },
            ThemeChoice::GruvboxLight => Palette {
                dark: false,
                editor_bg: hex(0xfbf1c7),
                chrome_bg: hex(0xf2e5bc),
                text: hex(0x3c3836),
                muted: hex(0x928374),
                border: hex(0xd5c4a1),
                accent: hex(0x076678),
                selection: hex(0xebdbb2),
                syntax: SyntaxColors {
                    comment: hex(0x928374),
                    keyword: hex(0x9d0006),
                    string: hex(0x79740e),
                    number: hex(0x8f3f71),
                    function: hex(0xb57614),
                    type_: hex(0x427b58),
                },
            },
            ThemeChoice::CatppuccinLatte => Palette {
                dark: false,
                editor_bg: hex(0xeff1f5),
                chrome_bg: hex(0xe6e9ef),
                text: hex(0x4c4f69),
                muted: hex(0x9ca0b0),
                border: hex(0xccd0da),
                accent: hex(0x1e66f5),
                selection: hex(0xccd0da),
                syntax: SyntaxColors {
                    comment: hex(0x8c8fa1),
                    keyword: hex(0x8839ef),
                    string: hex(0x40a02b),
                    number: hex(0xfe640b),
                    function: hex(0x1e66f5),
                    type_: hex(0xdf8e1d),
                },
            },
            ThemeChoice::RosePineDawn => Palette {
                dark: false,
                editor_bg: hex(0xfaf4ed),
                chrome_bg: hex(0xfffaf3),
                text: hex(0x575279),
                muted: hex(0x9893a5),
                border: hex(0xdfdad9),
                accent: hex(0x907aa9),
                selection: hex(0xdfdad9),
                syntax: SyntaxColors {
                    comment: hex(0x9893a5),
                    keyword: hex(0x286983),
                    string: hex(0xea9d34),
                    number: hex(0xb4637a),
                    function: hex(0xd7827e),
                    type_: hex(0x56949f),
                },
            },
            ThemeChoice::EverforestLight => Palette {
                dark: false,
                editor_bg: hex(0xfdf6e3),
                chrome_bg: hex(0xf4f0d9),
                text: hex(0x5c6a72),
                muted: hex(0xa6b0a0),
                border: hex(0xe0dcc7),
                accent: hex(0x8da101),
                selection: hex(0xe6e2cc),
                syntax: SyntaxColors {
                    comment: hex(0x939f91),
                    keyword: hex(0xf85552),
                    string: hex(0x8da101),
                    number: hex(0xdf69ba),
                    function: hex(0x35a77c),
                    type_: hex(0xdfa000),
                },
            },
            ThemeChoice::HighContrastLight => Palette {
                dark: false,
                editor_bg: hex(0xffffff),
                chrome_bg: hex(0xf0f0f0),
                text: hex(0x000000),
                muted: hex(0x555555),
                border: hex(0x767676),
                accent: hex(0x0000cc),
                selection: hex(0xadd6ff),
                syntax: SyntaxColors {
                    comment: hex(0x006400),
                    keyword: hex(0x0000cd),
                    string: hex(0xa31515),
                    number: hex(0x098658),
                    function: hex(0x795e26),
                    type_: hex(0x267f99),
                },
            },
            ThemeChoice::Lamplight => Palette {
                dark: true,
                editor_bg: hex(0x1f1d1a),
                chrome_bg: hex(0x26231f),
                text: hex(0xe3dccf),
                muted: hex(0x7d7466),
                border: hex(0x38332c),
                accent: hex(0xd9a55b),
                selection: hex(0x4a3f2e),
                syntax: SyntaxColors {
                    comment: hex(0x8a8072),
                    keyword: hex(0xd4876a),
                    string: hex(0xa8b878),
                    number: hex(0xc9a0dc),
                    function: hex(0x7fb4c9),
                    type_: hex(0xd9b85b),
                },
            },
            ThemeChoice::GithubDark => Palette {
                dark: true,
                editor_bg: hex(0x0d1117),
                chrome_bg: hex(0x161b22),
                text: hex(0xe6edf3),
                muted: hex(0x6e7681),
                border: hex(0x30363d),
                accent: hex(0x2f81f7),
                selection: hex(0x1c3a5e),
                syntax: SyntaxColors {
                    comment: hex(0x8b949e),
                    keyword: hex(0xff7b72),
                    string: hex(0xa5d6ff),
                    number: hex(0x79c0ff),
                    function: hex(0xd2a8ff),
                    type_: hex(0xffa657),
                },
            },
            ThemeChoice::OneDark => Palette {
                dark: true,
                editor_bg: hex(0x282c34),
                chrome_bg: hex(0x21252b),
                text: hex(0xabb2bf),
                muted: hex(0x5c6370),
                border: hex(0x3e4451),
                accent: hex(0x61afef),
                selection: hex(0x3e4451),
                syntax: SyntaxColors {
                    comment: hex(0x7f848e),
                    keyword: hex(0xc678dd),
                    string: hex(0x98c379),
                    number: hex(0xd19a66),
                    function: hex(0x61afef),
                    type_: hex(0xe5c07b),
                },
            },
            ThemeChoice::Nord => Palette {
                dark: true,
                editor_bg: hex(0x2e3440),
                chrome_bg: hex(0x3b4252),
                text: hex(0xd8dee9),
                muted: hex(0x616e88),
                border: hex(0x434c5e),
                accent: hex(0x88c0d0),
                selection: hex(0x434c5e),
                syntax: SyntaxColors {
                    comment: hex(0x616e88),
                    keyword: hex(0x81a1c1),
                    string: hex(0xa3be8c),
                    number: hex(0xb48ead),
                    function: hex(0x88c0d0),
                    type_: hex(0x8fbcbb),
                },
            },
            ThemeChoice::Dracula => Palette {
                dark: true,
                editor_bg: hex(0x282a36),
                chrome_bg: hex(0x21222c),
                text: hex(0xf8f8f2),
                muted: hex(0x6272a4),
                border: hex(0x44475a),
                accent: hex(0xbd93f9),
                selection: hex(0x44475a),
                syntax: SyntaxColors {
                    comment: hex(0x6272a4),
                    keyword: hex(0xff79c6),
                    string: hex(0xf1fa8c),
                    number: hex(0xbd93f9),
                    function: hex(0x50fa7b),
                    type_: hex(0x8be9fd),
                },
            },
            ThemeChoice::GruvboxDark => Palette {
                dark: true,
                editor_bg: hex(0x282828),
                chrome_bg: hex(0x1d2021),
                text: hex(0xebdbb2),
                muted: hex(0x928374),
                border: hex(0x3c3836),
                accent: hex(0x83a598),
                selection: hex(0x504945),
                syntax: SyntaxColors {
                    comment: hex(0x928374),
                    keyword: hex(0xfb4934),
                    string: hex(0xb8bb26),
                    number: hex(0xd3869b),
                    function: hex(0xfabd2f),
                    type_: hex(0x8ec07c),
                },
            },
            ThemeChoice::TokyoNight => Palette {
                dark: true,
                editor_bg: hex(0x1a1b26),
                chrome_bg: hex(0x16161e),
                text: hex(0xc0caf5),
                muted: hex(0x565f89),
                border: hex(0x292e42),
                accent: hex(0x7aa2f7),
                selection: hex(0x283457),
                syntax: SyntaxColors {
                    comment: hex(0x565f89),
                    keyword: hex(0xbb9af7),
                    string: hex(0x9ece6a),
                    number: hex(0xff9e64),
                    function: hex(0x7aa2f7),
                    type_: hex(0x2ac3de),
                },
            },
            ThemeChoice::CatppuccinMocha => Palette {
                dark: true,
                editor_bg: hex(0x1e1e2e),
                chrome_bg: hex(0x181825),
                text: hex(0xcdd6f4),
                muted: hex(0x6c7086),
                border: hex(0x313244),
                accent: hex(0x89b4fa),
                selection: hex(0x45475a),
                syntax: SyntaxColors {
                    comment: hex(0x7f849c),
                    keyword: hex(0xcba6f7),
                    string: hex(0xa6e3a1),
                    number: hex(0xfab387),
                    function: hex(0x89b4fa),
                    type_: hex(0xf9e2af),
                },
            },
            ThemeChoice::RosePine => Palette {
                dark: true,
                editor_bg: hex(0x191724),
                chrome_bg: hex(0x1f1d2e),
                text: hex(0xe0def4),
                muted: hex(0x6e6a86),
                border: hex(0x26233a),
                accent: hex(0xc4a7e7),
                selection: hex(0x403d52),
                syntax: SyntaxColors {
                    comment: hex(0x6e6a86),
                    keyword: hex(0x31748f),
                    string: hex(0xf6c177),
                    number: hex(0xeb6f92),
                    function: hex(0xebbcba),
                    type_: hex(0x9ccfd8),
                },
            },
            ThemeChoice::EverforestDark => Palette {
                dark: true,
                editor_bg: hex(0x2d353b),
                chrome_bg: hex(0x272e33),
                text: hex(0xd3c6aa),
                muted: hex(0x859289),
                border: hex(0x3d484d),
                accent: hex(0xa7c080),
                selection: hex(0x475258),
                syntax: SyntaxColors {
                    comment: hex(0x859289),
                    keyword: hex(0xe67e80),
                    string: hex(0xa7c080),
                    number: hex(0xd699b6),
                    function: hex(0x83c092),
                    type_: hex(0xdbbc7f),
                },
            },
            ThemeChoice::Monokai => Palette {
                dark: true,
                editor_bg: hex(0x272822),
                chrome_bg: hex(0x1e1f1c),
                text: hex(0xf8f8f2),
                muted: hex(0x75715e),
                border: hex(0x3e3d32),
                accent: hex(0x66d9ef),
                selection: hex(0x49483e),
                syntax: SyntaxColors {
                    comment: hex(0x75715e),
                    keyword: hex(0xf92672),
                    string: hex(0xe6db74),
                    number: hex(0xae81ff),
                    function: hex(0xa6e22e),
                    type_: hex(0x66d9ef),
                },
            },
            ThemeChoice::HighContrastDark => Palette {
                dark: true,
                editor_bg: hex(0x000000),
                chrome_bg: hex(0x0a0a0a),
                text: hex(0xffffff),
                muted: hex(0xb0b0b0),
                border: hex(0x6b6b6b),
                accent: hex(0xffd700),
                selection: hex(0x264f78),
                syntax: SyntaxColors {
                    comment: hex(0x8fd47a),
                    keyword: hex(0x6cb6ff),
                    string: hex(0xffa07a),
                    number: hex(0xb5cea8),
                    function: hex(0xfff59d),
                    type_: hex(0x4ec9b0),
                },
            },
        })
    }

    /// Whether it's a dark theme (`None` for Match System, which is either).
    pub fn is_dark(self) -> Option<bool> {
        self.palette().map(|p| p.dark)
    }

    /// Code highlighting colours. "Match System" depends on whether macOS
    /// is currently in dark mode.
    pub fn syntax_colors(self, dark_mode: bool) -> SyntaxColors {
        match self.palette() {
            Some(p) => p.syntax,
            None if dark_mode => DARK.syntax,
            None => LIGHT.syntax,
        }
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
    syntax: SyntaxColors,
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
    syntax: SyntaxColors {
        comment: hex(0x6a737d),
        keyword: hex(0xd73a49),
        string: hex(0x032f62),
        number: hex(0x005cc5),
        function: hex(0x6f42c1),
        type_: hex(0x22863a),
    },
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
    syntax: SyntaxColors {
        comment: hex(0x6a9955),
        keyword: hex(0x569cd6),
        string: hex(0xce9178),
        number: hex(0xb5cea8),
        function: hex(0xdcdcaa),
        type_: hex(0x4ec9b0),
    },
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
        // Menus and dialogs float over the panels, which share their
        // colour: raised a step (lighter in a dark theme, nearer white in a
        // light one), with a firmer edge and, in light themes, a clearer
        // shadow, so a menu over the inspector reads as on top of it.
        v.window_fill = if self.dark {
            self.chrome_bg.lerp_to_gamma(self.text, 0.14)
        } else {
            self.chrome_bg.lerp_to_gamma(Color32::WHITE, 0.6)
        };
        v.window_stroke.color = self.border.lerp_to_gamma(self.text, 0.3);
        if !self.dark {
            v.popup_shadow.color = Color32::from_black_alpha(45);
            v.window_shadow.color = Color32::from_black_alpha(45);
        }

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

/// A theme's colours for Sloth Writer's web page (CSS colours), sent with
/// each sync so writing on a phone looks like writing on the Mac.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct WebColors {
    pub dark: bool,
    /// The page.
    pub bg: String,
    pub text: String,
    /// Secondary text (synopses, counts): softer than the text, still read.
    pub quiet: String,
    /// Rules and borders.
    pub line: String,
    /// Links and buttons.
    pub accent: String,
    /// A wash of the accent, behind panels.
    pub tint: String,
    pub selection: String,
    /// Warnings ("not saved yet").
    pub danger: String,
}

impl ThemeChoice {
    /// This theme's colours for the web page; `None` for Match System (the
    /// page then follows the device's light or dark mode).
    pub fn web_colors(self) -> Option<WebColors> {
        let p = self.palette()?;
        let css = |c: Color32| format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b());
        let danger = if p.dark { hex(0xe8897d) } else { hex(0xa4362a) };
        Some(WebColors {
            dark: p.dark,
            bg: css(p.editor_bg),
            text: css(p.text),
            quiet: css(readable(p.text.lerp_to_gamma(p.editor_bg, 0.4), p.text, p.editor_bg)),
            line: css(p.border),
            accent: css(readable(p.accent, p.text, p.editor_bg)),
            tint: format!("rgba({}, {}, {}, {})", p.accent.r(), p.accent.g(), p.accent.b(), if p.dark { 0.12 } else { 0.08 }),
            selection: css(p.selection),
            danger: css(readable(danger, p.text, p.editor_bg)),
        })
    }
}

/// `color`, moved toward `text` just enough to be read on `bg` (a contrast
/// of 3, as for large text and controls).
fn readable(color: Color32, text: Color32, bg: Color32) -> Color32 {
    (0..=10).map(|i| color.lerp_to_gamma(text, i as f32 / 10.0)).find(|&c| contrast(c, bg) >= 3.0).unwrap_or(text)
}

/// WCAG contrast ratio between two colours (1 to 21).
fn contrast(a: Color32, b: Color32) -> f32 {
    let lum = |c: Color32| {
        let ch = |v: u8| {
            let v = v as f32 / 255.0;
            if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * ch(c.r()) + 0.7152 * ch(c.g()) + 0.0722 * ch(c.b())
    };
    let (x, y) = (lum(a), lum(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

/// The theme menu for a Settings window: Match System, then the light
/// themes and the dark ones under headings. Returns whether it changed.
pub fn picker(ui: &mut egui::Ui, id_salt: &str, choice: &mut ThemeChoice, width: f32) -> bool {
    let mut changed = false;
    egui::ComboBox::from_id_salt(id_salt).selected_text(choice.label()).width(width).height(520.0).show_ui(ui, |ui| {
        changed |= ui.selectable_value(choice, ThemeChoice::System, ThemeChoice::System.label()).changed();
        for (heading, dark) in [("Light", false), ("Dark", true)] {
            ui.add_space(4.0);
            ui.label(egui::RichText::new(heading).small().weak());
            for &t in ALL.iter().filter(|t| t.is_dark() == Some(dark)) {
                changed |= ui.selectable_value(choice, t, t.label()).changed();
            }
        }
    });
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_theme_can_be_read_on_the_web() {
        let parse = |css: &str| {
            let v = u32::from_str_radix(css.trim_start_matches('#'), 16).unwrap();
            hex(v)
        };
        assert_eq!(ThemeChoice::System.web_colors(), None, "the device's own light or dark");
        for &t in ALL {
            let Some(w) = t.web_colors() else { continue };
            let bg = parse(&w.bg);
            let name = t.label();
            assert!(contrast(parse(&w.text), bg) >= 4.5, "{name}: text");
            for (what, c) in [("quiet", &w.quiet), ("links", &w.accent), ("warnings", &w.danger)] {
                assert!(contrast(parse(c), bg) >= 3.0, "{name}: {what} {}", contrast(parse(c), bg));
            }
            assert!(w.tint.starts_with("rgba("));
        }
    }

    #[test]
    fn every_theme_is_listed_once_and_saves_by_name() {
        let mut keys: Vec<String> = ALL.iter().map(|t| t.key()).collect();
        let count = keys.len();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), count, "no theme twice");
        for &t in ALL {
            assert_eq!(ThemeChoice::from_key(&t.key()), Some(t));
        }
        // Light ones first, then dark.
        let dark: Vec<bool> = ALL.iter().filter_map(|t| t.is_dark()).collect();
        assert!(dark.windows(2).all(|w| w[0] <= w[1]), "grouped: light, then dark");
    }

    #[test]
    fn every_theme_can_be_read() {
        for &t in ALL {
            let Some(p) = t.palette() else { continue };
            let name = t.label();
            assert_eq!(p.dark, contrast(p.editor_bg, Color32::BLACK) < contrast(p.editor_bg, Color32::WHITE), "{name}: light or dark as it says");
            assert!(contrast(p.text, p.editor_bg) >= 4.5, "{name}: text {}", contrast(p.text, p.editor_bg));
            // (Labels, not reading: Solarized Light's are 4.4 by design.)
            assert!(contrast(p.text, p.chrome_bg) >= 4.0, "{name}: text on the bars");
            assert!(contrast(p.text, p.selection) >= 3.0, "{name}: selected text");
            assert!(contrast(p.selection, p.editor_bg) >= 1.1, "{name}: the selection shows");
            assert!(contrast(p.muted, p.editor_bg) >= 1.8, "{name}: line numbers {}", contrast(p.muted, p.editor_bg));
            let s = p.syntax;
            for (what, c) in [("comment", s.comment), ("keyword", s.keyword), ("string", s.string), ("number", s.number), ("function", s.function), ("type", s.type_)] {
                assert!(contrast(c, p.editor_bg) >= 2.0, "{name}: {what} {}", contrast(c, p.editor_bg));
            }
        }
    }
}
