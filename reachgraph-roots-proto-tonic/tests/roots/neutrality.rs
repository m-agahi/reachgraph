//! The rules this crate can only keep by asserting them — plan-04 §12.

use std::path::{Path, PathBuf};

/// A workspace crate's `src` directory.
fn crate_src(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the crate sits in the workspace")
        .join(name)
        .join("src")
}

fn rust_files(directory: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let entries = std::fs::read_dir(directory).expect("the crate is checked in");
    for entry in entries {
        let path = entry.expect("readable").path();
        if path.is_dir() {
            out.extend(rust_files(&path));
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            out.push(path);
        }
    }
    out.sort();
    out
}

/// `Root::join_key` is plugin-spelled and **core-opaque** — plan-04 §8.
///
/// The companion to plan-00 §6.1's `plugin_api_has_no_ra_ap_dependency`, and
/// the same mechanism: a rule the build enforces rather than a rule reviewers
/// remember. ADR-0007 is explicit that parsing a version out of a route string
/// is per-framework knowledge that must not reach the waist, and a key the core
/// only ever moves is what makes that enforceable rather than aspirational.
#[test]
fn join_key_is_never_parsed_by_the_core() {
    // Every way of taking a key apart. `clone`, a field read and a comparison
    // are what moving one looks like; everything here is what reading one
    // looks like.
    const DISSECTION: &[&str] = &[
        ".split",
        ".rsplit",
        ".splitn",
        ".find(",
        ".rfind(",
        ".strip_prefix",
        ".strip_suffix",
        ".starts_with",
        ".ends_with",
        ".contains(",
        ".chars()",
        ".bytes()",
        ".parse",
        ".get(",
        ".trim",
        "[..",
    ];

    let mut offenders: Vec<String> = Vec::new();
    for file in rust_files(&crate_src("reachgraph-core")) {
        let text = std::fs::read_to_string(&file).expect("readable");
        for (number, line) in text.lines().enumerate() {
            let code = line.trim_start();
            if code.starts_with("//") || !line.contains("join_key") {
                continue;
            }
            if DISSECTION.iter().any(|method| line.contains(method)) {
                offenders.push(format!("{}:{}: {}", file.display(), number + 1, code));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "the core takes a join key apart, which is protobuf-shaped knowledge in the waist:\n{}",
        offenders.join("\n")
    );
}

/// The guard above proves nothing unless the core actually holds a key.
///
/// A source scan that finds no occurrences passes by having nothing to find,
/// which is the green tick meaning nothing that plan-02 §7.2 names.
#[test]
fn the_core_does_carry_the_key_it_must_not_parse() {
    let carried = rust_files(&crate_src("reachgraph-core"))
        .iter()
        .filter_map(|file| std::fs::read_to_string(file).ok())
        .filter(|text| text.contains("join_key"))
        .count();
    assert!(
        carried >= 2,
        "the core moves the key into the artifact; if it no longer does, the guard above is empty"
    );
}

/// The parser stamp is a fact about the dependency, not a string someone typed.
#[test]
fn parser_string_matches_pinned_version() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the crate sits in the workspace")
        .join("Cargo.toml");
    let text = std::fs::read_to_string(manifest).expect("the workspace manifest is checked in");

    let pinned = text
        .lines()
        .find_map(|line| line.trim().strip_prefix("protox-parse = "))
        .map(|rest| rest.trim().trim_matches('"').to_owned())
        .expect("the workspace pins protox-parse");

    assert_eq!(
        pinned,
        reachgraph_roots_proto_tonic::PROTOX_PARSE_VERSION,
        "a bump that forgets the stamp would ship a lie in every parse error"
    );
    assert!(reachgraph_roots_proto_tonic::PARSER.ends_with(&pinned));
}
