//! `reachgraph merge` — ADR-0010.
//!
//! Two hand-written per-repository artifacts, in the exact shape a run
//! writes: `consumer` consumes `acme.v1.Store/Get` through a bound node, and
//! `server` serves it. The merge reads both, joins them, and writes
//! `estate.json` and `estate.html`. Nothing is analysed.

use std::fs;
use std::path::Path;

use serde_json::{json, Value};

use crate::support::{doc_of, registry_of, run, TempDir};

const GET: &str = "acme.v1.Store/Get";
const PUT: &str = "acme.v1.Store/Put";

fn binding(raw: &str) -> Value {
    json!({ "state": "bound", "node": { "plugin": "fixture", "raw": raw } })
}

fn operation(operation: &str, direction: &str, key: &str, raw: &str, shard: Value) -> Value {
    json!({
        "contract": "store.proto",
        "service": "Store",
        "operation": operation,
        "direction": direction,
        "versions": [{
            "version": "v1",
            "join_key": key,
            "binding": binding(raw),
            "shard": shard,
            "node_count": 1,
            "frontier_count": 0
        }]
    })
}

fn write_artifact(dir: &Path, operations: Vec<Value>, shards: Vec<(&str, Value)>) {
    fs::create_dir_all(dir.join("graph")).expect("graph dir");
    let endpoints = json!({
        "schema_version": 1,
        "generated_by": { "tool": "reachgraph", "version": "0.1.0" },
        "plugins": [],
        "operations": operations,
        "coverage": {
            "contracts": ["store.proto"], "versions": [], "roots_total": 1, "roots_bound": 1,
            "unbound_roots": [], "unexamined_contracts": [], "units_indexed": [], "plugins": [],
            "traversal_terminal_categories": [], "partial": false,
            "notes": ["proc-macro expansion is disabled in this index"]
        }
    });
    fs::write(dir.join("endpoints.json"), endpoints.to_string()).expect("endpoints");
    for (path, shard) in shards {
        fs::write(dir.join(path), shard.to_string()).expect("shard");
    }
}

fn served_shard(key: &str) -> Value {
    json!({
        "schema_version": 1,
        "root": {
            "contract": "store.proto", "version": "v1", "service": "Store", "operation": "Get",
            "direction": "served", "join_key": key, "binding": binding("srv:fn/get")
        },
        "depth_limit": 3,
        "plugins": [],
        "nodes": [{
            "id": { "plugin": "fixture", "raw": "srv:fn/get" },
            "symbol": {
                "name": "get", "kind": "method", "raw_kind": "Function",
                "range": { "file": "src/store.rs", "span": null },
                "doc": "Read one <record> by key.", "doc_format": "markdown",
                "is_test": false, "container": null
            },
            "unit": "unit:server", "category": "first_party", "depth": 0, "frontier": false
        }],
        "edges": [],
        "frontier": [],
        "stats": { "node_count": 1, "edge_count": 0, "unresolved_edge_count": 0 }
    })
}

fn estate(temp: &TempDir) -> (std::path::PathBuf, std::path::PathBuf) {
    let consumer = temp.join("consumer");
    write_artifact(
        &consumer,
        vec![
            operation("Get", "consumed", GET, "con:stub/get", Value::Null),
            operation("Put", "consumed", PUT, "con:stub/put", Value::Null),
        ],
        vec![],
    );
    let server = temp.join("server");
    write_artifact(
        &server,
        vec![operation(
            "Get",
            "served",
            GET,
            "srv:fn/get",
            json!("graph/get.json"),
        )],
        vec![("graph/get.json", served_shard(GET))],
    );
    (consumer, server)
}

fn registry(temp: &TempDir) -> reachgraph_plugin_api::Registry {
    registry_of(doc_of("minimal"), &temp.join("case"))
}

#[test]
fn merge_joins_two_artifacts_and_writes_both_files() {
    let temp = TempDir::new("merge-joins");
    let (consumer, server) = estate(&temp);
    let out = temp.join("estate");

    let result = run(
        &registry(&temp),
        &[
            "merge",
            consumer.to_str().unwrap(),
            &format!("srv={}", server.join("endpoints.json").display()),
            "-o",
            out.to_str().unwrap(),
        ],
    );
    assert_eq!(result.code, reachgraph_cli::EXIT_OK, "{}", result.err);

    let document: Value =
        serde_json::from_str(&fs::read_to_string(out.join("estate.json")).expect("estate.json"))
            .expect("estate.json parses");
    let labels: Vec<&str> = document["repos"]
        .as_array()
        .unwrap()
        .iter()
        .map(|repo| repo["label"].as_str().unwrap())
        .collect();
    assert_eq!(
        labels,
        ["consumer", "srv"],
        "a label= prefix names the repository"
    );

    let joins = document["joins"].as_array().unwrap();
    let get = joins.iter().find(|j| j["join_key"] == GET).expect("Get");
    assert_eq!(get["status"], "joined");
    let put = joins
        .iter()
        .find(|j| j["join_key"] == PUT)
        .expect("Put is kept");
    assert_eq!(put["status"], "consumed_not_served");

    let html = fs::read_to_string(out.join("estate.html")).expect("estate.html");
    assert!(
        html.contains("Read one &lt;record&gt; by key."),
        "the served handler's doc is on its box, escaped"
    );
    assert!(
        html.contains(r##"href="#join-acme-v1-Store-Get""##),
        "the consumed RPC links to the served handler's section"
    );
    assert!(
        html.contains("not served by any repository in this merge"),
        "the unmatched join is labelled, not dropped"
    );
    assert!(
        html.contains("proc-macro expansion is disabled in this index"),
        "each repository's limits travel into the overview"
    );
}

#[test]
fn merge_names_a_missing_input() {
    let temp = TempDir::new("merge-missing");
    let (consumer, _) = estate(&temp);
    let missing = temp.join("nope");
    let result = run(
        &registry(&temp),
        &[
            "merge",
            consumer.to_str().unwrap(),
            missing.to_str().unwrap(),
            "-o",
            temp.join("estate").to_str().unwrap(),
        ],
    );
    assert_ne!(result.code, reachgraph_cli::EXIT_OK);
    assert!(
        result.err.contains(&missing.display().to_string()),
        "{}",
        result.err
    );
}

#[test]
fn merge_refuses_to_overwrite_without_force() {
    let temp = TempDir::new("merge-force");
    let (consumer, server) = estate(&temp);
    let out = temp.join("estate");
    fs::create_dir_all(&out).unwrap();
    fs::write(out.join("estate.html"), "someone else's").unwrap();

    let args = [
        "merge",
        consumer.to_str().unwrap(),
        server.to_str().unwrap(),
        "-o",
        out.to_str().unwrap(),
    ];
    let refused = run(&registry(&temp), &args);
    assert_ne!(refused.code, reachgraph_cli::EXIT_OK);
    assert_eq!(
        fs::read_to_string(out.join("estate.html")).unwrap(),
        "someone else's"
    );

    let mut forced: Vec<&str> = args.to_vec();
    forced.push("--force");
    assert_eq!(run(&registry(&temp), &forced).code, reachgraph_cli::EXIT_OK);
}

#[test]
fn merge_needs_at_least_two_inputs() {
    let temp = TempDir::new("merge-one");
    let (consumer, _) = estate(&temp);
    let result = run(
        &registry(&temp),
        &["merge", consumer.to_str().unwrap(), "-o", "x"],
    );
    assert_eq!(result.code, reachgraph_cli::EXIT_USAGE, "{}", result.err);
}
