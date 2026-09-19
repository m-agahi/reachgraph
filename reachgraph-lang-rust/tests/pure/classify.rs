//! Classification — plan-03 §10, ADR-0008 leak 8.

use std::path::{Path, PathBuf};

use reachgraph_lang_rust::classify::{classify_facts, CrateOrigin, PathFacts};
use reachgraph_plugin_api::Category;

fn member(package: &str) -> CrateOrigin {
    CrateOrigin::Member {
        package: package.to_owned(),
    }
}

fn facts<'a>(
    path: &'a Path,
    sysroot_src: Option<&'a Path>,
    origin: CrateOrigin,
    unit_package: &'a str,
    recorded_out_dirs: &'a [&'a Path],
) -> PathFacts<'a> {
    PathFacts {
        path,
        sysroot_src,
        origin,
        unit_package,
        recorded_out_dirs,
    }
}

/// All five categories, over synthetic facts.
///
/// **The sysroot cases are supplied as data and the test passes without any of
/// those strings appearing in the source.** That is the whole design of §10:
/// design.md §8's measured prefix table is evidence that the five categories
/// are right, and `/nix/store/…rust-lib-src/` is a property of the author's
/// machine rather than of Rust. The same file lives under
/// `~/.rustup/toolchains/…/lib/rustlib/src/rust/` on a rustup install and under
/// `/usr/lib/rustlib/src/` on a distribution package. Three spellings, one
/// fact, and the rule reads the fact.
#[test]
fn classifier_rules_over_synthetic_facts() {
    let sysroots: [PathBuf; 3] = [
        PathBuf::from("/nix/store/abc123-rust-lib-src/lib/rustlib/src/rust/library"),
        PathBuf::from("/home/someone/.rustup/toolchains/1.98.0-x86_64-unknown-linux-gnu/lib/rustlib/src/rust/library"),
        PathBuf::from("/usr/lib/rustlib/src/rust/library"),
    ];

    for sysroot in &sysroots {
        let stdlib_file = sysroot.join("core/src/option.rs");
        assert_eq!(
            classify_facts(&facts(
                &stdlib_file,
                Some(sysroot),
                CrateOrigin::NotAMember,
                "a",
                &[],
            )),
            Category::Stdlib,
            "under the resolved sysroot source root, whatever it is called"
        );
    }

    let sysroot = &sysroots[0];

    assert_eq!(
        classify_facts(&facts(
            Path::new("/repo/a/src/lib.rs"),
            Some(sysroot),
            member("a"),
            "a",
            &[],
        )),
        Category::FirstParty
    );

    assert_eq!(
        classify_facts(&facts(
            Path::new("/repo/b/src/lib.rs"),
            Some(sysroot),
            member("b"),
            "a",
            &[],
        )),
        Category::WorkspaceSibling
    );

    assert_eq!(
        classify_facts(&facts(
            Path::new("/home/someone/.cargo/registry/src/index.crates.io-1/tokio-1.0.0/src/lib.rs"),
            Some(sysroot),
            CrateOrigin::NotAMember,
            "a",
            &[],
        )),
        Category::ThirdParty
    );

    assert_eq!(
        classify_facts(&facts(
            Path::new("/repo/target/debug/build/a-2f1c/out/gen.rs"),
            Some(sysroot),
            member("a"),
            "a",
            &[],
        )),
        Category::Generated
    );
}

/// Plan-03 §10 rules 3 and 4 compare **package** identity, with the target
/// qualifier stripped — not unit identity.
///
/// §7 emits a separate `Unit` per target kind, so a package with a library and
/// an integration test is two units over one `src/`. Comparing unit ids would
/// classify that package's own `src/lib.rs` as `WorkspaceSibling` while the
/// `mycrate (test)` unit was being indexed, and a package's own source is
/// first-party to every one of its targets. This is the shape `fx-impl`
/// creates.
#[test]
fn a_packages_own_source_is_first_party_to_every_one_of_its_targets() {
    let verdict = classify_facts(&facts(
        Path::new("/repo/a/src/lib.rs"),
        None,
        member("path+file:///repo/a#a@0.1.0"),
        "path+file:///repo/a#a@0.1.0",
        &[],
    ));

    assert_eq!(verdict, Category::FirstParty);
}

/// Rule 2 is the one genuine path rule, and it is genuinely Cargo-shaped.
#[test]
fn classifier_generated_prefix() {
    let generated = [
        "/repo/target/debug/build/x-hash/out/y.rs",
        "/repo/target/release/build/yadgar-task-237f97ebf0e011bd/out/yadgar.taskapi.v1.rs",
        "/elsewhere/custom-target-dir/target/debug/build/a-1/out/deep/nested.rs",
    ];
    for path in generated {
        assert_eq!(
            classify_facts(&facts(Path::new(path), None, member("a"), "a", &[])),
            Category::Generated,
            "{path}"
        );
    }

    let not_generated = [
        // Compiled output, not generated source.
        "/repo/target/debug/deps/libа-1234.rlib.rs",
        // `out` with no `<pkg>-<hash>` component between `build` and it, so
        // the window is one component short.
        "/repo/target/debug/build/out/y.rs",
        // The window is the right SHAPE and the fourth component carries no
        // `-`, so it is not a cargo build-output directory. Without this case
        // the `-` rule is unexercised: MEASURED by mutation, deleting the rule
        // left the suite green.
        "/repo/target/debug/build/outdir/out/y.rs",
        // A package that happens to have a directory called `target`.
        "/repo/src/target/debug/y.rs",
    ];
    for path in not_generated {
        assert_ne!(
            classify_facts(&facts(Path::new(path), None, member("a"), "a", &[])),
            Category::Generated,
            "{path}"
        );
    }
}

/// A recorded out-dir classifies as generated wherever it lives, because
/// `CARGO_TARGET_DIR` can put it somewhere the structural rule cannot see.
#[test]
fn a_recorded_out_dir_is_generated_wherever_it_lives() {
    let out_dir = PathBuf::from("/var/cache/cargo-target/somewhere/opaque");
    let recorded: [&Path; 1] = [out_dir.as_path()];
    let file = out_dir.join("gen.rs");

    assert_eq!(
        classify_facts(&facts(&file, None, member("a"), "a", &recorded)),
        Category::Generated
    );
}

/// Without a resolved sysroot source root — the MEASURED `rust-src`-absent
/// case — a stdlib file is not silently reported as stdlib on a guessed path.
///
/// It lands in `ThirdParty`, which is wrong, and that wrongness is exactly what
/// §11 check 4's `Warned` is for. A classifier that guessed a sysroot path
/// would hide the condition instead of reporting it.
#[test]
fn without_a_resolved_sysroot_nothing_is_stdlib() {
    let verdict = classify_facts(&facts(
        Path::new("/nix/store/abc123-rust-lib-src/lib/rustlib/src/rust/library/core/src/option.rs"),
        None,
        CrateOrigin::NotAMember,
        "a",
        &[],
    ));

    assert_ne!(verdict, Category::Stdlib);
    assert_eq!(verdict, Category::ThirdParty);
}

/// Order is load-bearing. A generated file's crate **is** a workspace member —
/// the file is compiled into it — so a rule order that tested membership first
/// would report it as first-party and lose the one distinction design.md §8
/// measured as useful.
#[test]
fn generated_beats_membership() {
    let verdict = classify_facts(&facts(
        Path::new("/repo/target/debug/build/a-2f1c/out/gen.rs"),
        None,
        member("a"),
        "a",
        &[],
    ));

    assert_eq!(verdict, Category::Generated);
    assert_ne!(verdict, Category::FirstParty);
}
