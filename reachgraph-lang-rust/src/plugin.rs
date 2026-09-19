//! `RustPlugin` — the contract, implemented over the engine.
//!
//! Plan-03 §5. Every trait method takes `&self` and `Plugin: Send + Sync`, so
//! the loaded workspace lives behind interior mutability. It is loaded **once
//! per root, lazily, on the first call that needs it**, and `discover_units`,
//! `symbols_in`, `edges_*` and `classify` all share it. A different root
//! invalidates and reloads.
//!
//! # Plan-03 §14 question 5, answered: a `Mutex`, not an `RwLock`
//!
//! §5 sketches `RwLock<Option<Loaded>>` and §14 question 5 asks whether
//! `AnalysisHost` and `Analysis` are `Send` and `Sync`. **MEASURED
//! 2026-09-19, by the compiler: `Send` but not `Sync`.** `RootDatabase`
//! reaches `salsa::plumbing::ZalsaLocal`, which holds a
//! `RefCell<QueryStack>` and an `UnsafeCell<HashMap<…>>`; salsa's own model is
//! one database handle per thread.
//!
//! `RwLock<T>: Sync` requires `T: Send + Sync`, so an `RwLock` cannot hold it.
//! `Mutex<T>: Sync` requires only `T: Send`, so a `Mutex` can. The consequence
//! is real rather than cosmetic and is recorded here because it constrains
//! every future caller: **calls into this plugin serialise.** Two units cannot
//! be walked concurrently through one `RustPlugin`. ADR-0006 already concludes
//! that this is a build-time artifact rather than an interactive tool
//! (`outgoing_calls` is one engine round trip per node, and a whole-repository
//! walk is minutes), so the serialisation costs parallelism the design was not
//! relying on.
//!
//! §5's alternative — mint a fresh snapshot per call under the lock — does not
//! help. `Analysis` is the same non-`Sync` type, so a stored snapshot and a
//! fresh one are equally unshareable; what the lock kind decides is whether
//! the crate compiles, not how long a snapshot lives.

use std::path::Path;
use std::sync::Mutex;

use reachgraph_plugin_api::{
    Capability, Category, Classifier, Detection, Edge, EdgeProvider, LanguagePlugin, NodeId,
    Plugin, PluginError, PluginId, PositionEncoding, Preflight, Symbol, SymbolProvider, Unit,
};

use crate::coverage::RustCoverage;
use crate::engine::{self, Loaded};
use crate::preflight::{preflight_outcome, CargoProbe, PreflightFacts, WorkspaceProbe};
use crate::PLUGIN_ID;

/// The Rust plugin.
#[derive(Default)]
pub struct RustPlugin {
    loaded: Mutex<Option<Loaded>>,
}

impl RustPlugin {
    /// A plugin that has loaded nothing yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Load `root` if it is not the currently loaded root, then run `f`.
    ///
    /// The lock is held across `f`, and it has to be: `Loaded` owns both the
    /// non-`Sync` database and the `Vfs` whose interning every `FileId` inside
    /// depends on (ADR-0008 leak 2). Handing a borrow out past the lock would
    /// let a reload invalidate ids a caller was still holding.
    fn with_loaded<T>(
        &self,
        root: &Path,
        f: impl FnOnce(&Loaded) -> Result<T, PluginError>,
    ) -> Result<T, PluginError> {
        let mut guard = self.loaded.lock().map_err(|_| poisoned())?;
        let stale = guard.as_ref().is_none_or(|loaded| !loaded.covers(root));
        if stale {
            // A different root invalidates the whole of `Loaded`, the `Vfs`
            // interning included (plan-03 §6's consistency rule). Nothing is
            // carried across.
            *guard = Some(engine::load(root)?);
        }
        let loaded = guard.as_ref().expect("loaded above when stale");
        f(loaded)
    }

    /// The root a `Unit` belongs to.
    ///
    /// `symbols_in` and `edges_in` take a `Unit` and no root, and a `Unit`
    /// carries its own crate directory rather than the repository's. Loading
    /// from the unit's own root is correct for a Cargo workspace: manifest
    /// discovery walks **up**, so any member resolves the same workspace.
    ///
    /// What is NOT correct is anchoring the emitted paths to it — see
    /// `Loaded::root`. A load reached through a member directory covers the
    /// whole workspace and renders paths against the workspace root, so
    /// `discover_units` and `symbols_in` agree on every `NodeId`.
    fn root_of(unit: &Unit) -> &Path {
        &unit.root
    }

    /// What the last load could not see.
    ///
    /// Plan-03 §11 specifies these facts as a run record in the artifact.
    /// There is no channel for them in the shipped contract, so they are here
    /// — see [`crate::coverage`] and plan-00 §8 question 8. Returns `None`
    /// before the first load.
    pub fn coverage(&self) -> Option<RustCoverage> {
        let guard = self.loaded.lock().ok()?;
        guard.as_ref().map(|loaded| loaded.coverage().clone())
    }
}

fn poisoned() -> PluginError {
    PluginError::Engine {
        plugin: PLUGIN_ID,
        engine: crate::ENGINE.to_owned(),
        detail: "the loaded workspace lock was poisoned by an earlier panic".to_owned(),
    }
}

impl Plugin for RustPlugin {
    fn id(&self) -> PluginId {
        PLUGIN_ID
    }

    fn provides(&self) -> &[Capability] {
        &[Capability::Symbols, Capability::Edges, Capability::Classify]
    }

    /// `Utf8Bytes` because `ra_ap` is byte-offset based (ADR-0008 leak 3).
    fn position_encoding(&self) -> PositionEncoding {
        PositionEncoding::Utf8Bytes
    }

    fn detection(&self) -> Detection {
        Detection {
            marker_files: &["Cargo.toml"],
            extensions: &[".rs"],
        }
    }

    /// Plan-03 §11.
    ///
    /// Check 1a runs before anything else and never resolves a name; checks 2,
    /// 3 and 4 read a load that already happened, because "was this indexed"
    /// cannot be answered without loading.
    fn preflight(&self, root: &Path) -> Preflight {
        let cargo = engine::probe_cargo();
        if matches!(cargo, CargoProbe::DidNotRespond { .. }) {
            return preflight_outcome(&PreflightFacts {
                cargo,
                workspace: WorkspaceProbe::Loaded,
                members_with_unindexed_generated_code: Vec::new(),
                rust_src_available: true,
                proc_macro_expansion: crate::coverage::ProcMacroExpansion::Disabled,
            });
        }

        match self.with_loaded(root, |loaded| Ok(loaded.preflight_facts(cargo.clone()))) {
            Ok(facts) => preflight_outcome(&facts),
            Err(error) => preflight_outcome(&PreflightFacts {
                cargo,
                workspace: WorkspaceProbe::NotResolvable {
                    root: root.display().to_string(),
                    detail: error.to_string(),
                },
                members_with_unindexed_generated_code: Vec::new(),
                rust_src_available: true,
                proc_macro_expansion: crate::coverage::ProcMacroExpansion::Disabled,
            }),
        }
    }
}

impl LanguagePlugin for RustPlugin {
    /// Plan-03 §7 — one `Unit` per workspace member target.
    ///
    /// Dependency crates are **not** units. They are still indexed and still
    /// receive symbols and `NodeId`s when an edge points into them; they are
    /// simply not walked. That is what keeps a whole-repository pass bounded to
    /// first-party code while leaving the edge into a dependency visible and
    /// classified.
    fn discover_units(&self, root: &Path) -> Result<Vec<Unit>, PluginError> {
        self.with_loaded(root, |loaded| Ok(loaded.units()))
    }
}

impl SymbolProvider for RustPlugin {
    fn symbols_in(&self, unit: &Unit) -> Result<Vec<Symbol>, PluginError> {
        self.with_loaded(Self::root_of(unit), |loaded| loaded.symbols_in(&unit.id))
    }
}

impl EdgeProvider for RustPlugin {
    fn edges_in(&self, unit: &Unit) -> Result<Vec<Edge>, PluginError> {
        self.with_loaded(Self::root_of(unit), |loaded| loaded.edges_in(&unit.id))
    }

    /// Plan-00 §8 question 1, answered: **no `&Unit` is needed**, because
    /// `NodeId::raw` is self-describing and carries its unit id (plan-03 §6).
    ///
    /// The cost of that answer is visible here: with no unit and no root in the
    /// signature, this method can only work against a workspace that is already
    /// loaded. A call before any load has no root to load *from* — the node
    /// names a unit, and a unit id is not a directory. That is reported as
    /// [`PluginError::UnknownNode`] rather than guessed at.
    fn edges_from(&self, node: &NodeId) -> Result<Vec<Edge>, PluginError> {
        let guard = self.loaded.lock().map_err(|_| poisoned())?;
        let Some(loaded) = guard.as_ref() else {
            return Err(PluginError::UnknownNode {
                plugin: PLUGIN_ID,
                node: node.clone(),
            });
        };
        loaded.edges_from(node)
    }
}

impl Classifier for RustPlugin {
    /// Plan-03 §10.
    ///
    /// Infallible by the contract's signature, so an unloaded plugin has to
    /// answer something. It answers `ThirdParty`, which is the category that
    /// claims least: it says "outside this unit's package" and nothing more.
    /// Answering `FirstParty` would assert membership nothing checked.
    fn classify(&self, path: &Path, unit: &Unit) -> Category {
        let Ok(guard) = self.loaded.lock() else {
            return Category::ThirdParty;
        };
        match guard.as_ref() {
            Some(loaded) => loaded.classify(path, unit),
            None => Category::ThirdParty,
        }
    }
}
