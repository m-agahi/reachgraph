//! ADR-1222 — every run resolves the sysroot, and a call through `?` keeps its
//! edge.
//!
//! # Whether rust-src is installed is decided by the harness, not the plugin
//!
//! The defect this file guards was a plugin that reported the standard
//! library's source as absent on every machine, installed or not. A test that
//! branched on the plugin's own answer would take the "absent" branch and pass
//! against exactly that defect. So the harness asks the toolchain itself —
//! `rustc --print sysroot` in the fixture, as the plugin does — and looks for
//! the source on disk. The plugin must then agree with it.
//!
//! # Neither branch skips
//!
//! With the source installed, the test asserts the edges. Without it, it
//! asserts that the run records the absence as a limit that names the
//! toolchain. Both are assertions about the run; neither returns early
//! without one.

use std::path::{Path, PathBuf};
use std::process::Command;

use reachgraph_lang_rust::coverage::StdlibSource;
use reachgraph_plugin_api::{EdgeTarget, Plugin, Preflight};

use crate::support::{all_edges, all_symbols, fixture, load, named};

/// The standard library's source for the toolchain `rustc` resolves in
/// `root`, when it is installed — the same lookup ra_ap's
/// `RustLibSource::Path` makes: `RUST_SRC_PATH` when it holds `core`, else
/// `<sysroot>/lib/rustlib/src/rust/library`.
fn installed_rust_src(root: &Path) -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("RUST_SRC_PATH").map(PathBuf::from) {
        if path.join("core").exists() {
            return Some(path);
        }
    }
    let output = Command::new("rustc")
        .args(["--print", "sysroot"])
        .current_dir(root)
        .env("RUSTUP_AUTO_INSTALL", "0")
        .output()
        .expect("the harness may ask rustc for its sysroot");
    assert!(
        output.status.success(),
        "rustc --print sysroot failed in {}: {}",
        root.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    let sysroot = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
    let library = sysroot.join("lib/rustlib/src/rust/library");
    library.join("core").exists().then_some(library)
}

/// The byte range of the first line of `text` containing `needle`.
fn line_of(text: &str, needle: &str) -> std::ops::Range<u32> {
    let mut start = 0usize;
    for line in text.split_inclusive('\n') {
        if line.contains(needle) {
            return start as u32..(start + line.len()) as u32;
        }
        start += line.len();
    }
    panic!("no line contains {needle:?}");
}

/// The discriminator. `Svc2::say_hello` calls `local_result` twice: once
/// through `?` and once plainly. Without `core` loaded the `?` call had no
/// edge and the plain call did — MEASURED 2026-11-12 on this crate's original.
/// With the source installed both have one. Without it, the run says so.
#[test]
fn a_call_through_the_question_mark_has_an_edge_like_the_plain_call() {
    let root = fixture("fx-try");
    let (plugin, units) = load("fx-try");
    let coverage = plugin.coverage().expect("a load happened");

    let Some(_library) = installed_rust_src(&root) else {
        // Honest absence: the plugin agrees, and the run records it — in
        // preflight with a remediation, and in the artifact's notes.
        assert!(
            matches!(coverage.stdlib, StdlibSource::SourceMissing { .. }),
            "the harness found no rust-src; the plugin says {:?}",
            coverage.stdlib
        );
        let Preflight::Warned {
            reason,
            remediation,
        } = plugin.preflight(&root)
        else {
            panic!("a missing rust-src is a recorded limit, not silence");
        };
        assert!(
            reason.contains("rust-src") && reason.contains("`?`"),
            "the limit names the component and what it cost: {reason:?}"
        );
        assert!(
            remediation.contains("rustup component add rust-src --toolchain"),
            "the remediation names the toolchain: {remediation:?}"
        );
        assert!(
            plugin.notes().iter().any(|note| note.contains("rust-src")),
            "the artifact states the absence too"
        );
        return;
    };

    let symbols = all_symbols(&plugin, &units);
    let local_result = named(&symbols, "local_result");
    let text = std::fs::read_to_string(root.join("t/src/lib.rs")).expect("the fixture is readable");
    let through_try = line_of(&text, "let q = local_result(&r.name)?;");
    let plain = line_of(&text, "let plain = local_result(&r.name);");

    let edges = all_edges(&plugin, &units);
    let call_lines: Vec<std::ops::Range<u32>> = edges
        .iter()
        .filter(|edge| matches!(&edge.to, EdgeTarget::Resolved(id) if id == &local_result.id))
        .filter_map(|edge| edge.call_site.as_ref()?.span)
        .map(|span| span.start..span.end)
        .collect();
    let has_edge_in = |line: &std::ops::Range<u32>| {
        call_lines
            .iter()
            .any(|span| line.start <= span.start && span.end <= line.end)
    };

    assert!(
        has_edge_in(&plain),
        "the plain call has an edge: {call_lines:?}"
    );
    assert!(
        has_edge_in(&through_try),
        "the call through `?` has an edge too: {call_lines:?}"
    );
    assert!(
        coverage.stdlib == StdlibSource::Resolved,
        "rust-src is installed for this toolchain, so the sysroot must resolve"
    );
}

/// `set_test: true` is a co-requirement of the sysroot (ADR-1222). MEASURED
/// on the probe: with the sysroot loaded and cfg(test) off, a `#[test]`
/// function resolved as cfg-gated and was not indexed at all.
#[test]
fn a_test_function_in_a_tests_target_stays_indexed() {
    let (plugin, units) = load("fx-try");
    let symbols = all_symbols(&plugin, &units);
    let the_test = named(&symbols, "orphan_is_seven");
    assert!(the_test.is_test, "#[test] sets is_test");
}
