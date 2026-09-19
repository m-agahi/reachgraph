//! The command surface — plan-06 §7.
//!
//! Every test here drives `reachgraph_cli::run_with` in process against the
//! fixture plugin (ADR-0008) and a temporary directory. No repository is
//! indexed, nothing is timed, and no process is spawned — the binary's own
//! `main` is three lines over this library, so a spawning harness would assert
//! the same behaviour more slowly and would argue with §4.1's own guard.

mod guards;
mod preflight;
#[cfg(feature = "render-html")]
mod renderers;
mod report;
mod support;
mod surface;

#[cfg(feature = "serve")]
mod serve;
