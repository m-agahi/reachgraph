//! Served or consumed — plan-04 §6, and the guards that stop a test double
//! from manufacturing a phantom root.
//!
//! # Two signals are MEASURED dead, and are recorded because both are the
//! obvious first guess
//!
//! 1. **build.rs flags do not discriminate.** MEASURED (plan-04 §1 M9):
//!    `.build_server(true).build_client(true)` is one call applied to every
//!    proto a build script compiles.
//! 2. **Generated-module presence does not discriminate.** MEASURED (M10):
//!    both `*_server` and `*_client` modules are generated for both contracts.
//!
//! Direction is a fact about what **this repository's own source does**.
//!
//! # The `!is_test` half of plan-04 §6 does not work as written
//!
//! Plan-04 §6 calls `!is_test` "the sharp one" and rests the whole M6
//! regression on it. MEASURED 2026-09-19 against `reachgraph-lang-rust`, by
//! dumping the symbols it emits for its own `fx-impl` fixture:
//!
//! ```text
//! SYM name="Mock"   raw_kind="impl Svc for Mock" is_test=false file=i/tests/mock.rs
//! SYM name="create" raw_kind="Method"            is_test=false file=i/tests/mock.rs
//! SYM name="the_mock_creates" raw_kind="Function" is_test=true file=i/tests/mock.rs
//! ```
//!
//! The engine populates `is_test` from `hir::Function::is_test`, so it is set
//! for a `#[test]` **function** and for nothing else — not for an impl block,
//! and not for an ordinary method inside a `tests/` target. A guard reading
//! `is_test` alone would let `impl TaskDbService for MockDb` supply a served
//! signal, which is precisely the five phantom roots plan-04 §6 exists to
//! prevent.
//!
//! [`is_test_context`] is the replacement: the engine's flag **or** a Cargo
//! test-target path. The path half is the same Cargo-shaped knowledge plan-04
//! §6 already sanctions for the `target/` guard, and this crate is the
//! tonic-and-Cargo plugin, so it is stated here rather than assumed anywhere
//! else.
//!
//! What it still does not catch: a mock inside `#[cfg(test)] mod tests` in
//! `src/`. `Symbol` carries no cfg information and no unit, so v0.1 cannot see
//! that shape, and it is recorded as a limitation rather than papered over.

use std::path::Path;

use reachgraph_plugin_api::{Direction, Symbol};

use crate::names::impl_header_names_trait;

/// What the repository's own source says about one service.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct ServiceEvidence {
    /// A first-party, non-test `impl <Service> for T` exists.
    pub first_party_impl: bool,
    /// A first-party, non-test source file names `<Service>Client`.
    pub client_reference: bool,
}

/// The direction call, **with the evidence that produced it**.
///
/// The two consumed variants are not the same answer: one is corroborated by a
/// client reference, the other is the default taken in the absence of any
/// evidence at all, and plan-04 §9 gives them different `reason` strings. A
/// bare `Direction` here would throw that away at the point it is decided.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DirectionCall {
    /// A first-party non-test impl of the service trait exists.
    Served,
    /// No such impl, and a first-party reference to `<Service>Client`.
    ConsumedWithClient,
    /// Neither signal fired.
    NoEvidence,
}

impl DirectionCall {
    /// The direction the waist receives.
    ///
    /// # The asymmetry is deliberate
    ///
    /// A wrongly-`Served` service produces **phantom roots**, which corrupt
    /// reachability and can make dead code read as live. A wrongly-`Consumed`
    /// service produces **reported gaps**, which are visible in the artifact
    /// and correct themselves under inspection. With no evidence, take the
    /// failure mode that shows up in the output.
    pub fn direction(self) -> Direction {
        match self {
            DirectionCall::Served => Direction::Served,
            DirectionCall::ConsumedWithClient | DirectionCall::NoEvidence => Direction::Consumed,
        }
    }
}

/// Call the direction of one service from the evidence gathered for it.
pub fn direction_of(evidence: ServiceEvidence) -> DirectionCall {
    if evidence.first_party_impl {
        DirectionCall::Served
    } else if evidence.client_reference {
        DirectionCall::ConsumedWithClient
    } else {
        DirectionCall::NoEvidence
    }
}

/// Is this symbol test-shaped, by the engine's flag or by its path?
///
/// See the module documentation: the flag alone is MEASURED insufficient, and
/// the two halves together are what the M6 regression needs.
pub fn is_test_context(symbol: &Symbol) -> bool {
    symbol.is_test || is_test_target_path(&symbol.range.file)
}

/// Does this symbol prove the repository serves `service`?
///
/// Three clauses, each earning its place: the impl header names the service as
/// a **trait** (the generated server module contains `impl<T> Service<…> for
/// <Service>Server<T>`, which is a different shape and does not match), the
/// symbol is not test-shaped, and it is not build output.
pub fn is_served_signal(symbol: &Symbol, service: &str) -> bool {
    impl_header_names_trait(&symbol.raw_kind, service)
        && !is_test_context(symbol)
        && !is_generated_path(&symbol.range.file)
}

/// Does this source text name `<service>Client` as an identifier?
///
/// # What this is and is not
///
/// It is a text scan, and being precise about that matters. It resolves
/// nothing, decides nothing about which symbol anything refers to, has no
/// candidate set and cannot bind a root. Its only output is which `reason` an
/// already-`Unbound` consumed root carries. That is the distinction ADR-0005
/// draws between asking a syntactic question and asking a semantic one.
///
/// It exists because `SymbolIndex` cannot answer the question: a typed field
/// `db: WidgetDbClient<Channel>` is a `Field` symbol named `db`, and the type
/// text is not in `Symbol` at all (plan-04 §13 question 3).
pub fn mentions_client(source: &str, service: &str) -> bool {
    let needle = format!("{service}Client");
    let bytes = source.as_bytes();
    let mut from = 0usize;
    while let Some(offset) = source[from..].find(&needle) {
        let start = from + offset;
        let end = start + needle.len();
        let before_ok = start == 0 || !is_ident_byte(bytes[start - 1]);
        let after_ok = end >= bytes.len() || !is_ident_byte(bytes[end]);
        if before_ok && after_ok {
            return true;
        }
        from = start + 1;
    }
    false
}

/// Is this path a Cargo test, bench or example target?
///
/// A **path component**, never a substring: `src/testing.rs` and
/// `src/tests_support.rs` are ordinary first-party source.
pub fn is_test_target_path(path: &Path) -> bool {
    has_component(path, &["tests", "benches", "examples"])
}

/// Is this path under a `target/` directory — build output rather than source?
pub fn is_generated_path(path: &Path) -> bool {
    has_component(path, &["target"])
}

/// Is this path a build script's output directory, `target/**/out/`?
///
/// Plan-04 §11's consumed-side binding wants the generated client leaf and
/// nothing else under `target/`.
pub fn is_generated_out_path(path: &Path) -> bool {
    is_generated_path(path) && has_component(path, &["out"])
}

fn has_component(path: &Path, names: &[&str]) -> bool {
    path.components().any(|component| {
        let component = component.as_os_str().to_string_lossy();
        names.iter().any(|name| component == *name)
    })
}

fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}
