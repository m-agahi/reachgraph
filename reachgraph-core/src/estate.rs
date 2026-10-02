//! The estate merge — several repositories' artifacts joined on `join_key`.
//!
//! reachgraph ADR-0010, from yadgarhq ADR-0842. A per-repository run binds a
//! **consumed** root to the node its operation leaves through, and a
//! **served** root to the node it enters at. Both carry the same
//! fully-qualified `join_key` (ADR-0007), so joining two repositories is a
//! lookup, not an inference: no name is matched, and no edge is synthesised
//! that a run did not record.
//!
//! # What the merge claims, and what it does not
//!
//! A join is between the artifacts handed to this merge and nothing else. A
//! key consumed here and served by a repository that was not merged is
//! labelled [`JoinStatus::ConsumedNotServed`]. That label means "not served
//! by any repository in this merge", never "served by no one". Every key
//! appears in the output. An unmatched join is labelled, never dropped,
//! because a dropped one is indistinguishable from one that was never
//! looked for — ADR-0007's partial-index problem, one level up.
//!
//! # A consumed side proves a client stub exists, not a call
//!
//! A per-repository run calls an operation **consumed** when the repository
//! carries its contract and does not serve it. Bound, the root names the
//! generated client method, which exists for every RPC of the contract whether
//! or not anything calls it. MEASURED on the yadgarhq estate: gateway
//! "consumes" all five `TaskDbService` RPCs and never names
//! `TaskDbServiceClient`. So every consumed side carries
//! [`CallSite::NotMeasured`], and the page says so wherever a consumed side is
//! drawn. Nothing is suppressed: a repository that serves no gRPC (gateway) has
//! no other place for a trace to start.
//!
//! # How a served handler's outgoing joins are found
//!
//! A served root's shard holds the nodes reachable from its handler. When
//! one of those nodes is the bound node of a consumed root **in the same
//! repository**, the handler reaches that consumed operation, and through its
//! join key the next repository. The path inside the shard is recovered from
//! the shard's own resolved edges, so the drill-down shows the calls a run
//! recorded and no others.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde::Serialize;

use crate::schema::{
    BindingRow, CategoryRow, DirectionRow, EdgeTargetRow, EndpointsDocument, NodeRef, ShardDocument,
};

/// The estate document's schema version.
pub const ESTATE_SCHEMA_VERSION: u32 = 1;

/// The sentence every estate artifact carries about its own scope.
pub const ESTATE_CLAIM: &str =
    "joined on join_key across the repositories in this merge only; a key with no \
     counterpart is labelled, not dropped";

/// One repository's artifact, as the merge reads it.
#[derive(Clone, Debug)]
pub struct RepoArtifact {
    /// How the repository is named in the merge.
    pub label: String,
    /// Its `endpoints.json`.
    pub endpoints: EndpointsDocument,
    /// Its shards, keyed by the path `endpoints.json` gives for each.
    pub shards: BTreeMap<String, ShardDocument>,
}

/// The merged estate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EstateDocument {
    /// [`ESTATE_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// [`ESTATE_CLAIM`].
    pub claim: String,
    /// One row per repository, in the order given.
    pub repos: Vec<EstateRepoRow>,
    /// One row per join key, sorted by key.
    pub joins: Vec<JoinRow>,
}

/// One merged repository.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EstateRepoRow {
    /// Its label.
    pub label: String,
    /// Roots the repository's run found.
    pub roots_total: usize,
    /// Roots it bound.
    pub roots_bound: usize,
    /// Whether its run declared itself partial.
    pub partial: bool,
    /// Its run's notes — what that index could not see.
    pub notes: Vec<String>,
}

/// How a join key fared across the merge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JoinStatus {
    /// Served by exactly one repository and consumed by at least one.
    Joined,
    /// Consumed, and no repository in this merge serves it.
    ConsumedNotServed,
    /// Served, and no repository in this merge consumes it.
    ServedNotConsumed,
    /// Served by two or more repositories. Not resolved by preference.
    AmbiguousServed,
    /// Served by exactly one repository, and every consumer side is unbound:
    /// no node on the consuming side to join.
    ConsumerUnbound,
}

/// What is known about the call sites behind a consumed side.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CallSite {
    /// A client stub exists; whether first-party code calls it was not
    /// measured. Every consumed side carries this today.
    NotMeasured,
}

/// The sentence a rendered consumed side carries, beside [`CallSite::NotMeasured`].
pub const STUB_ONLY_LABEL: &str = "client stub exists; call site not measured";

/// One join key, both sides.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct JoinRow {
    /// The fully-qualified operation key.
    pub join_key: String,
    /// How it fared.
    pub status: JoinStatus,
    /// Every repository that serves it.
    pub served: Vec<SideRow>,
    /// Every repository that consumes it.
    pub consumed: Vec<SideRow>,
}

/// One repository's side of a join.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SideRow {
    /// The repository.
    pub repo: String,
    /// The contract the root came from.
    pub contract: String,
    /// The service.
    pub service: String,
    /// The operation.
    pub operation: String,
    /// The version segment, `None` when the contract has none.
    pub version: Option<String>,
    /// The bound node, when the root bound and the node's symbol is known.
    pub node: Option<NodeCard>,
    /// Why the root did not bind, when it did not.
    pub unbound_reason: Option<String>,
    /// Consumed sides only: what is known about the calls behind the stub.
    /// `None` on a served side, which is a handler rather than a stub.
    pub call_site: Option<CallSite>,
    /// Served sides only: join keys the handler's shard reaches, with paths.
    pub reaches: Vec<ReachRow>,
    /// The first-party and generated nodes of the root's shard, for drill-down.
    pub shard_nodes: Vec<NodeCard>,
    /// The shard's depth limit; calls beyond it are not in the shard.
    pub depth_limit: Option<u32>,
    /// Nodes the shard cut at its depth limit.
    pub frontier_count: usize,
}

/// A served handler reaching a consumed operation of its own repository.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ReachRow {
    /// The consumed operation's key, which continues into the next repository.
    pub join_key: String,
    /// The call path inside the shard, handler first, the consumed root's node last.
    pub path: Vec<NodeCard>,
}

/// One node, as a box in the overview.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct NodeCard {
    /// The node's opaque raw id.
    pub raw: String,
    /// The symbol's name, or the raw id when the run had no symbol for it.
    pub name: String,
    /// The plugin's own kind spelling.
    pub raw_kind: Option<String>,
    /// The file, as the run rendered it.
    pub file: Option<String>,
    /// The doc comment, verbatim. `None` means the source has none.
    pub doc: Option<String>,
    /// The node's category.
    pub category: Option<CategoryRow>,
    /// Depth from the shard's root.
    pub depth: Option<u32>,
}

/// Join every repository's roots on `join_key`.
pub fn merge(repos: &[RepoArtifact]) -> EstateDocument {
    let mut by_key: BTreeMap<String, (Vec<SideRow>, Vec<SideRow>)> = BTreeMap::new();

    for repo in repos {
        // This repository's consumed roots, by bound node: what a served
        // shard in the same repository can reach.
        let consumed_at: BTreeMap<&NodeRef, &str> = repo
            .endpoints
            .operations
            .iter()
            .filter(|op| op.direction == DirectionRow::Consumed)
            .flat_map(|op| op.versions.iter())
            .filter_map(|version| match &version.binding {
                BindingRow::Bound { node } => Some((node, version.join_key.as_str())),
                BindingRow::Unbound { .. } => None,
            })
            .collect();

        for operation in &repo.endpoints.operations {
            for version in &operation.versions {
                let shard = version
                    .shard
                    .as_ref()
                    .and_then(|path| repo.shards.get(path));
                let (node, unbound_reason) = match &version.binding {
                    BindingRow::Bound { node } => (Some(card_for(node, shard)), None),
                    BindingRow::Unbound { reason } => (None, Some(reason.clone())),
                };
                let reaches = match (operation.direction, shard, &version.binding) {
                    (DirectionRow::Served, Some(shard), BindingRow::Bound { node: root }) => {
                        reaches_of(shard, root, &consumed_at)
                    }
                    _ => Vec::new(),
                };
                let side = SideRow {
                    repo: repo.label.clone(),
                    contract: operation.contract.clone(),
                    service: operation.service.clone(),
                    operation: operation.operation.clone(),
                    version: version.version.clone(),
                    node,
                    unbound_reason,
                    call_site: match operation.direction {
                        DirectionRow::Consumed => Some(CallSite::NotMeasured),
                        DirectionRow::Served => None,
                    },
                    reaches,
                    shard_nodes: shard.map(drill_down).unwrap_or_default(),
                    depth_limit: shard.and_then(|shard| shard.depth_limit),
                    frontier_count: version.frontier_count,
                };
                let entry = by_key.entry(version.join_key.clone()).or_default();
                match operation.direction {
                    DirectionRow::Served => entry.0.push(side),
                    DirectionRow::Consumed => entry.1.push(side),
                }
            }
        }
    }

    let joins = by_key
        .into_iter()
        .map(|(join_key, (served, consumed))| JoinRow {
            status: status_of(&served, &consumed),
            join_key,
            served,
            consumed,
        })
        .collect();

    EstateDocument {
        schema_version: ESTATE_SCHEMA_VERSION,
        claim: ESTATE_CLAIM.to_owned(),
        repos: repos
            .iter()
            .map(|repo| EstateRepoRow {
                label: repo.label.clone(),
                roots_total: repo.endpoints.coverage.roots_total,
                roots_bound: repo.endpoints.coverage.roots_bound,
                partial: repo.endpoints.coverage.partial,
                notes: repo.endpoints.coverage.notes.clone(),
            })
            .collect(),
        joins,
    }
}

fn status_of(served: &[SideRow], consumed: &[SideRow]) -> JoinStatus {
    let any_bound_consumer = consumed.iter().any(|side| side.node.is_some());
    match (served.len(), consumed.is_empty(), any_bound_consumer) {
        (0, _, _) => JoinStatus::ConsumedNotServed,
        (2.., _, _) => JoinStatus::AmbiguousServed,
        (1, true, _) => JoinStatus::ServedNotConsumed,
        (1, false, false) => JoinStatus::ConsumerUnbound,
        (1, false, true) => JoinStatus::Joined,
    }
}

/// The card for `node`, from the shard's own row when the shard holds it.
fn card_for(node: &NodeRef, shard: Option<&ShardDocument>) -> NodeCard {
    let row = shard.and_then(|shard| shard.nodes.iter().find(|row| &row.id == node));
    match row {
        Some(row) => card_of_row(row),
        None => NodeCard {
            raw: node.raw.clone(),
            name: node.raw.clone(),
            raw_kind: None,
            file: None,
            doc: None,
            category: None,
            depth: None,
        },
    }
}

fn card_of_row(row: &crate::schema::NodeRow) -> NodeCard {
    let symbol = row.symbol.as_ref();
    NodeCard {
        raw: row.id.raw.clone(),
        name: symbol.map_or_else(|| row.id.raw.clone(), |symbol| symbol.name.clone()),
        raw_kind: symbol.map(|symbol| symbol.raw_kind.clone()),
        file: symbol.map(|symbol| symbol.range.file.to_string_lossy().into_owned()),
        doc: symbol.and_then(|symbol| symbol.doc.clone()),
        category: row.category,
        depth: row.depth,
    }
}

/// The nodes worth a box: those with a symbol, which excludes the external
/// targets a run located and could not describe.
fn drill_down(shard: &ShardDocument) -> Vec<NodeCard> {
    let mut cards: Vec<NodeCard> = shard
        .nodes
        .iter()
        .filter(|row| row.symbol.is_some())
        .map(card_of_row)
        .collect();
    cards.sort_by(|a, b| (a.depth, &a.name).cmp(&(b.depth, &b.name)));
    cards
}

/// Every consumed operation of this repository the shard reaches, with the
/// shortest recorded call path to it.
fn reaches_of(
    shard: &ShardDocument,
    root: &NodeRef,
    consumed_at: &BTreeMap<&NodeRef, &str>,
) -> Vec<ReachRow> {
    let mut outgoing: BTreeMap<&NodeRef, Vec<&NodeRef>> = BTreeMap::new();
    for edge in &shard.edges {
        if let EdgeTargetRow::Resolved { node } = &edge.to {
            outgoing.entry(&edge.from).or_default().push(node);
        }
    }

    // Breadth-first from the handler, so each path is a shortest one.
    let mut parent: BTreeMap<&NodeRef, &NodeRef> = BTreeMap::new();
    let mut seen: BTreeSet<&NodeRef> = BTreeSet::from([root]);
    let mut queue = VecDeque::from([root]);
    while let Some(at) = queue.pop_front() {
        for &next in outgoing.get(at).map(Vec::as_slice).unwrap_or_default() {
            if seen.insert(next) {
                parent.insert(next, at);
                queue.push_back(next);
            }
        }
    }

    let mut reaches: Vec<ReachRow> = shard
        .nodes
        .iter()
        .filter(|row| &row.id != root)
        .filter_map(|row| {
            let key = consumed_at.get(&row.id)?;
            let mut path = vec![&row.id];
            let mut at = &row.id;
            while let Some(&up) = parent.get(at) {
                path.push(up);
                at = up;
            }
            path.reverse();
            Some(ReachRow {
                join_key: (*key).to_owned(),
                path: path
                    .into_iter()
                    .map(|id| card_for(id, Some(shard)))
                    .collect(),
            })
        })
        .collect();
    reaches.sort_by(|a, b| a.join_key.cmp(&b.join_key));
    reaches
}
