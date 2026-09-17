# ADR-0007: Endpoint version is first-class in root identity

**Status:** Accepted
**Date:** 2026-09-17

## Context

Real services run several versions of an API at once. `v1` and `v2` of the same operation
commonly route to **different code** — a rewritten handler, a compatibility shim
delegating to the new path, or two independent implementations maintained in parallel
during a migration.

If the tool treats version as incidental decoration on an operation name, it merges those
two root sets. Every question the tool exists to answer then returns a wrong answer:
reachability unions two code paths, the unreachable set is computed against a fictional
root, and "what does this endpoint touch" describes something that does not exist.

### A correction to record explicitly

Earlier in the design of the contract join, the cross-repository join key was stated as
the **bare RPC name** — for example `CreateTask`. That is wrong, and it is recorded as a
correction rather than silently revised.

The join key must be the **fully-qualified operation name, including the version-bearing
package**:

```
yadgar.task.v1.TaskService/CreateTask
```

MEASURED support, from the design document's own probe output (`docs/design.md` §4): the
tonic-generated client stub resolves to
`target/debug/build/yadgar-task-.../out/yadgar.task.v1.rs:272`. The proto package
`yadgar.task.v1` already carries the version, mechanically, in the generated path. gRPC
supplies the version for free; the earlier framing simply discarded it.

The fully-qualified name remains language-neutral, so ADR-0003's position — that
cross-repository identity needs no canonical symbol scheme, because the join key is the
contract operation — is unchanged and still correct. Only the key's spelling changes.

## Decision

**A version is part of root identity. `v1` and `v2` of an operation are separate roots,
always, even when the operation name is identical.**

### Root identity

```
(contract_id, version, service, operation, direction)
```

`version` is a **first-class field supplied by the roots plugin** (ADR-0003, field 6).

**The core must never parse a version out of a route string.** Extracting a version is
per-contract and per-framework knowledge, which is exactly what belongs in a plugin and
never in the waist.

### gRPC and REST are not the same problem

For gRPC the version is structural: it lives in the proto package and is therefore always
present and always unambiguous.

For REST and OpenAPI it is not. The version may appear in:

- a path prefix — `/api/v2/tasks`
- an `Accept` header — `application/vnd.acme.v2+json`
- a query parameter — `?version=2`
- only in the document's `info.version`, with nothing in the route at all

That is per-framework extraction. The roots-plugin contract therefore carries:

```rust
version: Option<String>
```

**A missing version is `None`. It is never defaulted to `"v1"`.** Inventing a version
where the contract does not state one manufactures a distinction the code does not have,
and — worse — makes two genuinely unversioned APIs look like the same version of one API.
`None` is a true statement about the contract. `"v1"` is a guess.

### Reachability is computed per version

Shared code gets a three-way classification:

| class | meaning |
|---|---|
| reachable from `v1` only | dies when `v1` is sunset |
| reachable from `v2` only | new path |
| reachable from both | shared; survives the sunset |

**"What dies when we sunset v1" is a first-class output.** It is arguably a better pitch
than generic dead-code detection: it is a question teams actually ask, on a schedule, with
a deadline attached, and it is one the tool can answer precisely because it is rooted at
endpoints rather than at symbols.

### The partial-index correctness problem

This is a correctness issue, not a wording preference.

If the index covers only `v2` roots — because the `v1` contract file was not passed, or a
plugin failed, or a directory was excluded — then **every `v1`-only function appears
unreachable**. That is precisely the false-positive class `docs/design.md` §8 calls the
most dangerous claim the tool can make: telling someone to delete working code is the one
failure that permanently destroys trust.

Binding requirements:

1. **The index records which contracts and which versions it covered.** This is part of
   the artifact, not a log line.
2. The §8 wording rule extends. Report:

   > **not reachable from any endpoint version in this index**

   Never "dead". The extended wording stays true when the version coverage is partial,
   and it still points the reader at the right places.
3. `unreachable.json` (ADR-0006) carries the covered-root set, so any consumer can see
   what the claim was computed against.

### The renderer must never silently union versions

A view that merges `v1` and `v2` into one graph without saying so reproduces the original
error in the user interface after the data layer got it right.

Grouping by operation with an explicit version toggle is fine, and is probably the right
default — the versions of one operation are genuinely related. Silently unioning them is
not.

## Consequences

- Composes with ADR-0006 at no cost: a versioned root is still one root, so `v1` and `v2`
  are separate shards under `graph/<root>.json`. No extra machinery.
- `endpoints.json` gains version as a grouping dimension. INFERRED: the endpoint list
  should group by operation and show versions as siblings, rather than presenting a flat
  list where `CreateTask` appears twice with nothing distinguishing the entries.
- The roots plugin contract gains a required `version: Option<String>` field before the
  first plugin is written. Retrofitting it later means every plugin already written
  produces roots with no version, which cannot be recovered after the fact.
- INFERRED: deprecation and sunset analysis is a natural second output of the same data,
  and needs no additional graph.

## Rejected alternatives

**Version as a display attribute on a single root.** Rejected: it merges two root sets
that route to different code, which corrupts reachability at the source rather than in the
presentation.

**Defaulting a missing version to `"v1"`.** Rejected: it fabricates a distinction the
contract does not make, and collapses genuinely unversioned APIs together.

**Parsing the version out of the route in the core.** Rejected: it is per-framework
knowledge. It belongs in the roots plugin, per ADR-0003's rule that nothing
framework-shaped reaches the waist.
