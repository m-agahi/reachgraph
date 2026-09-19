//! Every `reason` this crate can put on an unbound root — plan-04 §9.
//!
//! An unbound root is **carried into the artifact**, never dropped. ADR-0007's
//! partial-index problem is why: a dropped root is indistinguishable from a
//! root that was never looked for, and both make live code read as unreachable.
//!
//! The reasons are an enum rather than format strings at the call sites so that
//! every one of them is a value a test can name, and so a new case has to be
//! added here rather than improvised.

use std::fmt;

/// Why a root carries no node.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum UnboundReason {
    /// A consumed RPC, and the generated client stub is not in the index.
    ///
    /// # This is a pass, not a failure — and its remediation is not `cargo build`
    ///
    /// Plan-04 §9 row 2 says to tell the user to "build the workspace once to
    /// make the cross-repo leaf visible". MEASURED by PR D
    /// (`a_built_fixture_still_has_no_edge_into_generated_code`): building does
    /// **not** make it visible. Nothing reachgraph is permitted to do puts
    /// `OUT_DIR` code into the crate graph (plan-03 §9 D-D), so a built
    /// repository and an unbuilt one produce the same answer here. Printing the
    /// command anyway would ship a remediation that has been measured not to
    /// work.
    GeneratedStubNotIndexed {
        /// The fully-qualified key, which resolves in the serving repository.
        join_key: String,
    },
    /// A served RPC with no candidate of the expected handler name anywhere.
    NoCandidate {
        /// The proto service name.
        service: String,
        /// The expected handler name, after CamelCase → snake_case.
        handler: String,
    },
    /// Candidates of that name exist, and none is inside a matching impl.
    NoMatchingImpl {
        /// The proto service name.
        service: String,
        /// The expected handler name.
        handler: String,
        /// How many candidates carried the name.
        named: usize,
    },
    /// Two or more survivors, which are never resolved by preference.
    Ambiguous {
        /// The expected handler name.
        handler: String,
        /// How many survived every filter.
        count: usize,
    },
    /// A service with no first-party impl and no client reference at all.
    NoDirectionEvidence {
        /// The proto service name.
        service: String,
        /// The fully-qualified key.
        join_key: String,
    },
}

impl fmt::Display for UnboundReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GeneratedStubNotIndexed { join_key } => write!(
                f,
                "consumed: no handler is expected in this repository; join key `{join_key}` \
                 resolves in the serving repository. The generated client stub is not in the \
                 index either: reachgraph never asks the crate graph to load `OUT_DIR` code, so \
                 building the workspace does not make the leaf visible"
            ),
            Self::NoCandidate { service, handler } => write!(
                f,
                "served: no non-test method named `{handler}` in any `impl {service} for …`"
            ),
            Self::NoMatchingImpl {
                service,
                handler,
                named,
            } => write!(
                f,
                "served: {named} method(s) named `{handler}`, none inside an `impl {service} for …`"
            ),
            Self::Ambiguous { handler, count } => write!(
                f,
                "ambiguous: {count} non-test candidates named `{handler}` in matching impls"
            ),
            Self::NoDirectionEvidence { service, join_key } => write!(
                f,
                "no first-party impl and no client reference for service `{service}`; direction \
                 assumed consumed, and join key `{join_key}` is recorded for the cross-repository \
                 join"
            ),
        }
    }
}
