//! Plan-03 §13 Tiers B and C — the engine, against real Cargo workspaces.
//!
//! Behind `--features slow-tests`, because each case loads a workspace through
//! cargo. Run them with `cargo test --workspace --all-features`.
//!
//! # The harness builds the fixtures; reachgraph does not
//!
//! ADR-0001 and plan-03 §4 D-B are absolute and apply to the test suite too. A
//! plugin that built a fixture in order to index it would exercise a code path
//! that cannot exist in production, so `fx-macro`'s build is a **setup step**
//! in [`support`], run before the plugin is invoked and visible in the test
//! that needs it.

#![cfg(feature = "slow-tests")]

mod docs;
mod generated;
mod generics;
mod impls;
mod plain;
mod probe;
mod properties;
mod support;
