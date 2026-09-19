//! `golden_symbols_dump_matches` — plan-04 §12's cross-plugin contract, from
//! the producing side.
//!
//! `reachgraph-roots-proto-tonic` binds against a checked-in dump of what
//! `reachgraph-lang-rust` emits. That file is only a contract if something
//! notices when the producer stops agreeing with it, and nothing in the
//! consuming crate can notice: it must not load an `ra_ap` workspace at all
//! (plan-04 §12). This is the half that runs the engine.
//!
//! Behind `slow-tests` for the same reason plan-03 §13 puts Tier B there: it
//! loads a real Cargo workspace through cargo, which takes seconds. The four
//! gates run `--all-features`, so it runs there.

#![cfg(feature = "slow-tests")]

use std::path::{Path, PathBuf};

use xtask::golden::{self, GoldenTarget};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask sits one level below the workspace root")
        .to_path_buf()
}

#[test]
fn golden_symbols_dump_matches() {
    let root = workspace_root();
    let target = GoldenTarget::fx_impl(&root);

    let rendered = golden::render(&target, &root).expect("the fixture loads");
    let checked_in = std::fs::read_to_string(&target.golden).unwrap_or_else(|error| {
        panic!(
            "{} is missing or unreadable ({error}). Run `cargo xtask golden-symbols --bless`.",
            target.golden.display()
        )
    });

    if rendered == checked_in {
        return;
    }

    let differing: Vec<String> = rendered
        .lines()
        .zip(checked_in.lines())
        .enumerate()
        .filter(|(_, (now, before))| now != before)
        .map(|(line, (now, before))| {
            format!(
                "  line {}\n    checked in: {before}\n    rendered:   {now}",
                line + 1
            )
        })
        .take(20)
        .collect();

    panic!(
        "\n`reachgraph-lang-rust`'s symbol output has changed:\n\n{}\n\n\
         This is not a test to repair. `reachgraph-roots-proto-tonic` parses the \
         `raw_kind` grammar, the `container` relation and the `is_test` flag in that \
         file; a change here can silently unbind every root in a real repository. Read \
         the diff, check that the binder still agrees, then run \
         `cargo xtask golden-symbols --bless`.\n",
        differing.join("\n"),
    );
}
