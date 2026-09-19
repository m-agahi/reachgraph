//! What the build observed and continued past — plan-01 §4.

use reachgraph_plugin_api::{NodeId, PluginId};

use crate::root::RootIdentity;

/// A finding the build recorded rather than failed on.
///
/// Each one is a fact a consumer may need: none of them stops a build, and
/// every one of them changes how an artifact should be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BuildDiagnostic {
    /// The same identity was emitted twice. The first symbol was kept
    /// (plan-01 §4.3).
    DuplicateSymbol {
        /// The identity emitted twice.
        id: NodeId,
    },
    /// A root bound to an identity no provider emitted a symbol for. The root
    /// is retained and the node is created as external, so the shard exists and
    /// reaches nothing — a true statement and a visible one (plan-01 §4.4).
    RootBoundToUnindexedNode {
        /// Which root.
        root: RootIdentity,
        /// The identity it named.
        node: NodeId,
    },
    /// A plugin emitted symbols and registered no classifier, so its nodes
    /// carry no category. Not an error: classification is a plugin kind and a
    /// plugin may legitimately not provide it (plan-01 §7).
    NoClassifierForPlugin {
        /// The plugin with no classifier.
        plugin: PluginId,
    },
    /// A plugin passed preflight with a finding the user should act on.
    ///
    /// A `Warned` plugin runs. Dropping the remediation would put the value
    /// back outside the type ADR-0003 field 5 built to carry it.
    PreflightWarned {
        /// The plugin that reported it.
        plugin: PluginId,
        /// What was checked and what was found.
        reason: String,
        /// What the user should do about it.
        remediation: String,
    },
    /// A provider failed and `BuildOptions::allow_partial` let the build
    /// continue. `IndexCoverage::partial` is set whenever this is recorded.
    ProviderFailed {
        /// The plugin that failed.
        plugin: PluginId,
        /// What it reported.
        detail: String,
    },
}
