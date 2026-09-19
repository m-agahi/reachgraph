//! Preflight, and how a finding that can never clear is presented — plan-06 §4.

use std::io::Write;

use reachgraph_core::{BuildDiagnostic, Index};
use reachgraph_plugin_api::{Preflight, Registered};

use crate::report::Limit;
use crate::{Streams, EXIT_OK, EXIT_PREFLIGHT};

/// One plugin's answer.
pub struct Check {
    /// Who answered.
    pub plugin: String,
    /// What it answered.
    pub outcome: Preflight,
}

/// Run every detected plugin's own prerequisite check.
///
/// **Never `command -v`.** design.md §10 MEASURED that a name resolving on PATH
/// proves nothing, and under ADR-0001 there is no external binary to probe: the
/// checks are about the repository, not the environment.
///
/// The selected renderer is not preflighted, and would not be if one existed:
/// `Renderer` has no `preflight` method, so an "ok" row for it would be a
/// vacuous check reported as a passing one.
pub fn check_all(detected: &[&Registered], repo: &std::path::Path) -> Vec<Check> {
    detected
        .iter()
        .map(|entry| Check {
            plugin: entry.plugin().id().0.to_owned(),
            outcome: entry.plugin().preflight(repo),
        })
        .collect()
}

/// Whether this check stops the run.
pub fn is_failure(check: &Check) -> bool {
    matches!(check.outcome, Preflight::Failed { .. })
}

/// The table, on stderr.
///
/// Three labels, and the middle one is the point. `Warned` is **not** printed
/// as a warning: ADR-0728 disables proc-macro expansion unconditionally, so the
/// Rust plugin warns on every run for ever, and a `WARN` row that never clears
/// teaches a reader to skip the column. A `Warned` plugin runs, and what it
/// found is a property of the artifact — so the row says `noted` and the text
/// is repeated under "limits of this run", beside the artifact it qualifies.
pub fn render(checks: &[Check], into: &mut dyn Write) {
    let _ = writeln!(into, "preflight");
    for check in checks {
        match &check.outcome {
            Preflight::Ok => {
                let _ = writeln!(into, "  {:<28} ok", check.plugin);
            }
            Preflight::Warned { .. } => {
                let _ = writeln!(into, "  {:<28} noted", check.plugin);
            }
            Preflight::Failed {
                reason,
                remediation,
            } => {
                let _ = writeln!(into, "  {:<28} FAIL  {reason}", check.plugin);
                let _ = writeln!(into, "  {:<28}       → {remediation}", "");
            }
        }
    }
}

/// The `Warned` rows, as limits of the run.
pub fn limits(checks: &[Check]) -> Vec<Limit> {
    checks
        .iter()
        .filter_map(|check| match &check.outcome {
            Preflight::Warned {
                reason,
                remediation,
            } => Some(Limit {
                plugin: check.plugin.clone(),
                finding: reason.clone(),
                remediation: Some(remediation.clone()),
            }),
            _ => None,
        })
        .collect()
}

/// The plugin-authored notes a finished index carries.
///
/// These are the other half of plan-03 §9 D-D: preflight says what to do before
/// a run, a note says what the finished artifact contains. They arrive with no
/// remediation because there is nothing to do about them — the index is what it
/// is, and the reader's job is to read it correctly.
pub fn notes_as_limits(index: &Index) -> Vec<Limit> {
    let mut limits: Vec<Limit> = index
        .coverage()
        .notes
        .iter()
        .map(|note| Limit {
            plugin: "index".to_owned(),
            finding: note.clone(),
            remediation: None,
        })
        .collect();

    // A provider the build tolerated is a limit too, and a louder one: every
    // unreachability claim weakens when part of the index is missing.
    for diagnostic in index.diagnostics() {
        if let BuildDiagnostic::ProviderFailed { plugin, detail } = diagnostic {
            limits.push(Limit {
                plugin: plugin.0.to_owned(),
                finding: format!("this provider failed and the run continued: {detail}"),
                remediation: None,
            });
        }
    }

    limits
}

/// `reachgraph preflight <repo>` — the same checks, no analysis, so CI can gate
/// cheaply.
pub fn subcommand(
    registry: &reachgraph_plugin_api::Registry,
    repo: &std::path::Path,
    json: bool,
    streams: &mut Streams<'_>,
) -> u8 {
    let detected = registry.detect(repo);
    if detected.is_empty() {
        return crate::undetected(registry, &repo.display().to_string(), streams);
    }

    let checks = check_all(&detected, repo);

    if json {
        let rows: Vec<serde_json::Value> = checks
            .iter()
            .map(|check| {
                let (outcome, reason, remediation) = match &check.outcome {
                    Preflight::Ok => ("ok", None, None),
                    Preflight::Warned {
                        reason,
                        remediation,
                    } => ("noted", Some(reason.clone()), Some(remediation.clone())),
                    Preflight::Failed {
                        reason,
                        remediation,
                    } => ("failed", Some(reason.clone()), Some(remediation.clone())),
                };
                serde_json::json!({
                    "plugin": check.plugin,
                    "outcome": outcome,
                    "reason": reason,
                    "remediation": remediation,
                })
            })
            .collect();
        let document = serde_json::json!({ "checks": rows });
        match serde_json::to_string_pretty(&document) {
            Ok(text) => {
                let _ = writeln!(streams.out, "{text}");
            }
            Err(error) => {
                let _ = writeln!(streams.err, "error: {error}");
                return crate::EXIT_INTERNAL;
            }
        }
    }

    render(&checks, streams.err);

    if checks.iter().any(is_failure) {
        return EXIT_PREFLIGHT;
    }

    EXIT_OK
}
