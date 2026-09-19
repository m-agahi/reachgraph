//! Plan-04 §4 — discovery, and a `.proto` reduced to `(package, service, rpc)`.

use std::path::{Path, PathBuf};

use reachgraph_plugin_api::ContractId;
use reachgraph_roots_proto_tonic::contract::{discover, parse, ProtoContract};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn read(root: &Path, relative: &str) -> ProtoContract {
    let source = std::fs::read_to_string(root.join(relative)).expect("the fixture is checked in");
    parse(&ContractId(relative.to_owned()), &source)
        .unwrap_or_else(|error| panic!("{relative} did not parse: {error}"))
}

#[test]
fn parses_package_service_rpc() {
    let root = fixture("repo");

    let api = read(&root, "proto/api.proto");
    assert_eq!(api.package.as_deref(), Some("acme.api.v1"));
    assert_eq!(api.services.len(), 1);
    assert_eq!(api.services[0].name, "Widgets");
    assert_eq!(api.services[0].rpcs, vec!["CreateWidget", "ListWidgets"]);

    let store = read(&root, "proto/store.proto");
    assert_eq!(store.package.as_deref(), Some("acme.store.v2"));
    assert_eq!(store.services[0].name, "WidgetDb");
    assert_eq!(store.services[0].rpcs, vec!["CreateWidget", "GetWidget"]);

    let legacy = read(&root, "proto/legacy.proto");
    assert_eq!(legacy.package.as_deref(), Some("acme.legacy"));
    assert_eq!(
        legacy.version, None,
        "ADR-0007: no version segment, no version"
    );
    assert_eq!(legacy.services[0].rpcs, vec!["Ping"]);
}

/// A contract with no service is parsed, carries no service, and is still a
/// contract. Coverage depends on the distinction (plan-04 §10).
#[test]
fn a_serviceless_contract_parses_and_contributes_no_service() {
    let types = read(&fixture("repo"), "proto/types.proto");
    assert_eq!(types.package.as_deref(), Some("acme.api.v1"));
    assert!(types.services.is_empty());
}

/// MEASURED (plan-04 §4): `protox-parse` reads syntax only and never follows an
/// import, so `api.proto` parses although `acme/api/v1/types.proto` is not on
/// any include path and does not exist at that path at all.
#[test]
fn an_unresolvable_import_is_not_an_error() {
    let api = read(&fixture("repo"), "proto/api.proto");
    assert_eq!(api.services[0].rpcs.len(), 2);
}

#[test]
fn a_file_with_no_package_parses_with_none() {
    let bare = read(&fixture("nopackage"), "proto/bare.proto");
    assert_eq!(bare.package, None);
    assert_eq!(bare.version, None);
    assert_eq!(bare.services[0].name, "Svc");
}

/// Plan-04 §10: unreadable is not a coverage state, so it is an error here.
#[test]
fn an_unparseable_proto_is_an_error() {
    let root = fixture("broken");
    let source = std::fs::read_to_string(root.join("proto/broken.proto")).expect("checked in");
    let error = parse(&ContractId("proto/broken.proto".to_owned()), &source)
        .expect_err("a syntactically invalid file does not parse");
    let rendered = error.to_string();
    assert!(
        rendered.contains("proto/broken.proto"),
        "the error names the file: {rendered}"
    );
}

/// Discovery walks for `**/*.proto` and names each file by its repo-relative
/// path — the `ContractId` plan-04 §5 fixes.
#[test]
fn discovery_finds_every_proto_by_repo_relative_path() {
    let root = fixture("repo");
    let found = discover(&root).expect("the fixture tree is readable");

    let ids: Vec<&str> = found.iter().map(|file| file.contract.0.as_str()).collect();
    assert_eq!(
        ids,
        vec![
            "proto/api.proto",
            "proto/legacy.proto",
            "proto/store.proto",
            "proto/types.proto",
        ],
        "sorted, repo-relative, `/`-separated"
    );

    for file in &found {
        assert!(file.path.is_absolute(), "the path reads the file");
        assert!(file.path.exists());
    }
}

/// `target/` is build output, not a contract, and a VCS directory is not one
/// either. Both exclusions are checked against a tree that contains them.
#[test]
fn discovery_excludes_target_and_vcs_directories() {
    let root = tempdir();
    for relative in [
        "proto/live.proto",
        "target/debug/build/x/out/copied.proto",
        ".git/objects/stray.proto",
    ] {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().expect("has a parent")).expect("mkdir");
        std::fs::write(&path, "syntax = \"proto3\";\n").expect("write");
    }

    let found = discover(&root).expect("readable");
    let ids: Vec<&str> = found.iter().map(|file| file.contract.0.as_str()).collect();
    assert_eq!(ids, vec!["proto/live.proto"]);
}

/// Plan-04 §4: the vendored bundle tag is not the endpoint version, and the
/// simplest way to keep it out of one is never to read the file.
#[test]
fn the_vendored_bundle_tag_file_is_not_a_contract() {
    let found = discover(&fixture("repo")).expect("readable");
    assert!(
        found
            .iter()
            .all(|file| !file.contract.0.contains("PROTO_VERSION")),
        "PROTO_VERSION is not a `.proto` file and is not examined"
    );
}

/// A scratch directory under this test binary's own temp path.
fn tempdir() -> PathBuf {
    let base = std::env::temp_dir().join(format!(
        "reachgraph-roots-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("after the epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&base).expect("mkdir");
    base
}
