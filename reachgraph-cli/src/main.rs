//! The reachgraph binary — ADR-0001's single self-contained executable.
//!
//! **A stub on purpose.** `docs/plans/06-cli-and-serve.md` owns the argument
//! surface, the two registries and `serve`. What this file must not grow is a
//! language branch: ADR-0008 forbids `if is_rust_project(root)` here as much as
//! in the core, and plan-06 §3.2 asserts it.

#![forbid(unsafe_code)]

use reachgraph_core as _;
use reachgraph_plugin_api as _;

fn main() {
    eprintln!("reachgraph is not implemented yet; see docs/plans/06-cli-and-serve.md");
    std::process::exit(70);
}
