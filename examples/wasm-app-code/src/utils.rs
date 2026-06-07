//! Small helpers shared across the crate.

/// Install a panic hook so a Rust `panic!` shows a readable message and stack
/// trace in the browser's devtools console instead of the cryptic
/// "unreachable executed" you get by default. Safe to call more than once.
pub fn set_panic_hook() {
    console_error_panic_hook::set_once();
}

/// `console.log!(...)` — a `println!`-style macro that writes to the browser
/// console via `web_sys`. Handy for debugging from Rust without reaching for
/// JavaScript glue. Mirrors `console.log(...)` in Node/browser code.
#[macro_export]
macro_rules! log {
    ( $( $t:tt )* ) => {
        web_sys::console::log_1(&format!( $( $t )* ).into());
    };
}
