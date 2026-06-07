//! # Conway's Game of Life — Rust + WebAssembly
//!
//! A `cdylib` crate compiled to WebAssembly with `wasm-bindgen`. The browser
//! loads the generated glue (`wasm-pack build --target web`) and drives the
//! simulation from a little JavaScript file (`www/index.js`).
//!
//! Module map:
//! - [`cell`]      — the `Cell` enum (`Dead`/`Alive`).
//! - [`universe`]  — the `Universe` board + Conway's rules (pure, testable).
//! - [`render`]    — a `CanvasRenderer` that paints a `Universe` to a `<canvas>`.
//! - [`utils`]     — the panic hook and a `log!` macro.

#[macro_use]
mod utils;

mod cell;
mod render;
mod universe;

// Re-export the public types at the crate root so `wasm-bindgen` exposes them
// as top-level JS classes (`Universe`, `Cell`, `CanvasRenderer`).
pub use cell::Cell;
pub use render::CanvasRenderer;
pub use universe::Universe;

use wasm_bindgen::prelude::*;

/// Runs automatically when the Wasm module is instantiated (the
/// `#[wasm_bindgen(start)]` attribute is WebAssembly's equivalent of an ES
/// module's top-level side effect). We use it to install the panic hook so any
/// Rust panic prints a readable trace to the devtools console.
#[wasm_bindgen(start)]
pub fn start() {
    utils::set_panic_hook();
    log!("game-of-life wasm module initialized");
}

/// Expose the package version to JavaScript (a handy smoke test that the module
/// loaded and the JS<->Wasm bridge works).
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
