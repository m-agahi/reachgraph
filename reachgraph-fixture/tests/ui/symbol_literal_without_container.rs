// A `Symbol` literal that omits `container` must not compile.
//
// Plan-00 §8 question 3: binding a contract operation to a handler needs the
// enclosing definition, and `docs/design.md` §4 MEASURED that name alone is not
// enough — `create_task` exists twice in one repository, once as the handler
// and once on a `MockDb`. A plugin author must not be able to forget it, so it
// is a required field rather than a documented convention.
use std::path::PathBuf;

use reachgraph_plugin_api::{DocFormat, NodeId, PluginId, SourceRange, Symbol, SymbolKind};

fn main() {
    let _ = Symbol {
        id: NodeId {
            plugin: PluginId("fixture"),
            raw: "fn:handlers/create_task".to_owned(),
        },
        name: "create_task".to_owned(),
        kind: SymbolKind::Method,
        raw_kind: "fn".to_owned(),
        range: SourceRange {
            file: PathBuf::from("src/service/handlers.rs"),
            span: None,
        },
        doc: None,
        doc_format: DocFormat::Plain,
        is_test: false,
    };
}
