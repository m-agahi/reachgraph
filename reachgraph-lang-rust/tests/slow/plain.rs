//! `fx-plain` — two crates, `a` calls `b`.

use reachgraph_plugin_api::{Category, Classifier, EdgeTarget, InferenceMode, SymbolKind};

use crate::support::{all_edges, all_symbols, fixture, load, named};

#[test]
fn two_crates_are_two_units() {
    let (_plugin, units) = load("fx-plain");
    let names: Vec<&str> = units.iter().map(|u| u.display_name.as_str()).collect();
    assert_eq!(
        names,
        vec!["a", "b"],
        "one unit per workspace member target"
    );
}

/// One `Edge` per call site, not one per callee.
///
/// `caller` calls `b::leaf()` **twice from one body**. `CallItem::ranges` is a
/// `Vec<FileRange>` and plan-03 §9 emits one edge per range: three calls to
/// one function are three real call sites, and collapsing them here would
/// discard information the waist cannot recover.
#[test]
fn a_call_site_is_an_edge_and_two_call_sites_are_two_edges() {
    let (plugin, units) = load("fx-plain");
    let symbols = all_symbols(&plugin, &units);
    let caller = named(&symbols, "caller");
    let leaf = named(&symbols, "leaf");

    let edges = all_edges(&plugin, &units);
    let from_caller: Vec<_> = edges.iter().filter(|edge| edge.from == caller.id).collect();

    assert_eq!(
        from_caller.len(),
        2,
        "two call sites in one body: {:#?}",
        from_caller
    );

    for edge in &from_caller {
        assert!(
            matches!(&edge.to, EdgeTarget::Resolved(id) if id == &leaf.id),
            "the emitted Edge names the leaf's own node id, saw {:?}",
            edge.to
        );
        assert_eq!(edge.inference_mode, InferenceMode::Resolved);
        assert_eq!(edge.provenance.plugin, reachgraph_lang_rust::PLUGIN_ID);
        assert!(
            edge.provenance.engine.starts_with(&format!(
                "ra_ap_ide {}",
                reachgraph_lang_rust::RA_AP_VERSION
            )),
            "engine was {:?}",
            edge.provenance.engine
        );
        let call_site = edge.call_site.as_ref().expect("Rust knows its call sites");
        assert!(
            call_site.file.to_string_lossy().ends_with("a/src/lib.rs"),
            "call site was {:?}",
            call_site.file
        );
        assert!(call_site.span.is_some());
    }

    // The two edges are distinct call sites rather than one edge twice.
    assert_ne!(
        from_caller[0].call_site, from_caller[1].call_site,
        "two edges, two positions"
    );
}

/// A `NodeId` does not depend on which call loaded the workspace.
///
/// MEASURED as a defect and then by mutation: with paths anchored to the
/// directory the caller named, a plugin whose first call was `symbols_in` (load
/// anchored at `<workspace>/b`) emitted different raws from one whose first
/// call was `discover_units` (load anchored at `<workspace>`). Plan-03 §6's
/// whole design rests on a `raw` being a pure function of location.
///
/// The two plugins below are separate instances on purpose. Reusing one would
/// reuse its load and assert nothing.
#[test]
fn a_node_id_does_not_depend_on_which_call_loaded_the_workspace() {
    use reachgraph_plugin_api::{LanguagePlugin, SymbolProvider};

    let (via_discover, units) = load("fx-plain");
    let unit_b = units
        .iter()
        .find(|unit| unit.display_name == "b")
        .expect("unit b");

    // A fresh plugin, entered through the member unit rather than the
    // workspace: `symbols_in` loads from `<workspace>/b`.
    let via_member = reachgraph_lang_rust::RustPlugin::new();
    let member_symbols = via_member.symbols_in(unit_b).expect("symbols_in loads");
    let discover_symbols = via_discover.symbols_in(unit_b).expect("symbols_in");

    let member_raws: Vec<&str> = member_symbols
        .iter()
        .map(|symbol| symbol.id.raw.as_str())
        .collect();
    let discover_raws: Vec<&str> = discover_symbols
        .iter()
        .map(|symbol| symbol.id.raw.as_str())
        .collect();

    assert_eq!(member_raws, discover_raws);
    // And the emitted path is workspace-relative, not member-relative.
    let leaf = named(&member_symbols, "leaf");
    assert!(
        leaf.id.raw.ends_with("|b/src/lib.rs"),
        "anchored at the workspace root, saw {:?}",
        leaf.id.raw
    );
    assert_eq!(
        leaf.range.file,
        std::path::PathBuf::from("b/src/lib.rs"),
        "the SourceRange uses the same anchor"
    );
    let _ = via_discover.discover_units(&fixture("fx-plain"));
}

/// Plan-03 §10 rules 3 and 4, against a real workspace.
#[test]
fn a_units_own_source_is_first_party_and_a_siblings_is_not() {
    let (plugin, units) = load("fx-plain");
    let unit_a = units
        .iter()
        .find(|u| u.display_name == "a")
        .expect("unit a");

    let own = fixture("fx-plain").join("a/src/lib.rs");
    let sibling = fixture("fx-plain").join("b/src/lib.rs");

    assert_eq!(plugin.classify(&own, unit_a), Category::FirstParty);
    assert_eq!(
        plugin.classify(&sibling, unit_a),
        Category::WorkspaceSibling
    );
}

/// The symbol a call resolves to is the symbol the walk emitted, byte for byte.
#[test]
fn a_call_target_is_the_same_node_the_walk_emitted() {
    let (plugin, units) = load("fx-plain");
    let symbols = all_symbols(&plugin, &units);
    let leaf = named(&symbols, "leaf");
    assert_eq!(leaf.kind, SymbolKind::Function);

    let edges = all_edges(&plugin, &units);
    let target = edges
        .iter()
        .find_map(|edge| match &edge.to {
            EdgeTarget::Resolved(id) if id == &leaf.id => Some(id.clone()),
            _ => None,
        })
        .expect("the leaf is an edge target");

    assert_eq!(target.raw, leaf.id.raw);
}
