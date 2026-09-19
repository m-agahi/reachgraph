//! `NodeId::raw` — the leak-1 conversion, and the whole of it.
//!
//! Plan-03 §6. `EdgeProvider::edges_in` and `EdgeProvider::edges_from` take no
//! position, no cursor and no offset, while `ra_ap_ide::Analysis::outgoing_calls`
//! takes a `FilePosition`. The conversion happens here and appears nowhere in
//! `reachgraph-plugin-api`.
//!
//! # The grammar
//!
//! ```text
//! raw := "<unit_id>|<def_offset>|<path>"
//! ```
//!
//! The path is **last** so decoding is `splitn(3, '|')` and a `|` inside a path
//! is harmless. The unit id is first and must contain no `|`; Cargo package ids
//! do not, and [`RawParts::encode`] refuses rather than producing a raw that
//! decodes to something else.
//!
//! # Why this makes a table unnecessary for the forward direction
//!
//! `raw` is a pure function of `(unit, path, name-token offset)`, so converting
//! a node back to a position is a **decode, not a lookup**. Nothing has to have
//! seen the node before. That is what lets a call target outside every
//! enumerated unit — a dependency's library source, a generated module — get a
//! stable `NodeId` the first time it is met.

use std::fmt;

use reachgraph_plugin_api::{NodeId, PluginId, UnitId};

/// The separator, stated once.
const SEP: char = '|';

/// The three facts a [`NodeId::raw`] carries.
///
/// `offset` is the byte offset of the **name token**, not of the item — that is
/// what `FilePosition` needs for `outgoing_calls` to identify the item under it
/// (plan-03 §6). `Symbol::range` separately carries the item's full extent.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RawParts {
    /// The owning crate's unit id. Contains no [`SEP`].
    pub unit: UnitId,
    /// Byte offset of the name token, in the declared `PositionEncoding`.
    pub offset: u32,
    /// Repository-relative where the file is inside the repository, absolute
    /// where it is not — see [`RawParts::encode`].
    pub path: String,
}

/// Why a raw could not be built or read.
///
/// Every variant is a real defect rather than an expected absence, which is why
/// this is an error and not an `Option` (ADR-0003's honest-absence rule cuts the
/// other way here: there is nothing honest about an absent node identity).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum RawError {
    /// A unit id containing the separator would decode to a different unit.
    UnitContainsSeparator {
        /// The offending unit id, verbatim.
        unit: String,
    },
    /// Fewer than three separator-delimited fields.
    MissingFields {
        /// The raw as given.
        raw: String,
        /// How many fields were present.
        found: usize,
    },
    /// The offset field was not a decimal `u32`.
    OffsetNotAnOffset {
        /// The raw as given.
        raw: String,
        /// The field that failed to parse.
        field: String,
    },
    /// An empty path decodes to a file that cannot be looked up.
    EmptyPath {
        /// The raw as given.
        raw: String,
    },
}

impl fmt::Display for RawError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnitContainsSeparator { unit } => write!(
                f,
                "unit id {unit:?} contains {SEP:?}, which is the node id field separator"
            ),
            Self::MissingFields { raw, found } => write!(
                f,
                "node id {raw:?} has {found} field(s); the grammar is unit|offset|path"
            ),
            Self::OffsetNotAnOffset { raw, field } => write!(
                f,
                "node id {raw:?} has offset field {field:?}, which is not a decimal u32"
            ),
            Self::EmptyPath { raw } => write!(f, "node id {raw:?} carries an empty path"),
        }
    }
}

impl std::error::Error for RawError {}

impl RawParts {
    /// Render the raw.
    ///
    /// The path is written verbatim. Callers hand it a repository-relative,
    /// `/`-separated path for a file inside the repository and an absolute one
    /// for a file outside it — a dependency's extracted library source and the
    /// sysroot both live outside the repository and must still get an identity
    /// (plan-03 §9). Plan-03 §6 says "repo-relative" because it is describing
    /// first-party symbols; the out-of-repository case is the one §9 adds, and
    /// the two are distinguished by the path being absolute rather than by a
    /// flag.
    pub fn encode(&self) -> Result<String, RawError> {
        if self.unit.0.contains(SEP) {
            return Err(RawError::UnitContainsSeparator {
                unit: self.unit.0.clone(),
            });
        }
        if self.path.is_empty() {
            return Err(RawError::EmptyPath {
                raw: format!("{}{SEP}{}{SEP}", self.unit.0, self.offset),
            });
        }
        Ok(format!(
            "{}{SEP}{}{SEP}{}",
            self.unit.0, self.offset, self.path
        ))
    }

    /// Read a raw back.
    pub fn decode(raw: &str) -> Result<Self, RawError> {
        let mut fields = raw.splitn(3, SEP);
        let unit = fields.next().unwrap_or_default();
        let Some(offset) = fields.next() else {
            return Err(RawError::MissingFields {
                raw: raw.to_owned(),
                found: 1,
            });
        };
        let Some(path) = fields.next() else {
            return Err(RawError::MissingFields {
                raw: raw.to_owned(),
                found: 2,
            });
        };
        let offset = offset
            .parse::<u32>()
            .map_err(|_| RawError::OffsetNotAnOffset {
                raw: raw.to_owned(),
                field: offset.to_owned(),
            })?;
        if path.is_empty() {
            return Err(RawError::EmptyPath {
                raw: raw.to_owned(),
            });
        }
        Ok(Self {
            unit: UnitId(unit.to_owned()),
            offset,
            path: path.to_owned(),
        })
    }
}

/// Mint a [`NodeId`] for a located definition.
///
/// The single constructor both `symbols_in` and the call-target path go
/// through, so a callee that is also an emitted symbol produces a byte-identical
/// `raw` (plan-03 §9). Two constructors would be two chances to diverge.
pub fn node_id(plugin: PluginId, parts: &RawParts) -> Result<NodeId, RawError> {
    Ok(NodeId {
        plugin,
        raw: parts.encode()?,
    })
}
