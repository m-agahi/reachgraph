//! The hand-built `SymbolIndex`, and the shapes plan-03 was MEASURED to emit.
//!
//! Plan-04 §12: this exists so `SymbolIndex` has two independent consumers. A
//! field only a real engine can produce would stop this file compiling, which
//! turns a leak into a compile error rather than a month-nine discovery.

use std::path::{Path, PathBuf};

use reachgraph_plugin_api::{
    DocFormat, NodeId, PluginId, SourceRange, Span, Symbol, SymbolIndex, SymbolKind,
};

/// The plugin id every fake symbol carries.
///
/// It is `reachgraph-lang-rust`'s, because that is whose symbols a roots plugin
/// binds against. Using this crate's own id would make the fake index a
/// different shape from the real one in the one field `NodeId` equality uses.
pub const LANG_RUST: PluginId = PluginId("reachgraph-lang-rust");

/// A `Symbol` builder that states only what a test cares about.
pub struct Sym {
    symbol: Symbol,
}

impl Sym {
    /// A method, the kind a handler is.
    pub fn method(name: &str, file: &str, offset: u32) -> Self {
        Self::of(SymbolKind::Method, "Method", name, file, offset)
    }

    /// A free function, which `by_name` returns beside methods.
    pub fn function(name: &str, file: &str, offset: u32) -> Self {
        Self::of(SymbolKind::Function, "Function", name, file, offset)
    }

    /// An impl block. MEASURED: its `name` is the **self type**, and its
    /// `raw_kind` is the header — so an impl is never reachable by the service
    /// name through `by_name`.
    pub fn impl_block(self_ty: &str, header: &str, file: &str, offset: u32) -> Self {
        Self::of(SymbolKind::Other, header, self_ty, file, offset)
    }

    /// A trait declaration, whose `raw_kind` is the bare term `Trait`.
    pub fn trait_decl(name: &str, file: &str, offset: u32) -> Self {
        Self::of(SymbolKind::Type, "Trait", name, file, offset)
    }

    fn of(kind: SymbolKind, raw_kind: &str, name: &str, file: &str, offset: u32) -> Self {
        Self {
            symbol: Symbol {
                id: NodeId {
                    plugin: LANG_RUST,
                    raw: format!("fx@0.1.0::unit|{offset}|{file}"),
                },
                name: name.to_owned(),
                kind,
                raw_kind: raw_kind.to_owned(),
                range: SourceRange {
                    file: PathBuf::from(file),
                    span: Some(Span {
                        start: offset,
                        end: offset + 1,
                    }),
                },
                doc: None,
                doc_format: DocFormat::Markdown,
                container: None,
                is_test: false,
            },
        }
    }

    /// Give this symbol a container, the way the walk does.
    pub fn inside(mut self, container: &Symbol) -> Self {
        self.symbol.container = Some(container.id.clone());
        self
    }

    /// Set `is_test`.
    ///
    /// MEASURED 2026-09-19 against `reachgraph-lang-rust`: the engine sets this
    /// for a `#[test]` **function** and for nothing else — not for an impl
    /// block, and not for a plain method inside a `tests/` target. A fixture
    /// that sets it on a mock's method would be describing a world the engine
    /// does not produce.
    pub fn test_fn(mut self) -> Self {
        self.symbol.is_test = true;
        self
    }

    /// The finished symbol.
    pub fn build(self) -> Symbol {
        self.symbol
    }
}

/// A `SymbolIndex` over a fixed set of symbols.
pub struct FakeIndex {
    symbols: Vec<Symbol>,
}

impl FakeIndex {
    /// An index over these symbols.
    pub fn new(symbols: Vec<Symbol>) -> Self {
        Self { symbols }
    }
}

impl SymbolIndex for FakeIndex {
    fn by_name(&self, name: &str) -> Vec<&Symbol> {
        self.symbols
            .iter()
            .filter(|symbol| symbol.name == name)
            .collect()
    }

    fn in_file(&self, path: &Path) -> Vec<&Symbol> {
        self.symbols
            .iter()
            .filter(|symbol| symbol.range.file == path)
            .collect()
    }

    fn get(&self, id: &NodeId) -> Option<&Symbol> {
        self.symbols.iter().find(|symbol| symbol.id == *id)
    }
}
