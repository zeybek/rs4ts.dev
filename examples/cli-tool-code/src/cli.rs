//! Command-line interface, declared with clap's derive API.
//!
//! This is the Rust counterpart of a `commander`/`oclif` setup in Node: instead
//! of registering commands imperatively, we describe the CLI as a data type and
//! clap generates the parser, `--help`, `--version`, and error messages for us.

use clap::{Parser, Subcommand};

/// `taskr` — a tiny task manager that persists to a JSON file.
#[derive(Debug, Parser)]
#[command(
    name = "taskr",
    version,
    about = "A tiny task manager (TypeScript-developer's guide to Rust)",
    propagate_version = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

/// The subcommands the tool understands. Each variant becomes a subcommand;
/// its fields become positional args or flags.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Add a new task.
    Add {
        /// The task description. Multiple words are joined with spaces, so you
        /// can write `taskr add buy milk` without quotes.
        #[arg(required = true, num_args = 1.., value_name = "TEXT")]
        title: Vec<String>,
    },
    /// List tasks.
    List {
        /// Show only tasks that are still open.
        #[arg(long, conflicts_with = "done")]
        pending: bool,
        /// Show only tasks that are already done.
        #[arg(long)]
        done: bool,
    },
    /// Mark a task as done by id.
    Done {
        /// The id shown by `taskr list`.
        id: u32,
    },
    /// Remove a task by id.
    Remove {
        /// The id shown by `taskr list`.
        id: u32,
    },
    /// Delete every task (asks for confirmation unless `--yes`).
    Clear {
        /// Skip the confirmation prompt.
        #[arg(long, short = 'y')]
        yes: bool,
    },
}
