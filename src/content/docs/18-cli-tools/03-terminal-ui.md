---
title: "Terminal UIs with Ratatui"
description: "Build full-screen terminal apps in Rust with ratatui, whose immediate-mode draw loop mirrors React's UI-as-a-function-of-state, where Node would use Ink or blessed."
---

Build full-screen, interactive terminal applications in Rust with **ratatui**, the equivalent of Node's `blessed`, `ink`, or `terminal-kit`, but with an **immediate-mode** rendering model that will feel surprisingly close to React's render function.

---

## Quick Overview

A **TUI** (Terminal User Interface) is a full-screen, keyboard-driven app that runs inside your terminal: think `htop`, `lazygit`, `vim`, or `k9s`. In Rust, the dominant library is **ratatui** (the maintained successor to the unmaintained `tui-rs`). Its mental model is **immediate mode**: you do not build a persistent tree of widget objects and mutate them. Instead, on every frame you call a single `draw` closure that describes the *entire* screen from your application state, and ratatui diffs that description against the previous frame and writes only the changed cells to the terminal. If you have used React, this is the same idea as a `render()` function: UI as a pure function of state. You pair ratatui with a **backend** (almost always **crossterm**) that handles raw mode, the alternate screen, and keyboard/mouse events.

> **Note:** This page covers the ratatui rendering model, widgets, and the event loop. For colored *line-oriented* output (not full-screen), see [Colored Output](/18-cli-tools/05-colored-output/); for progress bars and spinners, see [Progress Bars](/18-cli-tools/04-progress-bars/).

---

## TypeScript/JavaScript Example

In the Node ecosystem, the closest analog is **Ink** — React for the terminal. You declare components, hold state with hooks, and Ink re-renders when state changes. Here is a small task list with keyboard navigation:

```tsx
// app.tsx — run with: npx tsx app.tsx
// deps: ink, react, @types/react
import React, { useState } from "react";
import { render, Box, Text, useInput, useApp } from "ink";

const TASKS = ["Write docs", "Review PR", "Fix the build", "Ship it"];

function App() {
  const [selected, setSelected] = useState(0);
  const { exit } = useApp();

  useInput((input, key) => {
    if (input === "q" || key.escape) exit();
    if (key.downArrow || input === "j") setSelected((i) => (i + 1) % TASKS.length);
    if (key.upArrow || input === "k")
      setSelected((i) => (i === 0 ? TASKS.length - 1 : i - 1));
  });

  return (
    <Box flexDirection="column" borderStyle="round">
      {TASKS.map((task, i) => (
        <Text key={task} inverse={i === selected}>
          {i === selected ? "> " : "  "}
          {task}
        </Text>
      ))}
    </Box>
  );
}

render(<App />);
```

Ink hides the event loop entirely: `useState` triggers re-renders, `useInput` wires up keyboard handling, and React's reconciler decides what to repaint. The cost is a heavy dependency tree (React, Yoga layout engine, the reconciler) and the usual JavaScript runtime overhead.

---

## Rust Equivalent

Create a project and add the two crates. `cargo new` selects the newest stable edition automatically; the repository's [pinned verification toolchain](/00-introduction/05-version-policy/) uses the 2024 edition.

```bash
cargo new task_tui
cd task_tui
cargo add ratatui
cargo add crossterm
```

That writes the current versions into `Cargo.toml`:

```toml
[dependencies]
crossterm = "0.29.0"
ratatui = "0.30.0"
```

> **Note:** ratatui re-exports its default backend, so `cargo add ratatui` already pulls in crossterm transitively. We add `crossterm` explicitly because we use its event types (`KeyCode`, `Event`) directly.

Here is the same task list in `src/main.rs`:

```rust
use std::io;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::{
    DefaultTerminal, Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

struct App {
    tasks: Vec<String>,
    state: ListState,
    should_quit: bool,
}

impl App {
    fn new() -> Self {
        let mut state = ListState::default();
        state.select(Some(0));
        Self {
            tasks: vec![
                "Write docs".into(),
                "Review PR".into(),
                "Fix the build".into(),
                "Ship it".into(),
            ],
            state,
            should_quit: false,
        }
    }

    fn next(&mut self) {
        let i = self.state.selected().map_or(0, |i| (i + 1) % self.tasks.len());
        self.state.select(Some(i));
    }

    fn previous(&mut self) {
        let i = self.state.selected().map_or(0, |i| {
            if i == 0 { self.tasks.len() - 1 } else { i - 1 }
        });
        self.state.select(Some(i));
    }
}

fn main() -> io::Result<()> {
    let mut terminal = ratatui::init();
    let app = App::new();
    let result = run(&mut terminal, app);
    ratatui::restore();
    result
}

fn run(terminal: &mut DefaultTerminal, mut app: App) -> io::Result<()> {
    while !app.should_quit {
        terminal.draw(|frame| render(frame, &mut app))?;
        handle_events(&mut app)?;
    }
    Ok(())
}

fn handle_events(app: &mut App) -> io::Result<()> {
    if event::poll(Duration::from_millis(100))?
        && let Event::Key(key) = event::read()?
        && key.kind == KeyEventKind::Press
    {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
            KeyCode::Down | KeyCode::Char('j') => app.next(),
            KeyCode::Up | KeyCode::Char('k') => app.previous(),
            _ => {}
        }
    }
    Ok(())
}

fn render(frame: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(3)])
        .split(frame.area());

    let items: Vec<ListItem> = app
        .tasks
        .iter()
        .map(|t| ListItem::new(Line::from(Span::raw(t))))
        .collect();

    let list = List::new(items)
        .block(Block::default().title(" Tasks ").borders(Borders::ALL))
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");

    frame.render_stateful_widget(list, chunks[0], &mut app.state);

    let help = Paragraph::new("j/k: move   q: quit")
        .block(Block::default().borders(Borders::ALL).title(" Help "))
        .style(Style::default().add_modifier(Modifier::DIM));

    frame.render_widget(help, chunks[1]);
}
```

Run it with `cargo run`. You get a full-screen task list with a highlighted selection, arrow/`j`/`k` navigation, and `q` to quit. On exit, `ratatui::restore()` puts the terminal back exactly as it was.

> **Tip:** A TUI takes over your terminal, so its visual output cannot be pasted as text here the way a `println!` can. The code above is compile-verified; later in [Best Practices](#best-practices) we render a widget to a headless `TestBackend` and show the *real* cell buffer it produces.

---

## Detailed Explanation

### `ratatui::init()` and `ratatui::restore()`

```rust
let mut terminal = ratatui::init();
// ... run the app ...
ratatui::restore();
```

These two convenience functions (added in ratatui 0.28+) bundle the boilerplate every TUI needs:

- **`init()`** enables the terminal's *raw mode* (so keystrokes arrive immediately, without line-buffering or echo), switches to the **alternate screen** (a separate full-screen buffer, so your shell scrollback is untouched), installs a panic hook so a crash still restores the terminal, and returns a `DefaultTerminal` (an alias for `Terminal<CrosstermBackend<Stdout>>`).
- **`restore()`** reverses all of that: leaves the alternate screen and disables raw mode.

In the pre-0.28 world you wrote `enable_raw_mode()`, `execute!(stdout, EnterAlternateScreen)`, and the reverse by hand. You may still see that in older tutorials; `init`/`restore` is the current idiom.

> **Warning:** If your program returns or panics *between* `init()` and `restore()` without restoring, the terminal is left in raw mode and your shell becomes unusable (no echo, no newline on Enter). The panic hook installed by `init()` handles panics; we cover the early-return case in [Common Pitfalls](#common-pitfalls).

### The draw loop is immediate mode

```rust
while !app.should_quit {
    terminal.draw(|frame| render(frame, &mut app))?;
    handle_events(&mut app)?;
}
```

This is the heart of every ratatui app, and the part that surprises developers coming from object-oriented or retained-mode UI toolkits (Qt, GTK, the DOM). You do **not** create a `List` object once and call `list.setSelected(2)` later. Instead, every iteration:

1. `terminal.draw(closure)` calls your closure with a fresh `Frame`. Inside, you construct *brand-new* widget values from the current `app` state and render them. These widgets are cheap, short-lived value types; they are dropped at the end of the frame.
2. ratatui compares the frame's cell buffer against the previous frame's buffer (a **double buffer**) and writes only the differing cells to the terminal. This diffing is why a TUI does not flicker even though you "redraw everything."
3. `handle_events` reads input and mutates `app`. The next loop iteration renders the new state.

This is exactly React's model: **UI = f(state)**. Your `render` function is pure with respect to drawing; all the mutation lives in event handling. The difference from Ink/React is that there is no reconciler and no hidden re-render trigger — *you* own the loop and decide when to redraw.

### Widgets, `Block`, and layout

```rust
let chunks = Layout::default()
    .direction(Direction::Vertical)
    .constraints([Constraint::Min(1), Constraint::Length(3)])
    .split(frame.area());
```

`frame.area()` is the full terminal rectangle (a `Rect` of `{ x, y, width, height }` in character cells). `Layout` is ratatui's flexbox-equivalent: it splits a `Rect` into sub-rectangles according to `Constraint`s. Here, `Length(3)` reserves exactly 3 rows for the help box and `Min(1)` gives the rest to the list. Other constraints include `Percentage(50)`, `Ratio(1, 3)`, `Fill(1)` (proportional, like flex-grow), and `Max(n)`.

A **`Block`** is the chrome around a widget: borders and a title. `List`, `Paragraph`, `Gauge`, etc. each take a `.block(...)` to draw inside.

### Stateful vs. stateless widgets

```rust
frame.render_stateful_widget(list, chunks[0], &mut app.state);
```

Most widgets are stateless: `render_widget(widget, area)` draws them and forgets them. But some widgets need **state that survives across frames**: a `List` needs to remember which item is selected and how far it is scrolled, a `Table` and `Scrollbar` likewise. That state lives in your `App` (here `ListState`), and you hand it in via `render_stateful_widget(widget, area, &mut state)`. The widget reads and updates the state during rendering (for example, adjusting the scroll offset so the selected item stays visible). This split keeps the widget itself a throwaway value while persisting only the minimal data that must outlive a frame.

### Event handling with `poll` and `read`

```rust
if event::poll(Duration::from_millis(100))?
    && let Event::Key(key) = event::read()?
    && key.kind == KeyEventKind::Press
{
    match key.code { /* ... */ }
}
```

`event::poll(timeout)` returns `true` if an event is waiting, blocking at most `timeout`. If it returns `true`, `event::read()` retrieves the event without blocking. The 100 ms timeout means: redraw at least ten times a second even if no key is pressed (useful for clocks, spinners, or live data). The chained `if ... && let ... && ...` is a **let-chain**, stable on the 2024 edition. It replaces the nested `if let` you would have written on older editions.

> **Note:** Checking `key.kind == KeyEventKind::Press` matters on Windows, where crossterm reports both key-press *and* key-release events. Without this filter, every keystroke fires your handler twice. This is one of the most common cross-platform TUI bugs.

---

## Key Differences

| Concept | Node (Ink / blessed) | Rust (ratatui) |
| --- | --- | --- |
| Rendering model | Retained component tree + React reconciler | Immediate mode: rebuild every widget each frame |
| What triggers a redraw | `setState` / hooks (automatic) | You call `terminal.draw(...)` in your loop |
| Widget lifetime | Persistent objects you mutate | Throwaway values constructed per frame |
| State storage | Component-local (`useState`) | Your own structs; widget "state" (`ListState`) is explicit |
| Event loop | Hidden by the framework | You own the `loop` and the `poll`/`read` calls |
| Layout | Yoga (CSS flexbox) in Ink | `Layout` + `Constraint` (flexbox-like, integer cells) |
| Async / concurrency | Single-threaded event loop | Free to spawn threads or a tokio task; send updates via a channel |
| Dependencies | React + reconciler + Yoga (large) | ratatui + crossterm (small, no runtime) |
| Diffing | Virtual DOM diff | Cell-buffer diff (double buffer) |

The deepest difference is ownership of the loop. In Ink you never see the loop; in ratatui the loop *is* your `main`. This is more code, but it means there is no hidden machinery: you can interleave input, timers, and background-thread messages explicitly, and you can reason precisely about exactly when a redraw happens.

> **Note:** "Immediate mode" describes the *API* you use, not how the screen is updated. The terminal is still updated incrementally via buffer diffing; ratatui does not repaint every cell every frame. You *describe* the whole UI each frame; ratatui *applies* only the deltas.

---

## Common Pitfalls

### Returning early without restoring the terminal

If a `?` propagates an error or you `return` between `init()` and `restore()`, you skip the restore and leave the terminal in raw mode. The fix is the structure used above: call `init()`, run a fallible `run(...)` that returns a `Result`, **always** call `restore()`, and only then propagate the result.

```rust
fn main() -> std::io::Result<()> {
    let mut terminal = ratatui::init();
    let result = run(&mut terminal); // may return Err
    ratatui::restore();              // runs no matter what
    result                           // propagate after restoring
}
```

For 0.30, there is also `ratatui::run(|terminal| { ... })`, a helper that initializes, runs your closure, and restores even on early return, verified to exist in this version:

```rust
fn main() -> std::io::Result<()> {
    ratatui::run(|terminal| {
        // init + restore are handled for you, including on early return
        loop {
            terminal.draw(|frame| { /* ... */ })?;
            break;
        }
        Ok(())
    })
}
```

### Using the deprecated `frame.size()`

Older code calls `frame.size()`; it was renamed to `frame.area()` and `size()` is now deprecated. Calling it still compiles but warns:

```rust
fn render(frame: &mut ratatui::Frame) {
    // deprecated: triggers a real compiler warning
    frame.render_widget(ratatui::widgets::Paragraph::new("hi"), frame.size());
}
```

The actual warning from `cargo build`:

```text
warning: use of deprecated method `ratatui::Frame::<'_>::size`: use `area()` instead
 --> src/main.rs:3:71
  |
3 |     frame.render_widget(ratatui::widgets::Paragraph::new("hi"), frame.size());
  |                                                                       ^^^^
  |
  = note: `#[warn(deprecated)]` on by default
```

Use `frame.area()` instead.

### Passing state to `render_widget` instead of `render_stateful_widget`

A `List` with selection needs `render_stateful_widget`. If you reach for the wrong method and pass the state as a third argument, you get an arity error; this snippet does **not** compile:

```rust
use ratatui::{Frame, widgets::{List, ListState}};

fn render(frame: &mut Frame, state: &mut ListState) {
    let list = List::new(vec!["a", "b"]);
    // does not compile (error[E0061]: this method takes 2 arguments but 3 were supplied)
    frame.render_widget(list, frame.area(), state);
}
```

The real error from `cargo build`:

```text
error[E0061]: this method takes 2 arguments but 3 arguments were supplied
  --> src/main.rs:6:11
   |
 6 |     frame.render_widget(list, frame.area(), state);
   |           ^^^^^^^^^^^^^                     ----- unexpected argument #3 of type `&mut ListState`
   |
note: method defined here
  --> .../ratatui-core-0.1.0/src/terminal/frame.rs:93:12
   |
93 |     pub fn render_widget<W: Widget>(&mut self, widget: W, area: Rect) {
   |            ^^^^^^^^^^^^^
help: remove the extra argument
   |
 6 -     frame.render_widget(list, frame.area(), state);
 6 +     frame.render_widget(list, frame.area());
   |
```

The fix is to call `frame.render_stateful_widget(list, frame.area(), state)`.

### Blocking the loop with `event::read()` when you need to tick

`event::read()` blocks until input arrives. If you call it directly in a loop that also needs to animate (a spinner, a clock, live metrics), the screen freezes between keystrokes. Always gate it behind `event::poll(timeout)?` so the loop wakes up on the timeout to redraw, even when the user is idle.

### Forgetting the `KeyEventKind::Press` filter on Windows

As noted above, crossterm delivers both press and release key events on Windows. Code tested only on macOS/Linux will appear to "double-press" on Windows. Filter on `key.kind == KeyEventKind::Press`.

---

## Best Practices

- **Separate state, update, and view.** Keep an `App` struct (state), free functions or methods that mutate it in response to events (update), and a pure `render`/`ui` function that only reads state and draws (view). This is the Elm/React architecture and it makes a TUI testable.

- **Make rendering a pure function so you can test it headlessly.** ratatui ships a `TestBackend` that renders into an in-memory cell grid with no real terminal, perfect for unit tests and CI. Here is a self-contained program that renders a greeting into a 20×3 buffer and prints the cells it produced:

  ```rust
  use ratatui::{
      Terminal,
      backend::TestBackend,
      widgets::{Block, Paragraph},
  };

  fn draw_greeting(name: &str) -> Terminal<TestBackend> {
      let backend = TestBackend::new(20, 3);
      let mut terminal = Terminal::new(backend).unwrap();
      terminal
          .draw(|frame| {
              let p = Paragraph::new(format!("Hi {name}")).block(Block::bordered());
              frame.render_widget(p, frame.area());
          })
          .unwrap();
      terminal
  }

  fn main() {
      let terminal = draw_greeting("Ada");
      let buffer = terminal.backend().buffer();
      let row0: String = (0..20).map(|x| buffer[(x, 0)].symbol()).collect();
      let row1: String = (0..20).map(|x| buffer[(x, 1)].symbol()).collect();
      println!("{row0}");
      println!("{row1}");
  }
  ```

  Running it with `cargo run` prints the actual rendered cells:

  ```text
  ┌──────────────────┐
  │Hi Ada            │
  ```

  You can assert against an expected buffer with `Buffer::with_lines`, which keeps tests readable. See [Section 13: Testing](/13-testing/) for the broader testing toolkit.

- **Prefer the `Stylize` shorthands for terse styling.** Instead of `Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD)`, import `ratatui::style::Stylize` and write `.blue().bold()` directly on strings and widgets: `"Counter".bold()`, `Block::bordered().blue()`, `Paragraph::new(text).centered()`.

- **Use a tick rate to decouple input latency from animation.** Compute the poll timeout from `tick_rate - last_tick.elapsed()` so you redraw on a steady cadence while still responding to keys instantly. The [Real-World Example](#real-world-example) does exactly this.

- **For long-running work, do it on a thread and send updates over a channel.** ratatui's loop must stay responsive. Spawn a `std::thread` (or a tokio task — see [Section 11: Async](/11-async/)) for I/O or computation, send progress via an `mpsc` channel, and `try_recv` it in your loop to update state. For pure progress reporting outside a TUI, [indicatif](/18-cli-tools/04-progress-bars/) is simpler.

- **Respect terminal capabilities.** Not every terminal supports truecolor or every Unicode box-drawing glyph. ratatui degrades gracefully, but for color-sensitive output also honor the `NO_COLOR` convention — see [Colored Output](/18-cli-tools/05-colored-output/).

---

## Real-World Example

A compact **service monitor** (a tiny `k9s`/`htop`): a tab bar, a stateful list of services on the left, a load gauge for the selected service on the right, a footer with key hints, and a tick-based update that simulates live metrics. This is fully compile-verified and clippy-clean.

```rust
use std::io;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::{
    DefaultTerminal, Frame,
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style, Stylize},
    text::Line,
    widgets::{Block, Gauge, List, ListItem, ListState, Tabs},
};

/// A single monitored service and its simulated load.
struct Service {
    name: String,
    load: u16, // 0..=100
}

struct App {
    services: Vec<Service>,
    selected: ListState,
    tab: usize,
    last_tick: Instant,
    should_quit: bool,
}

impl App {
    fn new() -> Self {
        let mut selected = ListState::default();
        selected.select(Some(0));
        Self {
            services: vec![
                Service { name: "api-gateway".into(), load: 22 },
                Service { name: "auth-service".into(), load: 64 },
                Service { name: "worker-pool".into(), load: 81 },
                Service { name: "postgres".into(), load: 35 },
            ],
            selected,
            tab: 0,
            last_tick: Instant::now(),
            should_quit: false,
        }
    }

    /// Advance simulated state. In a real app this is where you poll metrics.
    fn on_tick(&mut self) {
        for (i, svc) in self.services.iter_mut().enumerate() {
            let delta = ((self.last_tick.elapsed().as_millis() / 50) as u16 + i as u16) % 7;
            svc.load = (svc.load + delta) % 101;
        }
    }

    fn select_next(&mut self) {
        let i = self.selected.selected().map_or(0, |i| (i + 1) % self.services.len());
        self.selected.select(Some(i));
    }
}

const TICK_RATE: Duration = Duration::from_millis(250);

fn main() -> io::Result<()> {
    let mut terminal = ratatui::init();
    let result = run(&mut terminal, App::new());
    ratatui::restore();
    result
}

fn run(terminal: &mut DefaultTerminal, mut app: App) -> io::Result<()> {
    while !app.should_quit {
        terminal.draw(|frame| ui(frame, &mut app))?;

        // Wake up at least every tick so the gauges animate even when idle.
        let timeout = TICK_RATE.saturating_sub(app.last_tick.elapsed());
        if event::poll(timeout)?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
                KeyCode::Tab => app.tab = (app.tab + 1) % 2,
                KeyCode::Down | KeyCode::Char('j') => app.select_next(),
                _ => {}
            }
        }
        if app.last_tick.elapsed() >= TICK_RATE {
            app.on_tick();
            app.last_tick = Instant::now();
        }
    }
    Ok(())
}

fn ui(frame: &mut Frame, app: &mut App) {
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    // Header: tab bar.
    let tabs = Tabs::new(vec!["Services", "Logs"])
        .block(Block::bordered().title(" monitor "))
        .select(app.tab)
        .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
    frame.render_widget(tabs, header);

    // Body: split into a list (left) and a gauge (right).
    let [left, right] =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(body);

    let items: Vec<ListItem> = app
        .services
        .iter()
        .map(|s| ListItem::new(Line::from(format!("{:<14} {:>3}%", s.name, s.load))))
        .collect();
    let list = List::new(items)
        .block(Block::bordered().title(" Services "))
        .highlight_style(Style::default().bg(Color::Blue).fg(Color::White))
        .highlight_symbol("> ");
    frame.render_stateful_widget(list, left, &mut app.selected);

    let idx = app.selected.selected().unwrap_or(0);
    let svc = &app.services[idx];
    let color = if svc.load > 75 { Color::Red } else { Color::Green };
    let gauge = Gauge::default()
        .block(Block::bordered().title(format!(" {} load ", svc.name)))
        .gauge_style(color)
        .percent(svc.load);
    frame.render_widget(gauge, right);

    let footer_line = Line::from(" Tab: switch   j/↓: next   q: quit ").dim();
    frame.render_widget(footer_line, footer);
}
```

Things worth highlighting:

- The **tick rate** decouples redraw cadence from input. `event::poll(timeout)` blocks for at most the time remaining until the next tick, so keys feel instant while the gauges animate four times a second.
- `Layout::vertical([...]).areas(rect)` returns a fixed-size array you destructure directly into `[header, body, footer]`, cleaner than indexing `chunks[0]`, `chunks[1]`.
- The gauge color is recomputed from state every frame (red above 75%, green below). That is immediate mode in one line: the view is a pure function of `svc.load`.
- Only the `ListState` persists across frames; every widget value is rebuilt each `ui` call and dropped at frame end.

---

## Further Reading

- [Ratatui official website and tutorials](https://ratatui.rs/) — the authoritative guide, including the "Counter App" and "JSON Editor" walkthroughs.
- [Ratatui on docs.rs](https://docs.rs/ratatui/latest/ratatui/) — full widget and API reference for the current release.
- [Crossterm on docs.rs](https://docs.rs/crossterm/latest/crossterm/) — the backend's event, style, and terminal APIs.
- [The Elm Architecture](https://guide.elm-lang.org/architecture/): the model/update/view pattern that ratatui apps adopt for state management.

Related sections in this guide:

- [clap Basics](/18-cli-tools/00-clap-basics/) and [clap Derive](/18-cli-tools/01-clap-derive/): parse the arguments and flags that launch your TUI.
- [Subcommands](/18-cli-tools/02-subcommands/) — wire a TUI behind a `monitor` subcommand of a larger CLI.
- [Progress Bars](/18-cli-tools/04-progress-bars/) — for line-oriented progress (indicatif), the simpler alternative when you do not need a full-screen UI.
- [Colored Output](/18-cli-tools/05-colored-output/): ANSI color and `NO_COLOR` for non-fullscreen output.
- [Distribution](/18-cli-tools/10-distribution/): ship your TUI as a single binary with `cargo install` or prebuilt releases.
- [Section 11: Async](/11-async/) — drive a TUI from a tokio event loop and background tasks.
- [Section 13: Testing](/13-testing/) — the broader testing toolkit behind the `TestBackend` pattern above.
- [Section 01: Getting Started](/01-getting-started/) and [Section 02: Basics](/02-basics/): project setup and the syntax used throughout.
- [Section 19: WebAssembly](/19-wasm/) — ratatui can even target the browser via a wasm backend, reusing the same render code.

---

## Exercises

### Exercise 1: A counter app

**Difficulty:** Beginner

**Objective:** Internalize the draw loop by building the smallest possible interactive TUI.

**Instructions:** Create a TUI that displays a single integer counter centered on screen inside a bordered block. Pressing `+` or `Up` increments it, `-` or `Down` decrements it, and `q` quits. Use `ratatui::init()`/`restore()` and the `Stylize` shorthands (`.bold()`, `.centered()`).

<details>
<summary>Solution</summary>

```rust
use std::io;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::{
    DefaultTerminal, Frame,
    style::Stylize,
    widgets::{Block, Paragraph},
};

fn main() -> io::Result<()> {
    let mut terminal = ratatui::init();
    let result = run(&mut terminal);
    ratatui::restore();
    result
}

fn run(terminal: &mut DefaultTerminal) -> io::Result<()> {
    let mut counter: i64 = 0;
    loop {
        terminal.draw(|frame: &mut Frame| {
            let text = format!("Counter: {counter}\n\n(+/-: change, q: quit)");
            let widget = Paragraph::new(text)
                .block(Block::bordered().title(" Counter ".bold()))
                .centered()
                .blue();
            frame.render_widget(widget, frame.area());
        })?;

        if let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            match key.code {
                KeyCode::Char('q') => return Ok(()),
                KeyCode::Char('+') | KeyCode::Up => counter += 1,
                KeyCode::Char('-') | KeyCode::Down => counter -= 1,
                _ => {}
            }
        }
    }
}
```

Because there is no animation, this version blocks on `event::read()` directly — no `poll` needed. It only redraws when a key is pressed.

</details>

### Exercise 2: A chat-style input box

**Difficulty:** Intermediate

**Objective:** Manage editable text state and a two-pane layout.

**Instructions:** Build a TUI with a vertical layout: a scrolling `List` of submitted messages on top, and a single-line input box (3 rows, bordered) at the bottom. Typing appends characters to the input; `Backspace` removes the last character; `Enter` pushes the current input into the message list and clears it; `Esc` quits. Empty input on `Enter` should do nothing.

<details>
<summary>Solution</summary>

```rust
use std::io;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::{
    DefaultTerminal, Frame,
    layout::{Constraint, Layout},
    style::Stylize,
    widgets::{Block, List, ListItem, Paragraph},
};

struct App {
    input: String,
    messages: Vec<String>,
    should_quit: bool,
}

fn main() -> io::Result<()> {
    let mut terminal = ratatui::init();
    let app = App { input: String::new(), messages: Vec::new(), should_quit: false };
    let result = run(&mut terminal, app);
    ratatui::restore();
    result
}

fn run(terminal: &mut DefaultTerminal, mut app: App) -> io::Result<()> {
    while !app.should_quit {
        terminal.draw(|frame| ui(frame, &app))?;
        if let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            match key.code {
                KeyCode::Esc => app.should_quit = true,
                KeyCode::Enter => {
                    if !app.input.is_empty() {
                        app.messages.push(std::mem::take(&mut app.input));
                    }
                }
                KeyCode::Backspace => {
                    app.input.pop();
                }
                KeyCode::Char(c) => app.input.push(c),
                _ => {}
            }
        }
    }
    Ok(())
}

fn ui(frame: &mut Frame, app: &App) {
    let [list_area, input_area] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(3)]).areas(frame.area());

    let items: Vec<ListItem> =
        app.messages.iter().map(|m| ListItem::new(m.as_str())).collect();
    let list = List::new(items).block(Block::bordered().title(" Messages "));
    frame.render_widget(list, list_area);

    let input = Paragraph::new(app.input.as_str())
        .block(Block::bordered().title(" Type, Enter to send, Esc to quit "))
        .yellow();
    frame.render_widget(input, input_area);
}
```

Note `std::mem::take(&mut app.input)` moves the current `String` out (leaving an empty one) and pushes it without cloning — idiomatic and allocation-free.

</details>

### Exercise 3: Test a render function headlessly

**Difficulty:** Advanced

**Objective:** Prove your view is a pure function of state by testing it with `TestBackend` — no real terminal involved.

**Instructions:** Write a function `render_status(frame: &mut Frame, connected: bool)` that draws `"ONLINE"` when `connected` is `true` and `"OFFLINE"` otherwise. Then write two `#[test]`s that render it into a 1-row `TestBackend` and assert the resulting buffer equals the expected text using `Buffer::with_lines`. Confirm `cargo test` passes.

<details>
<summary>Solution</summary>

```rust
use ratatui::{Frame, widgets::Paragraph};

fn render_status(frame: &mut Frame, connected: bool) {
    let text = if connected { "ONLINE" } else { "OFFLINE" };
    frame.render_widget(Paragraph::new(text), frame.area());
}

fn main() {
    println!("run `cargo test` for the headless render tests");
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};

    fn render_to_buffer(connected: bool, w: u16) -> Buffer {
        let mut terminal = Terminal::new(TestBackend::new(w, 1)).unwrap();
        terminal
            .draw(|frame| render_status(frame, connected))
            .unwrap();
        terminal.backend().buffer().clone()
    }

    #[test]
    fn shows_online_when_connected() {
        let buffer = render_to_buffer(true, 7);
        assert_eq!(buffer, Buffer::with_lines(["ONLINE "]));
    }

    #[test]
    fn shows_offline_when_disconnected() {
        let buffer = render_to_buffer(false, 7);
        assert_eq!(buffer, Buffer::with_lines(["OFFLINE"]));
    }
}
```

Running `cargo test` produces the real output:

```text
running 2 tests
test tests::shows_offline_when_disconnected ... ok
test tests::shows_online_when_connected ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

The `"ONLINE "` literal has a trailing space because the buffer is 7 cells wide and `"ONLINE"` is 6 characters; the unused cell renders as a space. This is exactly the kind of off-by-one a headless test catches before it ships.

</details>
