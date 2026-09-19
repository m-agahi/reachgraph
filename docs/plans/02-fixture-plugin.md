# Plan 02 — `reachgraph-fixture`, the mechanical n=2

**Status:** ready to build
**Date:** 2026-09-17
**Amended:** 2026-09-17 — open questions 2, 3 and 6 resolved, question 7 opened (§8); the
fixture reports a `file` with no `span`, and a fixture `file` is a label rather than a
claim about disk (§3.1); it is explicitly selected, never detected (§3.2); the leak-1
guard is a public-API snapshot (§7.2.1); the walk-cost figure corrected (§5.1).
**Depends on:** plan-00 (the trait contract), ADR-0008
**Parallel with:** plan-01 (the waist) — see plan-00 §7
**Blocks:** plan-03, plan-04

ADR-0008 makes this crate the central mechanism of the neutrality decision, not a testing
convenience. It is **permanent test infrastructure, not scaffolding to delete when a second
language lands** — ADR-0008 Consequences says so explicitly, and its value as a fast
deterministic harness for the waist grows rather than shrinks.

Every factual claim below is labelled **MEASURED** or **INFERRED**. Design choices carry
neither.

---

## 1. What this crate is for

Two jobs, in priority order:

1. **Turn a leaked rust-analyzer-ism into a build failure now**, rather than a discovery in
   month nine when the second real language is half-written (ADR-0008).
2. **Be the entire test corpus for the waist** — no `cargo metadata`, no indexing, no
   timing variance, no requirement that a repository has been built.

What it is explicitly **not**: it does not count toward ADR-0002's n=3. It is a synthetic
consumer. It exercises the _shape_ of an interface, never the awkwardness of a real
language's semantics. §5 is an honest accounting of where that boundary falls.

Feature-gated per plan-00 §1: enabled in `dev-dependencies` and tests, never in a release
build.

---

## 2. The fixture format

One JSON document per case, at `<case-dir>/reachgraph.fixture.json`. The directory is what
`discover_units` and `preflight` receive as `root`, so a fixture case looks to the core
exactly like a repository looks.

### 2.1 Format rules, binding

These are the point of the format, not its packaging.

| rule                                                       | why                                                                                                                       |
| ---------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| `#[serde(deny_unknown_fields)]` on every struct            | a typo'd key is a parse error, not a silently ignored field                                                               |
| **no `#[serde(default)]` anywhere, on any field**          | every default is a place the harness could invent data the plugin never asserted                                          |
| `version` must be present, as a string or `null`           | ADR-0007: a missing version is `None`, and `None` must be an _assertion_, not an absence. A missing key is a parse error. |
| `container`, `is_test`, `doc`, `doc_format` all required   | a plugin author cannot forget `container` (plan-00 §8 question 3)                                                         |
| `provenance` and `inference_mode` required on every edge   | ADR-0003 field 4                                                                                                          |
| **no span, offset, line, column or position field exists** | §4, §5. A symbol's `file` IS case data; the plugin emits `span: None` regardless (§3.1)                                   |
| **no `confidence` field exists, anywhere**                 | plan-00 §8 question 5                                                                                                     |
| `join_key` required on every root                          | ADR-0007's cross-repo key, spelled by the plugin and opaque to the core (plan-00 §2)                                      |
| no file-content field                                      | the fixture cannot be re-parsed, so nothing downstream may assume source text is available                                |

The no-defaults rule is the one to enforce in review. MEASURED, design.md §5, is what it
guards against at the other end of the pipeline: code_graph's `Function.docstring` keeps
only the last line of a `///` block and 0 of 40 `Method` nodes carry any docstring at all —
absence rendering as if it were content. A `#[serde(default)]` is that failure, upstream.

**The rule above is necessary and NOT sufficient, MEASURED 2026-09-19 while building this
crate.** Writing no `#[serde(default)]` guards only the _explicit_ spelling. Serde supplies
an _implicit_ one that neither that rule nor `deny_unknown_fields` touches: for any
`Option<T>` field with no `default` attribute, the derive routes a missing key through
`serde::__private::de::missing_field`, whose deserializer answers `deserialize_option` with
`visit_none`. **Every bare `Option` field is optional whether or not anyone asked.**

This was not theoretical. `every_case_is_either_loadable_or_listed_invalid` went red on its
first run: the `version_key_missing` case — a document with no `version` key at all —
parsed cleanly, which made ADR-0007's core property ("a missing version is an absence;
`None` must be an _assertion_, not an absence") silently false in the format as first
written.

**The second clause, binding:** any `Option<T>` field whose absence must be a parse error
carries `#[serde(deserialize_with = "Option::deserialize")]`. That attribute takes the
other branch of the derive and reports the missing key. Applied here to
`FixtureRoot::version`, `FixtureCoverageVersion::version`, `FixtureSymbol::doc` and
`FixtureSymbol::container`.

The general form, worth carrying to any schema outside this crate: **in serde, "null" and
"absent" are the same thing by default, and a format that needs them to differ must say so
per field.**

### 2.2 Schema

```jsonc
{
  "fixture_version": 1,          // format version; bumped on breaking change
  "plugin_id": "fixture",        // becomes PluginId; a case may use a different id
  "position_encoding": "utf8_bytes" | "utf16_code_units" | "utf32_code_points",
  "capabilities": ["symbols", "edges", "roots", "classify"],   // subset allowed
  // ALWAYS empty. An empty marker list matches nothing (plan-00 §2), so the fixture
  // is structurally undetectable and can only be chosen explicitly (§3.2).
  "detection": { "marker_files": [], "extensions": [] },
  "preflight": "ok"
    | { "failed": { "reason": "…", "remediation": "…" } },
  "engine": "reachgraph-fixture 0.1.0",      // goes into every Provenance

  "units": [
    { "id": "unit:app", "display_name": "app", "root": "src" }
  ],

  "symbols": {
    "unit:app": [
      {
        "raw": "fn:handlers_v1/create_task",   // NodeId::raw; plugin_id supplies the half
        "name": "create_task",
        "kind": "method",                      // neutral six: function|method|type|module|field|other
        "raw_kind": "fn",                      // display only; the waist never matches on it
        "file": "src/service/handlers_v1.rs",
        "doc": "Create a task.",
        "doc_format": "markdown",
        "is_test": false,
        "container": "impl:TaskService_for_TaskServer"   // raw of another node, or null
      }
    ]
  },

  "edges": {
    "unit:app": [
      {
        "from": "fn:handlers_v1/create_task",
        "to": { "resolved": "fn:db/insert_task" },
        "provenance_plugin": "fixture",
        "engine": "reachgraph-fixture 0.1.0",
        "inference_mode": "resolved"           // resolved|lexical|type_inferred|enclosure
      },
      {
        "from": "fn:db/insert_task",
        "to": { "unresolved": { "name": "execute",
                                "candidates": ["fn:db/execute_pg", "fn:db/execute_sqlite"] } },
        "provenance_plugin": "fixture",
        "engine": "reachgraph-fixture 0.1.0",
        "inference_mode": "lexical"
      }
    ]
  },

  "roots": [
    {
      "contract": "acme.task",
      "version": "v1",                          // required key; null is a valid value
      "service": "TaskService",
      "operation": "CreateTask",
      "direction": "served",                    // served|consumed
      "join_key": "acme.task.v1.TaskService/CreateTask",   // required; opaque to the core
      "binding": { "bound": "fn:handlers_v1/create_task" }
                | { "unbound": { "reason": "…" } }
    }
  ],

  "coverage": {
    "contracts": ["acme.task"],
    "versions": [["acme.task", "v1"], ["acme.task", "v2"]]
  },

  "classify": [
    { "prefix": "src/", "category": "first_party" },
    { "prefix": "gen/", "category": "generated" },
    { "prefix": "vendor/", "category": "third_party" }
  ],
  "classify_fallback": "third_party"
}
```

Note what carries a node reference: a bare `raw` string, resolved to a `NodeId` by pairing
it with the document's `plugin_id`. The fixture author never writes a `{plugin, raw}` pair,
so a case cannot accidentally emit an id under another plugin's namespace — except in the
one case that deliberately tests cross-plugin namespacing (§6, `two_plugins`), which loads
two documents.

### 2.3 Worked example — `versioned_pair`

The case ADR-0007 is about: one operation, two versions, routing to different code.

```json
{
  "fixture_version": 1,
  "plugin_id": "fixture",
  "position_encoding": "utf8_bytes",
  "capabilities": ["symbols", "edges", "roots", "classify"],
  "detection": { "marker_files": [], "extensions": [] },
  "preflight": "ok",
  "engine": "reachgraph-fixture 0.1.0",

  "units": [{ "id": "unit:app", "display_name": "app", "root": "src" }],

  "symbols": {
    "unit:app": [
      {
        "raw": "impl:TaskService_for_TaskServer",
        "name": "TaskServer",
        "kind": "type",
        "raw_kind": "impl TaskService for TaskServer",
        "file": "src/service/mod.rs",
        "doc": null,
        "doc_format": "plain",
        "is_test": false,
        "container": null
      },

      {
        "raw": "fn:v1/create_task",
        "name": "create_task",
        "kind": "method",
        "raw_kind": "fn",
        "file": "src/service/handlers_v1.rs",
        "doc": "Create a task. v1.",
        "doc_format": "markdown",
        "is_test": false,
        "container": "impl:TaskService_for_TaskServer"
      },

      {
        "raw": "fn:v2/create_task",
        "name": "create_task",
        "kind": "method",
        "raw_kind": "fn",
        "file": "src/service/handlers_v2.rs",
        "doc": "Create a task. v2, validating.",
        "doc_format": "markdown",
        "is_test": false,
        "container": "impl:TaskService_for_TaskServer"
      },

      {
        "raw": "fn:shared/persist",
        "name": "persist",
        "kind": "function",
        "raw_kind": "fn",
        "file": "src/db/persist.rs",
        "doc": null,
        "doc_format": "plain",
        "is_test": false,
        "container": null
      },

      {
        "raw": "fn:v1only/legacy_audit",
        "name": "legacy_audit",
        "kind": "function",
        "raw_kind": "fn",
        "file": "src/audit/legacy.rs",
        "doc": "Audit hook retired in v2.",
        "doc_format": "markdown",
        "is_test": false,
        "container": null
      },

      {
        "raw": "fn:v2only/validate",
        "name": "validate",
        "kind": "function",
        "raw_kind": "fn",
        "file": "src/validate.rs",
        "doc": null,
        "doc_format": "plain",
        "is_test": false,
        "container": null
      },

      {
        "raw": "fn:orphan/unused_helper",
        "name": "unused_helper",
        "kind": "function",
        "raw_kind": "fn",
        "file": "src/util.rs",
        "doc": null,
        "doc_format": "plain",
        "is_test": false,
        "container": null
      }
    ]
  },

  "edges": {
    "unit:app": [
      {
        "from": "fn:v1/create_task",
        "to": { "resolved": "fn:shared/persist" },
        "provenance_plugin": "fixture",
        "engine": "reachgraph-fixture 0.1.0",
        "inference_mode": "resolved"
      },
      {
        "from": "fn:v1/create_task",
        "to": { "resolved": "fn:v1only/legacy_audit" },
        "provenance_plugin": "fixture",
        "engine": "reachgraph-fixture 0.1.0",
        "inference_mode": "resolved"
      },
      {
        "from": "fn:v2/create_task",
        "to": { "resolved": "fn:shared/persist" },
        "provenance_plugin": "fixture",
        "engine": "reachgraph-fixture 0.1.0",
        "inference_mode": "resolved"
      },
      {
        "from": "fn:v2/create_task",
        "to": { "resolved": "fn:v2only/validate" },
        "provenance_plugin": "fixture",
        "engine": "reachgraph-fixture 0.1.0",
        "inference_mode": "type_inferred"
      }
    ]
  },

  "roots": [
    {
      "contract": "acme.task",
      "version": "v1",
      "service": "TaskService",
      "operation": "CreateTask",
      "direction": "served",
      "join_key": "acme.task.v1.TaskService/CreateTask",
      "binding": { "bound": "fn:v1/create_task" }
    },
    {
      "contract": "acme.task",
      "version": "v2",
      "service": "TaskService",
      "operation": "CreateTask",
      "direction": "served",
      "join_key": "acme.task.v2.TaskService/CreateTask",
      "binding": { "bound": "fn:v2/create_task" }
    }
  ],

  "coverage": {
    "contracts": ["acme.task"],
    "versions": [
      ["acme.task", "v1"],
      ["acme.task", "v2"]
    ]
  },

  "classify": [{ "prefix": "src/", "category": "first_party" }],
  "classify_fallback": "third_party"
}
```

Expected waist output, and the reason the case exists:

| node                              | reached by | class                                          |
| --------------------------------- | ---------- | ---------------------------------------------- |
| `fn:v1/create_task`               | v1         | `v1_only`                                      |
| `fn:v2/create_task`               | v2         | `v2_only`                                      |
| `fn:v1only/legacy_audit`          | v1         | `v1_only` — **dies when v1 is sunset**         |
| `fn:v2only/validate`              | v2         | `v2_only`                                      |
| `fn:shared/persist`               | v1, v2     | `both` — survives the sunset                   |
| `fn:orphan/unused_helper`         | —          | unreachable                                    |
| `impl:TaskService_for_TaskServer` | —          | unreachable (a container is not a call target) |

Two shards, never one. If the waist merged `v1` and `v2`, `legacy_audit` would read as
`both` and the sunset answer would be wrong — the exact corruption ADR-0007 exists to
prevent, visible in a seven-node fixture.

The last row is deliberate: the `impl` node is reachable by nobody, because containment is
not a call edge (plan-01 §4.3). It appearing in the unreachable list is correct behaviour
and a mildly surprising one, which is why it is pinned by a test rather than left to be
"fixed" later.

---

## 3. Implementing the traits

One struct, several `impl` blocks. Loading is lazy and cached, so `discover_units` parses
once and every later call reads the parsed document.

```rust
pub struct FixturePlugin {
    doc: OnceLock<FixtureDoc>,
    path: PathBuf,
}
```

| trait (plan-00)                  | implementation                                                           | note                                                                                    |
| -------------------------------- | ------------------------------------------------------------------------ | --------------------------------------------------------------------------------------- |
| `Plugin::id`                     | `PluginId(doc.plugin_id)`                                                | a case may declare a non-`"fixture"` id                                                 |
| `Plugin::provides`               | `doc.capabilities`                                                       | a case may declare a subset, which is how plan-01's unpaired-provider error gets tested |
| `Plugin::position_encoding`      | `doc.position_encoding`                                                  | the only second encoding that exists at n=1 — §5, leak 3                                |
| `Plugin::detection`              | `doc.detection`, always empty                                            | matches nothing, by construction — §3.2                                                 |
| `Plugin::preflight`              | `doc.preflight`, or `Failed` when the file is missing or will not parse  | the parse error is the `reason`; the `remediation` names the path                       |
| `LanguagePlugin::discover_units` | `doc.units`                                                              | no manifest, no `Cargo.toml`, no build state — ADR-0008 leak 4                          |
| `SymbolProvider::symbols_in`     | `doc.symbols[unit.id]`, each row lifted to `Symbol`                      | `range.file` from the case, `range.span: None` always — §3.1                            |
| `EdgeProvider::edges_in`         | `doc.edges[unit.id]`                                                     | `call_site: None`, always                                                               |
| `EdgeProvider::edges_from`       | linear scan of every unit's edges for `from == node`                     | §5: this is the method the fixture makes look easy                                      |
| `RootProvider::roots`            | `doc.roots`; the `&dyn SymbolIndex` argument is **accepted and ignored** | see below                                                                               |
| `RootProvider::coverage`         | `doc.coverage`                                                           |                                                                                         |
| `Classifier::classify`           | longest-matching prefix from `doc.classify`, else `classify_fallback`    | prefixes are per-case data, never compiled in — ADR-0008 leak 8                         |

There is no `Renderer` row: a renderer is not a `Plugin` (plan-00 §3.6), and the fixture
implements no renderer. `Capability::Render` no longer exists, so no case can declare it.

### 3.2 Explicitly selected, never detected

The fixture declares `Detection { marker_files: &[], extensions: &[] }`. Under plan-00
§2's rule that an empty marker list matches nothing, `Registry::detect` can never return
it. The guarantee is structural — there is no special case in `detect` to be removed by a
later refactor.

Selection is by one of the two explicit routes plan-00 §5 specifies: a test passes
`&[&fixture]` straight to `Index::build`, or a developer types `--plugin fixture`, a
hidden flag gated on the same Cargo feature as this crate.

The risk being closed: a detectable fixture would, on any repository that happened to
contain a `reachgraph.fixture.json`, silently replace real analysis with hand-written JSON
and emit a complete, plausible, entirely fictional call graph. `fixture_is_never_detected`
(plan-01 §10.2) is the test.

**`roots` ignores `SymbolIndex` deliberately.** A fixture case states its bindings
directly; it has no handler-binding logic to perform. That is itself a neutrality signal:
if the trait ever required something only a real index can answer — a lookup whose result
changes the shape of the returned `Root` — the fixture would have to fake it, and faking it
is the moment to stop and ask whether the requirement belongs in the trait. The fixture
still _takes_ the argument, so the signature stays honest.

### 3.1 A file, and no offset — said out loud

`Symbol::range` is `SourceRange { file, span: Option<Span> }` (plan-00 §2, amended). The
fixture knows the file, because the case author typed it. It has no offsets at all, so it
reports none:

```rust
range: SourceRange { file: row.file.clone(), span: None },   // every fixture symbol
call_site: None,                                             // every fixture edge
```

This is the shape the crate argued for and got. Two earlier shapes were both wrong in the
same direction:

| shape                                     | what the fixture had to do       | why it was wrong                                                                                                                                         |
| ----------------------------------------- | -------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `range: SourceRange { file, span: Span }` | emit `Span { 0, 0 }`             | a sentinel indistinguishable from a real offset 0 — lying quietly, in the one crate that exists to make lying about the contract mechanically impossible |
| `range: Option<SourceRange>`              | emit `None`, discarding the file | honest about the offset by throwing away a fact it knew; cost plan-01 classification its file granularity                                                |
| **`span: Option<Span>`**                  | emit the file, omit the span     | says exactly what is known and exactly what is not                                                                                                       |

`fixture_symbols_have_no_span` asserts `range.span.is_none()` over the whole corpus. It
replaces `fixture_symbols_have_no_range`, which replaced `fixture_spans_are_sentinel`. The
property asserted has never changed and is still the right one: **the fixture does not lie
about offsets it does not have.** Only the type it is expressed against improved.

**A fixture `file` is a label, not a claim about disk.** `"src/service/handlers_v1.rs"`
names nothing that exists — the case directory holds one JSON document and no Rust. The
path is there to be classified by prefix, carried into the artifact, and displayed. No
test may assert that a fixture path exists, is readable, is absolute, is canonical, or
resolves under the case directory, and `preflight` must not check it. A fixture that had
to ship real files on disk would have re-acquired the build-state prerequisite ADR-0008
leak 4 exists to keep out (`§4`), one directory deeper.

---

## 4. What the fixture must be unable to express

This is the crate's reason for existing, so it is stated as a list of absences rather than
a list of features.

| the fixture cannot say                                           | consequence for the contract                                                                                                                                                                                  |
| ---------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| a byte offset, a line, a column, a cursor, a `FilePosition`      | a query keyed on a position has nothing to key on — and since plan-00's amendment the fixture _says_ it has no offset (`span: None`) rather than emitting a zero, while still reporting the file it does know |
| the contents of a source file                                    | nothing downstream may assume the text can be re-read or re-parsed                                                                                                                                            |
| a manifest, a workspace, a build directory, "the repo was built" | ADR-0008 leak 4; and design.md §8's MEASURED second hard prerequisite cannot leak into a trait                                                                                                                |
| a database, a snapshot, an engine handle, a salsa `FileId`       | ADR-0008 leaks 2 and 5                                                                                                                                                                                        |
| a `Documentation` type — only a `String` and a `DocFormat`       | ADR-0008 leak 7                                                                                                                                                                                               |
| a confidence float on a root                                     | plan-00 §8 question 5                                                                                                                                                                                         |
| a root with a missing `version` key                              | ADR-0007 — `null` is sayable, absence is not                                                                                                                                                                  |
| a "best guess" target on an unresolved edge                      | plan-00's `EdgeTarget`; design.md §8's binding rule                                                                                                                                                           |
| an edge without `provenance` or `inference_mode`                 | ADR-0003 field 4                                                                                                                                                                                              |
| a hardcoded path prefix                                          | ADR-0008 leak 8 — prefixes are case data                                                                                                                                                                      |

---

## 5. Which of ADR-0008's eight leaks this actually catches

The honest accounting. Three mechanisms are available, and they are not equally strong:

- **(A) Compile error, no fixture needed.** `plugin-api` must not depend on `ra_ap_*`
  (plan-00 §6.1). Any signature naming an `ra_ap` type fails to build, and the fixture
  crate could not name the type even if it wanted to.
- **(B) Compile error, fixture-dependent.** The fixture cannot produce a value of the
  required type, so `impl` fails.
- **(C) Test failure or reviewed diff.** The fixture _could_ stub the method; the guard is
  a test that says the stub is wrong, or the public-API snapshot (§7.2) turning the change
  into a diff someone must approve.

(C) is weaker than (A) or (B) — it catches a change rather than preventing one — and the
difference is worth naming rather than rounding off. It is materially stronger than it was
before the snapshot replaced the grep: a grep catches one leak by one spelling, a snapshot
catches every signature change in the contract.

| #   | leak                       | caught?                           | by what                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| --- | -------------------------- | --------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 1   | position-based queries     | **partially, better than before** | (A) for the `ra_ap` spelling: `FilePosition` cannot appear in `plugin-api` at all. **(C)** for a structurally position-shaped signature written in waist-owned types — `edges_at(&SourceRange)` or `edges_at(file, u32)` still compiles against the fixture, which would return `vec![]`. Two guards, both improved: `public_api_snapshot_matches` makes the added method a reviewed diff rather than a string match, and `fixture_symbols_have_no_span` now asserts the fixture _declares_ it has no offsets instead of asserting it fakes them consistently. A position-shaped query against a corpus that reports `span: None` everywhere returns nothing for every node, which is a visibly broken result rather than a plausible one. |
| 2   | `FileId`                   | **yes** (A)                       | an interned salsa integer is an `ra_ap` type. A waist-owned `FileId(u32)` would _not_ be caught — ADR-0008 explicitly permits the waist its own interning, so this is a leak against `ra_ap`'s interning, not against interning.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| 3   | `TextSize` / encoding      | **partially, and uniquely**       | The fixture is the only thing at n=1 that declares a second `PositionEncoding`. The `utf16_plugin` case (§6), loaded alongside a `utf8_bytes` case, drives `position_encoding_is_per_plugin_not_global` (plan-01 §10.2): the shard `plugins` table must carry two different encodings, each matching its declaring plugin. That is the whole claim — the core cannot collapse encoding to one global value. It does **not** prove offset conversion is correct, because the fixture has no real offsets. That correctness is untested until a second real language exists, and the fixture reports `span: None` rather than a zero, so it cannot even stand in for an offset.                                                              |
| 4   | Cargo workspace assumption | **yes** (C, strongly)             | a fixture case is a directory with one JSON file. Any `Cargo.toml` requirement in `discover_units` or `preflight` fails every case at once.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| 5   | salsa snapshot lifecycle   | **partially**                     | (A) for a lifetime-carrying `Snapshot<'db>` or any `ra_ap` handle in a signature. **Not caught** is an _implicit ordering_ requirement — a core that only works if `symbols_in` runs before `edges_from` passes silently, because the fixture answers in any order. `edges_from_works_before_symbols_in` exists for exactly that, and it is (C).                                                                                                                                                                                                                                                                                                                                                                                           |
| 6   | `SymbolKind`               | **yes** (C)                       | the `foreign_shapes` case emits `raw_kind` values no Rust plugin produces — `go_receiver`, `java_class`, `py_class` — with `kind: "other"`. Any core that matches on `raw_kind` gets a wrong answer on that case.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| 7   | `Documentation` type       | **yes** (A)                       | `ra_ap`'s type cannot appear in `plugin-api`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| 8   | classifier prefixes        | **yes** (C, strongly)             | the fixture's prefixes are case data (`vendor/`, `node_modules/`, `/usr/lib/go/`). Plan-01's `core_contains_no_path_prefix` is the paired source assertion.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |

### 5.1 What the fixture does not catch, at all

Stated plainly, because ADR-0008 warns that the fixture's value will be overstated.

1. **A real language's semantic awkwardness.** ADR-0002 already says this: three languages
   reveal the axis, two can be a coincidence, and a synthetic consumer is not one of the
   two. Nothing here changes that.

2. **`edges_from` is made to look easy.** The fixture answers it with a linear scan over a
   parsed document.

   design.md §8's figure — one round trip per node, a whole-repository walk in minutes —
   **does not apply here, and this plan corrects its own earlier use of it.** That number
   was MEASURED against the LSP round-trip architecture ADR-0001 rejected. In-process
   `ra_ap_ide` has no round trips: ADR-0004 MEASURED `Analysis::outgoing_calls` as an
   ordinary public function. **OPEN MEASUREMENT — the real cost of `edges_from` against a
   linked engine is unknown**, and plan-01 §8.7, plan-05 and plan-06 record it the same
   way. What would settle it: time it against one real repository at plan-03.

   The point survives the correction intact, and is arguably sharper for it. The fixture
   makes `edges_from` look cheap, and nobody knows what it actually costs — so plan-00
   open question 1 — whether `edges_from(&NodeId)` maps onto `ra_ap`
   without a live salsa snapshot per call — is _hidden_ by the fixture, not resolved by it.
   Plan-01 open question 1 compounds it: v0.1's batch build may never call `edges_from` at
   all, which would make it the only trait method shipping with no real implementation
   behind it.

3. **Performance and scale.** Every case is a handful of nodes. Nothing here says anything
   about the ~100k-node figure ADR-0006 INFERRED, about memory, or about whether the
   traversal is fast enough.

4. **Offset encoding correctness.** §5 leak 3, above.

5. **Error and partiality behaviour of a real engine.** A fixture fails by declaring
   `preflight: failed` or by being malformed. rust-analyzer's actual failure modes — open
   issue #19358, calls missed through generics (design.md §8) — produce _plausible
   incomplete output_, which is a category the fixture cannot imitate because it has no
   analysis to be incomplete.

6. **Whether doc comments are good enough to label a box with.** design.md §9 Q1, still the
   project's one unverified premise. A fixture supplies whatever doc text its author typed.

---

## 6. The fixture corpus

Each case is a directory under `reachgraph-fixture/fixtures/`. The task's five required
cases are marked ★.

| case                      | contents                                                                                                                                                                                                               | what it drives                                                                                             |
| ------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| `minimal`                 | 1 unit, 2 symbols, 1 edge, 1 bound root                                                                                                                                                                                | plan-01's first three tests; the unblocking deliverable                                                    |
| ★ `versioned_pair`        | §2.3                                                                                                                                                                                                                   | `v1_and_v2_are_separate_roots`, three-way classification, sunset answer                                    |
| ★ `unversioned_contract`  | one root, `"version": null`                                                                                                                                                                                            | `missing_version_stays_none`, `version_key_none_is_not_serialized_as_v1`                                   |
| ★ `unbound_root`          | one bound root, one `unbound` with a reason                                                                                                                                                                            | `unbound_root_is_reported_not_dropped`, coverage recording                                                 |
| ★ `unresolved_edge`       | one edge, `unresolved`, two candidates, one candidate otherwise unreached                                                                                                                                              | `unresolved_edge_does_not_propagate_reachability`, `possibly_reachable_annotation_does_not_filter_list`    |
| ★ `test_symbol_collision` | two symbols named `create_task`: one `is_test: false`, `container: "impl:TaskService_for_TaskServer"`; one `is_test: true`, `container: "impl:MockDb"`                                                                 | design.md §4's MEASURED collision; `container_is_copied_not_interpreted`; plan-04's binder acceptance case |
| `three_versions`          | v1, v2, v3 of one operation                                                                                                                                                                                            | `three_versions_emit_reached_by_not_class`                                                                 |
| `utf16_plugin`            | `"position_encoding": "utf16_code_units"`, `plugin_id: "fixture16"`; loaded together with a `utf8_bytes` case                                                                                                          | §5 leak 3; `position_encoding_is_per_plugin_not_global`                                                    |
| `foreign_shapes`          | `raw_kind` of `go_receiver`, `java_class`, `py_class`; `kind: "other"`; prefixes `vendor/`, `node_modules/`, `/usr/lib/go/`, **all three in one unit** — which is what proves classification is per file, not per unit | §5 leaks 6 and 8; plan-01's `classification_is_per_file_within_one_unit`                                   |
| `nested_containers`       | a three-level chain: function → `impl` block → module, each a symbol, each linked by `container`; two units                                                                                                            | plan-01 §3.1 and §10.2 — `container_chain_nests_and_terminates`, compound boxes for plan-05                |
| `container_cycle`         | two symbols whose `container` fields point at each other                                                                                                                                                               | `container_chain_survives_a_containment_cycle`                                                             |
| `cycle_and_diamond`       | A→B→C→A, plus one node reached from two roots                                                                                                                                                                          | `cycle_terminates`, `two_roots_sharing_a_node_both_include_it`                                             |
| `deep_chain`              | a 6-deep chain from one root                                                                                                                                                                                           | `depth_limit_marks_frontier_not_leaf`, `unreachable_uses_unlimited_depth`                                  |
| `external_target`         | an edge resolving to a `raw` no symbol declares                                                                                                                                                                        | `external_target_is_leaf_and_never_unreachable`                                                            |
| `dangling_container`      | `container` naming an unindexed `raw`                                                                                                                                                                                  | `dangling_container_is_not_an_error`                                                                       |
| `structured_raw_ids`      | `raw` strings containing `::`, `/`, `#`, `{`, and a whole JSON document                                                                                                                                                | `node_id_with_structured_looking_raw_is_not_parsed`                                                        |
| `no_classifier`           | `capabilities` omits `classify`                                                                                                                                                                                        | `no_classifier_yields_none_not_error`, `unclassified_nodes_are_counted_not_dropped`                        |
| `symbols_only`            | `capabilities: ["symbols"]`                                                                                                                                                                                            | `unpaired_symbol_provider_is_a_build_error`                                                                |
| `two_plugins`             | two documents, two `plugin_id`s, the same `raw` string in both                                                                                                                                                         | `same_raw_different_plugin_does_not_collide`                                                               |
| `preflight_fails`         | `"preflight": { "failed": { … } }`                                                                                                                                                                                     | `preflight_failure_aborts_with_remediation`                                                                |
| `version_key_missing`     | invalid on purpose: the `version` key is absent                                                                                                                                                                        | `missing_version_key_is_a_parse_error`                                                                     |

Twenty-one cases. Whether that is already too many to maintain is open question 4 (§8).

---

## 7. Tests

Test-driven, red first.

### 7.1 Build order

| #   | red test                               | red state                         | green when                                                                  |
| --- | -------------------------------------- | --------------------------------- | --------------------------------------------------------------------------- |
| 1   | `fixture_doc_parses`                   | no format type                    | `FixtureDoc` deserialises `minimal`                                         |
| 2   | `fixture_implements_every_trait`       | `FixturePlugin` does not exist    | every trait `impl` compiles                                                 |
| 3   | `minimal_case_round_trips`             | providers return nothing          | `discover_units` / `symbols_in` / `edges_in` return the document's contents |
| 4   | `plugin_api_has_no_ra_ap_dependency`   | no metadata test                  | `cargo metadata` assertion in place                                         |
| 5   | `no_plugin_depends_on_core`            | no metadata test                  | same file, second assertion                                                 |
| 6   | `public_api_snapshot_matches`          | no snapshot checked in            | `plugin-api`'s public surface captured and asserted                         |
| 7   | `fixture_symbols_have_no_span`         | invariant not enforced            | `range.span: None` asserted over the whole corpus                           |
| 8   | `missing_version_key_is_a_parse_error` | absent key deserialises to `None` | no `#[serde(default)]`; `version_key_missing` fails to parse                |

Test 2 is the gate. It is ADR-0008's mechanism made executable, and nothing downstream —
plan-01's entire suite included — can run before it is green.

**The corpus grows case by case, pulled by the test that needs it.** Only `minimal` exists
at step 3. Every later case in §6 is written when the plan-01 test it feeds goes red — and
that test is the red step, not the fixture file. `nested_containers` and `container_cycle`
arrive with plan-01's `container_chain_*` tests; `deep_chain` with
`depth_limit_marks_frontier_not_leaf`; `utf16_plugin` with
`position_encoding_is_per_plugin_not_global`. Writing all twenty-one up front would be
writing fixtures against an interface no test has exercised yet, which is the opposite of
red-first.

Two corpus-wide invariants — `fixture_symbols_have_no_span` (step 7) and
`fixture_detection_is_always_empty` (§7.4) — therefore start green over one case and stay
green as cases arrive. That is their job: they constrain every case added later.

### 7.2 The four neutrality guards (plan-00 §6.1)

| test                                 | mechanism                                                                                                                                                                   | strength                                                                              |
| ------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------- |
| `fixture_implements_every_trait`     | `static_assertions::assert_impl_all!(FixturePlugin: Plugin, LanguagePlugin, SymbolProvider, EdgeProvider, RootProvider, Classifier)`                                        | compile-level. The strongest guard in the project.                                    |
| `no_plugin_depends_on_core`          | `cargo metadata`: assert `reachgraph-core` is absent from the dependency graph of every `reachgraph-*` crate except `core` and `cli`                                        | strong; transitive                                                                    |
| `plugin_api_has_no_ra_ap_dependency` | `cargo metadata`: no `ra_ap_*` in `plugin-api`'s graph                                                                                                                      | strong; transitive. This is what makes §5 mechanism (A) work for leaks 1, 2, 5 and 7. |
| `public_api_snapshot_matches`        | `cargo public-api` (or equivalent) renders `plugin-api`'s complete public surface; the rendering is checked in as `plugin-api/public-api.txt` and asserted byte-equal in CI | **structural.** Covers all eight leaks at once, not one by string match.              |

The last three shell out to `cargo`. ADR-0001 forbids subprocesses in the **shipped
binary**; a dev-dependency test harness is not the shipped binary. Stated explicitly so it
is not mistaken for a violation, and so nobody "fixes" it by removing the test.

### 7.2.1 The public-API snapshot

It replaces `no_position_type_in_edge_api`, which was a grep over `EdgeProvider`'s method
signatures for `position`, `offset` and `cursor`.

Why the replacement is not merely tidier:

|                                      | grep                                            | snapshot                                              |
| ------------------------------------ | ----------------------------------------------- | ----------------------------------------------------- |
| scope                                | one trait, one leak                             | every type, field and signature in `plugin-api`       |
| evasion                              | rename the parameter to `at`                    | none; the rendering is the whole surface              |
| failure mode                         | silent pass on a spelling it did not anticipate | a diff, which somebody must approve                   |
| covers a _new_ leak nobody predicted | no                                              | yes — an unforeseen addition still shows up as a diff |

ADR-0008 calls leak 1 the highest risk of the eight and the anti-leak test it prescribes —
_could `ra_ap`'s return value be substituted verbatim here?_ — is a question about the
whole surface, not about one parameter name. A snapshot is that question made into a
reviewable artifact.

Mechanics, and the honest caveat: the assertion is only as good as the review of the diff.
A reviewer who regenerates the snapshot without reading it has defeated it exactly as a
rubber-stamped lockfile update defeats a dependency review. The snapshot makes the change
_visible_; it cannot make anyone look. That is still strictly better than a grep, which
made nothing visible.

Regeneration must be a deliberate command (`cargo xtask public-api --bless`), never an
automatic fixup on test failure. A test that repairs itself asserts nothing.

### 7.3 Compile-fail tests (`trybuild`)

Cases in `tests/ui/` that must **fail** to compile. These convert three conventions into
build failures:

| case                                  | must fail because                                                              |
| ------------------------------------- | ------------------------------------------------------------------------------ |
| `plugin_uses_core_internals.rs`       | `use reachgraph_core::…` from a plugin crate — plan-00 §1's dependency rule    |
| `symbol_literal_without_container.rs` | a `Symbol` struct literal omitting `container` — the field cannot be forgotten |
| `root_literal_with_confidence.rs`     | a `Root` literal setting `confidence` — the field is gone, not deprecated      |
| `root_literal_without_binding.rs`     | a `Root` literal omitting `binding`                                            |

These are cheap and they are precise: each asserts one field's existence or absence, which
a prose rule cannot.

### 7.4 Format-rule tests

| test                                         | asserts                                                                                                                                                                                                                                                                           |
| -------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `unknown_field_is_a_parse_error`             | `deny_unknown_fields` on every struct                                                                                                                                                                                                                                             |
| `missing_version_key_is_a_parse_error`       | ADR-0007; §2.1                                                                                                                                                                                                                                                                    |
| `null_version_parses_to_none`                | and never to `"v1"`                                                                                                                                                                                                                                                               |
| `missing_container_key_is_a_parse_error`     | the field cannot be forgotten                                                                                                                                                                                                                                                     |
| `missing_inference_mode_is_a_parse_error`    | ADR-0003 field 4                                                                                                                                                                                                                                                                  |
| `missing_join_key_is_a_parse_error`          | plan-00 §2 — the cross-repo key cannot be forgotten                                                                                                                                                                                                                               |
| `join_key_is_never_parsed_by_the_fixture`    | the fixture emits it verbatim; it does not derive the version from it either                                                                                                                                                                                                      |
| `no_serde_default_in_fixture_format`         | source assertion over the format module: the string `serde(default` does not appear                                                                                                                                                                                               |
| `format_has_no_span_field`                   | source assertion: no offset/line/column field in the format types. `file` is present and is not one.                                                                                                                                                                              |
| `format_has_no_confidence_field`             | source assertion: `confidence` does not appear                                                                                                                                                                                                                                    |
| `fixture_file_is_never_checked_against_disk` | §3.1 — no test asserts a fixture path exists, is absolute, or resolves under the case directory; `preflight` does not stat it                                                                                                                                                     |
| `fixture_detection_is_always_empty`          | every case _declares_ empty `marker_files` and `extensions` (§3.2). Not a duplicate of plan-01's `fixture_is_never_detected`, which asserts what `Registry::detect` _does_ with that declaration — declaration here, behaviour there, and either could regress without the other. |

The three source assertions are (C)-strength and are there because the corresponding
positive test cannot exist — you cannot write a test that a field is absent from a format
except by reading the format.

### 7.5 Lifecycle and ordering

| test                                 | asserts                                                                         |
| ------------------------------------ | ------------------------------------------------------------------------------- |
| `edges_from_works_before_symbols_in` | no implicit ordering requirement leaked into the core (§5, leak 5)              |
| `edges_from_agrees_with_edges_in`    | for every node, `edges_from(n)` equals the `edges_in` edges whose `from` is `n` |
| `discover_units_is_idempotent`       | called twice, same result; no hidden state                                      |

`edges_from_agrees_with_edges_in` is worth having even though it is trivially true for the
fixture: when `lang-rust` lands, the same test against a real engine is the one that
answers plan-00 open question 1.

### 7.6 Waist tests hosted here

Plan-00 §6.2's seven waist tests, plus plan-01 §10's full suite, run against this corpus.
They live in `reachgraph-core`'s test tree and depend on `reachgraph-fixture` as a
dev-dependency — the dependency direction stays legal, because `core` is allowed to depend
on a plugin in dev only, while no plugin depends on `core`.

Golden-artifact snapshots (`insta`) over the emitted `out/` tree, one per case, live here
too. They are the regression net for plan-05's renderer.

---

## 8. Open questions

1. **Where do the `cargo metadata` tests belong?** They are workspace-wide assertions
   living in one plugin crate, which is slightly wrong — a future contributor deleting the
   fixture crate would delete the workspace's dependency-direction guard with it. A
   separate `reachgraph-neutrality` test crate, or an `xtask`, would be more honest. Not
   worth the extra `Cargo.toml` until it bites.

2. ~~**Is a grep an acceptable long-term guard for leak 1?**~~
   **RESOLVED 2026-09-17 — no. The grep is replaced by a checked-in public-API snapshot**
   of `plugin-api`, asserted in CI. See §7.2.1.

   A grep guarded one leak by one spelling and was defeated by renaming a parameter. The
   snapshot is structural: it covers all eight leaks at once, and any added type, added
   field or changed signature becomes a diff a reviewer must approve — including a leak
   nobody predicted, which is the category a targeted grep can never cover.

   The residual weakness is named rather than hidden: a snapshot asserts that a change was
   _seen_, not that it was _understood_. A reviewer who blesses the diff unread has
   defeated it. Strictly better than a grep, which made nothing visible at all.

3. ~~**Should `Symbol::range` be `Option<SourceRange>`?**~~
   **RESOLVED 2026-09-17, then corrected the same day — the sentinel had to go, but
   `Option<SourceRange>` was the wrong place to put the `Option`.** Superseded by
   question 6. Final shape: `SourceRange { file, span: Option<Span> }` (§3.1).

   The diagnosis was right and is unchanged: a sentinel `Span { 0, 0 }` is
   indistinguishable from a real offset 0, so a plugin that does not know was lying in a
   way nothing downstream could detect — the same defect as `confidence: f32` and a
   renderer's defaulted `PositionEncoding` (plan-00 §3.6). An absence must be sayable.

   The first prescription overshot. Left as a record of the overshoot rather than
   rewritten, because the failure mode is instructive: fixing "this cannot express an
   absence" by wrapping the _enclosing_ type makes a second, known fact inexpressible
   too. Question 6 is what caught it.

4. **How many cases before the corpus is its own maintenance burden?** Twenty-one at the
   outset (§6). Each schema change touches all of them. A shared base document with
   per-case overlays would cut that, at the cost of making each case less readable in
   isolation — and readability is much of the point. INFERRED that flat documents are right
   below roughly thirty cases; unmeasured.

5. **Does the fixture need a second `fixture_version`?** Version 1 exists so a breaking
   format change is visible. Whether old versions are ever supported, or the corpus simply
   migrates in one commit, is undecided. Migrating in one commit is INFERRED adequate while
   the only consumer is this repository.

6. ~~**`SourceRange` bundles a fact the plugin knows with one it may not.**~~
   **RESOLVED 2026-09-17 — adopt `SourceRange { file: PathBuf, span: Option<Span> }`.**
   Plan-00 §2 amended (c); plan-01 §7 rewritten; §3.1 above is the fixture's side.

   `range` is required again and the `Option` sits on the _offset_, which is the only
   fact a plugin may not have. A plugin that names a symbol can place it in a file; where
   in the file is a separate claim of lower certainty, and the type now says so.

   What this bought, concretely:

   | before (`Option<SourceRange>`)                                              | after (`span: Option<Span>`)                                                       |
   | --------------------------------------------------------------------------- | ---------------------------------------------------------------------------------- |
   | fixture discards the file it knows                                          | fixture reports the file, omits the span                                           |
   | plan-01 falls back to `unit.root`, with a `ClassifiedByUnitRoot` diagnostic | fallback and diagnostic **deleted**; every indexed node has a real path            |
   | `src/` and `vendor/` in one unit classify identically                       | they classify differently — plan-01's `classification_is_per_file_within_one_unit` |

   The rejected alternative was `Symbol { file: Option<PathBuf>, range: Option<SourceRange> }`
   — two fields and two ways to state one file, the redundancy the contract avoids
   everywhere else.

   Generalised, because this is the fourth instance of one pattern in this contract
   (`confidence: f32`, the sentinel span, a renderer's defaulted `PositionEncoding`, and
   now this): **put the `Option` on exactly the fact that may be unknown — never on a
   type that also carries facts that are known.** Widening the optionality to the
   enclosing type destroys information just as surely as a sentinel invents it.

   Settled before `lang-rust` ships, which was the condition attached.

7. **Do any real plugins actually need `span: None`?** The fixture does, by construction.
   INFERRED that a tree-sitter-backed plugin will too, for symbols it can name but not
   locate precisely — but `lang-rust` via `ra_ap` always has offsets, so at n=1 the
   variant is exercised only by the fixture. That is the normal condition for the
   ADR-0003 fields (fields 1, 2 and 4 are all in the same position), and it is recorded
   here so nobody later removes it as unused. It is not unused; it is un-_reached_, which
   is different, and ADR-0003's Consequences warn about exactly that confusion.
