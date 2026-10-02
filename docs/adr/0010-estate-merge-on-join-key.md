# ADR-0010: `reachgraph merge` joins per-repository artifacts on `join_key`

**Status:** Accepted
**Date:** 2026-10-02
**Relates to:** ADR-0007 (the join key), ADR-0009 (consumed roots bind), yadgarhq ADR-0842

## Context

ADR-0007 made the fully-qualified operation name the join key, so that a consumed
operation in one repository and the served one in another name the same thing. v0.1
analyses one repository at a time, and plan-06 §8 question 2 kept cross-repository
stitching out of the analyse path. yadgarhq ADR-0842 (2026-10-02) asks for an
estate-level merge that joins per-repository endpoints on `join_key`, consumed to served,
into one overview with drill-down.

## Decision

A separate subcommand reads artifacts that runs already wrote, and analyses nothing:

```
reachgraph merge [label=]<artifact> [label=]<artifact>... [-o <dir>] [--force]
```

Each `<artifact>` is an `endpoints.json`, or the directory that holds one. The merge
reads its shards from beside it. The merge writes two files:

- `estate.json` is the join as data (`reachgraph_core::estate::EstateDocument`).
- `estate.html` is one self-contained page with no script. It holds a repository table
  with each run's own limits, traces, a join-key table and a `<details>` drill-down per
  key.

The join is in `reachgraph-core` (`estate::merge`) and is language-neutral. It compares
join keys and node ids that runs recorded, and nothing else:

- Keys are grouped across repositories. A key with exactly one serving repository and at
  least one **bound** consumer side is `joined`. A key with exactly one serving repository
  whose consumer sides are all unbound is `consumer_unbound`, because the consuming side
  has no node to join.
- A served handler **reaches** a consumed key when that consumed root's bound node is in
  the handler's shard in the same repository. The path to it comes from the shard's
  resolved edges. This is what lets a trace continue from one repository into the next.
- **Unmatched keys are labelled, never dropped**:
  - `consumed_not_served` means not served by any repository in this merge.
  - `served_not_consumed` means not consumed by any repository in this merge.
  - `ambiguous_served` means two or more repositories serve the key. The merge does not
    pick one.
  - `consumer_unbound` means one repository serves the key, and every consumer side is
    unbound.
- **Every consumed side is labelled `call_site: not_measured`**, and on the page it reads
  "client stub exists; call site not measured". It appears on every consumed box, in every
  trace, and in the key table's consumed column. The label comes from how a run decides
  direction: a repository that carries a contract and does not serve it counts as
  consuming every RPC in it. Once bound, the root names a generated client method, and that
  method exists whether or not anything calls it. MEASURED on the yadgarhq estate: gateway
  "consumes" all five `TaskDbService` RPCs and never names `TaskDbServiceClient`, and 7 of
  13 `IamService` RPCs have no non-test caller in gateway. Such traces are labelled, not
  suppressed. A repository that serves no gRPC, such as gateway, cannot show caller
  evidence for any of its consumed roots, so suppression would delete real traces along
  with phantom ones. Recording first-party call sites per consumed root is the fix that
  would separate them. That needs a new per-repository capability and is not in this
  record.
- Every box shows its node's doc comment. If there is none, the box says "no doc comment
  in source". A consumed node without a doc shows the serving handler's doc and names the
  repository it came from.
- Unmeasured parts are labelled: an unbound root shows its reason, and a shard cut at its
  depth limit says how many nodes it cut.

## Consequences

- The merge claims only what its inputs claim. Its scope sentence is "joined on join_key
  across the repositories in this merge only". It is as current as its oldest input.
- Joins on the consumer side need ADR-0009's `--read-build-output`. Without it, consumed
  roots are unbound. A key that another repository in the merge serves is then
  `consumer_unbound`, not `joined`, and a key no repository serves is
  `consumed_not_served`. Either way, each side keeps its unbound reason.
- Two inputs may not share a label. The label keys the trace entries and reach sets, and
  the default label is the directory name, so two `…/out` directories need `label=`.
- A trace starts at a consumed root that no served handler in its own repository
  reaches. For a repository that serves nothing, every consumed root qualifies, so the
  trace starts at the point where that repository's traffic leaves it, not at its own
  entry point. A non-gRPC entry point, such as an HTTP handler, is not a root, so it is
  not shown.
- `served_not_consumed` rarely fires while direction comes from contract presence: every
  repository that carries a contract counts as consuming it. MEASURED: 0 of 49 keys in the
  five-repository demo.

## Rejected alternatives

- **Merge inside the analyse path, several repositories per run.** That would change
  every per-repository artifact and the single-repository shape that plan-06 §8 question 2
  kept. A merge over artifacts composes with runs made at different times and places.
- **Drop unmatched keys from the overview.** Rejected for ADR-0007's reason: a dropped key
  cannot be told apart from one that was never looked for.
