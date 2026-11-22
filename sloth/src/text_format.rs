//! Turning file bytes into editable text and back: character encodings,
//! byte-order marks and line endings.
//!
//! The editor always works with UTF-8 text using `\n` line endings. When a
//! file is opened we remember how it was stored ([`FileFormat`]) so it can be
//! saved back the same way.

use encoding_rs::Encoding;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum LineEnding {
    /// `\n`: macOS, Linux.
    #[default]
    Lf,
    /// `\r\n`: Windows.
    CrLf,
}

impl LineEnding {
    pub fn label(self) -> &'static str {
        match self {
            LineEnding::Lf => "LF (macOS, Linux)",
            LineEnding::CrLf => "CRLF (Windows)",
        }
    }

    pub fn short_name(self) -> &'static str {
        match self {
            LineEnding::Lf => "LF",
            LineEnding::CrLf => "CRLF",
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            LineEnding::Lf => "\n",
            LineEnding::CrLf => "\r\n",
        }
    }
}

/// How a file's text is stored on disk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileFormat {
    pub encoding: &'static Encoding,
    /// Whether the file starts with a byte-order mark.
    pub bom: bool,
    pub line_ending: LineEnding,
}

impl Default for FileFormat {
    fn default() -> Self {
        Self {
            encoding: encoding_rs::UTF_8,
            bom: false,
            line_ending: LineEnding::Lf,
        }
    }
}

impl FileFormat {
    /// Short description for the status bar, e.g. "UTF-8 · LF".
    pub fn summary(&self) -> String {
        let name = TextEncoding::from_encoding(self.encoding)
            .map(TextEncoding::short_name)
            .unwrap_or(self.encoding.name());
        let bom = if self.bom { " with BOM" } else { "" };
        format!("{name}{bom} · {}", self.line_ending.short_name())
    }
}

pub struct Decoded {
    /// UTF-8 with `\n` line endings.
    pub text: String,
    pub format: FileFormat,
    /// Some bytes weren't valid in the encoding and were replaced with `�`.
    pub had_errors: bool,
}

/// Decode file contents. With `forced`, use that encoding instead of
/// detecting one (File → Reopen with Encoding).
pub fn decode(bytes: &[u8], forced: Option<&'static Encoding>) -> Decoded {
    let bom = Encoding::for_bom(bytes);
    let (encoding, body, has_bom) = match (forced, bom) {
        // A BOM is only skipped if it matches the encoding we're using.
        (Some(f), Some((e, len))) if f == e => (f, &bytes[len..], true),
        (Some(f), _) => (f, bytes, false),
        (None, Some((e, len))) => (e, &bytes[len..], true),
        (None, None) => (detect(bytes), bytes, false),
    };
    let (text, had_errors) = encoding.decode_without_bom_handling(body);
    let (text, line_ending) = normalize_line_endings(&text);
    Decoded {
        text,
        format: FileFormat {
            encoding,
            bom: has_bom,
            line_ending,
        },
        had_errors,
    }
}

/// Guess the encoding of bytes without a BOM.
fn detect(bytes: &[u8]) -> &'static Encoding {
    if std::str::from_utf8(bytes).is_ok() {
        return encoding_rs::UTF_8;
    }
    use chardetng::{EncodingDetector, Iso2022JpDetection, Utf8Detection};
    // ISO-2022-JP is 7-bit, so it would already have passed as UTF-8 above.
    let mut detector = EncodingDetector::new(Iso2022JpDetection::Deny);
    detector.feed(bytes, true);
    detector.guess(None, Utf8Detection::Allow)
}

/// Convert all line endings to `\n`, returning the most common original style.
/// (A lone `\r`, the pre-2001 Mac style, is treated as a line break too.)
fn normalize_line_endings(text: &str) -> (String, LineEnding) {
    let crlf = text.matches("\r\n").count();
    let lf = text.matches('\n').count() - crlf;
    let style = if crlf > lf { LineEnding::CrLf } else { LineEnding::Lf };
    if !text.contains('\r') {
        return (text.to_owned(), style);
    }
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    (normalized, style)
}

/// Some characters in the text can't be represented in the target encoding.
#[derive(Debug, PartialEq, Eq)]
pub struct Unmappable {
    pub encoding: &'static Encoding,
}

/// Encode text (with `\n` line endings) for writing to disk.
pub fn encode(text: &str, format: &FileFormat) -> Result<Vec<u8>, Unmappable> {
    let text = match format.line_ending {
        LineEnding::Lf => std::borrow::Cow::Borrowed(text),
        LineEnding::CrLf => std::borrow::Cow::Owned(text.replace('\n', LineEnding::CrLf.as_str())),
    };
    let enc = format.encoding;

    let mut out = Vec::with_capacity(text.len() + 3);
    if format.bom {
        out.extend_from_slice(bom_bytes(enc));
    }
    // encoding_rs can't write UTF-16 (it's a decode-only "encoding" there),
    // so do that by hand.
    if enc == encoding_rs::UTF_16LE {
        out.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
    } else if enc == encoding_rs::UTF_16BE {
        out.extend(text.encode_utf16().flat_map(u16::to_be_bytes));
    } else {
        let (bytes, _, unmappable) = enc.encode(&text);
        if unmappable {
            return Err(Unmappable { encoding: enc });
        }
        out.extend_from_slice(&bytes);
    }
    Ok(out)
}

fn bom_bytes(enc: &'static Encoding) -> &'static [u8] {
    if enc == encoding_rs::UTF_8 {
        &[0xEF, 0xBB, 0xBF]
    } else if enc == encoding_rs::UTF_16LE {
        &[0xFF, 0xFE]
    } else if enc == encoding_rs::UTF_16BE {
        &[0xFE, 0xFF]
    } else {
        &[]
    }
}

/// Encodings offered in the File menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextEncoding {
    Utf8,
    Utf16Le,
    Utf16Be,
    Windows1252,
    MacRoman,
    Windows1250,
    Windows1251,
    Koi8R,
    Windows1253,
    Windows1254,
    Windows1255,
    Windows1256,
    ShiftJis,
    EucJp,
    Gbk,
    Big5,
    EucKr,
}

pub const ENCODINGS: &[TextEncoding] = &[
    TextEncoding::Utf8,
    TextEncoding::Utf16Le,
    TextEncoding::Utf16Be,
    TextEncoding::Windows1252,
    TextEncoding::MacRoman,
    TextEncoding::Windows1250,
    TextEncoding::Windows1251,
    TextEncoding::Koi8R,
    TextEncoding::Windows1253,
    TextEncoding::Windows1254,
    TextEncoding::Windows1255,
    TextEncoding::Windows1256,
    TextEncoding::ShiftJis,
    TextEncoding::EucJp,
    TextEncoding::Gbk,
    TextEncoding::Big5,
    TextEncoding::EucKr,
];

impl TextEncoding {
    pub fn encoding(self) -> &'static Encoding {
        use encoding_rs::*;
        match self {
            TextEncoding::Utf8 => UTF_8,
            TextEncoding::Utf16Le => UTF_16LE,
            TextEncoding::Utf16Be => UTF_16BE,
            TextEncoding::Windows1252 => WINDOWS_1252,
            TextEncoding::MacRoman => MACINTOSH,
            TextEncoding::Windows1250 => WINDOWS_1250,
            TextEncoding::Windows1251 => WINDOWS_1251,
            TextEncoding::Koi8R => KOI8_R,
            TextEncoding::Windows1253 => WINDOWS_1253,
            TextEncoding::Windows1254 => WINDOWS_1254,
            TextEncoding::Windows1255 => WINDOWS_1255,
            TextEncoding::Windows1256 => WINDOWS_1256,
            TextEncoding::ShiftJis => SHIFT_JIS,
            TextEncoding::EucJp => EUC_JP,
            TextEncoding::Gbk => GBK,
            TextEncoding::Big5 => BIG5,
            TextEncoding::EucKr => EUC_KR,
        }
    }

    pub fn from_encoding(enc: &'static Encoding) -> Option<Self> {
        ENCODINGS.iter().copied().find(|e| e.encoding() == enc)
    }

    pub fn label(self) -> &'static str {
        match self {
            TextEncoding::Utf8 => "Unicode (UTF-8)",
            TextEncoding::Utf16Le => "Unicode (UTF-16 LE)",
            TextEncoding::Utf16Be => "Unicode (UTF-16 BE)",
            TextEncoding::Windows1252 => "Western (Windows Latin 1)",
            TextEncoding::MacRoman => "Western (Mac OS Roman)",
            TextEncoding::Windows1250 => "Central European (Windows 1250)",
            TextEncoding::Windows1251 => "Cyrillic (Windows 1251)",
            TextEncoding::Koi8R => "Cyrillic (KOI8-R)",
            TextEncoding::Windows1253 => "Greek (Windows 1253)",
            TextEncoding::Windows1254 => "Turkish (Windows 1254)",
            TextEncoding::Windows1255 => "Hebrew (Windows 1255)",
            TextEncoding::Windows1256 => "Arabic (Windows 1256)",
            TextEncoding::ShiftJis => "Japanese (Shift JIS)",
            TextEncoding::EucJp => "Japanese (EUC-JP)",
            TextEncoding::Gbk => "Chinese Simplified (GBK)",
            TextEncoding::Big5 => "Chinese Traditional (Big5)",
            TextEncoding::EucKr => "Korean (EUC-KR)",
        }
    }

    pub fn short_name(self) -> &'static str {
        match self {
            TextEncoding::Utf8 => "UTF-8",
            TextEncoding::Utf16Le => "UTF-16 LE",
            TextEncoding::Utf16Be => "UTF-16 BE",
            TextEncoding::Windows1252 => "Windows Latin 1",
            TextEncoding::MacRoman => "Mac OS Roman",
            TextEncoding::Windows1250 => "Windows 1250",
            TextEncoding::Windows1251 => "Windows 1251",
            TextEncoding::Koi8R => "KOI8-R",
            TextEncoding::Windows1253 => "Windows 1253",
            TextEncoding::Windows1254 => "Windows 1254",
            TextEncoding::Windows1255 => "Windows 1255",
            TextEncoding::Windows1256 => "Windows 1256",
            TextEncoding::ShiftJis => "Shift JIS",
            TextEncoding::EucJp => "EUC-JP",
            TextEncoding::Gbk => "GBK",
            TextEncoding::Big5 => "Big5",
            TextEncoding::EucKr => "EUC-KR",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(bytes: &[u8]) {
        let d = decode(bytes, None);
        assert!(!d.had_errors);
        assert_eq!(encode(&d.text, &d.format).unwrap(), bytes, "format {:?}", d.format);
    }

    #[test]
    fn utf8_with_and_without_bom_round_trips() {
        round_trip("héllo\nworld".as_bytes());
        round_trip(b"\xEF\xBB\xBFh\xC3\xA9llo\n");
        let d = decode(b"\xEF\xBB\xBFhi", None);
        assert_eq!(d.text, "hi");
        assert!(d.format.bom);
    }

    #[test]
    fn utf16_with_bom_round_trips() {
        let mut le = vec![0xFF, 0xFE];
        le.extend("héllo\r\nok".encode_utf16().flat_map(u16::to_le_bytes));
        round_trip(&le);
        let d = decode(&le, None);
        assert_eq!(d.text, "héllo\nok");
        assert_eq!(d.format.encoding, encoding_rs::UTF_16LE);
        assert_eq!(d.format.line_ending, LineEnding::CrLf);
    }

    #[test]
    fn detects_legacy_encoding_and_saves_it_back() {
        // "café crème" in Windows-1252: not valid UTF-8.
        let bytes = b"caf\xE9 cr\xE8me, d\xE9j\xE0 vu, \xE9t\xE9 \xE0 la plage";
        let d = decode(bytes, None);
        assert_eq!(d.format.encoding, encoding_rs::WINDOWS_1252);
        assert!(d.text.starts_with("café crème"));
        round_trip(bytes);
    }

    #[test]
    fn crlf_files_keep_crlf() {
        let d = decode(b"a\r\nb\r\nc", None);
        assert_eq!(d.text, "a\nb\nc");
        assert_eq!(d.format.line_ending, LineEnding::CrLf);
        round_trip(b"a\r\nb\r\nc");
    }

    #[test]
    fn mixed_line_endings_use_the_majority() {
        let d = decode(b"a\nb\nc\r\nd", None);
        assert_eq!(d.text, "a\nb\nc\nd");
        assert_eq!(d.format.line_ending, LineEnding::Lf);
    }

    #[test]
    fn forced_encoding_overrides_detection() {
        let d = decode("é".as_bytes(), Some(encoding_rs::WINDOWS_1252));
        assert_eq!(d.text, "Ã©");
    }

    #[test]
    fn invalid_bytes_are_flagged() {
        let d = decode(b"ok \xFF\xFE\xFD", Some(encoding_rs::UTF_8));
        assert!(d.had_errors);
        assert!(d.text.contains('\u{FFFD}'));
    }

    #[test]
    fn unmappable_characters_are_reported() {
        let format = FileFormat {
            encoding: encoding_rs::WINDOWS_1252,
            ..FileFormat::default()
        };
        assert!(encode("café", &format).is_ok());
        assert_eq!(
            encode("snow ☃", &format),
            Err(Unmappable { encoding: encoding_rs::WINDOWS_1252 })
        );
    }

    #[test]
    fn summary_names_the_format() {
        let d = decode(b"\xEF\xBB\xBFa\r\nb", None);
        assert_eq!(d.format.summary(), "UTF-8 with BOM · CRLF");
    }
}
