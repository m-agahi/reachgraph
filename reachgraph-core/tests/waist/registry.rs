//! Detection — plan-00 §5, ADR-0008.

use reachgraph_plugin_api::{Plugin, PluginId, Registration, Registry};

use crate::support::{case, fixtures_dir};

/// Plan-00 §2. A plugin that declares no markers declares no claim to any
/// repository. The inverse rule would let one misconfigured plugin hijack
/// detection for every repository.
#[test]
fn empty_marker_files_matches_nothing() {
    let plugin = case("minimal");
    assert!(plugin.detection().marker_files.is_empty());

    let mut registry = Registry::new();
    registry
        .register(
            Registration::of(case("minimal"))
                .symbols()
                .edges()
                .roots()
                .classifier(),
        )
        .expect("the case declares every capability it hands over");

    assert!(registry.detect(&fixtures_dir().join("minimal")).is_empty());
}

/// Plan-00 §5. A detectable fixture would, on any repository containing a
/// fixture document, replace real analysis with hand-written JSON and emit a
/// complete, plausible, entirely fictional call graph.
#[test]
fn fixture_is_never_detected() {
    let mut registry = Registry::new();
    registry
        .register(
            Registration::of(case("minimal"))
                .symbols()
                .edges()
                .roots()
                .classifier(),
        )
        .expect("the case declares every capability it hands over");

    // The case directory holds the document the fixture reads, which is the
    // one place a marker-file rule would most plausibly fire.
    assert!(registry.detect(&fixtures_dir().join("minimal")).is_empty());
    assert!(registry.detect(&fixtures_dir()).is_empty());

    assert!(
        registry.select(PluginId("fixture")).is_some(),
        "explicit selection is the only way in"
    );
    assert!(registry.select(PluginId("not-registered")).is_none());
}
