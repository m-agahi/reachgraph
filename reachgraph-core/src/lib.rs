//! The reachgraph waist — ADR-0003.
//!
//! **Empty on purpose.** `docs/plans/01-core-waist.md` owns everything that
//! goes here: graph assembly, interning, reachability, the complement over all
//! roots, classification and sharding, plus the schema types plan-00 §1 names
//! but does not define (`Node`, `GraphView`, `Shard`, `IndexCoverage`,
//! `PluginDescriptor`), which land in `reachgraph-plugin-api` rather than here.
//!
//! The crate exists now so `no_plugin_depends_on_core` has a node to assert the
//! absence of. The `reachgraph-plugin-api` dependency below is plan-00 §1's
//! declared direction, stated from the first commit so a later addition cannot
//! quietly reverse it.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use reachgraph_plugin_api as _;
