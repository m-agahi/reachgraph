//! Test doubles for the two things no fixture case can express.
//!
//! A corpus case declares data; it never declares a failure to *produce* that
//! data, and it never records what the core asked it. Both are behaviours of a
//! provider rather than contents of a document, so they are written here and
//! the corpus stays what plan-02 says it is.

use std::cell::RefCell;
use std::path::Path;
use std::sync::Mutex;

use reachgraph_fixture::format::FixtureDoc;
use reachgraph_fixture::{FixturePlugin, FIXTURE_DOCUMENT_NAME};
use reachgraph_plugin_api::{
    Capability, Category, Classifier, Detection, Edge, EdgeProvider, LanguagePlugin, NodeId,
    Plugin, PluginError, PluginId, PositionEncoding, Preflight, Symbol, SymbolProvider, Unit,
};

use crate::support::fixtures_dir;

/// One corpus case, parsed but not yet turned into a plugin.
///
/// A test that needs data no case declares edits the parsed document and builds
/// a plugin from it. That keeps the mutation visible in the test that needs it,
/// rather than adding a corpus case whose only reader is one assertion.
pub fn doc_of(name: &str) -> FixtureDoc {
    let path = fixtures_dir().join(name).join(FIXTURE_DOCUMENT_NAME);
    let text = std::fs::read_to_string(&path).expect("the case is readable");
    serde_json::from_str(&text).expect("the case parses")
}

/// A plugin built from an edited document.
pub fn plugin_from(name: &str, doc: FixtureDoc) -> FixturePlugin {
    FixturePlugin::from_doc(fixtures_dir().join(name), doc)
}

/// A provider that answers every question with the same failure.
///
/// It declares symbols and edges so it pairs with itself: the build must reach
/// the provider call rather than stopping at the pairing check.
pub struct FailingProvider {
    pub id: PluginId,
}

impl Plugin for FailingProvider {
    fn id(&self) -> PluginId {
        self.id
    }

    fn provides(&self) -> &[Capability] {
        &[Capability::Symbols, Capability::Edges]
    }

    fn position_encoding(&self) -> PositionEncoding {
        PositionEncoding::Utf8Bytes
    }

    fn detection(&self) -> Detection {
        Detection {
            marker_files: &[],
            extensions: &[],
        }
    }

    fn preflight(&self, _root: &Path) -> Preflight {
        Preflight::Ok
    }

    /// A double that fails every call has nothing to report about the index it
    /// did not contribute to.
    fn notes(&self) -> Vec<String> {
        Vec::new()
    }
}

impl LanguagePlugin for FailingProvider {
    fn discover_units(&self, _root: &Path) -> Result<Vec<Unit>, PluginError> {
        Err(PluginError::Engine {
            plugin: self.id,
            engine: "a double 0.0.0".to_owned(),
            detail: "this provider fails on purpose".to_owned(),
        })
    }
}

impl SymbolProvider for FailingProvider {
    fn symbols_in(&self, _unit: &Unit) -> Result<Vec<Symbol>, PluginError> {
        Err(PluginError::Engine {
            plugin: self.id,
            engine: "a double 0.0.0".to_owned(),
            detail: "this provider fails on purpose".to_owned(),
        })
    }
}

impl EdgeProvider for FailingProvider {
    fn edges_in(&self, _unit: &Unit) -> Result<Vec<Edge>, PluginError> {
        Err(PluginError::Engine {
            plugin: self.id,
            engine: "a double 0.0.0".to_owned(),
            detail: "this provider fails on purpose".to_owned(),
        })
    }

    fn edges_from(&self, node: &NodeId) -> Result<Vec<Edge>, PluginError> {
        Err(PluginError::UnknownNode {
            plugin: self.id,
            node: node.clone(),
        })
    }
}

/// A classifier that records every path it was asked about.
///
/// Plan-00 §3.5: a core that called into a language crate directly would have
/// re-created ADR-0008's forbidden language branch in a different costume. The
/// spy is how the trait boundary is observed rather than assumed.
pub struct SpyClassifier<'a> {
    inner: &'a FixturePlugin,
    seen: Mutex<RefCell<Vec<String>>>,
}

impl<'a> SpyClassifier<'a> {
    pub fn new(inner: &'a FixturePlugin) -> Self {
        Self {
            inner,
            seen: Mutex::new(RefCell::new(Vec::new())),
        }
    }

    /// Every path the core passed, in call order.
    pub fn calls(&self) -> Vec<String> {
        let guard = self.seen.lock().expect("the spy is not poisoned");
        let calls = guard.borrow().clone();
        calls
    }
}

impl Plugin for SpyClassifier<'_> {
    fn id(&self) -> PluginId {
        self.inner.id()
    }

    fn provides(&self) -> &[Capability] {
        self.inner.provides()
    }

    fn position_encoding(&self) -> PositionEncoding {
        self.inner.position_encoding()
    }

    fn detection(&self) -> Detection {
        self.inner.detection()
    }

    fn preflight(&self, root: &Path) -> Preflight {
        self.inner.preflight(root)
    }

    fn notes(&self) -> Vec<String> {
        self.inner.notes()
    }
}

impl Classifier for SpyClassifier<'_> {
    fn classify(&self, path: &Path, unit: &Unit) -> Category {
        {
            let guard = self.seen.lock().expect("the spy is not poisoned");
            guard.borrow_mut().push(path.to_string_lossy().into_owned());
        }
        self.inner.classify(path, unit)
    }
}

/// A plugin that records the directory the waist preflighted it against, and
/// reports a finding it must not lose.
///
/// No corpus case can express either. `FixturePlugin::preflight` accepts its
/// argument and ignores it on purpose (plan-02 §3.1), so a case cannot observe
/// what it was handed; and the fixture format has no `warned` spelling, so a
/// case cannot return one.
pub struct PreflightSpy<'a> {
    inner: &'a FixturePlugin,
    seen: Mutex<RefCell<Vec<std::path::PathBuf>>>,
    pub reason: &'static str,
    pub remediation: &'static str,
}

impl<'a> PreflightSpy<'a> {
    pub fn new(inner: &'a FixturePlugin) -> Self {
        Self {
            inner,
            seen: Mutex::new(RefCell::new(Vec::new())),
            reason: "a finding the plugin reports while still running",
            remediation: "what the reader should do about the finding",
        }
    }

    /// Every directory the waist passed, in call order.
    pub fn roots(&self) -> Vec<std::path::PathBuf> {
        let guard = self.seen.lock().expect("the spy is not poisoned");
        let roots = guard.borrow().clone();
        roots
    }
}

impl Plugin for PreflightSpy<'_> {
    fn id(&self) -> PluginId {
        self.inner.id()
    }

    fn provides(&self) -> &[Capability] {
        self.inner.provides()
    }

    fn position_encoding(&self) -> PositionEncoding {
        self.inner.position_encoding()
    }

    fn detection(&self) -> Detection {
        self.inner.detection()
    }

    fn preflight(&self, root: &Path) -> Preflight {
        {
            let guard = self.seen.lock().expect("the spy is not poisoned");
            guard.borrow_mut().push(root.to_path_buf());
        }
        Preflight::Warned {
            reason: self.reason.to_owned(),
            remediation: self.remediation.to_owned(),
        }
    }

    fn notes(&self) -> Vec<String> {
        self.inner.notes()
    }
}

impl LanguagePlugin for PreflightSpy<'_> {
    fn discover_units(&self, root: &Path) -> Result<Vec<Unit>, PluginError> {
        self.inner.discover_units(root)
    }
}

impl SymbolProvider for PreflightSpy<'_> {
    fn symbols_in(&self, unit: &Unit) -> Result<Vec<Symbol>, PluginError> {
        self.inner.symbols_in(unit)
    }
}

impl EdgeProvider for PreflightSpy<'_> {
    fn edges_in(&self, unit: &Unit) -> Result<Vec<Edge>, PluginError> {
        self.inner.edges_in(unit)
    }

    fn edges_from(&self, node: &NodeId) -> Result<Vec<Edge>, PluginError> {
        self.inner.edges_from(node)
    }
}
