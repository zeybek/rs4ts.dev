//! The Game of Life board (`Universe`) and its evolution rules.
//!
//! This is plain, allocation-light Rust: a flat `Vec<Cell>` plus width/height.
//! Everything marked `#[wasm_bindgen]` is callable from JavaScript; everything
//! else stays private to Rust. The simulation never touches the DOM — keeping
//! the rules pure makes them trivial to unit-test natively (see the `tests`
//! module at the bottom).

use wasm_bindgen::prelude::*;

use crate::cell::Cell;

/// The toroidal Game of Life board.
///
/// Cells are stored row-major in a single flat `Vec<Cell>` (index = `row *
/// width + column`) rather than a `Vec<Vec<Cell>>`. A flat buffer is one
/// contiguous allocation — cache-friendly and, crucially, it can be handed to
/// JavaScript as a raw pointer + length with zero copying (see [`Self::cells`]).
#[wasm_bindgen]
pub struct Universe {
    width: u32,
    height: u32,
    cells: Vec<Cell>,
    // Scratch buffer reused every tick so we don't allocate on each frame.
    next: Vec<Cell>,
}

#[wasm_bindgen]
impl Universe {
    /// Build a `width` x `height` board seeded with a classic deterministic
    /// pattern (every cell where `i % 2 == 0 || i % 7 == 0` starts alive).
    /// Deterministic seeding makes the first frame reproducible across runs.
    #[wasm_bindgen(constructor)]
    pub fn new(width: u32, height: u32) -> Universe {
        // A board must be at least 1x1; clamp degenerate sizes so the toroidal
        // neighbor math below never underflows on a zero dimension.
        let width = width.max(1);
        let height = height.max(1);
        // Cast each operand before multiplying so a large width*height widens to
        // usize instead of overflowing u32.
        let len = width as usize * height as usize;
        let cells = (0..len)
            .map(|i| {
                if i % 2 == 0 || i % 7 == 0 {
                    Cell::Alive
                } else {
                    Cell::Dead
                }
            })
            .collect();

        Universe {
            width,
            height,
            cells,
            next: vec![Cell::Dead; len],
        }
    }

    /// Advance the simulation by one generation, applying Conway's four rules.
    pub fn tick(&mut self) {
        for row in 0..self.height {
            for col in 0..self.width {
                let idx = self.get_index(row, col);
                let cell = self.cells[idx];
                let live_neighbors = self.live_neighbor_count(row, col);

                // Conway's rules. `match` on a tuple expresses them as a
                // truth table — far clearer than a chain of `if`s.
                self.next[idx] = match (cell, live_neighbors) {
                    // Underpopulation: a live cell with < 2 neighbors dies.
                    (Cell::Alive, n) if n < 2 => Cell::Dead,
                    // Survival: a live cell with 2 or 3 neighbors lives on.
                    (Cell::Alive, 2) | (Cell::Alive, 3) => Cell::Alive,
                    // Overpopulation: a live cell with > 3 neighbors dies.
                    (Cell::Alive, n) if n > 3 => Cell::Dead,
                    // Reproduction: a dead cell with exactly 3 neighbors is born.
                    (Cell::Dead, 3) => Cell::Alive,
                    // Everything else keeps its state.
                    (state, _) => state,
                };
            }
        }

        // Swap the buffers: `next` becomes current, the old current becomes the
        // scratch buffer for the following tick. No allocation, no copy.
        std::mem::swap(&mut self.cells, &mut self.next);
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// Return a raw pointer to the cell buffer. JavaScript reads the cells
    /// directly out of the Wasm linear memory with this pointer + `width *
    /// height` — no per-cell function calls, no serialization. This is the
    /// key trick that makes the Rust version fast.
    pub fn cells(&self) -> *const Cell {
        self.cells.as_ptr()
    }

    /// Toggle a single cell (called when the user clicks the canvas).
    pub fn toggle_cell(&mut self, row: u32, col: u32) {
        let idx = self.get_index(row, col);
        self.cells[idx].toggle();
    }

    /// Kill every cell. Useful for drawing your own patterns from scratch.
    pub fn clear(&mut self) {
        for cell in self.cells.iter_mut() {
            *cell = Cell::Dead;
        }
    }

    /// Reseed the whole board randomly. Uses `Math.random()` from JavaScript
    /// (via `js_sys`) so we don't need a Rust RNG crate in the Wasm build.
    pub fn randomize(&mut self) {
        for cell in self.cells.iter_mut() {
            *cell = if js_sys::Math::random() < 0.5 {
                Cell::Alive
            } else {
                Cell::Dead
            };
        }
    }

    /// Stamp a glider (a small pattern that walks across the board) with its
    /// top-left corner at (`row`, `col`). Nice for the "Insert glider" button.
    pub fn insert_glider(&mut self, row: u32, col: u32) {
        // Relative coordinates of the canonical glider, then set them alive.
        const GLIDER: [(u32, u32); 5] = [(0, 1), (1, 2), (2, 0), (2, 1), (2, 2)];
        for (dr, dc) in GLIDER {
            let r = (row + dr) % self.height;
            let c = (col + dc) % self.width;
            let idx = self.get_index(r, c);
            self.cells[idx] = Cell::Alive;
        }
    }

    /// Render the board to an ASCII string (`◻` dead, `◼` alive). Handy in the
    /// browser and reused by the native tests below. `to_string` via `Display`
    /// would also work, but a named method is friendlier to call from JS.
    pub fn render(&self) -> String {
        let mut buf = String::with_capacity(self.cells.len() + self.height as usize);
        for line in self.cells.chunks(self.width as usize) {
            for &cell in line {
                buf.push(if cell == Cell::Alive {
                    '\u{25FC}'
                } else {
                    '\u{25FB}'
                });
            }
            buf.push('\n');
        }
        buf
    }
}

// --- Methods NOT exported to JavaScript (no `#[wasm_bindgen]`) ---
impl Universe {
    /// Flatten a (row, col) coordinate into an index into the flat `cells` Vec.
    fn get_index(&self, row: u32, column: u32) -> usize {
        (row * self.width + column) as usize
    }

    /// Safe, read-only view of the cell buffer for native Rust consumers (e.g.
    /// the canvas renderer). JavaScript instead uses the raw-pointer `cells()`
    /// to read the buffer straight out of Wasm memory with no copy.
    pub fn cells_slice(&self) -> &[Cell] {
        &self.cells
    }

    /// Count the live cells in the 8-neighborhood, wrapping around the edges
    /// (the board is a torus). The `+ self.height - 1` / `% self.height` dance
    /// avoids underflow on `u32` when we're on row/column 0.
    fn live_neighbor_count(&self, row: u32, column: u32) -> u8 {
        let mut count = 0;
        for delta_row in [self.height - 1, 0, 1] {
            for delta_col in [self.width - 1, 0, 1] {
                if delta_row == 0 && delta_col == 0 {
                    continue; // skip the cell itself
                }
                let neighbor_row = (row + delta_row) % self.height;
                let neighbor_col = (column + delta_col) % self.width;
                let idx = self.get_index(neighbor_row, neighbor_col);
                count += self.cells[idx] as u8;
            }
        }
        count
    }

    /// Read-only view of the cells. Used by tests; not exported to JS.
    #[cfg(test)]
    pub fn get_cells(&self) -> &[Cell] {
        &self.cells
    }

    /// Force a specific set of cells alive (test helper).
    #[cfg(test)]
    pub fn set_cells(&mut self, alive: &[(u32, u32)]) {
        for &(r, c) in alive {
            let idx = self.get_index(r, c);
            self.cells[idx] = Cell::Alive;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::Cell;

    /// A blinker (3 cells in a row) oscillates with period 2: horizontal ->
    /// vertical -> horizontal. This is the canonical correctness test.
    #[test]
    fn blinker_oscillates() {
        let mut u = Universe::new(5, 5);
        u.clear();
        // Horizontal blinker centered in a 5x5 board.
        u.set_cells(&[(2, 1), (2, 2), (2, 3)]);

        u.tick();
        // After one tick it should be vertical.
        let expected_vertical = {
            let mut v = Universe::new(5, 5);
            v.clear();
            v.set_cells(&[(1, 2), (2, 2), (3, 2)]);
            v
        };
        assert_eq!(u.get_cells(), expected_vertical.get_cells());

        u.tick();
        // After two ticks it's back to horizontal.
        let expected_horizontal = {
            let mut v = Universe::new(5, 5);
            v.clear();
            v.set_cells(&[(2, 1), (2, 2), (2, 3)]);
            v
        };
        assert_eq!(u.get_cells(), expected_horizontal.get_cells());
    }

    #[test]
    fn lonely_cell_dies() {
        let mut u = Universe::new(5, 5);
        u.clear();
        u.set_cells(&[(2, 2)]); // single cell, 0 neighbors
        u.tick();
        assert!(u.get_cells().iter().all(|&c| c == Cell::Dead));
    }

    #[test]
    fn block_is_stable() {
        // A 2x2 block is a "still life": every cell has exactly 3 neighbors.
        let mut u = Universe::new(4, 4);
        u.clear();
        u.set_cells(&[(1, 1), (1, 2), (2, 1), (2, 2)]);
        let before: Vec<Cell> = u.get_cells().to_vec();
        u.tick();
        assert_eq!(u.get_cells(), before.as_slice());
    }

    #[test]
    fn degenerate_dimensions_do_not_underflow() {
        // A zero dimension is clamped to at least 1x1, so the toroidal neighbor
        // math can't underflow `u32` (which would panic in debug). Ticking a
        // 0x0 and a 1x1 board must both be panic-free.
        let mut zero = Universe::new(0, 0);
        assert!(zero.width() >= 1 && zero.height() >= 1);
        zero.tick();

        let mut tiny = Universe::new(1, 1);
        tiny.tick();
        assert_eq!(tiny.cells_slice().len(), 1);
    }

    #[test]
    fn toggle_flips_state() {
        let mut u = Universe::new(3, 3);
        u.clear();
        assert_eq!(u.get_cells()[u.get_index(1, 1)], Cell::Dead);
        u.toggle_cell(1, 1);
        assert_eq!(u.get_cells()[u.get_index(1, 1)], Cell::Alive);
        u.toggle_cell(1, 1);
        assert_eq!(u.get_cells()[u.get_index(1, 1)], Cell::Dead);
    }
}
