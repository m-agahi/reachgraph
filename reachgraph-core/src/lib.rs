//! The reachgraph waist — ADR-0003.
//!
//! Graph construction, reachability and sharding are the fixed point. They are
//! not plugins, they are not swappable, and everything else in the workspace
//! plugs into what is here.
//!
//! # What this crate never does
//!
//! It never parses a `NodeId`, a `Symbol::container` or a `Root::join_key`. It
//! hashes them, compares them for equality and emits them. It holds no
//! language name, no framework name, no file extension and no path fragment of
//! any kind; classification happens through a `Classifier` trait object, and
//! the rules that produce a `Category` belong to the plugin that registered it.
//!
//! # The shape of a run
//!
//! [`Index::build`] takes [`BuildInputs`] — one slice per capability — plus
//! [`BuildOptions`], and returns an [`Index`]. [`Index::emit`] writes the
//! artifact through an `OutputSink`.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod assemble;
mod classify;
mod diag;
mod emit;
mod graph;
mod intern;
mod reach;
mod root;
pub mod schema;
mod shard;
mod versions;

pub use assemble::{BuildError, BuildInputs, BuildOptions, Index, UnreachableNode};
pub use diag::BuildDiagnostic;
pub use emit::DirectorySink;
pub use root::RootIdentity;
pub use schema::UNREACHABLE_CLAIM;

/// The file name for one root's shard, relative to the output root.
///
/// Public because a consumer that holds a [`RootIdentity`] must be able to find
/// the file without re-deriving the rule — and because the rule is derived from
/// root identity, never from a node identity.
pub fn shard_path(identity: &RootIdentity) -> String {
    shard::shard_path(identity)
}
