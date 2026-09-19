//! `OutputSink` — plan-00 §3.6.
//!
//! The trait has no behaviour of its own, so what is asserted here is the one
//! property a consumer depends on and a careless edit would remove: it is
//! **object-safe**. Plan-00 §3.6 spells the renderer's parameter
//! `sink: &mut dyn OutputSink`, and plan-01 §8.6 has `emit.rs` and a plugin
//! renderer writing through one mechanism — both need a trait object.
//!
//! A generic method, a `Self: Sized` bound or a `self`-by-value receiver would
//! each make the trait un-objectifiable, and the failure would surface in
//! plan-05 rather than here. `&mut dyn OutputSink` below is the assertion; it
//! fails to compile rather than fails at runtime.

use std::collections::BTreeMap;

use reachgraph_plugin_api::OutputSink;

/// A sink that keeps what it was handed. Not a filesystem: plan-00 §3.6 is
/// explicit that the core owns where bytes land, and a renderer never touches
/// the filesystem itself — so the in-memory case is the ordinary one, not a
/// test-only special case.
#[derive(Default)]
struct Recording {
    written: BTreeMap<String, Vec<u8>>,
}

impl OutputSink for Recording {
    fn write(&mut self, relative_path: &str, bytes: &[u8]) -> std::io::Result<()> {
        self.written
            .insert(relative_path.to_owned(), bytes.to_vec());
        Ok(())
    }
}

/// Write through a trait object, which is the shape plan-00 §3.6 hands a
/// renderer.
fn render_into(sink: &mut dyn OutputSink) -> std::io::Result<()> {
    sink.write("endpoints.json", b"[]")?;
    sink.write("graph/task-create.json", b"{}")
}

#[test]
fn a_sink_is_usable_as_a_trait_object() {
    let mut sink = Recording::default();
    render_into(&mut sink).expect("the recording sink accepts every write");

    assert_eq!(
        sink.written.keys().collect::<Vec<_>>(),
        vec!["endpoints.json", "graph/task-create.json"]
    );
    assert_eq!(sink.written["endpoints.json"], b"[]");
}

/// ADR-0006's `out/` layout is the core's. A sink is handed a relative path and
/// stores it as given — it does not resolve, reject or rewrite one, because the
/// component that owns the layout is the component that constructed the sink.
#[test]
fn a_relative_path_reaches_the_sink_unaltered() {
    let mut sink = Recording::default();
    let path = "vendor/cytoscape.min.js";
    sink.write(path, b"x").expect("the sink accepts the write");

    assert!(sink.written.contains_key(path));
}
