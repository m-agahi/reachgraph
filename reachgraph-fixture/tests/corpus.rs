//! The corpus walk, the invariants that hold over every case in it, and the
//! five cases the plan marks required (plan-02 §6, §7.1, §7.4).
//!
//! # Why the corpus is six cases and not twenty-one
//!
//! Plan-02 §7.1 is explicit: "the corpus grows case by case, pulled by the test
//! that needs it… writing all twenty-one up front would be writing fixtures
//! against an interface no test has exercised yet, which is the opposite of
//! red-first." The six here are the ones a test in *this* crate pulls. The
//! remaining fifteen feed plan-01's suite, which does not exist yet; each
//! arrives with the test that goes red without it.
//!
//! # Why a case can be invalid on purpose
//!
//! `version_key_missing` must fail to parse — that is the whole of what it
//! asserts. Rather than hiding it in a second directory, it is named in
//! [`EXPECTED_INVALID`] and the partition is itself asserted: every listed case
//! must exist and must fail, every unlisted case must parse. A new case
//! directory added without thought therefore lands in "must parse", which is
//! the right default, and cannot go quietly unwalked.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use reachgraph_fixture::format::FixtureDoc;
use reachgraph_fixture::{FixturePlugin, FIXTURE_DOCUMENT_NAME};
use reachgraph_plugin_api::{
    Capability, Category, Classifier, EdgeProvider, EdgeTarget, LanguagePlugin, Plugin, Preflight,
    Root, RootBinding, Symbol, SymbolIndex, SymbolProvider, Unit,
};

/// Cases that must NOT parse. Everything else in `fixtures/` must.
const EXPECTED_INVALID: [&str; 1] = ["version_key_missing"];

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}

/// Every case directory in the corpus, sorted.
fn case_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(fixtures_dir())
        .expect("the corpus directory is readable")
        .map(|entry| entry.expect("a corpus entry is readable").path())
        .filter(|path| path.is_dir())
        .collect();
    dirs.sort();

    assert!(
        !dirs.is_empty(),
        "the corpus is empty, so every invariant over it would pass by finding nothing"
    );
    dirs
}

fn case_name(dir: &Path) -> String {
    dir.file_name()
        .expect("a case directory has a name")
        .to_string_lossy()
        .into_owned()
}

/// Every case that is meant to parse, loaded.
///
/// The `Vec` is asserted non-empty by [`case_dirs`] and again here: a
/// corpus-wide invariant over an empty set is the green tick meaning nothing
/// that plan-02 §7.2 warns about.
fn valid_corpus() -> Vec<(String, FixturePlugin)> {
    let loaded: Vec<(String, FixturePlugin)> = case_dirs()
        .into_iter()
        .filter(|dir| !EXPECTED_INVALID.contains(&case_name(dir).as_str()))
        .map(|dir| {
            let name = case_name(&dir);
            let plugin = FixturePlugin::load(&dir)
                .unwrap_or_else(|error| panic!("{name} should load: {error}"));
            (name, plugin)
        })
        .collect();

    assert!(
        !loaded.is_empty(),
        "every case is listed invalid; the corpus-wide invariants would assert over nothing"
    );
    loaded
}

fn case(name: &str) -> FixturePlugin {
    FixturePlugin::load(fixtures_dir().join(name))
        .unwrap_or_else(|error| panic!("{name} should load: {error}"))
}

/// The one unit of a single-unit case.
fn only_unit(plugin: &FixturePlugin) -> Unit {
    let mut units = plugin
        .discover_units(plugin.case_dir())
        .expect("units discover");
    assert_eq!(units.len(), 1, "this case declares exactly one unit");
    units.remove(0)
}

/// An index that answers nothing.
///
/// `RootProvider::roots` takes a `&dyn SymbolIndex` and the fixture ignores it
/// (plan-02 §3.2). Passing an index that would be useless if consulted is how
/// this file states that the argument is unused rather than merely unimportant:
/// a fixture that started depending on it would fail here rather than silently
/// start needing a real index.
struct NoIndex;

impl SymbolIndex for NoIndex {
    fn by_name(&self, _name: &str) -> Vec<&Symbol> {
        Vec::new()
    }

    fn in_file(&self, _path: &Path) -> Vec<&Symbol> {
        Vec::new()
    }

    fn get(&self, _id: &reachgraph_plugin_api::NodeId) -> Option<&Symbol> {
        None
    }
}

fn roots_of(plugin: &FixturePlugin) -> Vec<Root> {
    use reachgraph_plugin_api::RootProvider;
    plugin
        .roots(plugin.case_dir(), &NoIndex)
        .expect("roots are declared, not computed")
}

// ---------------------------------------------------------------------------
// The walk itself
// ---------------------------------------------------------------------------

/// Plan-02 §7.1 step 1: a fixture document deserialises.
#[test]
fn fixture_doc_parses() {
    let path = fixtures_dir().join("minimal").join(FIXTURE_DOCUMENT_NAME);
    let text = std::fs::read_to_string(&path).expect("the minimal case is readable");

    let doc: FixtureDoc =
        serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()));

    assert_eq!(doc.plugin_id, "fixture");
    assert_eq!(doc.units.len(), 1);
}

/// The partition is asserted, not assumed: a case is either loadable or listed.
#[test]
fn every_case_is_either_loadable_or_listed_invalid() {
    let names: BTreeSet<String> = case_dirs().iter().map(|dir| case_name(dir)).collect();

    for listed in EXPECTED_INVALID {
        assert!(
            names.contains(listed),
            "{listed} is listed invalid but is not in the corpus, so nothing asserts it fails"
        );

        let error = FixturePlugin::load(fixtures_dir().join(listed))
            .err()
            .unwrap_or_else(|| panic!("{listed} is listed invalid but parsed"));

        assert!(
            matches!(error, reachgraph_plugin_api::PluginError::Parse { .. }),
            "{listed} failed, but not as a parse error: {error}"
        );
    }

    // The other half. `valid_corpus` panics on the first case that does not
    // load, so reaching the assertion means every unlisted case parsed.
    let valid = valid_corpus();
    assert_eq!(valid.len(), names.len() - EXPECTED_INVALID.len());
}

/// Plan-02 §7.1 step 3: the providers return the document's contents.
#[test]
fn minimal_case_round_trips() {
    let plugin = case("minimal");
    let unit = only_unit(&plugin);

    assert_eq!(plugin.id().0, "fixture");
    assert_eq!(unit.id.0, "unit:app");
    assert_eq!(unit.display_name, "app");
    assert_eq!(unit.root, Path::new("src"));

    let symbols = plugin.symbols_in(&unit).expect("symbols are declared");
    let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["create_task", "insert_task"]);
    assert_eq!(symbols[0].doc.as_deref(), Some("Create a task."));
    assert_eq!(symbols[1].doc, None);

    let edges = plugin.edges_in(&unit).expect("edges are declared");
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].from.raw, "fn:handlers/create_task");
    match &edges[0].to {
        EdgeTarget::Resolved(node) => assert_eq!(node.raw, "fn:db/insert_task"),
        other => panic!("the minimal edge resolves: {other:?}"),
    }
    assert_eq!(edges[0].provenance.plugin.0, "fixture");
    assert_eq!(edges[0].call_site, None);

    let roots = roots_of(&plugin);
    assert_eq!(roots.len(), 1);
    assert_eq!(roots[0].join_key, "acme.task.v1.TaskService/CreateTask");

    assert_eq!(
        plugin.classify(Path::new("src/db/insert.rs"), &unit),
        Category::FirstParty
    );
    assert_eq!(
        plugin.classify(Path::new("vendor/x.rs"), &unit),
        Category::ThirdParty
    );
    assert!(matches!(plugin.preflight(plugin.case_dir()), Preflight::Ok));
}

// ---------------------------------------------------------------------------
// Corpus-wide invariants (plan-02 §7.1 step 7, §7.4)
// ---------------------------------------------------------------------------

/// Plan-02 §7.1 step 7 and §3.1.
///
/// The property has never changed through three spellings of the type: **the
/// fixture does not lie about offsets it does not have.** It does not emit a
/// sentinel `Span { 0, 0 }`, which is indistinguishable from a real offset 0,
/// and it does not throw away the file in order to be honest about the span.
#[test]
fn fixture_symbols_have_no_span() {
    let mut seen = 0usize;

    for (name, plugin) in valid_corpus() {
        for unit in plugin.discover_units(plugin.case_dir()).expect("units") {
            for symbol in plugin.symbols_in(&unit).expect("symbols") {
                assert!(
                    symbol.range.span.is_none(),
                    "{name}: {} carries a span. The fixture has no offsets; a case that \
                     supplies one is the contract asking for something this crate must not \
                     invent.",
                    symbol.id.raw
                );
                assert!(
                    !symbol.range.file.as_os_str().is_empty(),
                    "{name}: {} has no file. The file is known and must not be discarded.",
                    symbol.id.raw
                );
                seen += 1;
            }
        }
    }

    assert!(seen > 0, "no symbol was examined, so this asserted nothing");
}

/// Plan-02 §7.4 and §3.2 — the declaration half.
///
/// Plan-01's `fixture_is_never_detected` asserts what `Registry::detect` *does*
/// with an empty declaration. This asserts that every case makes one. Either
/// could regress without the other, so neither is a duplicate of the other.
#[test]
fn fixture_detection_is_always_empty() {
    let mut seen = 0usize;

    for (name, plugin) in valid_corpus() {
        let detection = plugin.detection();
        assert!(
            detection.marker_files.is_empty(),
            "{name} declares marker files {:?}. An empty marker list matches nothing \
             (plan-00 §2); a non-empty one would let this plugin claim a repository that \
             happens to contain a reachgraph.fixture.json and replace real analysis with \
             hand-written JSON.",
            detection.marker_files
        );
        assert!(
            detection.extensions.is_empty(),
            "{name} declares extensions {:?}",
            detection.extensions
        );
        seen += 1;
    }

    assert!(seen > 0, "no case was examined, so this asserted nothing");
}

/// Plan-02 §3.1: a fixture `file` is a label, not a claim about disk.
///
/// Two halves, and the second is what makes the first mean something. No path
/// any case names exists — not under the case directory, not as an absolute
/// path — and `preflight` answers exactly what the case declared anyway. So
/// preflight passed while every path in the document was fictional, which is
/// the behaviour §3.1 requires rather than a promise that nobody will check.
#[test]
fn fixture_file_is_never_checked_against_disk() {
    let mut seen = 0usize;

    for (name, plugin) in valid_corpus() {
        for unit in plugin.discover_units(plugin.case_dir()).expect("units") {
            for symbol in plugin.symbols_in(&unit).expect("symbols") {
                let file = &symbol.range.file;
                assert!(
                    !plugin.case_dir().join(file).exists() && !file.exists(),
                    "{name}: {} resolves to a real file. Every fixture path must name \
                     nothing that exists, or a later reader will take these for claims \
                     about disk — and shipping real files re-acquires the build-state \
                     prerequisite ADR-0008 leak 4 keeps out.",
                    file.display()
                );
                seen += 1;
            }
        }

        let declared_ok = matches!(
            plugin.doc().preflight,
            reachgraph_fixture::format::FixturePreflight::Ok
        );
        assert_eq!(
            declared_ok,
            matches!(plugin.preflight(plugin.case_dir()), Preflight::Ok),
            "{name}: preflight disagrees with the case's own declaration, which means it \
             consulted something other than the document"
        );
    }

    assert!(seen > 0, "no path was examined, so this asserted nothing");
}

/// The `symbols` and `edges` maps are keyed by unit, and a key naming no
/// declared unit is data nothing would ever read.
#[test]
fn every_symbol_and_edge_key_names_a_declared_unit() {
    for (name, plugin) in valid_corpus() {
        let declared: BTreeSet<&String> = plugin.doc().units.iter().map(|u| &u.id.0).collect();

        for key in plugin.doc().symbols.keys().chain(plugin.doc().edges.keys()) {
            assert!(
                declared.contains(&key.0),
                "{name}: symbols or edges are keyed by {:?}, which no unit declares, so \
                 they would never be read",
                key.0
            );
        }
    }
}

// ---------------------------------------------------------------------------
// The five required cases (plan-02 §6)
// ---------------------------------------------------------------------------

/// ★ `versioned_pair` — the case ADR-0007 is about.
///
/// One operation, two versions, routing to different code. The waist-side
/// consequence (two shards, `legacy_audit` dying at the v1 sunset while
/// `persist` survives) is plan-01's to assert. What is assertable now is the
/// input that makes it possible: two roots differing only in `version`, bound
/// to two different handlers, each reaching one exclusive callee.
#[test]
fn versioned_pair_routes_two_versions_to_different_code() {
    let plugin = case("versioned_pair");
    let unit = only_unit(&plugin);
    let roots = roots_of(&plugin);

    assert_eq!(roots.len(), 2);
    for root in &roots {
        assert_eq!(root.contract.0, "acme.task");
        assert_eq!(root.operation, "CreateTask");
    }

    let versions: Vec<Option<&str>> = roots.iter().map(|r| r.version.as_deref()).collect();
    assert_eq!(versions, [Some("v1"), Some("v2")]);

    let bound: Vec<&str> = roots
        .iter()
        .map(|root| match &root.binding {
            RootBinding::Bound(node) => node.raw.as_str(),
            RootBinding::Unbound { .. } => panic!("both roots in this case bind"),
        })
        .collect();
    assert_eq!(bound, ["fn:v1/create_task", "fn:v2/create_task"]);

    // The join keys differ, so a core that merged the two versions could not
    // claim it was following the key.
    assert_ne!(roots[0].join_key, roots[1].join_key);

    let targets = |from: &str| -> BTreeSet<String> {
        plugin
            .edges_in(&unit)
            .expect("edges")
            .iter()
            .filter(|edge| edge.from.raw == from)
            .map(|edge| match &edge.to {
                EdgeTarget::Resolved(node) => node.raw.clone(),
                EdgeTarget::Unresolved { name, .. } => panic!("{name} is unresolved here"),
            })
            .collect()
    };

    let from_v1 = targets("fn:v1/create_task");
    let from_v2 = targets("fn:v2/create_task");

    assert!(from_v1.contains("fn:shared/persist") && from_v2.contains("fn:shared/persist"));
    assert!(
        from_v1.contains("fn:v1only/legacy_audit") && !from_v2.contains("fn:v1only/legacy_audit")
    );
    assert!(from_v2.contains("fn:v2only/validate") && !from_v1.contains("fn:v2only/validate"));

    // A container is not a call target, so the `impl` node is reached by
    // nobody. Pinned here because it is mildly surprising and correct, and
    // therefore the kind of thing a later reader "fixes".
    let reached: BTreeSet<String> = from_v1.union(&from_v2).cloned().collect();
    assert!(!reached.contains("impl:TaskService_for_TaskServer"));
    assert!(!reached.contains("fn:orphan/unused_helper"));
}

/// ★ `unversioned_contract` — `None` is an assertion, never a default.
#[test]
fn unversioned_contract_version_is_none_and_never_v1() {
    let plugin = case("unversioned_contract");
    let roots = roots_of(&plugin);

    assert_eq!(roots.len(), 1);
    assert_eq!(
        roots[0].version, None,
        "ADR-0007: the case wrote `null` and the plugin must carry it through"
    );

    let coverage = {
        use reachgraph_plugin_api::RootProvider;
        plugin.coverage()
    };
    assert_eq!(coverage.versions.len(), 1);
    assert_eq!(
        coverage.versions[0].version, None,
        "a null version is a real coverage entry, not a gap in the list"
    );

    // The join key names no version either, and — more importantly — the
    // plugin never reads it to recover one.
    assert_eq!(roots[0].join_key, "acme.legacy.PingService/Ping");
}

/// ★ `unbound_root` — a reported gap, never a dropped row.
#[test]
fn unbound_root_is_reported_not_dropped() {
    let plugin = case("unbound_root");
    let roots = roots_of(&plugin);

    assert_eq!(roots.len(), 2, "the unbound root is still a row");

    let unbound: Vec<&Root> = roots
        .iter()
        .filter(|root| matches!(root.binding, RootBinding::Unbound { .. }))
        .collect();
    assert_eq!(unbound.len(), 1);
    assert_eq!(unbound[0].operation, "DeleteTask");

    match &unbound[0].binding {
        RootBinding::Unbound { reason } => assert!(
            reason.contains("DeleteTask"),
            "the reason is plugin-authored text carried into the artifact, not a flag"
        ),
        RootBinding::Bound(_) => unreachable!(),
    }

    // The unbound operation is still inside the declared coverage, which is
    // what stops it reading as something the provider never looked at.
    let coverage = {
        use reachgraph_plugin_api::RootProvider;
        plugin.coverage()
    };
    assert!(coverage
        .versions
        .iter()
        .any(|key| key.contract.0 == "acme.task" && key.version.as_deref() == Some("v1")));
}

/// ★ `unresolved_edge` — the candidates survive, and no guess is made.
#[test]
fn unresolved_edge_keeps_its_candidates_and_resolves_nothing() {
    let plugin = case("unresolved_edge");
    let unit = only_unit(&plugin);
    let edges = plugin.edges_in(&unit).expect("edges");

    assert_eq!(edges.len(), 1);
    match &edges[0].to {
        EdgeTarget::Unresolved { name, candidates } => {
            assert_eq!(name, "execute");
            let raws: Vec<&str> = candidates.iter().map(|c| c.raw.as_str()).collect();
            assert_eq!(raws, ["fn:db/execute_pg", "fn:db/execute_sqlite"]);
        }
        EdgeTarget::Resolved(node) => panic!(
            "design.md §8: show a missing edge as missing. This one was resolved to {}",
            node.raw
        ),
    }

    // Neither candidate is the target of any resolved edge, so nothing else in
    // the case can make `execute_sqlite` reachable by accident.
    assert!(edges
        .iter()
        .all(|edge| !matches!(&edge.to, EdgeTarget::Resolved(_))));
}

/// ★ `test_symbol_collision` — design.md §4's MEASURED collision.
///
/// Two symbols named `create_task` in one repository. Name alone cannot tell
/// the handler from the mock; `container` and `is_test` together can. The waist
/// never interprets `container` — it is copied, like [`reachgraph_plugin_api::NodeId::raw`].
#[test]
fn test_symbol_collision_is_separable_by_container_and_is_test() {
    let plugin = case("test_symbol_collision");
    let unit = only_unit(&plugin);
    let symbols = plugin.symbols_in(&unit).expect("symbols");

    let colliding: Vec<&Symbol> = symbols.iter().filter(|s| s.name == "create_task").collect();
    assert_eq!(
        colliding.len(),
        2,
        "the collision is the point of this case"
    );

    let real = colliding
        .iter()
        .find(|s| !s.is_test)
        .expect("one of the two is the real handler");
    let mock = colliding
        .iter()
        .find(|s| s.is_test)
        .expect("the other is the mock");

    assert_ne!(real.id.raw, mock.id.raw);
    assert_eq!(
        real.container.as_ref().map(|c| c.raw.as_str()),
        Some("impl:TaskService_for_TaskServer")
    );
    assert_eq!(
        mock.container.as_ref().map(|c| c.raw.as_str()),
        Some("impl:MockDb")
    );

    // Copied verbatim and into this plugin's own namespace — never parsed,
    // never split, never matched on.
    for symbol in [real, mock] {
        let container = symbol.container.as_ref().expect("both are contained");
        assert_eq!(container.plugin, plugin.id());
        assert!(symbols.iter().any(|s| s.id == *container));
    }
}

/// The declared capability set is case data, which is how plan-01's
/// unpaired-provider error gets a case to fail on later.
#[test]
fn capabilities_are_case_data() {
    let plugin = case("minimal");
    assert_eq!(
        plugin.provides(),
        [
            Capability::Symbols,
            Capability::Edges,
            Capability::Roots,
            Capability::Classify
        ]
    );
}
