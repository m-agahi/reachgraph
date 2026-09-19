//! The renderer half of the binary — plan-06 §1.1, §3.1 and plan-05.

use std::fs;

use reachgraph_cli::args::{self, Analyse, Command};
use reachgraph_cli::renderers::{default_registry, renderer_registry};
use reachgraph_plugin_api::Registry;

use crate::support::{self, TempDir};

fn parse(args: &[&str]) -> Result<Command, args::UsageError> {
    let owned: Vec<String> = args.iter().map(|arg| (*arg).to_owned()).collect();
    args::parse(&owned)
}

fn analysed(case: &str, label: &str) -> (support::Run, TempDir, std::path::PathBuf) {
    let temp = TempDir::new(label);
    let repo = support::repo_for(case, &temp);
    let out = temp.join("out");
    let registry = support::registry_of(support::doc_of(case), &repo);
    let result = support::run(
        &registry,
        &[
            repo.to_str().expect("utf-8"),
            "-o",
            out.to_str().expect("utf-8"),
        ],
    );
    (result, temp, out)
}

/// **Plan-00 §3.6, as a test rather than a sentence.** `Capability::Render` no
/// longer exists; "this is a renderer" is expressed by type, and the analysis
/// registry cannot hold one. An output format is **asked for**, never detected
/// from a repository — a repository that happened to look like one could
/// otherwise choose how it is displayed.
#[test]
fn detect_never_returns_a_renderer() {
    let temp = TempDir::new("detect-renderer");
    let repo = support::repo_for("minimal", &temp);
    let registry = support::registry_of(support::doc_of("minimal"), &repo);

    let detected = registry.detect(&repo);
    assert!(
        !detected.is_empty(),
        "the guard would be vacuous if detection found nothing at all"
    );

    let formats = default_registry();
    let format_names: Vec<&str> = formats.all().map(|entry| entry.renderer().id().0).collect();
    assert!(
        !format_names.is_empty(),
        "this build registers no renderer, so the guard proves nothing"
    );

    for entry in detected {
        let id = entry.plugin().id().0;
        assert!(
            !format_names.contains(&id),
            "detection returned a renderer: {id}"
        );
    }

    // And the other direction: an empty analysis registry detects nothing,
    // renderers included.
    assert!(Registry::new().detect(&repo).is_empty());
}

/// Plan-06 §1: two tables, because there are two registries, and the second
/// says how it is reached.
#[test]
fn the_plugins_subcommand_lists_output_formats_separately() {
    let temp = TempDir::new("plugins-formats");
    let repo = support::repo_for("minimal", &temp);
    let registry = support::registry_of(support::doc_of("minimal"), &repo);

    let result = support::run(&registry, &["plugins"]);
    assert_eq!(result.code, reachgraph_cli::EXIT_OK);
    assert!(result.out.contains("analysis plugins"), "{}", result.out);
    assert!(result.out.contains("output formats"), "{}", result.out);
    assert!(result.out.contains("never detected"), "{}", result.out);
    assert!(result.out.contains("html"), "{}", result.out);

    let plugins_at = result.out.find("analysis plugins").expect("the heading");
    let formats_at = result.out.find("output formats").expect("the heading");
    assert!(plugins_at < formats_at, "{}", result.out);
}

/// ADR-0006's layout, as this build actually writes it. The waist's four JSON
/// documents, the binary's run record, and the renderer's page, presenter,
/// sidecar and vendored bundles.
#[test]
fn a_run_writes_the_page_beside_the_waists_json() {
    let (result, _temp, out) = analysed("versioned_pair", "layout");
    assert_eq!(result.code, reachgraph_cli::EXIT_OK, "{}", result.err);

    for name in [
        "endpoints.json",
        "unreachable.json",
        "versions.json",
        "run.json",
        "index.html",
        "overview.html",
        "structure.json",
        "loader.js",
    ] {
        assert!(out.join(name).is_file(), "{name} was not written");
    }
    assert!(out.join("graph").is_dir());
    assert!(out.join("vendor").is_dir());
    assert!(out.join("vendor/cytoscape.min.js").is_file());
    assert!(out.join("vendor/LICENSES.txt").is_file());
}

/// Plan-05 §6.5's inlined page carries the same bytes the sharded directory
/// holds, and this is the assertion that proves the seam rather than arguing
/// it: one serialiser, the waist's, and the renderer copies.
#[test]
fn the_inlined_page_carries_the_files_on_disk() {
    let (result, _temp, out) = analysed("versioned_pair", "inline-equal");
    assert_eq!(result.code, reachgraph_cli::EXIT_OK, "{}", result.err);

    let page = fs::read_to_string(out.join("overview.html")).expect("the page is written");
    let start = page
        .find(r#"<script type="application/json" id="rg-data">"#)
        .expect("the data block is present");
    let body = &page[start..];
    let open = body.find('>').expect("the tag closes") + 1;
    let end = body.find("</script>").expect("the block closes");
    let inline: serde_json::Value =
        serde_json::from_str(&body[open..end]).expect("the inline block is json");

    for name in ["endpoints.json", "unreachable.json", "versions.json"] {
        let on_disk: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(out.join(name)).expect("written"))
                .expect("json");
        assert_eq!(inline[name], on_disk, "{name} drifted");
    }

    // Every shard too, by the path the page keys them on.
    for entry in fs::read_dir(out.join("graph")).expect("the shard directory is readable") {
        let entry = entry.expect("readable");
        let key = format!("graph/{}", entry.file_name().to_string_lossy());
        let on_disk: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(entry.path()).expect("written"))
                .expect("json");
        assert_eq!(inline[&key], on_disk, "{key} drifted");
    }
}

/// Plan-05 §6.5: `--no-overview` is a different instruction from a small
/// threshold, and neither touches the sharded directory.
#[test]
fn no_overview_suppresses_the_single_file_page_only() {
    let temp = TempDir::new("no-overview");
    let repo = support::repo_for("minimal", &temp);
    let out = temp.join("out");
    let registry = support::registry_of(support::doc_of("minimal"), &repo);

    let result = support::run(
        &registry,
        &[
            repo.to_str().expect("utf-8"),
            "-o",
            out.to_str().expect("utf-8"),
            "--no-overview",
        ],
    );
    assert_eq!(result.code, reachgraph_cli::EXIT_OK, "{}", result.err);
    assert!(!out.join("overview.html").exists());
    assert!(out.join("index.html").is_file());
    assert!(out.join("graph").is_dir());
}

/// A threshold nothing fits under leaves the sharded output alone too.
#[test]
fn a_tiny_threshold_suppresses_the_single_file_page_only() {
    let temp = TempDir::new("tiny-threshold");
    let repo = support::repo_for("minimal", &temp);
    let out = temp.join("out");
    let registry = support::registry_of(support::doc_of("minimal"), &repo);

    let result = support::run(
        &registry,
        &[
            repo.to_str().expect("utf-8"),
            "-o",
            out.to_str().expect("utf-8"),
            "--inline-threshold",
            "1",
        ],
    );
    assert_eq!(result.code, reachgraph_cli::EXIT_OK, "{}", result.err);
    assert!(!out.join("overview.html").exists());
    assert!(out.join("index.html").is_file());
}

/// Two settings of one decision. Accepting both and picking one silently would
/// make the page's presence depend on a precedence rule nobody stated.
#[test]
fn no_overview_and_inline_threshold_together_are_a_usage_error() {
    let error = parse(&["repo", "--no-overview", "--inline-threshold", "100"])
        .expect_err("the combination is refused");
    assert!(error.0.contains("pass one"), "{}", error.0);
}

/// The three flags parse into the shape the renderer is built from.
#[test]
fn the_renderer_flags_parse() {
    let Command::Analyse(options) =
        parse(&["repo", "--renderer", "html", "--inline-threshold", "4096"])
            .expect("the flags parse")
    else {
        panic!("expected an analyse command");
    };
    assert_eq!(options.renderer.as_deref(), Some("html"));
    assert_eq!(options.inline_threshold, Some(4096));
    assert!(!options.no_overview);

    let Command::Analyse(refused) = parse(&["repo", "--no-overview"]).expect("parses") else {
        panic!("expected an analyse command");
    };
    assert!(refused.no_overview);
    assert_eq!(refused.inline_threshold, None);
}

/// A value-taking flag with no value is a usage error, not a silent default.
#[test]
fn a_renderer_flag_without_a_value_is_a_usage_error() {
    assert!(parse(&["repo", "--renderer"]).is_err());
    assert!(parse(&["repo", "--inline-threshold"]).is_err());
    assert!(parse(&["repo", "--inline-threshold", "lots"]).is_err());
}

/// Plan-06 §3.1's rule for detection, applied to selection: a failure that
/// says what was available is diagnostic; one that says "unsupported" is not.
#[test]
fn an_unknown_renderer_names_the_ones_that_exist() {
    let temp = TempDir::new("unknown-renderer");
    let repo = support::repo_for("minimal", &temp);
    let out = temp.join("out");
    let registry = support::registry_of(support::doc_of("minimal"), &repo);

    let result = support::run(
        &registry,
        &[
            repo.to_str().expect("utf-8"),
            "-o",
            out.to_str().expect("utf-8"),
            "--renderer",
            "svg",
        ],
    );
    assert_eq!(result.code, reachgraph_cli::EXIT_USAGE);
    assert!(
        result.err.contains("no output format named svg"),
        "{}",
        result.err
    );
    assert!(result.err.contains("html"), "{}", result.err);
    assert!(
        !out.exists(),
        "nothing was written for a run that was refused"
    );
}

/// The registry is built per run, because a renderer carries the run's own
/// options rather than receiving them through a parameter the next renderer
/// would have to ignore.
#[test]
fn the_registry_carries_the_runs_options() {
    use reachgraph_render_html::Overview;

    let refused = renderer_registry(&Analyse {
        no_overview: true,
        ..Analyse::default()
    });
    let renderer = refused.select(None).expect("a default format");
    assert_eq!(renderer.renderer().id().0, "html");

    let sized = renderer_registry(&Analyse {
        inline_threshold: Some(77),
        ..Analyse::default()
    });
    assert!(sized.select(Some("html")).is_ok());
    assert!(sized.select(Some("dot")).is_err());

    // The instance really did take the option, rather than the flag being
    // parsed and dropped.
    assert_eq!(
        reachgraph_render_html::HtmlRenderer::with_overview(Overview::Under(77)).overview(),
        Overview::Under(77)
    );
}

/// Plan-06 §1.1: the artifact is regenerated, never merged into. A re-run
/// removes the previous renderer output as well as the previous JSON, and
/// `Renderer::owns` is what tells the binary which names those are.
#[test]
fn a_rerun_replaces_the_page_and_its_vendored_bundles() {
    let temp = TempDir::new("rerun-page");
    let repo = support::repo_for("minimal", &temp);
    let out = temp.join("out");
    let registry = support::registry_of(support::doc_of("minimal"), &repo);
    let args = [
        repo.to_str().expect("utf-8"),
        "-o",
        out.to_str().expect("utf-8"),
    ];

    assert_eq!(support::run(&registry, &args).code, reachgraph_cli::EXIT_OK);

    // A file from a previous run that this run will not write again.
    let stale = out.join("vendor/from-a-previous-release.js");
    fs::write(&stale, b"stale").expect("writable");
    assert!(stale.is_file());

    let second = support::run(&registry, &args);
    assert_eq!(second.code, reachgraph_cli::EXIT_OK, "{}", second.err);
    assert!(
        !stale.exists(),
        "a stale vendored bundle survived a re-run and would be served beside a fresh page"
    );
    assert!(out.join("index.html").is_file());
}

/// The renderer's own files count as reachgraph's. Without this, a second run
/// into the same directory would be refused as dirty by the output the first
/// run wrote.
#[test]
fn the_page_does_not_make_the_directory_look_foreign() {
    let temp = TempDir::new("not-foreign");
    let repo = support::repo_for("minimal", &temp);
    let out = temp.join("out");
    let registry = support::registry_of(support::doc_of("minimal"), &repo);
    let args = [
        repo.to_str().expect("utf-8"),
        "-o",
        out.to_str().expect("utf-8"),
    ];

    assert_eq!(support::run(&registry, &args).code, reachgraph_cli::EXIT_OK);
    let second = support::run(&registry, &args);
    assert_eq!(second.code, reachgraph_cli::EXIT_OK, "{}", second.err);

    // And a file this tool never writes still makes it foreign.
    fs::write(out.join("notes.txt"), b"mine").expect("writable");
    let third = support::run(&registry, &args);
    assert_eq!(third.code, reachgraph_cli::EXIT_USAGE, "{}", third.err);
    assert!(
        out.join("notes.txt").is_file(),
        "a foreign file was deleted"
    );
}
