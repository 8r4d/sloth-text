//! Printing through the standard macOS print dialog (which also offers
//! Save as PDF): the text is laid out in an off-screen native text view and
//! handed to `NSPrintOperation`.

use objc2::{MainThreadMarker, MainThreadOnly};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{
    NSFont, NSPrintHeaderAndFooter, NSPrintInfo, NSPrintOperation, NSPrintingPaginationMode,
    NSTextView,
};
use objc2_foundation::{NSNumber, NSPoint, NSRect, NSSize, NSString};

/// Point size used for printed text.
const PRINT_FONT_SIZE: f64 = 10.0;

/// Show the print dialog for `text`. `font_family` is the editor font, if
/// one was chosen (otherwise the system's monospaced font is used).
/// Returns false if printing isn't possible here or the user cancelled.
pub fn print(title: &str, text: &str, font_family: Option<&str>) -> bool {
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
    view.setString(&NSString::from_str(text));
    view.setFont(Some(&font(font_family)));
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
