# Plan 04 — `reachgraph-roots-proto-tonic`

**Status:** ready to build
**Date:** 2026-09-17
**Depends on:** [plan-00](00-workspace-and-plugin-api.md) (as amended 2026-09-17), plan-01
(waist), plan-02 (fixture plugin); ADR-0003, ADR-0006, ADR-0007, ADR-0008
**Blocks:** plan-06

The crate that turns a `.proto` file into roots. **Every tonic-specific and
protobuf-specific fact in the system lives here and nowhere else** (ADR-0003 field 6,
ADR-0007, ADR-0008). The waist receives `Root` and `Coverage` and learns nothing about
traits, impl blocks, CamelCase, generated modules or `.proto` syntax.

It implements `RootProvider` only:

```rust
fn provides(&self) -> &[Capability] { &[Capability::Roots] }
fn position_encoding(&self) -> PositionEncoding { PositionEncoding::Utf8Bytes }
fn detection(&self) -> Detection {
    Detection { marker_files: &["Cargo.toml"], extensions: &[".proto"] }
}
```

It provides no symbols and no edges; it consumes `SymbolIndex`, which plan-03 fills.

---

## 1. The measured evidence base

Everything in this plan is grounded in one repository, re-measured 2026-09-17 at
`/home/max/git/yadgarhq/task`. It is **evidence, not a fixture** — no test in this plan may
depend on it (§12).

| #   | measurement                                                                                   | location                                                                                                                                                                                                       |
| --- | --------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| M1  | six RPCs in `taskapi.proto` join onto six handlers, CamelCase→snake_case, 6/6                 | `proto/yadgar/taskapi/v1/taskapi.proto:107-113`; `src/service/handlers.rs:22,84,136,184,284,389`                                                                                                               |
| M2  | the served trait impl                                                                         | `src/service/handlers.rs:21` — `impl TaskService for Task`                                                                                                                                                     |
| M3  | the trait is imported from a generated `*_server` module                                      | `src/service/handlers.rs:17` — `use crate::pb::yadgar::taskapi::v1::task_service_server::TaskService;`                                                                                                         |
| M4  | `task` **consumes** a second contract                                                         | `proto/yadgar/task/v1/task.proto:117-122`, service `TaskDbService`, five RPCs                                                                                                                                  |
| M5  | the consumed client is constructed in first-party source                                      | `src/service.rs:6,54,60` — `TaskDbServiceClient::new(channel)`                                                                                                                                                 |
| M6  | `create_task` exists twice; the second is a test double implementing the **consumed** service | `tests/service.rs:145` — `impl TaskDbService for MockDb`, `:146` — `async fn create_task`                                                                                                                      |
| M7  | **`CreateTask` exists in both contracts**                                                     | `yadgar.taskapi.v1.TaskService/CreateTask` (served) and `yadgar.task.v1.TaskDbService/CreateTask` (consumed)                                                                                                   |
| M8  | the generated stub carries the fully-qualified name as a literal                              | `target/debug/build/yadgar-task-237f97ebf0e011bd/out/yadgar.taskapi.v1.rs:241,507` — `"/yadgar.taskapi.v1.TaskService/CreateTask"`; `:812` — `pub const SERVICE_NAME: &str = "yadgar.taskapi.v1.TaskService";` |
| M9  | build.rs requests **both** halves for **both** contracts                                      | `build.rs` — `tonic_prost_build::configure().build_server(true).build_client(true)`                                                                                                                            |
| M10 | both server and client modules are generated for both protos                                  | `out/yadgar.taskapi.v1.rs:131,373`; `out/yadgar.task.v1.rs:149,362`                                                                                                                                            |

M7 is the sharpest of the ten and §3 is built on it.

---

## 2. Scope

**In:** proto parsing to `(package, service, rpc)`; direction determination; spelling the
fully-qualified join key into `Root::join_key` (§8); handler binding via `container` +
`is_test`; `Root` construction with `version: Option<String>`;
`RootBinding::Unbound { reason }`; `coverage()`; Consumed-direction stubs recorded as
cross-repo join keys.

**Out:** the cross-repo _join itself_ (v0.2 — ADR-0008 scope). OpenAPI, GraphQL, FastAPI,
axum. Any language other than Rust. Any reading of `raw_kind` by anything but this crate.

---

## 3. The join key is the fully-qualified name

ADR-0007 states it; M7 proves it twice over, on evidence the ADR did not have.

MEASURED (M7): `CreateTask` is an RPC name in **two different services in two different
packages** in one repository:

```
yadgar.taskapi.v1.TaskService/CreateTask     served    → src/service/handlers.rs:22
yadgar.task.v1.TaskDbService/CreateTask      consumed  → no handler in src/, by design
```

A bare-name join binds the **consumed** RPC to the **served** handler. That is a phantom
root: reachability is then computed from an endpoint this service does not serve, and the
real `TaskDbService` client leaf is attributed to the wrong contract. This is an
independent proof of the FQN requirement, stronger than the version-bearing-package
argument alone — the collision here is on the _service_, and it would survive any amount
of version handling.

### Spelling

```
<package>.<Service>/<Rpc>
```

MEASURED (M8) the generated code contains this exact string as a literal, twice per RPC
(client call path and server dispatch arm), plus `SERVICE_NAME` for the service half. The
spelling is not this project's invention; it is gRPC's wire path.

When a `.proto` declares no `package`, the key is `<Service>/<Rpc>`. It is **not**
prefixed with a synthesised package, for the same reason a missing version is not
defaulted (§5).

### A correction to ADR-0007's illustrative string

ADR-0007 writes the example as `yadgar.task.v1.TaskService/CreateTask`. MEASURED (M7, M8):
that string splices the **consumed** package `yadgar.task.v1` onto the **served** service
name `TaskService`; neither contract contains it. The served key is
`yadgar.taskapi.v1.TaskService/CreateTask`. The ADR's _decision_ — the join key is the
fully-qualified operation name, never the bare RPC name — is untouched and is what this
plan implements. The example string was drawn from the generated client stub path
(`yadgar.task.v1.rs:272`), which is the consumed side. ADR files are not edited; this is
where the corrected spelling lives.

---

## 4. Proto parsing

### Candidate crates — MEASURED 2026-09-17, crates.io API

| crate            | version | SPDX              | shape                                                                             | needs a `protoc` binary?                                                                          |
| ---------------- | ------- | ----------------- | --------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| `protox-parse`   | 0.9.0   | MIT OR Apache-2.0 | parses **one** `.proto` file → `FileDescriptorProto`                              | **no** — MEASURED: its own lexer (`logos`), and its doc text states it never reads imported files |
| `protox`         | 0.9.1   | MIT OR Apache-2.0 | full pure-Rust protobuf **compiler**; resolves imports → `FileDescriptorSet`      | no                                                                                                |
| `protobuf-parse` | 3.7.2   | MIT               | parses `.proto`; offers **either** a pure-Rust parser **or** shelling to `protoc` | only in protoc mode                                                                               |
| `prost-reflect`  | 0.16.5  | MIT OR Apache-2.0 | reflection over an **existing** `FileDescriptorSet`                               | n/a — pairs with a parser, does not replace one                                                   |

All four are licence-compatible with MIT OR Apache-2.0 (ADR-0001). The discriminating
column is the last one: ADR-0001 forbids external binaries, so `protobuf-parse` qualifies
only in its pure-Rust mode, and a crate whose default path is protoc is the wrong default
to adopt.

### Choice: `protox-parse`

**Roots need only `package`, `service`, `method`.** Message types, field numbers and
import closure are all irrelevant to root identity, so import resolution is not required —
which means the include-path problem does not arise at all. MEASURED, the very problem
avoided: `/home/max/git/yadgarhq/task/build.rs` spends 20 lines and a `PROTOC_INCLUDE`
environment variable locating `google/protobuf/*.proto` for protoc. A roots plugin that
never resolves imports never needs any of it.

MEASURED 2026-09-17, docs.rs `protox_parse::parse` — signature, behaviour and the import
question, all settled:

```rust
pub fn parse(name: &str, source: &str) -> Result<FileDescriptorProto, ParseError>
```

> "This function only looks at the syntax of the file, without resolving type names or
> reading imported files."

It returns a `FileDescriptorProto` — protobuf's own descriptor schema, in which `service`
and `method` are structural fields — rather than a bespoke AST. So `(package, service,
rpc)` extraction is field access, not parsing.

**MEASURED: it does not reject a file whose imports cannot be resolved.** Imports are
recorded as dependency _names_ on the descriptor (the crate's own example shows
`import "dep.proto";` surfacing as `dependency: vec!["dep.proto".to_owned()]`) and are
never followed. That is the behaviour this crate needs and the reason the choice holds:
a roots plugin can read every service declaration in a repository without a single include
path, without `protoc`, and without the file's dependencies being present at all.

**The cost that is now avoided rather than deferred.** Had `parse` rejected unresolved
imports, the choice would have inverted to `protox`, and with it the whole include-path
problem: `google/protobuf/*.proto` must then be located, which is exactly the 20 lines and
`PROTOC_INCLUDE` environment variable MEASURED in `/home/max/git/yadgarhq/task/build.rs`
— a search whose right answer depends on how the user installed protoc (`/usr/include` on
Debian; self-contained on nix or Homebrew). Under ADR-0001 that is a prerequisite the tool
would have to acquire or guess at, and design.md §9 Q5's zero-configuration bar would be
gone. None of it is needed.

Escalate to `protox` only if a real repository is found where a service declaration cannot
be read without resolving imports. INFERRED that none exists: `service` and `rpc`
declarations are syntactically local to their file, which is what the measured doc text
above asserts.

MEASURED consequence of the choice: it pulls `prost-types` 0.14 into the tree.

### Discovery

Walk `repo_root` for `**/*.proto`, excluding `target/` and any VCS directory. MEASURED at
`task`: three files under `proto/`, vendored from an upstream contract repository at the
tag in `PROTO_VERSION` (`v1.11.2`).

**The vendored contract tag is not the endpoint version.** `v1.11.2` is the version of the
_bundle of proto files_; `yadgar.taskapi.v1` is the version of the _operation_. ADR-0007's
`version` field means the second. Conflating them would put a bundle release number on a
root and produce exactly the fictional distinction ADR-0007 rejects.

---

## 5. Version extraction — ADR-0007

Per-contract knowledge, which is why it lives here and never in the waist.

For gRPC the version is **structural**: it is a segment of the proto package. Rule:

```
version = last dot-segment of `package` matching  ^v[0-9]+([a-z]+[0-9]+)?$
```

MEASURED: `yadgar.taskapi.v1` → `Some("v1")`; `yadgar.task.v1` → `Some("v1")`.
Also matched: `v2`, `v1beta1`, `v1alpha2` — the Google API versioning convention.
Not matched: `v`, `version1`, `api`, `vnext`.

**No match, or no `package` at all, yields `None`.** Never `"v1"`. ADR-0007 is explicit and
the reason is worth restating: `"v1"` fabricates a distinction the contract does not make,
and makes two genuinely unversioned APIs look like the same version of one API. `None` is
a true statement about the contract.

`ContractId` is the **repo-relative path of the `.proto` file**, e.g.
`proto/yadgar/taskapi/v1/taskapi.proto` — not the package. Two files may share a package,
`Coverage` must name what was _examined_, and a file path is what a user passes, excludes
or forgets. `Coverage::versions` then carries `(ContractId, Option<String>)` per file
(§10).

---

## 6. Direction — where a direction-blind join produces phantom roots

MEASURED (M4): `task` **serves** `taskapi.proto` and **consumes** `task.proto`. The five
`TaskDbService` RPCs correctly join onto nothing in `task/src/`.

### Two signals that are MEASURED dead

Recorded because both are the obvious first guess:

1. **build.rs flags do not discriminate.** M9: `.build_server(true).build_client(true)` —
   one call, applied to every proto compiled by that build script. The repository asks for
   both halves of both contracts.
2. **Generated-module presence does not discriminate.** M10: `task_service_client` **and**
   `task_service_server` are generated for `taskapi.proto`; `task_db_service_client` **and**
   `task_db_service_server` are generated for `task.proto`. Presence of a `*_server` module
   proves only that `build_server(true)` was set.

### The live rule: hand-written first-party usage

Direction is a fact about **what this repository's own source does**, not about what its
build script generated.

For each `(package, service)`:

```
served   ⟸ ∃ symbol S : S.raw_kind == "impl <Service> for <T>"
                         ∧ !S.is_test
                         ∧ S.range.file is not under a `target/` component

consumed ⟸ ¬served, corroborated by a reference to `<Service>Client`
                     in first-party non-test source
```

Each clause earns its place:

- **`impl <Service> for <T>`** — MEASURED (M2). The pattern is naturally specific: the
  generated server module contains `impl<T> Service<…> for <Service>Server<T>`, not
  `impl <Service> for …`, so generated code does not match it. The `target/` guard is
  belt-and-braces and is the one Cargo-shaped fact this crate holds; it is acceptable here
  for the same reason the tonic knowledge is (this is the tonic-and-Cargo plugin), and it
  is stated rather than assumed.
- **`!is_test`** — and this is the sharp one. MEASURED (M6): `tests/service.rs:145` is
  `impl TaskDbService for MockDb`. Without the `is_test` filter, the **consumed** service
  `TaskDbService` acquires a served signal from a test double, flips to `Served`, and
  produces five phantom roots pointing at a mock. **`is_test` is load-bearing for
  direction, not only for handler disambiguation.** It is the difference between the
  measured-correct answer and five fabricated endpoints.
- **The `<Service>Client` corroboration** — MEASURED (M5): `src/service.rs:6,54,60`. It
  distinguishes _consumed_ from _declared but never used_.

### Reading the client reference, and its limitation

`SymbolIndex` exposes `by_name`, `in_file` and `get` (plan-00 §3.4). A typed field
`db: TaskDbServiceClient<Channel>` is a `Field` symbol named `db`; **the type text is not
in `Symbol`.** So `SymbolIndex` alone cannot see M5.

v0.1 mechanism: this crate reads first-party `.rs` files under `repo_root` (excluding
`target/` and test targets) and looks for the identifier `<Service>Client`. That is a text
scan, and it is worth being precise about what it is and is not. It is not resolution — it
decides nothing about which symbol anything refers to, has no candidate set and cannot
bind a root. Its only output is the `reason` string on an already-`Unbound` consumed root.
That is the distinction ADR-0005 draws between asking a parser a syntactic question and
asking it a semantic one.

§13 open question 3 records the cleaner alternative.

### When neither signal fires

A service with no first-party impl and no client reference is emitted as
`direction: Consumed` with `RootBinding::Unbound { reason }` naming the absence.

The asymmetry is deliberate and is the safety argument for the default: a wrongly-`Served`
service produces **phantom roots**, which corrupt reachability and can make real code read
as reachable that is not. A wrongly-`Consumed` service produces **reported gaps**, which
are visible in the artifact and correct themselves under inspection. When the evidence is
absent, take the failure mode that shows up in the output.

---

## 7. Handler binding

For each `(package, service, rpc)` with `direction == Served`:

```rust
// 1. tonic's method-name rule. MEASURED (M1): 6/6 on taskapi.proto.
let expected = camel_to_snake(rpc);          // "CreateTask" -> "create_task"

// 2. candidates by name — plan-00 §3.4: `by_name` returns a Vec precisely because
//    design.md §4 MEASURED that this name collides.
let candidates = symbols.by_name(&expected);

// 3. filter, never on the name alone
let bound: Vec<&Symbol> = candidates.iter()
    .filter(|s| matches!(s.kind, SymbolKind::Method | SymbolKind::Function))
    .filter(|s| !s.is_test)
    .filter(|s| match &s.container {
        Some(c) => symbols.get(c)
            .map(|c| impl_header_names_trait(&c.raw_kind, service))
            .unwrap_or(false),
        None => false,
    })
    .collect();

// 4. exactly one, or nothing
match bound.as_slice() {
    [one] => RootBinding::Bound(one.id.clone()),
    []    => RootBinding::Unbound { reason: /* §9 */ },
    many  => RootBinding::Unbound { reason: format!(
        "ambiguous: {} non-test candidates named `{}` in matching impls", many.len(), expected) },
}
```

### `impl_header_names_trait` — the string contract with plan-03

plan-03 §8 specifies the exact grammar of an impl `Symbol`'s `raw_kind`:

```
impl <Trait> for <SelfTy>      // trait impl
impl <SelfTy>                  // inherent impl
```

with `<Trait>` the trait's **declared** name, not the path it was imported by. MEASURED
(M3): the import is
`crate::pb::yadgar::taskapi::v1::task_service_server::TaskService`, and the declared trait
name at `out/yadgar.taskapi.v1.rs:384` is `TaskService` — **equal to the proto service
name**. That equality is what lets this crate compare against the service name directly
and never resolve a Rust import.

Parse it **anchored**, never by substring search:

1. require the literal prefix `impl `;
2. split once on `for`; no match → inherent impl → not a trait impl → reject;
3. take the left side, strip generic arguments, take the segment after the last `::`;
4. compare to the proto service name, exactly.

A substring search for `TaskService` would match `impl TaskServiceExt for …` and
`impl Foo for TaskServiceClient<T>`. Both are real shapes.

### Two or more survivors are never resolved by preference

No "prefer `src/` over `tests/`", no "prefer the first", no score. This is the
`confidence: f32` decision (plan-00 §8 open question 5) applied one layer down: an
ambiguous binding is a reported gap. MEASURED, design.md §5, the failure mode being
avoided: code_graph emits 59 `CALLS` edges at confidence 0.55, each with two or three
candidate targets — a number that records indecision and renders as though it recorded a
measurement.

### `camel_to_snake`

`CreateTask` → `create_task`; `ListTasks` → `list_tasks`; `TransitionTask` →
`transition_task`. All six MEASURED cases (M1) are simple. The unmeasured cases are
acronyms — `GetTaskByID`, `ExportCSV`, `HTTPProxy` — where the rule must match what
`tonic-prost-build` actually emits (it uses `heck`'s snake-casing), not what looks
reasonable.

Do not guess. The generated server trait **contains the emitted method names verbatim**
(MEASURED, `out/yadgar.taskapi.v1.rs:384`), so it is an oracle: §12's
`snake_case_matches_generated_trait` test compares the computed name against the generated
trait for a fixture containing acronym RPCs, and a disagreement is a failing test rather
than a silently unbound root.

Using the generated trait as the _runtime_ source of truth is tempting and is deferred —
it would make binding depend on the workspace having been built (plan-03 §11 check 2),
trading a small correctness gain for a large new prerequisite. §13 open question 4.

---

## 8. `Root` construction

```rust
Root {
    contract:  ContractId("proto/yadgar/taskapi/v1/taskapi.proto".into()),
    version:   Some("v1".into()),          // §5 — from the package. None is never "v1".
    service:   "TaskService".into(),       // the bare service name, for display
    operation: "CreateTask".into(),        // the bare RPC name, for display
    direction: Direction::Served,
    // plan-00 §2, amended: plugin-spelled, core-opaque. THIS is the join key.
    join_key:  "yadgar.taskapi.v1.TaskService/CreateTask".into(),
    binding:   RootBinding::Bound(node_id_of_create_task),
}
```

### `join_key` is where the fully-qualified name lives

`Root::join_key` is spelled by this plugin and is **opaque to the core** — the same rule as
`NodeId::raw` (ADR-0003 field 3). The core compares join keys, groups by them and carries
them into the artifact; it never parses one, never splits on `/` or `.`, and never extracts
a version, a package or a service from one. ADR-0007 is explicit that parsing a version out
of a route string is per-framework knowledge that must not reach the waist; `join_key`
being opaque is what makes that rule enforceable rather than aspirational.

This replaces the earlier reconstruct-it-from-parts scheme, and the replacement is a
straightforward improvement: reconstruction required the core (or every consumer) to know
that a gRPC key is spelled `<package>.<Service>/<Rpc>`, which is exactly the
protobuf-shaped knowledge ADR-0003 keeps out of the waist. A plugin-spelled opaque string
moves that knowledge back where it belongs. §12 asserts the spelling here, in this crate's
own tests, which is the only place that can legitimately assert it.

`service` and `operation` stay **bare** and are display fields. Putting the FQN in
`operation` instead would render a protobuf-shaped string wherever an operation name is
shown, and would make the REST case (ADR-0007: version in a path prefix, a header, a query
parameter, or nowhere) render inconsistently against the gRPC case. `version` and
`contract` continue to carry the structured facts the waist _does_ act on.

MEASURED, and it is why the spelling needs no invention: the generated code contains this
exact string as a literal (M8) — `"/yadgar.taskapi.v1.TaskService/CreateTask"`, modulo the
leading slash of the HTTP/2 path. `join_key` is stored **without** the leading slash;
the slash is gRPC's path syntax, not part of the operation's identity.

`Root` carries no `node` field and no `confidence`. plan-00 §2, amended 2026-09-17:
`binding: RootBinding` subsumes both, because a separate `node` is incoherent for an
unbound root. This plan consumes that contract as written; it proposes no change to it.

---

## 9. `RootBinding::Unbound` — the reasons, enumerated

An unbound root is **carried into the artifact**, never dropped. ADR-0007's partial-index
problem is why: a dropped root is indistinguishable from a root that was never looked for,
and both make live code read as unreachable.

| case                                                 | `reason`                                                                                                                                    |
| ---------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| consumed RPC, **the expected case**                  | `"consumed: no handler expected in this repository; join key `yadgar.task.v1.TaskDbService/CreateTask` resolves in the serving repository"` |
| consumed RPC, generated client stub absent           | `"consumed: generated client stub not found under target/; build the workspace once to make the cross-repo leaf visible"`                   |
| served, no candidate of that name                    | `"served: no non-test method named `create_task`in any`impl TaskService for …`"`                                                            |
| served, candidates exist but none in a matching impl | `"served: 2 methods named `create_task`, none inside an `impl TaskService for …`"`                                                          |
| served, ambiguous                                    | `"ambiguous: 2 non-test candidates named `create_task` in matching impls"`                                                                  |
| service with no direction evidence                   | `"no first-party impl and no client reference for service `X`; direction assumed consumed"`                                                 |

**Row 1 is a pass, not a failure.** MEASURED (M4): the five `TaskDbService` RPCs join onto
nothing in `task/src/`, and that is the correct answer. A test asserts it as a success
case (§12), because a suite that only ever asserts successful binding will eventually be
"fixed" by making consumed RPCs bind to something.

---

## 10. `coverage()` — a correctness requirement, not a log line

```rust
fn coverage(&self) -> Coverage
```

ADR-0007 binding requirement 1: the index records which contracts and which versions it
covered, because a partial root set makes live code appear unreachable — design.md §8's
most dangerous claim.

Contents, for the measured repository:

```rust
Coverage {
    contracts: vec![
        ContractId("proto/yadgar/common/v1/common.proto"),
        ContractId("proto/yadgar/task/v1/task.proto"),
        ContractId("proto/yadgar/taskapi/v1/taskapi.proto"),
    ],
    versions: vec![
        (ContractId("proto/yadgar/common/v1/common.proto"),  Some("v1")),
        (ContractId("proto/yadgar/task/v1/task.proto"),      Some("v1")),
        (ContractId("proto/yadgar/taskapi/v1/taskapi.proto"), Some("v1")),
    ],
}
```

`common.proto` declares no service and contributes no root. **It is still listed**, because
coverage answers "what did you look at", not "what produced output". A reader must be able
to tell a contract with no services from a contract that was never opened.

### A `.proto` that fails to parse is a hard error

**Amended 2026-09-19 by ADR-0743. This section is kept because its argument is still
load-bearing; only its conclusion changed.**

As written:

> `Coverage` has no slot for "found but unreadable", and inventing one by omission is exactly
> the partial-index bug. So: a discovered `.proto` that `protox-parse` rejects makes
> `roots()` return `Err(PluginError)` and fails the run.
>
> This is a deliberate choice of loud over lenient. The lenient alternative — skip the file,
> carry on — produces an index that looks complete, silently omits a contract's roots, and
> reports every function behind them as not reachable from any endpoint. ADR-0007 calls that
> class of false positive the one that permanently destroys trust. Refusing to produce an
> artifact is recoverable; producing a confidently wrong one is not.

Both premises hold. Omission **by silence** is the partial-index bug, and an omitted
contract does make live code read as unreachable. What the section missed is that those
premises do not choose between two options but between three, and it rejected the worst
one rather than the alternative to its own:

1. skip the file silently — the bug, rejected then and rejected now;
2. fail the run — what was chosen;
3. **record the file and carry on** — what ADR-0743 chose instead.

MEASURED 2026-09-19, release.yaml run 35459855283: option 2 is what made both native-runner
smoke tests exit 1 against reachgraph's own repository, over the truncated fixture at
`tests/fixtures/broken/proto/broken.proto`. The tool could not analyse itself, and no
repository holding a partial or vendored-sample contract got anything at all. "Refusing to
produce an artifact is recoverable" is true only for the person who can fix the contract;
for everybody else the artifact simply does not exist.

The slot `Coverage` did not have is now there: `Coverage::unexamined_contracts`, one entry
per file found and not read, carrying the parser's own message. It raises
`IndexCoverage::partial`, which puts a non-dismissible banner on the page and names the file
in `rg-unexamined`. So the index states the shortfall instead of hiding it, and the user
keeps the roots of every contract that did read.

**The boundary.** Per file: recorded. Per tree: fatal. A discovered file that cannot be read
— bytes that are not UTF-8, or syntax the parser rejects — is recorded, because the
repository was still enumerated and the claim stays exact. `discover` failing stays
`PluginError::Io`, because a directory that cannot be walked yields no file to name and no
count to state; recording "something under here, unknown" would be an assertion about a tree
nobody enumerated.

---

## 11. Consumed-direction stubs as cross-repo join keys

v0.2 performs the join (ADR-0008 scope). **The data must be produced now**, because a root
set emitted without consumed-direction entries cannot have them recovered later — the same
argument as ADR-0003's six fields.

What v0.1 emits, per consumed RPC:

- a `Root` with `direction: Consumed`, its own `version`, `contract`, `service` and
  `operation`, and its `join_key` spelled `yadgar.task.v1.TaskDbService/CreateTask`;
- a `binding` that points at the **generated client method** where the build artifacts
  exist, and `Unbound { reason }` where they do not.

### Binding a consumed root to the generated client leaf

Note carefully what this is and is not. M4's "joins onto nothing in `task/src/`" is about
**first-party source**. The generated client method in `target/` is a different thing: it
is the cross-repo **leaf**, and MEASURED, design.md §4, it is exactly where
`outgoingCalls` from the real handler terminated:

```
create_task   pub async fn create_task(&mut self, request: impl tonic::IntoRequest<…>)
              target/debug/build/yadgar-task-373aa483270a13fe/out/yadgar.task.v1.rs:272
```

Binding the consumed root there attaches the join key to a **real node in the graph**, so
the v0.2 join becomes an edge between two existing nodes rather than a synthesised one.

Rule, and it reuses §7's machinery rather than adding any:

```
candidates = by_name(camel_to_snake(rpc))
filter: container's raw_kind matches `impl <Service>Client<…>`   (inherent impl)
        ∧ the symbol's file is under a `target/**/out/` path
exactly one → Bound; zero → Unbound (row 2 of §9); many → Unbound (ambiguous)
```

plan-03 §10 classifies that file `Generated`, which is what lets a renderer draw the leaf
differently from first-party code without this crate saying anything about rendering.

**This binding depends on the repository having been built, and that is now decided rather
than open.** plan-03 §4 D-B: **reachgraph never runs a build.** So on an unbuilt
repository there is no generated client stub, and every consumed root takes row 2 of §9 —
`Unbound` with a reason naming the missing artifact and the one command that fixes it.

That is the designed degradation, not a defect: the gap is reported in the root set, and
plan-03 §11 records the same fact structurally in the run record
(`out_dir_loaded: false`, `out_dir_mechanism: none` for the member concerned). A consumer can therefore distinguish
"this repository consumes an RPC whose client stub was not indexed" from "this repository
consumes nothing" — which a silently shorter root list could not express.

---

## 12. Tests

Test-driven, standing project rule: failing test first, red → green → refactor.

### The test double, and why it is not plan-03

Plan-04's tests use a **hand-built `SymbolIndex`**, not `reachgraph-lang-rust`. Two reasons,
and the second is the important one:

1. It keeps this crate off plan-03's slow Tier-B path (plan-03 §13), which needs a loaded
   `ra_ap` workspace and — since plan-03 §4 D-B forbids reachgraph running a build — a
   fixture workspace that some other step already built. Plan-04's tests are milliseconds
   and need none of that.
2. **It makes `SymbolIndex` have two independent consumers.** That is ADR-0008's fixture-
   plugin mechanism applied to a smaller interface: if `SymbolIndex` or `Symbol` grows a
   field only a real engine can produce, `FakeIndex` stops compiling. A leak becomes a
   compile error rather than a month-nine discovery.

```rust
struct FakeIndex { symbols: Vec<Symbol> }
impl SymbolIndex for FakeIndex {
    fn by_name(&self, n: &str) -> Vec<&Symbol> { … }
    fn in_file(&self, p: &Path) -> Vec<&Symbol> { … }
    fn get(&self, id: &NodeId) -> Option<&Symbol> { … }
}
```

Built from a small builder so each test states only what it cares about.

### Proto fixtures

Small real `.proto` files under `crates/reachgraph-roots-proto-tonic/tests/fixtures/`.
They reproduce the measured shapes of §1 without depending on that repository:

| file            | package         | service    | rpcs                          | role                                                                   |
| --------------- | --------------- | ---------- | ----------------------------- | ---------------------------------------------------------------------- |
| `api.proto`     | `acme.api.v1`   | `Widgets`  | `CreateWidget`, `ListWidgets` | served                                                                 |
| `store.proto`   | `acme.store.v2` | `WidgetDb` | `CreateWidget`, `GetWidget`   | consumed; **`CreateWidget` collides with `api.proto`** (reproduces M7) |
| `legacy.proto`  | `acme.legacy`   | `Old`      | `Ping`                        | no version segment in the package                                      |
| `types.proto`   | `acme.api.v1`   | —          | —                             | messages only, no service; coverage must still list it                 |
| `acronym.proto` | `acme.api.v1`   | `Odd`      | `GetWidgetByID`, `ExportCSV`  | the snake-case oracle test                                             |
| `broken.proto`  | —               | —          | —                             | syntactically invalid                                                  |

### Named tests

**Parsing and keys**

- `parses_package_service_rpc` — all four valid fixtures.
- `fqn_is_the_join_key` — the emitted `Root::join_key` is exactly
  `acme.api.v1.Widgets/CreateWidget`. Asserts it is never the bare `CreateWidget` and never
  carries a leading slash.
- `join_key_matches_the_generated_path_literal` — the golden assertion, MEASURED against M8:
  `join_key` equals the generated dispatch literal with its leading `/` removed. A fixture
  ships both the `.proto` and the string the generator emits for it.
- `no_package_yields_bare_service_key` — a package-less fixture keys as `Svc/Rpc`.
- `join_key_is_never_parsed_by_the_core` — a source-level assertion over
  `reachgraph-core`: no split, slice or regex over `Root::join_key`. The companion to
  plan-00 §6.1's `plugin_api_has_no_ra_ap_dependency`, and the same mechanism — a rule the
  build enforces rather than a rule reviewers remember.

**Direction — the phantom-root guard**

- `bare_name_join_would_produce_phantom_root` — the regression test for M7. A `FakeIndex`
  holding one `create_widget` inside `impl Widgets for Svc`. Asserts the `Widgets`
  operation binds to it **and** that the `WidgetDb` operation of the same RPC name does
  **not**. A name-only implementation passes the first assertion and fails the second.
- `test_double_does_not_make_a_service_served` — the M6 regression. `FakeIndex` contains
  `create_widget` inside `impl WidgetDb for MockDb` with `is_test: true`. Asserts
  `WidgetDb`'s direction stays `Consumed` and that **no** root binds to the mock. Flipping
  the `is_test` flag in the fixture must flip the test to failing — that is what proves the
  flag is load-bearing rather than decorative.
- `generated_server_module_is_not_a_served_signal` — a symbol from a `target/**/out/` path
  with an impl-shaped `raw_kind` does not make a service served (M9, M10).

**Binding**

- `join_covers_every_served_rpc` — 2/2 for `Widgets`, mirroring MEASURED 6/6 (M1).
- `mock_collision_resolved_by_container_and_is_test` — two symbols named `create_widget`;
  one in `impl Widgets for Svc`, one in `impl WidgetDb for MockDb` + `is_test`. The right
  one binds.
- `container_trait_must_match_the_service` — a `create_widget` inside
  `impl Unrelated for Svc` does not bind.
- `impl_header_parsing_is_anchored` — `impl WidgetsExt for Svc` and
  `impl Foo for WidgetsClient<T>` both fail to match service `Widgets`. This is the
  substring-search regression test.
- `ambiguous_candidates_stay_unbound` — two non-test candidates in matching impls yield
  `Unbound`, never a pick, and the reason names the count.
- `camel_to_snake_table` — the six measured names plus acronym cases.

**Versions — ADR-0007**

- `missing_version_stays_none` — `acme.legacy` yields `None`. Asserts
  `!= Some("v1".into())` explicitly, so a future default is caught by name.
- `version_segment_variants` — `v1`, `v2`, `v1beta1`, `v1alpha2` match; `v`, `version1`,
  `api`, `vnext` do not.
- `v1_and_v2_are_separate_roots` — the same service and operation name in `acme.api.v1` and
  a `v2` copy produce two `Root`s with different `version`, never one merged root.
- `vendored_bundle_tag_is_not_the_endpoint_version` — a `PROTO_VERSION`-style file in the
  fixture tree is not read and does not appear in any `version` (§4).

**Unbound and coverage**

- `consumed_rpc_is_unbound_with_reason` — **asserted as a pass**. Every `WidgetDb` RPC is
  `Unbound`, the reason is non-empty and contains the fully-qualified key.
- `consumed_rpc_binds_generated_client_when_present` — `FakeIndex` gains a
  `create_widget` whose container is `impl WidgetDbClient<T>` at a `target/**/out/` path;
  the consumed root becomes `Bound` to it (§11).
- `coverage_lists_every_contract_including_serviceless` — all five valid fixtures,
  `types.proto` among them.
- `coverage_records_version_per_contract` — `Some("v1")`, `Some("v2")`, `None`.
- `unparseable_proto_is_an_error_not_silent_coverage_loss` — `broken.proto` makes `roots()`
  return `Err`. Asserts specifically that it does **not** return `Ok` with four contracts.

**Cross-plugin contract**

- `binds_against_lang_rust_golden_symbols` — loads plan-03's checked-in `fx-impl` symbol
  dump (plan-03 §13, Tier B) as static JSON into `FakeIndex` and runs the binder over it.
  No `ra_ap`, no engine, no slow path — but it fails if plan-03 changes the `raw_kind`
  grammar the binder parses. This is the only mechanism that couples the two plans, and it
  couples them through checked-in data rather than through a build dependency.

  **Two guards, without which this test is self-consistent rather than contractual.**
  First, the golden file is regenerated only by a deliberate, reviewed step — never
  automatically from a plan-03 test run — because a change that rewrites the golden dump
  updates both sides of the contract at once and the test then passes through a break.
  Second, `impl_header_parsing_is_anchored` and the other parser cases above are authored
  from the **grammar written in plan-03 §8**, not copied out of the golden dump. The golden
  file proves the two crates agree today; the hand-written grammar cases are what notice
  when one of them moves.

### Not tested here

Graph construction, reachability, shard emission — plan-01. Whether `container` and
`is_test` are populated correctly from real Rust — plan-03 §13 `fx-impl`. The cross-repo
join itself — v0.2.

---

## 13. Open questions

1. ~~**Does `protox-parse` reject a file with unresolvable imports?**~~
   **CLOSED 2026-09-17 — no.** MEASURED (§4): `parse` "only looks at the syntax of the
   file, without resolving type names or reading imported files", and records imports as
   dependency names. The crate choice holds and the include-path problem never arises.
2. ~~**`protox-parse`'s exact `parse` signature and error type.**~~
   **CLOSED 2026-09-17** — MEASURED
   `pub fn parse(name: &str, source: &str) -> Result<FileDescriptorProto, ParseError>`
   (§4).
3. **`SymbolIndex` cannot see types, so consumed-direction detection is a text scan.** (§6)
   The clean alternative is a `RootProvider` that can ask for _edges_ — the client
   construction at `src/service.rs:60` is a resolved call into the generated client's
   `new`, which plan-03 already emits as an edge. That would make the corroboration a graph
   query instead of a scan. It also widens `RootProvider`'s surface, which plan-00 §3.4
   deliberately kept narrow ("enough to bind a handler, not enough to re-implement the
   graph"). Raise at n=2, when a second roots plugin exists to say whether the need is
   general.
4. **Should the generated server trait be the runtime oracle for method names?** (§7) It is
   authoritative and MEASURED to exist, but using it makes binding depend on the workspace
   having been built. v0.1 computes and tests against it; v0.2 decides.
5. **Two traits with the same declared name in one repository** render identical
   `raw_kind` (plan-03 §8, its open question 6). No mitigation in v0.1. A proto service
   name colliding with an unrelated local trait is the realistic bad case.
6. **`Detection` fits a roots plugin awkwardly.** It was shaped for language plugins
   (marker files plus extensions). This crate declares `Cargo.toml` + `.proto`, which is a
   coarse gate that says "a Rust repository containing protos" rather than "a repository
   whose protos I can read". `coverage()` reports what was actually examined, so nothing is
   unsound — but the registry's detection contract is n=1-shaped here in the way ADR-0002
   warns about. Feedback for plan-00 at n=2; **not** an edit to plan-00 now.
7. **Streaming RPCs** (`client_streaming`, `server_streaming`) are parsed and currently
   ignored. A streaming endpoint is still a root, so no root is lost. This is **not**
   ADR-0003-class and needs no field in `Root`: unlike `container`, the information is
   fully recoverable, because the `.proto` file is the source and `Coverage` names it — a
   later version re-reads the same file and derives the flags again. So the only open
   question is a rendering one: should a streaming endpoint be drawn differently from a
   unary one? Answer it when there is real output to look at, not now.
