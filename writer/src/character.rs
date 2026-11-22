//! A character profile, as stored: a plain Markdown file whose opening text
//! is the description, followed by a section per characteristic.
//!
//! ```text
//! Mara is the eldest daughter, and the one who left.
//!
//! ## Motivations
//!
//! To be needed somewhere she chose.
//! ```
//!
//! The file stays readable (and editable) anywhere; the app shows it as a
//! form (see character_ui.rs).

/// A character's profile.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sheet {
    pub description: String,
    pub sections: Vec<Section>,
}

/// One characteristic: "Motivations" and what's written about it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Section {
    pub heading: String,
    pub text: String,
}

impl Sheet {
    /// Read a profile from its file's text. Every `## ` line starts a
    /// section; whatever comes before the first is the description.
    pub fn parse(text: &str) -> Self {
        let mut sheet = Sheet::default();
        let mut current: Option<Section> = None;
        let mut body = String::new();
        for line in text.lines() {
            if let Some(heading) = line.strip_prefix("## ") {
                match current.take() {
                    Some(mut section) => {
                        section.text = body.trim().to_owned();
                        sheet.sections.push(section);
                    }
                    None => sheet.description = body.trim().to_owned(),
                }
                body.clear();
                current = Some(Section { heading: heading.trim().to_owned(), text: String::new() });
            } else {
                body.push_str(line);
                body.push('\n');
            }
        }
        match current {
            Some(mut section) => {
                section.text = body.trim().to_owned();
                sheet.sections.push(section);
            }
            None => sheet.description = body.trim().to_owned(),
        }
        sheet
    }

    /// The file's text for this profile.
    pub fn to_text(&self) -> String {
        let mut out = self.description.trim().to_owned();
        for section in &self.sections {
            if !out.is_empty() {
                out.push_str("\n\n");
            }
            out.push_str("## ");
            out.push_str(section.heading.trim());
            out.push_str("\n\n");
            out.push_str(section.text.trim());
        }
        out.push('\n');
        out
    }

    pub fn has(&self, heading: &str) -> bool {
        self.sections.iter().any(|s| s.heading == heading)
    }

    /// Rename a characteristic's section (when the project's list changes).
    /// Returns whether this profile had it.
    pub fn rename(&mut self, old: &str, new: &str) -> bool {
        let mut found = false;
        for section in self.sections.iter_mut().filter(|s| s.heading == old) {
            section.heading = new.to_owned();
            found = true;
        }
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_and_writes_a_profile() {
        let text = "Mara is the eldest daughter.\nShe left first.\n\n## Motivations\n\nTo be needed.\n\n## Voice\n\nDry.\nQuiet.\n";
        let sheet = Sheet::parse(text);
        assert_eq!(sheet.description, "Mara is the eldest daughter.\nShe left first.");
        assert_eq!(
            sheet.sections,
            [
                Section { heading: "Motivations".into(), text: "To be needed.".into() },
                Section { heading: "Voice".into(), text: "Dry.\nQuiet.".into() },
            ]
        );
        assert_eq!(sheet.to_text(), text);
        assert_eq!(Sheet::parse(&sheet.to_text()), sheet);
    }

    #[test]
    fn handles_empty_parts() {
        assert_eq!(Sheet::parse(""), Sheet::default());
        assert_eq!(Sheet::default().to_text(), "\n");
        // A section with nothing written yet, and no description.
        let sheet = Sheet { description: String::new(), sections: vec![Section { heading: "Arc".into(), text: String::new() }] };
        assert_eq!(sheet.to_text(), "## Arc\n\n\n");
        assert_eq!(Sheet::parse(&sheet.to_text()), sheet);
    }

    #[test]
    fn renames_a_characteristic() {
        let mut sheet = Sheet::parse("## Motives\n\nMoney.");
        assert!(sheet.rename("Motives", "Motivations"));
        assert!(sheet.has("Motivations") && !sheet.has("Motives"));
        assert!(!sheet.rename("Voice", "Speech"));
    }
}
