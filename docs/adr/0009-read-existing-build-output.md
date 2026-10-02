# ADR-0009: reachgraph may read an existing build's output, and still never runs a build

**Status:** Accepted
**Date:** 2026-10-02
**Amends:** ADR-0001's "reachgraph never runs a build" section, and plan-03 §4 D-B and §9 D-D

## Context

Plan-03 §4 D-B and ADR-0001 (second user decision, 2026-09-17) ruled that reachgraph
runs no build, and plan-03 §9 D-D then ruled that v0.1 ships with generated code
unindexed and says so. The second ruling rested on a measurement: on 2026-09-19 a built
fixture's `OUT_DIR` was put into the VFS through `CargoConfig::extra_includes`, and the
call into it still did not resolve. The recorded conclusion was that no permitted
mechanism loads generated code at all, so even a built repository shows no generated
code.

The cost of that conclusion is the cross-repository join. A consumed RPC leaves a
repository through a tonic **client stub**, and the stub is build-script output. MEASURED
on yadgarhq/task at its default branch, 2026-10-02: the 6 served `TaskService` RPCs bind,
and the 5 consumed `TaskDbService` RPCs stay unbound because no stub is in the index.
Without a bound consumed root, the join key has nothing on this side to attach to.

The yadgarhq estate ruled on this in its own ADR-0842 (2026-10-02): reachgraph may read
**existing** build output (`OUT_DIR` from a prior cargo build) to bind consumed RPCs, and
it still never runs a build itself.

## Decision

**When the caller names a cargo target directory that an earlier build populated,
reachgraph reads each workspace member's build-script output from it.** The flag is
`--read-build-output <target-dir>`, on `reachgraph <repo>` and on `reachgraph preflight`.
Without the flag nothing changes: no build output is read, no sysroot is requested, and
the index and its coverage statement are what they were.

reachgraph still **runs no build**. No `cargo build`, no `cargo check`, and
`load_out_dirs_from_check` stays `false`. What is read is what a build the user ran left
behind.

### How the output is read

All MEASURED on the `fx-macro` fixture and on yadgarhq/task, gateway, iam and iam-db,
2026-10-02:

1. For each member that declares a build script, the output directory is the one cargo
   recorded in `<target>/<profile>/build/<pkg>-<hash>/root-output`. That file is cargo's
   own record of the `OUT_DIR` it gave the script, so it is read rather than
   reconstructed. When a package has several, the most recently written record wins, and
   the directory chosen is stated in the run's notes.
2. The output directory joins the VFS through `extra_includes`, which places it in the
   member's own source root. `include!` resolves a path only inside the source root of
   the file that calls it, so that placement matters.
3. `OUT_DIR` is set on the member crate's env **after** the load, through the
   `Crate::env` input. The 2026-09-19 measurement passed it through `load_workspace`'s
   `extra_env`. That parameter feeds the proc-macro server, not a crate's env, so `env!`
   never saw it.
4. The sysroot is requested. `include!`, `env!` and `concat!` are declared in `core`.
   Without a sysroot, none of them resolves, the inclusion never expands, and the
   generated file sits in the VFS belonging to no module. The default load requests no
   sysroot at all, which is the second reason the 2026-09-19 measurement saw nothing.

With all four, the `fx-macro` edge from `caller` into `generated_leaf` resolves. On
yadgarhq/task, 5 of 5 consumed roots bind to their generated client methods and 6 of 6
served roots still bind to their handlers.

### The sysroot is found without installing anything

`RustLibSource::Discover` is **not** used. MEASURED 2026-10-02: when the standard
library's source is missing, `Sysroot::discover` runs `rustup component add rust-src`.
That is a download and a change to the user's toolchain, which ADR-0001 forbids. The
first measurement run of this change triggered it on the author's machine. reachgraph
instead asks `rustc --print sysroot` in the repository, so a `rust-toolchain.toml` there
selects the toolchain. It then passes that path as `RustLibSource::Path`, which looks for
the source (honouring `RUST_SRC_PATH`) and installs nothing. `rustc` is the target
language's own toolchain, the same carve-out ADR-0001's 2026-09-17 amendment makes for
`cargo`. It is asked a question and builds nothing.

### It fails loudly

If the flag is given and any of the following is true, the run refuses with a preflight
failure that names the path:

- the target directory does not exist;
- it holds no build-script output for any member;
- the standard library's source does not resolve.

A flag that was given and silently read nothing would produce an artifact that looks
like one that read the build. That is the misleading-completeness failure plan-03 §9 D-D
exists to prevent. The failure is reported as build output that cannot be read, not as
a missing workspace, because the workspace loaded.

## Consequences

- Consumed roots bind to their generated client stubs. Their join keys now sit on real
  nodes, which is what an estate-level merge joins on.
- Served roots' shards reach the stubs their handlers call. MEASURED on yadgarhq/iam:
  `IamService/Login`'s shard contains the `IamDbService` `get_password_hash` and
  `create_credential` stubs.
- Generated server code also enters the graph, and served roots still bind. MEASURED: 6/6
  on task, 13/13 on iam, 16/16 on iam-db.
- The sysroot is loaded in this mode only. Standard-library nodes appear in shards as
  external targets, and the load is slower (about 9 s against 6 s on yadgarhq/task).
- The run's notes state each output directory read and the target it came from. The index
  is exactly as current as that build. reachgraph cannot tell a stale build from a
  current one.
- The `rust-src` component is now a precondition of this mode, so `rust-toolchain.toml`
  lists it.

## Not measured

- Workspaces with several members that have build scripts. Every output directory is added
  to every local package's includes, so which source root owns a file may be ambiguous.
- A target directory outside the repository, or not named `target`. The roots plugin
  recognises a generated stub by a `target/**/out/` path, so a stub elsewhere would not
  bind.
- A consumed stub deeper than the shard's depth limit (3) below a served handler. It is
  cut from the shard like any other node.

## Rejected alternatives

- **Run `cargo check` inside reachgraph** (`load_out_dirs_from_check: true`). Rejected
  for the reasons ADR-0001 gives: it runs the user's build scripts, at a time the user did
  not choose. yadgarhq ADR-0842 rejects it for the same reasons.
- **Synthesise `WorkspaceBuildScripts`.** Its fields are private and it has no public
  constructor. The only public way to fill it runs a command (`run_build_script_command`).
- **`RustLibSource::Discover`.** It can install a toolchain component. See above.
