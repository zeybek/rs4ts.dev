//! `taskr` entry point.
//!
//! `main` returns `anyhow::Result<()>`: any error produced with `?` is printed
//! with its full context chain and the process exits non-zero — the Rust way to
//! do what an unhandled `throw` plus `process.exit(1)` does in Node.

mod cli;
mod commands;
mod store;
mod task;

use anyhow::Result;
use clap::Parser;

use crate::cli::Cli;
use crate::store::Store;

fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut store = Store::open_default()?;
    commands::run(cli.command, &mut store)?;
    Ok(())
}
