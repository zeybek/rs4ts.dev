//! DOM helpers built on `web-sys`. These are the typed Rust equivalents
//! of `document.getElementById`, `el.textContent = ...`, and
//! `el.addEventListener(...)` — the difference is that every call that
//! could fail returns a `Result`/`Option` you must handle.

use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::Closure;
use web_sys::{Document, Element, HtmlInputElement, HtmlTextAreaElement, window};

/// Grab `document`, panicking with a clear message if we're somehow not
/// in a browser. In a real app you'd surface this more gracefully.
pub fn document() -> Document {
    window()
        .expect("no global window (are we in a browser?)")
        .document()
        .expect("window has no document")
}

/// `document.getElementById`, but returns a typed `Element`.
pub fn get_by_id(doc: &Document, id: &str) -> Element {
    doc.get_element_by_id(id)
        .unwrap_or_else(|| panic!("missing #{id} in the DOM"))
}

/// Read the trimmed value out of an `<input>` by id.
pub fn input_value(doc: &Document, id: &str) -> String {
    get_by_id(doc, id)
        .dyn_into::<HtmlInputElement>()
        .expect("element is not an <input>")
        .value()
        .trim()
        .to_string()
}

/// Read the trimmed value out of a `<textarea>` by id.
pub fn textarea_value(doc: &Document, id: &str) -> String {
    get_by_id(doc, id)
        .dyn_into::<HtmlTextAreaElement>()
        .expect("element is not a <textarea>")
        .value()
        .trim()
        .to_string()
}

/// Clear the value of an `<input>` by id (after a successful submit).
pub fn clear_input(doc: &Document, id: &str) {
    if let Some(el) = doc.get_element_by_id(id) {
        if let Ok(input) = el.dyn_into::<HtmlInputElement>() {
            input.set_value("");
        }
    }
}

/// Clear the value of a `<textarea>` by id.
pub fn clear_textarea(doc: &Document, id: &str) {
    if let Some(el) = doc.get_element_by_id(id) {
        if let Ok(area) = el.dyn_into::<HtmlTextAreaElement>() {
            area.set_value("");
        }
    }
}

/// Attach a click listener. We `forget()` the closure so it lives for the
/// lifetime of the page — the wasm equivalent of not dropping a JS
/// callback you still need. (For dynamic UIs you'd store and reuse these.)
pub fn on_click<F: 'static + FnMut()>(element: &Element, mut handler: F) {
    let closure = Closure::<dyn FnMut()>::new(move || handler());
    element
        .add_event_listener_with_callback("click", closure.as_ref().unchecked_ref())
        .expect("failed to attach click listener");
    closure.forget();
}
