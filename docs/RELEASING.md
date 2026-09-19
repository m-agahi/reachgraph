# Releasing reachgraph

Plan-07 §7.3's checklist, with the measurements that were open when it was written
answered in place. **Every number here was produced by a command, and the command is
given.** Where a measurement could not be taken, that is said rather than filled in.

`.github/workflows/release.yaml` runs steps 1–8 and 12. Steps 9 to 11 are a human's.

---

## 1. Before the tag

| #   | check                                                                   | who runs it                                              |
| --- | ----------------------------------------------------------------------- | -------------------------------------------------------- |
| 1   | `cargo metadata --locked --offline` — the lockfile is current           | the `cargo-locked` pre-commit hook, on every commit      |
| 2   | `cargo deny check licenses advisories sources`                          | the `cargo-deny` pre-commit hook, and the `licences` job |
| 3   | `cargo xtask third-party-licenses` — the notice regenerates identically | the `licences` job                                       |
| 4   | `cargo test --locked -p xtask --test packaging` — the five repo guards  | the `licences` job                                       |
| 5   | **the vendored-JavaScript advisory check — manual, below**              | a human, before tagging                                  |
| 6   | the feature sweep — tests and clippy across every combination           | `ci-pr.yaml`, plus the sweep below                       |

### The vendored-JavaScript advisory check

**This is the weakest link in the supply-chain posture and plan-07 §5 says so.** ADR-0001
removed npm from the chain, which also removed npm's advisory tooling: neither
`cargo-audit` nor `cargo-deny` can see a CVE in a `.js` file. There is no automated
substitute, so the check is manual and it is not skippable.

For each `[[bundle]]` in `reachgraph-render-html/vendor/VENDOR.toml`, compare the pinned
version against the upstream release list and against the GitHub Advisory Database, and
record the result in the release notes:

| bundle                    | pinned | upstream project                               |
| ------------------------- | ------ | ---------------------------------------------- |
| cytoscape                 | 3.34.2 | `cytoscape/cytoscape.js`                       |
| cytoscape-fcose           | 2.2.0  | `iVis-at-Bilkent/cytoscape.js-fcose`           |
| cose-base                 | 2.2.0  | `iVis-at-Bilkent/cose-base`                    |
| layout-base               | 2.0.1  | `iVis-at-Bilkent/layout-base`                  |
| cytoscape-expand-collapse | 4.1.1  | `iVis-at-Bilkent/cytoscape.js-expand-collapse` |

An upstream bump is a deliberate re-vendor: replace the file, update `bytes` and `sha256`
in `VENDOR.toml`, and let `vendor_bytes_are_unmodified` prove the new bytes are the
published ones.

### The feature sweep

`--all-features` cannot catch a `--no-default-features` bug — it is structurally blind to
one, and PR G's CI failure was exactly that. Run all six:

```console
$ for f in "--all-features" "--no-default-features" "" \
           "--no-default-features --features render-html" \
           "--no-default-features --features lang-rust,roots-proto-tonic" \
           "--no-default-features --features serve"; do
    cargo test --locked --workspace $f
    cargo clippy --locked --workspace --all-targets $f -- -D warnings
  done
```

---

## 2. Tag, and what the workflow does

```console
$ git tag -a v0.1.0 -m "v0.1.0"
$ git push origin v0.1.0
```

`release.yaml` then builds the matrix, checks each wheel, builds a vendored sdist, and
stops at a protected `pypi` environment. **Nothing is published without that approval.**

### The platform matrix — PROPOSED, and the first run is the measurement

Plan-07 §9 measurements 6 and 7 **cannot be taken on a developer machine**, and the
workflow is written to take them rather than to assume them:

- `auditwheel show` reads the glibc symbols a built wheel actually requires. It is not
  installed here, and more to the point a NixOS build reports a platform tag that says
  nothing about what a manylinux runner would produce. **No manylinux tag is claimed in
  this document.** The `auditwheel` step writes the tag it finds into the run summary.
- musl for `ra_ap_*` is unverified. **INFERRED, and worth checking rather than assuming:**
  the classic musl blocker for rust-analyzer is loading a proc-macro server as a dynamic
  library, and ADR-0728 disables proc-macro expansion in v0.1 outright. If that is the
  only obstacle, musllinux is cheaper than the tier table assumes. Nobody has built it.

| tier                 | targets                                                                      | commitment                            |
| -------------------- | ---------------------------------------------------------------------------- | ------------------------------------- |
| **1 — smoke-tested** | `x86_64-unknown-linux-gnu` (manylinux), `aarch64-apple-darwin`               | `gate` fails the release if one fails |
| **2 — best effort**  | `aarch64-unknown-linux-gnu`, `x86_64-apple-darwin`, `x86_64-pc-windows-msvc` | failure documented, release proceeds  |
| **3 — not built**    | musllinux, 32-bit anything, `aarch64-pc-windows-msvc`                        | source build from the sdist           |

**The tier column is documentation, not enforcement, and the difference matters.** `gate`
fails when ANY wheel job fails, tier-2 included. "Failure documented, release proceeds" is
therefore a human reading which target failed, recording it, and re-running `gate` — not
something the workflow decides. It is written this way on purpose: `continue-on-error`
would mark a failed tier-2 job green, and a required check that cannot tell "built" from
"failed quietly" is the vacuous-green shape this repository's guards exist to refuse.

### sdist — plan-07 §4.1's decision gate, resolved

**Wheels plus a vendored sdist.** The gate was "if the vendor tree leaves the compressed
sdist comfortably under 100.0 MiB, publish one". CONFIRMED 2026-09-19 against PyPI's own
help page: the per-file limit is **100.0 MiB by default**, the project limit is 10.0 GiB,
and both can be raised on request.

MEASURED by building one: the sdist is **36 426 802 bytes — 34.7% of the limit** — and
carries all 11 877 vendored files, the root manifest, `Cargo.lock`, all seven crates,
`rust-toolchain.toml` and a `.cargo/config.toml` whose `replace-with =
"vendored-sources"` stanza points at the tree. Unpacked, `cargo check --offline` built it
in 25 s with no network at all.

The tree is built in the workflow and is **not** in git. ADR-0001's 2026-09-19 amendment
has the measurement behind that.

**One thing the sdist forces, found by building one rather than by reading maturin's
documentation.** maturin REWRITES the root `[workspace] members` list, dropping `xtask`
because it is `publish = false`, and ships the repository's `Cargo.lock` unchanged. The
lockfile then names two packages — `xtask` and its `prettyplease` dependency — that the
trimmed workspace no longer has, and `cargo --locked` refuses to remove them:

```
error: cannot update the lock file … because --locked was passed to prevent this
```

So the build inside the sdist is the **one** cargo invocation in `release.yaml` without
`--locked`, and the flag is replaced by something stronger rather than dropped. With
`crates-io` replaced by the vendored directory, `--offline` already makes an unvendored
crate unresolvable from anywhere; the workflow additionally diffs the lockfile across the
build and fails if anything was added, or if anything but those two names was removed.

---

## 3. The manual browser check — step 9, and nothing else covers it

**NOT PERFORMED FOR THIS RELEASE.** No browser has ever opened a reachgraph page. Plan-05
§8.1 records that there is no browser harness, and `cargo test` asserts the emitted HTML
by parsing it — which proves the document is well-formed and says nothing about whether
Cytoscape, fcose or expand-collapse actually initialise. **This checklist item is the only
thing in the project that would catch an integration failure in the vendored JavaScript.**

Open the golden artifact and verify, by eye:

1. `overview.html` from `file://` — a single file, no server. The endpoint list renders.
2. One shard draws. Nodes appear, the fcose layout runs, and the compound boxes collapse
   and expand on click. A Cytoscape extension that failed to register leaves a page that
   looks almost right and draws nothing.
3. The version toggle switches between endpoint versions.
4. The unreachable panel shows the ADR-0007 binding wording — "not reachable from any
   endpoint version in this index" — and the coverage line.
5. `reachgraph serve out/` and repeat 2–4 against the sharded directory, which is the path
   `file://` cannot exercise because it blocks `fetch()`.
6. The browser console is clean. A vendored bundle that 404s or throws on load is a
   console error and a blank canvas, not a build failure.

Record the browser and version in the release notes. "Checked in Firefox 143" is a fact;
an unticked box is honest; a ticked box nobody ticked is the thing this project exists to
refuse.

---

## 4. Reproducibility — step 10

**Rust release builds are not byte-reproducible by default**, and this document claims no
more than it measured. Absolute paths, build timestamps and zip metadata all vary.

```console
$ maturin build --release --locked --out dist-a/
$ cargo clean && maturin build --release --locked --out dist-b/
$ sha256sum dist-a/*.whl dist-b/*.whl
```

The result goes in the release notes with its qualifier. "Reproducible on the same image,
not yet across images" is a true and useful statement; "reproducible builds" without the
qualifier is not.

---

## 5. Publish — steps 11 and 12

The `publish` job runs only on a `refs/tags/v*` ref and only after the `pypi` environment's
approval. It uses **Trusted Publishing**: no long-lived token in repository secrets, a
short-lived OIDC credential, and PEP 740 attestations so each file is verifiable against
the workflow that built it.

Afterwards, on one tier-1 platform:

```console
$ python -m venv /tmp/rg && /tmp/rg/bin/pip install reachgraph
$ /tmp/rg/bin/reachgraph --version
$ /tmp/rg/bin/reachgraph ./some-repo -o /tmp/out
```

---

## 6. Plan-07 §9's open measurements, answered

Every row was produced on one machine — NixOS, 24 cores, glibc 2.42, rustc 1.98.0 — on
2026-09-19. Where the answer needs a runner this repository does not have, the row says so
instead of guessing.

### 1 — is any launcher needed? **No.**

```console
$ maturin build --release --locked --out dist/
$ python3 -m venv /tmp/rg && /tmp/rg/bin/pip install --no-index dist/*.whl
$ /tmp/rg/bin/reachgraph --version          # → reachgraph 0.1.0
```

The wheel holds `reachgraph-0.1.0.data/scripts/reachgraph` and **no `.py` file of any
kind**. `pip` installed it straight into `bin/`, executable bit intact, and the installed
binary analysed a real repository end to end — `index.html`, `overview.html`, the four
JSON documents and `vendor/` with its `LICENSES.txt`. The wheel's interpreter tag is
`py3`, not `cp3xx`, which is the tell that no Python ABI is involved.

**Plan-07 §1.2's reasoning holds and the launcher contingency is not taken.** The wheel's
`wheel_has_no_python_module` guard in the release workflow therefore stands as written.

### 3 — binary size, and the wheel

| artifact                              | bytes         |
| ------------------------------------- | ------------- |
| `target/release/reachgraph`, as built | 19 107 176    |
| the same, `strip -s`                  | 15 394 448    |
| the binary inside the wheel           | 15 394 464    |
| the binary **deflated** in the wheel  | 6 633 993     |
| **the wheel**                         | **6 698 343** |

**6.4% of PyPI's per-file limit** — 6 698 343 of 104 857 600 bytes. CONFIRMED against
PyPI's own help page rather than from memory: the default per-file limit is **100.0 MiB**,
the default project limit is 10.0 GiB, and administrators can raise either on request. No
increase is needed here.

ADR-0001's INFERRED 40–80 MB range was pessimistic by a factor of four. It reasoned from
rust-analyzer's own 14.8 MB gzipped release asset, and reachgraph links `ra_ap_ide` rather
than shipping the server.

The wheel also carries a CycloneDX SBOM maturin generates unprompted (335 693 bytes), the
two licence files and the attribution notice.

### 4 — size versus runtime, measured together

Four profiles, one binary each, three timed runs apiece against the same repository — 8
units, 227 symbols, 364 edges. Sizes are `strip -s` then `gzip -9`, which approximates a
wheel's deflate.

| profile                            | unstripped     | stripped       | gz-9          | median wall |
| ---------------------------------- | -------------- | -------------- | ------------- | ----------- |
| `release`, as cargo ships it       | 30 353 432     | 20 949 824     | 7 553 999     | 5.41 s      |
| `opt-level="z"`, `lto="thin"`      | 28 407 440     | 14 147 200     | 4 795 531     | 8.21 s      |
| **`lto="fat"`, `codegen-units=1`** | **19 107 176** | **15 394 448** | **6 446 978** | **4.86 s**  |
| the same plus `panic="abort"`      | 16 655 008     | 13 690 848     | 5 627 572     | 4.18 s      |

**The trade plan-07 §3.1 was written around does not exist in the direction it feared.**
Fat LTO is both smaller and faster than the default. `opt-level = "z"` is the worst row on
the number a user feels: it buys 1.2 MB of stripped binary for 52% more runtime.

Every one of the four produced a **byte-identical artifact** — only `run.json`, which
carries the timings and the output path, differed.

### 5 — `panic = "abort"` against salsa cancellation. **Not usable.**

MEASURED by reading the dependency sources rather than by reasoning about them:

- `salsa::Cancelled::throw` is `panic::resume_unwind(Box::new(self))`.
- `salsa::Cancelled::catch` is `panic::catch_unwind`, downcasting to `Cancelled` and
  re-raising anything else.
- `ra_ap_ide::Analysis::with_db` wraps **every** query in `Cancelled::catch`.

So `outgoing_calls` returning `Err` — which `reachgraph-lang-rust/src/engine.rs:880`
already handles — is an unwind being caught. Under `panic = "abort"` there is nothing to
catch and the process dies. `throw`'s own comment says it uses `resume_unwind` rather than
`panic!` **specifically to skip the panic hook**, so the abort would print nothing at all.

Plan-07 §3.1 INFERRED this hazard and guessed the size win was small. The win is not small
— 1.70 MB stripped and 14% of the runtime — and the setting is still refused, on the
failure mode alone. A 1.7 MB saving on a 6.7 MB wheel against a 100 MB limit does not buy
a silent death.

### 6 and 7 — the platform matrix and musl. **NOT MEASURABLE HERE.**

`file` on the installed binary reports its interpreter as
`/nix/store/…-glibc-2.42-67/lib/ld-linux-x86-64.so.2`. maturin tagged the wheel
`manylinux_2_39_x86_64`, and **that tag is an artifact of this host, not a property of the
build**: the wheel would not run on a manylinux system at all. `auditwheel` is not
installed here, and installing it would not fix the underlying problem.

**No manylinux baseline is claimed.** `release.yaml`'s `auditwheel show` step takes the
measurement on the first run.

musl is likewise unbuilt. See the inference under the tier table above — ADR-0728 may have
already removed the usual blocker, and nobody has checked.

### 8 — wheel reproducibility. **The binary yes; the wheel no, and the cause is not ours.**

Two builds of the same commit on the same machine:

- Without `cargo clean`: the wheel's sha256 **differs**, and a member-by-member comparison
  puts the difference in exactly two files — the CycloneDX SBOM, whose `serialNumber` is a
  fresh random UUID and whose `metadata.timestamp` is wall-clock, and `RECORD`, which
  carries the SBOM's hash. Every other member, the 15 394 464-byte binary included, is
  byte-identical. The zip entries are already stamped 1980-01-01, so maturin normalises
  those on its own.
- After `cargo clean --release` — 4 596 files and 1.6 GiB removed, then a full rebuild —
  the binary is **byte-identical**: sha256
  `83cf170724010565e57e1c83592f2242bafe5b22cb52c5d2c04bf9a12a543291`, 15 394 464 bytes,
  the same hash as before the clean. This is the half that tests the compiler rather than
  the archiver, and it passed.

maturin 1.14.1 offers `--sbom-include` to add SBOM files and **no flag to suppress the one
it generates**, so the claim that may honestly be made today is: _every file in the wheel
is reproducible on the same machine except an SBOM the packaging tool stamps with a random
UUID and the current time._ Not "reproducible builds", and not "not reproducible" either.

### 9 — the vendored JavaScript. **Answered in PR G, carried here.**

Five bundles, 790 890 bytes, recorded with an upstream sha256 each in
`reachgraph-render-html/vendor/VENDOR.toml`. They are inside the binary via `include_str!`
and inside the 6 633 993-byte deflated payload above.

---

## 7. What the release notes must carry

- The four size numbers per platform: unstripped, stripped, the wheel, and the sdist.
- The manylinux tag `auditwheel` reported, per linux wheel.
- Which tier-2 targets failed, if any, and why.
- The vendored-JavaScript advisory check's result, per bundle.
- The reproducibility result, with its qualifier.
- The browser and version the manual check was performed in — or that it was not.
