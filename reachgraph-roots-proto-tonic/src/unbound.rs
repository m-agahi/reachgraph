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
use std::path::PathBuf;

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
    ///
    /// Task 2062: the count alone is not actionable. MEASURED on `discrim`,
    /// `hello.v1.Greeter` and `hello.v2.Greeter` both read "2 non-test
    /// candidates", and the user could not see that the two candidates are one
    /// impl per version, nor where they are. So the reason also names every
    /// package that declares the service and every candidate's location.
    Ambiguous {
        /// The proto service name.
        service: String,
        /// The expected handler name.
        handler: String,
        /// How many survived every filter.
        count: usize,
        /// Every package in this run's contracts that declares a service of
        /// this name, sorted. Empty until the plugin, which holds the
        /// contracts, fills it in.
        packages: Vec<String>,
        /// Every survivor, sorted by file and offset.
        candidates: Vec<Candidate>,
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
            Self::Ambiguous {
                service,
                handler,
                count,
                packages,
                candidates,
            } => {
                write!(
                    f,
                    "ambiguous: {count} non-test candidates named `{handler}` in matching impls"
                )?;
                if !packages.is_empty() {
                    write!(
                        f,
                        ". Service `{service}` is declared in {} package(s) in this index: {}",
                        packages.len(),
                        packages.join(", ")
                    )?;
                }
                if !candidates.is_empty() {
                    write!(
                        f,
                        ". Nothing in the index says which candidate serves which declaration, \
                         so none is bound. The candidates: "
                    )?;
                    for (n, candidate) in candidates.iter().enumerate() {
                        if n > 0 {
                            write!(f, "; ")?;
                        }
                        write!(f, "{candidate}")?;
                    }
                }
                Ok(())
            }
            Self::NoDirectionEvidence { service, join_key } => write!(
                f,
                "no first-party impl and no client reference for service `{service}`; direction \
                 assumed consumed, and join key `{join_key}` is recorded for the cross-repository \
                 join"
            ),
        }
    }
}

/// The package a contract without a `package` line is listed under.
pub const NO_PACKAGE: &str = "(no package)";

/// One survivor of an ambiguous binding, located for the user.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Candidate {
    /// The file, as the symbol index spells it.
    pub file: PathBuf,
    /// The UTF-8 byte offset the index gave, when it gave one.
    pub offset: Option<u32>,
    /// The 1-based line of that offset. `None` until the plugin, which holds
    /// the repository root, reads the file, and `None` when it cannot.
    pub line: Option<usize>,
    /// The containing impl header, as the index spells it.
    pub container: Option<String>,
}

impl fmt::Display for Candidate {
    /// `file:line`, and never a byte offset written where a line belongs.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let file = self.file.display();
        match (self.line, self.offset) {
            (Some(line), _) => write!(f, "{file}:{line}")?,
            (None, Some(offset)) => write!(f, "{file} (byte {offset})")?,
            (None, None) => write!(f, "{file}")?,
        }
        if let Some(container) = &self.container {
            write!(f, " `{container}`")?;
        }
        Ok(())
    }
}
