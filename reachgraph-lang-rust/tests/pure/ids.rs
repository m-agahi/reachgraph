//! `NodeId::raw` — plan-03 §6, ADR-0008 leak 1.

use reachgraph_lang_rust::ids::{node_id, RawError, RawParts};
use reachgraph_plugin_api::UnitId;
use reachgraph_plugin_api::{NodeId, PluginId};

fn parts(unit: &str, offset: u32, path: &str) -> RawParts {
    RawParts {
        unit: UnitId(unit.to_owned()),
        offset,
        path: path.to_owned(),
    }
}

/// Plan-03 §13's named cases, each for a reason.
///
/// The `|` case is why the path is last in the grammar; the non-ASCII case is
/// ADR-0008 leak 3 (an encoding mix-up that ASCII inputs hide); `0` and
/// `u32::MAX` are the ends of the offset range.
#[test]
fn node_id_encode_decode_roundtrip() {
    let cases = [
        parts("path+file:///r#a@0.1.0", 0, "src/lib.rs"),
        parts("path+file:///r#a@0.1.0", u32::MAX, "src/lib.rs"),
        parts("registry+https://x#b@1.0.0", 42, "src/weird|name.rs"),
        parts("path+file:///r#a@0.1.0", 7, "src/naïve/日本語.rs"),
        parts("path+file:///r#a@0.1.0", 7, "src/a file with spaces.rs"),
        parts(
            "path+file:///r#a@0.1.0",
            9,
            "/nix/store/whatever/library/core/src/option.rs",
        ),
    ];

    for case in cases {
        let raw = case.encode().expect("every case above is encodable");
        let back = RawParts::decode(&raw).expect("what this crate encoded, it decodes");
        assert_eq!(back, case, "roundtrip of {raw:?}");
    }
}

/// A `|` inside a path survives, because `splitn(3, '|')` stops splitting.
///
/// Named separately from the roundtrip because the roundtrip would also pass if
/// both halves were wrong in the same way.
#[test]
fn a_separator_inside_a_path_is_not_a_field_boundary() {
    let raw = parts("a", 5, "src/x|y|z.rs").encode().expect("encodable");
    assert_eq!(raw, "a|5|src/x|y|z.rs");

    let back = RawParts::decode(&raw).expect("decodable");
    assert_eq!(back.path, "src/x|y|z.rs");
    assert_eq!(back.offset, 5);
    assert_eq!(back.unit.0, "a");
}

/// A unit id containing the separator would decode to a different unit, so it
/// is refused rather than encoded into something that reads back wrong.
#[test]
fn a_separator_inside_a_unit_id_is_refused() {
    let error = parts("a|b", 5, "src/x.rs")
        .encode()
        .expect_err("a unit id with a separator is not encodable");

    assert_eq!(
        error,
        RawError::UnitContainsSeparator {
            unit: "a|b".to_owned()
        }
    );
}

/// Every malformed raw is an error, never a silently empty result.
///
/// Plan-03 §6's validation rule: a `NodeId` whose path is no longer resolvable
/// is a `PluginError`. That cannot hold if a malformed raw decodes to
/// something plausible instead.
#[test]
fn a_malformed_raw_is_an_error_not_a_guess() {
    assert!(matches!(
        RawParts::decode("only-one-field"),
        Err(RawError::MissingFields { found: 1, .. })
    ));
    assert!(matches!(
        RawParts::decode("unit|12"),
        Err(RawError::MissingFields { found: 2, .. })
    ));
    assert!(matches!(
        RawParts::decode("unit|not-a-number|src/x.rs"),
        Err(RawError::OffsetNotAnOffset { .. })
    ));
    assert!(matches!(
        RawParts::decode("unit|-1|src/x.rs"),
        Err(RawError::OffsetNotAnOffset { .. })
    ));
    assert!(matches!(
        RawParts::decode("unit|12|"),
        Err(RawError::EmptyPath { .. })
    ));
}

/// Plan-03 §6 and §9: the same location, reached two different ways, produces
/// a byte-identical `raw`.
///
/// The two ways are what the test is about. `symbols_in` mints a `NodeId` for a
/// definition it walked to; `edges_in` mints one for a `CallItem` target it has
/// never seen before. A callee that is also an emitted symbol must be the same
/// node, and it is the same node only because both paths go through one
/// constructor over the same three facts.
#[test]
fn node_id_is_a_pure_function_of_location() {
    let location = parts("path+file:///r#a@0.1.0", 128, "src/service/handlers.rs");

    let from_symbol_walk = node_id(PluginId("reachgraph-lang-rust"), &location).expect("encodable");
    let from_call_target = node_id(PluginId("reachgraph-lang-rust"), &location).expect("encodable");

    assert_eq!(from_symbol_walk, from_call_target);
    assert_eq!(
        from_symbol_walk.raw,
        "path+file:///r#a@0.1.0|128|src/service/handlers.rs"
    );

    // And the identity is a decode away from a position, with no table.
    let decoded = RawParts::decode(&from_call_target.raw).expect("decodable");
    assert_eq!(decoded, location);
}

/// The minted id carries this plugin's identity, so the waist can reject a
/// foreign node without reading the raw (ADR-0003 field 3).
#[test]
fn node_id_carries_the_minting_plugin() {
    let minted: NodeId = node_id(
        reachgraph_lang_rust::PLUGIN_ID,
        &parts("a", 1, "src/lib.rs"),
    )
    .expect("encodable");

    assert_eq!(minted.plugin, reachgraph_lang_rust::PLUGIN_ID);
}
