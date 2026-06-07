//! The actual behavior behind each subcommand.
//!
//! Each function takes the parsed arguments plus a mutable `Store`, does its
//! work, prints colored feedback with `owo-colors`, and returns a `Result` so
//! that any IO failure bubbles up to `main` via the `?` operator.

use std::io::{self, Write};

use anyhow::Result;
use owo_colors::OwoColorize;

use crate::cli::Command;
use crate::store::Store;
use crate::task::Task;

/// Dispatch a parsed command against the store, saving when needed.
pub fn run(command: Command, store: &mut Store) -> Result<()> {
    match command {
        Command::Add { title } => add(store, title)?,
        Command::List { pending, done } => list(store, pending, done),
        Command::Done { id } => done(store, id)?,
        Command::Remove { id } => remove(store, id)?,
        Command::Clear { yes } => clear(store, yes)?,
    }
    Ok(())
}

fn add(store: &mut Store, title_words: Vec<String>) -> Result<()> {
    let title = title_words.join(" ");
    let id = store.add(title.clone());
    store.save()?;
    println!(
        "{} task {} {}",
        "Added".green().bold(),
        format!("#{id}").cyan(),
        format!("\"{title}\"").dimmed()
    );
    Ok(())
}

fn list(store: &Store, pending_only: bool, done_only: bool) {
    let tasks: Vec<&Task> = store
        .tasks()
        .iter()
        .filter(|task| {
            if pending_only {
                !task.done
            } else if done_only {
                task.done
            } else {
                true
            }
        })
        .collect();

    if tasks.is_empty() {
        println!("{}", "No tasks yet. Add one with `taskr add ...`.".dimmed());
        return;
    }

    let total = store.tasks().len();
    let completed = store.tasks().iter().filter(|task| task.done).count();
    println!(
        "{}",
        format!("Tasks ({completed}/{total} done)").bold().underline()
    );

    for task in tasks {
        let checkbox = if task.done {
            "[x]".green().to_string()
        } else {
            "[ ]".yellow().to_string()
        };
        let id = format!("#{}", task.id);
        // Dim and strike-through-ish styling for completed items.
        let title = if task.done {
            task.title.dimmed().strikethrough().to_string()
        } else {
            task.title.to_string()
        };
        println!("  {checkbox} {} {title}", id.cyan());
    }
}

fn done(store: &mut Store, id: u32) -> Result<()> {
    match store.get_mut(id) {
        Some(task) => {
            if task.mark_done() {
                let title = task.title.clone();
                store.save()?;
                println!(
                    "{} {} {}",
                    "Completed".green().bold(),
                    format!("#{id}").cyan(),
                    format!("\"{title}\"").dimmed()
                );
            } else {
                println!(
                    "{} #{id} was already done.",
                    "Note:".yellow().bold()
                );
            }
        }
        None => return Err(missing(id)),
    }
    Ok(())
}

fn remove(store: &mut Store, id: u32) -> Result<()> {
    match store.remove(id) {
        Some(task) => {
            store.save()?;
            println!(
                "{} task {} {}",
                "Removed".red().bold(),
                format!("#{id}").cyan(),
                format!("\"{}\"", task.title).dimmed()
            );
        }
        None => return Err(missing(id)),
    }
    Ok(())
}

fn clear(store: &mut Store, skip_confirm: bool) -> Result<()> {
    if store.tasks().is_empty() {
        println!("{}", "Nothing to clear.".dimmed());
        return Ok(());
    }
    if !skip_confirm && !confirm("Delete ALL tasks? [y/N] ")? {
        println!("{}", "Aborted.".dimmed());
        return Ok(());
    }
    let count = store.clear();
    store.save()?;
    println!(
        "{} {count} task(s).",
        "Cleared".red().bold()
    );
    Ok(())
}

/// Build a consistent "no such task" error. Returning it from a command makes
/// `main` print `Error: ...` to stderr and exit with a non-zero status, the way
/// a Node CLI would `process.exit(1)`.
fn missing(id: u32) -> anyhow::Error {
    anyhow::anyhow!("no task with id #{id}")
}

/// Prompt on stdout and read a yes/no answer from stdin.
fn confirm(prompt: &str) -> Result<bool> {
    print!("{prompt}");
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    Ok(matches!(answer.trim(), "y" | "Y" | "yes" | "YES"))
}
