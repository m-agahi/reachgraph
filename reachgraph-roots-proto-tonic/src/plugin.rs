//! `ProtoTonicPlugin` — the contract, implemented over the decisions.
//!
//! Plan-04 §2. The plugin gathers facts (which `.proto` files exist, what the
//! repository's own `.rs` files contain, what the symbol index holds) and every
//! judgement it then makes lives in [`crate::direction`], [`crate::bind`],
//! [`crate::version`] or [`crate::keys`].
//!
//! # Why this type holds a lock
//!
//! `RootProvider::coverage` takes `&self` and returns by value, and `Plugin` is
//! `Send + Sync`, so what a run examined has to live behind interior
//! mutability. It is written once per `roots()` call and read afterwards, which
//! is the same shape `RustPlugin` uses for its loaded workspace.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use reachgraph_plugin_api::{
    Capability, ContractId, Coverage, Detection, Direction, Plugin, PluginError, PluginId,
    PositionEncoding, Preflight, Root, RootBinding, RootProvider, Symbol, SymbolIndex, VersionKey,
};

use crate::bind::{bind_generated_client, bind_handler, Binding, Operation};
use crate::contract::{discover, parse, ProtoContract};
use crate::direction::{
    direction_of, is_served_signal, is_test_target_path, mentions_client, DirectionCall,
    ServiceEvidence,
};
use crate::unbound::UnboundReason;
use crate::PLUGIN_ID;

/// The gRPC roots plugin.
#[derive(Default)]
pub struct ProtoTonicPlugin {
    examined: Mutex<Examined>,
}

/// What the last completed run looked at.
#[derive(Clone, Default)]
struct Examined {
    contracts: Vec<ContractId>,
    versions: Vec<VersionKey>,
}

impl ProtoTonicPlugin {
    /// A plugin that has examined nothing yet.
    pub fn new() -> Self {
        Self::default()
    }
}

impl Plugin for ProtoTonicPlugin {
    fn id(&self) -> PluginId {
        PLUGIN_ID
    }

    /// Roots, and only roots. This plugin provides no symbols and no edges; it
    /// consumes the index another plugin fills.
    fn provides(&self) -> &[Capability] {
        &[Capability::Roots]
    }

    fn position_encoding(&self) -> PositionEncoding {
        PositionEncoding::Utf8Bytes
    }

    /// A coarse gate, and plan-04 §13 question 6 records that it is one:
    /// `Detection` was shaped for language plugins, so this says "a Rust
    /// repository containing protos" rather than "a repository whose protos I
    /// can read". Nothing is unsound, because [`RootProvider::coverage`]
    /// reports what was actually examined.
    fn detection(&self) -> Detection {
        Detection {
            marker_files: &["Cargo.toml"],
            extensions: &[".proto"],
        }
    }

    /// What this plugin can contribute before it is asked to contribute it.
    ///
    /// A repository with no `.proto` file is not an error — many are not gRPC
    /// services — but a roots plugin that will emit nothing should say so
    /// rather than let an empty root set read as a repository with no
    /// endpoints. That distinction is ADR-0007's whole subject.
    fn preflight(&self, root: &Path) -> Preflight {
        match discover(root) {
            Ok(found) if found.is_empty() => Preflight::Warned {
                reason: format!(
                    "no `.proto` contract was found under {}, so this plugin contributes no roots",
                    root.display()
                ),
                remediation: "if this repository serves gRPC, check that its contracts are \
                              checked in rather than generated into `target/`"
                    .to_owned(),
            },
            Ok(_) => Preflight::Ok,
            Err(source) => Preflight::Failed {
                reason: format!(
                    "{} could not be walked for contracts: {source}",
                    root.display()
                ),
                remediation: "check that the directory exists and is readable".to_owned(),
            },
        }
    }

    /// Nothing, and the empty list is the finding rather than a stub.
    ///
    /// Every fact this plugin learns about its own run already has a
    /// **structured** home in the contract: what it examined is
    /// [`reachgraph_plugin_api::Coverage`], and an operation it could not bind
    /// is a [`reachgraph_plugin_api::RootBinding::Unbound`] carrying the
    /// provider's own reason. Restating either as free text would put one fact
    /// in two shapes, and a consumer would then have to decide which spelling
    /// to trust — the failure a plugin-authored note exists to avoid, not to
    /// create.
    ///
    /// A note belongs here only for something the contract has no field for,
    /// which is why `reachgraph-lang-rust` has three and this crate has none.
    fn notes(&self) -> Vec<String> {
        Vec::new()
    }
}

impl RootProvider for ProtoTonicPlugin {
    fn roots(&self, repo_root: &Path, symbols: &dyn SymbolIndex) -> Result<Vec<Root>, PluginError> {
        let contracts = read_contracts(repo_root)?;
        let sources = read_first_party_sources(repo_root)?;

        let mut roots = Vec::new();
        for contract in &contracts {
            for service in &contract.services {
                let call = direction_of(evidence_for(&service.name, &sources, symbols));
                for rpc in &service.rpcs {
                    let operation = Operation::new(contract.package.as_deref(), &service.name, rpc);
                    let binding = match call {
                        DirectionCall::Served => bind_handler(symbols, &operation),
                        DirectionCall::ConsumedWithClient => {
                            bind_generated_client(symbols, &operation)
                        }
                        DirectionCall::NoEvidence => {
                            consumed_without_evidence(symbols, &operation, &service.name)
                        }
                    };
                    roots.push(root_of(contract, &operation, call.direction(), binding));
                }
            }
        }

        // Written only once the whole run succeeded: a failed run must not
        // leave behind a coverage claim it did not earn (plan-04 §10).
        self.record(&contracts);
        Ok(roots)
    }

    /// What the last completed run examined.
    ///
    /// Before any run this is empty, which is the honest answer: the plugin has
    /// looked at nothing, and reporting contracts it has not opened would be
    /// the partial-index claim inverted.
    fn coverage(&self) -> Coverage {
        let examined = match self.examined.lock() {
            Ok(guard) => guard.clone(),
            // A poisoned lock means an earlier panic, and claiming coverage on
            // the strength of a panicked run would be a claim nothing supports.
            Err(_) => Examined::default(),
        };
        Coverage {
            contracts: examined.contracts,
            versions: examined.versions,
        }
    }
}

impl ProtoTonicPlugin {
    fn record(&self, contracts: &[ProtoContract]) {
        let examined = Examined {
            contracts: contracts
                .iter()
                .map(|contract| contract.contract.clone())
                .collect(),
            versions: contracts
                .iter()
                .map(|contract| VersionKey {
                    contract: contract.contract.clone(),
                    version: contract.version.clone(),
                })
                .collect(),
        };
        if let Ok(mut guard) = self.examined.lock() {
            *guard = examined;
        }
    }
}

/// One first-party Rust file, as the direction pass needs it.
struct RustSource {
    path: PathBuf,
    relative: PathBuf,
    text: String,
}

/// Every `.proto` under the repository, parsed.
///
/// A file that does not parse fails the run (plan-04 §10): `Coverage` has no
/// slot for "found but unreadable", so skipping it would produce an index that
/// looks complete and silently omits a contract's roots.
fn read_contracts(repo_root: &Path) -> Result<Vec<ProtoContract>, PluginError> {
    let files = discover(repo_root).map_err(|source| PluginError::Io {
        plugin: PLUGIN_ID,
        path: repo_root.to_path_buf(),
        source,
    })?;

    let mut contracts = Vec::with_capacity(files.len());
    for file in files {
        let source = std::fs::read_to_string(&file.path).map_err(|source| PluginError::Io {
            plugin: PLUGIN_ID,
            path: file.path.clone(),
            source,
        })?;
        let contract = parse(&file.contract, &source).map_err(|error| PluginError::Parse {
            plugin: PLUGIN_ID,
            path: file.path.clone(),
            detail: error.detail().to_owned(),
        })?;
        contracts.push(contract);
    }
    Ok(contracts)
}

/// Every first-party, non-test `.rs` file under the repository.
///
/// `target/` is excluded because it is build output, and a Cargo test, bench or
/// example target is excluded because code there is not what the repository
/// serves — MEASURED (plan-04 §1 M6), that exclusion is what stops a test
/// double manufacturing five phantom roots.
///
/// A file that cannot be read as UTF-8 is skipped rather than failing the run,
/// and the asymmetry with a `.proto` is deliberate: an unreadable contract
/// silently removes roots, while an unreadable `.rs` file can only weaken a
/// direction signal, whose absence already degrades to `Consumed` and is
/// reported.
fn read_first_party_sources(repo_root: &Path) -> Result<Vec<RustSource>, PluginError> {
    let mut out = Vec::new();
    walk_rust(repo_root, repo_root, &mut out).map_err(|source| PluginError::Io {
        plugin: PLUGIN_ID,
        path: repo_root.to_path_buf(),
        source,
    })?;
    Ok(out)
}

fn walk_rust(repo_root: &Path, directory: &Path, out: &mut Vec<RustSource>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();

        if entry.file_type()?.is_dir() {
            if name == "target" || name.starts_with('.') {
                continue;
            }
            walk_rust(repo_root, &path, out)?;
            continue;
        }

        if !path.extension().is_some_and(|extension| extension == "rs") {
            continue;
        }
        let relative = path.strip_prefix(repo_root).unwrap_or(&path).to_path_buf();
        if is_test_target_path(&relative) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        out.push(RustSource {
            path,
            relative,
            text,
        });
    }
    Ok(())
}

/// What this repository's own source says about one service.
fn evidence_for(
    service: &str,
    sources: &[RustSource],
    symbols: &dyn SymbolIndex,
) -> ServiceEvidence {
    let mut evidence = ServiceEvidence::default();
    for source in sources {
        if !evidence.first_party_impl
            && symbols_in(symbols, source)
                .iter()
                .any(|symbol| is_served_signal(symbol, service))
        {
            evidence.first_party_impl = true;
        }
        if !evidence.client_reference && mentions_client(&source.text, service) {
            evidence.client_reference = true;
        }
        if evidence.first_party_impl && evidence.client_reference {
            break;
        }
    }
    evidence
}

/// The symbols in one file, under either spelling of its path.
///
/// `SymbolIndex::in_file` compares paths for equality and the contract does not
/// fix whose spelling wins: `reachgraph-lang-rust` was MEASURED to emit
/// repo-relative paths, another plugin may emit absolute ones, and a roots
/// plugin cannot know which produced the index it was handed. Asking both ways
/// is the honest reading of a contract that does not say (plan-00 §8 is where
/// the question belongs at n=2).
fn symbols_in<'a>(symbols: &'a dyn SymbolIndex, source: &RustSource) -> Vec<&'a Symbol> {
    let relative = symbols.in_file(&source.relative);
    if !relative.is_empty() {
        return relative;
    }
    symbols.in_file(&source.path)
}

/// A consumed operation with no evidence at all.
///
/// The generated leaf is still looked for — a stub can be indexed while no
/// first-party file names the client type — and only its absence produces
/// plan-04 §9's row 6 reason.
fn consumed_without_evidence(
    symbols: &dyn SymbolIndex,
    operation: &Operation,
    service: &str,
) -> Binding {
    match bind_generated_client(symbols, operation) {
        bound @ Binding::Bound(_) => bound,
        Binding::Unbound(_) => Binding::Unbound(UnboundReason::NoDirectionEvidence {
            service: service.to_owned(),
            join_key: operation.join_key.clone(),
        }),
    }
}

fn root_of(
    contract: &ProtoContract,
    operation: &Operation,
    direction: Direction,
    binding: Binding,
) -> Root {
    Root {
        contract: contract.contract.clone(),
        version: contract.version.clone(),
        service: operation.service.clone(),
        operation: operation.rpc.clone(),
        direction,
        join_key: operation.join_key.clone(),
        binding: match binding {
            Binding::Bound(node) => RootBinding::Bound(node),
            Binding::Unbound(reason) => RootBinding::Unbound {
                reason: reason.to_string(),
            },
        },
    }
}
