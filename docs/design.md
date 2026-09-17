# Endpoint-rooted call graph — design

Status: draft for review. Written 2026-09-17.
Every claim below is marked **MEASURED** (observed this session, command given in the
appendix) or **INFERRED** (reasoned, not observed). Nothing is asserted without one of
those two labels, because the tool's own premise is that a derived claim must be
traceable to its derivation.

Name: **reachgraph**.

---

## 1. Thesis

> The call graph is per-repo and mechanical. The **seam between repos is derived from the
> interface contract** both sides already compile against.

That second half is the novel part. A language server can follow calls until the process
edge and then stops — it has no idea what is on the other side of a socket. But a gRPC
client stub carries an **RPC name that is written down in a `.proto` file**, and the
service that answers it carries the same name. So crossing the boundary is a *join on a
declared contract*, not a heuristic, not a guess, and not an inference.

Same shape generalises: OpenAPI `operationId`, GraphQL field names, message topic names.
Anywhere two services agree on a schema, the schema **is** the edge.

## 2. What it produces

Four outputs, all off **one graph plus one root set**. That is the economy of the design:
nothing below needs a second analysis.

| output | derivation |
|---|---|
| request flow | reachable set from one endpoint, depth-limited |
| cross-repo links | client-stub leaves joined to served handlers by RPC name |
| unreachable code | complement of the reachable set over all endpoints |
| refactoring signals | per-symbol metrics attached to nodes |

## 3. Architecture

Four layers. The first three are lifted from `crabviz`, whose split I read and think is
correct; the fourth is the new part.

```
  ┌────────────────────────────────────────────────────────┐
  │ 4. ROOTS + REACHABILITY          ← the novel layer     │
  │    contract parse → RPC names → handler symbols        │
  │    reachable(roots), unreachable = all − reachable     │
  ├────────────────────────────────────────────────────────┤
  │ 3. RENDER                                              │
  │    graph JSON → one self-contained HTML file           │
  ├────────────────────────────────────────────────────────┤
  │ 2. GRAPH BUILD                                         │
  │    symbols + edges + docs + metrics → typed graph       │
  ├────────────────────────────────────────────────────────┤
  │ 1. INGEST (per language)                               │
  │    SCIP for symbols + doc comments                     │
  │    LSP callHierarchy for call edges                    │
  │    AST pass for metrics                                │
  └────────────────────────────────────────────────────────┘
```

Layer 1 is the only per-language layer, and it is deliberately thin. **This is the
structural answer to what killed Sourcetrail** — MEASURED: archived 2021-12-13 at 16.5k
stars, after years of maintaining its own per-language C++ indexers against moving Clang
and Qt releases. Renting other people's indexers instead of owning one per language is the
single most important decision in this design.

## 4. The contract join

The mechanism, concretely, for gRPC:

1. Parse the `.proto`. Collect `(service, rpc, direction)` where direction is **served** or
   **consumed** — a repo can do both and the join must know which.
2. For a **served** RPC: tonic generates a trait per service, and the `impl XServer for T`
   block's method names bind 1:1 to RPC names under CamelCase→snake_case. Those methods
   are the **roots**.
3. For a **consumed** RPC: the chain terminates in a generated *client stub*. That stub's
   RPC name is the **join key** to the other repo's served root.
4. Stitch per-repo graphs by matching join keys. No multi-repo index, no `CROSS_*` edge
   type, no shared symbol space required.

**MEASURED**: the CamelCase↔snake_case join covers 6/6 RPCs in `taskapi.proto` onto the
six handlers in `task/src/service/handlers.rs`.

**MEASURED**: `callHierarchy/outgoingCalls` from `handlers.rs::create_task` resolved
through `#[tonic::async_trait]` (a proc macro) *and* a nested `async move` closure, and
returned the client-stub leaf with its full signature:

```
create_task   pub async fn create_task(&mut self, request: impl tonic::IntoRequest<
              super::CreateTaskRequest>) -> Result<tonic::Response<…>, tonic::Status>
              target/debug/build/yadgar-task-373aa483270a13fe/out/yadgar.task.v1.rs:272
```

That single line is the whole cross-repo mechanism working.

### Two traps in the join, both measured

**Name alone collides.** `create_task` exists twice in `task` — the real handler in
`src/service/handlers.rs` and a `MockDb` in `tests/service.rs`. Disambiguate on the
enclosing `impl` block's trait, or on `is_test`. Never on the name.

**Direction matters.** `task` *serves* `taskapi.proto` and *consumes* `task.proto`. The
five `TaskDbService` RPCs join onto nothing in `task/src/` — correctly, because `task` is
that service's client. A join without direction silently produces phantom roots.

## 5. Data sources — what supplies what

| need | source | status |
|---|---|---|
| symbols, ranges, qualified names | SCIP (`rust-analyzer scip`) | **MEASURED** available; the `scip` subcommand exists |
| **doc comments, full text** | SCIP `SymbolInformation.documentation` | INFERRED complete; rust-analyzer passes `ide::Documentation` through unmodified. **Not yet verified on this estate — see §9.** |
| call edges | LSP `callHierarchy/outgoingCalls` | **MEASURED** working, including through proc macros |
| endpoints | contract files (`.proto` today) | new work, small |
| **metrics (complexity, cognitive, loop depth)** | neither SCIP nor LSP carries these | **GAP — needs a third ingestion.** See §9. |

### Why not the obvious alternatives

**SCIP alone cannot give call edges.** MEASURED from its schema: `Relationship` carries
only `is_reference`, `is_implementation`, `is_type_definition`, `is_definition`. A call
graph from SCIP must be *inferred* by finding a reference occurrence and asking which
definition's range encloses it. `Beneficial-AI-Foundation/scip-callgraph` already
implements exactly that inference — read it rather than rewriting it. But prefer
`callHierarchy` where a language server is available, because it answers the question
directly instead of inferring it.

**rustdoc JSON is the wrong choice for docs even though it works.** It requires nightly,
and MEASURED: no nightly toolchain is installed here (`rustup toolchain list` → stable,
1.95.0, 1.98.0). SCIP via rust-analyzer runs on stable and is one binary instead of two.

**`codebase-memory-mcp` (code_graph) is disqualified as a source, measured not assumed.**
Its `Function.docstring` keeps only the **last line** of a `///` block, sigil attached,
mid-sentence — longest value in the whole `task` repo is 83 characters against source
blocks of 8–10 lines, and 0 of 40 `Method` nodes (which is where every gRPC handler lands)
carries any docstring at all. Its `Route`→`HANDLES`→`Function` query returns zero rows;
`HANDLES` is `Route`→`Route` inside the proto file. 35% of its `CALLS` edges come from name
heuristics, 59 of them at confidence 0.55 with two or three candidate targets. It also
classified three URLs found in `.pre-commit-config.yaml` as HTTP routes and promoted each
to an `api` layer. Useful negative evidence: this is what tree-sitter plus heuristics
produces, and it is not good enough to label a box with.

## 6. Prior art

| tool | state | why it doesn't do this |
|---|---|---|
| **crabviz** | MEASURED alive — 1414 stars, AGPL-3.0, pushed 2026-09-01 | Closest by far. LSP-based, real call edges, collapse-by-file, saves HTML/SVG. But `add_file(path, symbols: Vec<DocumentSymbol>)` is its ingestion unit, and **LSP's `DocumentSymbol` has no documentation field** — only `detail`, the signature. No endpoint concept anywhere in `GraphGenerator`. VS Code only (`editors/` contains one directory, `code`). Single workspace. |
| Sourcetrail | archived 2021-12-13 | never supported Rust; died of per-language indexer maintenance |
| CodeSee | dead, acquired 2024 | — |
| Structure101 | absorbed into Sonar | no Rust |
| Sourcegraph | enterprise-only | renders no diagram at all |
| CodeQL | Rust GA Oct 2025 | free licence forbids private repos and CI |
| Understand | alive | Rust is syntax highlighting only, not analysis |
| CodeScene | alive, real Rust support | draws hotspots and change coupling, no call graph |
| Nx / Turborepo | alive | stop at the package node |
| Jaeger / Tempo service graphs | alive | service-to-service only; no function-level aggregation exists in any OTel UI |
| Swimm, CodeBoarding, GitDiagram | alive | descriptions are LLM-authored by vendor documentation |

**crabviz is ~60–70% of this.** Its architecture is sound and worth copying. Its licence is
AGPL-3.0, so deriving from the code makes this AGPL too; and its README states it "is
better suited as an IDE/editor extension than a standalone command line tool", so a
headless CLI argues with the author's stated position rather than filling a gap he wants
filled. **Recommendation: borrow the architecture, not the code.**

## 7. v0.1 scope

One command. One repo. One HTML file. No server, no account, no config file, no database.

```
  <tool> ./path/to/repo  →  ./overview.html
```

Ships:
- Rust only.
- Roots from `.proto` served RPCs, joined to the tonic trait impl.
- Nodes = functions and methods, labelled with **name + first line of doc comment**.
- Edges from `callHierarchy`, filtered by path prefix (see §8).
- Collapsible module boxes, expand-on-click, depth slider defaulting to 3.
- An "unreachable from any endpoint" list, as **text**, with the §8 wording rule.

Explicitly **not** in v0.1:
- The multi-language SCIP abstraction. One language first.
- Cross-repo stitching. The mechanism is proven; shipping it is v0.2.
- Metrics. Unsourced as of §5 — do not ship a number whose provenance is unsettled.
- Any store. The graph is regenerated each run, so it is JSON inlined in the HTML.

### Store and renderer

**No store at v1.** Because output is regenerated rather than mutated, the store has no
runtime role — its job ends when the file is written. Inline the graph JSON in the HTML.
SQLite, dropped and rebuilt per run, only if build-time traversal becomes painful.

Do **not** adopt Kùzu — MEASURED archived 2025-10-10, despite being the best technical fit
for an embedded Cypher store. Reject Neo4j Community: GPLv3, JVM server, one database per
install.

Renderer: **Cytoscape.js** (MIT, Canvas, native compound/nested nodes, real UMD build on
cdnjs so it works from one `<script>` tag with no build step) with **fCoSE** or the ELK
adapter for layout. AntV G6 is the strongest alternative and has better native nesting
(Combos with built-in expand/collapse). Sigma.js is out — no compound-node support.
Mermaid is out on the one hard number in the survey: default `maxEdges` is **500**.

Scale, reframed: ~4000 symbols estate-wide is the *index* size, but drill-down means a few
hundred on screen. So choose the renderer on compound-node support and zero-build
bundling, not on raw throughput.

## 8. Known failure modes

**Trait dispatch and generics.** rust-analyzer open issue #19358: call hierarchy misses
calls through generics. The probe crossed the boundaries in one handler; expect gaps
elsewhere. Show a missing edge as missing; never infer one to fill a hole.

**Async is not execution order.** The visible call tree is the *static* structure. A
diagram of it is not a sequence diagram and must not be labelled as one.

**"Dead code" is the most dangerous claim in the tool.** In Rust, false positives come
from trait dispatch, generics, `pub` library surface, and test-only code — and #19358
means the analysis is knowingly incomplete. Telling someone to delete working code is the
one failure that permanently destroys trust.

> **Wording rule, binding:** report **"not reachable from any endpoint in this index"**.
> Never "dead". The first is a true statement about the measurement that stays true when
> the analysis is incomplete, and it still points the reader at exactly the right places.

**Edge noise.** MEASURED: 20 edges from one handler, of which the useful ones separate by
**path prefix alone** — no name matching, no confidence score:

| verdict | prefix | examples |
|---|---|---|
| first-party | `src/` | `tel_scope`, `passthrough` |
| cross-repo leaf | `target/debug/build/*/out/` | the generated client stub |
| in-org crate boundary | the org's own crates | `Call::start`, `call.run` |
| drop | `/nix/store/…rust-lib-src/` | `pin`, `map`, `trim`, `Ok`, `Err`, `Some` |
| drop | third-party crates | `into_inner`, `invalid_argument`, `encoded_len` |

**Two hard prerequisites.**
1. `rust-analyzer` must be present. MEASURED: it is *not* installed here — the binary on
   PATH is `rustup` proxying to itself and looping. A bare `command -v rust-analyzer`
   **succeeds and proves nothing**. The tool must verify the server actually responds to
   `initialize`, not that a name resolves.
2. The repo must have been **built at least once**. The client-stub edge resolved only
   because `target/debug/build/…/out/` existed. A fresh clone has no generated proto code
   and therefore no visible cross-repo leaf.

**`outgoingCalls` is one round trip per node.** A whole-repo walk is minutes, not seconds.
This is a CI-generated artefact, not an interactive tool. Say so in the README.

## 9. Open questions, in the order they should be answered

1. **Are the doc comments actually good enough to label a box with?** Generate a real
   `index.scip` for one repo and read the `documentation` fields. This is the only
   unverified link in the whole chain and it is ~10 minutes. **If the answer is no, the
   project's premise fails and nothing else matters.**
2. **Where do metrics come from?** Compute from the AST, or take from an existing tool.
   Unsettled; v0.1 ships without them rather than shipping an unsourced number.
3. **Is `sunerpy/codegraph-rust` a shortcut?** MIT, tree-sitter, explicitly "no AI/LLM
   inside", 38 languages. Unconfirmed whether it emits a renderable diagram or extracts
   `///` text. One hour of reading could collapse this project to a renderer.
4. **Licence.** AGPL follows from touching crabviz code. MIT or Apache-2.0 is the norm for
   a dev tool people will actually adopt. Decide before the first commit, because it
   constrains what may be copied.
5. **Adoption bar.** Every dead tool in §6 required setup before it gave value. Does v0.1
   run with genuinely zero configuration on a repo the author has never seen?

## 10. Evidence appendix

Commands run this session, so any claim above can be re-checked.

```bash
# rust-analyzer exists but is NOT installed — proxy loop
type -a rust-analyzer                      # → /etc/profiles/per-user/max/bin/rust-analyzer
readlink -f /etc/profiles/per-user/max/bin/rust-analyzer   # → rustup-1.29.0/bin/rustup
rustup component list --installed          # no rust-analyzer
nix shell nixpkgs#rust-analyzer --command rust-analyzer --version   # → 2026-06-15

# SCIP subcommand exists
nix shell nixpkgs#rust-analyzer --command rust-analyzer --help | grep -A2 scip

# the probe: outgoingCalls from handlers.rs::create_task, 20 edges
# (driver committed at docs/probe/ra_probe.py)

# crabviz: alive, AGPL, VS Code only
gh-personal api repos/chanhx/crabviz -q '.stargazers_count, .license.spdx_id, .archived'
gh-personal api repos/chanhx/crabviz/contents/editors -q '.[].name'    # → code

# crabviz core builds standalone, no wasm feature
cd core && cargo build --release           # → Finished in 6.56s, 1 dead-code warning

# LSP DocumentSymbol has no documentation field
grep -n "struct DocumentSymbol" -A 14 core/src/types/lsp.rs

# code_graph docstring truncation
yadgar code-graph query <repo> --project task-1c8ae1eb105a \
  'MATCH (f:Function) RETURN f.name, length(f.docstring) AS L ORDER BY L DESC LIMIT 3'
  # → passthrough 83, builder 82, watch_set 81
yadgar code-graph query <repo> --project task-1c8ae1eb105a \
  'MATCH (n:Method) WHERE n.docstring <> "" RETURN count(n)'    # → 0 rows
```

Related estate records: ADR-0703 (every claim parser-extracted, never model-written),
ledger 867 and its body page `yadgarhq_docs_task-867`.
