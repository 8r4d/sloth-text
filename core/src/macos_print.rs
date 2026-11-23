//! Printing through the standard macOS print dialog (which also offers
//! Save as PDF): the text is laid out in an off-screen native text view and
//! handed to `NSPrintOperation`, coloured and numbered as print_text.rs
//! lays it out.

use objc2::{MainThreadMarker, MainThreadOnly};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{
    NSColor, NSFont, NSForegroundColorAttributeName, NSMutableParagraphStyle, NSParagraphStyleAttributeName,
    NSPrintHeaderAndFooter, NSPrintInfo, NSPrintOperation, NSPrintingPaginationMode, NSTextView,
};
use objc2_foundation::{NSArray, NSNumber, NSPoint, NSRange, NSRect, NSSize, NSString};

use crate::print_text::PrintText;

/// Point size used for printed text.
const PRINT_FONT_SIZE: f64 = 10.0;

/// Show the print dialog for `page` (see print_text.rs). `font_family` is
/// the editor font, if one was chosen (otherwise the system's monospaced
/// font is used). Returns false if printing isn't possible here or the user
/// cancelled.
pub fn print(title: &str, page: &PrintText, font_family: Option<&str>) -> bool {
    let Some(mtm) = MainThreadMarker::new() else {
        return false;
    };

    let info = NSPrintInfo::sharedPrintInfo();
    info.setHorizontalPagination(NSPrintingPaginationMode::Fit);
    info.setVerticallyCentered(false);
    // File name, date and page numbers at the top and bottom of each page.
    unsafe {
        info.dictionary().setObject_forKey(
            &NSNumber::new_bool(true),
            ProtocolObject::from_ref(NSPrintHeaderAndFooter),
        );
    }

    // A text view as wide as the printable area; it grows to fit the text.
    let paper = info.paperSize();
    let width = paper.width - info.leftMargin() - info.rightMargin();
    let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(width, paper.height));
    let view = NSTextView::initWithFrame(NSTextView::alloc(mtm), frame);
    view.setEditable(false);
    view.setRichText(false);
    view.setVerticallyResizable(true);
    view.setString(&NSString::from_str(&page.text));
    view.setFont(Some(&font(font_family)));
    // Printed on paper: black text, whatever the screen's theme.
    view.setTextColor(Some(&NSColor::blackColor()));
    // SAFETY: the view (which keeps its storage alive) outlives this use.
    if let Some(storage) = unsafe { view.textStorage() } {
        storage.beginEditing();
        let whole = NSRange::new(0, page.text.encode_utf16().count());
        if page.number_digits > 0 {
            // The numbers in a column of their own: the text starts at the
            // tab after each, and wrapped lines line up with it.
            let column = (page.number_digits + 2) as f64 * PRINT_FONT_SIZE * 0.6;
            let style = NSMutableParagraphStyle::new();
            style.setTabStops(Some(&NSArray::new()));
            style.setDefaultTabInterval(column);
            style.setHeadIndent(column);
            // SAFETY: a paragraph style for the paragraph-style attribute.
            unsafe { storage.addAttribute_value_range(NSParagraphStyleAttributeName, &style, whole) };
        }
        for (range, color) in &page.runs {
            let [r, g, b, _] = color.to_array();
            let color = NSColor::colorWithSRGBRed_green_blue_alpha(r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0, 1.0);
            let range = NSRange::new(range.start, range.end - range.start);
            // SAFETY: a colour for the colour attribute, within the text.
            unsafe { storage.addAttribute_value_range(NSForegroundColorAttributeName, &color, range) };
        }
        storage.endEditing();
    }
    view.sizeToFit();

    let operation = NSPrintOperation::printOperationWithView_printInfo(&view, &info);
    operation.setJobTitle(Some(&NSString::from_str(title)));
    operation.setShowsPrintPanel(true);
    operation.runOperation()
}

fn font(family: Option<&str>) -> Retained<NSFont> {
    family
        .and_then(|name| NSFont::fontWithName_size(&NSString::from_str(name), PRINT_FONT_SIZE))
        // The fixed-width font chosen in macOS settings (normally Menlo).
        .or_else(|| NSFont::userFixedPitchFontOfSize(PRINT_FONT_SIZE))
        .unwrap_or_else(|| NSFont::systemFontOfSize(PRINT_FONT_SIZE))
}
