//! `cargo xtask <task>`.

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use xtask::SnapshotTarget;

const USAGE: &str = "usage: cargo xtask public-api [--bless]";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["public-api"] => public_api(false),
        ["public-api", "--bless"] => public_api(true),
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(64)
        }
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or(Path::new("."))
        .to_path_buf()
}

/// Render the contract's public surface, and either write it or report that it
/// has moved.
///
/// Writing happens ONLY under `--bless`. Plan-02 §7.2.1: regeneration must be a
/// deliberate command, never an automatic fixup on test failure, because a test
/// that repairs itself asserts nothing.
fn public_api(bless: bool) -> ExitCode {
    let target = SnapshotTarget::plugin_api(&workspace_root());

    let rendered = match xtask::render(&target) {
        Ok(rendered) => rendered,
        Err(error) => {
            eprintln!("error: {error}");
            return ExitCode::FAILURE;
        }
    };

    if bless {
        return match std::fs::write(&target.snapshot, &rendered) {
            Ok(()) => {
                println!("wrote {}", target.snapshot.display());
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("error: {}: {error}", target.snapshot.display());
                ExitCode::FAILURE
            }
        };
    }

    match std::fs::read_to_string(&target.snapshot) {
        Ok(checked_in) if checked_in == rendered => {
            println!("{} is current", target.snapshot.display());
            ExitCode::SUCCESS
        }
        Ok(_) => {
            eprintln!(
                "{} is out of date. Read the change, then run `cargo xtask public-api --bless`.",
                target.snapshot.display()
            );
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("error: {}: {error}", target.snapshot.display());
            ExitCode::FAILURE
        }
    }
}
