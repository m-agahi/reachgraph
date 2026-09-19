//! ADR-0729, made visible: a trait declaration is not the code that runs.
//!
//! # The claim this module exists to stop a reader believing
//!
//! MEASURED in `ra_ap_ide` 0.0.352 (ADR-0729): a call dispatched through a
//! generic — `fn f<T: Tr>(x: &T) { x.method() }` — resolves to the **trait's**
//! declaration, not to any implementor, even when exactly one implementor
//! exists. The same call on a concrete type resolves to the implementation.
//! The edge is therefore real and points at a body-less declaration, and
//! nothing downstream may treat it as reaching the code that executes.
//!
//! ADR-0729's Consequences say so in one sentence: *a renderer should make a
//! trait-declaration target visually distinct from a resolved implementation*.
//! That sentence is this module. A reader who sees a handler reach
//! `Tr::method` and believes the implementation is covered has been given the
//! tool's most dangerous claim in its most convincing form.
//!
//! # Why a per-plugin table, and what is wrong with it
//!
//! The distinction is not in the waist's vocabulary. [`SymbolKind`] is neutral
//! by construction — a Rust `trait` and a Rust `impl` block are both
//! [`SymbolKind::Type`] and [`SymbolKind::Other`] respectively — so the only
//! place the difference survives is `raw_kind`, which is the emitting plugin's
//! own term and which ADR-0003 forbids the *waist* from reading. A renderer is
//! a plugin and plan-05 §9.2 places exactly this reading here.
//!
//! What plan-05 did not anticipate is that reading it means knowing whose
//! vocabulary it is. So the table below is **keyed by [`PluginId`]** and holds
//! one entry. For any other plugin it answers [`None`] and the page claims
//! nothing — silence rather than a guess in another language's terms.
//!
//! **This is a recorded gap, not a design.** The right shape is a
//! plugin-supplied value on `Symbol` — something the plugin *means*, the same
//! remedy plan-05 §9.3 reaches for on the unindexed-label problem — and that
//! is a plan-00 change this plan may not make. Until then the page also
//! carries `container_raw_kind` verbatim on every node (see
//! `crate::structure`), which is language-neutral, and the badge below is the
//! extra that one plugin's vocabulary buys.

use reachgraph_plugin_api::{PluginId, SymbolKind};

/// What the enclosing definition says about whether reaching this method
/// reaches code that runs.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Dispatch {
    /// The method is declared on an interface. An edge arriving here proves a
    /// call was made **through** the interface; it does not name the body that
    /// executes, and that body may be in the unreachable list.
    TraitDeclaration,
    /// The method is defined in an implementation. An edge arriving here
    /// reaches the code that runs.
    Implementation,
}

impl Dispatch {
    /// The token the page carries, and the class the stylesheet keys on.
    pub fn as_str(self) -> &'static str {
        match self {
            Dispatch::TraitDeclaration => "trait-declaration",
            Dispatch::Implementation => "implementation",
        }
    }
}

/// The one plugin whose vocabulary this module knows.
const RUST: &str = "reachgraph-lang-rust";

/// `reachgraph-lang-rust`'s term for a trait declaration, from that crate's
/// `kinds.rs` mapping table: both `RustItem::Trait` and `RustItem::TraitAlias`
/// report `"Trait"`.
const RUST_TRAIT: &str = "Trait";

/// The anchored prefix of `reachgraph-lang-rust`'s impl-block header grammar,
/// from that crate's `render_impl_header`:
///
/// ```text
/// header := "impl " <trait> " for " <self-ty>   when the impl implements a trait
///         | "impl " <self-ty>                   otherwise
/// ```
///
/// Anchored, never a substring search — the same rule plan-04 §7 states for
/// the other consumer of this grammar. A type literally named `impl_thing`
/// must not match.
const RUST_IMPL_PREFIX: &str = "impl ";

/// Classify a method by what encloses it.
///
/// `container_kind` and `container_raw_kind` are the **innermost container's**,
/// followed by equality through `GraphView::container_of`. [`None`] is the
/// honest answer for every plugin this table has not been taught and for every
/// container shape it does not recognise: claiming `Implementation` by default
/// would assert the dangerous direction — that the code which runs is covered
/// — on no evidence at all.
pub fn classify(
    plugin: PluginId,
    container_kind: SymbolKind,
    container_raw_kind: &str,
) -> Option<Dispatch> {
    if plugin.0 != RUST {
        return None;
    }

    match container_kind {
        SymbolKind::Type if container_raw_kind == RUST_TRAIT => Some(Dispatch::TraitDeclaration),
        SymbolKind::Other if container_raw_kind.starts_with(RUST_IMPL_PREFIX) => {
            Some(Dispatch::Implementation)
        }
        _ => None,
    }
}

/// The sentence the page shows beside a trait-declaration badge.
///
/// A literal here rather than in the template, so the wording travels with the
/// table that produces the badge and cannot drift from it.
pub const TRAIT_DECLARATION_NOTE: &str =
    "This target is the trait's declaration, not an implementation. A call dispatched through a \
     generic resolves here, so reaching this node does not show that the code which runs is \
     reached — that body may be in the not-reachable list.";
