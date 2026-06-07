//! WASM frontend entry point.
//!
//! Compiles to a `.wasm` module that the browser loads. `run()` runs once
//! on startup (like a top-level `<script type="module">`): it wires up the
//! "Add note" button, then kicks off an async load of the note list.
//!
//! The whole thing is the Rust analogue of a tiny vanilla-JS SPA: fetch
//! JSON, build DOM nodes, re-render on change.

mod api;
mod dom;
mod models;

use std::rc::Rc;

use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;
use web_sys::Document;

use crate::models::{CreateNote, Note};

/// `#[wasm_bindgen(start)]` marks the function the runtime calls
/// automatically once the module is instantiated — no manual init from JS.
#[wasm_bindgen(start)]
pub fn run() {
    // Route Rust panics to the browser console with a readable stack,
    // instead of an opaque "unreachable" trap.
    console_error_panic_hook::set_once();

    let doc = dom::document();

    // Wire up the "Add note" button. The closure captures `doc` (cheaply
    // cloneable — it's a handle) and spawns an async task on click.
    let add_button = dom::get_by_id(&doc, "add-btn");
    let doc_for_click = doc.clone();
    dom::on_click(&add_button, move || {
        let doc = doc_for_click.clone();
        spawn_local(async move {
            submit_new_note(&doc).await;
        });
    });

    // Initial load: fetch existing notes and render them.
    spawn_local(async move {
        refresh_notes(&doc).await;
    });
}

/// Read the form, POST a new note, clear the form, then re-render.
async fn submit_new_note(doc: &Document) {
    let title = dom::input_value(doc, "title-input");
    let body = dom::textarea_value(doc, "body-input");

    if title.is_empty() {
        set_status(doc, "Title is required.");
        return;
    }

    set_status(doc, "Saving...");
    let payload = CreateNote { title, body };

    match api::create_note(&payload).await {
        Ok(_) => {
            dom::clear_input(doc, "title-input");
            dom::clear_textarea(doc, "body-input");
            set_status(doc, "");
            refresh_notes(doc).await;
        }
        Err(err) => set_status(doc, &format!("Failed to save: {err}")),
    }
}

/// Fetch all notes from the API and rebuild the list in the DOM.
async fn refresh_notes(doc: &Document) {
    match api::fetch_notes().await {
        Ok(notes) => render_notes(doc, &notes),
        Err(err) => set_status(doc, &format!("Failed to load notes: {err}")),
    }
}

/// Replace the contents of `#notes` with a freshly built list.
fn render_notes(doc: &Document, notes: &[Note]) {
    let list = dom::get_by_id(doc, "notes");
    list.set_inner_html(""); // clear previous render

    if notes.is_empty() {
        let empty = doc
            .create_element("p")
            .expect("create <p> failed");
        empty.set_text_content(Some("No notes yet. Add one above."));
        list.append_child(&empty).expect("append failed");
        return;
    }

    for note in notes {
        list.append_child(&note_element(doc, note))
            .expect("append note failed");
    }
}

/// Build a single note `<li>` with a title, body, and delete button.
fn note_element(doc: &Document, note: &Note) -> web_sys::Element {
    let item = doc.create_element("li").expect("create <li> failed");
    item.set_class_name("note");

    let title = doc.create_element("h3").expect("create <h3> failed");
    title.set_text_content(Some(&note.title));
    item.append_child(&title).expect("append title failed");

    if !note.body.is_empty() {
        let body = doc.create_element("p").expect("create <p> failed");
        body.set_text_content(Some(&note.body));
        item.append_child(&body).expect("append body failed");
    }

    // Render the server-assigned timestamp via the browser's own
    // `Date` (js-sys), so we use the `created_at` field the API returns.
    let meta = doc.create_element("small").expect("create <small> failed");
    let when = js_sys::Date::new(&JsValue::from_f64(note.created_at as f64));
    let formatted: String = when
        .to_locale_string("en-US", &JsValue::UNDEFINED)
        .into();
    meta.set_text_content(Some(&formatted));
    item.append_child(&meta).expect("append meta failed");

    let del = doc.create_element("button").expect("create button failed");
    del.set_text_content(Some("Delete"));
    del.set_class_name("delete");

    // `Rc` lets the click closure and the surrounding code share the same
    // document handle without fighting the borrow checker.
    let doc_rc = Rc::new(doc.clone());
    let id = note.id;
    dom::on_click(&del, move || {
        let doc = doc_rc.clone();
        spawn_local(async move {
            if api::delete_note(id).await.is_ok() {
                refresh_notes(&doc).await;
            } else {
                set_status(&doc, "Failed to delete note.");
            }
        });
    });
    item.append_child(&del).expect("append delete failed");

    item
}

/// Write a short message into the `#status` line.
fn set_status(doc: &Document, message: &str) {
    if let Some(el) = doc.get_element_by_id("status") {
        el.set_text_content(Some(message));
    }
}
