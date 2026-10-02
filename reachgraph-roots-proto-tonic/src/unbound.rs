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
    /// (`a_built_fixture_still_has_no_edge_into_generated_code`): building
    /// alone does **not** make it visible — reachgraph does not go looking for
    /// build output. Printing `cargo build` on its own would ship a remediation
    /// measured not to work.
    ///
    /// ADR-0009 (2026-10-02) added what does work: a run given an existing
    /// build's target directory loads its `OUT_DIR` code, and the consumed root
    /// then binds to the generated stub. So the reason names that flag.
    GeneratedStubNotIndexed {
        /// The fully-qualified key, which resolves in the serving repository.
        join_key: String,
    },
    /// A consumed RPC in a run that DID read build output (ADR-0009), and no
    /// generated client method for it is under a `target/**/out/` path.
    ///
    /// Kept apart from [`UnboundReason::GeneratedStubNotIndexed`] because the
    /// remediation differs: advising `--read-build-output` to a run that had it
    /// would be false. The likely causes are a build older than the contract,
    /// or a target directory not under `target/`.
    GeneratedStubNotInBuildOutput {
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
                 index either: this run read no build output, so `OUT_DIR` code is not in the \
                 crate graph. Re-run with `--read-build-output <target-dir>` pointing at an \
                 existing build (ADR-0009) to bind the stub"
            ),
            Self::GeneratedStubNotInBuildOutput { join_key } => write!(
                f,
                "consumed: no handler is expected in this repository; join key `{join_key}` \
                 resolves in the serving repository. This run's build output was read, and no generated \
                 client method for this RPC is in it under a `target/**/out/` path: the build \
                 may predate the contract, or the target directory is not under `target/`. \
                 The stub is not bound, so calls into it are unmeasured"
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
