//! The Rust language plugin — plan-03, ADR-0004, ADR-0008.
//!
//! The only crate in the workspace permitted to know that rust-analyzer
//! exists. Everything it learns from `ra_ap_*` leaves through
//! `reachgraph-plugin-api`'s types and nothing else: no `FilePosition`, no
//! `FileId`, no `Cancellable`, no database and no salsa lifetime appears in any
//! signature the waist can see (ADR-0008 leaks 1, 2 and 5).
//!
//! # The two halves
//!
//! Every decision this crate makes is a **pure function over plain data** in
//! one of the modules below, and the engine-facing code gathers the data.
//! Plan-03 §13's three test tiers exist to keep that split honest: Tier A tests
//! the decisions with no `ra_ap` anywhere near them, and a decision that could
//! not be tested that way would be a decision fused to the engine.
//!
//! - [`ids`] — `NodeId::raw`, the leak-1 conversion.
//! - [`kinds`] — the kind table and the impl-header grammar.
//! - [`classify`] — the five categories, from facts the engine already holds.
//! - [`preflight`] — every `reason` and `remediation` this crate can return.
//! - [`coverage`] — what a run could not see.
//!
//! [`RustPlugin`] is the engine-facing half, and it is the only type here that
//! holds state.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod engine;
mod plugin;

pub mod classify;
pub mod coverage;
pub mod ids;
pub mod kinds;
pub mod preflight;

pub use plugin::RustPlugin;

use reachgraph_plugin_api::PluginId;

/// This plugin's identity.
pub const PLUGIN_ID: PluginId = PluginId("reachgraph-lang-rust");

/// The pinned `ra_ap_*` version, as one constant.
///
/// `engine_string_matches_pinned_version` reads `Cargo.toml` and asserts this
/// equals the version pinned there, so a re-vendor that bumps the dependency
/// and forgets the stamp fails a test rather than shipping a lie in every
/// `Provenance` (plan-03 §2, §13 Tier A).
pub const RA_AP_VERSION: &str = "0.0.352";

/// The engine stamp on every [`reachgraph_plugin_api::Provenance`].
///
/// # The grammar, fixed by plan-03 §11
///
/// ```text
/// engine := "<crate> <version>" [ " (" <mode> ")" ]
/// ```
///
/// The version stamp is the **prefix** and is always present; run-mode text is
/// a parenthesised suffix. The suffix is load-bearing rather than decorative:
/// an edge produced with proc-macro expansion off means something different
/// from one produced with it on, and a single edge should carry the provenance
/// of the mode that produced it.
///
/// It is a `const` rather than a runtime value because the mode is a constant.
/// [`coverage::ProcMacroExpansion`] records why there is only one.
pub const ENGINE: &str = concat!("ra_ap_ide ", "0.0.352", " (proc-macros: disabled)");
