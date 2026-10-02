//! `estate::merge` — the cross-repository join on `join_key` (yadgarhq
//! ADR-0842, reachgraph ADR-0010).
//!
//! Three synthetic repositories shaped like the measured estate: a gateway
//! that only consumes, a logic service that serves one RPC and consumes its
//! storage twin, and the twin that serves. Built from the artifact schema's
//! own JSON, so the test reads exactly what a per-repository run writes.

use std::collections::BTreeMap;

use reachgraph_core::estate::{merge, CallSite, JoinStatus, RepoArtifact};
use reachgraph_core::schema::{EndpointsDocument, ShardDocument};
use serde_json::{json, Value};

fn node(raw: &str) -> Value {
    json!({ "plugin": "fixture", "raw": raw })
}

fn symbol(name: &str, doc: Option<&str>, file: &str) -> Value {
    json!({
        "name": name,
        "kind": "method",
        "raw_kind": "Function",
        "range": { "file": file, "span": null },
        "doc": doc,
        "doc_format": "markdown",
        "is_test": false,
        "container": null
    })
}

fn shard_node(raw: &str, name: &str, doc: Option<&str>, file: &str, depth: u32) -> Value {
    json!({
        "id": node(raw),
        "symbol": symbol(name, doc, file),
        "unit": "unit:app",
        "category": "first_party",
        "depth": depth,
        "frontier": false
    })
}

fn edge(from: &str, to: &str) -> Value {
    json!({
        "from": node(from),
        "to": { "state": "resolved", "node": node(to) },
        "call_site": null,
        "provenance": { "plugin": "fixture", "engine": "fixture" },
        "inference_mode": "resolved"
    })
}

/// One root: `(service, operation, direction, join_key, bound node or unbound reason, shard)`.
struct Root<'a> {
    service: &'a str,
    operation: &'a str,
    direction: &'a str,
    join_key: &'a str,
    bound: Result<&'a str, &'a str>,
    shard: Option<(&'a str, Value)>,
}

fn repo(label: &str, roots: Vec<Root<'_>>) -> RepoArtifact {
    let mut operations = Vec::new();
    let mut shards = BTreeMap::new();
    for root in roots {
        let binding = match root.bound {
            Ok(raw) => json!({ "state": "bound", "node": node(raw) }),
            Err(reason) => json!({ "state": "unbound", "reason": reason }),
        };
        let (shard_path, node_count) = match &root.shard {
            Some((path, doc)) => {
                let mut doc = doc.clone();
                doc["root"] = json!({
                    "contract": "c.proto",
                    "version": "v1",
                    "service": root.service,
                    "operation": root.operation,
                    "direction": root.direction,
                    "join_key": root.join_key,
                    "binding": binding.clone(),
                });
                let parsed: ShardDocument = serde_json::from_value(doc).expect("a valid shard");
                let count = parsed.nodes.len();
                shards.insert((*path).to_owned(), parsed);
                (json!(path), count)
            }
            None => (Value::Null, 0),
        };
        operations.push(json!({
            "contract": "c.proto",
            "service": root.service,
            "operation": root.operation,
            "direction": root.direction,
            "versions": [{
                "version": "v1",
                "join_key": root.join_key,
                "binding": binding,
                "shard": shard_path,
                "node_count": node_count,
                "frontier_count": 0
            }]
        }));
    }
    let endpoints: EndpointsDocument = serde_json::from_value(json!({
        "schema_version": 1,
        "generated_by": { "tool": "reachgraph", "version": "0.1.0" },
        "plugins": [],
        "operations": operations,
        "coverage": {
            "contracts": ["c.proto"],
            "versions": [],
            "roots_total": 0,
            "roots_bound": 0,
            "unbound_roots": [],
            "unexamined_contracts": [],
            "units_indexed": [],
            "plugins": [],
            "traversal_terminal_categories": [],
            "partial": false,
            "notes": [format!("a note from {label}")]
        }
    }))
    .expect("a valid endpoints document");
    RepoArtifact {
        label: label.to_owned(),
        endpoints,
        shards,
    }
}

fn shard(nodes: Vec<Value>, edges: Vec<Value>) -> Value {
    let count = nodes.len();
    json!({
        "schema_version": 1,
        "root": null,
        "depth_limit": 3,
        "plugins": [],
        "nodes": nodes,
        "edges": edges,
        "frontier": [],
        "stats": { "node_count": count, "edge_count": 0, "unresolved_edge_count": 0 }
    })
}

const LOGIN: &str = "acme.iam.v1.IamService/Login";
const GET_HASH: &str = "acme.iamdb.v1.IamDbService/GetHash";
const ORPHAN: &str = "acme.billing.v1.Billing/Charge";
const UNUSED: &str = "acme.iamdb.v1.IamDbService/Unused";
const UNBOUND: &str = "acme.iam.v1.IamService/Logout";

fn estate() -> Vec<RepoArtifact> {
    let gateway = repo(
        "gateway",
        vec![
            Root {
                service: "IamService",
                operation: "Login",
                direction: "consumed",
                join_key: LOGIN,
                bound: Ok("gw:stub/login"),
                shard: Some((
                    "graph/gw-login.json",
                    shard(
                        vec![shard_node(
                            "gw:stub/login",
                            "login",
                            None,
                            "target/out/iam.rs",
                            0,
                        )],
                        vec![],
                    ),
                )),
            },
            Root {
                service: "Billing",
                operation: "Charge",
                direction: "consumed",
                join_key: ORPHAN,
                bound: Ok("gw:stub/charge"),
                shard: None,
            },
            Root {
                service: "IamService",
                operation: "Logout",
                direction: "consumed",
                join_key: UNBOUND,
                bound: Err("the generated client stub is not in the index"),
                shard: None,
            },
        ],
    );
    let iam = repo(
        "iam",
        vec![
            Root {
                service: "IamService",
                operation: "Login",
                direction: "served",
                join_key: LOGIN,
                bound: Ok("iam:fn/login"),
                shard: Some((
                    "graph/iam-login.json",
                    shard(
                        vec![
                            shard_node(
                                "iam:fn/login",
                                "login",
                                Some("Username and password to a token."),
                                "src/rpc.rs",
                                0,
                            ),
                            shard_node(
                                "iam:fn/inner",
                                "login_inner",
                                Some("Everything Login does."),
                                "src/login.rs",
                                1,
                            ),
                            shard_node(
                                "iam:stub/get_hash",
                                "get_hash",
                                None,
                                "target/out/iamdb.rs",
                                2,
                            ),
                        ],
                        vec![
                            edge("iam:fn/login", "iam:fn/inner"),
                            edge("iam:fn/inner", "iam:stub/get_hash"),
                        ],
                    ),
                )),
            },
            Root {
                service: "IamDbService",
                operation: "GetHash",
                direction: "consumed",
                join_key: GET_HASH,
                bound: Ok("iam:stub/get_hash"),
                shard: None,
            },
        ],
    );
    let iam_db = repo(
        "iam-db",
        vec![
            Root {
                service: "IamDbService",
                operation: "GetHash",
                direction: "served",
                join_key: GET_HASH,
                bound: Ok("db:fn/get_hash"),
                shard: Some((
                    "graph/db-get.json",
                    shard(
                        vec![shard_node(
                            "db:fn/get_hash",
                            "get_hash",
                            Some("Read the stored hash."),
                            "src/store.rs",
                            0,
                        )],
                        vec![],
                    ),
                )),
            },
            Root {
                service: "IamDbService",
                operation: "Unused",
                direction: "served",
                join_key: UNUSED,
                bound: Ok("db:fn/unused"),
                shard: None,
            },
        ],
    );
    vec![gateway, iam, iam_db]
}

fn join<'a>(
    document: &'a reachgraph_core::estate::EstateDocument,
    key: &str,
) -> &'a reachgraph_core::estate::JoinRow {
    document
        .joins
        .iter()
        .find(|join| join.join_key == key)
        .unwrap_or_else(|| panic!("{key} is in the merge, labelled, never dropped"))
}

#[test]
fn a_consumed_root_joins_the_served_handler_by_join_key() {
    let merged = merge(&estate());
    let login = join(&merged, LOGIN);

    assert_eq!(login.status, JoinStatus::Joined);
    assert_eq!(login.consumed.len(), 1);
    assert_eq!(login.consumed[0].repo, "gateway");
    let stub = login.consumed[0].node.as_ref().expect("the stub is bound");
    assert_eq!(stub.raw, "gw:stub/login");
    assert_eq!(login.served.len(), 1);
    assert_eq!(login.served[0].repo, "iam");
    let handler = login.served[0].node.as_ref().expect("the handler is bound");
    assert_eq!(handler.name, "login");
    assert_eq!(
        handler.doc.as_deref(),
        Some("Username and password to a token.")
    );
}

/// The handler's shard reaches the storage twin's stub, so the join chains on:
/// gateway → iam → iam-db, each hop a join key, each step a path in a shard.
#[test]
fn a_served_handler_records_the_join_keys_its_shard_reaches_with_the_path() {
    let merged = merge(&estate());
    let login = join(&merged, LOGIN);

    let reaches = &login.served[0].reaches;
    assert_eq!(reaches.len(), 1, "{reaches:#?}");
    assert_eq!(reaches[0].join_key, GET_HASH);
    let names: Vec<&str> = reaches[0].path.iter().map(|n| n.name.as_str()).collect();
    assert_eq!(names, ["login", "login_inner", "get_hash"]);

    let get_hash = join(&merged, GET_HASH);
    assert_eq!(get_hash.status, JoinStatus::Joined);
    assert_eq!(get_hash.served[0].repo, "iam-db");
    assert_eq!(get_hash.consumed[0].repo, "iam");
}

#[test]
fn unmatched_joins_are_labelled_not_dropped() {
    let merged = merge(&estate());

    assert_eq!(join(&merged, ORPHAN).status, JoinStatus::ConsumedNotServed);
    assert_eq!(join(&merged, UNUSED).status, JoinStatus::ServedNotConsumed);

    let unbound = join(&merged, UNBOUND);
    assert_eq!(unbound.status, JoinStatus::ConsumedNotServed);
    assert!(unbound.consumed[0].node.is_none());
    assert_eq!(
        unbound.consumed[0].unbound_reason.as_deref(),
        Some("the generated client stub is not in the index")
    );
}

/// Two repositories serving one key is not a join to pick from.
#[test]
fn a_key_served_in_two_repositories_is_ambiguous_rather_than_picked() {
    let mut repos = estate();
    repos.push(repo(
        "iam-v2",
        vec![Root {
            service: "IamService",
            operation: "Login",
            direction: "served",
            join_key: LOGIN,
            bound: Ok("iam2:fn/login"),
            shard: None,
        }],
    ));
    let merged = merge(&repos);

    assert_eq!(join(&merged, LOGIN).status, JoinStatus::AmbiguousServed);
    assert_eq!(join(&merged, LOGIN).served.len(), 2);
}

#[test]
fn every_repository_is_listed_with_its_own_notes() {
    let merged = merge(&estate());
    let labels: Vec<&str> = merged.repos.iter().map(|r| r.label.as_str()).collect();
    assert_eq!(labels, ["gateway", "iam", "iam-db"]);
    assert_eq!(merged.repos[1].notes, ["a note from iam"]);
}

/// A served key whose only consumer side is UNBOUND has no node on the
/// consuming side to join, so it is not `joined`. MEASURED as a defect on the
/// iam + iam-db demo run without `--read-build-output`: 16 keys read `joined`
/// with no consumer node.
#[test]
fn a_served_key_with_only_unbound_consumers_is_not_joined() {
    let repos = vec![
        repo(
            "client",
            vec![Root {
                service: "IamDbService",
                operation: "GetHash",
                direction: "consumed",
                join_key: GET_HASH,
                bound: Err("the generated client stub is not in the index"),
                shard: None,
            }],
        ),
        repo(
            "server",
            vec![Root {
                service: "IamDbService",
                operation: "GetHash",
                direction: "served",
                join_key: GET_HASH,
                bound: Ok("db:fn/get_hash"),
                shard: None,
            }],
        ),
    ];
    let merged = merge(&repos);
    assert_eq!(join(&merged, GET_HASH).status, JoinStatus::ConsumerUnbound);
}

/// Direction comes from contract presence, so a bound consumed side proves a
/// generated client stub exists, not that anything calls it. Every consumed
/// side says so in the data; nothing is suppressed.
#[test]
fn every_consumed_side_is_labelled_stub_only_and_kept() {
    let merged = merge(&estate());
    for join in &merged.joins {
        for side in &join.consumed {
            assert_eq!(
                side.call_site,
                Some(CallSite::NotMeasured),
                "{} in {}",
                join.join_key,
                side.repo
            );
        }
        for side in &join.served {
            assert_eq!(
                side.call_site, None,
                "a served side is a handler, not a stub"
            );
        }
    }
    assert_eq!(
        join(&merged, LOGIN).status,
        JoinStatus::Joined,
        "labelled, not suppressed"
    );
}
