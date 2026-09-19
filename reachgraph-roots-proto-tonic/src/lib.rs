//! The gRPC roots plugin — plan-04, ADR-0007, ADR-0008.
//!
//! The only crate in the workspace permitted to know that protobuf and tonic
//! exist. What leaves it is `Root` and `Coverage`: the waist receives
//! `(contract, version, service, operation, direction, join_key, binding)` and
//! learns nothing about `.proto` syntax, impl blocks, CamelCase, generated
//! modules or HTTP/2 path spelling (ADR-0003 field 6).
//!
//! # The two halves
//!
//! Every decision this crate makes is a **pure function over plain data** in
//! one of the modules below; [`ProtoTonicPlugin`] gathers the data and is the
//! only type here that holds state. The split is the same one plan-03 §13
//! draws, for the same reason: a decision that cannot be tested without a
//! filesystem is a decision fused to the walk.
//!
//! - [`names`] — CamelCase → snake_case, and the impl-header grammar
//!   plan-03 §8 writes and this crate reads.
//! - [`version`] — ADR-0007's version segment. `None` is never `"v1"`.
//! - [`keys`] — the fully-qualified join key, spelled here and opaque
//!   everywhere else.
//! - [`contract`] — a `.proto` file reduced to `(package, service, rpc)`.
//! - [`direction`] — served or consumed, and the guards that stop a test
//!   double from manufacturing a phantom root.
//! - [`bind`] — a root's handler, or an honest `Unbound` reason.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod plugin;

pub mod bind;
pub mod contract;
pub mod direction;
pub mod keys;
pub mod names;
pub mod unbound;
pub mod version;

pub use plugin::ProtoTonicPlugin;

use reachgraph_plugin_api::PluginId;

/// This plugin's identity.
pub const PLUGIN_ID: PluginId = PluginId("reachgraph-roots-proto-tonic");

/// The pinned parser, as one constant.
///
/// `parser_string_matches_pinned_version` reads `Cargo.toml` and asserts this
/// equals the version pinned there, so a bump that forgets the stamp fails a
/// test rather than shipping a lie in every parse error.
pub const PROTOX_PARSE_VERSION: &str = "0.9.0";

/// The parser stamp, for the one place a parse failure names what read the file.
pub const PARSER: &str = concat!("protox-parse ", "0.9.0");
