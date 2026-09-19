//! The kind table and the impl-header grammar — plan-03 §8, ADR-0008 leak 6.

use reachgraph_lang_rust::kinds::{map_kind, render_impl_header, RustItem};
use reachgraph_plugin_api::SymbolKind;

/// Every row of plan-03 §8's table, including that `raw_kind` is preserved
/// verbatim for `Trait`, `Impl`, `Macro` and `Static`.
///
/// The table is written out here rather than derived, because a test that
/// re-derived the mapping from the same source as the implementation would
/// assert that the code equals itself.
#[test]
fn symbol_kind_mapping_table() {
    let header = "impl TaskService for Task";
    let rows: Vec<(RustItem, SymbolKind, &str)> = vec![
        (RustItem::Function, SymbolKind::Function, "Function"),
        (RustItem::Method, SymbolKind::Method, "Method"),
        (RustItem::Struct, SymbolKind::Type, "Struct"),
        (RustItem::Enum, SymbolKind::Type, "Enum"),
        (RustItem::Union, SymbolKind::Type, "Union"),
        (RustItem::TypeAlias, SymbolKind::Type, "TypeAlias"),
        (RustItem::Trait, SymbolKind::Type, "Trait"),
        (RustItem::TraitAlias, SymbolKind::Type, "Trait"),
        (
            RustItem::Impl {
                header: header.to_owned(),
            },
            SymbolKind::Other,
            header,
        ),
        (RustItem::Module, SymbolKind::Module, "Module"),
        (RustItem::Field, SymbolKind::Field, "Field"),
        (RustItem::Macro, SymbolKind::Other, "Macro"),
        (RustItem::Static, SymbolKind::Other, "Static"),
        (RustItem::Const, SymbolKind::Other, "Const"),
        (
            RustItem::Other("BuiltinType".to_owned()),
            SymbolKind::Other,
            "BuiltinType",
        ),
    ];

    for (item, expected_kind, expected_raw) in rows {
        let (kind, raw_kind) = map_kind(&item);
        assert_eq!(kind, expected_kind, "kind for {item:?}");
        assert_eq!(raw_kind, expected_raw, "raw_kind for {item:?}");
    }
}

/// An unmapped kind keeps the engine's own term rather than flattening to a
/// label. `SymbolKind::Other` is the neutral answer; `raw_kind` still tells the
/// truth about what was found.
#[test]
fn an_unmapped_kind_keeps_its_term() {
    let (kind, raw_kind) = map_kind(&RustItem::Other("Variant".to_owned()));
    assert_eq!(kind, SymbolKind::Other);
    assert_eq!(raw_kind, "Variant");
    assert_ne!(raw_kind, "Other", "the term is reported, not swallowed");
}

/// Plan-03 §8's grammar, exactly — plan-04 §7 parses these strings with an
/// anchored rule, so an extra space or a reordered half is a contract break.
#[test]
fn impl_header_rendering() {
    assert_eq!(
        render_impl_header(Some("TaskService"), "Task"),
        "impl TaskService for Task"
    );
    assert_eq!(render_impl_header(None, "MockDb"), "impl MockDb");
    assert_eq!(
        render_impl_header(None, "Wrapper<T>"),
        "impl Wrapper<T>",
        "a generic self type is rendered as the engine spells it"
    );
    assert_eq!(
        render_impl_header(Some("From<u32>"), "Wrapper<T>"),
        "impl From<u32> for Wrapper<T>",
        "a trait with generic arguments keeps them"
    );
}

/// The trait half is the **declared** name, never the use-path it was imported
/// by (plan-03 §8). The renderer cannot enforce that on its own — it renders
/// what it is handed — so the test states which of the two the caller must
/// hand it, and the walk that supplies it is asserted in Tier B.
#[test]
fn the_impl_header_renders_the_name_it_is_given_verbatim() {
    let declared = render_impl_header(Some("TaskService"), "Task");
    let use_path = render_impl_header(
        Some("crate::pb::yadgar::taskapi::v1::task_service_server::TaskService"),
        "Task",
    );

    assert_ne!(
        declared, use_path,
        "the two differ, so which one the walk supplies is a real choice"
    );
    assert_eq!(declared, "impl TaskService for Task");
}
