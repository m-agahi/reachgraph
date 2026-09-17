# Plan 03 — `reachgraph-lang-rust`

**Status:** ready to build. Two of §4's three ADR-0001 questions are decided; the third is
measured and leaves one ADR-level decision open.
**Date:** 2026-09-17
**Depends on:** [plan-00](00-workspace-and-plugin-api.md) (as amended 2026-09-17), plan-01
(waist), plan-02 (fixture plugin); ADR-0001, ADR-0003, ADR-0004, ADR-0005, ADR-0008
**Blocks:** plan-04 (consumes the `Symbol` shape), plan-06

The crate that makes v0.1 real. It implements `SymbolProvider`, `EdgeProvider`,
`Classifier` and `discover_units` over the `ra_ap_*` family (ADR-0004), and it is the only
crate in the workspace permitted to know that rust-analyzer exists.

Every `ra_ap` fact below is labelled MEASURED (verified against docs.rs for version
0.0.352 on 2026-09-17, URL given) or UNVERIFIED (needs a docs.rs or compile check before
the code is written). Nothing is asserted from memory.

---

## 1. Scope

**In:**

- `discover_units(root) -> Vec<Unit>` returning Cargo crates (ADR-0008 leak 4).
- `SymbolProvider::symbols_in(unit)` — functions, methods, impl blocks, modules, with
  docs, `container`, `is_test`, neutral `SymbolKind` plus `raw_kind`.
- `EdgeProvider::edges_in(unit)` and `edges_from(node)` — **no position in either
  signature** (ADR-0008 leak 1, the one this crate exists to absorb).
- `Classifier::classify(path, unit) -> Category`.
- `preflight(root)`.
- The vendoring discipline for `ra_ap_*` (ADR-0001).

**Out:**

- Metrics. Unsourced (design.md §9 Q2); ADR-0008 says do not build.
- Python, Go, Java. Anything.
- Cross-repo stitching. Plan-04 produces the join keys; joining is v0.2.
- Dropping edges. This crate **classifies**; the waist and the renderer decide policy.
  design.md §8's `drop` column is a consumer decision over `Category`, not a filter here.

### Capability declaration

```rust
fn provides(&self) -> &[Capability] {
    &[Capability::Symbols, Capability::Edges, Capability::Classify]
}
fn position_encoding(&self) -> PositionEncoding { PositionEncoding::Utf8Bytes }
fn detection(&self) -> Detection {
    Detection { marker_files: &["Cargo.toml"], extensions: &[".rs"] }
}
```

`Utf8Bytes` because `ra_ap` is byte-offset based (ADR-0008 leak 3). One crate declaring
three capabilities is ADR-0003 field 1 doing its job: the repository is indexed once.

---

## 2. Dependencies and vendoring (ADR-0001)

MEASURED 2026-09-17 (crates.io, carried from ADR-0001/ADR-0004): ~48 `ra_ap_*` crates at
version **0.0.352**, published 2026-09-14, MIT OR Apache-2.0, republished weekly in
lockstep with rust-analyzer nightlies. 24 crates.io reverse dependencies including
`cargo-callgraph` and `cargo-modules` — evidence of real library consumption, not merely a
public surface.

**0.0.x carries no semver promise.** The containment is ADR-0001's, unchanged:

1. Pin **exact**: `ra_ap_ide = "=0.0.352"`, every crate, no caret.
2. `cargo vendor` into the repository; source replacement in `.cargo/config.toml`.
3. Build `--locked` in CI and in the release job.
4. An upgrade is a **deliberate re-vendor** on its own pull request, with the expectation
   that it breaks. The re-vendor PR must run the full Tier-B suite (§13) before merge,
   because that suite is the only thing that detects a silent behaviour change in an
   engine with no changelog contract.

Direct dependencies, INFERRED from the API surface this plan uses — **the exact set is
UNVERIFIED and is settled by the first successful compile**, not by this list:

| crate | why |
|---|---|
| `ra_ap_ide` | `Analysis`, `outgoing_calls`, `CallItem`, `NavigationTarget` |
| `ra_ap_ide_db` | `RootDatabase`, symbol search |
| `ra_ap_hir` | semantic walk, `HasAttrs` for docs, `Impl` for `container` |
| `ra_ap_load-cargo` | `load_workspace_at`, `LoadCargoConfig` |
| `ra_ap_project_model` | `CargoConfig`, manifest discovery, sysroot resolution |
| `ra_ap_vfs` | `Vfs`, `FileId` ↔ path (never crosses the plugin boundary — leak 2) |
| `ra_ap_paths` | `AbsPathBuf` |
| `ra_ap_proc-macro-srv` | only if §4 D-C's in-process wiring is taken. MEASURED: its own dependencies are `ra_ap_intern`, `ra_ap_paths`, `ra_ap_span`, `ra_ap_stdx`, `rustc-hash` — all already in the tree, so it adds no new third-party surface |
| `ra_ap_syntax` | only if §14 open question 8 is taken |

The engine string on every `Provenance` (`"ra_ap_ide 0.0.352"`) is built from a `const`
derived from the pinned version, never hand-typed. A test asserts it (§13, Tier C).

---

## 3. The measured `ra_ap` surface

MEASURED 2026-09-17, docs.rs for version 0.0.352. Verbatim signatures:

```rust
// https://docs.rs/ra_ap_ide/0.0.352/ra_ap_ide/struct.Analysis.html
pub fn call_hierarchy(
    &self,
    position: FilePosition,
    config: &CallHierarchyConfig<'_>,
) -> Cancellable<Option<RangeInfo<Vec<NavigationTarget>>>>

pub fn incoming_calls(
    &self,
    config: &CallHierarchyConfig<'_>,
    position: FilePosition,
) -> Cancellable<Option<Vec<CallItem>>>

pub fn outgoing_calls(
    &self,
    config: &CallHierarchyConfig<'_>,
    position: FilePosition,
) -> Cancellable<Option<Vec<CallItem>>>

pub fn file_structure(
    &self,
    config: &FileStructureConfig,
    file_id: FileId,
) -> Cancellable<Vec<StructureNode>>
```

Note the parameter order differs between `call_hierarchy` (position first) and
`outgoing_calls` (config first). ADR-0004 quotes these signatures with parameters elided;
that quotation is an abbreviation of the same measurement, not a different one.

```rust
// struct.CallItem.html
pub struct CallItem { pub target: NavigationTarget, pub ranges: Vec<FileRange> }

// struct.NavigationTarget.html
pub struct NavigationTarget {
    pub file_id: FileId,
    pub full_range: TextRange,
    pub focus_range: Option<TextRange>,
    pub name: Symbol,
    pub kind: Option<SymbolKind>,
    pub container_name: Option<Symbol>,
    pub description: Option<String>,
    pub alias: Option<Symbol>,
}

// struct.StructureNode.html
pub struct StructureNode {
    pub parent: Option<usize>,     // index into the returned Vec
    pub label: String,
    pub navigation_range: TextRange,
    pub node_range: TextRange,
    pub kind: StructureNodeKind,
    pub detail: Option<String>,
    pub deprecated: bool,
}

// https://docs.rs/ra_ap_load-cargo/0.0.352/ra_ap_load_cargo/
pub fn load_workspace_at(
    root: &Path,
    cargo_config: &CargoConfig,
    load_config: &LoadCargoConfig,
    progress: &(dyn Fn(String) + Sync),
) -> Result<(RootDatabase, Vfs, Option<ProcMacroClient>)>

pub struct LoadCargoConfig {
    pub load_out_dirs_from_check: bool,
    pub with_proc_macro_server: ProcMacroServerChoice,
    pub prefill_caches: bool,
    pub num_worker_threads: usize,
    pub proc_macro_processes: usize,
}

pub enum ProcMacroServerChoice { Sysroot, Explicit(AbsPathBuf), None }
```

Two of these fields are why §4 exists.

---

## 4. The ADR-0001 collision — two decisions and one measured gap

ADR-0001 says: *no external binaries, no subprocesses, no runtime downloads.* The measured
`LoadCargoConfig` shape (§3) puts three subprocess-shaped decisions on the critical path.
Two are now settled by user decision; the third is measured and leaves exactly one
ADR-level question open.

### D-A. `cargo metadata` — **DECIDED: carved out**

`load_workspace_at` takes a `&CargoConfig` and obtains the workspace by invoking `cargo`.

**Decision 2026-09-17: the target language's own build toolchain is carved out of
ADR-0001.** The ban covers analysis tooling a user would install *for reachgraph's sake*
— rust-analyzer, gopls, jdtls. It does not cover `cargo`. The test: a tool is carved out
when it is a precondition of the repository being analysable at all, and a Rust repository
on a machine with no Rust toolchain is not analysable by anything. ADR-0001 is being
amended separately; this plan references the carve-out and does not restate it.

Consequence here: `load_workspace_at` is used as-is. No hand-written workspace loader.
`preflight()` verifies `cargo` **responds** rather than that a name resolves (§11) —
ADR-0003 field 5, and the same discipline the rustup proxy loop taught.

### D-B. `load_out_dirs_from_check` — **DECIDED: reachgraph never runs a build**

**Decision 2026-09-17: reachgraph runs no build, ever.** No `cargo check`, no
`cargo build`. `load_out_dirs_from_check` is **not** set to a value that builds.

This is the decision D-A does not cover, and the distinction is exact: reading a
workspace's structure is a precondition of analysis; *producing artifacts* is doing the
user's build for them, with the user's build scripts, at a time the user did not choose.

It matches design.md §8's second hard prerequisite — MEASURED, the client-stub leaf at
`target/debug/build/yadgar-task-.../out/yadgar.task.v1.rs:272` resolved **only because
that directory already existed**.

**The run degrades honestly rather than failing.** When `target/*/out/` is absent:

- `preflight()` reports it with structured remediation (§11 check 2);
- generated code is not indexed, so cross-repo leaves do not appear;
- **that fact is recorded in the artifact's coverage**, not merely logged (§11).

A missing leaf is thereby a reported gap, exactly as an unbound root is (plan-04 §9).
The failure mode this forecloses is the dangerous one: an index that looks complete,
silently lacks every generated node, and reports the code behind them as not reachable
from any endpoint.

### D-C. Proc-macro expansion — MEASURED, and one ADR-level question remains

MEASURED, design.md §4: `outgoingCalls` from `create_task` resolved **through
`#[tonic::async_trait]`, a proc macro**, and through a nested `async move` closure. That
single result is the evidence the whole cross-repo mechanism rests on. So this is the
question that matters most, and it is now measured rather than inferred.

**MEASURED 2026-09-17, `ra_ap_load-cargo` 0.0.352 source
(`docs.rs/ra_ap_load-cargo/0.0.352/src/ra_ap_load_cargo/lib.rs.html`, lines 48–133).
`load_workspace_at` has no in-process path. All three variants spawn or disable:**

```rust
ProcMacroServerChoice::Sysroot => ws.find_sysroot_proc_macro_srv().map(|it| {
    it.and_then(|it| { ProcMacroClient::spawn(&it, extra_env, ws.toolchain.as_ref(),
                                              load_config.proc_macro_processes) …
ProcMacroServerChoice::Explicit(path) => Some(
    ProcMacroClient::spawn(path, extra_env, ws.toolchain.as_ref(),
                           load_config.proc_macro_processes) …
ProcMacroServerChoice::None => Some(Err(ProcMacroLoadingError::Disabled)),
```

MEASURED: `Explicit` takes a path to an **executable** and spawns it;
`find_sysroot_proc_macro_srv() -> Option<Result<AbsPathBuf>>` returns a path that is
spawned the same way. So through this API the only choices are *spawn a server binary* or
*disable expansion*. There is no third option in `load-cargo`.

**MEASURED: the in-process capability nonetheless exists, one layer down.**
`ra_ap_proc-macro-srv` 0.0.352 (MIT OR Apache-2.0, `has_lib: true`, `bin_names: []`,
dependencies `ra_ap_intern`, `ra_ap_paths`, `ra_ap_span`, `ra_ap_stdx`, `rustc-hash` —
no IPC crate among them) exposes, from its own `lib.rs` source:

```rust
pub struct ProcMacroSrv<'env> { … }
pub fn new(env: &'env EnvSnapshot) -> Self
pub fn expand<'a, S: ProcMacroSrvSpan + 'a>(
    &self, lib: impl AsRef<Utf8Path>, env: &[(String, String)],
    current_dir: Option<impl AsRef<Path>>, macro_name: &str,
    macro_body: token_stream::TokenStream<S>, attribute: Option<token_stream::TokenStream<S>>,
    def_site: S, call_site: S, mixed_site: S,
    tracked_env: &'a mut TrackedEnv, callback: Option<ProcMacroClientHandle<'a>>,
) -> Result<token_stream::TokenStream<S>, ExpandError>
pub fn list_macros(&self, dylib_path: &Utf8Path) -> Result<Vec<(String, ProcMacroKind)>, String>
```

MEASURED: expansion happens **in this process**, by `dlopen`-ing the compiled proc-macro
dynamic library (`expanders: Mutex<FxHashMap<Utf8PathBuf, Arc<dylib::Expander>>>`). The
crate's own description is "This library is able to call compiled Rust custom derive
dynamic libraries on arbitrary code." No subprocess is involved in `expand`.

**So the gap is wiring, not capability.** `load_workspace_at` routes through
`ProcMacroClient`, which is the IPC client half. Using `ProcMacroSrv` directly means
supplying reachgraph's own proc-macro expander to the database instead of taking
`load-cargo`'s. INFERRED, UNVERIFIED: the seam is the expander that `ra_ap_base_db` /
`ra_ap_hir_expand` holds per proc macro; implementing it over `ProcMacroSrv` is the path.
Verify that seam before committing to it — it is the one remaining engineering unknown.

**Two consequences that are not negotiable, whichever way the wiring goes:**

1. `ProcMacroSrv::expand` takes `lib`, a path to a **compiled** proc-macro dylib — built
   by cargo into `target/*/deps/`. Under D-B reachgraph never builds, so **in-process
   expansion is available only on a repository that has already been built**, exactly like
   `OUT_DIR`. The two prerequisites are one prerequisite. §11 check 2 covers both.
2. **The carve-out of D-A does not extend to a proc-macro server.** A proc-macro server is
   rust-analyzer's own component, not the target language's build toolchain, so spawning
   one is not permitted by D-A and is **an ADR-level decision, flagged rather than
   assumed** (§14 open question 3). Recorded because it is the borderline the ADR must
   rule on: MEASURED, `Sysroot` resolves that binary from within the toolchain sysroot,
   which is why it looks like the carve-out and is not — the binary is there because
   rustup ships rust-analyzer's helper, not because cargo needs it.

**Recommended order of work, and it does not require the ADR to rule first:** verify the
expander seam and wire `ProcMacroSrv` in-process. If it works, no subprocess is ever
spawned, ADR-0001 is satisfied exactly as written, and the ADR-level question never has to
be asked. Only if the seam proves closed does the choice become *spawn a server* versus
*lose the design's central measured result*, and that choice belongs in an ADR.

Until it is wired, the ADR-0001-compatible configuration is
`ProcMacroServerChoice::None`, and §11 check 3 makes the degradation visible.

---

## 5. Plugin lifecycle and state

`Plugin: Send + Sync` and every trait method takes `&self` (plan-00 §3.0). The engine is
not stateless, so the crate holds its loaded workspace behind interior mutability:

```rust
pub struct RustPlugin {
    loaded: RwLock<Option<Loaded>>,
}

struct Loaded {
    root: PathBuf,
    host: AnalysisHost,          // owns RootDatabase
    vfs: Vfs,                    // FileId <-> path, valid only while held
    sysroot_src: Option<PathBuf>,
    units: Vec<Unit>,
    crate_of_file: HashMap<PathBuf, CrateFacts>,
    nodes: NodeTable,            // §6
}
```

**Load once per `root`, lazily, on the first call that needs it.** `discover_units`,
`symbols_in`, `edges_*` and `classify` all share it. A different `root` invalidates and
reloads; `NodeTable` is cleared with it.

**Leak 5 (salsa lifecycle) never crosses the boundary.** No database, snapshot, `FileId`
or `Cancellable` appears in any signature in `plugin-api`. `Cancellable::Err` is mapped to
`PluginError` at the crate edge.

**UNVERIFIED, and it constrains the above:** whether `AnalysisHost` and the `Analysis`
snapshot it hands out are `Send` and `Sync`. rust-analyzer's own use is one snapshot per
worker thread. If `Analysis` is `Send` but not `Sync`, `Loaded` must produce a fresh
snapshot per call under the lock rather than storing one. Verify before writing §9.

---

## 6. Node identity and the leak-1 conversion

This is the section ADR-0008 calls the highest risk of the eight, and plan-00 §8 open
question 1.

### The rule

`EdgeProvider::edges_in(&Unit)` and `edges_from(&NodeId)` take no position, no cursor, no
offset and no `FilePosition`. `ra_ap_ide::Analysis::outgoing_calls` takes a
`FilePosition` (MEASURED, §3). **The conversion happens entirely inside this crate and
appears nowhere in `plugin-api`.**

### `NodeId.raw` is self-describing

```
raw := "<unit_id>|<def_offset>|<repo-relative path>"
```

- `unit_id` — the Cargo package id of the owning crate; contains no `|`.
- `def_offset` — decimal byte offset (UTF-8, matching the declared `PositionEncoding`) of
  the **name token**: `NavigationTarget::focus_range.start()`, falling back to
  `full_range.start()` when `focus_range` is `None`.
- path — repo-relative, `/`-separated, **last**, so decoding is `splitn(3, '|')` and a
  `|` inside a path is harmless.

The offset is the name token, not the item start, because that is exactly what
`FilePosition` needs for `outgoing_calls` to identify the item under it. `Symbol::range`
separately carries the full item range for display.

**`SourceRange` is now `{ file: PathBuf, span: Option<Span> }`** (plan-00 §2, amended):
the file is always known, the span may not be. **Rust supplies both, always.** MEASURED
(§3): `NavigationTarget::full_range` is a plain `TextRange`, not an option, so every symbol
this crate emits carries `span: Some(_)`. The `None` case exists for plugins whose
resolver knows a file but not an offset; `lang-rust` never produces it, and §13 asserts
that.

The two optionalities are unrelated and must not be conflated. `span: None` means *located
in a file, offset unknown*. A target that fails the `Vfs` lookup entirely (§9) has no file
either, so no `Symbol` is emitted at all — plan-01's "external". A `Symbol` with a file and
no span is still classifiable, which is the property plan-01 §7.0's termination argument
depends on.

**Consequence, and it is the design's simplification:** `NodeId` is a *pure function* of
`(unit, path, name-token offset)`. Converting a node to a position is therefore a **decode,
not a lookup** — no table is required for the forward direction, and no table is required
to mint a `NodeId` for a call target discovered mid-traversal. That matters: the
cross-repo leaf at `target/debug/build/*/out/yadgar.task.v1.rs:272` is not a member of any
first-party unit, was never returned by `symbols_in`, and must still get a stable
`NodeId`.

### What the side table is actually for

`NodeTable` is not the conversion mechanism. It is:

1. **Unit attribution** — which `Unit` a raw belongs to, for `edges_in` batching.
2. **Validation** — a `NodeId` whose plugin half is not this plugin, or whose path is no
   longer in the `Vfs`, is a `PluginError`, never a silent empty result.
3. **`FileId` resolution** — `path -> FileId` through the held `Vfs`. `FileId` is a `Vfs`
   interning valid only while that `Vfs` lives (ADR-0008 leak 2), so it is resolved on
   demand and never stored in a `NodeId`.

```rust
struct NodeTable {
    // raw -> (absolute path, name-token offset); populated by symbols_in
    by_raw: HashMap<String, (AbsPathBuf, TextSize)>,
}
```

### Consistency rule

The table is built during `symbols_in` and **invalidated whenever `Loaded` is replaced**.
It is never persisted and never written to the artifact. Because `raw` is decodable, a
`NodeId` that survives across a reload of the *same* commit still resolves; a `NodeId`
from a *different* commit resolves to a wrong or absent offset and is caught by check (2)
above. Node identity is stable within one run and across runs of one commit — which is
all ADR-0006 needs, since the artifact is regenerated per run rather than mutated.

### `edges_from` needs no `&Unit`

plan-00 §8 open question 1 asked whether `edges_from` might need a `&Unit` for context.
**It does not**, given a self-describing `raw`: the unit id is inside it. The question
re-opens only if §4 D-C's wiring forces a per-unit engine instance. Re-verify once §5's `Send`/`Sync`
question is settled.

---

## 7. `discover_units`

```rust
fn discover_units(&self, root: &Path) -> Result<Vec<Unit>, PluginError>
```

Rust's unit of analysis is the **crate** (ADR-0008 leak 4).

1. Discover the manifest at or above `root` (`ra_ap_project_model` manifest discovery).
2. Load the workspace (§4 D-A: cargo is carved out of ADR-0001).
3. Emit one `Unit` per **workspace member** crate:
   - `id: UnitId(<cargo package id>)` — stable, unique, no `|`.
   - `display_name` — the crate name, plus target kind where a package has several
     (`mycrate`, `mycrate (test)`, `mycrate (build)`), so two units never display
     identically.
   - `root` — the crate's manifest directory.
4. Dependency crates are **not** units. They are still indexed and still receive symbols
   and `NodeId`s when an edge points into them; they are simply not walked by `edges_in`.
   This is what keeps a whole-repo pass bounded to first-party code while leaving the edge
   into a dependency visible and classified.

No `Cargo.toml`, manifest, workspace or crate concept appears in `plugin-api`.

---

## 8. `symbols_in`

```rust
fn symbols_in(&self, unit: &Unit) -> Result<Vec<Symbol>, PluginError>
```

### Source of truth

Two candidate walks, both MEASURED to exist:

- **(a) semantic**, via `ra_ap_hir`: crate → modules → `ModuleDef` → `Function`, `Impl`,
  `Trait`, `Struct`, … `HasAttrs` is MEASURED to be implemented for 24 types including
  `Function`, `Impl`, `Trait` and `Module`.
- **(b) syntactic**, via `Analysis::file_structure(config, file_id) -> Vec<StructureNode>`,
  which is MEASURED to carry `parent: Option<usize>` — a ready-made container relation —
  but **no documentation field**.

**Use (a).** Documentation is not optional (ADR-0005; design.md §9 Q1 calls it the premise
question), and (b) cannot supply it. (b) remains useful as a cheap cross-check that the
semantic walk did not miss an item, and as the fallback if §4 D-C degrades the semantic
view.

### Kind mapping — ADR-0008 leak 6

A pure function, table-driven, unit-testable without an engine:

| `ra_ap` kind | `SymbolKind` | `raw_kind` |
|---|---|---|
| `Function` | `Function` | `"Function"` |
| `Function` in an `impl` | `Method` | `"Method"` |
| `Struct`, `Enum`, `Union`, `TypeAlias` | `Type` | the ra_ap term |
| `Trait`, `TraitAlias` | `Type` | `"Trait"` |
| `Impl` | `Other` | **the rendered impl header** — see below |
| `Module` | `Module` | `"Module"` |
| `Field` | `Field` | `"Field"` |
| `Macro`, `Static`, `Const`, everything else | `Other` | the ra_ap term |

`raw_kind` is display text that the waist is forbidden to interpret (plan-00 §3.4). One
consumer *is* permitted to read it: a roots plugin, walking up through `container`.

### `container` — the field plan-04 depends on

Every method gets `container: Some(<NodeId of its impl block>)`, and the impl block is
emitted as a `Symbol` in its own right. Free functions get the enclosing module; a
top-level item gets `None`.

The impl `Symbol` carries, per plan-00 §3.4's worked example:

- `name` — the self type's name, e.g. `Task`.
- `raw_kind` — the **rendered impl header**, one of:
  - `impl <Trait> for <SelfTy>` for a trait impl, e.g. `impl TaskService for Task`
  - `impl <SelfTy>` for an inherent impl, e.g. `impl MockDb`

`<Trait>` is the trait's **declared name**, not the path it was imported by. MEASURED
2026-09-17, `/home/max/git/yadgarhq/task/src/service/handlers.rs:17,21`:

```rust
use crate::pb::yadgar::taskapi::v1::task_service_server::TaskService;
#[tonic::async_trait]
impl TaskService for Task {
```

and MEASURED, the generated stub at
`target/debug/build/yadgar-task-237f97ebf0e011bd/out/yadgar.taskapi.v1.rs:384`:
`pub trait TaskService: …` inside `pub mod task_service_server`. The declared trait name
equals the proto service name. Using the declared name rather than the use-path means
plan-04 compares against the proto service name directly and never has to resolve a Rust
import.

**This makes `raw_kind` a string contract between two plugins.** It is stated here, in
one place, with an exact grammar; plan-04 §7 parses it with an anchored rule and never a
substring search. Residual risk, accepted and recorded: two distinct traits with the same
declared name in one repository render identically. §14 open question 6.

UNVERIFIED: the exact `ra_ap_hir::Impl` accessors for the trait and self type (something
of the shape `Impl::trait_(db)` / `Impl::self_ty(db)`). Verify against docs.rs before
writing the renderer. The *content* of the string is settled here; the call that produces
it is not.

### `is_test`

`true` when any of:

- the file belongs to a target of kind `test` (a `tests/` integration target) or `bench`;
- the item, or any enclosing item, carries `#[cfg(test)]` or `#[test]`.

MEASURED basis, design.md §4 and re-verified 2026-09-17:
`/home/max/git/yadgarhq/task/tests/service.rs:145` — `impl TaskDbService for MockDb` with
`async fn create_task` at `:146`, against the real handler at
`src/service/handlers.rs:22`. Both are named `create_task`. `is_test` is not decoration;
plan-04 §6 uses it to decide **direction**, where getting it wrong manufactures a phantom
root.

### Docs — ADR-0005 and leak 7

`doc: Option<String>`, `doc_format: DocFormat::Markdown` (Rust doc comments are Markdown
by convention).

MEASURED, docs.rs `ra_ap_hir::HasAttrs`:

```rust
fn attrs(self, db: &dyn HirDatabase) -> AttrsWithOwner
fn hir_docs(self, db: &dyn HirDatabase) -> Option<&Docs>
```

MEASURED 2026-09-17, `ra_ap_hir::Docs` — **the conversion exists and is direct**:

```rust
pub fn into_docs(self) -> String     // owned
pub fn docs(&self) -> &str           // borrowed
```

plus `macro_calls`, `find_ast_range`, `shift_by`, `prepend_str`, `append_str` and
`append`. There is **no `Display` and no `AsRef` implementation**, so those two accessors
are the whole interface. `Docs` derives `Clone, Debug, Eq, Hash, PartialEq` and is
`Send + Sync`.

The call is therefore `sym.hir_docs(db).map(|d| d.docs().to_owned())`. Two details stay
UNVERIFIED and are cheaper to settle at the first compile than by more reading: the exact
spelling of the borrowed accessor's return type (docs.rs rendered it malformed), and
whether the text arrives with `///` sigils already stripped and lines joined. §13's
`fx-docs` fixture asserts both, so a wrong assumption is a failing test rather than a
shipped defect.

Two things this must not become:

- **Not `Analysis::hover`.** MEASURED to exist, but it returns rendered hover markup —
  signature, type, links, doc text, all fused. Scraping doc text back out of presentation
  markup is the wrong mechanism and would be fragile against every release.
- **Not tree-sitter.** ADR-0005 is explicit: the engine that resolved the call has already
  parsed the file. Rust needs no second parser.

**Fallback, stated in advance so it is not invented under pressure:** if no accessor
yields clean doc text, fall back to **name plus signature** — which is what crabviz ships
(MEASURED, design.md §6: LSP's `DocumentSymbol` has no documentation field, only
`detail`), and which ADR-0005 already names as the fallback. `NavigationTarget::description`
is MEASURED to exist and is the source for it. The tool degrades to worse labels; it does
not degrade to guessed labels.

The standard to beat is MEASURED in design.md §5: `code_graph` retains only the **last
line** of a `///` block, sigil attached, mid-sentence, longest value 83 characters, and
0 of 40 `Method` nodes carry any docstring at all. §13's `fx-docs` fixture asserts against
exactly that.

---

## 9. `edges_in` and `edges_from`

### `edges_in(unit)`

```
for each Symbol s in unit where s.kind in {Function, Method}:
    (path, offset) = decode(s.id.raw)
    file_id        = vfs.file_id(path)?
    pos            = FilePosition { file_id, offset }
    items          = analysis.outgoing_calls(&config, pos)?      // Cancellable<Option<Vec<CallItem>>>
    for item in items:
        target_node = node_id_for(item.target)                   // pure function, §6
        for range in item.ranges:                                // MEASURED Vec<FileRange>
            emit Edge {
                from: s.id.clone(),
                to: EdgeTarget::Resolved(target_node.clone()),
                call_site: Some(SourceRange::from(range)),
                provenance: Provenance { plugin: PLUGIN_ID, engine: ENGINE },
                inference_mode: InferenceMode::Resolved,
            }
```

**One `Edge` per call site**, not one per callee. `Edge::call_site` is singular
(plan-00 §2), and `CallItem::ranges` is MEASURED to be a `Vec<FileRange>` — three calls to
the same function from one body are three real call sites. Collapsing them here would
discard information the waist cannot recover. Deduplication is a renderer decision.

`node_id_for(target)` uses `focus_range.start()` when present, else `full_range.start()`,
and the target's own path — identical to §6's encoding, so a callee that is also a symbol
produces a byte-identical `raw`. A round-trip test enforces that (§13, Tier C).

`outgoing_calls` returning `Ok(None)` means "no call hierarchy at this position" and is
**not** an error: emit no edges for that symbol and record it in the per-unit counters
(§10).

### `edges_from(node)`

Decode `raw` → `(path, offset)` → `FilePosition` → the same `outgoing_calls` call. Used by
the core for depth-limited expansion from a root (ADR-0006: a shard is the reachable set
from one root). Reuses `edges_in`'s emit path verbatim; the two must not diverge.

### Locating out-of-workspace edge targets — plan-01's provider obligation

Plan-01 §7.0 makes classification per-file and an unclassified node terminate nothing, so
a reachable-but-unlocatable node cannot be stopped at a third-party boundary. That places
an obligation on this crate:

> Emit a `Symbol` for any edge target whose location is known, even when that target lies
> outside the enumerated units.

This is plan-01 §11 question 8, assigned here. **Answer: MEASURED, the obligation is
honoured for three of four target classes at no extra cost, and the fourth has a named
mechanism that has not been verified.** Details below, because "partial" is the true
answer and the partition matters to plan-01's termination argument.

#### The lookup is two steps, both cheap, both fallible

MEASURED (§3): `NavigationTarget` carries `file_id: FileId` and **no path**. The path comes
from the `Vfs` this crate already holds (§5):

```rust
// ra_ap_vfs 0.0.352
pub fn exists(&self, file_id: FileId) -> bool
pub fn file_path(&self, file_id: FileId) -> &VfsPath   // PANICS if the id is not present
pub fn as_path(&self) -> Option<&AbsPath>              // on VfsPath; None for a virtual path
```

So: `exists` → `file_path` → `as_path`. Both fallible steps are real and both must be
handled — `file_path` is MEASURED to **panic** on an unknown `FileId`, and `VfsPath` is
MEASURED to be an opaque identifier whose `as_path` returns `None` for an in-memory path.
A target failing either step is genuinely **external** in plan-01's sense: located nowhere,
no `Symbol` emitted. Never call `file_path` without `exists` first.

Cost is a slab index and a string, per target. Nothing is loaded on demand, so the
obligation adds no I/O to `edges_in`.

#### Which classes resolve

MEASURED, `ra_ap_project_model::ProjectWorkspace::to_roots() -> Vec<PackageRoot>`, whose
entries `ProjectFolders::new` feeds to the VFS loader. `PackageRoot` is MEASURED to carry
`is_local: bool`, `include: Vec<AbsPathBuf>`, `exclude: Vec<AbsPathBuf>`.

| target class | resolves? | mechanism |
|---|---|---|
| workspace member | **yes** | member roots are loaded; these are the enumerated units |
| dependency crate (registry, git, path) | **yes** | MEASURED: `to_roots()` emits `PackageRoot`s for non-member packages too, with `is_local: false`. Source comes from cargo's own extracted checkout. MEASURED limit, in the source's own words — *"For non-workspace-members, we only resolve library targets"* — so a dependency's examples, tests and benches are out of scope. Not a practical gap: a call lands in a lib target. |
| sysroot / stdlib | **conditional** | MEASURED: `mk_sysroot()` sets `include: self.sysroot.rust_lib_src_root().map(\|it\| it.to_path_buf())`. **Resolves only when the `rust-src` component is installed.** Absent, there is no sysroot root, stdlib files have no `FileId`, and every stdlib target is external. |
| generated code in `OUT_DIR` | **no, under §4 D-B — see below** | MEASURED: `to_roots()` adds it via `build_scripts.get_output(pkg).and_then(\|it\| it.out_dir.clone())` then `include.extend(out_dir)` |

The stdlib row is checkable in advance and therefore belongs in `preflight()` as an
informational fact rather than a surprise — §11 check 4, feeding `rust_src_available` in
the run record. MEASURED corroboration that it *was* present for the probe: design.md §8
lists `/nix/store/…rust-lib-src/` paths among the 20 edges, which is the stdlib row
resolving.

#### Closed by this measurement: `Node::display`

Plan-05 raised `display: Option<String>` on `Node`, because a node the language plugin
never indexed has nothing to render but a bare opaque `NodeId` — and ADR-0003 field 3
forbids the core parsing one for a name.

**CLOSED 2026-09-17 — the field is not being added.** The table above is the reason: the
two classes a renderer would actually meet, workspace members and dependency library
sources, both resolve, so a located target gets a real `Symbol` with a real `name`.
Unindexed nodes are rare rather than routine, which is what the proposal assumed they were
not. Recorded here because the measurement that closed it is here, not in plan-05.

#### The `OUT_DIR` row is a real problem, and it is not the one §11 check 2 describes

**MEASURED: the out-dir is added to the VFS only from build-script output data.** That
data is `WorkspaceBuildScripts`, obtained from `ProjectWorkspace::run_build_scripts` (which
runs cargo) or handed over via `set_build_scripts`. With `load_out_dirs_from_check: false`
— which §4 D-B requires — the data is empty, `get_output(pkg)` yields `None`, and the
out-dir never enters `include`.

**So generated code is not indexed even when it is present on disk.** That is a sharper
statement than §11 check 2 and §12 currently make, and it must not be softened: the gap is
not "the user has not built the repository", it is "reachgraph never told the VFS to look".
A user who follows check 2's remediation, builds the workspace and re-runs, gets the same
empty result.

Whether an escape exists, MEASURED as far as documentation goes:

- `ProjectWorkspace::set_build_scripts(&mut self, bs: WorkspaceBuildScripts)` is **public**,
  and `load_workspace(ws, extra_env, load_config)` takes an already-constructed workspace —
  so the *shape* "construct the build-script data from what is already on disk, then load"
  exists.
- **But `WorkspaceBuildScripts` has private fields and no public constructor except
  `Default`** (MEASURED). A downstream crate cannot populate it with an out-dir it
  discovered itself. `set_build_scripts` has nothing useful to be handed.
- MEASURED: `ProjectWorkspace::extra_includes: Vec<AbsPathBuf>` is a **public field**,
  documented as *"Additional includes to add for the VFS."* Pushing each discovered
  `target/<profile>/build/<pkg>-<hash>/out/` onto it before `load_workspace` should make
  those files VFS-resident and therefore locatable. **UNVERIFIED** — this is the one thing
  here that needs running code, and it is the highest-value next measurement in this plan.

#### What is honestly claimable today

Two different failures are being distinguished, and conflating them would be the
correctness bug the coordinator warned about:

1. **A call into generated code that `ra_ap` resolves, whose target cannot be located.**
   This would breach the obligation. INFERRED that it does not arise: the out-dir module
   reaches the crate graph through the same build-script data that is missing, so without
   it `ra_ap` has no generated module to resolve *into* and returns no `CallItem` at all.
   No target, nothing to locate, obligation intact.
2. **The edge is absent entirely.** This is what actually happens, it is already documented
   (§11's run record, §12), and it is a reachability gap rather than a termination gap.

So plan-01's traversal termination is **not** threatened by the out-dir finding, and this
plan does not assume otherwise. What is threatened is design.md §4's cross-repo leaf and
plan-04 §11's consumed-root binding — both of which already depend on this and now depend
on it for a second, sharper reason.

#### D-D. The fallback is decided: v0.1 ships without generated code indexed

**Decision 2026-09-17: if `extra_includes` does not load `OUT_DIR`, v0.1 ships with
generated code unindexed and records the fact in coverage. §4 D-B is not revisited, and
there is no `--allow-build` flag.**

So this is no longer a branch. Question 9a (§14) stays open because a working
`extra_includes` is strictly better — it would recover both classes at no cost — but its
*failure* is now a known outcome with specified behaviour, not a decision waiting to be
made. Nothing downstream should be written as though the answer is pending.

The specified behaviour, and it is the behaviour already written elsewhere in this plan
rather than a new mechanism:

- the run record carries `out_dir_mechanism: none` and `out_dir_loaded: false` for every
  affected member (§11);
- the artifact carries the workspace-level statement **"generated code was not indexed for
  N of M members; calls into generated code from those members are absent from this index,
  not proven absent from the code"** (§11).

**That sentence is the entire point of the ruling.** It lets a reader distinguish *not
indexed* from *not called*. Those are different facts about the world, and only one of
them is about the code. An index that simply showed fewer edges would collapse them and
would be making design.md §8's most dangerous claim by omission — presenting an artifact
of the tool's own configuration as a property of the user's code.

#### The consequence accepted with that decision

**Under this path the cross-repo client-stub leaf is unavailable in v0.1.** Stated plainly
because it is the visible cost: a handler that calls into generated code shows **fewer
outgoing edges than it has**, and design.md §4's measured stub leaf at
`target/debug/build/yadgar-task-.../out/yadgar.task.v1.rs:272` does not appear.

What that does and does not break:

- It **defers a v0.2 prerequisite** rather than breaking a v0.1 deliverable. design.md §7
  already puts cross-repo stitching in v0.2; ADR-0008 repeats the exclusion. Plan-04 §11's
  consumed roots take row 2 of its §9 — `Unbound`, with a reason naming the missing
  artifact — which is exactly the reported-gap shape that plan's root set is built around.
- It does **not** touch plan-01 §7.0's provider obligation, and it does **not** touch
  plan-01's traversal termination. The distinction drawn immediately above is load-bearing
  and must survive any future edit to this section: **the edge is absent, not
  unlocatable.** No node arrives at the traversal that cannot be located and therefore
  cannot be terminated at a boundary. A reachability gap and a termination gap are
  different failures, and only the first one is in play here.

### Cost, and why this is a CI artifact

MEASURED, design.md §8: `outgoingCalls` is one round trip per node; a whole-repository
walk is minutes, not seconds. Linking the engine removes the LSP round trip but not the
per-node query. ADR-0006's conclusion stands: **a build-time artifact, not an interactive
tool.**

### `inference_mode` is always `Resolved`, and what that costs

Every edge this crate emits is `InferenceMode::Resolved` — `ra_ap` answered directly.
`Lexical`, `TypeInferred` and `Enclosure` are never produced by the Rust plugin; they
exist for the languages ADR-0004 says must be written.

The honest consequence: **`EdgeTarget::Unresolved` is also never produced in v0.1.** When
`ra_ap` cannot resolve a call it returns no `CallItem` at all — there is no candidate set
to report. So an unresolved call is *invisible* rather than *reported*, which is in tension
with design.md §8's rule that a missing edge is shown as missing. See §12 and §14
open question 8.

---

## 10. Classifier

```rust
fn classify(&self, path: &Path, unit: &Unit) -> Category
```

### The measured observation this implements

MEASURED, design.md §8 — 20 edges from one handler, separating by path prefix alone, no
name matching and no confidence score:

| verdict | prefix observed | examples |
|---|---|---|
| first-party | `src/` | `tel_scope`, `passthrough` |
| cross-repo leaf | `target/debug/build/*/out/` | the generated client stub |
| in-org crate boundary | the org's own crates | `Call::start`, `call.run` |
| drop | `/nix/store/…rust-lib-src/` | `pin`, `map`, `trim`, `Ok`, `Err`, `Some` |
| drop | third-party crates | `into_inner`, `invalid_argument`, `encoded_len` |

That table is the **evidence that the five categories are the right five**. It is not the
implementation, and turning it into a literal prefix list would be a bug, because two of
those five prefixes are properties of the author's machine rather than of Rust:
`/nix/store/…rust-lib-src/` is the same file that lives at
`~/.rustup/toolchains/<toolchain>/lib/rustlib/src/rust/library/` on a rustup install and
under `/usr/lib/rustlib/src/` on a distribution package. Parameterising the nix string
would not fix it; it would just move the same trap.

### Implementation — structural, in this order

`ra_ap` already knows the sysroot and which crates are workspace members, so four of the
five categories are **queries, not prefixes**:

1. **`Stdlib`** — the file is under the *resolved* sysroot source directory, as reported by
   the loaded project model. Never a literal path.
2. **`Generated`** — the path contains a `target/<profile>/build/<pkg>-<hash>/out/`
   component, or is under an `OUT_DIR` recorded during the workspace load. This is the one
   genuine path-prefix rule, and it is genuinely Cargo-shaped, which is why it belongs in
   this crate (ADR-0008 leak 8).
3. **`FirstParty`** — the file's crate is a workspace member **and** belongs to the
   `unit`'s own **package**.
4. **`WorkspaceSibling`** — the file's crate is a workspace member of a different package.

Rules 3 and 4 compare **package identity, with the target qualifier stripped**, not unit
identity. §7 emits a separate `Unit` per target kind for a package with several, so
comparing unit ids would classify `src/lib.rs` as `WorkspaceSibling` while indexing the
`mycrate (test)` unit of the same package. That is the exact shape §13's `fx-impl` fixture
creates, and it is wrong: a package's own `src/` is first-party to every one of its
targets.
5. **`ThirdParty`** — everything else: registry, git and path dependencies outside the
   workspace.

Only rule 2 contains a literal. Rules 1, 3, 4 and 5 are facts the engine already holds.

The `drop` column above is **not** implemented here. This crate returns a `Category`; the
core and the renderer decide what to hide. A classifier that dropped edges would make a
policy decision invisible to the artifact.

### The unresolved case: "in-org crate boundary"

design.md §8's third row has no Cargo concept behind it. `yadgar_telemetry` is a separate
repository's crate; under the rules above it lands in `ThirdParty` alongside `tokio`,
which loses a distinction the measurement says is real and useful.

Candidate mechanisms, none verified, none free:

- Compare the dependency's source (git host / registry) against the repository's own
  `origin` remote host. INFERRED workable for git dependencies; says nothing for a private
  registry.
- A configured org prefix — rejected on sight: design.md §9 Q5's zero-configuration
  adoption bar, and ADR-0002's rule that the classifier is a plugin rather than user
  configuration.
- Ship `ThirdParty` for v0.1 and record the gap.

**v0.1 ships `ThirdParty` and records the gap.** §14 open question 7.

---

## 11. `preflight()`

```rust
fn preflight(&self, root: &Path) -> Preflight
```

ADR-0003 field 5: structured guidance, and **never `command -v`**.

### What preflight does *not* check, and why that is written down

It does not check for a `rust-analyzer` binary. **There is no external binary under
ADR-0001** — the engine is linked in. The check is absent by design, and the reason is
recorded here so nobody re-adds it as a "safety" measure:

MEASURED, design.md §8 and §10 — on the author's machine `command -v rust-analyzer`
**succeeds and proves nothing**. `type -a rust-analyzer` resolves to
`/etc/profiles/per-user/max/bin/rust-analyzer`, `readlink -f` shows it is
`rustup-1.29.0/bin/rustup` proxying to itself and looping, and
`rustup component list --installed` shows no rust-analyzer. A name resolving is not a
capability.

It also does not check for a proc-macro server binary, for the same reason (§4 D-C).

### Check 1 — a usable Cargo toolchain and a resolvable workspace

Two things, in order, and neither is a name lookup.

**1a. `cargo` responds.** Under §4 D-A the target language's build toolchain is carved out
of ADR-0001, so `cargo` may be invoked — but ADR-0003 field 5 still forbids proving it
exists by resolving its name. Run it and read the output (`cargo --version`, parse the
version line). The rustup proxy loop above is exactly a name that resolves and a binary
that does nothing.

```
Failed {
  reason: "cargo did not respond: {err}. reachgraph reads the workspace through cargo, \
           so a Rust repository cannot be analysed without it.",
  remediation: "install a Rust toolchain (https://rustup.rs) and re-run",
}
```

**1b. Manifest discovery** at or above `root`, then a successful workspace load.

```
Failed {
  reason: "no Cargo workspace at {root}: {err}",
  remediation: "point reachgraph at a directory containing Cargo.toml, or at any \
                member of the workspace you want indexed",
}
```

### Check 2 — build artifacts exist, because reachgraph will not create them

**§4 D-B is decided: reachgraph never runs a build.** So this check asks "was this built",
and it never asks "can we build it". Both of D-C's artifacts are the same prerequisite and
are checked together.

For every workspace member with a `build.rs`, look for a
`target/<profile>/build/<pkg>-<hash>/out/` directory containing at least one `.rs` file.
Separately, if in-process proc-macro expansion is wired (§4 D-C), look for the compiled
proc-macro dylib under `target/<profile>/deps/` for each proc-macro dependency.

```
Failed {
  reason: "{pkg} has a build script but no generated output under target/. reachgraph \
           does not run builds, so generated code — tonic client stubs among it — will \
           not be indexed and cross-repo leaves will not appear in the graph.",
  remediation: "cargo build --workspace once, then re-run reachgraph",
}
```

MEASURED basis: design.md §8 prerequisite 2, and the §4 stub path that resolved only
because `target/debug/build/…/out/` already existed.

### Degrading honestly: what `coverage` records

`Failed` refuses the run, which is correct when the user can fix it in one command. But a
partially-built workspace — some members built, others not — must still produce an
artifact, and **the artifact must say what it could not see**. This is D-B's "degrade
honestly" clause, and it is the same discipline ADR-0007 applies to root coverage: a gap
reaches the output, never only a log line.

The run record carries, per workspace member:

| field | meaning |
|---|---|
| `has_build_script` | the member declares a `build.rs` |
| `out_dir_loaded` | generated output was actually loaded into the VFS for this member — not merely present on disk (§9) |
| `proc_macro_expansion` | `in_process`, `disabled`, or `unavailable_not_built` |

and two workspace-level facts, both MEASURED in §9 to be real conditionals rather than
theoretical ones:

| field | meaning |
|---|---|
| `rust_src_available` | the sysroot source root resolved, so stdlib targets are locatable and classifiable. False means every stdlib call target is **external** in plan-01 §7.0's sense |
| `out_dir_mechanism` | `build_script_data`, `extra_includes`, or `none` — **which** mechanism put generated code in the VFS, not merely whether a directory existed on disk (§9) |

`out_dir_mechanism` is deliberately not a boolean. §9 MEASURED that an out-dir present on
disk is *still* unindexed when no mechanism loaded it, so a boolean named
a boolean named `out_dir_indexed` would answer a different question from the one a reader asks.

and one workspace-level statement: **generated code was not indexed for N of M members;
calls into generated code from those members are absent from this index, not proven
absent from the code.**

That wording is design.md §8's rule, extended to a second cause. A consumer that sees
`out_dir_loaded: false` knows the cross-repo leaves are missing by prerequisite rather
than by analysis, which is exactly the distinction plan-04 §9 row 2 relies on to phrase its
unbound consumed roots.

### Check 3 — proc-macro expansion mode is recorded, not refused

`Preflight` is binary: `Ok` or `Failed`. Degraded proc-macro expansion is not a reason to
refuse to run — MEASURED, design.md §4 shows what is *lost* (the `#[tonic::async_trait]`
crossing), and losing an edge class is a reported gap, not a broken run.

So: return `Ok`, and record the expansion mode in **both** places it matters — the run
record above, and `Provenance::engine`
(`"ra_ap_ide 0.0.352 (proc-macros: disabled)"`), so that a single edge carries the
provenance of the mode that produced it.

That has a cost: `Provenance::engine` becomes load-bearing text rather than a version
stamp. The field's grammar is therefore fixed here so the two halves do not collide:

```
engine := "<crate> <version>" [ " (" <mode> ")" ]
```

The version stamp is the **prefix** and is always present; any run-mode text is a
parenthesised suffix. §13's `engine_string_matches_pinned_version` therefore asserts the
prefix, not string equality — otherwise adding the mode suffix turns a green test red on
day one.

### Check 4 — `rust-src`, reported and never fatal

MEASURED (§9): sysroot source resolves only when the `rust-src` component is installed.
Without it, every stdlib call target has no `FileId`, cannot be located, and is external in
plan-01 §7.0's sense.

**This is never a `Failed`.** MEASURED, design.md §8's edge-noise table: stdlib targets
(`pin`, `map`, `trim`, `Ok`, `Err`, `Some`) are in the `drop` column. Refusing to run over
a component whose contribution is dropped anyway would be strictly worse for the user than
running. What is lost is the ability to *classify* those targets as `Stdlib` and drop them
deliberately rather than by absence — a real but small difference, and one that belongs in
the record rather than in an error.

So: check it, return `Ok`, and record it.

```
rust_src_available: false
remediation: "rustup component add rust-src — without it, calls into the standard \
              library cannot be located and are reported as external rather than \
              classified as stdlib"
```

The remediation text is carried in the run record beside the flag, so a reader who wonders
why a node is external finds the fix next to the symptom.

**The same `Preflight` type limitation as check 3.** `Preflight` is `Ok | Failed` and has
no warning variant (plan-00 §2), so a non-fatal finding cannot be expressed in the return
value at all. Both check 3 and check 4 therefore return `Ok` and route their finding to the
run record. Two checks now needing a shape the type does not have is weak evidence that
`Preflight` wants a third variant; it is **not** this plan's call to make, and it is logged
as §14 open question 11 rather than worked around by abusing `Failed`.

---

## 12. Known gaps, carried into the artifact

**Generics and trait dispatch.** rust-analyzer open issue **#19358**: call hierarchy misses
calls through generics. MEASURED, design.md §8: the probe crossed the boundaries in one
handler; gaps are expected elsewhere. The binding rule is design.md §8's and ADR-0004
repeats it: **show a missing edge as missing; never infer one to fill a hole.** §13's
`fx-generic` fixture asserts the gap rather than papering over it.

**Unresolved calls are invisible, not reported.** §9. `ra_ap` returns no `CallItem` for a
call it cannot resolve, so there is no candidate set and `EdgeTarget::Unresolved` is never
constructed by this crate. The waist supports the shape; Rust does not yet feed it.

Cheap v0.1 mitigation, and it is worth the small cost: emit per-unit counters —
functions queried, functions where `outgoing_calls` returned `None`, edges produced — into
the run record. A unit with 400 functions and 12 edges is then visibly suspicious rather
than quietly wrong. This is a count, not an inferred edge, so it does not reopen the
confidence-score failure mode (design.md §5).

**Generated code may be unindexed even when it is on disk.** Two distinct causes, and
§9 measured the second one:

- the repository was never built, so the artifacts do not exist (§4 D-B: reachgraph will
  not create them);
- the artifacts exist but no mechanism loaded them into the VFS — MEASURED, the out-dir
  enters only through build-script output data that `load_out_dirs_from_check: false`
  leaves empty.

**§9 D-D decides the outcome either way: v0.1 ships with generated code unindexed and
records it.** Consequences, all reported rather than silent (§11): cross-repo leaves do not
appear, plan-04's consumed roots take its §9 row 2, and — where the proc-macro dylibs are
also absent — the `#[tonic::async_trait]` crossing that design.md §4 MEASURED is lost.
Nothing is inferred around. The index states which members lacked which artifacts, and
states that the missing calls are **absent from the index, not proven absent from the
code**.

Keep the two failure modes apart when reading this: the edge is *absent*, not
*unlocatable*. plan-01 §7.0's provider obligation and plan-01's traversal termination are
untouched (§9).

**Async is not execution order.** design.md §8. The graph is static structure. Nothing in
this crate may label it a sequence.

**Rust is not representative.** ADR-0004 and ADR-0008 both say it and it belongs here too:
this crate is cheap and high quality because a complete engine already exists. No schedule
or quality expectation taken from it transfers to Go or Java.

---

## 13. Tests

Test-driven, standing project rule: failing test first, red → green → refactor. Three
tiers, and the split is deliberate — **plan-04 must never be forced onto Tier B's slow
path** (plan-04 §12 builds a hand-written `SymbolIndex` instead).

### Tier A — pure unit tests, no `ra_ap`, no filesystem

These run in milliseconds and are the bulk of the suite. Each requires the logic under
test to be a free function over plain data, which is itself the point: it forces the
engine-facing half and the decision-making half apart.

| test | asserts |
|---|---|
| `node_id_encode_decode_roundtrip` | `(unit, offset, path)` → `raw` → back, byte-identical. Cases: a path containing `\|`, a non-ASCII path, a path with spaces, offset 0, offset `u32::MAX`. |
| `node_id_is_a_pure_function_of_location` | the same `(unit, path, offset)` from `symbols_in` and from a `CallItem` target produce byte-identical `raw` |
| `symbol_kind_mapping_table` | every row of §8's table, including that `raw_kind` is preserved verbatim for `Trait`, `Impl`, `Macro`, `Static` |
| `impl_header_rendering` | `impl TaskService for Task`, `impl MockDb`, generic self type, trait with generic args — exact strings, since plan-04 parses them |
| `classifier_rules_over_synthetic_facts` | `classify_facts(&PathFacts) -> Category` over all five categories. Sysroot cases supplied as data: a nix store path, a rustup toolchain path, `/usr/lib/rustlib/src/`. **The test passes without any of those strings appearing in the source.** |
| `classifier_generated_prefix` | `target/debug/build/x-hash/out/y.rs` → `Generated`; `target/debug/deps/…` → not `Generated` |
| `preflight_failure_messages` | the exact `reason` and `remediation` strings of §11. Reviewing remediation text is the point; a test is how it gets reviewed. |
| `engine_string_matches_pinned_version` | `ENGINE` **starts with** `"ra_ap_ide <version>"` for the version pinned in `Cargo.toml` — a prefix assertion, per §11's grammar, so an appended run-mode suffix does not break it. Catches a re-vendor that forgot the stamp. |

### Tier B — integration, against checked-in fixture workspaces

`crates/reachgraph-lang-rust/tests/fixtures/`, each a tiny real Cargo workspace. Behind
`--features slow-tests`, because each loads a workspace through `cargo`.

**The test harness builds the fixtures; reachgraph does not.** §4 D-B is absolute, and it
applies to the test suite too — a plugin that built a fixture in order to index it would
be exercising a code path that cannot exist in production. So `fx-macro`, and any fixture
whose assertions need `OUT_DIR` contents or a compiled proc-macro dylib, are built by the
harness as a setup step *before* the plugin is invoked. That makes the prerequisite
explicit rather than incidental, and it gives the suite a second, free assertion: run
`fx-macro` **without** the setup step and `preflight()` must return the §11 check 2
`Failed`, with the run record marking `out_dir_loaded: false` and `out_dir_mechanism: none`. The degraded path is
tested, not merely described.

Fixtures are **small samples authored inside `reachgraph`**. `/home/max/git/yadgarhq/task`
is cited throughout this plan as measured evidence; no test may depend on it.

| fixture | shape | asserts |
|---|---|---|
| `fx-plain` | two crates, `a` calls `b` | `discover_units` → 2; one `Resolved` edge; `FirstParty` and `WorkspaceSibling`; `provenance.engine` populated |
| `fx-docs` | `///` multi-line block, `//!` module doc, `#[doc = "…"]` | full text, sigils stripped, **all** lines present. Explicitly asserts the failure design.md §5 measured: not last-line-only, not truncated at 83 chars, and a method's doc is non-empty |
| `fx-impl` | `trait Svc`; `impl Svc for Real` in `src/`; `impl Svc for Mock` in `tests/`; inherent `impl Real` | `container` set on every method; container `raw_kind` exactly `impl Svc for Real` / `impl Svc for Mock` / `impl Real`; `is_test` true only for the `tests/` one. **This is the fixture plan-04 binds against**, and its symbol dump is checked in as golden JSON for plan-04 to consume statically. |
| `fx-macro` | an attribute proc-macro crate in the workspace, plus a build script writing a module into `OUT_DIR`, with a call crossing both | the edge exists, and the target is classified `Generated`. **The expensive, load-bearing test**: it is the in-repo equivalent of design.md §4's measured result, and it is the test that fails if §4 D-C ends at `ProcMacroServerChoice::None`, and the test that must be run against both a built and an unbuilt fixture because §4 D-B forbids reachgraph building it. |
| `fx-generic` | a call dispatched through a generic parameter | **asserts the edge is absent**, citing rust-analyzer #19358, with a comment stating that an upstream fix makes this test fail and that the correct response is to delete the test and update §12 — not to relax the assertion |

### Tier C — properties over Tier B output

- `every_emitted_node_id_decodes` — over every symbol and every edge endpoint.
- `no_edge_target_is_unresolved_in_v01` — encodes §9's honest limitation, so a change to it
  is deliberate.
- `every_edge_carries_provenance_and_mode` — ADR-0003 field 4 never silently absent.
- `every_symbol_has_a_span` — `lang-rust` never emits `SourceRange { span: None }` (§6).
  The `None` case is the waist's, for resolvers that know a file but not an offset.
- `edge_targets_are_symbols_or_external` — every `EdgeTarget::Resolved` node id either
  appears in the emitted symbol set or was rejected by the `Vfs` lookup. Nothing resolves
  to a node that was located and then not emitted. This is plan-01 §7.0's provider
  obligation (§9) made executable, and it is the test that fails if a future change starts
  discarding out-of-unit targets.
- `dependency_and_stdlib_targets_are_emitted` — over `fx-plain` plus a fixture that calls
  into a registry dependency and into `std`. Asserts a `Symbol` for each located target,
  classified `ThirdParty` and `Stdlib`. The stdlib half is skipped, with an explicit
  message, when `rust-src` is unavailable — §9 measured that as a real conditional, and a
  test that silently passes on a machine without `rust-src` would hide it.
- `no_symbol_range_exceeds_file_length` — catches an encoding mix-up (leak 3) that ASCII
  fixtures would otherwise hide. `fx-docs` therefore contains non-ASCII doc text on purpose.

### Not tested here

Graph construction, reachability, sharding — plan-01, against the fixture plugin. Proto
parsing and handler binding — plan-04. HTML — plan-05.

---

## 14. Open questions

Named, not answered. Questions 1, 2 and 4 are **closed** and are kept with their
resolutions rather than deleted, so a later reader sees what was decided and on what.
Question 3 is the only one still blocking engine-facing code.

1. ~~**Does loading a Cargo workspace shell out to `cargo metadata`?**~~
   **CLOSED 2026-09-17 — the target language's build toolchain is carved out of
   ADR-0001** (§4 D-A). `load_workspace_at` is used as-is; no hand-written loader.
   `preflight()` verifies `cargo` responds (§11 check 1a).
2. ~~**`load_out_dirs_from_check`: does reachgraph run `cargo check` itself?**~~
   **CLOSED 2026-09-17 — reachgraph never runs a build** (§4 D-B). An unbuilt repository
   is indexed with holes and the holes are recorded in the run record (§11, §12).
3. **The proc-macro wiring seam, and the ADR-level question behind it.** (§4 D-C)
   MEASURED: `ra_ap_proc-macro-srv` 0.0.352 exposes `ProcMacroSrv::{new, expand,
   list_macros}` and expands **in-process** by `dlopen`-ing the compiled dylib — no
   subprocess. MEASURED: `ra_ap_load-cargo` offers no route to it; all three
   `ProcMacroServerChoice` variants spawn a binary or disable expansion. UNVERIFIED: the
   seam by which reachgraph supplies its own expander to the database instead of
   `load-cargo`'s `ProcMacroClient`. **Verify that seam first** — success means no
   subprocess is ever spawned and the ADR question never has to be asked. Only if the seam
   is closed does spawning a proc-macro server become a live proposal, and that is an
   ADR-level decision (the D-A carve-out does not cover it), not this plan's.
4. ~~**The exact doc accessor.**~~ **CLOSED 2026-09-17** — MEASURED
   `ra_ap_hir::Docs::{into_docs() -> String, docs() -> &str}`, no `Display` and no
   `AsRef` (§8). The call is `hir_docs(db).map(|d| d.docs().to_owned())`. Residual: the
   borrowed accessor's exact return-type spelling, and whether sigils arrive stripped —
   both asserted by `fx-docs` (§13) rather than researched further. Fallback unchanged:
   name plus signature (ADR-0005), never markup scraping.
5. **Are `AnalysisHost` and `Analysis` `Send` and `Sync`?** (§5) Decides whether `Loaded`
   stores a snapshot or mints one per call under the lock.
6. **The `ra_ap_hir::Impl` accessors for trait and self type**, and the same-name-trait
   collision (§8). The string contract's *content* is settled; the call that produces it is
   not, and the collision has no mitigation in v0.1.
7. **"In-org crate boundary" has no Cargo concept** (§10). Comparing dependency source
   hosts against the repository's `origin` is the only mechanism that does not require
   configuration, and it is unverified. v0.1 ships `ThirdParty`.
8. **Should this crate surface unresolved calls as `EdgeTarget::Unresolved`?** (§9, §12)
   The mechanism would be an AST pass over call expressions via `ra_ap_syntax`, diffed
   against the resolved set. It would make #19358's gaps visible instead of silent, which
   is what design.md §8 asks for. It is also a second analysis, and getting it wrong
   manufactures noise. v0.1 ships counters instead; v0.2 decides.
9. **The exact direct-dependency set among the ~48 `ra_ap_*` crates** (§2). Settled by the
   first compile, not by this list.
9a. **Does `ProjectWorkspace::extra_includes` put an `OUT_DIR` into the VFS without running
   build scripts?** (§9) **The highest-value unmeasured item in this plan**, and the only
   one here that documentation cannot settle. MEASURED: the field is public and documented
   as "Additional includes to add for the VFS"; MEASURED: the normal route is closed,
   because `WorkspaceBuildScripts` has private fields and only `Default`, so
   `set_build_scripts` cannot be handed data discovered on disk. Success recovers both
   design.md §4's cross-repo leaf and plan-04 §11's consumed-root binding, at no cost —
   which is why it is still worth measuring. **Failure is no longer a decision point**: §9
   D-D rules that v0.1 then ships with generated code unindexed and says so in coverage.
   Measure it; do not block on it.
9b. **plan-01 §11 question 8 is answered in §9** — partially, and the partition is the
   answer: workspace members and dependency lib sources resolve at no extra cost; stdlib
   resolves only with `rust-src` installed; `OUT_DIR` does not resolve under D-B, pending
   9a. plan-01 §7.0's termination argument is **not** threatened by the `OUT_DIR` gap —
   §9 explains why the unlocatable-but-reachable node does not arise there — but plan-01
   should read that reasoning rather than take the conclusion on trust.
10. **plan-00 §8 open question 1 is answered here** (§6): `edges_from` does not need a
    `&Unit`, because `NodeId::raw` is self-describing. It re-opens only if question 5
    forces a per-unit engine instance.
11. **`Preflight` has no warning variant.** (§11) Checks 3 and 4 both produce findings that
    are non-fatal but materially change what the output means, and `Ok | Failed` cannot
    express either — so both return `Ok` and route the finding to the run record. That
    works, and it leaves the structured remediation text (ADR-0003 field 5's actual point)
    outside the type designed to carry it. Two independent instances at n=1 is weak
    evidence for a third variant, not strong evidence; raise it at n=2, and never emit a
    `Failed` for a non-fatal finding in the meantime.
