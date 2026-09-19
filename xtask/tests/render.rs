//! What the public-API renderer counts as public, and what it normalises away.
//!
//! The snapshot guard is only as strong as these. A renderer that silently
//! under-reports produces a snapshot that stays byte-identical while the
//! contract changes, which is the grep's failure mode wearing a structural
//! costume.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use xtask::SnapshotTarget;

/// A single-file crate written to a scratch directory, deleted on drop.
struct ScratchCrate(PathBuf);

impl ScratchCrate {
    fn new(label: &str, files: &[(&str, &str)]) -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "reachgraph-render-{}-{label}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("src")).expect("the temporary directory is writable");
        for (name, source) in files {
            fs::write(root.join("src").join(name), source).expect("the file is writable");
        }
        Self(root)
    }

    fn render(&self) -> String {
        let target = SnapshotTarget {
            crate_name: "scratch".to_owned(),
            lib_rs: self.0.join("src/lib.rs"),
            snapshot: self.0.join("public-api.txt"),
        };
        xtask::render(&target).expect("the scratch crate renders")
    }
}

impl Drop for ScratchCrate {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_private_item_is_absent() {
    let rendered = ScratchCrate::new(
        "private",
        &[("lib.rs", "pub struct Seen;\nstruct Hidden;\n")],
    )
    .render();

    assert!(rendered.contains("Seen"), "{rendered}");
    assert!(!rendered.contains("Hidden"), "{rendered}");
}

/// Effective visibility, not declared visibility. A `pub` item inside a private
/// module is not reachable from outside the crate, and a renderer that reported
/// it would make the snapshot churn on changes that are not API changes.
#[test]
fn a_pub_item_inside_a_private_module_is_not_public() {
    let rendered = ScratchCrate::new(
        "private-mod",
        &[(
            "lib.rs",
            "mod hidden {\n    pub struct NotReallyPublic;\n}\npub mod shown {\n    pub struct ReallyPublic;\n}\n",
        )],
    )
    .render();

    assert!(rendered.contains("ReallyPublic"), "{rendered}");
    assert!(!rendered.contains("NotReallyPublic"), "{rendered}");
}

/// `pub(crate)` is the same case as a private module and is the easier one to
/// write by accident.
#[test]
fn a_pub_in_crate_item_is_not_public() {
    let rendered = ScratchCrate::new(
        "pub-crate",
        &[(
            "lib.rs",
            "pub(crate) struct Internal;\npub struct External;\n",
        )],
    )
    .render();

    assert!(rendered.contains("External"), "{rendered}");
    assert!(!rendered.contains("Internal"), "{rendered}");
}

/// A public module in its own file is followed. Without this the renderer would
/// report an empty surface for any crate that is not one file, and would do it
/// silently.
#[test]
fn a_public_module_in_its_own_file_is_followed() {
    let rendered = ScratchCrate::new(
        "file-mod",
        &[
            ("lib.rs", "pub mod elsewhere;\n"),
            ("elsewhere.rs", "pub struct OverThere;\n"),
        ],
    )
    .render();

    assert!(rendered.contains("OverThere"), "{rendered}");
}

/// Documentation is not API. Rewording a doc comment must not produce a diff a
/// reviewer has to read, or the snapshot stops being read at all.
#[test]
fn doc_comments_are_normalised_away() {
    let rendered = ScratchCrate::new(
        "docs",
        &[(
            "lib.rs",
            "/// A prose sentence that is not part of the contract.\npub struct Documented;\n",
        )],
    )
    .render();

    assert!(rendered.contains("Documented"), "{rendered}");
    assert!(
        !rendered.contains("prose sentence"),
        "documentation reached the snapshot: {rendered}"
    );
}

/// Derives ARE API — `Clone` on a schema type is something a consumer depends
/// on, and removing one is a breaking change a reviewer must see.
#[test]
fn derives_are_part_of_the_surface() {
    let rendered = ScratchCrate::new(
        "derives",
        &[("lib.rs", "#[derive(Clone, Debug)]\npub struct Derived;\n")],
    )
    .render();

    assert!(rendered.contains("Clone"), "{rendered}");
    assert!(rendered.contains("Debug"), "{rendered}");
}

/// A function body is implementation. Only the signature is contract.
#[test]
fn a_function_body_is_not_part_of_the_surface() {
    let rendered = ScratchCrate::new(
        "bodies",
        &[(
            "lib.rs",
            "pub fn answer() -> u32 {\n    let intermediate = 21;\n    intermediate * 2\n}\n",
        )],
    )
    .render();

    assert!(rendered.contains("answer"), "{rendered}");
    assert!(
        !rendered.contains("intermediate"),
        "a function body reached the snapshot: {rendered}"
    );
}

/// Source order is not API. Moving a type must not churn the diff, or the
/// snapshot's signal is buried under its noise.
#[test]
fn source_order_does_not_change_the_rendering() {
    let one = ScratchCrate::new(
        "order-a",
        &[("lib.rs", "pub struct Beta;\npub struct Alpha;\n")],
    )
    .render();
    let other = ScratchCrate::new(
        "order-b",
        &[("lib.rs", "pub struct Alpha;\npub struct Beta;\n")],
    )
    .render();

    assert_eq!(one, other);
}

/// A method signature inside an inherent `impl` is API, and its privacy is
/// decided per method rather than by the block.
#[test]
fn a_public_method_is_rendered_and_a_private_one_is_not() {
    let rendered = ScratchCrate::new(
        "impls",
        &[(
            "lib.rs",
            "pub struct Holder;\nimpl Holder {\n    pub fn reachable(&self) -> u32 { 0 }\n    fn unreachable(&self) -> u32 { 0 }\n}\n",
        )],
    )
    .render();

    assert!(rendered.contains("reachable"), "{rendered}");
    assert!(!rendered.contains("unreachable"), "{rendered}");
}

/// A missing module file is a hard error, never an empty section. A renderer
/// that shrugged would emit a smaller surface than the crate has and the
/// snapshot would agree with itself.
#[test]
fn a_missing_module_file_is_an_error() {
    let scratch = ScratchCrate::new("missing-mod", &[("lib.rs", "pub mod absent;\n")]);
    let target = SnapshotTarget {
        crate_name: "scratch".to_owned(),
        lib_rs: scratch.0.join("src/lib.rs"),
        snapshot: scratch.0.join("public-api.txt"),
    };

    assert!(xtask::render(&target).is_err());
}

/// A private field is not API, and leaking one is worse than merely wrong: the
/// snapshot then churns whenever an implementation detail moves, and a snapshot
/// that churns is a snapshot nobody reads.
#[test]
fn a_private_struct_field_is_absent() {
    let rendered = ScratchCrate::new(
        "fields",
        &[(
            "lib.rs",
            "pub struct Mixed {\n    pub shown: u32,\n    hidden: Vec<String>,\n}\n",
        )],
    )
    .render();

    assert!(rendered.contains("shown"), "{rendered}");
    assert!(
        !rendered.contains("hidden"),
        "a private field reached the snapshot: {rendered}"
    );
}

/// An enum variant's fields carry no visibility of their own — they are as
/// public as the enum. Dropping them would erase the contract.
#[test]
fn enum_variant_fields_survive() {
    let rendered = ScratchCrate::new(
        "variants",
        &[(
            "lib.rs",
            "pub enum Outcome {\n    Failed { reason: String },\n}\n",
        )],
    )
    .render();

    assert!(rendered.contains("reason"), "{rendered}");
}

/// The artifact must satisfy this repository's own `end-of-file-fixer` hook.
///
/// It does not merely have to look tidy. The hook rewrites a file that ends in
/// a blank line, so a renderer that emits one puts the hook and this guard in
/// a loop: `--bless` writes the artifact, the hook trims it, and the next test
/// run is red for a reason that has nothing to do with the contract.
#[test]
fn the_rendering_ends_with_exactly_one_newline() {
    let rendered = ScratchCrate::new("eof", &[("lib.rs", "pub struct Only;\n")]).render();

    assert!(rendered.ends_with('\n'), "{rendered:?}");
    assert!(!rendered.ends_with("\n\n"), "{rendered:?}");
}

/// An `impl` whose self type is not a public type OF THIS CRATE must still be
/// rendered.
///
/// `impl LocalTrait for String` is public surface: a consumer gets a new method
/// on a foreign type. The first version of this renderer kept an `impl` only
/// when its self type was a local public type, which dropped this case and
/// every blanket `impl<T: Bound>` without saying so. Silently emitting a
/// smaller surface than the crate has is the one failure this artifact cannot
/// afford, and it is the failure the renderer refuses by erroring on an item
/// kind it does not know.
#[test]
fn an_impl_on_a_foreign_type_is_rendered() {
    let rendered = ScratchCrate::new(
        "foreign-impl",
        &[(
            "lib.rs",
            "pub trait Extra {\n    fn extra(&self) -> usize;\n}\nimpl Extra for String {\n    fn extra(&self) -> usize { self.len() }\n}\n",
        )],
    )
    .render();

    assert!(
        rendered.contains("impl Extra for String"),
        "an impl on a foreign type vanished: {rendered}"
    );
}

/// The case the type filter exists for, kept alongside the one above so the
/// two cannot be confused: an `impl` on a PRIVATE local type is not surface.
#[test]
fn an_impl_on_a_private_local_type_is_absent() {
    let rendered = ScratchCrate::new(
        "private-impl",
        &[(
            "lib.rs",
            "struct Hidden;\nimpl Hidden {\n    pub fn method_on_private(&self) -> u32 { 0 }\n}\npub struct Shown;\nimpl Shown {\n    pub fn method_on_public(&self) -> u32 { 0 }\n}\n",
        )],
    )
    .render();

    assert!(rendered.contains("method_on_public"), "{rendered}");
    assert!(
        !rendered.contains("method_on_private"),
        "an impl on a private type reached the snapshot: {rendered}"
    );
}
