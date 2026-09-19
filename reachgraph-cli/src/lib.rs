//! The reachgraph binary, as a library — plan-06.
//!
//! It owns the plugin registry, the feature flags and the argument surface, and
//! nothing else. Every analysis decision belongs to a plugin; every graph
//! decision belongs to the waist. There is no language branch here: ADR-0008
//! forbids `if is_rust_project(root)` in this crate as much as in the core, and
//! `tests/cli/guards.rs` asserts it over these sources.
//!
//! # Why a library with a three-line `main`
//!
//! Every test in `tests/cli` drives [`run_with`] in process, against the
//! fixture plugin and a temporary directory. A harness that spawned the built
//! binary would assert the same behaviour more slowly, would couple every
//! assertion to a build profile, and would sit awkwardly beside §4.1's guard
//! against subprocesses.
//!
//! [`run_with`] also takes the registry, which is how the fixture is reached
//! without the binary being able to link it: `reachgraph-fixture` is a
//! **dev-dependency**, so no feature combination puts a hand-written JSON
//! document into a shipped analysis. Plan-06 §3 sketches a `fixture` Cargo
//! feature; a dev-dependency is the same rule enforced one level harder,
//! because a feature that is off by default can be switched on.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod args;
mod outdir;
mod preflight;
pub mod registry;
pub mod renderers;
pub mod repo;
pub mod report;
mod sink;

#[cfg(feature = "serve")]
pub mod serve;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

use reachgraph_core::{BuildInputs, BuildOptions, DirectorySink, Index};
use reachgraph_plugin_api::{Registry, RenderInput};

use crate::args::{Analyse, Command, UsageError};
use crate::report::{RunReport, Timings};
use crate::sink::RecordingSink;

/// Analysis completed and the artifact was written.
pub const EXIT_OK: u8 = 0;
/// Something inside failed.
pub const EXIT_INTERNAL: u8 = 1;
/// A plugin's own prerequisites are not met (plan-06 §4).
pub const EXIT_PREFLIGHT: u8 = 2;
/// No registered plugin claims this repository (plan-06 §3.1).
pub const EXIT_UNDETECTED: u8 = 3;
/// Bad usage.
pub const EXIT_USAGE: u8 = 4;

/// Where output goes. Machine-readable output is stdout's; everything a human
/// reads is stderr's, so a pipeline consuming stdout never receives a spinner.
pub struct Streams<'a> {
    /// `--json` and the `plugins` table.
    pub out: &'a mut dyn Write,
    /// Progress, the report, warnings and errors.
    pub err: &'a mut dyn Write,
}

/// Run against the registry this build ships.
pub fn run(args: &[String], streams: &mut Streams<'_>) -> u8 {
    let registry = match registry::analysis_registry() {
        Ok(registry) => registry,
        Err(error) => {
            let _ = writeln!(
                streams.err,
                "error: the plugin registry is mis-wired: {error}"
            );
            return EXIT_INTERNAL;
        }
    };

    run_with(&registry, args, streams)
}

/// Run against a caller-supplied registry.
pub fn run_with(registry: &Registry, args: &[String], streams: &mut Streams<'_>) -> u8 {
    let command = match args::parse(args) {
        Ok(command) => command,
        Err(UsageError(message)) => {
            let _ = writeln!(streams.err, "error: {message}");
            let _ = writeln!(streams.err, "{}", args::USAGE);
            return EXIT_USAGE;
        }
    };

    match command {
        Command::Help => emit(streams.out, args::USAGE),
        Command::Version => emit(
            streams.out,
            &format!("reachgraph {}", env!("CARGO_PKG_VERSION")),
        ),
        Command::Plugins => plugins(registry, streams),
        Command::Preflight { repo, json } => match resolved(&repo, streams) {
            Ok(repo) => preflight::subcommand(registry, &repo, json, streams),
            Err(code) => code,
        },
        Command::Analyse(options) => match resolved(&options.repo, streams) {
            Ok(repo) => analyse(registry, &Analyse { repo, ..options }, streams),
            Err(code) => code,
        },
        Command::Serve { out, port } => serve_command(&out, port, streams),
    }
}

/// The repository argument, resolved once — see [`repo`] for the panic this
/// gate exists to stop.
///
/// BOTH entry points that take a repository come through here. `preflight` is
/// not an afterthought in that list: it hands the same path to the same
/// plugins, so a version of this that only covered `analyse` would panic in
/// exactly the same place one subcommand over.
fn resolved(named: &Path, streams: &mut Streams<'_>) -> Result<PathBuf, u8> {
    match repo::resolve(named) {
        Ok(resolved) => Ok(resolved),
        Err(error) => {
            let _ = writeln!(streams.err, "error: {}", error.reason);
            let _ = writeln!(streams.err, "  → {}", error.remediation);
            Err(EXIT_USAGE)
        }
    }
}

fn emit(into: &mut dyn Write, text: &str) -> u8 {
    match writeln!(into, "{text}") {
        Ok(()) => EXIT_OK,
        Err(_) => EXIT_INTERNAL,
    }
}

/// Plan-06 §1: two tables, because there are two registries.
///
/// The second one can exist now that `Renderer` does (plan-05), and the split
/// is the point rather than a layout choice: an analysis plugin is **detected**
/// from a repository and a renderer is **asked for**. A plugin table that
/// listed both would suggest a renderer could be detected, which
/// `detect_never_returns_a_renderer` exists to refuse.
fn plugins(registry: &Registry, streams: &mut Streams<'_>) -> u8 {
    let _ = writeln!(streams.out, "analysis plugins");
    for entry in registry.plugins() {
        let plugin = entry.plugin();
        let capabilities: Vec<String> = plugin
            .provides()
            .iter()
            .map(|capability| format!("{capability:?}").to_lowercase())
            .collect();
        let detection = plugin.detection();
        let _ = writeln!(
            streams.out,
            "  {:<28} {:<14} {}",
            plugin.id().0,
            encoding(plugin.position_encoding()),
            capabilities.join(", ")
        );
        let _ = writeln!(
            streams.out,
            "  {:<28} markers: {}   extensions: {}",
            "",
            list(detection.marker_files),
            list(detection.extensions)
        );
    }

    let _ = writeln!(streams.out);
    let _ = writeln!(
        streams.out,
        "output formats  (asked for with --renderer, never detected)"
    );
    let formats = renderers::default_registry();
    let mut any = false;
    for entry in formats.all() {
        any = true;
        let renderer = entry.renderer();
        let _ = writeln!(
            streams.out,
            "  {:<28} writes: {}",
            renderer.id().0,
            list(renderer.owns())
        );
    }
    if !any {
        // A build with every renderer feature off. Said plainly rather than
        // printed as an empty heading, which reads as a bug in the table.
        let _ = writeln!(
            streams.out,
            "  none — this build has no renderer feature enabled"
        );
    }

    EXIT_OK
}

fn encoding(value: reachgraph_plugin_api::PositionEncoding) -> &'static str {
    match value {
        reachgraph_plugin_api::PositionEncoding::Utf8Bytes => "utf8_bytes",
        reachgraph_plugin_api::PositionEncoding::Utf16CodeUnits => "utf16_code_units",
        reachgraph_plugin_api::PositionEncoding::Utf32CodePoints => "utf32_code_points",
    }
}

fn list(values: &[&str]) -> String {
    if values.is_empty() {
        return "none".to_owned();
    }
    values.join(" ")
}

fn analyse(registry: &Registry, options: &Analyse, streams: &mut Streams<'_>) -> u8 {
    let started = Instant::now();
    let repo = options.repo.display().to_string();
    let out = options.out.display().to_string();

    let detected = registry.detect(&options.repo);
    if detected.is_empty() {
        return undetected(registry, &repo, streams);
    }
    progress(
        options,
        streams,
        &format!("detect: {} plugins", detected.len()),
    );

    // An absent optional renderer is NOT an error. The waist writes the
    // artifact either way (ADR-0727), and a page is an addition to it rather
    // than a precondition for it — a build with the renderer feature off still
    // analyses a repository and still emits every JSON document.
    let formats = renderers::renderer_registry(options);
    let renderer = match formats.select(options.renderer.as_deref()) {
        renderers::Selection::Chosen(entry) => Some(entry.renderer()),
        renderers::Selection::NoneAvailable => None,
        renderers::Selection::Unknown { available } => {
            let asked = options.renderer.as_deref().unwrap_or_default();
            let _ = writeln!(streams.err, "error: no output format named {asked}");
            if available.is_empty() {
                // Never `available: none` — `none` is not a name anybody
                // could pass, and offering it as one sends the reader to try
                // it.
                let _ = writeln!(
                    streams.err,
                    "  this build has no output format compiled in; it writes the artifact's \
                     JSON and no page"
                );
            } else {
                let _ = writeln!(streams.err, "  available: {}", list(&available));
            }
            return EXIT_USAGE;
        }
    };
    let owned = outdir::owned(renderer);

    if let Err(message) = outdir::check(&options.out, &owned, options.force) {
        let _ = writeln!(streams.err, "error: {message}");
        return EXIT_USAGE;
    }

    let preflight_started = Instant::now();
    let checks = preflight::check_all(&detected, &options.repo);
    if !options.quiet {
        preflight::render(&checks, streams.err);
    }
    if checks.iter().any(preflight::is_failure) {
        let _ = writeln!(
            streams.err,
            "error: {} preflight check(s) failed; nothing was analysed",
            checks.iter().filter(|c| preflight::is_failure(c)).count()
        );
        return EXIT_PREFLIGHT;
    }
    let preflight_ms = preflight_started.elapsed().as_millis() as u64;

    let mut inputs = BuildInputs::default();
    for entry in &detected {
        if let Some(view) = entry.symbols() {
            inputs.symbols.push(view);
        }
        if let Some(view) = entry.edges() {
            inputs.edges.push(view);
        }
        if let Some(view) = entry.roots() {
            inputs.roots.push(view);
        }
        if let Some(view) = entry.classifier() {
            inputs.classifiers.push(view);
        }
    }

    progress(options, streams, "analysis: indexing");
    let analysis_started = Instant::now();
    let index = match Index::build(&options.repo, &inputs, &BuildOptions::default()) {
        Ok(index) => index,
        Err(error) => return build_failed(error, streams),
    };
    let analysis_ms = analysis_started.elapsed().as_millis() as u64;

    let emit_started = Instant::now();
    if let Err(error) = outdir::clear(&options.out, &owned) {
        let _ = writeln!(streams.err, "error: {}: {error}", out);
        return EXIT_INTERNAL;
    }

    // The sink records what it lands, and the recording is what reaches the
    // renderer. Plan-05 §6.5's single-file page inlines the same JSON the
    // sharded directory holds, and "the same" is the requirement: a second
    // serialisation could drift in key order, in number formatting, or in a
    // field one side forgot. There is one serialiser, and it is the waist's.
    let mut sink = RecordingSink::new(DirectorySink::new(&options.out));
    if let Err(error) = index.emit(&mut sink) {
        let _ = writeln!(streams.err, "error: {}: {error}", out);
        return EXIT_INTERNAL;
    }

    let mut limits = preflight::limits(&checks);
    limits.extend(preflight::notes_as_limits(&index));

    let report = RunReport::of(
        &index,
        &repo,
        &out,
        Timings {
            preflight_ms,
            analysis_ms,
            emit_ms: emit_started.elapsed().as_millis() as u64,
            wall_clock_ms: started.elapsed().as_millis() as u64,
        },
        limits,
    );

    if let Err(error) = write_run_record(&mut sink, &report) {
        let _ = writeln!(streams.err, "error: run.json: {error}");
        return EXIT_INTERNAL;
    }

    // After `emit`, because the renderer reads what `emit` wrote. Absent when
    // this build has no output format, which is a configuration rather than a
    // fault — nothing is said about it.
    if let Some(renderer) = renderer {
        let written = sink.recorded();
        let render_input = RenderInput {
            view: index.view(),
            shards: index.shards(),
            artifact: &written,
        };
        if let Err(error) = renderer.render(&render_input, &mut sink) {
            let _ = writeln!(streams.err, "error: {}: {error}", renderer.id().0);
            return EXIT_INTERNAL;
        }
        progress(
            options,
            streams,
            &format!("rendered {} with {}", out, renderer.id().0),
        );
    }

    if options.json {
        match serde_json::to_string_pretty(&report) {
            Ok(text) => {
                let _ = writeln!(streams.out, "{text}");
            }
            Err(error) => {
                let _ = writeln!(streams.err, "error: {error}");
                return EXIT_INTERNAL;
            }
        }
    }

    if !options.quiet && report.render(streams.err).is_err() {
        return EXIT_INTERNAL;
    }

    EXIT_OK
}

fn write_run_record(
    sink: &mut RecordingSink,
    report: &RunReport,
) -> Result<(), Box<dyn std::error::Error>> {
    use reachgraph_plugin_api::OutputSink;

    let mut bytes = serde_json::to_vec_pretty(report)?;
    bytes.push(b'\n');
    sink.write("run.json", &bytes)?;
    Ok(())
}

/// Plan-06 §3.1: zero matches prints what each registered plugin looks for, so
/// the failure is diagnostic rather than "unsupported".
fn undetected(registry: &Registry, repo: &str, streams: &mut Streams<'_>) -> u8 {
    let _ = writeln!(streams.err, "error: no registered plugin claims {repo}");
    for entry in registry.plugins() {
        let detection = entry.plugin().detection();
        let _ = writeln!(
            streams.err,
            "  {:<28} looks for {}   analyses {}",
            entry.plugin().id().0,
            list(detection.marker_files),
            list(detection.extensions)
        );
    }
    EXIT_UNDETECTED
}

fn build_failed(error: reachgraph_core::BuildError, streams: &mut Streams<'_>) -> u8 {
    match error {
        reachgraph_core::BuildError::PreflightFailed {
            plugin,
            reason,
            remediation,
        } => {
            let _ = writeln!(streams.err, "error: {} refused to run: {reason}", plugin.0);
            let _ = writeln!(streams.err, "  → {remediation}");
            EXIT_PREFLIGHT
        }
        other => {
            let _ = writeln!(streams.err, "error: {other}");
            EXIT_INTERNAL
        }
    }
}

fn progress(options: &Analyse, streams: &mut Streams<'_>, line: &str) {
    if options.quiet {
        return;
    }
    let _ = writeln!(streams.err, "{line}");
}

#[cfg(feature = "serve")]
fn serve_command(out: &Path, port: u16, streams: &mut Streams<'_>) -> u8 {
    if !out.is_dir() {
        let _ = writeln!(streams.err, "error: {} is not a directory", out.display());
        return EXIT_USAGE;
    }

    let bound = match serve::bind(port) {
        Ok(bound) => bound,
        Err(error) => {
            let _ = writeln!(streams.err, "error: {error}");
            return EXIT_INTERNAL;
        }
    };

    let _ = writeln!(
        streams.err,
        "serving {} at http://{}/ — loopback only; forward a port deliberately if you need \
         remote access",
        out.display(),
        bound.address()
    );

    match bound.serve(out) {
        Ok(()) => EXIT_OK,
        Err(error) => {
            let _ = writeln!(streams.err, "error: {error}");
            EXIT_INTERNAL
        }
    }
}

#[cfg(not(feature = "serve"))]
fn serve_command(_out: &Path, _port: u16, streams: &mut Streams<'_>) -> u8 {
    let _ = writeln!(
        streams.err,
        "error: this build has no `serve` feature; open the artifact with a static file server"
    );
    EXIT_USAGE
}
