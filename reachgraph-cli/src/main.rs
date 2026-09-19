//! The reachgraph binary — ADR-0001's single self-contained executable.
//!
//! Three lines over `reachgraph_cli`, so the whole command surface is testable
//! in process (see the library's documentation).

#![forbid(unsafe_code)]

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut out = std::io::stdout();
    let mut err = std::io::stderr();
    let mut streams = reachgraph_cli::Streams {
        out: &mut out,
        err: &mut err,
    };

    ExitCode::from(reachgraph_cli::run_with(
        &match reachgraph_cli::registry::analysis_registry() {
            Ok(registry) => registry,
            Err(error) => {
                eprintln!("error: the plugin registry is mis-wired: {error}");
                return ExitCode::from(reachgraph_cli::EXIT_INTERNAL);
            }
        },
        &args,
        &mut streams,
    ))
}
