//! Provider orchestration, pairing and the diagnostics of plan-01 §4.

use std::path::Path;

use reachgraph_core::schema::BindingRow;
use reachgraph_core::{BuildDiagnostic, BuildError, BuildInputs, BuildOptions, Index};
use reachgraph_fixture::format::{FixtureRaw, FixtureRootBinding, FixtureUnitId};
use reachgraph_plugin_api::{Capability, ContractId, NodeId, Plugin, PluginId, UnexaminedContract};

use crate::doubles::{doc_of, plugin_from, FailingProvider, PartialRootProvider, PreflightSpy};
use crate::support::{build, case, emit, endpoints, inputs, shards, unreachable};

/// Plan-01 §10.1 step 1. Symbols and edges are collected per unit from a
/// paired plugin, and both halves of the pair saw the same `Unit` values —
/// which is the whole point of pairing (ADR-0003 field 1).
#[test]
fn pairing_by_plugin_id() {
    let plugin = case("minimal");
    let index = build(&plugin);

    let view = index.view();
    let raws: Vec<&str> = view.nodes.iter().map(|node| node.id.raw.as_str()).collect();
    assert_eq!(raws, ["fn:handlers/create_task", "fn:db/insert_task"]);

    assert_eq!(view.edges.len(), 1);
    assert_eq!(view.edges[0].from.raw, "fn:handlers/create_task");

    assert_eq!(index.coverage().units_indexed.len(), 1);
    assert_eq!(index.coverage().units_indexed[0].0, "unit:app");
}

/// Plan-01 §10.1 step 2, §4.2. **The load-bearing error.** A symbol provider
/// with no edges yields isolated nodes, so every symbol reads as not reachable
/// from any endpoint version in this index — an entire repository reported as
/// such. Failing loudly is the only defensible behaviour.
#[test]
fn unpaired_symbol_provider_is_a_build_error() {
    let plugin = case("symbols_only");
    let error = Index::build(
        plugin.case_dir(),
        &inputs(&plugin),
        &BuildOptions::default(),
    )
    .expect_err("a symbol provider with no edge partner is a hard error");

    assert!(
        matches!(error, BuildError::UnpairedSymbolProvider { plugin } if plugin.0 == "fixture"),
        "{error:?}"
    );
}

/// Plan-01 §4.2. Meaningless rather than dangerous, and still a configuration
/// error: every end of every edge would be unindexed.
#[test]
fn unpaired_edge_provider_is_a_build_error() {
    let plugin = case("minimal");
    let error = Index::build(
        plugin.case_dir(),
        &BuildInputs {
            symbols: Vec::new(),
            edges: vec![&plugin],
            roots: Vec::new(),
            classifiers: Vec::new(),
        },
        &BuildOptions::default(),
    )
    .expect_err("an edge provider with no symbol partner is a hard error");

    assert!(
        matches!(error, BuildError::UnpairedEdgeProvider { plugin } if plugin.0 == "fixture"),
        "{error:?}"
    );
}

/// A provider registered for a capability it does not declare. This is what
/// keeps `provides()` load-bearing rather than decorative.
#[test]
fn provider_registered_for_an_undeclared_capability_is_a_build_error() {
    let plugin = case("no_classifier");
    let error = Index::build(
        plugin.case_dir(),
        &BuildInputs {
            symbols: vec![&plugin],
            edges: vec![&plugin],
            roots: vec![&plugin],
            classifiers: vec![&plugin],
        },
        &BuildOptions::default(),
    )
    .expect_err("this case declares no classify capability");

    assert!(
        matches!(
            error,
            BuildError::CapabilityNotDeclared {
                capability: Capability::Classify,
                ..
            }
        ),
        "{error:?}"
    );
}

/// ADR-0003 field 5. Both strings are surfaced, because a reason with no
/// remediation is a complaint.
#[test]
fn preflight_failure_aborts_with_remediation() {
    let plugin = case("preflight_fails");
    let error = Index::build(
        plugin.case_dir(),
        &inputs(&plugin),
        &BuildOptions::default(),
    )
    .expect_err("a failed preflight aborts");

    match error {
        BuildError::PreflightFailed {
            reason,
            remediation,
            ..
        } => {
            assert!(reason.contains("prerequisite is not met"));
            assert!(remediation.contains("Nothing to do"));
        }
        other => panic!("{other:?}"),
    }
}

/// Plan-01 §4.3. The first symbol wins, and the second is reported: merging
/// would mean deciding which plugin's doc or raw kind wins, which is plugin
/// knowledge.
#[test]
fn duplicate_symbol_takes_first_and_diagnoses() {
    let mut doc = doc_of("minimal");
    let unit = FixtureUnitId("unit:app".to_owned());
    let mut first = doc.symbols[&unit][0].clone();
    first.name = "a_later_claim_about_the_same_id".to_owned();
    doc.symbols
        .get_mut(&unit)
        .expect("the unit has symbols")
        .push(first);

    let plugin = plugin_from("minimal", doc);
    let index = build(&plugin);

    let create = index
        .view()
        .node(&NodeId {
            plugin: plugin.id(),
            raw: "fn:handlers/create_task".to_owned(),
        })
        .expect("the node exists");
    assert_eq!(
        create
            .symbol
            .as_ref()
            .map(|symbol| symbol.name.as_str())
            .unwrap_or_default(),
        "create_task",
        "the first symbol is kept"
    );

    assert!(index.diagnostics().iter().any(|diagnostic| matches!(
        diagnostic,
        BuildDiagnostic::DuplicateSymbol { id } if id.raw == "fn:handlers/create_task"
    )));
}

/// Plan-01 §4.2, open question 3. Abort is the default because a partial index
/// makes live code look unreachable.
#[test]
fn provider_error_aborts_unless_allow_partial() {
    let failing = FailingProvider {
        id: PluginId("a-double"),
    };
    let inputs = BuildInputs {
        symbols: vec![&failing],
        edges: vec![&failing],
        roots: Vec::new(),
        classifiers: Vec::new(),
    };

    let error = Index::build(Path::new("."), &inputs, &BuildOptions::default())
        .expect_err("a provider failure aborts by default");
    assert!(matches!(error, BuildError::Provider { .. }), "{error:?}");
}

/// ADR-0007. A consumer must be able to see that every unreachability claim in
/// this artifact is weaker than usual.
#[test]
fn allow_partial_sets_coverage_partial_flag() {
    let failing = FailingProvider {
        id: PluginId("a-double"),
    };
    let inputs = BuildInputs {
        symbols: vec![&failing],
        edges: vec![&failing],
        roots: Vec::new(),
        classifiers: Vec::new(),
    };

    let index = Index::build(
        Path::new("."),
        &inputs,
        &BuildOptions {
            allow_partial: true,
            ..BuildOptions::default()
        },
    )
    .expect("allow_partial continues");

    assert!(index.coverage().partial);
    assert!(index
        .diagnostics()
        .iter()
        .any(|diagnostic| matches!(diagnostic, BuildDiagnostic::ProviderFailed { .. })));

    let sink = emit(&index);
    assert!(
        unreachable(&sink).coverage.partial,
        "the flag reaches the artifact, not just the in-memory index"
    );
}

/// Plan-01 §4.4. The shard exists and reaches nothing, which is a true
/// statement and a visible one.
#[test]
fn root_bound_to_unindexed_node_is_diagnosed_not_dropped() {
    let mut doc = doc_of("minimal");
    doc.roots[0].binding = FixtureRootBinding::Bound(FixtureRaw("fn:never_emitted".to_owned()));

    let index = build(&plugin_from("minimal", doc));

    assert!(index.diagnostics().iter().any(|diagnostic| matches!(
        diagnostic,
        BuildDiagnostic::RootBoundToUnindexedNode { node, .. } if node.raw == "fn:never_emitted"
    )));
    assert_eq!(index.shards().len(), 1, "the root still gets a shard");
    assert_eq!(index.shards()[0].view.nodes.len(), 1);
}

/// Plan-00 §6.2 and plan-01 §4.4. An unbound root is never dropped: dropping
/// it would make a real endpoint invisible and make whatever it would have
/// reached read as unreachable.
#[test]
fn unbound_root_is_reported_not_dropped() {
    let index = build(&case("unbound_root"));

    assert_eq!(index.roots().len(), 2);
    assert_eq!(index.coverage().roots_total, 2);
    assert_eq!(index.coverage().roots_bound, 1);
}

/// Plan-01 §4.4. A row in the endpoint list, visibly gapped, not an absence.
#[test]
fn unbound_root_has_no_shard_but_is_in_endpoints() {
    let sink = emit(&build(&case("unbound_root")));
    let document = endpoints(&sink);

    let delete = document
        .operations
        .iter()
        .find(|operation| operation.operation == "DeleteTask")
        .expect("the unbound operation is still listed");

    assert_eq!(delete.versions.len(), 1);
    assert_eq!(delete.versions[0].shard, None);
    assert_eq!(delete.versions[0].node_count, 0);
    assert!(matches!(
        &delete.versions[0].binding,
        BindingRow::Unbound { reason } if reason.contains("DeleteTask")
    ));

    assert_eq!(shards(&sink).len(), 1, "only the bound root gets a shard");
}

/// `IndexCoverage::unbound_roots`, with the provider's own reason.
#[test]
fn unbound_root_recorded_in_coverage() {
    let sink = emit(&build(&case("unbound_root")));
    let coverage = unreachable(&sink).coverage;

    assert_eq!(coverage.unbound_roots.len(), 1);
    assert_eq!(coverage.unbound_roots[0].operation, "DeleteTask");
    assert_eq!(coverage.unbound_roots[0].version.as_deref(), Some("v1"));
    assert!(coverage.unbound_roots[0].reason.contains("DeleteTask"));
}

/// Plan-01 §7. Classification is an ADR-0002 plugin kind, and a plugin may
/// legitimately not provide it. The absence is reported, never an error.
#[test]
fn no_classifier_yields_none_not_error() {
    let index = build(&case("no_classifier"));

    for node in &index.view().nodes {
        assert_eq!(node.category, None, "{} was classified", node.id.raw);
    }
    assert!(index
        .diagnostics()
        .iter()
        .any(|diagnostic| matches!(diagnostic, BuildDiagnostic::NoClassifierForPlugin { .. })));
}

/// ADR-0003 field 5, plan-01 §4.2 step 1. The plugin is preflighted against the
/// repository the caller named, not against the process's working directory.
///
/// Nothing in the corpus can observe this: `FixturePlugin::preflight` accepts
/// its argument and ignores it, which is exactly the behaviour plan-02 §3.1
/// requires of it. A plugin whose prerequisites are a property of a directory —
/// every real one — would silently check the wrong tree.
#[test]
fn preflight_receives_the_repository_root() {
    let plugin = case("minimal");
    let spy = PreflightSpy::new(&plugin);

    let index = Index::build(
        plugin.case_dir(),
        &BuildInputs {
            symbols: vec![&spy],
            edges: vec![&spy],
            roots: Vec::new(),
            classifiers: Vec::new(),
        },
        &BuildOptions::default(),
    )
    .expect("a warned plugin runs");

    assert_eq!(spy.roots(), [plugin.case_dir().to_path_buf()]);
    assert_eq!(index.view().nodes.len(), 2, "a warned plugin runs");
}

/// ADR-0003 field 5, as amended 2026-09-19. A `Warned` plugin runs, and the
/// remediation it reported is carried rather than dropped — the honest-absence
/// rule again: a value that says less than the plugin knows.
#[test]
fn preflight_warning_is_recorded_and_the_plugin_still_runs() {
    let plugin = case("minimal");
    let spy = PreflightSpy::new(&plugin);

    let index = Index::build(
        plugin.case_dir(),
        &BuildInputs {
            symbols: vec![&spy],
            edges: vec![&spy],
            roots: Vec::new(),
            classifiers: Vec::new(),
        },
        &BuildOptions::default(),
    )
    .expect("a warned plugin runs");

    assert!(index.diagnostics().iter().any(|diagnostic| matches!(
        diagnostic,
        BuildDiagnostic::PreflightWarned {
            reason,
            remediation,
            ..
        } if reason == spy.reason && remediation == spy.remediation
    )));
}

// ---------------------------------------------------------------------------
// The run-record channel — plan-03 §9 D-D's second half
// ---------------------------------------------------------------------------

/// A plugin-authored note reaches the artifact unaltered.
///
/// D-D rules that generated code goes unindexed **and the artifact says so**,
/// so a reader can tell *not indexed* from *not called*. The facts are one
/// language's vocabulary, so the waist carries them as opaque strings rather
/// than as fields — widening `IndexCoverage` with `out_dir_loaded` would be the
/// ADR-0003 violation the fixture plugin exists to catch.
#[test]
fn a_plugin_note_reaches_the_artifact_verbatim() {
    let sentence = "generated code was not indexed for 1 of 2 members; calls into \
                    generated code from those members are absent from this index, not \
                    proven absent from the code";

    let mut doc = doc_of("minimal");
    doc.notes = vec![sentence.to_owned()];
    let plugin = plugin_from("minimal", doc);

    let index = Index::build(Path::new("."), &inputs(&plugin), &BuildOptions::default())
        .expect("the case builds");

    assert_eq!(index.coverage().notes, vec![sentence.to_owned()]);
    assert_eq!(
        endpoints(&emit(&index)).coverage.notes,
        vec![sentence.to_owned()],
        "the note is carried into the artifact, which is where a reader is"
    );
}

/// One plugin commonly fills three of the four slices. Asking each slice would
/// write its sentence into the artifact three times, and a reader would take
/// the repetition for three findings.
#[test]
fn a_plugin_in_several_slices_is_asked_once() {
    let mut doc = doc_of("minimal");
    doc.notes = vec!["said once".to_owned()];
    let plugin = plugin_from("minimal", doc);

    let inputs = inputs(&plugin);
    assert!(
        !inputs.symbols.is_empty() && !inputs.edges.is_empty() && !inputs.classifiers.is_empty(),
        "the case under test occupies several slices, which is what makes the dedup load-bearing"
    );

    let index =
        Index::build(Path::new("."), &inputs, &BuildOptions::default()).expect("the case builds");

    assert_eq!(index.coverage().notes, vec!["said once".to_owned()]);
}

/// Two plugins each keep their own sentence. A `Vec<String>` the waist neither
/// parses nor merges is the point: nothing here may decide two notes are "the
/// same finding".
#[test]
fn notes_from_two_plugins_are_both_carried() {
    let mut first = doc_of("two_plugins_a");
    first.notes = vec!["from a".to_owned()];
    let mut second = doc_of("two_plugins_b");
    second.notes = vec!["from b".to_owned()];

    let a = plugin_from("two_plugins_a", first);
    let b = plugin_from("two_plugins_b", second);

    let index = Index::build(
        Path::new("."),
        &crate::support::inputs_of(&[&a, &b]),
        &BuildOptions::default(),
    )
    .expect("both cases build");

    assert_eq!(
        index.coverage().notes,
        vec!["from a".to_owned(), "from b".to_owned()]
    );
}

/// A run nobody had anything to say about emits an empty list, not a missing
/// key. Absence of notes is a statement; a missing field would be a gap.
#[test]
fn a_run_with_no_notes_emits_an_empty_list() {
    let index = build(&case("minimal"));

    assert!(index.coverage().notes.is_empty());
    assert!(endpoints(&emit(&index)).coverage.notes.is_empty());
}

/// ADR-0743. A provider that returned roots and named a contract it could not
/// read has produced a **smaller** index, not a failed one, and the waist
/// carries that fact rather than deciding it does not matter.
///
/// The aggregation and the flag are asserted separately on purpose: wiring the
/// list into `IndexCoverage` and forgetting to raise `partial` is the exact
/// half-fix that leaves every consumer reading a truncated index as a complete
/// one.
#[test]
fn an_unexamined_contract_is_aggregated_and_makes_the_index_partial() {
    let provider = PartialRootProvider {
        id: PluginId("a-double"),
        unexamined: vec![UnexaminedContract {
            contract: ContractId("proto/broken.proto".to_owned()),
            reason: "expected 'stream' or a type name, but reached end of file".to_owned(),
        }],
    };
    let inputs = BuildInputs {
        symbols: Vec::new(),
        edges: Vec::new(),
        roots: vec![&provider],
        classifiers: Vec::new(),
    };

    let index = Index::build(Path::new("."), &inputs, &BuildOptions::default())
        .expect("a provider that skipped one file did not fail");

    let coverage = index.coverage();
    assert_eq!(
        coverage.unexamined_contracts,
        [UnexaminedContract {
            contract: ContractId("proto/broken.proto".to_owned()),
            reason: "expected 'stream' or a type name, but reached end of file".to_owned(),
        }],
        "the provider's own words, carried verbatim"
    );
    assert!(
        coverage.partial,
        "a contract nobody read may hold roots nobody bound, so every \
         unreachability claim here is weaker"
    );
    assert!(
        !coverage
            .contracts
            .contains(&ContractId("proto/broken.proto".to_owned())),
        "examined and unexamined are disjoint: {:?}",
        coverage.contracts
    );
}

/// The complement of the test above, and the guard that keeps `partial` from
/// being wired to something that is always true. A provider with nothing to
/// skip leaves the index complete.
#[test]
fn a_provider_with_nothing_unexamined_leaves_the_index_complete() {
    let provider = PartialRootProvider {
        id: PluginId("a-double"),
        unexamined: Vec::new(),
    };
    let inputs = BuildInputs {
        symbols: Vec::new(),
        edges: Vec::new(),
        roots: vec![&provider],
        classifiers: Vec::new(),
    };

    let index = Index::build(Path::new("."), &inputs, &BuildOptions::default())
        .expect("nothing failed and nothing was skipped");

    assert!(index.coverage().unexamined_contracts.is_empty());
    assert!(!index.coverage().partial);
}
