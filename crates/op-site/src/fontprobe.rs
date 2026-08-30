//! Reports whether a font family actually resolves in this browser.
//!
//! `document.fonts.check()` only answers for faces registered in the
//! `FontFaceSet`; for any other family it returns true, which made badges lie
//! for locally-installed (or missing) fonts. This probe measures instead: the
//! same test string is laid out with the candidate family in front of two
//! different generic fallbacks; if the candidate resolves, both measurements
//! agree with each other and differ from at least one bare fallback.

use wasm_bindgen::JsCast;
use web_sys::HtmlElement;

const TEST_TEXT: &str = "OpenPOWER firmware AaGgKkQq 0123 mmmwwwiii";

fn width_with(family_list: &str) -> Option<i32> {
    let document = web_sys::window()?.document()?;
    let body = document.body()?;
    let span: HtmlElement = document.create_element("span").ok()?.dyn_into().ok()?;
    span.set_attribute(
        "style",
        &format!(
            "position:absolute;left:-9999px;top:-9999px;visibility:hidden;white-space:pre;font-size:48px;font-family:{family_list}"
        ),
    )
    .ok()?;
    span.set_text_content(Some(TEST_TEXT));
    body.append_child(&span).ok()?;
    let width = span.offset_width();
    span.remove();
    Some(width)
}

/// True when `family` (a bare family name, not a stack) resolves to an actual
/// font in this browser, whether from the embedded set or a local install.
pub fn family_resolves(family: &str) -> bool {
    let quoted = format!("'{}'", family.replace(['\'', '"'], ""));
    let (Some(serif), Some(mono)) = (width_with("serif"), width_with("monospace")) else {
        return false;
    };
    let (Some(with_serif), Some(with_mono)) = (
        width_with(&format!("{quoted}, serif")),
        width_with(&format!("{quoted}, monospace")),
    ) else {
        return false;
    };
    with_serif == with_mono && (with_serif != serif || with_mono != mono)
}
