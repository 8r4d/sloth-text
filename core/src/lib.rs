//! Code shared by Sloth and Sloth Writer: documents and their file formats,
//! the text editor widget, find and replace, tabs, themes, syntax
//! highlighting, fonts, crash recovery, and the macOS print and open helpers.

pub mod document;
pub mod editing;
pub mod editor;
pub mod file_types;
pub mod find;
pub mod fonts;
pub mod markdown;
pub mod motion;
pub mod print_text;
#[cfg(target_os = "macos")]
pub mod macos_open;
#[cfg(target_os = "macos")]
pub mod macos_print;
pub mod recovery;
pub mod search_results;
pub mod spell;
pub mod syntax;
pub mod tabs;
pub mod text_format;
pub mod theme;
pub mod trash;
