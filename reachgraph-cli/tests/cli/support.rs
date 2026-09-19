//! Shared harness: a registry holding the fixture, and a temporary directory.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use reachgraph_fixture::format::FixtureDoc;
use reachgraph_fixture::{FixturePlugin, FIXTURE_DOCUMENT_NAME};
use reachgraph_plugin_api::{
    Capability, Category, Classifier, Coverage, Detection, Edge, EdgeProvider, LanguagePlugin,
    NodeId, Plugin, PluginError, PluginId, PositionEncoding, Preflight, Registration, Registry,
    Root, RootProvider, Symbol, SymbolIndex, SymbolProvider, Unit,
};

/// The fixture corpus, reached from this crate's directory.
pub fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the crate has a parent directory")
        .join("reachgraph-fixture")
        .join("fixtures")
}

pub fn doc_of(name: &str) -> FixtureDoc {
    let path = fixtures_dir().join(name).join(FIXTURE_DOCUMENT_NAME);
    let text = fs::read_to_string(&path).expect("the case is readable");
    serde_json::from_str(&text).expect("the case parses")
}

/// A fixture case wearing a marker file, so `Registry::detect` can find it.
///
/// The fixture plugin declares no markers and is **structurally undetectable**
/// (plan-00 §5, plan-02 §1) — that is what stops a hand-written JSON document
/// from masquerading as an analysis in a shipped binary. The cli's analyse path
/// is driven by detection, so testing it needs a plugin that detection returns,
/// and this double is the smallest thing that is one: it declares a marker and
/// delegates every other method unaltered.
pub struct DetectableFixture {
    inner: FixturePlugin,
}

impl DetectableFixture {
    pub fn new(inner: FixturePlugin) -> Self {
        Self { inner }
    }
}

impl Plugin for DetectableFixture {
    fn id(&self) -> PluginId {
        self.inner.id()
    }

    fn provides(&self) -> &[Capability] {
        self.inner.provides()
    }

    fn position_encoding(&self) -> PositionEncoding {
        self.inner.position_encoding()
    }

    /// The one method that is not delegation.
    fn detection(&self) -> Detection {
        Detection {
            marker_files: &[FIXTURE_DOCUMENT_NAME],
            extensions: &["json"],
        }
    }

    fn preflight(&self, root: &Path) -> Preflight {
        self.inner.preflight(root)
    }

    fn notes(&self) -> Vec<String> {
        self.inner.notes()
    }
}

impl LanguagePlugin for DetectableFixture {
    fn discover_units(&self, root: &Path) -> Result<Vec<Unit>, PluginError> {
        self.inner.discover_units(root)
    }
}

impl SymbolProvider for DetectableFixture {
    fn symbols_in(&self, unit: &Unit) -> Result<Vec<Symbol>, PluginError> {
        self.inner.symbols_in(unit)
    }
}

impl EdgeProvider for DetectableFixture {
    fn edges_in(&self, unit: &Unit) -> Result<Vec<Edge>, PluginError> {
        self.inner.edges_in(unit)
    }

    fn edges_from(&self, node: &NodeId) -> Result<Vec<Edge>, PluginError> {
        self.inner.edges_from(node)
    }
}

impl RootProvider for DetectableFixture {
    fn roots(&self, repo_root: &Path, symbols: &dyn SymbolIndex) -> Result<Vec<Root>, PluginError> {
        self.inner.roots(repo_root, symbols)
    }

    fn coverage(&self) -> Coverage {
        self.inner.coverage()
    }
}

impl Classifier for DetectableFixture {
    fn classify(&self, path: &Path, unit: &Unit) -> Category {
        self.inner.classify(path, unit)
    }
}

/// A registry holding one detectable fixture case, with every view it declares.
pub fn registry_of(doc: FixtureDoc, case_dir: &Path) -> Registry {
    let mut registry = Registry::new();
    registry
        .register(
            Registration::of(DetectableFixture::new(FixturePlugin::from_doc(
                case_dir, doc,
            )))
            .symbols()
            .edges()
            .roots()
            .classifier(),
        )
        .expect("the case declares every capability it hands over");
    registry
}

/// A directory under the system temporary directory, deleted on drop.
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(label: &str) -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "reachgraph-cli-{}-{label}-{unique}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("the system temporary directory is writable");
        Self(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// What one `run_with` call produced.
pub struct Run {
    pub code: u8,
    pub out: String,
    pub err: String,
}

/// Drive the cli in process.
pub fn run(registry: &Registry, args: &[&str]) -> Run {
    let owned: Vec<String> = args.iter().map(|arg| (*arg).to_owned()).collect();
    let mut out: Vec<u8> = Vec::new();
    let mut err: Vec<u8> = Vec::new();
    let code = {
        let mut streams = reachgraph_cli::Streams {
            out: &mut out,
            err: &mut err,
        };
        reachgraph_cli::run_with(registry, &owned, &mut streams)
    };

    Run {
        code,
        out: String::from_utf8(out).expect("the cli writes utf-8"),
        err: String::from_utf8(err).expect("the cli writes utf-8"),
    }
}

/// The repository a fixture case stands in for: the case directory, plus the
/// marker file the registry's plugin declares.
pub fn repo_for(case: &str, temp: &TempDir) -> PathBuf {
    let source = fixtures_dir().join(case).join(FIXTURE_DOCUMENT_NAME);
    let repo = temp.join("repo");
    fs::create_dir_all(&repo).expect("the temporary directory is writable");
    fs::copy(source, repo.join(FIXTURE_DOCUMENT_NAME)).expect("the case is readable");
    repo
}
