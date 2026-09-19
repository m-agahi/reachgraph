//! Shard naming and shard extraction — plan-01 §8.1, ADR-0006.

use reachgraph_plugin_api::Direction;

use crate::root::RootIdentity;

/// Everything outside `[A-Za-z0-9._-]` becomes `_`.
fn sanitize(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn direction(value: Direction) -> &'static str {
    match value {
        Direction::Served => "served",
        Direction::Consumed => "consumed",
    }
}

/// A 64-bit fingerprint of the canonical root tuple, rendered as 16 hex digits.
///
/// Plan-01 §8.1 writes `blake3`. This is FNV-1a instead, and the substitution
/// is deliberate: plan-01 §2 fixes this crate's dependencies at
/// `reachgraph-plugin-api`, `serde` and `serde_json`, "nothing else in v0.1",
/// and the two statements cannot both hold. What the suffix has to do is
/// separate tuples the readable prefix merges — it is not a security boundary,
/// and nothing authenticates against it.
///
/// The encoding below is what makes the separation hold. Every field is length
/// prefixed and a version is tagged, so `None` and `Some("none")` produce
/// different input, which is the one collision ADR-0007 spends a section
/// forbidding.
fn fingerprint(identity: &RootIdentity) -> String {
    let mut hash: u64 = 0xcbf5_2963_3274_25d5;

    let mut eat = |bytes: &[u8]| {
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    };

    let mut field = |bytes: &[u8]| {
        eat(&(bytes.len() as u64).to_be_bytes());
        eat(bytes);
    };

    field(identity.contract.0.as_bytes());
    match &identity.version {
        None => field(&[0x00]),
        Some(version) => {
            let mut tagged = vec![0x01];
            tagged.extend_from_slice(version.as_bytes());
            field(&tagged);
        }
    }
    field(identity.service.as_bytes());
    field(identity.operation.as_bytes());
    field(direction(identity.direction).as_bytes());

    format!("{hash:016x}")
}

/// The file name for one root's shard.
///
/// Derived from root identity, never from a `NodeId` — slugging a node
/// identity would be parsing it (ADR-0003 field 3).
///
/// `_none` renders an absent version **for display only**. A contract with a
/// literal version string `"none"`, or one containing a separator, is kept
/// apart by the fingerprint rather than by the readable prefix.
///
/// **The slug is a file name, not an identity.** Full root identity is inside
/// the file, and a consumer joins on the tuple.
pub(crate) fn slug(identity: &RootIdentity) -> String {
    let version = match &identity.version {
        None => "_none".to_owned(),
        Some(version) => sanitize(version),
    };

    format!(
        "{}__{}__{}__{}__{}__{}",
        sanitize(&identity.contract.0),
        version,
        sanitize(&identity.service),
        sanitize(&identity.operation),
        direction(identity.direction),
        fingerprint(identity)
    )
}

/// Where that shard lands under the output root.
pub(crate) fn shard_path(identity: &RootIdentity) -> String {
    format!("graph/{}.json", slug(identity))
}
