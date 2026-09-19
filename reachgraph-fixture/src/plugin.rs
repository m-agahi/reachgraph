//! `FixturePlugin` — every trait in the contract, implemented by something that
//! is not a language (plan-02 §3).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use reachgraph_plugin_api::{
    Capability, Category, Classifier, ContractId, Coverage, Detection, DocFormat, Edge,
    EdgeProvider, EdgeTarget, InferenceMode, LanguagePlugin, NodeId, Plugin, PluginError, PluginId,
    PositionEncoding, Preflight, Provenance, Root, RootBinding, RootProvider, Symbol, SymbolIndex,
    SymbolKind, SymbolProvider, Unit, UnitId, VersionKey,
};

use crate::format::{
    FixtureCapability, FixtureCategory, FixtureDirection, FixtureDoc, FixtureDocFormat,
    FixtureEdgeTarget, FixtureInferenceMode, FixturePositionEncoding, FixturePreflight, FixtureRaw,
    FixtureRootBinding, FixtureSymbolKind, FixtureUnitId,
};
use crate::intern;

/// The one file a fixture case directory contains.
///
/// A case looks to the core exactly like a repository looks: it is handed a
/// directory. What is in the directory is one JSON document and no source,
/// which is ADR-0008 leak 4 made structural — a `Cargo.toml` requirement in
/// `discover_units` or `preflight` would fail every case at once.
pub const FIXTURE_DOCUMENT_NAME: &str = "reachgraph.fixture.json";

/// The identity a load failure is attributed to.
///
/// It is the crate's own name rather than any case's `plugin_id`, and the
/// distinction is the point: when a document cannot be read or will not parse,
/// its declared id has not been read yet. Reporting a guess here — `"fixture"`,
/// say — would attribute the failure to an identity no case necessarily claims.
pub const LOADER_ID: PluginId = PluginId("reachgraph-fixture");

/// One fixture case, loaded.
///
/// # Why loading is eager and fallible
///
/// Plan-02 §3 sketches `doc: OnceLock<FixtureDoc>` with loading deferred, and
/// gives `preflight` a second job: report `Failed` when the document is missing
/// or will not parse. Those are two different failures wearing one name.
///
/// A document that will not parse is this crate's own input being broken, and
/// [`PluginError::Parse`] is the typed value the contract has for exactly that
/// — strictly more than a `Preflight::Failed` carrying the same text as a
/// string. A case that declares `preflight: { "failed": … }` is the *contract's*
/// preflight being exercised, and the `preflight_fails` case does that.
///
/// Splitting them also removes the problem that made the sketch awkward:
/// [`Plugin::id`] is infallible, so a plugin that exists before its document
/// does has to answer with an id no case declared. Here it cannot exist first.
/// The property §3 actually asks for — parse once, every later call reads the
/// parsed document — is unchanged; only the moment moved.
#[derive(Debug)]
pub struct FixturePlugin {
    id: PluginId,
    case_dir: PathBuf,
    doc: FixtureDoc,
    capabilities: Vec<Capability>,
    detection: Detection,
    declared_units: BTreeSet<FixtureUnitId>,
    declared_symbols: BTreeSet<FixtureRaw>,
}

impl FixturePlugin {
    /// Read and parse `<case_dir>/reachgraph.fixture.json`.
    ///
    /// **Nothing else on disk is touched, then or later.** The paths a case
    /// writes in `file` and `root` are labels; `"src/service/handlers_v1.rs"`
    /// names nothing that exists. A fixture that had to ship real files would
    /// have re-acquired the build-state prerequisite ADR-0008 leak 4 exists to
    /// keep out, one directory deeper (plan-02 §3.1).
    pub fn load(case_dir: impl Into<PathBuf>) -> Result<Self, PluginError> {
        let case_dir = case_dir.into();
        let path = case_dir.join(FIXTURE_DOCUMENT_NAME);

        let text = std::fs::read_to_string(&path).map_err(|source| PluginError::Io {
            plugin: LOADER_ID,
            path: path.clone(),
            source,
        })?;

        let doc: FixtureDoc = serde_json::from_str(&text).map_err(|error| PluginError::Parse {
            plugin: LOADER_ID,
            path: path.clone(),
            detail: error.to_string(),
        })?;

        Ok(Self::from_doc(case_dir, doc))
    }

    /// Build a plugin from an already-parsed document.
    ///
    /// Separate from [`FixturePlugin::load`] so a test can assert on a document
    /// it composed itself without writing a file, and so the corpus walk parses
    /// each case exactly once.
    pub fn from_doc(case_dir: impl Into<PathBuf>, doc: FixtureDoc) -> Self {
        let id = PluginId(intern::str(&doc.plugin_id));

        let capabilities = doc
            .capabilities
            .iter()
            .map(|capability| match capability {
                FixtureCapability::Symbols => Capability::Symbols,
                FixtureCapability::Edges => Capability::Edges,
                FixtureCapability::Roots => Capability::Roots,
                FixtureCapability::Classify => Capability::Classify,
            })
            .collect();

        // Interned rather than hardcoded to `&[]`. Every corpus case declares
        // both lists empty and `fixture_detection_is_always_empty` asserts it —
        // but a guard over a value this crate substitutes for the case's own
        // would assert nothing. What a case declares is what gets reported.
        let detection = Detection {
            marker_files: intern::list(&doc.detection.marker_files),
            extensions: intern::list(&doc.detection.extensions),
        };

        let declared_units = doc.units.iter().map(|unit| unit.id.clone()).collect();
        let declared_symbols = doc
            .symbols
            .values()
            .flatten()
            .map(|symbol| symbol.raw.clone())
            .collect();

        Self {
            id,
            case_dir: case_dir.into(),
            doc,
            capabilities,
            detection,
            declared_units,
            declared_symbols,
        }
    }

    /// The directory this case was loaded from.
    pub fn case_dir(&self) -> &Path {
        &self.case_dir
    }

    /// The parsed document, for tests that assert on case data directly.
    pub fn doc(&self) -> &FixtureDoc {
        &self.doc
    }

    /// The engine identity this case declares — what a [`PluginError::Engine`]
    /// would name, and what every edge in the case repeats on its own
    /// provenance.
    pub fn engine(&self) -> &str {
        &self.doc.engine
    }

    /// A case-authored `raw` paired with this document's plugin id.
    ///
    /// The one place a [`NodeId`] is minted. A case never writes a
    /// `{plugin, raw}` pair, so no case can put an id in another plugin's
    /// namespace by accident.
    fn node(&self, raw: &FixtureRaw) -> NodeId {
        NodeId {
            plugin: self.id,
            raw: raw.0.clone(),
        }
    }

    /// The document's key for a unit the core handed back, or
    /// [`PluginError::UnknownUnit`] if this case never emitted it.
    fn key_for(&self, unit: &Unit) -> Result<FixtureUnitId, PluginError> {
        let key = FixtureUnitId(unit.id.0.clone());
        if self.declared_units.contains(&key) {
            Ok(key)
        } else {
            Err(PluginError::UnknownUnit {
                plugin: self.id,
                unit: unit.id.clone(),
            })
        }
    }

    fn edge(&self, row: &crate::format::FixtureEdge) -> Edge {
        Edge {
            from: self.node(&row.from),
            to: match &row.to {
                FixtureEdgeTarget::Resolved(raw) => EdgeTarget::Resolved(self.node(raw)),
                FixtureEdgeTarget::Unresolved(target) => EdgeTarget::Unresolved {
                    name: target.name.clone(),
                    candidates: target
                        .candidates
                        .iter()
                        .map(|candidate| self.node(candidate))
                        .collect(),
                },
            },
            // Always. A fixture knows which file a symbol is in because the
            // case author typed it; it has no offsets at all, and a call site
            // is an offset (plan-02 §3.1).
            call_site: None,
            provenance: Provenance {
                plugin: PluginId(intern::str(&row.provenance_plugin)),
                engine: row.engine.clone(),
            },
            inference_mode: match row.inference_mode {
                FixtureInferenceMode::Resolved => InferenceMode::Resolved,
                FixtureInferenceMode::Lexical => InferenceMode::Lexical,
                FixtureInferenceMode::TypeInferred => InferenceMode::TypeInferred,
                FixtureInferenceMode::Enclosure => InferenceMode::Enclosure,
            },
        }
    }
}

fn category(value: FixtureCategory) -> Category {
    match value {
        FixtureCategory::FirstParty => Category::FirstParty,
        FixtureCategory::Generated => Category::Generated,
        FixtureCategory::WorkspaceSibling => Category::WorkspaceSibling,
        FixtureCategory::ThirdParty => Category::ThirdParty,
        FixtureCategory::Stdlib => Category::Stdlib,
    }
}

impl Plugin for FixturePlugin {
    fn id(&self) -> PluginId {
        self.id
    }

    fn provides(&self) -> &[Capability] {
        &self.capabilities
    }

    fn position_encoding(&self) -> PositionEncoding {
        match self.doc.position_encoding {
            FixturePositionEncoding::Utf8Bytes => PositionEncoding::Utf8Bytes,
            FixturePositionEncoding::Utf16CodeUnits => PositionEncoding::Utf16CodeUnits,
            FixturePositionEncoding::Utf32CodePoints => PositionEncoding::Utf32CodePoints,
        }
    }

    fn detection(&self) -> Detection {
        self.detection.clone()
    }

    /// The case's own declaration, and **no filesystem check whatever**.
    ///
    /// `root` is accepted and ignored. Every path a case names is a label
    /// (plan-02 §3.1), so statting one would assert a fact the format does not
    /// claim; and looking for a manifest or a build directory is ADR-0008
    /// leak 4 arriving through the back door. `docs/design.md` §10 MEASURED the
    /// related failure at the other end: a name resolving on `PATH` proves
    /// nothing, because `rust-analyzer` resolved there as a `rustup` proxy that
    /// loops and is not installed.
    ///
    /// There is no path from here to [`Preflight::Warned`]. The format has no
    /// spelling for it — see [`crate::format::FixturePreflight`].
    fn preflight(&self, _root: &Path) -> Preflight {
        match &self.doc.preflight {
            FixturePreflight::Ok => Preflight::Ok,
            FixturePreflight::Failed(failure) => Preflight::Failed {
                reason: failure.reason.clone(),
                remediation: failure.remediation.clone(),
            },
        }
    }
}

impl LanguagePlugin for FixturePlugin {
    /// The units the case declares. `root` is accepted and ignored, for the
    /// reason [`Plugin::preflight`] gives.
    fn discover_units(&self, _root: &Path) -> Result<Vec<Unit>, PluginError> {
        Ok(self
            .doc
            .units
            .iter()
            .map(|unit| Unit {
                id: UnitId(unit.id.0.clone()),
                display_name: unit.display_name.clone(),
                root: unit.root.clone(),
            })
            .collect())
    }
}

impl SymbolProvider for FixturePlugin {
    fn symbols_in(&self, unit: &Unit) -> Result<Vec<Symbol>, PluginError> {
        let key = self.key_for(unit)?;

        Ok(self
            .doc
            .symbols
            .get(&key)
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .map(|row| Symbol {
                id: self.node(&row.raw),
                name: row.name.clone(),
                kind: match row.kind {
                    FixtureSymbolKind::Function => SymbolKind::Function,
                    FixtureSymbolKind::Method => SymbolKind::Method,
                    FixtureSymbolKind::Type => SymbolKind::Type,
                    FixtureSymbolKind::Module => SymbolKind::Module,
                    FixtureSymbolKind::Field => SymbolKind::Field,
                    FixtureSymbolKind::Other => SymbolKind::Other,
                },
                raw_kind: row.raw_kind.clone(),
                // The file the case author typed, and no span — ever. Not a
                // sentinel `Span { 0, 0 }`, which is indistinguishable from a
                // real offset 0, and not a discarded file either. The fixture
                // says exactly what it knows and exactly what it does not.
                range: reachgraph_plugin_api::SourceRange {
                    file: row.file.clone(),
                    span: None,
                },
                doc: row.doc.clone(),
                doc_format: match row.doc_format {
                    FixtureDocFormat::Plain => DocFormat::Plain,
                    FixtureDocFormat::Markdown => DocFormat::Markdown,
                },
                container: row.container.as_ref().map(|raw| self.node(raw)),
                is_test: row.is_test,
            })
            .collect())
    }
}

impl EdgeProvider for FixturePlugin {
    fn edges_in(&self, unit: &Unit) -> Result<Vec<Edge>, PluginError> {
        let key = self.key_for(unit)?;

        Ok(self
            .doc
            .edges
            .get(&key)
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .map(|row| self.edge(row))
            .collect())
    }

    /// A linear scan of every unit's edges.
    ///
    /// Plan-02 §5.1 names this the method the fixture makes look easy, and the
    /// scan is kept deliberately naive so it keeps looking easy for the right
    /// reason. What `edges_from` costs against a linked `ra_ap` engine is an
    /// OPEN MEASUREMENT; nothing here is evidence about it.
    ///
    /// A node this case never emitted is [`PluginError::UnknownNode`], not an
    /// empty vector. The contract says so in as many words: a node the plugin
    /// did not emit "is never a silent empty result". An edge target no symbol
    /// declares — the `external_target` shape — is such a node.
    fn edges_from(&self, node: &NodeId) -> Result<Vec<Edge>, PluginError> {
        let raw = FixtureRaw(node.raw.clone());

        if node.plugin != self.id || !self.declared_symbols.contains(&raw) {
            return Err(PluginError::UnknownNode {
                plugin: self.id,
                node: node.clone(),
            });
        }

        Ok(self
            .doc
            .edges
            .values()
            .flatten()
            .filter(|row| row.from == raw)
            .map(|row| self.edge(row))
            .collect())
    }
}

impl RootProvider for FixturePlugin {
    /// The roots the case declares.
    ///
    /// **`symbols` is accepted and ignored, deliberately.** A case states its
    /// bindings directly and has no handler-binding logic to perform. That is
    /// itself a neutrality signal: if this trait ever required something only a
    /// real index can answer — a lookup whose result changes the shape of a
    /// returned [`Root`] — the fixture would have to fake it, and faking it is
    /// the moment to stop and ask whether the requirement belongs in the trait.
    /// The argument is still taken, so the signature stays honest.
    fn roots(
        &self,
        _repo_root: &Path,
        _symbols: &dyn SymbolIndex,
    ) -> Result<Vec<Root>, PluginError> {
        Ok(self
            .doc
            .roots
            .iter()
            .map(|row| Root {
                contract: ContractId(row.contract.clone()),
                // Copied, never defaulted. `None` here is what the case author
                // wrote as `null` (ADR-0007).
                version: row.version.clone(),
                service: row.service.clone(),
                operation: row.operation.clone(),
                direction: match row.direction {
                    FixtureDirection::Served => reachgraph_plugin_api::Direction::Served,
                    FixtureDirection::Consumed => reachgraph_plugin_api::Direction::Consumed,
                },
                // Verbatim. This crate never parses it, never splits it, and
                // never recovers `version` from it.
                join_key: row.join_key.clone(),
                binding: match &row.binding {
                    FixtureRootBinding::Bound(raw) => RootBinding::Bound(self.node(raw)),
                    FixtureRootBinding::Unbound(unbound) => RootBinding::Unbound {
                        reason: unbound.reason.clone(),
                    },
                },
            })
            .collect())
    }

    fn coverage(&self) -> Coverage {
        Coverage {
            contracts: self
                .doc
                .coverage
                .contracts
                .iter()
                .map(|contract| ContractId(contract.clone()))
                .collect(),
            versions: self
                .doc
                .coverage
                .versions
                .iter()
                .map(|entry| VersionKey {
                    contract: ContractId(entry.contract.clone()),
                    version: entry.version.clone(),
                })
                .collect(),
        }
    }
}

impl Classifier for FixturePlugin {
    /// Longest matching prefix from the case's own rules, else the case's own
    /// fallback.
    ///
    /// ADR-0008 leak 8: there is no prefix compiled into this crate. `src/`,
    /// `vendor/` and `/usr/lib/go/` are things a case author writes, and
    /// plan-01's `core_contains_no_path_prefix` is the paired assertion on the
    /// other side of the waist.
    ///
    /// `unit` is accepted and ignored: classification is per file, which is
    /// what lets one unit hold `src/` and `vendor/` and classify them
    /// differently.
    fn classify(&self, path: &Path, _unit: &Unit) -> Category {
        let path = path.to_string_lossy();

        self.doc
            .classify
            .iter()
            .filter(|rule| path.starts_with(&rule.prefix))
            .max_by_key(|rule| rule.prefix.len())
            .map(|rule| category(rule.category))
            .unwrap_or_else(|| category(self.doc.classify_fallback))
    }
}
