//! The command surface — plan-06 §7.
//!
//! Every test here drives `reachgraph_cli::run_with` in process against the
//! fixture plugin (ADR-0008) and a temporary directory. No repository is
//! indexed, nothing is timed, and no process is spawned — the binary's own
//! `main` is three lines over this library, so a spawning harness would assert
//! the same behaviour more slowly and would argue with §4.1's own guard.

mod formats;
mod guards;
mod paths;
mod preflight;
#[cfg(feature = "render-html")]
mod renderers;
mod report;
mod support;
mod surface;
// The roots plugin this exercises is behind its own feature, and the module
// names its type directly rather than through a trait object.
#[cfg(feature = "roots-proto-tonic")]
mod unexamined;

#[cfg(feature = "serve")]
mod serve;
