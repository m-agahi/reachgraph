//! `structure.json` — plan-05 §4.4.1, §9.2 and ADR-0729.

use reachgraph_plugin_api::{GraphView, PluginId, SymbolKind};
use reachgraph_render_html::dispatch::{self, Dispatch};
use reachgraph_render_html::structure::structure_of;
use serde_json::Value;

use crate::support::{self, ids};

fn emitted() -> Value {
    let view = support::index_view();
    support::render(&view, &[], &support::artifact()).json("structure.json")
}

fn row<'a>(document: &'a Value, raw: &str) -> &'a Value {
    document["nodes"]
        .as_array()
        .expect("nodes is an array")
        .iter()
        .find(|row| row["raw"] == raw)
        .unwrap_or_else(|| panic!("no row for {raw}"))
}

fn box_of<'a>(document: &'a Value, id: &str) -> &'a Value {
    document["boxes"]
        .as_array()
        .expect("boxes is an array")
        .iter()
        .find(|row| row["id"] == id)
        .unwrap_or_else(|| panic!("no box {id}"))
}

/// Plan-05 §4.4.1: the outer box is the unit, and the inner boxes come from
/// walking the container chain.
#[test]
fn container_chain_becomes_nested_boxes() {
    let document = emitted();

    // `create_task` on the trait sits inside the trait, which sits inside the
    // module, which sits inside the unit.
    let method = row(&document, &ids::trait_method().raw);
    let type_box = box_of(&document, method["box"].as_str().expect("a box"));
    assert_eq!(type_box["kind"], "type");
    assert_eq!(type_box["label"], "TaskService");
    assert_eq!(type_box["raw_kind"], "Trait");

    let module_box = box_of(&document, type_box["parent"].as_str().expect("a parent"));
    assert_eq!(module_box["kind"], "module");
    assert_eq!(module_box["label"], "task");

    let unit_box = box_of(&document, module_box["parent"].as_str().expect("a parent"));
    assert_eq!(unit_box["kind"], "unit");
    // Plan-05 §4.4.3's rule generalised: a `UnitId` is the plugin's spelling
    // and is displayed verbatim, never prettified.
    assert_eq!(unit_box["label"], "crate:task");
    assert!(unit_box["parent"].is_null());
}

/// Plan-05 §9.2: an ancestor that is not a grouping is not a box. An `impl`
/// block is `SymbolKind::Other`, so the method inside it lands in the module
/// box — and the impl block is still read, for what ADR-0729 needs.
#[test]
fn an_impl_block_is_not_a_box_but_is_still_read() {
    let document = emitted();
    let method = row(&document, &ids::impl_method().raw);

    let holding = box_of(&document, method["box"].as_str().expect("a box"));
    assert_eq!(holding["kind"], "module", "{holding}");

    let impl_boxes: Vec<&Value> = document["boxes"]
        .as_array()
        .expect("boxes is an array")
        .iter()
        .filter(|row| row["label"] == "impl TaskService for Task")
        .collect();
    assert!(impl_boxes.is_empty(), "{impl_boxes:?}");

    assert_eq!(method["container_raw_kind"], "impl TaskService for Task");

    // The impl block is a node like any other: it has a row, it sits in the
    // module box, and it claims no dispatch class of its own — ADR-0729's
    // question is about a method, not about the block around it.
    let block = row(&document, &ids::impl_block().raw);
    assert_eq!(block["box"], holding["id"]);
    assert!(block["dispatch"].is_null(), "{block}");

    // And the module itself sits directly in the unit box, with nothing
    // between them.
    let module = row(&document, &ids::module().raw);
    let unit = box_of(&document, module["box"].as_str().expect("a box"));
    assert_eq!(unit["kind"], "unit");
}

/// Plan-05 §4.4.1: an external node was never indexed, so it belongs to no
/// unit and is drawn **outside** every box — not inside a synthetic "unknown"
/// one, and never dropped.
#[test]
fn an_unindexed_node_has_no_box_and_is_not_dropped() {
    let document = emitted();
    let external = row(&document, &ids::external().raw);

    assert!(external["box"].is_null(), "{external}");
    assert!(external["container_raw_kind"].is_null(), "{external}");
    assert!(external["dispatch"].is_null(), "{external}");
}

/// Plan-05 §4.4.3 and ADR-0003 field 3: the opaque id round-trips byte for
/// byte. The fixture id carries both `|` and `/` and `:` so a split on any of
/// them would show up here.
#[test]
fn node_id_is_never_split() {
    let document = emitted();
    let raw = ids::external().raw;
    assert!(
        raw.contains('|') && raw.contains('/') && raw.contains(':'),
        "{raw}"
    );

    let external = row(&document, &raw);
    assert_eq!(external["raw"], raw);
    assert_eq!(external["plugin"], "reachgraph-lang-rust");
}

/// ADR-0729, the whole point. A method declared on a trait is marked as the
/// declaration; the same name defined in an `impl` block is marked as the
/// implementation. The two must not read alike, or a reader believes the
/// handler reaches the code that runs.
#[test]
fn a_trait_declaration_is_distinguished_from_an_implementation() {
    let document = emitted();

    assert_eq!(
        row(&document, &ids::trait_method().raw)["dispatch"],
        "trait-declaration"
    );
    assert_eq!(
        row(&document, &ids::impl_method().raw)["dispatch"],
        "implementation"
    );

    // The sentence travels with the classification rather than being composed
    // by the page.
    let note = document["trait_declaration_note"]
        .as_str()
        .expect("the note is a string");
    assert!(note.contains("trait"), "{note}");
    assert!(note.contains("not an implementation"), "{note}");
}

/// The table is keyed by plugin, and silence is the answer for every plugin it
/// has not been taught. Claiming `implementation` by default would assert the
/// dangerous direction on no evidence.
#[test]
fn the_dispatch_table_claims_nothing_for_an_unknown_plugin() {
    assert_eq!(
        dispatch::classify(support::RUST, SymbolKind::Type, "Trait"),
        Some(Dispatch::TraitDeclaration)
    );
    assert_eq!(
        dispatch::classify(support::OTHER, SymbolKind::Type, "Trait"),
        None
    );
    assert_eq!(
        dispatch::classify(support::OTHER, SymbolKind::Other, "impl X for Y"),
        None
    );
}

/// The impl-header rule is anchored, never a substring search — the same rule
/// plan-04 §7 states for the other consumer of that grammar. A type whose own
/// name merely contains `impl` is not an implementation.
#[test]
fn the_impl_header_rule_is_anchored() {
    assert_eq!(
        dispatch::classify(support::RUST, SymbolKind::Other, "impl Task"),
        Some(Dispatch::Implementation)
    );
    assert_eq!(
        dispatch::classify(support::RUST, SymbolKind::Other, "simple impl Task"),
        None
    );
    assert_eq!(
        dispatch::classify(support::RUST, SymbolKind::Other, "implicit"),
        None
    );
    // A trait is a `Type`; a kind mismatch is not a near miss to be forgiven.
    assert_eq!(
        dispatch::classify(support::RUST, SymbolKind::Other, "Trait"),
        None
    );
}

/// Plan-05 §9.2: a plugin that emits no container links yields unit-level
/// grouping only — a worse diagram, not a broken one, and rendered without a
/// special case.
#[test]
fn no_containers_degrades_to_unit_boxes() {
    let mut view = support::index_view();
    let nodes: Vec<_> = view
        .nodes
        .iter()
        .cloned()
        .map(|mut node| {
            if let Some(symbol) = node.symbol.as_mut() {
                symbol.container = None;
            }
            node
        })
        .collect();
    view = GraphView::new(
        nodes,
        view.edges.clone(),
        view.roots.clone(),
        view.plugins.clone(),
        view.coverage.clone(),
    );

    let document = structure_of(&view);
    assert!(
        document
            .boxes
            .iter()
            .all(|row| matches!(row.kind, reachgraph_render_html::structure::BoxKind::Unit)),
        "{:?}",
        document.boxes
    );
    assert_eq!(document.boxes.len(), 1);
    assert!(document.nodes.iter().all(|row| row.dispatch.is_none()));
}

/// A containment loop is the waist's to guard, and it guards it: the chain
/// terminates rather than hanging. This asserts the renderer inherits that
/// rather than re-implementing it — plan-05 §4.4.1 forbids re-guarding.
#[test]
fn a_containment_loop_terminates() {
    let first = support::id(support::RUST, "a|0|src/lib.rs");
    let second = support::id(support::RUST, "b|0|src/lib.rs");

    let view = GraphView::new(
        vec![
            support::node(
                first.clone(),
                Some(support::symbol(
                    &first,
                    "A",
                    SymbolKind::Module,
                    "Module",
                    "src/lib.rs",
                    Some(second.clone()),
                )),
                Some("crate:x"),
            ),
            support::node(
                second.clone(),
                Some(support::symbol(
                    &second,
                    "B",
                    SymbolKind::Module,
                    "Module",
                    "src/lib.rs",
                    Some(first.clone()),
                )),
                Some("crate:x"),
            ),
        ],
        Vec::new(),
        Vec::new(),
        vec![support::descriptor()],
        support::coverage(),
    );

    let document = structure_of(&view);
    assert_eq!(document.nodes.len(), 2);
    assert!(document.boxes.len() <= 5, "{:?}", document.boxes);
}

/// The sidecar names its own schema version and its author. It is a different
/// document with a different owner from the waist's files, and a shared
/// version number would make a bump in either place look like a bump in both.
#[test]
fn the_sidecar_states_its_schema_and_its_author() {
    let document = emitted();
    assert_eq!(document["schema_version"], 1);
    assert_eq!(document["generated_by"], "html");
}

/// A box id is an ordinal assigned here, never built out of an identity.
/// Deriving one from a `NodeId` would be parsing one (ADR-0003 field 3).
#[test]
fn a_box_id_carries_no_identity() {
    let document = emitted();
    for row in document["boxes"].as_array().expect("boxes is an array") {
        let id = row["id"].as_str().expect("an id");
        assert!(
            id.starts_with('b') && id[1..].chars().all(|c| c.is_ascii_digit()),
            "{id}"
        );
    }
}

/// Plugin identity survives onto every row, because a `NodeId` is two halves
/// and the page joins on both.
#[test]
fn every_row_carries_both_halves_of_the_identity() {
    let document = emitted();
    let rows = document["nodes"].as_array().expect("nodes is an array");
    assert_eq!(rows.len(), support::index_view().nodes.len());
    for row in rows {
        assert_eq!(row["plugin"], PluginId("reachgraph-lang-rust").0);
        assert!(row["raw"].as_str().is_some_and(|raw| !raw.is_empty()));
    }
}
