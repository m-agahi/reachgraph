//! The renderer's suite — plan-05 §8.
//!
//! # What is asserted, and what is not
//!
//! **There is no browser harness and v0.1 will not have one** (plan-05 §8.1).
//! Nothing below executes `loader.js`. Nothing verifies that Cytoscape draws a
//! graph, that the version toggle switches, that the depth slider filters, or
//! that a compound box collapses. Saying otherwise would be the exact failure
//! this project's premise forbids.
//!
//! What is asserted is every byte this crate writes: the page is **parsed**
//! with `scraper` and asserted over as a document (a string match on generated
//! HTML passes on a page that would not render), the computed sidecar is
//! asserted as a structure, the inlined data is round-tripped through a JSON
//! parser, and the vendored bytes are compared against the hashes
//! `VENDOR.toml` records.
//!
//! # What plan-05 §8.2 asks for and why most of it is not here
//!
//! §8.2 lists twenty-one emission tests — one shard per root, slug injectivity,
//! `schema_version` on every file, `unreachable_carries_coverage`, and so on.
//! **Those documents are the waist's**, written by `reachgraph-core`'s
//! `emit.rs` since plan-01, and asserted by its own golden artifacts in
//! `reachgraph-core/tests/golden/`. Re-asserting them from here would test
//! `reachgraph-core` through a renderer and would buy a second place to update
//! when the schema moves.
//!
//! What is left is this crate's, and it is all here: the page's structure
//! (§8.3), the wording guard (§8.4), inlining (§8.5), the renderer contract
//! (§8.6), and the box hierarchy and dispatch classification §4.4.1 and
//! ADR-0729 put in this crate rather than in the waist.

mod contract;
mod inline;
mod page;
mod structure;
mod support;
mod vendor;
mod wording;
