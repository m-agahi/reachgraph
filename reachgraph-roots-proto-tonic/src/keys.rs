//! `Root::join_key` — plan-04 §3 and §8, ADR-0007.
//!
//! The key is spelled here and is **opaque everywhere else**. The core compares
//! join keys, groups by them and carries them into the artifact; it never
//! splits one on `/` or `.`, and never extracts a version, a package or a
//! service from one. `join_key_is_never_parsed_by_the_core` asserts that over
//! `reachgraph-core`'s source.

/// The fully-qualified operation name, as gRPC spells it on the wire.
///
/// ```text
/// <package>.<Service>/<Rpc>
/// ```
///
/// MEASURED: `tonic-prost-build` emits this exact string as a literal in both
/// halves of the generated code, and again as `SERVICE_NAME` for the service
/// half. The spelling is not this project's invention.
///
/// # Two things it is deliberately not
///
/// It carries **no leading slash**. The slash is the HTTP/2 path's syntax, not
/// part of the operation's identity, and `join_key_matches_the_generated_path_literal`
/// asserts the relationship by adding it back rather than by storing it.
///
/// A file with no `package` keys as `<Service>/<Rpc>`, with no synthesised
/// prefix — the same rule as a missing version (plan-04 §5): inventing a
/// package would fabricate a distinction the contract does not make.
pub fn join_key(package: Option<&str>, service: &str, rpc: &str) -> String {
    match package {
        Some(package) if !package.is_empty() => format!("{package}.{service}/{rpc}"),
        _ => format!("{service}/{rpc}"),
    }
}
