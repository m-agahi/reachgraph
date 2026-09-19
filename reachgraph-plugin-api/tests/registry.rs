//! `Registry` behaviour, and the two type-level rules that have no other home
//! until the fixture plugin lands.
//!
//! Plan-00 §5 and plan-01 §10.2 own `empty_marker_files_matches_nothing` and
//! `fixture_is_never_detected`. The second is spelled here as
//! `a_plugin_declaring_no_markers_is_only_reachable_through_select`, because
//! `reachgraph-fixture` has no `FixturePlugin` yet (that is PR B) — the shape
//! it will have is what is asserted.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use reachgraph_plugin_api::{
    Capability, Detection, Plugin, PluginId, PositionEncoding, Preflight, Registry, SourceRange,
    Span,
};

// ---------------------------------------------------------------------------
// Test doubles
// ---------------------------------------------------------------------------

/// A plugin that declares exactly what a test tells it to and does nothing
/// else. It is not `reachgraph-fixture`: that crate reads hand-written JSON and
/// implements every provider trait. This one exists only so `Registry` has
/// something to hold.
struct DeclaringPlugin {
    id: PluginId,
    detection: Detection,
}

impl Plugin for DeclaringPlugin {
    fn id(&self) -> PluginId {
        self.id
    }

    fn provides(&self) -> &[Capability] {
        &[]
    }

    fn position_encoding(&self) -> PositionEncoding {
        PositionEncoding::Utf8Bytes
    }

    fn detection(&self) -> Detection {
        self.detection.clone()
    }

    fn preflight(&self, _root: &Path) -> Preflight {
        Preflight::Ok
    }
}

fn plugin(id: &'static str, marker_files: &'static [&'static str]) -> Box<dyn Plugin> {
    Box::new(DeclaringPlugin {
        id: PluginId(id),
        detection: Detection {
            marker_files,
            extensions: &["rs"],
        },
    })
}

/// A directory under the system temporary directory, deleted on drop.
///
/// Hand-rolled rather than `tempfile`, so that `reachgraph-plugin-api` keeps an
/// empty dependency graph — normal and dev alike. That emptiness is what
/// discharges `plugin_api_has_no_ra_ap_dependency` by construction rather than
/// by inspection.
struct TempRepo(PathBuf);

impl TempRepo {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "reachgraph-registry-{}-{label}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("the system temporary directory is writable");
        Self(path)
    }

    fn with_file(self, name: &str) -> Self {
        fs::write(self.0.join(name), b"").expect("the temporary directory is writable");
        self
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn ids(found: Vec<&dyn Plugin>) -> Vec<&'static str> {
    found.into_iter().map(|p| p.id().0).collect()
}

// ---------------------------------------------------------------------------
// detect
// ---------------------------------------------------------------------------

#[test]
fn detect_returns_a_plugin_whose_marker_file_is_present() {
    let repo = TempRepo::new("present").with_file("Cargo.toml");
    let mut registry = Registry::new();
    registry.register(plugin("lang-rust", &["Cargo.toml"]));

    assert_eq!(ids(registry.detect(repo.path())), vec!["lang-rust"]);
}

#[test]
fn detect_skips_a_plugin_whose_marker_file_is_absent() {
    let repo = TempRepo::new("absent").with_file("Cargo.toml");
    let mut registry = Registry::new();
    registry.register(plugin("lang-go", &["go.mod"]));

    assert!(registry.detect(repo.path()).is_empty());
}

#[test]
fn detect_matches_on_any_declared_marker_not_all_of_them() {
    let repo = TempRepo::new("any").with_file("setup.py");
    let mut registry = Registry::new();
    registry.register(plugin("lang-python", &["pyproject.toml", "setup.py"]));

    assert_eq!(ids(registry.detect(repo.path())), vec!["lang-python"]);
}

/// Plan-00 §2: "An empty `marker_files` matches NOTHING, never everything."
///
/// The repository here contains a `.rs` file and the plugin declares the `rs`
/// extension, so an implementation that fell back to extension matching — or
/// that read an empty marker list as "no constraint" — returns this plugin.
/// Both are the failure this asserts against.
#[test]
fn empty_marker_files_matches_nothing() {
    let repo = TempRepo::new("empty-markers").with_file("main.rs");
    let mut registry = Registry::new();
    registry.register(plugin("declares-nothing", &[]));

    assert!(registry.detect(repo.path()).is_empty());
}

#[test]
fn detect_returns_every_match_not_the_first() {
    let repo = TempRepo::new("several")
        .with_file("Cargo.toml")
        .with_file("go.mod");
    let mut registry = Registry::new();
    registry.register(plugin("lang-rust", &["Cargo.toml"]));
    registry.register(plugin("lang-go", &["go.mod"]));

    assert_eq!(
        ids(registry.detect(repo.path())),
        vec!["lang-rust", "lang-go"]
    );
}

// ---------------------------------------------------------------------------
// select
// ---------------------------------------------------------------------------

#[test]
fn select_finds_a_plugin_by_id() {
    let mut registry = Registry::new();
    registry.register(plugin("lang-rust", &["Cargo.toml"]));

    assert_eq!(
        registry.select(PluginId("lang-rust")).map(|p| p.id()),
        Some(PluginId("lang-rust"))
    );
}

#[test]
fn select_is_none_for_an_unregistered_id() {
    let mut registry = Registry::new();
    registry.register(plugin("lang-rust", &["Cargo.toml"]));

    assert!(registry.select(PluginId("lang-go")).is_none());
}

/// Plan-00 §5: the fixture plugin is explicitly selected, never detected, and
/// the guarantee is structural rather than a special case inside `detect`.
#[test]
fn a_plugin_declaring_no_markers_is_only_reachable_through_select() {
    let repo = TempRepo::new("fixture-shaped").with_file("reachgraph.fixture.json");
    let mut registry = Registry::new();
    registry.register(Box::new(DeclaringPlugin {
        id: PluginId("fixture"),
        detection: Detection {
            marker_files: &[],
            extensions: &[],
        },
    }));

    assert!(registry.detect(repo.path()).is_empty());
    assert!(registry.select(PluginId("fixture")).is_some());
}

// ---------------------------------------------------------------------------
// Honest absence, at the type level
// ---------------------------------------------------------------------------

/// ADR-0003's honest-absence rule, instances 2 and 4 together. Plan-01 §10.2
/// asserts the same thing over the emitted artifact; this asserts it over the
/// type, which is where the artifact's behaviour comes from.
#[test]
fn span_none_is_not_offset_zero_and_the_file_survives_both() {
    let file = PathBuf::from("src/lib.rs");
    let unknown = SourceRange {
        file: file.clone(),
        span: None,
    };
    let start_of_file = SourceRange {
        file: file.clone(),
        span: Some(Span { start: 0, end: 0 }),
    };

    assert_ne!(unknown, start_of_file);
    assert_eq!(unknown.file, file);
    assert_eq!(start_of_file.file, file);
}

/// ADR-0003 field 5 gains a third variant. `Ok | Failed` could not express
/// "the plugin will run, and what it produces means something different from
/// what you expect", so plan-03 §11's non-fatal findings were routed to a side
/// channel instead of the type built to carry them.
///
/// The match arm is the assertion: delete the variant and this fails to
/// compile, which is the strongest failure available.
#[test]
fn preflight_warned_is_non_fatal_and_carries_remediation() {
    let outcomes = [
        Preflight::Ok,
        Preflight::Warned {
            reason: "the repository has not been built".into(),
            remediation: "build the repository so generated code is indexed".into(),
        },
        Preflight::Failed {
            reason: "no toolchain".into(),
            remediation: "install one".into(),
        },
    ];

    let fatal: Vec<bool> = outcomes
        .iter()
        .map(|outcome| match outcome {
            Preflight::Ok | Preflight::Warned { .. } => false,
            Preflight::Failed { .. } => true,
        })
        .collect();

    assert_eq!(fatal, vec![false, false, true]);

    let Preflight::Warned { remediation, .. } = &outcomes[1] else {
        panic!("outcomes[1] is the Warned case");
    };
    assert!(!remediation.is_empty());
}

/// Plan-00 §8 question 7, **decided 2026-09-19 while plan-03 §11 was written**.
///
/// `Warned` carries a `reason` as well as a `remediation`, for the reason
/// plan-00 §8 itself named as the discriminator: "treat a plugin that fuses
/// finding and fix into one `remediation` string as the evidence that the field
/// is wanted." Plan-03 §11's own draft text for check 4 is exactly that fusion
/// — `"rustup component add rust-src — without it, calls into the standard
/// library cannot be located …"` is a fix with a finding welded to its tail.
///
/// The argument recorded against the field was that a warning's fact is already
/// in the run record. MEASURED 2026-09-19 while building `reachgraph-lang-rust`:
/// **there is no run record.** No type in this crate and none in
/// `reachgraph-core` carries per-unit plugin findings, so the fact had nowhere
/// else to be spelled and the duplication the objection feared cannot arise.
///
/// A warning that says only what to do, without what was found, is
/// ADR-0003's honest-absence rule broken once more: a value that says less than
/// the plugin knows.
#[test]
fn preflight_warned_carries_the_finding_and_the_fix() {
    let warned = Preflight::Warned {
        reason: "the rust-src component is not installed, so calls into the \
                 standard library cannot be located"
            .into(),
        remediation: "rustup component add rust-src".into(),
    };

    let Preflight::Warned {
        reason,
        remediation,
    } = &warned
    else {
        panic!("the value under test is the Warned case");
    };

    // The two halves are separable. Fusing them into one string is what the
    // decision above rejected, so the test that would pass on a fused value is
    // not the test to write.
    assert!(!reason.is_empty(), "a warning states what it found");
    assert!(!remediation.is_empty(), "a warning states what to do");
    assert!(
        !remediation.contains("cannot be located"),
        "the finding belongs in `reason`, not welded to the remediation"
    );
}
