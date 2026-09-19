//! `reachgraph .` — the invocation a user actually types.
//!
//! MEASURED 2026-09-19 by the release workflow's smoke test, run 35457883578:
//! the shipped binary **panicked** on it.
//!
//! ```text
//! thread 'main' panicked at ra_ap_paths-0.0.352/src/lib.rs:97:36:
//! expected absolute path, got .
//! ```
//!
//! `ra_ap_paths::AbsPathBuf::assert` refuses a relative path by panicking
//! rather than by returning an error, so a relative repository argument became
//! a backtrace out of a dependency with no message of reachgraph's own and no
//! remediation. Every matrix job that EXECUTED the binary failed and every job
//! that only cross-compiled it passed, which is what identifies this as
//! universal rather than platform-specific.
//!
//! # Why this is its own test binary, and why it is ONE test
//!
//! `.` has no meaning without a current directory, so asserting over it means
//! changing one. `std::env::set_current_dir` is process-global while the test
//! harness runs `#[test]` functions on threads, so two tests that both move the
//! current directory race. A separate integration target is a separate process;
//! keeping it to a single `#[test]` that walks the shapes in sequence is what
//! makes the move safe.
//!
//! The shapes that need no current directory — a path that does not exist, a
//! path that is a file, a literal `~` — are ordinary tests in `tests/cli`.

use std::fs;
use std::path::{Path, PathBuf};

use reachgraph_fixture::{FixturePlugin, FIXTURE_DOCUMENT_NAME};
use reachgraph_plugin_api::{
    Capability, Category, Classifier, Coverage, Detection, Edge, EdgeProvider, LanguagePlugin,
    NodeId, Plugin, PluginError, PluginId, PositionEncoding, Preflight, Registration, Registry,
    Root, RootProvider, Symbol, SymbolIndex, SymbolProvider, Unit,
};

/// The fixture plugin declares no markers and is structurally undetectable
/// (plan-00 §5), and the analyse path is detection-driven — so the smallest
/// thing that can be analysed at all is a fixture wearing a marker.
struct DetectableFixture(FixturePlugin);

impl Plugin for DetectableFixture {
    fn id(&self) -> PluginId {
        self.0.id()
    }
    fn provides(&self) -> &[Capability] {
        self.0.provides()
    }
    fn position_encoding(&self) -> PositionEncoding {
        self.0.position_encoding()
    }
    fn detection(&self) -> Detection {
        Detection {
            marker_files: &[FIXTURE_DOCUMENT_NAME],
            extensions: &["json"],
        }
    }
    fn preflight(&self, root: &Path) -> Preflight {
        self.0.preflight(root)
    }
    fn notes(&self) -> Vec<String> {
        self.0.notes()
    }
}

impl LanguagePlugin for DetectableFixture {
    fn discover_units(&self, root: &Path) -> Result<Vec<Unit>, PluginError> {
        self.0.discover_units(root)
    }
}
impl SymbolProvider for DetectableFixture {
    fn symbols_in(&self, unit: &Unit) -> Result<Vec<Symbol>, PluginError> {
        self.0.symbols_in(unit)
    }
}
impl EdgeProvider for DetectableFixture {
    fn edges_in(&self, unit: &Unit) -> Result<Vec<Edge>, PluginError> {
        self.0.edges_in(unit)
    }
    fn edges_from(&self, node: &NodeId) -> Result<Vec<Edge>, PluginError> {
        self.0.edges_from(node)
    }
}
impl RootProvider for DetectableFixture {
    fn roots(&self, repo_root: &Path, symbols: &dyn SymbolIndex) -> Result<Vec<Root>, PluginError> {
        self.0.roots(repo_root, symbols)
    }
    fn coverage(&self) -> Coverage {
        self.0.coverage()
    }
}
impl Classifier for DetectableFixture {
    fn classify(&self, path: &Path, unit: &Unit) -> Category {
        self.0.classify(path, unit)
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the crate has a parent directory")
        .to_path_buf()
}

/// A repository the fixture plugin claims, at an absolute, canonical path
/// under the system temporary directory.
///
/// Canonical because macOS makes `/tmp` a symlink to `/private/tmp`, so a
/// comparison against a non-canonical spelling would fail there and nowhere
/// else — which is the worst shape a test failure can take.
fn repo_at(label: &str) -> PathBuf {
    let source = workspace_root()
        .join("reachgraph-fixture/fixtures/minimal")
        .join(FIXTURE_DOCUMENT_NAME);
    let repo =
        std::env::temp_dir().join(format!("reachgraph-relpath-{}-{label}", std::process::id()));
    let _ = fs::remove_dir_all(&repo);
    fs::create_dir_all(&repo).expect("the system temporary directory is writable");
    fs::copy(source, repo.join(FIXTURE_DOCUMENT_NAME)).expect("the case is readable");
    repo.canonicalize().expect("the repository exists")
}

fn registry_for(repo: &Path) -> Registry {
    let text = fs::read_to_string(repo.join(FIXTURE_DOCUMENT_NAME)).expect("readable");
    let doc = serde_json::from_str(&text).expect("the case parses");
    let mut registry = Registry::new();
    registry
        .register(
            Registration::of(DetectableFixture(FixturePlugin::from_doc(repo, doc)))
                .symbols()
                .edges()
                .roots()
                .classifier(),
        )
        .expect("the case declares every capability it hands over");
    registry
}

/// Run in process and return `(code, stdout, stderr)`.
fn run(registry: &Registry, args: &[&str]) -> (u8, String, String) {
    let owned: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = {
        let mut streams = reachgraph_cli::Streams {
            out: &mut out,
            err: &mut err,
        };
        reachgraph_cli::run_with(registry, &owned, &mut streams)
    };
    (
        code,
        String::from_utf8(out).expect("utf-8"),
        String::from_utf8(err).expect("utf-8"),
    )
}

/// The repository path a run reports, read back out of `--json`.
fn reported_repo(stdout: &str) -> String {
    let value: serde_json::Value = serde_json::from_str(stdout).expect("--json emits json");
    value["repo"]
        .as_str()
        .expect("the report names the repository")
        .to_owned()
}

/// Every relative shape a user can type for the repository, from inside it.
///
/// The assertion is not merely "it does not panic". **The path the cli hands a
/// plugin must be the repository's absolute, canonical path**, because that is
/// the property whose absence made `AbsPathBuf::assert` panic — and because
/// ADR-0003 makes `node_id` an identity, so two spellings of one repository
/// that resolved differently would emit two sets of ids for one codebase.
/// `--json` is how the resolved path is read back without reaching into the
/// cli's internals.
#[test]
fn a_relative_repository_argument_resolves_before_it_reaches_a_plugin() {
    let repo = repo_at("shapes");
    let registry = registry_for(&repo);
    let out_root =
        std::env::temp_dir().join(format!("reachgraph-relpath-out-{}", std::process::id()));
    let _ = fs::remove_dir_all(&out_root);

    // `nested` gives `..` somewhere to come back from; `linked` gives the
    // symlink shape a target. Both are built inside the repository so the
    // shapes are real rather than contrived.
    fs::create_dir_all(repo.join("nested")).expect("writable");
    let linked = repo
        .parent()
        .expect("the repository has a parent")
        .join("reachgraph-relpath-symlink");
    let _ = fs::remove_file(&linked);
    #[cfg(unix)]
    std::os::unix::fs::symlink(&repo, &linked).expect("the temporary directory allows symlinks");

    let previous = std::env::current_dir().expect("the test process has a current directory");
    std::env::set_current_dir(&repo).expect("the fixture repository is a directory");

    let mut shapes: Vec<(&str, String)> = vec![
        // The invocation the smoke test ran, and the one that panicked.
        ("dot", ".".to_owned()),
        // A trailing separator, which `Path` keeps as an empty final component.
        ("trailing-slash", "./".to_owned()),
        // `..` from a subdirectory of the repository back to its root.
        ("dotdot", "nested/..".to_owned()),
    ];
    #[cfg(unix)]
    shapes.push((
        // A symlink to the repository root, named relatively from inside it.
        "symlink",
        format!(
            "../{}",
            linked.file_name().expect("named").to_string_lossy()
        ),
    ));

    for (label, argument) in &shapes {
        let out = out_root.join(label);
        let (code, stdout, stderr) = run(
            &registry,
            &[argument, "--json", "-o", out.to_str().expect("utf-8")],
        );

        assert_eq!(code, 0, "{label}: stderr: {stderr}");
        let reported = reported_repo(&stdout);
        assert_eq!(
            Path::new(&reported),
            repo.as_path(),
            "{label}: `{argument}` reached the plugins as `{reported}`; every spelling of one \
             repository must resolve to one path, and a relative one makes \
             ra_ap_paths::AbsPathBuf::assert panic"
        );
    }

    // A relative path naming a directory NO plugin claims still exits cleanly.
    // `nested` holds no fixture document, so this is ADR-0008's detection
    // speaking — and the message must name the resolved path, not the typed
    // one, or the user cannot tell which directory was looked at.
    let (code, _, stderr) = run(&registry, &["nested", "--json"]);
    assert_eq!(code, 3, "expected EXIT_UNDETECTED, stderr: {stderr}");
    assert!(
        stderr.contains(repo.join("nested").to_str().expect("utf-8")),
        "the message names the resolved path: {stderr}"
    );

    std::env::set_current_dir(previous).expect("the previous directory is still there");
    let _ = fs::remove_file(&linked);
    let _ = fs::remove_dir_all(&out_root);
    let _ = fs::remove_dir_all(&repo);
}
