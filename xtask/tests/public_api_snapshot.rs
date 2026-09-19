//! `public_api_snapshot_matches` — plan-00 §6.1, plan-02 §7.2.1.
//!
//! The artifact this asserts on is the emitted file, not the renderer's return
//! value. The test re-emits the surface to a scratch path, reads BOTH files
//! back off disk and compares their bytes — so a renderer that returns the
//! right string and writes the wrong bytes fails here, and the thing a reviewer
//! reads in a diff is the thing the test checked.
//!
//! `--bless` and this test call the same [`xtask::emit`] with the same
//! [`xtask::SnapshotTarget`]. A second emit path would let the snapshot be
//! blessed through code the test never runs.

use std::fs;
use std::path::{Path, PathBuf};

use xtask::SnapshotTarget;

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask sits one level below the workspace root")
        .to_path_buf()
}

/// The differing lines, and nothing else.
///
/// A whole-string `assert_eq!` over a 250-line artifact prints both copies and
/// buries the change in them. Plan-02 §7.2.1's honest caveat is that the
/// snapshot makes a change visible but cannot make anyone look; printing ten
/// kilobytes is how it would stop being looked at.
fn differing_lines(emitted: &str, checked_in: &str) -> Vec<String> {
    let emitted: Vec<&str> = emitted.lines().collect();
    let checked_in: Vec<&str> = checked_in.lines().collect();

    (0..emitted.len().max(checked_in.len()))
        .filter_map(|line| {
            let now = emitted.get(line).copied();
            let before = checked_in.get(line).copied();
            (now != before).then(|| {
                format!(
                    "  line {}\n    checked in: {}\n    rendered:   {}",
                    line + 1,
                    before.unwrap_or("<end of file>"),
                    now.unwrap_or("<end of file>")
                )
            })
        })
        .take(40)
        .collect()
}

#[test]
fn public_api_snapshot_matches() {
    let target = SnapshotTarget::plugin_api(&workspace_root());

    let scratch =
        std::env::temp_dir().join(format!("reachgraph-public-api-{}.txt", std::process::id()));
    xtask::emit(&target, &scratch).expect("the public surface renders");

    let emitted = fs::read_to_string(&scratch).expect("the emitted snapshot is readable");
    let _ = fs::remove_file(&scratch);

    let checked_in = fs::read_to_string(&target.snapshot).unwrap_or_else(|error| {
        panic!(
            "{} is missing or unreadable ({error}). Run `cargo xtask public-api --bless`.",
            target.snapshot.display()
        )
    });

    if emitted == checked_in {
        return;
    }

    panic!(
        "\n{}'s public surface has changed:\n\n{}\n\n\
         This is not a test to repair. It is a diff somebody has to approve: every type, \
         field and signature in the contract is in {}, so an added parameter or a leaked \
         type shows up whatever it is named (plan-02 §7.2.1). If the change is intended, \
         run `cargo xtask public-api --bless` and put the resulting diff in the pull \
         request.\n",
        target.crate_name,
        differing_lines(&emitted, &checked_in).join("\n"),
        target.snapshot.display(),
    );
}
