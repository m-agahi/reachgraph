# Plan 06 — `reachgraph-cli`

**Status:** ready to build
**Date:** 2026-09-17
**Depends on:** plan-00 (traits, registry), plan-01 (the waist), plan-05 (the renderer)
**Blocks:** plan-07

The binary. It owns the plugin registry, the feature flags and the argument surface, and
nothing else. Every analysis decision belongs to a plugin; every graph decision belongs to
the waist.

---

## 1. Command surface

```
reachgraph <repo> [-o|--out <dir>]        analyse a repository → out/
reachgraph serve <out> [--port <n>]       static file server over an output directory
reachgraph preflight <repo>               run plugin preflight only, report, exit
reachgraph plugins                        list registered analysis plugins and renderers
reachgraph --version | --help
```

`reachgraph <repo>` is the default subcommand: a bare path argument analyses. design.md
§7's one-command shape (`<tool> ./path/to/repo`) is the adoption bar and is kept.

`reachgraph plugins` prints **two tables, not one** — analysis plugins with their
capabilities, detection markers and position encoding; renderers with their id alone. A
`Renderer` has no capabilities, no markers and no encoding to print (plan-00 §3.6), and
printing a column of dashes for it would re-suggest the shape the contract deleted.

### 1.1 Flags on the analyse path

| flag                            | default    | notes                                                                                                                                                                                                            |
| ------------------------------- | ---------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `-o, --out <dir>`               | `./out`    | ADR-0006's directory. Refuses to write into a non-empty directory it did not create unless `--force`; the output is regenerated, so a stale mixed directory is a real hazard.                                    |
| `--renderer <id>`               | `html`     | Looked up by `PluginId` in the **renderer registry**, which is separate from the analysis registry (§3.1). Not a match on a hardcoded list, and not a capability filter — `Capability::Render` no longer exists. |
| `--inline-threshold <bytes>`    | `5MiB`     | plan-05 §6.5.                                                                                                                                                                                                    |
| `--no-overview`                 | off        | Suppress `overview.html` even under threshold.                                                                                                                                                                   |
| `--contract <path>`             | repeatable | Files or directories the roots plugin should read. **Load-bearing for ADR-0007**: omitting a contract silently narrows coverage, which is the partial-index problem. §6 states what the CLI does about it.       |
| `--json`                        | off        | Machine-readable run report on stdout.                                                                                                                                                                           |
| `-q, --quiet` / `-v, --verbose` | —          | Progress verbosity (§5).                                                                                                                                                                                         |

### 1.2 Exit codes

| code | meaning                                     |
| ---- | ------------------------------------------- |
| 0    | analysis completed, artifact written        |
| 1    | internal error                              |
| 2    | preflight failed (§4)                       |
| 3    | no plugin detected for this repository (§3) |
| 4    | bad usage                                   |

**There is no exit code for "unreachable code was found", and no `--fail-on-unreachable`
flag.** This is deliberate and is not a v0.1 omission to be filled in later.

design.md §8: telling someone to delete working code is the one failure that permanently
destroys trust. ADR-0007 adds that a partial root set makes live code read as unreachable.
A CI job that fails the build on that output converts a knowingly-incomplete analysis
(rust-analyzer issue #19358 misses calls through generics, MEASURED in design.md §8) into
a merge blocker. The artifact reports; a human reads. If a gate is ever wanted, it belongs
downstream of a renderer that can express coverage, not in the exit code of the analyser.

---

## 2. `serve` is deliberately dumb

**A static file server over a directory. No state, no API, no queries, no database, no
templating, no analysis.** ADR-0006 specifies it, and specifies why it exists:

> INFERRED, `fetch()` against a `file://` origin is CORS-blocked in current Chrome and
> Firefox, so lazily-loaded shards cannot be read from a bare file open.

That is the entire reason. It is a convenience, not architecture, and it locks in nothing.

**It must never grow into an application server.** ADR-0006's rationale is not a taste
preference: _the graph cannot be computed at request time._ MEASURED (design.md §8), the
whole-repository walk is an offline cost — see §5 for the precise label — so a server could
only ever serve data that was precomputed by the analyse path. A stateful server would add
a store, an API, a deployment and an authentication problem for private code, and would buy
nothing until cross-commit diffing or incremental re-indexing exists, neither of which is
in scope.

### 2.1 Crate choice

| candidate                             | assessment                                                                                                                                                                                                                                                                  |
| ------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **`tower-http::ServeDir`** — _chosen_ | "Roughly 20 lines" is only true with `ServeDir`, because `ServeDir` **is** the traversal-safe path resolution plus the MIME table. It handles `..`, percent-encoded traversal, symlink escape and content types as library code that other people test.                     |
| `tiny_http` — fallback                | Smaller tree, no async runtime. But path resolution, the MIME table and the traversal guard become ours to write, and that is the one place a hand-rolled static server earns a CVE — in a tool whose entire risk surface is leaking a map of private source. Not 20 lines. |

The "keep async out of the binary" argument for `tiny_http` is weak against an INFERRED
40–80 MB `ra_ap_*` baseline (ADR-0001), so the dependency-tree argument does not carry the
decision on its own. It is still worth a number.

**OPEN MEASUREMENT — the cost of `ServeDir`, and the number that would flip this choice.**

```bash
cargo tree -p reachgraph-cli --no-default-features --features serve-tower -e normal | wc -l
cargo tree -p reachgraph-cli --no-default-features -e normal | wc -l
# and the binary delta, built both ways:
cargo build --release --locked && ls -l target/release/reachgraph
```

Flip to `tiny_http` if `ServeDir` adds more than ~5 MB to the stripped binary or pulls a
dependency tree comparable in size to the analysis path. Below that, library-tested
traversal safety wins. Either way §2.3's tests are written and must pass.

### 2.2 Binding

**Loopback only. `127.0.0.1`, with no `--bind` and no `--host` flag.**

ADR-0006: the artifact is a structural map of private source — file paths, function names,
doc text, service topology, and precisely which endpoints reach which code. Serving that
on `0.0.0.0` from a laptop on a shared network, or from a CI runner, is a disclosure with
no compensating benefit for a convenience command. A user who genuinely needs remote access
can forward a port deliberately, which is a decision they make and we do not make for them.

`--port <n>` exists, defaulting to an ephemeral free port, with the URL printed.

### 2.3 Anti-growth, enforced mechanically

Stating "it must not grow" in prose is exactly the good-intentions policy ADR-0008 rejects.
Three checks:

| check                               | mechanism                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| ----------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `serve_module_has_no_graph_imports` | source-level: `src/serve.rs` contains no `use reachgraph_core` and no `use reachgraph_plugin_api`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| `serve_module_stays_small`          | `src/serve.rs` is under **40 lines**. A blunt ratchet, deliberately. The number is **calibrated to the `ServeDir` design** of §2.1 — argument parsing, one `ServeDir`, one bind, one printed URL — and 40 is already generous for that. It is the check that fires when someone adds a query endpoint, and the person raising it has to justify it in review. A switch to `tiny_http` must re-justify this ceiling explicitly rather than silently inherit headroom, since hand-rolled resolution legitimately needs more lines and that is itself an argument against the switch. |
| `serve_has_no_outbound_http_client` | `cargo metadata`: `reqwest`, `ureq` and `curl` are absent from the cli's dependency graph. **Asserted by name, not as "no HTTP crate"** — with `ServeDir` chosen, hyper is in the tree and serves the inbound role.                                                                                                                                                                                                                                                                                                                                                                |

Traversal tests, required regardless of which crate is chosen: `../../etc/passwd`,
`%2e%2e%2f`, an absolute path, a symlink inside `out/` pointing outside it, and a path with
a NUL byte. Each must be refused, and refusal must not leak the resolved filesystem path
into the response.

---

## 3. Registry wiring — no language branch

ADR-0008: detection is plugin-declared from the first commit, and the core must not
contain

```rust
if is_rust_project(root) { ... }      // forbidden, ADR-0008
```

The cli builds **two** registries from Cargo features (plan-00 §1) and nothing else:

```rust
/// Analysis plugins only. `Registry` cannot hold a renderer (plan-00 §3.6).
fn analysis_registry() -> Registry {
    let mut r = Registry::new();
    #[cfg(feature = "lang-rust")]         r.register(Box::new(reachgraph_lang_rust::RustPlugin::new()));
    #[cfg(feature = "roots-proto-tonic")] r.register(Box::new(reachgraph_roots_proto_tonic::Plugin::new()));
    #[cfg(feature = "fixture")]           r.register(Box::new(reachgraph_fixture::FixturePlugin::new()));
    r
}

/// Output formats. A plain `PluginId -> &dyn Renderer` map. No detection, no preflight,
/// no capabilities — a `Renderer` has none of those (plan-00 §3.6).
fn renderer_registry() -> RendererRegistry {
    let mut r = RendererRegistry::new();
    #[cfg(feature = "render-html")] r.register(Box::new(reachgraph_render_html::HtmlRenderer::new()));
    r
}
```

**Two registries, not one, is the point rather than an inconvenience.** Plan-00 §3.6
removed `Capability::Render` and stopped `Renderer` extending `Plugin`, so there is no
type that could hold both. The separation also states the right thing about behaviour: an
analysis plugin is **detected** from a repository, and an output format is **asked for**.
`RendererRegistry` therefore has no `detect` method at all — not an unimplemented one, and
not one that returns everything.

v0.1 registers exactly **one real language plugin plus the fixture** (ADR-0008). A registry
with one entry costs nothing; a hardcoded branch costs a core change per language.

The `fixture` feature is never enabled in a release build (plan-00 §1). A test asserts
that, because a fixture plugin reachable from a shipped binary would let a hand-written
JSON file masquerade as an analysis.

### 3.1 Selection and dispatch

- **Language plugins:** `registry.detect(repo_root)` returns the matches. Zero matches →
  exit 3, printing what each registered plugin looks for (`Cargo.toml`, `.rs`), so the
  failure is diagnostic rather than "unsupported".
- **Renderers:** looked up in `renderer_registry()` by `PluginId`, via
  `RendererRegistry::select(id)`. An unknown id is a usage error listing the registered
  ids. The cli does not know that `html` is Cytoscape, and it cannot reach a renderer
  through `Registry::detect` even by mistake — the analysis registry holds no renderers.
- **Roots providers:** filtered by `Capability::Roots`. `--contract` paths are passed
  through; the cli never parses a `.proto`, never sees a tonic `impl`, and never extracts a
  version from a route (ADR-0007 forbids the last one in the waist, and the cli is not even
  the waist).

### 3.2 The guard test, scoped deliberately

`no_language_specific_tokens_in_core_or_cli` — asserts the identifiers `is_rust_project`,
`ra_ap`, `rust_analyzer` and the literal `Cargo.toml` appear nowhere in the non-test,
non-build-script sources of `reachgraph-core` and `reachgraph-cli`.

**The token list is deliberately narrow and must not be widened to `cargo`, `rustc` or
`.rs`.** Those appear legitimately in doc comments, error strings and this project's own
help text, so a broad scan false-positives immediately. A guard that cries wolf gets
disabled, and then there is no guard.

Paired with plan-00 §6.1's `cargo metadata` assertions, stated with the right
direct-versus-transitive precision:

- `ra_ap_*` absent from `reachgraph-core`'s dependency graph, **direct and transitive**.
- `ra_ap_*` absent from `reachgraph-cli`'s **direct** dependencies. It is present
  transitively via `lang-rust` and always will be; asserting otherwise would assert
  something false.

---

## 4. Preflight

ADR-0003 field 5. Run for every detected analysis plugin before analysis starts.

**The selected renderer is not preflighted.** `Renderer` has no `preflight` method
(plan-00 §3.6): it analyses nothing and has no prerequisite to check, and the `OutputSink`
owns the filesystem, so there is nothing for it to verify. An "ok" row for a renderer would
be a vacuous check reported as a passing one.

```
$ reachgraph ./some-repo
preflight
  rust                   FAIL  the repository has not been built
                               generated code under target/ is absent, so proto client
                               stubs will not resolve
                               remediation: run `cargo build` once in ./some-repo
  roots-proto-tonic      ok
error: 1 preflight check failed
```

Rules:

- **Never `command -v`.** MEASURED (design.md §10): `rust-analyzer` resolves on PATH on
  the author's machine and is a `rustup` proxy that loops and is not installed. A name
  resolving proves nothing. Under ADR-0001 there is no external binary to probe in the
  first place — the checks are about the _repository_ (has it been built, does the
  contract file exist, is the workspace manifest readable), not about the environment.
- **Structured, not a string.** `Preflight::Failed { reason, remediation }` (plan-00 §2).
  Both fields are rendered; a check that can fail without saying what to do about it is
  not finished.
- **Failure is fatal — exit 2.** No `--skip-preflight`. design.md §8's second hard
  prerequisite — the repository must have been built at least once — is MEASURED to change
  the _content_ of the answer, not merely its completeness: the cross-repo client-stub edge
  resolved only because `target/debug/build/…/out/` existed. Proceeding past that produces
  a graph that is quietly wrong, and quietly-wrong output is what ADR-0007 and design.md §8
  are both organised against.
- `reachgraph preflight <repo>` runs the same checks and exits without analysing, so CI can
  gate cheaply. `--json` emits the same table as structured data.

### 4.1 No subprocesses at all — mechanical guard

`no_process_spawn_in_workspace` — asserts `std::process::Command` appears in no non-test,
non-build-script first-party source in the workspace.

This is ADR-0001's "no external binaries, no subprocesses" as a build failure rather than a
convention. **The scoping is deliberate:** test harnesses legitimately spawn processes (a
packaging smoke test in plan-07 runs the built binary), and a build script may need to. A
scan that includes them fails on day one and gets deleted.

---

## 5. Progress and runtime expectations

### 5.1 The measurement, labelled correctly

**MEASURED (design.md §8), for the rejected architecture:** `callHierarchy/outgoingCalls`
over LSP costs one round trip per node, so a whole-repository walk is minutes, not seconds.

**That number does not transfer unchanged to what we are building.** ADR-0001 links
`ra_ap_ide` in-process, which removes the round trips entirely. What remains is
rust-analyzer's own analysis cost — loading the workspace, expanding macros, type-checking
— which **nobody has measured**. It could be faster or slower than the LSP figure.

**OPEN MEASUREMENT — in-process wall time on a real repository.** Blocks the README's
runtime claim.

```bash
# after plan-03 lands, on a repository that has been built once:
/usr/bin/time -v ./target/release/reachgraph ./some-repo -o /tmp/out
# record: wall time, peak RSS, node count, edge count, and the per-phase breakdown
#         printed by the run report (§5.3)
```

Report it per phase, because "unit discovery took 4 s and symbol extraction took 6 min" is
actionable and a single total is not.

**The conclusion survives either way.** ADR-0006 rests on "the graph cannot be computed at
request time", and any figure in minutes — or even in tens of seconds — supports that.
Only the label changes. But shipping a MEASURED tag on a number measured against a
different architecture is precisely the error this project's own premise forbids, so the
README says:

> reachgraph analyses a whole repository in one pass. It is a CI-generated artefact, not
> an interactive tool: run it in a workflow and read the output, rather than expecting it
> to answer a question while you wait.

— with a concrete figure added once the measurement above exists, and not before.

### 5.2 Progress output

- Progress goes to **stderr**. Artifacts go to disk. `--json` machine output goes to
  **stdout**. A pipeline consuming stdout must never receive a spinner.
- **Phase-based with real counts, never a synthetic percentage.** `symbols: 1240 in 18
units` is true; `62%` is invented, because the denominator is unknown until the phase
  ends. Phases: detect → preflight → discover units → symbols → edges → roots → graph
  build → reachability → render.
- Non-TTY (CI) → one line per phase completion, no redraw, no ANSI. `-q` suppresses all but
  errors.

### 5.3 Run report

Printed at the end and emitted as `run.json` beside the artifact:

```
analysed ./some-repo in 4m12s
  units 18   symbols 1240   edges 3180   roots 14 (2 unbound)
  unresolved edge targets 47
  not reachable from any endpoint version in this index: 96 symbols
  coverage: 2 contracts, 3 versions
wrote ./out (index.html, 14 shards, overview.html)
```

Two lines earn their place. **`roots 14 (2 unbound)`** surfaces plan-00's `RootBinding`
gaps at the top level, because an unbound root is why a real handler may be sitting in the
unreachable list. **`unresolved edge targets 47`** surfaces design.md §8's missing-edge
rule at the summary level — a large number there means the "not reachable" claim is weaker
than it looks, and the reader should see that without opening the artifact.

The unreachable line uses the binding wording verbatim (ADR-0007). The word "dead" appears
nowhere in this crate's output. The wording guard test is scoped exactly as plan-05 §8.4
scopes it — this crate's own string literals, not the analysed repository's text.

---

## 6. Privacy warning, at the point of use

ADR-0006 makes this binding. The warning is printed **after a successful run, adjacent to
the output path**, where the user is deciding what to do with the directory — not buried in
a README section nobody opens.

```
note: ./out is a structural map of this repository. It contains file paths, function and
      method names, doc comment text, service topology, and which endpoints reach which
      code. Treat it with the same care as the source.
      GitHub Pages publishes publicly on Free and Pro repositories. Do not publish this
      from a private repository.
```

Supporting rules:

- `serve` prints that it is bound to loopback only (§2.2).
- **The tool offers no hosted or upload-based delivery path** (ADR-0006). §2.3's
  no-outbound-client test is that prohibition made mechanical: there is no HTTP client in
  the binary, so there is nothing to add an upload flag to without a dependency change that
  shows up in review.
- The README documents the private-repository path as ADR-0006 states it — a CI workflow
  artifact zip, which respects repository permissions and is retention-limited, downloaded
  and opened locally or via `reachgraph serve`. GitHub Pages is opt-in, documented as
  open-source-only, and the tool never configures it.

### 6.1 Coverage is a first-class part of the report

When `--contract` narrows what was read, or a roots provider returns an `Unbound` root, the
run report says so (§5.3) and the artifact records it (plan-05 §4.5). ADR-0007's
partial-index problem is a correctness issue: if the `v1` contract was not passed, every
`v1`-only function appears unreachable. The cli cannot detect what it was not given — it
can only state precisely what it covered, loudly enough that a reader notices the gap.

---

## 7. Tests

Test-driven: failing first. The cli's tests run against `reachgraph-fixture` (ADR-0008) and
a temporary directory — no real repository, no indexing, no timing.

### 7.1 Command surface

| test                                          | asserts                                                                   |
| --------------------------------------------- | ------------------------------------------------------------------------- |
| `bare_path_analyses_to_default_out`           | `reachgraph <repo>` writes `./out` with ADR-0006's layout                 |
| `out_flag_redirects_output`                   | `-o` honoured; no write outside it                                        |
| `refuses_dirty_out_without_force`             | pre-existing foreign files → error, no partial write                      |
| `no_fail_on_unreachable_flag_exists`          | §1.2: the flag is absent, and a run producing unreachable symbols exits 0 |
| `exit_3_when_no_plugin_detects`               | empty directory → exit 3, message lists each plugin's markers             |
| `json_report_is_on_stdout_progress_on_stderr` | streams do not cross                                                      |

### 7.2 Registry and neutrality

| test                                | asserts                                                                                                                                                              |
| ----------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `registry_has_no_language_branch`   | §3.2, with the narrow token list                                                                                                                                     |
| `renderer_selected_by_id`           | a second registered renderer is selectable by `--renderer <id>` with no cli change; an unknown id is a usage error that lists the registered ids                     |
| `detect_never_returns_a_renderer`   | `Registry::detect` on any fixture repository yields no renderer — enforced by type (plan-00 §3.6), asserted so a future merge of the two registries fails here first |
| `fixture_absent_from_release_build` | the `fixture` feature is off in the release profile                                                                                                                  |
| `ra_ap_not_a_direct_cli_dependency` | §3, direct-only assertion                                                                                                                                            |
| `no_process_spawn_in_workspace`     | §4.1, scoped                                                                                                                                                         |

### 7.3 Preflight

| test                                      | asserts                                                                       |
| ----------------------------------------- | ----------------------------------------------------------------------------- |
| `preflight_failure_exits_2`               | fixture plugin returning `Failed` → exit 2, nothing analysed, nothing written |
| `preflight_prints_reason_and_remediation` | both fields reach the output                                                  |
| `preflight_subcommand_does_not_analyse`   | no artifact written                                                           |
| `preflight_json_is_machine_readable`      | parses, one entry per analysis plugin                                         |
| `preflight_has_no_renderer_row`           | §4: the selected renderer produces no row, not an `ok` row                    |

### 7.4 `serve`

With `ServeDir` chosen (§2.1), the traversal and binding tests **assert the composition,
not our own resolution logic** — the library does the resolving. They are kept as
regression guards, and they are exactly what makes a later switch to `tiny_http` safe: the
day the crate changes, these are already written and the hand-rolled path has to satisfy
them. Nobody reading this table should go looking for our traversal code; there is none.

| test                                | asserts                                                                                            |
| ----------------------------------- | -------------------------------------------------------------------------------------------------- |
| `serve_returns_shard_json`          | `GET /graph/<slug>.json` → 200, correct content type                                               |
| `serve_rejects_parent_traversal`    | `../../etc/passwd`, `%2e%2e%2f`, absolute path, embedded NUL → 4xx, no filesystem path in the body |
| `serve_rejects_symlink_escape`      | symlink inside `out/` pointing outside → refused                                                   |
| `serve_binds_loopback_only`         | listener address is `127.0.0.1`; no flag can change it                                             |
| `serve_module_has_no_graph_imports` | §2.3                                                                                               |
| `serve_module_stays_small`          | §2.3, the 120-line ratchet                                                                         |
| `serve_has_no_outbound_http_client` | §2.3, by crate name                                                                                |

### 7.5 Report and privacy

| test                               | asserts                                                           |
| ---------------------------------- | ----------------------------------------------------------------- |
| `report_uses_binding_wording`      | the unreachable line is ADR-0007's exact sentence                 |
| `cli_authors_no_dead_wording`      | scoped as plan-05 §8.4 — this crate's own literals only           |
| `privacy_note_printed_on_success`  | present on stderr after a successful run                          |
| `report_surfaces_unbound_roots`    | `roots N (M unbound)` when the fixture supplies an `Unbound` root |
| `report_surfaces_unresolved_edges` | count matches the fixture's unresolved targets                    |

### 7.6 Not tested here

Analysis quality (plan-03), proto binding (plan-04), rendered page behaviour (plan-05 §8.1
— there is no browser harness), wheel installation (plan-07).

---

## 8. Open questions

1. **Does `--contract` belong on the cli at all, or should the roots plugin discover
   contracts itself?** Discovery is the zero-configuration answer (design.md §9 Q5) and is
   what a plugin-declared design implies. An explicit flag is what a monorepo with fifty
   `.proto` files needs. Leaning: plugin discovers by default, `--contract` narrows or
   adds. Settle in plan-04, since the answer depends on what proto discovery actually costs.
2. **Should `reachgraph <repo>` accept more than one repository?** Cross-repository
   stitching is explicitly out of v0.1 (ADR-0008), and accepting multiple paths now would
   invite a half-implementation of it. Single path only, and the flag shape is not reserved.
3. **Where does `run.json` live — inside `out/` or beside it?** Inside makes the artifact
   self-describing; beside keeps `out/` exactly ADR-0006's four entries plus `vendor/`.
   Leaning inside, since a reviewer opening a downloaded zip should see the run's coverage
   without a second file. Decide before plan-07's smoke test asserts a file list.
