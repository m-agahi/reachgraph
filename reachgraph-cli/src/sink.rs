//! A sink that lands bytes and remembers them.
//!
//! # Why the renderer receives bytes rather than documents
//!
//! The artifact schema is `reachgraph-core`'s serde mirror (ADR-0727), and
//! plan-05 §8.6 keeps `reachgraph-core` out of a renderer's dependency graph.
//! A renderer therefore cannot construct — or re-serialise — a single artifact
//! document, and plan-05 §6.5's single-file page has to inline the same JSON
//! the sharded directory holds.
//!
//! "The same" is the requirement rather than a nicety. A second serialisation
//! could differ in key order, in how a number is formatted, or in a field one
//! side forgot, and the two pages would then disagree about the repository
//! with nothing saying which was right. Recording what the waist wrote and
//! handing those bytes over makes them identical by construction.
//!
//! The recording is in memory and is the whole artifact's JSON. That is
//! bounded by the same number ADR-0006's inline threshold is written against —
//! roughly 5 MB for a graph the single-file page would hold — and a large
//! index costs one extra copy of files that were just built in memory anyway.

use std::io;

use reachgraph_core::DirectorySink;
use reachgraph_plugin_api::{ArtifactFile, OutputSink};

/// Writes through to a real sink and keeps a copy of everything it landed.
pub struct RecordingSink {
    inner: DirectorySink,
    recorded: Vec<ArtifactFile>,
}

impl RecordingSink {
    /// Wrap a sink.
    pub fn new(inner: DirectorySink) -> Self {
        Self {
            inner,
            recorded: Vec::new(),
        }
    }

    /// What has been written so far, in write order.
    ///
    /// A snapshot rather than a borrow of the live list, because the renderer
    /// writes through the same sink while it reads this — and a renderer
    /// inlining its own output into its own page is a shape nobody asked for.
    pub fn recorded(&self) -> Vec<ArtifactFile> {
        self.recorded.clone()
    }
}

impl OutputSink for RecordingSink {
    fn write(&mut self, relative_path: &str, bytes: &[u8]) -> io::Result<()> {
        self.inner.write(relative_path, bytes)?;
        self.recorded.push(ArtifactFile {
            path: relative_path.to_owned(),
            bytes: bytes.to_vec(),
        });
        Ok(())
    }
}
