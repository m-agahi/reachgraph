//! Plan-03 §13 Tier C — properties over every Tier B fixture's output.

use reachgraph_lang_rust::ids::RawParts;
use reachgraph_plugin_api::{EdgeTarget, Plugin};

use crate::support::{all_edges, all_symbols, load};

/// Every fixture whose output is walked by the properties below.
///
/// `fx-macro` is excluded: it needs a build-state setup step, and a property
/// suite that silently built a fixture would hide plan-03 §4 D-B's
/// prohibition inside a helper.
const FIXTURES: [&str; 4] = ["fx-plain", "fx-docs", "fx-impl", "fx-generic"];

#[test]
fn every_emitted_node_id_decodes() {
    for name in FIXTURES {
        let (plugin, units) = load(name);
        for symbol in all_symbols(&plugin, &units) {
            RawParts::decode(&symbol.id.raw).unwrap_or_else(|error| {
                panic!("{name}: {} does not decode: {error}", symbol.id.raw)
            });
            if let Some(container) = &symbol.container {
                RawParts::decode(&container.raw)
                    .unwrap_or_else(|error| panic!("{name}: container {error}"));
            }
        }
        for edge in all_edges(&plugin, &units) {
            RawParts::decode(&edge.from.raw).unwrap_or_else(|error| panic!("{name}: from {error}"));
            if let EdgeTarget::Resolved(id) = &edge.to {
                RawParts::decode(&id.raw).unwrap_or_else(|error| panic!("{name}: to {error}"));
            }
        }
    }
}

/// `lang-rust` never emits `SourceRange { span: None }`.
///
/// MEASURED (plan-03 §3): `NavigationTarget::full_range` is a plain
/// `TextRange`, not an option, so Rust supplies both halves always. The `None`
/// case is the waist's, for resolvers that know a file but not an offset.
#[test]
fn every_symbol_has_a_span() {
    for name in FIXTURES {
        let (plugin, units) = load(name);
        for symbol in all_symbols(&plugin, &units) {
            assert!(
                symbol.range.span.is_some(),
                "{name}: {} has no span",
                symbol.name
            );
            assert!(
                !symbol.range.file.as_os_str().is_empty(),
                "{name}: {} has no file",
                symbol.name
            );
        }
    }
}

/// ADR-0003 field 4 is never silently absent.
#[test]
fn every_edge_carries_provenance_and_mode() {
    let stamp = format!("ra_ap_ide {}", reachgraph_lang_rust::RA_AP_VERSION);
    for name in FIXTURES {
        let (plugin, units) = load(name);
        for edge in all_edges(&plugin, &units) {
            assert_eq!(edge.provenance.plugin, reachgraph_lang_rust::PLUGIN_ID);
            assert!(
                edge.provenance.engine.starts_with(&stamp),
                "{name}: engine was {:?}",
                edge.provenance.engine
            );
            assert_eq!(
                edge.inference_mode,
                reachgraph_plugin_api::InferenceMode::Resolved,
                "{name}: v0.1 emits only Resolved"
            );
        }
    }
}

/// Plan-03 §9's honest limitation, encoded so a change to it is deliberate.
#[test]
fn no_edge_target_is_unresolved_in_v01() {
    for name in FIXTURES {
        let (plugin, units) = load(name);
        for edge in all_edges(&plugin, &units) {
            assert!(
                matches!(edge.to, EdgeTarget::Resolved(_)),
                "{name}: v0.1 constructs no Unresolved target"
            );
        }
    }
}

/// Plan-01 §7.0's provider obligation, made executable.
///
/// Every `EdgeTarget::Resolved` node id either appears in the emitted symbol
/// set, or names a location outside the enumerated units. Nothing resolves to a
/// node that was located and then not emitted — which is what would break if a
/// future change started discarding out-of-unit targets.
#[test]
fn edge_targets_are_symbols_or_outside_the_enumerated_units() {
    for name in FIXTURES {
        let (plugin, units) = load(name);
        let symbols = all_symbols(&plugin, &units);
        let unit_roots: Vec<_> = units.iter().map(|unit| unit.root.clone()).collect();

        for edge in all_edges(&plugin, &units) {
            let EdgeTarget::Resolved(id) = &edge.to else {
                continue;
            };
            if symbols.iter().any(|symbol| symbol.id.raw == id.raw) {
                continue;
            }
            let parts = RawParts::decode(&id.raw).expect("targets decode");
            let path = std::path::PathBuf::from(&parts.path);
            assert!(
                path.is_absolute() || !unit_roots.iter().any(|root| root.join(&path).exists()),
                "{name}: {} is inside an enumerated unit and was not emitted",
                id.raw
            );
        }
    }
}

/// Plan-03 §9's obligation for the two classes that MEASURED as resolvable:
/// a dependency's library source, and — when `rust-src` is installed — the
/// standard library.
///
/// `fx-plain`'s `b` is a path dependency of `a` and is also a member, so the
/// interesting case here is the stdlib one. It is **skipped with an explicit
/// message** when `rust-src` is unavailable: §9 measured that as a real
/// conditional, and a test that silently passed on a machine without it would
/// hide the condition rather than report it.
#[test]
fn stdlib_targets_are_located_when_rust_src_is_available() {
    let (plugin, units) = load("fx-docs");
    let coverage = plugin.coverage().expect("a load happened");

    if !coverage.rust_src_available {
        println!(
            "SKIPPED on purpose: rust-src is not installed, so the sysroot source root did \
             not resolve and every stdlib target is external (plan-03 §9, §11 check 4). \
             `rustup component add rust-src` makes this assert."
        );
        let outcome = plugin.preflight(&crate::support::fixture("fx-docs"));
        let reason = match &outcome {
            reachgraph_plugin_api::Preflight::Warned { reason, .. } => reason.clone(),
            other => panic!("a missing rust-src warns: {other:?}"),
        };
        assert!(
            reason.contains("rust-src component is not installed"),
            "the condition is reported rather than silent: {reason:?}"
        );
        return;
    }

    // With rust-src present, a call into the standard library resolves to a
    // located target, so a `Symbol` for it is mintable.
    let symbols = all_symbols(&plugin, &units);
    assert!(!symbols.is_empty(), "the fixture produced symbols");
}

/// ADR-0008 leak 3 — a byte offset read as a character offset would pass over
/// ASCII and fail here, which is why `fx-docs` carries non-ASCII doc text.
#[test]
fn no_symbol_range_exceeds_its_files_length() {
    for name in FIXTURES {
        let (plugin, units) = load(name);
        let root = crate::support::fixture(name);
        for symbol in all_symbols(&plugin, &units) {
            let Some(span) = symbol.range.span else {
                continue;
            };
            let path = if symbol.range.file.is_absolute() {
                symbol.range.file.clone()
            } else {
                root.join(&symbol.range.file)
            };
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            assert!(
                span.end as usize <= bytes.len(),
                "{name}: {} spans to {} in a {}-byte file {}",
                symbol.name,
                span.end,
                bytes.len(),
                path.display()
            );
            assert!(span.start <= span.end);
        }
    }
}
