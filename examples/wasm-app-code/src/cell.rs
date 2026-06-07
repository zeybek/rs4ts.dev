//! The state of a single cell on the board.

use wasm_bindgen::prelude::*;

/// A cell is either `Dead` or `Alive`.
///
/// We give it an explicit `u8` representation so the values are stable across
/// the JS/Wasm boundary and so we can do arithmetic on them when counting live
/// neighbors (a dead cell counts as 0, a live one as 1).
///
/// In TypeScript you might model this as `type Cell = 0 | 1` or an enum; here
/// the `#[repr(u8)]` makes the Rust enum behave like that union but with real
/// type-checking and pattern-matching.
#[wasm_bindgen]
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cell {
    Dead = 0,
    Alive = 1,
}

impl Cell {
    /// Flip a cell between alive and dead. Used when the user clicks a cell.
    pub fn toggle(&mut self) {
        *self = match *self {
            Cell::Dead => Cell::Alive,
            Cell::Alive => Cell::Dead,
        };
    }
}
