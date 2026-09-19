# ADR-0002: Compile-time plugin architecture

**Status:** Accepted
**Date:** 2026-09-17

## Context

The project goal is that every piece is a plugin, so each can be developed and maintained
independently and language support can be added one language at a time.

Under a shell-out architecture the natural plugin mechanism was **subprocess JSON-RPC over
stdio**, mirroring LSP. The justification was specific and good: a plugin author writes
the plugin in the language it indexes, so the person who knows Go tooling writes the Go
plugin in Go. Rust has no stable ABI, so dynamic loading would otherwise mean
`abi_stable`/`stabby` and per-platform plugin builds; WASM components would mean a
wasmtime dependency and a wasm toolchain for authors. Subprocesses avoid all of that, and
IPC overhead is irrelevant because the workload is already IO-bound.

ADR-0001 removes that justification entirely. With one self-contained binary and no
subprocesses, every plugin is Rust compiled into the same artifact. Nobody writes a Go
plugin in Go, because there is no process for it to run in.

## Decision

**Plugins are Rust crates in a Cargo workspace, behind Cargo features, implementing traits
defined by the core.**

Modularity for development and maintenance — the actual stated goal — is fully preserved:
one crate per plugin, independently testable, added one at a time. What is lost is
_runtime_ extensibility by third parties.

That loss is not a compromise. One binary and supply-chain control are **incompatible**
with loading third-party plugins at runtime: an artifact that loads arbitrary foreign code
is not a self-contained artifact. The goals agree rather than conflict.

### Five plugin kinds

| kind                   | varies by               | example                                                       |
| ---------------------- | ----------------------- | ------------------------------------------------------------- |
| symbol/doc provider    | language                | `ra_ap_ide` for Rust; tree-sitter extraction (ADR-0005)       |
| call-edge provider     | language                | `ra_ap_ide::Analysis::outgoing_calls`; hand-written resolvers |
| root/contract provider | language × framework    | proto+tonic, OpenAPI+axum, FastAPI decorators                 |
| classifier             | language × build system | first-party vs generated vs vendored vs stdlib                |
| renderer               | nothing                 | static HTML, JSON, DOT, SARIF, PR comment                     |

Symbol/doc and call-edge are **separate kinds**, not one "language plugin". Rust already
proves the split is real: one engine may supply both, or two sources may supply one each.
A plugin declares which kinds it provides (ADR-0003, field 1) so a single crate can
satisfy several without being invoked twice.

**The classifier is a plugin, not user configuration.** The path-prefix table in
`docs/design.md` §8 — `src/` first-party, `target/debug/build/*/out/` generated,
`/nix/store/…rust-lib-src/` drop — is Rust-and-Cargo-specific. Go needs `vendor/` and
`$GOPATH/pkg/mod/`; TypeScript needs `node_modules/`; Python needs `site-packages/`.
Pushing that knowledge onto the user directly contradicts the zero-configuration adoption
bar in `docs/design.md` §9 Q5.

### Language order

**Rust → Python → Go → Java.**

Ordered by descending import-availability, which ADR-0004 measures. Rust imports a
complete engine today. Python may import one. Go and Java import nothing and must be
written.

**v0.1 ships Rust only** (ADR-0008). The ordering above remains the plan, but only the
first entry is implemented. In the interim, a **fixture plugin** — a `LanguagePlugin`
implementation that reads hand-written JSON rather than analysing a language — supplies a
mechanical second implementation of every trait. Its purpose is to turn a leaked
rust-analyzer-ism into a compile error rather than a month-nine discovery. See ADR-0008
for the eight specific leaks the traits must guard against.

### The interface stays unstable until n=3

The plugin traits are marked **unstable and free to break** until three languages are
implemented. Two languages can be a coincidence; three reveals the axis.

Because there is no third-party plugin ecosystem to break, an interface change is a
workspace-wide refactor rather than a breaking release. This is a concrete benefit of the
compile-time choice: it substantially defuses the risk of freezing an interface against
n=1, which was the main hazard of designing plugins before a second language exists. The
six fields in ADR-0003 still matter, but they are now cheap to revise rather than
permanent.

**The fixture plugin does not count toward n=3.** It is a synthetic consumer: it exercises
the _shape_ of an interface, never the awkwardness of a real language's semantics. Three
means three real languages.

**Languages 2 and 3 must be written by the maintainers, not by contributors.** The point
of the exercise is to break our own interface before anyone depends on it. A
contributor-written language 2 means learning the interface is wrong _and_ owing someone a
migration.

## Consequences

- Adding a language requires a rebuild and a release. Acceptable: the artifact is a
  per-platform wheel, and users install it from PyPI rather than composing it themselves.
- No third-party plugins. Contributions arrive as pull requests against the workspace,
  which is also where the supply-chain review happens.
- Binary size grows with every language compiled in. Cargo features allow trimmed builds;
  INFERRED that the published wheel enables all supported languages by default, since
  per-language wheels would recreate the acquisition problem ADR-0001 exists to remove.
- A stdio subprocess plugin kind remains _possible_ later as an escape hatch for a
  language nobody wants to port. It would be opt-in and off by default, and it would
  forfeit the ADR-0001 guarantees for anyone who enables it.

### Carried-forward evidence: one LSP client, never one per plugin

MEASURED 2026-09-17 (subagent probe): `gopls` sends `window/workDoneProgress/create`
during `initialize` as a server-to-client **request** — it carries an `id` and requires a
response — not a notification. A client that only listens for notifications and never
replies to inbound server requests hangs silently on the subsequent
`prepareCallHierarchy`. The probe reproduced the hang and fixed it by adding generic
inbound-request acknowledgement.

This is recorded because it is the decisive argument against per-plugin LSP clients: every
plugin author would otherwise rediscover it independently. If any LSP-backed path is ever
revived (the escape hatch above), the client belongs in the core, once, and a language
plugin reduces to a declarative manifest — binary, arguments, root marker.

## Rejected alternatives

**Subprocess JSON-RPC over stdio.** The right answer under the shell-out reading, and
rejected only because ADR-0001 removed its premise. Retained as a possible future escape
hatch.

**Dynamic libraries (`dylib` + `abi_stable`/`stabby`).** Rust has no stable ABI; this
means unsafe FFI boundaries and per-platform plugin builds, for extensibility ADR-0001
already rules out.

**WASM components (wasmtime + WIT).** Sandboxed and language-agnostic for authors, but it
adds a large runtime dependency and a wasm toolchain requirement, again to enable runtime
extensibility that is incompatible with the self-contained artifact.

**Making everything a plugin, including the graph and the reachability algorithm.**
Rejected — see ADR-0003. A system with no fixed point has no contract to version.
