//! `fx-macro` — plan-03 §4 D-B, §9 D-D and §11 check 2, against both a built
//! and an unbuilt fixture.
//!
//! Plan-03 §13 calls this "the expensive, load-bearing test". What it
//! load-bears has changed since §13 was written, and the change is the point:
//! §13 expected it to assert that the edge into generated code **exists**.
//! MEASURED 2026-09-19 that no mechanism ADR-0001 permits puts generated code
//! into the crate graph, and §9 D-D rules that v0.1 therefore ships with it
//! unindexed and **says so**. So the assertion is the D-D behaviour: the edge
//! is absent, the absence is reported, and the control edge beside it is
//! present so an absent edge cannot be confused with a broken fixture.

use reachgraph_plugin_api::{EdgeTarget, Plugin, Preflight};

use crate::support::{all_edges, all_symbols, build_fixture, load, named, unbuilt_copy};

/// Built, and the generated leaf is still not indexed.
///
/// This is the sharper statement plan-03 §9 says must not be softened: the gap
/// is not "the user has not built the repository", it is "reachgraph never told
/// the crate graph to look". A user who builds and re-runs gets the same
/// result, which is why §11 check 2's remediation must not say `cargo build`.
#[test]
fn a_built_fixture_still_has_no_edge_into_generated_code() {
    build_fixture("fx-macro");

    let (plugin, units) = load("fx-macro");
    let symbols = all_symbols(&plugin, &units);
    let edges = all_edges(&plugin, &units);

    let caller = named(&symbols, "caller");
    let local_leaf = named(&symbols, "local_leaf");

    let from_caller: Vec<_> = edges.iter().filter(|edge| edge.from == caller.id).collect();

    assert!(
        from_caller.iter().any(|edge| matches!(
            &edge.to,
            EdgeTarget::Resolved(id) if id.raw == local_leaf.id.raw
        )),
        "the control edge into local code is present; without it this test proves nothing"
    );

    assert!(
        !symbols.iter().any(|symbol| symbol.name == "generated_leaf"),
        "the generated item is not indexed"
    );
    assert_eq!(
        from_caller.len(),
        1,
        "one edge, to the local leaf; the generated call produced none: {from_caller:#?}"
    );
}

/// The absence is **reported**, and that is the whole of D-D's ruling.
///
/// An index that merely showed fewer edges would collapse *not indexed* into
/// *not called*. Those are different facts about the world and only one of them
/// is about the code.
#[test]
fn the_absence_is_reported_rather_than_merely_present() {
    build_fixture("fx-macro");
    let (plugin, _units) = load("fx-macro");

    let coverage = plugin.coverage().expect("a load happened");
    let member = coverage
        .members
        .iter()
        .find(|member| member.package.starts_with("m@"))
        .expect("the fixture's one member");

    assert!(member.has_build_script, "the fixture declares build.rs");
    assert!(
        member.out_dir_on_disk,
        "the harness built it, so the out dir is on disk"
    );
    assert!(
        !member.out_dir_loaded,
        "and it is still not in the crate graph — the two facts are kept apart"
    );

    let statement = coverage
        .generated_code_statement()
        .expect("a member with unindexed generated code produces the statement");
    assert_eq!(
        statement,
        "generated code was not indexed for 1 of 1 members; calls into generated code \
         from those members are absent from this index, not proven absent from the code"
    );
}

/// Plan-03 §11 check 2, and it **warns** where §11 specifies `Failed`.
///
/// §11's justification for refusing the run is that the user "can fix it in one
/// command". MEASURED: the command does not fix it. §9 D-D then rules that
/// v0.1 ships the index and records the gap, which a refused run cannot do.
/// A `Failed` here would be a plugin that would have run reporting as one that
/// cannot — exactly what §14 question 11 forbids.
#[test]
fn an_unbuilt_fixture_warns_and_still_runs() {
    let root = unbuilt_copy("fx-macro");

    let plugin = reachgraph_lang_rust::RustPlugin::new();
    let outcome = plugin.preflight(&root);

    let Preflight::Warned {
        reason,
        remediation,
    } = &outcome
    else {
        panic!("an unbuilt workspace warns rather than refusing: {outcome:?}");
    };
    assert!(
        reason.contains("generated code is not in the index"),
        "{reason:?}"
    );
    assert!(
        !remediation.contains("cargo build"),
        "the remediation does not name a command that does not work: {remediation:?}"
    );

    let coverage = plugin.coverage().expect("preflight loaded the workspace");
    let member = &coverage.members[0];
    assert!(!member.out_dir_on_disk, "nothing was built");
    assert!(!member.out_dir_loaded);

    // And the plugin still produces an index.
    use reachgraph_plugin_api::LanguagePlugin;
    let units = plugin.discover_units(&root).expect("a warned plugin runs");
    assert!(!units.is_empty());
}
