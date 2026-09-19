//! Plan-02 §7.3 — conventions converted into build failures.
//!
//! Each case in `tests/ui/` must FAIL to compile, and its checked-in `.stderr`
//! says why. They are cheap and precise: each asserts one field's existence or
//! absence, which a prose rule cannot.
//!
//! # What these are worth, honestly
//!
//! Three of the four are E0063 and E0560 — a missing field and a field that
//! does not exist. Those diagnostics are stable and the assertion is exactly
//! the claim: `Symbol` cannot be built without `container`, `Root` cannot be
//! built without `binding`, and `confidence` is **gone rather than deprecated**
//! (plan-00 §8 question 5), so a literal that sets it does not compile.
//!
//! `plugin_uses_core_internals` is the weak one and is kept anyway. It fails
//! because `reachgraph-core` is not a dependency of this crate, which is the
//! same fact `no_plugin_depends_on_core` asserts over `cargo metadata` — so it
//! is a second spelling of one guard rather than a second guard. What it adds
//! is the failure a contributor actually meets: the line they typed, refused
//! where they typed it.
//!
//! # The version coupling, stated so a CI-only failure is legible
//!
//! trybuild compares compiler output byte for byte, and compiler output is a
//! property of the compiler version. `rust-toolchain.toml` pins 1.98.0 and
//! rustup honours an in-tree pin over an installed default, so CI should agree
//! with a local run. If these ever fail in CI alone with a diff that is pure
//! diagnostic wording, that pin is what to check first — the assertion has not
//! found a defect, it has found two rustc versions.

#[test]
fn conventions_are_build_failures() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/*.rs");
}
