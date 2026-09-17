# ADR-0001: Single self-contained binary, no external tooling

**Status:** Accepted
**Date:** 2026-09-17

## Context

`reachgraph` builds an endpoint-rooted call graph across several languages. Call edges
have to come from somewhere, and the two candidate shapes are fundamentally different:

- **Shell out.** Drive an existing language server (`gopls`, `basedpyright`, `jdtls`,
  `rust-analyzer`) over LSP as a subprocess. The analysis is rented; the binaries are not
  ours.
- **Import.** Link the analysis engine into our own binary, or write it ourselves.

The shell-out path measured extremely well on capability. MEASURED 2026-09-17 (subagent
probe, LSP `initialize` + live `callHierarchy/outgoingCalls` against hello-world samples):
`callHierarchyProvider: true` **and real returned edges** for rust-analyzer,
`gopls` 0.23.0, `pyright` 1.1.411, `basedpyright` 1.39.8,
`typescript-language-server` 5.3.0, `clangd` 21.1.8 and `jdtls` 1.60.0. Only the secondary
Python servers lacked it — `jedi-language-server` 0.47.0 and `python-lsp-server` 1.14.0
both returned `-32601 Method Not Found: textDocument/prepareCallHierarchy`.

That path is nevertheless rejected. Its cost is an acquisition problem: every user must
end up with the right server, at the right version, without installing anything by hand.
MEASURED 2026-09-17, the cheapest concrete instance of that cost — Python:

| package | self-contained? | evidence |
|---|---|---|
| `pyright` (PyPI) | **No.** Third-party wrapper (`RobertCraigie/pyright-python`, not Microsoft). Prefers a global `node` on PATH, else downloads Node via `nodeenv` at first run | source read of `src/pyright/node.py` |
| `basedpyright` (PyPI) | **Yes.** Hard-depends on `nodejs-wheel-binaries`, no fallback path | source read of `basedpyright/run_node.py` |
| `nodejs-wheel-binaries` | Real per-platform wheels with an embedded Node binary | PyPI JSON: 61.1 MB manylinux x86_64, 63.3 MB musllinux, 56.0 MB macOS arm64, 42.4 MB win_amd64 |

So the only self-contained Python option drags a 56–63 MB third-party repackaging of
Node.js into the dependency chain. npm and Node are the largest supply-chain surface in
the set, and a runtime download at first run is worse still.

## Decision

**One Rust binary. No external binaries, no subprocesses, no runtime downloads, no system
package manager, no nix.** Distribution is PyPI, one per-platform wheel containing the
binary.

Where a Rust-native analysis engine exists, **import it**. Where it does not, **write our
own**. See ADR-0004 for the per-language consequences of that split.

### Vendor, do not fork

For rust-analyzer specifically the engine is available as ordinary crates — MEASURED
2026-09-17: ~48 `ra_ap_*` crates on crates.io at version 0.0.352, republished 2026-09-14,
with `ra_ap_ide::Analysis::{call_hierarchy, incoming_calls, outgoing_calls}` as public
functions.

"Copy the source and maintain it ourselves" is the wrong mechanism *here*: rust-analyzer
is ~48 crates on a fast release cadence, and copying them means owning a fork of a
moving project. That is Sourcetrail's failure mode in milder form — not writing indexers,
but maintaining forks of them.

The decision instead:

- Depend on `ra_ap_*` from crates.io.
- Pin exact versions.
- `cargo vendor` the sources into the repository.
- Build with `--locked`.

This yields auditable source in-tree and no network fetch at build time — supply-chain
determinism — without owning the fork. An upstream update becomes a deliberate re-vendor,
never a merge conflict.

The same policy applies to every imported analysis crate, and the measured evidence says
it is necessary rather than cautious. MEASURED 2026-09-17: `ra_ap_*` is 0.0.x republished
weekly in lockstep with rust-analyzer nightlies (2026-09-14, 2026-09-07, 2026-08-31);
`ruff_python_semantic` and `ty_python_semantic` (ADR-0004) publish on the same weekly
rhythm and both self-describe as "an internal component crate of Ruff". **Neither family
offers a semver stability promise.** Pinning exactly and vendoring converts that volatility
from a build-breaking surprise into a scheduled task.

Note that "internal component crate" is an API-stability caveat, not a usage restriction —
both are MIT and the licence text imposes no such limit.

### Licence

**MIT OR Apache-2.0** (dual), the norm for a developer tool.

MEASURED 2026-09-17, licences of the candidate sources:

| project | SPDX | copyable into an MIT/Apache tool? |
|---|---|---|
| rust-analyzer (`ra_ap_*`) | MIT OR Apache-2.0 | yes |
| ruff (`ruff_python_semantic`) | MIT | yes |
| ty (`ty_python_semantic`) | MIT | yes |
| tree-sitter core + grammars | MIT | yes |
| gopls (golang/tools) | BSD-3-Clause | yes |
| pyright | MIT | yes |
| clangd / LLVM | `Apache-2.0 WITH LLVM-exception` | yes |
| Eclipse JDT LS | EPL-2.0 | file-level copyleft — avoid |
| crabviz | AGPL-3.0 | **no — legally unavailable** |

Two of these required reading the licence file rather than trusting metadata. MEASURED
2026-09-17: GitHub's licence detector returns `NOASSERTION` for both `microsoft/pyright`
and `llvm/llvm-project`; reading `LICENSE.txt` and `LICENSE.TXT` respectively confirms MIT
and `Apache-2.0 WITH LLVM-exception`.

**crabviz is not merely "do not copy" — it is legally unavailable to this project.**
INFERRED legal conclusion from a MEASURED SPDX identifier; not legal advice. Under
ADR-0001 the product is a *single linked binary*. Vendoring any crabviz source into it
would place the entire binary under AGPL-3.0, and AGPL extends the copyleft trigger to
network and SaaS use, not only to distribution. That is incompatible with MIT OR
Apache-2.0 distribution. The architecture may be borrowed — `docs/design.md` §3 openly
derives its four-layer split from crabviz, and that is fine. Not one line of its code may
be.

**Eclipse JDT LS (EPL-2.0)** is file-level copyleft: vendored files remain EPL-2.0. It is
legally vendorable, but produces a mixed-licence codebase with disclosure obligations. It
is moot in any case — JDT is Java source and cannot link into a Rust binary.

**The sharp point: language mismatch, not licence, is the blocker on copying source.**
Almost every analyzer above is permissively licensed. None of gopls, pyright or jdtls can
be compiled into a Rust binary, because they are Go, TypeScript and Java respectively.
Permission was never the constraint.

### Toolchain carve-out

User decision, 2026-09-17, prompted by a collision found while planning the Rust plugin:
`ra_ap_load-cargo` needs `cargo` to discover a workspace, which under the absolute reading
of this ADR is both an external binary and a subprocess. The carve-out is taken rather than
the absolute reading.

**Still banned, unchanged:**

- Analysis tooling that reachgraph would make a user install *for reachgraph's sake* —
  rust-analyzer, gopls, jdtls, pyright.
- Runtime downloads.
- npm and Node.
- nix, or any system package manager.
- Install-time fetches.

**Carved out:** the **target language's own build toolchain** — `cargo` today, `go` and
`javac` later.

#### The test that decides future cases

> **A tool is carved out when it is a precondition of the repository being analyzable at
> all, not a thing installed for reachgraph.**

You cannot analyze a Rust repository on a machine with no Rust toolchain, because the
repository does not build there. `cargo` is intrinsic to the repository. `rust-analyzer`
was intrinsic to reachgraph. That is the entire distinction, and it generalizes cleanly to
every language added later.

#### Consequences of the carve-out

- `preflight()` (ADR-0003 field 5) verifies the toolchain and returns structured
  remediation. **Never `command -v`** — MEASURED, `docs/design.md` §8: a bare
  `command -v rust-analyzer` succeeded on the author's machine against a `rustup` proxy
  that loops and is not installed. A name resolving proves nothing.
- The supply-chain posture is materially unchanged. The carve-out adds no download, no
  npm, and no reachgraph-specific install.

#### Open — not settled by this carve-out

Whether rust-analyzer's proc-macro expansion needs a subprocess. MEASURED:
`ra_ap_proc-macro-srv` 0.0.352 is a library crate with no binaries, so in-process linking
is plausible — but unverified.

A proc-macro server is **rust-analyzer's own component, not the target language's build
toolchain**, so it is *not* covered by the carve-out as worded. If it turns out to require
a subprocess, that is a separate ADR-level decision, not an extension of this one.

The stakes are specific: `ProcMacroServerChoice::None` would kill expansion through
`#[tonic::async_trait]`, and MEASURED (`docs/design.md` §4) that crossing is what the
cross-repo mechanism rests on.

### reachgraph never runs a build

Second user decision, 2026-09-17, recorded here because it bounds the carve-out above.

**reachgraph does not build the repository.** No `cargo check`, no
`load_out_dirs_from_check`-driven build, no invocation of the toolchain to *produce*
artifacts. The carve-out permits reading a workspace, not constructing one.

When `target/*/out/` is absent, `preflight()` reports it and the run **degrades honestly**:
cross-repo leaves are not indexed, and that fact is recorded in `coverage` rather than
being silently absent from the output.

This matches the second hard prerequisite already stated in `docs/design.md` §8 — MEASURED
there that the client-stub edge resolved only because `target/debug/build/…/out/` existed,
and that a fresh clone has no generated proto code and therefore no visible cross-repo
leaf.

## Consequences

**Gained, precisely:**

- No npm and no Node anywhere in the chain.
- No runtime download, no install-time fetch, no post-install script.
- No user-installed tool of unknown provenance or version.
- A single hashable, signable, attestable artifact per platform.
- The **reachgraph-specific** prerequisite class from `docs/design.md` §8 disappears —
  including the rustup-proxy-loop trap, where `command -v rust-analyzer` succeeds and
  proves nothing. Nobody installs a tool for reachgraph's sake. This makes the
  zero-configuration adoption bar (§9 Q5) reachable in a way the shell-out path never
  was.

  Two prerequisites from §8 survive, both intrinsic to the repository rather than to
  reachgraph: the target language's build toolchain (the carve-out above) and the
  requirement that the repository has been built at least once for generated code to
  exist. Both are reported by `preflight()`, not assumed.

**Residual, honestly:**

- crates.io dependencies remain. Vendoring makes them deterministic and auditable; it
  does not make them absent.
- **CVEs in vendored code become ours to patch and re-release.** This is a real,
  recurring maintenance obligation, not a one-time cost.
- **Vendored JavaScript is invisible to Rust tooling.** Plan-05 vendors Cytoscape's UMD
  bundles in-tree and compiles them in with `include_str!`, because a CDN `<script src>`
  is a runtime download performed by the browser rather than by the binary. Removing npm
  also removed npm's advisory tooling: neither `cargo-audit` nor `cargo-deny` can see a
  CVE in that JavaScript. The check is manual and belongs in the release checklist
  (plan-07). The posture remains far better than an npm dependency chain — the point is
  that this residual is named now rather than discovered later.
- INFERRED: linking `ra_ap_*` plus tree-sitter grammars produces a large binary.
  rust-analyzer's own release asset is 14.8 MB gzipped (MEASURED 2026-09-17), so
  plausibly 40–80 MB unpacked. MEASURED: PyPI's default per-file limit is 100.0 MiB and
  individual projects may request an increase. Rust and Python look comfortable; Java may
  not.
- Go and Java gain no imported engine at all and must be written from scratch
  (ADR-0004).

## Rejected alternatives

**Shell out to language servers, shipped inside wheels.** Rejected despite measuring best
on raw capability. It re-introduces an acquisition problem for every language, and the
Python instance of it costs a 56–63 MB third-party Node repackaging. MEASURED 2026-09-17,
the Go instance is worse: `gopls` publishes **zero** binary release assets (`go install`
only) and there is no PyPI package named `gopls`, so we would have to become the
publisher of a `gopls`-binaries wheel ourselves.

**Require the user to install language servers.** Rejected on the adoption bar. Every
dead tool in `docs/design.md` §6 demanded setup before it returned value.

**Fork rust-analyzer.** Rejected in favour of pinned-and-vendored dependencies, above.
