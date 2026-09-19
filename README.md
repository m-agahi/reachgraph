# reachgraph

An endpoint-rooted call graph for a repository, emitted as a static artifact.

reachgraph starts from the endpoints a service actually exposes — a gRPC method, an HTTP
route — and walks the call graph outwards from each one. What it produces is not "every
function and who calls it" but an answer to a narrower and more useful question: **which
code is reachable from which endpoint version, and which code is reachable from none of
them.**

It is one binary. It installs from PyPI, links its analysis engine in-process, downloads
nothing at run time, and runs no subprocess of its own beyond the target language's own
build toolchain (ADR-0001).

## Install

```console
$ pip install reachgraph
$ reachgraph --version
```

The wheel contains an executable and no Python. Nothing is importable; nothing is
compiled at install time.

## Use

```console
$ reachgraph ./some-repo -o ./out
$ reachgraph serve ./out
```

`./out` is a static directory: `index.html`, one shard per root under `graph/`, a
single-file `overview.html`, and the machine-readable `endpoints.json`,
`unreachable.json` and `run.json` beside them. `serve` exists only because `file://`
blocks `fetch()` for the sharded pages — `overview.html` opens straight from disk
(ADR-0006).

**The artifact is a structural map of your repository.** It carries file paths, function
and method names, doc comment text and service topology. Treat it with the same care as
the source, and read the warning the binary prints before publishing one anywhere.

## Runtime

reachgraph analyses a whole repository in one pass. It is a CI-generated artefact, not an
interactive tool: run it in a workflow and read the output, rather than expecting it to
answer a question while you wait.

**MEASURED 2026-09-19**, release profile, warm page cache, against one small Rust
workspace — 8 units, 227 symbols, 364 edges: **6.0 s wall, 901 MB peak resident**
(`command time -v ./target/release/reachgraph …`; the binary's own `run.json` reports
5.16 s, of which 4.83 s is analysis).

That figure is one repository and it is small. **It does not establish what reachgraph
costs on a large one.** `docs/design.md` §8's "minutes, not seconds" was measured against
the LSP round-trip architecture ADR-0001 rejected, and it has never been re-measured
against the linked engine at scale. Peak resident memory is the number to watch: 901 MB
on 227 symbols is rust-analyzer's own working set, and it grows with the repository
rather than with the graph.

## What it does not claim

An absent edge is not a proven absence. reachgraph reports the limits of each run — in
the terminal, and inside the artifact — and the wording is deliberate throughout: code is
described as **not reachable from any endpoint version in this index**, never as dead
(ADR-0007). Proc-macro expansion is off in v0.1 (ADR-0728) and generated code is not
loaded, so calls crossing either are unmeasured rather than absent.

## Licence

**MIT OR Apache-2.0**, at your option. See [LICENSE-MIT](LICENSE-MIT) and
[LICENSE-APACHE](LICENSE-APACHE).

The binary statically links its dependencies and the emitted artifact carries vendored
JavaScript, so both redistribute third-party code:
[THIRD-PARTY-LICENSES.md](THIRD-PARTY-LICENSES.md) covers the crates and the bundles, and
every emitted artifact carries its own `vendor/LICENSES.txt`.

The four-layer architecture is derived from [crabviz](https://github.com/chanhx/crabviz),
which is AGPL-3.0. Ideas are not copyrightable and the credit belongs in prose; **not one
line of its code is here**, and CI asserts that.

## Contributing

`docs/adr/` holds the decisions and `docs/plans/` the per-module implementation plans.
Every factual claim in either is labelled **MEASURED** or **INFERRED**; keep that up.

```console
$ cargo test --workspace --all-features
$ pre-commit run --all-files
```
