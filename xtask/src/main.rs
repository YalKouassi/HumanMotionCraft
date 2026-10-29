//! Repo tasks (`cargo xtask <task>`, the alias is in `.cargo/config.toml`).
//!
//! ```text
//! cargo xtask check [--keep-going]   the whole pre-commit gate: fmt, clippy, tests (nextest, else
//!                                    cargo test), `ac fixtures`, names, assets, docs; stops at the
//!                                    first failure unless --keep-going
//! cargo xtask names [--hash <word>]  other products' names appear nowhere (src/names.rs);
//!                                    --hash prints a word's hash for the deny-list
//! cargo xtask assets                 every asset has an ATTRIBUTION.md row; MANIFEST.json matches
//! cargo xtask docs                   every path and crate AGENTS.md cites exists
//! ```
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

mod assets;
mod docs;
mod names;
mod repo;

use std::path::Path;
use std::process::{Command, ExitCode};
use std::time::Instant;

/// One guard's findings, printed; true when there were none.
fn report(what: &str, found: Result<Vec<String>, String>) -> bool {
    match found {
        Ok(list) if list.is_empty() => {
            println!("ok    {what}");
            true
        }
        Ok(list) => {
            println!("FAIL  {what}: {} problem(s)", list.len());
            for f in list {
                println!("      {f}");
            }
            false
        }
        Err(e) => {
            println!("FAIL  {what}: {e}");
            false
        }
    }
}

fn cargo() -> Command {
    Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
}

/// Run a cargo step from the repo root; true when it succeeded.
fn step(root: &Path, name: &str, args: &[&str]) -> bool {
    println!("----  {name}: cargo {}", args.join(" "));
    let started = Instant::now();
    let status = cargo().args(args).current_dir(root).status();
    let secs = started.elapsed().as_secs_f64();
    match status {
        Ok(s) if s.success() => {
            println!("ok    {name} ({secs:.0} s)");
            true
        }
        Ok(s) => {
            println!("FAIL  {name} ({s})");
            false
        }
        Err(e) => {
            println!("FAIL  {name}: can't run cargo: {e}");
            false
        }
    }
}

fn has_nextest(root: &Path) -> bool {
    cargo()
        .args(["nextest", "--version"])
        .current_dir(root)
        .output()
        .is_ok_and(|o| o.status.success())
}

fn check(root: &Path, keep_going: bool) -> bool {
    let tests: &[&str] = if has_nextest(root) {
        &["nextest", "run", "--workspace"]
    } else {
        println!(
            "note  cargo-nextest is not installed: the tests run with cargo test (cargo install cargo-nextest)"
        );
        &["test", "--workspace"]
    };
    let cargo_steps: [(&str, &[&str]); 4] = [
        ("fmt", &["fmt", "--all", "--check"]),
        (
            "clippy",
            &["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"],
        ),
        ("tests", tests),
        ("fixtures", &["run", "--quiet", "-p", "ac-cli", "--", "fixtures"]),
    ];
    let mut failed = Vec::new();
    for (name, args) in cargo_steps {
        if !step(root, name, args) {
            failed.push(name);
            if !keep_going {
                break;
            }
        }
    }
    if failed.is_empty() || keep_going {
        let guards = [
            ("names", names::check(root)),
            ("assets", assets::check(root)),
            ("docs", docs::check(root)),
        ];
        for (name, found) in guards {
            if !report(name, found) {
                failed.push(name);
            }
        }
    }
    if failed.is_empty() {
        println!("check passed");
        true
    } else {
        println!("check FAILED: {}", failed.join(", "));
        false
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let root = repo::root();
    let ok = match args.as_slice() {
        ["check"] => check(&root, false),
        ["check", "--keep-going"] => check(&root, true),
        ["names"] => report("names", names::check(&root)),
        ["names", "--hash", word] => {
            println!("{:#018x}", names::fnv1a(word.to_lowercase().as_bytes()));
            true
        }
        ["assets"] => report("assets", assets::check(&root)),
        ["docs"] => report("docs", docs::check(&root)),
        _ => {
            eprintln!("usage: cargo xtask check [--keep-going] | names [--hash <word>] | assets | docs");
            return ExitCode::from(2);
        }
    };
    if ok { ExitCode::SUCCESS } else { ExitCode::from(1) }
}
