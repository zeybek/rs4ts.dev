//! Headless-browser tests for the Wasm build.
//!
//! Run with: `wasm-pack test --headless --firefox` (or `--chrome`).
//! These exercise the same public API the browser uses, but inside a real
//! Wasm runtime — so things like `js_sys::Math::random()` actually work here,
//! unlike the native `cargo test` in `src/universe.rs`.

use game_of_life::Universe;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn blinker_oscillates_in_wasm() {
    let mut u = Universe::new(5, 5);
    u.clear();
    u.toggle_cell(2, 1);
    u.toggle_cell(2, 2);
    u.toggle_cell(2, 3);

    let horizontal = u.render();
    u.tick();
    let vertical = u.render();
    u.tick();
    let back_to_horizontal = u.render();

    assert_ne!(horizontal, vertical, "blinker should change shape after one tick");
    assert_eq!(horizontal, back_to_horizontal, "period-2 oscillator");
}

#[wasm_bindgen_test]
fn randomize_uses_js_math_random() {
    // This would panic under native `cargo test` (no JS engine), proving these
    // tests really run inside the browser's Wasm runtime.
    let mut u = Universe::new(16, 16);
    u.randomize();
    let alive = u.render().chars().filter(|&c| c == '\u{25FC}').count();
    assert!(alive > 0, "a random 16x16 board should have some live cells");
}
