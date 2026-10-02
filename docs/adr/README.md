# Architecture decision records

Decisions for `reachgraph`, the endpoint-rooted call graph tool. Background and the
original survey are in [`../design.md`](../design.md); where an ADR and the design
document disagree, **the ADR is current**.

Every factual claim in these records is labelled **MEASURED** (observed, with the command
or source given) or **INFERRED** (reasoned, not observed). That discipline is the project's
own premise: a derived claim must be traceable to its derivation.

| #                                                | title                                                            | status   | summary                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| ------------------------------------------------ | ---------------------------------------------------------------- | -------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [0001](0001-single-self-contained-binary.md)     | Single self-contained binary, no external tooling                | Accepted | One Rust binary on PyPI. No external binaries, subprocesses, runtime downloads or system package manager. Import analysis engines where they exist; write them where they do not. Depend, pin exact, build `--locked` — never fork. **Amended 2026-09-19:** the vendor tree is generated into the sdist at release time, not committed; MEASURED, `cargo vendor` writes 219 MB of which 57.5 MB is prebuilt Windows import libraries, so "auditable source in-tree" is not what it delivers. MIT OR Apache-2.0. |
| [0002](0002-compile-time-plugin-architecture.md) | Compile-time plugin architecture                                 | Accepted | Plugins are Rust crates in a Cargo workspace behind features, not subprocesses. Five kinds: symbol/doc, call-edge, root/contract, classifier, renderer. Language order Rust → Python → Go → Java. Interface stays unstable until n=3.                                                                                                                                                                                                                                                                           |
| [0003](0003-graph-schema-is-the-waist.md)        | The graph schema is the waist and is not a plugin                | Accepted | The schema, graph build and reachability algorithm are the fixed point. Six fields must exist in v0.1 that Rust alone does not need — capability declaration, `position_encoding`, opaque `node_id`, edge `provenance`/`inference_mode`, structured `preflight()`, language-neutral roots.                                                                                                                                                                                                                      |
| [0004](0004-call-edge-sources-per-language.md)   | Call-edge sources per language                                   | Accepted | Rust imports `ra_ap_ide`. Python imports `ruff_python_semantic` (lexical) plus `ty_python_semantic` (type inference) — both tiers are required, since method calls need types. Go and Java import nothing and must be written; `stack-graphs` is archived and never covered Go.                                                                                                                                                                                                                                 |
| [0005](0005-doc-comments-from-tree-sitter.md)    | Doc comments come from the resolver's own parse tree             | Accepted | An engine that resolves calls has already parsed the file. Rust and Python take doc text from `ra_ap` and `ruff_python_ast`. tree-sitter narrows to being the parser substrate for Go and Java. SCIP is rejected on availability, not quality.                                                                                                                                                                                                                                                                  |
| [0006](0006-root-sharded-static-output.md)       | Root-sharded static output artifact                              | Accepted | A static directory, one shard per root. `serve` is a dumb static file server, present only because `file://` blocks `fetch()`. Single inlined file under ~5 MB. GitHub Pages is opt-in and open-source-only — a call graph of private code is a disclosure.                                                                                                                                                                                                                                                     |
| [0007](0007-endpoint-version-is-first-class.md)  | Endpoint version is first-class in root identity                 | Accepted | `v1` and `v2` are separate roots; they may route to different code. Join key corrected to the fully-qualified operation name. A missing version is `None`, never `"v1"`. A partial root set makes live code look unreachable, so the index records its coverage and the wording extends to "not reachable from any endpoint version in this index".                                                                                                                                                             |
| [0008](0008-rust-only-v01-fixture-plugin.md)     | Rust-only v0.1; language neutrality enforced by a fixture plugin | Accepted | v0.1 implements Rust alone. An interface cannot be proven neutral from n=1, so a fixture plugin reading hand-written JSON supplies a mechanical second implementation — a leaked rust-analyzer-ism becomes a compile error, not a month-nine discovery. Eight named `ra_ap` leaks the traits must guard against; detection is plugin-declared from the first commit.                                                                                                                                            |
| [0009](0009-read-existing-build-output.md)       | Read an existing build's output; still never run a build         | Accepted | `--read-build-output <target-dir>` reads each member's build-script `OUT_DIR` from a build the user ran, so tonic client stubs enter the index and consumed roots bind. Amends ADR-0001's never-runs-a-build section and plan-03 D-B/D-D. Cites yadgarhq ADR-0842. Fails loudly, naming the path, when the flag reads nothing.                                                                                                                                                                                  |
| [0010](0010-estate-merge-on-join-key.md)         | `reachgraph merge` joins per-repository artifacts on `join_key`  | Accepted | A subcommand over artifacts runs already wrote: consumed and served roots are grouped by join key, a served handler's shard records the consumed keys it reaches, and `estate.json` plus a script-free `estate.html` show traces across repositories. Unmatched keys are labelled, never dropped. From yadgarhq ADR-0842.                                                                                                                                                                                       |

## Reading order

0001 and 0002 set the architecture; everything else follows from them. 0003 defines what
stays fixed while the plugins change. 0004 and 0005 are the per-language consequences.
0006 and 0007 define the output and should be read together — 0007's coverage requirement
constrains 0006's `unreachable.json`.

0008 is the one to read before writing any code: it scopes v0.1 to Rust and specifies how
the plugin interfaces are kept honest while only one language exists. It depends on 0002
for the plugin kinds and on 0003 for the six waist fields.

## Conventions

- Format: `Status`, `Date`, `Context`, `Decision`, `Consequences`, and `Rejected
alternatives` where something concrete was rejected.
- Numbers are permanent. Filenames do not change once written, even if a title does —
  0005 is retained under its original filename for link stability and records the change
  in a `History` section.
- A superseded ADR is marked `Superseded by ADR-NNNN` rather than deleted.

## Records from the decision ledger

Numbers in the 07xx range are not part of the sequence above. They are records in the yadgar
decision ledger for project `m-agahi/reachgraph`. Source comments and docs cite them, and a
citation is resolvable only when the record is exported here. Exported so far:

- [ADR-0728](0728-proc-macro-expansion-disabled.md) — v0.1 disables proc-macro expansion,
  because in-process expansion does not exist on a stable toolchain.

Cited in this repository and **not yet exported**: ADR-0727, ADR-0729, ADR-0730, ADR-0731,
ADR-0732, ADR-0738 and ADR-0743. Until each one is exported, read it in the ledger.
