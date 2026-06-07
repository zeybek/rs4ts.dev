//! Draw a [`Universe`] onto an HTML `<canvas>` 2D context, entirely from Rust
//! via `web-sys`. This shows the *other* WASM rendering strategy: instead of
//! letting JavaScript read the cell buffer and draw, Rust drives the canvas
//! API directly. Both approaches ship in this project; the JS glue picks one.

use wasm_bindgen::prelude::*;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};

use crate::cell::Cell;
use crate::universe::Universe;

const CELL_SIZE: f64 = 8.0; // pixels per cell
const GRID_COLOR: &str = "#cccccc";
const DEAD_COLOR: &str = "#ffffff";
const ALIVE_COLOR: &str = "#1a1a1a";

/// Owns a canvas + its 2D context and knows how to paint a `Universe`.
#[wasm_bindgen]
pub struct CanvasRenderer {
    ctx: CanvasRenderingContext2d,
}

#[wasm_bindgen]
impl CanvasRenderer {
    /// Look up a `<canvas>` by `id`, size it to fit `universe`, and grab its
    /// 2D context. Returns a `Result` so a missing element surfaces as a JS
    /// exception instead of a panic.
    #[wasm_bindgen(constructor)]
    pub fn new(canvas_id: &str, universe: &Universe) -> Result<CanvasRenderer, JsValue> {
        let document = web_sys::window()
            .ok_or_else(|| JsValue::from_str("no global `window` exists"))?
            .document()
            .ok_or_else(|| JsValue::from_str("should have a document on window"))?;

        let canvas = document
            .get_element_by_id(canvas_id)
            .ok_or_else(|| JsValue::from_str(&format!("no element with id `{canvas_id}`")))?
            .dyn_into::<HtmlCanvasElement>()?;

        // +1 leaves room for the 1px grid lines around each cell.
        let width = universe.width();
        let height = universe.height();
        canvas.set_width(((CELL_SIZE as u32 + 1) * width) + 1);
        canvas.set_height(((CELL_SIZE as u32 + 1) * height) + 1);

        let ctx = canvas
            .get_context("2d")?
            .ok_or_else(|| JsValue::from_str("failed to get 2d context"))?
            .dyn_into::<CanvasRenderingContext2d>()?;

        Ok(CanvasRenderer { ctx })
    }

    /// Repaint the whole board: grid lines first, then the cells.
    pub fn draw(&self, universe: &Universe) {
        self.draw_grid(universe);
        self.draw_cells(universe);
    }

    fn draw_grid(&self, universe: &Universe) {
        let width = universe.width();
        let height = universe.height();
        let step = CELL_SIZE + 1.0;

        self.ctx.begin_path();
        self.ctx.set_stroke_style_str(GRID_COLOR);

        // Vertical lines.
        for i in 0..=width {
            let x = i as f64 * step + 1.0;
            self.ctx.move_to(x, 0.0);
            self.ctx.line_to(x, step * height as f64 + 1.0);
        }
        // Horizontal lines.
        for j in 0..=height {
            let y = j as f64 * step + 1.0;
            self.ctx.move_to(0.0, y);
            self.ctx.line_to(step * width as f64 + 1.0, y);
        }

        self.ctx.stroke();
    }

    fn draw_cells(&self, universe: &Universe) {
        let width = universe.width();
        let step = CELL_SIZE + 1.0;

        // Borrow the cell buffer safely. (The raw-pointer `cells()` exists for
        // the JS-reads-Wasm-memory path; native Rust has no need for `unsafe`.)
        let cells: &[Cell] = universe.cells_slice();

        self.ctx.begin_path();

        // Batch by color: set fill once, draw all dead cells, then all alive.
        // Switching fillStyle is the expensive canvas call, so we minimize it.
        self.ctx.set_fill_style_str(ALIVE_COLOR);
        for (idx, &cell) in cells.iter().enumerate() {
            if cell != Cell::Alive {
                continue;
            }
            let row = idx as u32 / width;
            let col = idx as u32 % width;
            self.ctx.fill_rect(
                col as f64 * step + 1.0,
                row as f64 * step + 1.0,
                CELL_SIZE,
                CELL_SIZE,
            );
        }

        self.ctx.set_fill_style_str(DEAD_COLOR);
        for (idx, &cell) in cells.iter().enumerate() {
            if cell == Cell::Alive {
                continue;
            }
            let row = idx as u32 / width;
            let col = idx as u32 % width;
            self.ctx.fill_rect(
                col as f64 * step + 1.0,
                row as f64 * step + 1.0,
                CELL_SIZE,
                CELL_SIZE,
            );
        }

        self.ctx.stroke();
    }
}
