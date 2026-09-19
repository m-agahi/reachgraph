//! The fixture plugin — ADR-0008's mechanical n=2.
//!
//! A second [`reachgraph_plugin_api::LanguagePlugin`] implementation that is
//! **not a language**. Its job is to turn a leaked rust-analyzer-ism into a
//! build failure now rather than a discovery in month nine when the second real
//! language is half-written, and to be the whole test corpus for the waist —
//! no `cargo metadata`, no indexing, no timing variance, no requirement that a
//! repository has ever been built.
//!
//! It is **permanent test infrastructure, not scaffolding to delete when a
//! second language lands.** ADR-0008 Consequences says so, and its value as a
//! fast deterministic harness grows rather than shrinks.
//!
//! # What makes it work is what it cannot express
//!
//! [`format`] has no offset, no line, no column, no cursor and no confidence
//! number, and it never will. A case states which file a symbol is in and
//! stops there, so every symbol this crate emits carries
//! `range.span: None` — saying exactly what is known and exactly what is not.
//! If a change to the contract makes a case want a span, that is this crate
//! doing its job: change the contract or change the test, never the format.
//!
//! # Explicitly selected, never detected
//!
//! Every case declares `detection: { marker_files: [], extensions: [] }`, and
//! an empty marker list matches nothing (plan-00 §2). So
//! [`reachgraph_plugin_api::Registry::detect`] can never return this plugin —
//! structurally, with no special case in `detect` for a later refactor to
//! remove. A detectable fixture would, on any repository that happened to
//! contain a `reachgraph.fixture.json`, silently replace real analysis with
//! hand-written JSON and emit a complete, plausible, entirely fictional call
//! graph.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod format;
mod intern;
mod plugin;

pub use plugin::{FixturePlugin, FIXTURE_DOCUMENT_NAME};
