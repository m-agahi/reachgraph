//! The run report — plan-06 §5.3 and §6.
//!
//! Printed at the end and written as `run.json` **inside** the artifact.
//! Plan-06 §8 question 3 is DECIDED that way: a reviewer opening a downloaded
//! artifact zip should see the run's coverage without a second file, and an
//! output directory that describes itself survives being moved.

use std::io::Write;

use reachgraph_core::{Index, UNREACHABLE_CLAIM};
use reachgraph_plugin_api::EdgeTarget;
use serde::Serialize;

/// ADR-0006, printed after a successful run and adjacent to the output path —
/// where the user is deciding what to do with the directory, rather than in a
/// README section nobody opens.
pub const PRIVACY_NOTE: &str = "\
note: %OUT% is a structural map of this repository. It contains file paths,
      function and method names, doc comment text, service topology, and which
      endpoints reach which code. Treat it with the same care as the source.
      GitHub Pages publishes publicly on Free and Pro repositories. Do not
      publish this from a private repository.";

/// One thing this run could not do, as the plugin that could not do it said.
#[derive(Clone, Debug, Serialize)]
pub struct Limit {
    /// Who reported it.
    pub plugin: String,
    /// What was found.
    pub finding: String,
    /// What to do about it, when there is something to do.
    pub remediation: Option<String>,
}

/// What one run produced. Rendered for a reader and serialised as `run.json`.
#[derive(Clone, Debug, Serialize)]
pub struct RunReport {
    /// The repository analysed.
    pub repo: String,
    /// Where the artifact went.
    pub out: String,
    /// Wall clock for the whole run.
    pub wall_clock_ms: u64,
    /// Detection plus preflight, which is where a workspace load happens.
    pub preflight_ms: u64,
    /// Symbols, edges, roots, graph build and reachability.
    pub analysis_ms: u64,
    /// Writing the artifact.
    pub emit_ms: u64,
    /// Units enumerated.
    pub units: usize,
    /// Nodes an indexing provider emitted a symbol for.
    pub symbols: usize,
    /// Edges in the index-wide view, unresolved ones included.
    pub edges: usize,
    /// Roots held, bound and unbound together.
    pub roots_total: usize,
    /// Roots bound to a handler.
    pub roots_bound: usize,
    /// The rest. An unbound root is why a real handler may be in the list
    /// below.
    pub roots_unbound: usize,
    /// Calls the plugin could not resolve. A large number here means the
    /// unreachability claim is weaker than it looks.
    pub unresolved_edge_targets: usize,
    /// Indexed symbols no bound root reaches.
    pub unreachable: usize,
    /// ADR-0007's sentence, carried so a consumer of `run.json` uses the same
    /// wording rather than composing its own.
    pub unreachable_claim: String,
    /// Shards written.
    pub shards: usize,
    /// Contracts covered.
    pub contracts: usize,
    /// Version keys covered.
    pub versions: usize,
    /// What this run could not do.
    pub limits: Vec<Limit>,
}

impl RunReport {
    /// Read the counts off a built index.
    pub fn of(index: &Index, repo: &str, out: &str, timings: Timings, limits: Vec<Limit>) -> Self {
        let view = index.view();
        let coverage = index.coverage();

        Self {
            repo: repo.to_owned(),
            out: out.to_owned(),
            wall_clock_ms: timings.wall_clock_ms,
            preflight_ms: timings.preflight_ms,
            analysis_ms: timings.analysis_ms,
            emit_ms: timings.emit_ms,
            units: coverage.units_indexed.len(),
            symbols: view
                .nodes
                .iter()
                .filter(|node| node.symbol.is_some())
                .count(),
            edges: view.edges.len(),
            roots_total: coverage.roots_total,
            roots_bound: coverage.roots_bound,
            roots_unbound: coverage.unbound_roots.len(),
            unresolved_edge_targets: view
                .edges
                .iter()
                .filter(|edge| matches!(edge.to, EdgeTarget::Unresolved { .. }))
                .count(),
            unreachable: index.unreachable().len(),
            unreachable_claim: UNREACHABLE_CLAIM.to_owned(),
            shards: index.shards().len(),
            contracts: coverage.contracts.len(),
            versions: coverage.versions.len(),
            limits,
        }
    }

    /// The human form, on stderr.
    ///
    /// The unreachable line uses ADR-0007's wording **verbatim**, from the
    /// constant `reachgraph-core` ships. The word "dead" appears nowhere in
    /// this crate's output: design.md §8 makes telling someone to delete
    /// working code the one failure that permanently destroys trust.
    pub fn render(&self, into: &mut dyn Write) -> std::io::Result<()> {
        writeln!(
            into,
            "analysed {} in {}",
            self.repo,
            duration(self.wall_clock_ms)
        )?;
        writeln!(
            into,
            "  units {}   symbols {}   edges {}   roots {} ({} unbound)",
            self.units, self.symbols, self.edges, self.roots_total, self.roots_unbound
        )?;
        writeln!(
            into,
            "  unresolved edge targets {}",
            self.unresolved_edge_targets
        )?;
        writeln!(
            into,
            "  {}: {} symbols",
            self.unreachable_claim, self.unreachable
        )?;
        writeln!(
            into,
            "  coverage: {} contracts, {} versions",
            self.contracts, self.versions
        )?;
        writeln!(
            into,
            "  phases: preflight {}   analysis {}   write {}",
            duration(self.preflight_ms),
            duration(self.analysis_ms),
            duration(self.emit_ms)
        )?;
        writeln!(
            into,
            "wrote {} (endpoints.json, {} shards, unreachable.json, versions.json, run.json)",
            self.out, self.shards
        )?;

        self.render_limits(into)?;

        writeln!(into)?;
        writeln!(into, "{}", PRIVACY_NOTE.replace("%OUT%", &self.out))
    }

    /// **A finding that can never clear is not spelled as a warning.**
    ///
    /// ADR-0728 disables proc-macro expansion unconditionally, so the Rust
    /// plugin returns `Warned` on every run, for ever. A `WARN` row that never
    /// goes away trains a reader to ignore warnings, and suppressing it at the
    /// source would be a lie about what the index contains.
    ///
    /// So these are reported as what they are: limits of the run, stated once,
    /// beside the artifact they qualify — and the same sentences are inside the
    /// artifact through `IndexCoverage::notes`, so the fact outlives the
    /// terminal it was printed in.
    fn render_limits(&self, into: &mut dyn Write) -> std::io::Result<()> {
        if self.limits.is_empty() {
            return Ok(());
        }

        writeln!(into)?;
        writeln!(into, "limits of this run")?;
        for limit in &self.limits {
            for (position, line) in limit.finding.lines().enumerate() {
                match position {
                    0 => writeln!(into, "  {:<22} {line}", limit.plugin)?,
                    _ => writeln!(into, "  {:<22} {line}", "")?,
                }
            }
            if let Some(remediation) = &limit.remediation {
                for line in remediation.lines() {
                    writeln!(into, "  {:<22} → {line}", "")?;
                }
            }
        }

        Ok(())
    }
}

/// How long each observable phase took.
///
/// **Three phases, not plan-06 §5.2's nine.** `Index::build` runs unit
/// discovery, symbol extraction, edge extraction, root binding, graph assembly
/// and reachability behind one call and publishes no progress channel, so a cli
/// that printed nine phase lines would be inventing six of them. What is
/// measured here is what is observable from outside the waist, and the
/// workspace load lands in `preflight` because that is the call that triggers
/// it.
#[derive(Clone, Copy, Debug, Default)]
pub struct Timings {
    /// Detection and preflight, including any workspace load they trigger.
    pub preflight_ms: u64,
    /// `Index::build`.
    pub analysis_ms: u64,
    /// `Index::emit` plus `run.json`.
    pub emit_ms: u64,
    /// The three above plus argument handling.
    pub wall_clock_ms: u64,
}

fn duration(milliseconds: u64) -> String {
    let seconds = milliseconds / 1000;
    if seconds < 60 {
        return format!("{}.{:02}s", seconds, (milliseconds % 1000) / 10);
    }
    format!("{}m{:02}s", seconds / 60, seconds % 60)
}
