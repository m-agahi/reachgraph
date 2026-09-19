//! Root identity — the tuple ADR-0007 makes a root, and nothing else.
//!
//! `join_key` is deliberately absent. ADR-0007 fixes identity as
//! `(contract, version, service, operation, direction)`; the key is a
//! plugin-spelled string the waist stores, emits and never joins on.

use reachgraph_plugin_api::{ContractId, Direction, Root};

/// The five fields that make one root distinct from another.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RootIdentity {
    /// The contract the operation belongs to.
    pub contract: ContractId,
    /// ADR-0007: `None` is an assertion, never a default and never `"v1"`.
    pub version: Option<String>,
    /// The service, as the plugin spells it.
    pub service: String,
    /// The operation, as the plugin spells it.
    pub operation: String,
    /// Served or consumed.
    pub direction: Direction,
}

impl RootIdentity {
    /// The identity of a root, dropping everything that is not identity.
    pub fn of(root: &Root) -> Self {
        Self {
            contract: root.contract.clone(),
            version: root.version.clone(),
            service: root.service.clone(),
            operation: root.operation.clone(),
            direction: root.direction,
        }
    }
}
