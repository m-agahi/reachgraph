# Plan 07 — Wheel build and distribution

**Status:** blocked on measurement — see §9
**Date:** 2026-09-17
**Depends on:** plan-06 (the binary exists)
**Blocks:** release

ADR-0001: **one self-contained Rust binary, distributed on PyPI, no external binaries, no
runtime downloads, no system package manager.**

This plan carries more unmeasured facts than any other in the set, and plan-00 §7 says so
explicitly. **Every one is written as an OPEN MEASUREMENT with the command that settles
it.** Nothing here guesses a number into a design, because a packaging design built on a
guessed binary size is a design that gets rebuilt.

---

## 1. maturin configuration

### 1.1 The wheel contains a binary and no Python

maturin's `bindings = "bin"` builds a wheel whose payload is an executable placed in the
wheel's `.data/scripts/` directory, which `pip` installs into the environment's `bin/` (or
`Scripts\` on Windows) and marks executable. No Python module, no `__init__.py`, no import
machinery.

```toml
# pyproject.toml
[build-system]
requires = ["maturin>=1.7,<2.0"]
build-backend = "maturin"

[project]
name = "reachgraph"
requires-python = ">=3.8"
license = { text = "MIT OR Apache-2.0" }
dynamic = ["version"]

[tool.maturin]
bindings = "bin"
manifest-path = "crates/reachgraph-cli/Cargo.toml"
strip = true
locked = true
include = ["LICENSE-MIT", "LICENSE-APACHE", "THIRD-PARTY-LICENSES.md"]
```

### 1.2 The `__init__.py` launcher question — reasoned through

Earlier session reasoning assumed a Python launcher shim was needed. **INFERRED: it is
not, and the reason is ADR-0002.**

A Python launcher exists to solve one of two problems, and neither applies:

1. **Dispatching to a binary whose location Python must compute at runtime** — the
   `basedpyright` shape (ADR-0001 MEASURED it: `run_node.py` locating a Node binary from a
   dependency wheel). Our binary is installed directly onto `PATH` by `pip`, so nothing
   needs to locate it.
2. **Being importable by other Python packages** — the plugin-wheel shape. **ADR-0002
   makes plugins compile-time Rust crates behind Cargo features. There is no third-party
   plugin wheel, and there never will be under that ADR**, because an artifact that loads
   arbitrary foreign code is not a self-contained artifact. So nothing external needs to
   `import reachgraph`.

With both gone, the wheel needs to do exactly one thing: put an executable on `PATH`.

**A sharp consequence worth stating, because it is easy to get wrong:** `bindings = "bin"`
and `[project.scripts]` are **alternatives, not partners**. `[project.scripts]` generates a
Python console-script stub that imports a module and calls a function — which requires a
Python module that, per the above, does not exist. Declaring both under the same name
`reachgraph` produces two things claiming the same filename in `bin/`. **Do not carry both
"because it seems safer."**

**OPEN MEASUREMENT — confirm the bin-only wheel behaves as described.** This is the single
measurement that decides §1.1 versus a launcher design, and it takes minutes.

```bash
maturin build --release --locked --out dist/
unzip -l dist/reachgraph-*.whl        # expect: reachgraph-*.data/scripts/reachgraph
                                      # expect: NO reachgraph/__init__.py
python -m venv /tmp/rg && /tmp/rg/bin/pip install dist/reachgraph-*.whl
/tmp/rg/bin/reachgraph --version      # must run
ls -l /tmp/rg/bin/reachgraph          # must be executable, must be the binary itself
# Windows leg, separately: the .exe lands in Scripts\ and runs from a plain shell
```

**Contingency if it does not hold** — e.g. the executable bit is lost on some platform, or
Windows needs a launcher: add a minimal `reachgraph/__init__.py` + `__main__.py` that
`os.execv`s the packaged binary, switch to `bindings = "bin"` plus an explicit
`[project.scripts]` only if maturin's own docs require that pairing, and record here which
measurement forced it. Do not add it pre-emptively.

### 1.3 Version single-sourcing

`dynamic = ["version"]`; maturin reads it from `Cargo.toml`. One place. A test asserts
`reachgraph --version` matches the wheel's metadata version, because a packaging mismatch
is invisible until a user reports the wrong number.

---

## 2. `cargo vendor` and `--locked`

ADR-0001 is explicit: depend on `ra_ap_*` from crates.io, **pin exact versions**,
**`cargo vendor` the sources into the repository**, and **build with `--locked`**. Auditable
source in-tree, no network fetch at build time, and an upstream update becomes a deliberate
re-vendor rather than a merge conflict.

```bash
cargo vendor --locked vendor/          # writes vendor/ and prints the config stanza
```

```toml
# .cargo/config.toml — committed
[source.crates-io]
replace-with = "vendored-sources"
[source.vendored-sources]
directory = "vendor"
```

Rules:

- `vendor/` is committed. `Cargo.lock` is committed. Exact `=x.y.z` pins on the `ra_ap_*`
  family, which ADR-0001 MEASURED to be 0.0.x republished weekly in lockstep with
  rust-analyzer nightlies, with **no semver stability promise**.
- Release builds run `cargo build --release --locked --offline`. `--offline` is the check
  that the vendoring is actually complete; `--locked` alone still permits a registry read.
- **Vendor-drift test:** re-run `cargo vendor --locked` in CI and fail if `git diff
--exit-code vendor/ Cargo.lock` is non-empty. A vendor tree that has silently diverged
  from the lockfile is worse than no vendor tree, because it looks auditable and is not.
- An upstream bump is its own commit: re-vendor, re-lock, review the diff, note the
  rust-analyzer version in the changelog.

**OPEN MEASUREMENT — vendor tree size.** ~48 `ra_ap_*` crates plus their transitive
dependencies land in the repository, and this number decides §4's sdist question and
affects clone time for every contributor.

```bash
cargo vendor --locked vendor/ && du -sh vendor/ && find vendor -type f | wc -l
```

---

## 3. Binary size

The one number the whole distribution design rests on, and it has never been measured.

**MEASURED, for reference only, not as our size:** rust-analyzer's own release asset
`rust-analyzer-x86_64-unknown-linux-gnu.gz` is **14.8 MB gzipped** (ADR-0001, 2026-09-17).

**INFERRED in ADR-0001: plausibly 40–80 MB unpacked** for a binary linking `ra_ap_*`. That
is a reasoned range, never an observation, and it is not a basis for a platform decision.

**MEASURED and directly usable: PyPI's default per-file limit is 100.0 MiB, and individual
projects may request an increase** (ADR-0001).

**OPEN MEASUREMENT — actual binary size, and the number that matters is the compressed
one.** A wheel is a zip, so the per-file limit applies to the _deflated_ artifact.

```bash
cargo build --release --locked --offline -p reachgraph-cli
ls -l target/release/reachgraph                     # unstripped
strip -s target/release/reachgraph && ls -l target/release/reachgraph
maturin build --release --locked --out dist/
ls -l dist/reachgraph-*.whl                         # THIS is what the 100.0 MiB limit governs
```

Record all four numbers per platform in the release notes, so the trend is visible before
it becomes a problem.

### 3.1 Size tuning, and the trade it hides

Candidate settings: `strip = true`, `lto = "thin"` or `"fat"`, `codegen-units = 1`,
`opt-level = "z"`, `panic = "abort"`.

Two cautions, both real:

- **`opt-level = "z"` trades against analysis runtime.** plan-06 §5.1 leaves the
  in-process wall time an OPEN MEASUREMENT. Optimising the binary for size before knowing
  the runtime risks buying a smaller wheel with a slower tool, and the runtime is the
  number a user actually feels. **Measure size and runtime together, in the same matrix,
  never separately.**
- **`panic = "abort"` is a hazard with `ra_ap_*`, INFERRED.** salsa implements query
  cancellation by unwinding. A build that aborts on panic may turn a cancellation into a
  process death. Verify against plan-03's integration tests before enabling it, and prefer
  leaving it off — the size win is small next to the failure mode.

---

## 4. Platform matrix

**OPEN MEASUREMENT — the matrix itself.** Which targets build at all is unknown until
`ra_ap_*` has been compiled on each. Two specific unknowns:

- **The manylinux baseline.** `manylinux_2_17` versus `manylinux_2_28` depends on what
  glibc symbols the dependency tree requires. A wrong choice produces a wheel that installs
  and then fails at runtime on older distributions.
- **musl.** `ra_ap_*` on musl is unverified.

```bash
# per target, in the release workflow, recording success/failure and wheel size:
maturin build --release --locked --target <triple> --out dist/
auditwheel show dist/*.whl      # linux only: prints the manylinux tag actually satisfied
```

Proposed tiers, to be confirmed by the measurement above:

| tier                             | targets                                                                      | commitment                           |
| -------------------------------- | ---------------------------------------------------------------------------- | ------------------------------------ |
| **1 — must build, smoke-tested** | `x86_64-unknown-linux-gnu` (manylinux), `aarch64-apple-darwin`               | CI fails the release if either fails |
| **2 — built, best effort**       | `aarch64-unknown-linux-gnu`, `x86_64-apple-darwin`, `x86_64-pc-windows-msvc` | failure documented, release proceeds |
| **3 — not built in v0.1**        | musllinux, 32-bit anything, `aarch64-pc-windows-msvc`                        | source build via `cargo install`     |

**A note on build-time tooling, since ADR-0001 is strict about dependencies.** ADR-0001
constrains the _artifact_: no external binaries, no subprocesses, no runtime downloads, no
install-time fetch. It does not forbid a CI runner from having a cross-compiler. Using
`maturin build --zig` or a cross-compilation container is therefore permitted, and it
changes nothing about what the user installs. The line is precise: **tools may exist at
build time; nothing may be fetched or executed at install time or run time.**

### 4.1 sdist — a decision that must be made, not defaulted

maturin publishes an sdist by default. Two options, and the choice is not free:

| option                         | consequence                                                                                                                                                                                                                                                                                                               |
| ------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Wheels only** (`--no-sdist`) | A platform outside the matrix cannot `pip install` at all. Clean, but it removes the escape hatch.                                                                                                                                                                                                                        |
| **Wheels plus an sdist**       | The sdist must contain `vendor/` to build offline and honour §2. That makes it large — how large is §2's OPEN MEASUREMENT — and **PyPI's 100.0 MiB per-file limit applies to the sdist too**. An sdist _without_ `vendor/` builds only with network access, which contradicts §2's posture for anyone who builds from it. |

**Decision gate:** if `du -sh vendor/` (§2) leaves the compressed sdist comfortably under
100.0 MiB, publish wheels plus a vendored sdist, and document that building from sdist
requires a Rust toolchain. If it does not, publish wheels only and document the
`git clone` + `cargo build --locked --offline` path instead. **Do not publish a
vendor-less sdist as a compromise** — it would be an artifact that silently violates the
no-network-at-build property the rest of this plan is built on.

---

## 5. Supply-chain posture

Stated precisely, gains first, then residuals. ADR-0001's framing, applied to the
distribution.

**What is genuinely absent:**

- **No npm and no Node anywhere in the chain.** ADR-0001 MEASURED what the alternative
  cost: the only self-contained Python language-server option drags a 56–63 MB third-party
  repackaging of Node.js (`nodejs-wheel-binaries`) into the dependency graph.
- **No runtime download.** The binary fetches nothing, ever.
- **No install-time fetch and no post-install script.** `pip install` unpacks a zip.
- **No user-installed tool of unknown provenance or version.** The whole prerequisite class
  from design.md §8 is gone, including the rustup-proxy-loop trap where
  `command -v rust-analyzer` succeeds and proves nothing.
- **No network fetch at build time**, given §2's vendoring and `--offline`.
- **One hashable, signable, attestable artifact per platform.**

**Residuals, honestly, because a posture that lists only gains is marketing:**

1. **crates.io dependencies remain.** Vendoring makes them deterministic and auditable; it
   does not make them absent (ADR-0001's own wording).
2. **CVEs in vendored code become ours to patch and re-release.** ADR-0001 calls this a
   real, recurring maintenance obligation, not a one-time cost. `cargo-audit` runs in CI
   against the committed `Cargo.lock`; `cargo-deny` enforces the licence allow-list (§6).
   Policy: a RustSec advisory affecting the vendored tree is a patch release, not a
   next-minor item.
3. **The vendored JavaScript is a supply chain ADR-0001 does not cover.** plan-05 §3
   vendors Cytoscape, `cytoscape-fcose`, `cose-base` and `layout-base` into the binary.
   Those are ours to patch and re-release on exactly the same terms as item 2 — **and
   neither `cargo-audit` nor `cargo-deny` can see them**, because there is no npm in the
   loop to query an advisory database with. The ADR-0001 posture that removed npm from the
   chain also removed npm's advisory tooling. The check is therefore manual and must be
   scheduled: on each release, compare the pinned versions in
   `crates/reachgraph-render-html/vendor/VENDOR.toml` against upstream releases and the
   GitHub Advisory Database, and record the check in the release checklist (§7). This is
   the weakest link in the posture and is stated as such.
4. **PyPI itself.** Mitigated by Trusted Publishing (§6.3) rather than eliminated.

---

## 6. Licence compliance shipping

### 6.1 reachgraph's own licence

**MIT OR Apache-2.0**, dual (ADR-0001) — the norm for a developer tool. `LICENSE-MIT` and
`LICENSE-APACHE` are committed, referenced from `[project].license`, and included in the
wheel via `[tool.maturin] include`.

### 6.2 Attribution for vendored dependencies

A statically linked binary redistributes its dependencies' code. MIT and Apache-2.0 both
require the notice to travel with the redistribution.

- `THIRD-PARTY-LICENSES.md` is **generated** (`cargo-about` or `cargo-bundle-licenses`),
  committed, regenerated in CI, and CI fails if the regenerated file differs. A hand-
  maintained attribution file is wrong within two dependency bumps.
- Apache-2.0 dependencies additionally require any `NOTICE` file to be carried; the
  generator must be configured to include them.
- **The vendored JavaScript appears in this file too** (Cytoscape, fcose, cose-base,
  layout-base — all MIT). They are dependencies that happen not to be crates, and
  `cargo-about` will not find them; they are added from `VENDOR.toml` by the same CI step.
- **plan-05 §7 is the other half of this obligation and is not optional:** the notice must
  also be present in the _emitted artifact_, because emitting `out/vendor/*.js` and
  `overview.html` is itself a distribution of MIT-licensed code, to people who never see
  this repository. No minification or banner-stripping pass may run over those bundles —
  the `/*! ... */` banner **is** the notice.

### 6.3 crabviz — the hard line

**crabviz is AGPL-3.0 (MEASURED, ADR-0001) and is legally unavailable to this project.**

Under ADR-0001 the product is a single linked binary. Vendoring any crabviz source into it
would place **the entire artifact** under AGPL-3.0, and AGPL extends the copyleft trigger
to network and SaaS use, not only to distribution. That is incompatible with MIT OR
Apache-2.0 distribution. (INFERRED legal conclusion from a MEASURED SPDX identifier; not
legal advice.)

**The architecture may be borrowed; the code may never be.** design.md §3 openly derives
its four-layer split from crabviz and that is fine — ideas are not copyrightable, and the
attribution is in prose where it belongs.

Mechanical guard, because "everyone knows" is not a control: CI fails if the string
`crabviz` appears in any `Cargo.toml`, in `Cargo.lock`, or anywhere under `vendor/`. It is
permitted in `docs/` prose, which is where the architectural credit lives.

Related, from ADR-0001's table: **Eclipse JDT LS (EPL-2.0)** is file-level copyleft —
legally vendorable but producing a mixed-licence codebase with disclosure obligations, and
moot anyway because it is Java and cannot link into a Rust binary. `cargo-deny`'s allow-list
is `MIT`, `Apache-2.0`, `Apache-2.0 WITH LLVM-exception`, `BSD-3-Clause`, `ISC`, `Unicode-3.0`
— and anything outside it fails the build rather than prompting a judgement call at 5pm on a
release day.

---

## 7. Reproducibility and the release checklist

### 7.1 Reproducibility is a target, not a claim

**Rust release builds are not byte-reproducible by default.** Absolute paths, build
timestamps and zip metadata all vary. State this as a goal with a measurement, never as a
property we have.

Inputs we control: `rust-toolchain.toml` pinning the exact toolchain; `--locked --offline`;
`SOURCE_DATE_EPOCH` set from the tag's commit date; `--remap-path-prefix` for absolute
paths; deterministic zip ordering and timestamps in maturin.

**OPEN MEASUREMENT — is the wheel reproducible?**

```bash
# same machine, same container image, twice, with SOURCE_DATE_EPOCH fixed:
maturin build --release --locked --out dist-a/
cargo clean && maturin build --release --locked --out dist-b/
sha256sum dist-a/*.whl dist-b/*.whl        # target: identical
# if they differ, diff the zip contents to find which member varies:
diffoscope dist-a/*.whl dist-b/*.whl
```

Record the result honestly. "Reproducible on the same image, not yet across images" is a
true and useful statement; "reproducible builds" without the qualifier is not.

### 7.2 Publishing

**PyPI Trusted Publishing (OIDC).** No long-lived API token in repository secrets. Publish
from a tag-triggered workflow via `pypa/gh-action-pypi-publish`, with PEP 740 attestations
enabled so each artifact is signed and verifiable against the workflow that built it.

### 7.3 Release checklist

1. `cargo vendor --locked` produces no diff (§2 drift test).
2. `cargo audit` clean, or every finding triaged in writing.
3. `cargo deny check licenses` clean (§6.3 allow-list).
4. `THIRD-PARTY-LICENSES.md` regenerates identically; the JS entries from `VENDOR.toml`
   are present.
5. **Vendored JS advisory check (§5 residual 3)** — pinned versions compared against
   upstream releases and the GitHub Advisory Database. Manual, recorded, not skippable.
6. `crabviz` absent from manifests, lockfile and `vendor/` (§6.3).
7. Full matrix builds; sizes recorded (§3), including the compressed wheel.
8. Smoke test on every tier-1 platform (§8).
9. **The golden artifact is opened in a real browser** — plan-05 §8.1's honestly-recorded
   human check, since there is no browser harness. Verify: the endpoint list renders, one
   shard draws, the version toggle switches, the unreachable panel shows the binding
   wording and the coverage line, and the page works from `file://` for `overview.html` and
   via `reachgraph serve` for the sharded directory.
10. Reproducibility measurement re-run (§7.1); result recorded in the release notes.
11. Publish via Trusted Publishing; verify attestations on the published files.
12. `pip install reachgraph` from PyPI into a clean venv on one tier-1 platform, and run it.

---

## 8. Tests

Packaging correctness is verified by integration checks, not unit tests, and this plan says
so rather than pretending otherwise. The test-driven rule still applies in its meaningful
form: **each check below is written as a failing CI job before the configuration that
satisfies it exists.**

### 8.1 Wheel smoke test — per platform, in the release workflow

```bash
python -m venv smoke && ./smoke/bin/pip install dist/reachgraph-*.whl
./smoke/bin/reachgraph --version                     # matches Cargo.toml (§1.3)
./smoke/bin/reachgraph tests/fixtures/tiny-repo -o /tmp/smoke-out
test -f /tmp/smoke-out/index.html                    # ADR-0006 layout
test -f /tmp/smoke-out/endpoints.json
test -f /tmp/smoke-out/unreachable.json
test -d /tmp/smoke-out/vendor
./smoke/bin/reachgraph serve /tmp/smoke-out &        # then curl the shard, then kill
```

The fixture repository is the smallest thing that produces a real artifact, so the smoke
test proves the installed binary works end to end and not merely that it starts.

### 8.2 Repository-level checks

| check                        | fails when                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| ---------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `vendor_matches_lockfile`    | `cargo vendor --locked` produces a diff                                                                                                                                                                                                                                                                                                                                                                                                                      |
| `builds_offline`             | `cargo build --release --locked --offline` fails — the real proof that vendoring is complete                                                                                                                                                                                                                                                                                                                                                                 |
| `licences_within_allowlist`  | `cargo deny check licenses` finds an SPDX outside §6.3's list                                                                                                                                                                                                                                                                                                                                                                                                |
| `attribution_is_current`     | regenerated `THIRD-PARTY-LICENSES.md` differs from the committed one                                                                                                                                                                                                                                                                                                                                                                                         |
| `vendored_js_attributed`     | a `VENDOR.toml` entry is missing from `THIRD-PARTY-LICENSES.md`                                                                                                                                                                                                                                                                                                                                                                                              |
| `no_crabviz`                 | the string appears in a manifest, the lockfile or `vendor/`                                                                                                                                                                                                                                                                                                                                                                                                  |
| `wheel_has_no_python_module` | a `.py` file appears in `unzip -l` **while §1.2's measurement stands unrefuted**. This guard is conditional on the bin-only decision holding, and deliberately so: if measurement 1 forces the launcher contingency, this check is **replaced** — not deleted quietly — by one asserting the launcher is present and `os.execv`s the packaged binary. A guard that fires on a sanctioned design change is a guard that gets deleted the first time it fires. |
| `wheel_under_pypi_limit`     | the wheel exceeds 100.0 MiB, before PyPI rejects it                                                                                                                                                                                                                                                                                                                                                                                                          |
| `version_is_single_sourced`  | wheel metadata version ≠ `reachgraph --version`                                                                                                                                                                                                                                                                                                                                                                                                              |

---

## 9. Open measurements — the full list

This plan cannot be executed to completion until these are run. They are collected here so
none is lost in the prose.

| #   | measurement                                                              | command                                   | what it decides                                                         |
| --- | ------------------------------------------------------------------------ | ----------------------------------------- | ----------------------------------------------------------------------- |
| 1   | bin-only wheel behaviour; is any launcher needed                         | §1.2                                      | §1.1's entire configuration; whether `[project.scripts]` is used at all |
| 2   | vendor tree size                                                         | `cargo vendor --locked && du -sh vendor/` | §4.1's sdist decision; contributor clone cost                           |
| 3   | binary size — unstripped, stripped, and **compressed in the wheel**      | §3                                        | whether 100.0 MiB is close; whether a limit increase must be requested  |
| 4   | size versus runtime under `opt-level = "z"` / LTO, **measured together** | §3.1 with plan-06 §5.1's timing command   | the release profile                                                     |
| 5   | `panic = "abort"` against salsa cancellation                             | plan-03 integration tests                 | whether that setting is usable at all                                   |
| 6   | which targets build; the manylinux tag actually satisfied                | §4, `auditwheel show`                     | the tier table                                                          |
| 7   | musl viability for `ra_ap_*`                                             | §4                                        | whether musllinux leaves tier 3                                         |
| 8   | wheel reproducibility, same image                                        | §7.1                                      | what the release notes may claim                                        |
| 9   | vendored JS bundle set and size                                          | plan-05 §3                                | contributes to measurement 3; §5 residual 3's watch list                |

Inherited from plan-06 and load-bearing on measurement 4: **in-process analysis wall time
is itself unmeasured** (plan-06 §5.1). design.md §8's "minutes, not seconds" was MEASURED
against the LSP round-trip architecture that ADR-0001 rejected, and it does not transfer
unchanged to a linked `ra_ap_ide`.
