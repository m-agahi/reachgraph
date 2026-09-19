//! The kind mapping and the impl-header grammar — ADR-0008 leak 6, plan-03 §8.
//!
//! Both are pure functions over plain data so that the table can be read, and
//! tested, without an engine anywhere near it.

use reachgraph_plugin_api::SymbolKind;

/// A Rust item, as far as the mapping table cares.
///
/// This is deliberately **not** `ra_ap_hir::ModuleDef` or
/// `ra_ap_ide::SymbolKind`. Naming the engine's type here would put an
/// `ra_ap` import in the half of the crate plan-03 §13 Tier A exists to keep
/// engine-free, and the mapping is a decision rather than a translation.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum RustItem {
    /// A free function.
    Function,
    /// A function whose parent is an `impl` block or a trait.
    Method,
    /// `struct`.
    Struct,
    /// `enum`.
    Enum,
    /// `union`.
    Union,
    /// `type X = …`.
    TypeAlias,
    /// `trait`.
    Trait,
    /// `trait X = …`.
    TraitAlias,
    /// An `impl` block, carrying its rendered header.
    Impl {
        /// The output of [`render_impl_header`].
        header: String,
    },
    /// `mod`.
    Module,
    /// A struct or enum-variant field.
    Field,
    /// Any macro definition.
    Macro,
    /// `static`.
    Static,
    /// `const`.
    Const,
    /// Anything the walk meets and the table above does not name.
    ///
    /// The string is the engine's own term and is carried through to
    /// `raw_kind` verbatim — a kind this table has not been taught is still
    /// reported honestly rather than flattened to `"Other"`.
    Other(String),
}

/// The neutral kind and the display term, for one item.
///
/// `raw_kind` is display text the waist is forbidden to interpret
/// (plan-00 §3.4). One consumer is permitted to read it: a roots plugin,
/// walking up through `container`, which is why the `Impl` row's content is a
/// grammar rather than a label.
pub fn map_kind(item: &RustItem) -> (SymbolKind, String) {
    match item {
        RustItem::Function => (SymbolKind::Function, "Function".to_owned()),
        RustItem::Method => (SymbolKind::Method, "Method".to_owned()),
        RustItem::Struct => (SymbolKind::Type, "Struct".to_owned()),
        RustItem::Enum => (SymbolKind::Type, "Enum".to_owned()),
        RustItem::Union => (SymbolKind::Type, "Union".to_owned()),
        RustItem::TypeAlias => (SymbolKind::Type, "TypeAlias".to_owned()),
        // Both trait forms report `"Trait"`. Plan-03 §8's table says so, and
        // plan-04 parses the impl header against a trait name rather than
        // against this term, so the collapse costs that consumer nothing.
        RustItem::Trait => (SymbolKind::Type, "Trait".to_owned()),
        RustItem::TraitAlias => (SymbolKind::Type, "Trait".to_owned()),
        RustItem::Impl { header } => (SymbolKind::Other, header.clone()),
        RustItem::Module => (SymbolKind::Module, "Module".to_owned()),
        RustItem::Field => (SymbolKind::Field, "Field".to_owned()),
        RustItem::Macro => (SymbolKind::Other, "Macro".to_owned()),
        RustItem::Static => (SymbolKind::Other, "Static".to_owned()),
        RustItem::Const => (SymbolKind::Other, "Const".to_owned()),
        RustItem::Other(term) => (SymbolKind::Other, term.clone()),
    }
}

/// Render an `impl` block's header — the string contract between two plugins.
///
/// ```text
/// header := "impl " <trait> " for " <self-ty>   when the impl implements a trait
///         | "impl " <self-ty>                   otherwise
/// ```
///
/// `trait` is the trait's **declared** name, never the path it was imported by
/// (plan-03 §8). Plan-04 §7 parses this with an anchored rule and never a
/// substring search, which is why the grammar is stated in exactly one place
/// and why both halves are written verbatim rather than normalised.
pub fn render_impl_header(trait_name: Option<&str>, self_ty: &str) -> String {
    match trait_name {
        Some(trait_name) => format!("impl {trait_name} for {self_ty}"),
        None => format!("impl {self_ty}"),
    }
}

/// The declared trait name in an impl header, read from the text the source
/// wrote — `"api::v1::Svc<T>"` gives `Svc`.
///
/// # Why this exists
///
/// MEASURED 2026-09-19 on `/home/max/git/yadgarhq/task`: `hir::Impl::trait_`
/// answers `None` for `impl TaskService for Task`, because `TaskService` is
/// declared in build-script output that plan-03 §9 D-D leaves out of the crate
/// graph. The header then read `impl Task`, plan-04 §7's anchored
/// `impl <Trait> for <Self>` rule saw no trait, all six served roots went
/// unbound, no shard was written and the whole repository read as not
/// reachable from any endpoint. The fixture `fx-attr` holds that shape, with
/// two controls showing the attribute macro above the block is not the cause.
///
/// # Why reading it from the source is not inference
///
/// ADR-0728 keeps calls *through* an unexpanded macro absent rather than
/// guessed, because they were genuinely not measured. A trait clause is not a
/// call: it is written in the file, and plan-03 §8 already specifies this
/// field as the trait's **declared** name rather than a resolved path — so the
/// syntax and the resolver are two routes to one string, and the resolver is
/// merely the one that stops working when the trait is not in the index.
///
/// `None` for anything that is not a plain path: a tuple, a reference, a
/// `dyn` type. A header cannot name those as a trait, and returning a
/// mangled fragment would be worse than saying nothing.
pub fn declared_trait_name(written: &str) -> Option<String> {
    let without_generics = written.split('<').next()?.trim();
    let last = without_generics.rsplit("::").next()?.trim();

    let mut characters = last.chars();
    let first = characters.next()?;
    if !(first.is_alphabetic() || first == '_') {
        return None;
    }
    if !characters.all(|character| character.is_alphanumeric() || character == '_') {
        return None;
    }

    Some(last.to_owned())
}
