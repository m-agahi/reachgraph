//! The fixture plugin — ADR-0008's mechanical n=2.
//!
//! **Empty on purpose.** `docs/plans/02-fixture-plugin.md` owns `FixturePlugin`,
//! the JSON format types and the corpus. The crate exists now so the two
//! `cargo metadata` guards in `tests/neutrality.rs` have a real plugin crate to
//! assert over, and so the mutation that proves they discriminate — making a
//! plugin depend on `reachgraph-core` — has something to mutate.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use reachgraph_plugin_api as _;
