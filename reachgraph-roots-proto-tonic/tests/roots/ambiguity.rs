//! Task 2062 — an ambiguous root says which packages and which candidates.
//!
//! The shape is the one MEASURED on the `discrim` repository: `hello.v1.Greeter`
//! and `hello.v2.Greeter` declare the same service name, two impls name a trait
//! `Greeter`, and nothing in the index says which impl serves which version. The
//! binding stays a reported gap (plan-04 §7); what these cases pin is that the
//! reason is actionable — every declaring package and every candidate's
//! `file:line`.
//!
//! # Why these repositories are written at test time
//!
//! A `file:line` needs the file's text, so the candidates have to exist on disk
//! under the repository root. The generated stubs of B2 live under
//! `target/**/out/`, which `.gitignore` excludes repository-wide, so they cannot
//! be a checked-in fixture. The other cases are written the same way so that the
//! index and the text it points into are built from one source and cannot drift.

use std::path::{Path, PathBuf};

use reachgraph_plugin_api::{Direction, Root, RootBinding, RootProvider, Symbol, SymbolIndex};
use reachgraph_roots_proto_tonic::unbound::UnboundReason;
use reachgraph_roots_proto_tonic::ProtoTonicPlugin;

use crate::fake::{FakeIndex, Sym};

/// A repository written under Cargo's per-test temporary directory.
struct Scratch {
    root: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let root = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("ambiguity")
            .join(name);
        if root.exists() {
            std::fs::remove_dir_all(&root).expect("the old scratch repository is removable");
        }
        std::fs::create_dir_all(&root).expect("the scratch repository is creatable");
        Self { root }
    }

    fn write(&self, relative: &str, text: &str) -> &Self {
        let path = self.root.join(relative);
        std::fs::create_dir_all(path.parent().expect("a file has a parent"))
            .expect("the parent is creatable");
        std::fs::write(&path, text).expect("the file is writable");
        self
    }

    /// The byte offset of the `nth` (0-based) occurrence of `needle`, which is
    /// what `Symbol::range.span` carries.
    fn offset(&self, relative: &str, needle: &str, nth: usize) -> u32 {
        let text = std::fs::read_to_string(self.root.join(relative)).expect("the file is readable");
        let found = text
            .match_indices(needle)
            .nth(nth)
            .unwrap_or_else(|| panic!("{relative} has no occurrence {nth} of {needle:?}"));
        u32::try_from(found.0).expect("the fixture is small")
    }

    fn roots(&self, plugin: &ProtoTonicPlugin, index: &FakeIndex) -> Vec<Root> {
        plugin.roots(&self.root, index).expect("roots")
    }
}

fn greeter_proto(package: &str) -> String {
    format!(
        "syntax = \"proto3\";\n\npackage {package};\n\nmessage HelloRequest {{}}\nmessage \
         HelloReply {{}}\n\nservice Greeter {{\n  rpc SayHello(HelloRequest) returns \
         (HelloReply);\n}}\n"
    )
}

fn say_hello_roots(roots: &[Root]) -> Vec<&Root> {
    let found: Vec<&Root> = roots
        .iter()
        .filter(|root| root.service == "Greeter" && root.operation == "SayHello")
        .collect();
    assert!(!found.is_empty(), "no Greeter/SayHello root in {roots:#?}");
    found
}

fn reason_of(root: &Root) -> &str {
    match &root.binding {
        RootBinding::Unbound { reason } => reason,
        RootBinding::Bound(node) => panic!("{} bound to {node:?}", root.join_key),
    }
}

/// The served side of `discrim`, written out.
///
/// Line 6 holds `Svc::say_hello` and line 10 holds `Svc2::say_hello`.
const DISCRIM_LIB: &str = "\
//! Two versions of one service, implemented side by side.
pub struct Svc;
pub struct Svc2;

impl v1::Greeter for Svc {
    fn say_hello(&self) {}
}

impl v2::Greeter for Svc2 {
    fn say_hello(&self) {}
}
";

fn discrim(name: &str) -> (Scratch, FakeIndex) {
    let repo = Scratch::new(name);
    repo.write("proto/hello_v1.proto", &greeter_proto("hello.v1"))
        .write("proto/hello_v2.proto", &greeter_proto("hello.v2"))
        .write("src/lib.rs", DISCRIM_LIB);

    // MEASURED (wiki reachgraph-task-2062-plan): lang-rust keeps only the last
    // segment of the written trait path, so both headers read `impl Greeter`.
    let first = Sym::impl_block(
        "Svc",
        "impl Greeter for Svc",
        "src/lib.rs",
        repo.offset("src/lib.rs", "impl v1", 0),
    )
    .build();
    let second = Sym::impl_block(
        "Svc2",
        "impl Greeter for Svc2",
        "src/lib.rs",
        repo.offset("src/lib.rs", "impl v2", 0),
    )
    .build();
    let index = FakeIndex::new(vec![
        Sym::method(
            "say_hello",
            "src/lib.rs",
            repo.offset("src/lib.rs", "fn say_hello", 0),
        )
        .inside(&first)
        .build(),
        Sym::method(
            "say_hello",
            "src/lib.rs",
            repo.offset("src/lib.rs", "fn say_hello", 1),
        )
        .inside(&second)
        .build(),
        first,
        second,
    ]);
    (repo, index)
}

/// B1 — two served versions, two impls: each reason names both packages and
/// both impls' `file:line`.
#[test]
fn served_versions_name_every_package_and_every_impl_location() {
    let (repo, index) = discrim("b1-served");
    let roots = repo.roots(&ProtoTonicPlugin::new(), &index);

    let found = say_hello_roots(&roots);
    assert_eq!(found.len(), 2, "one root per version: {found:#?}");
    for root in found {
        assert_eq!(root.direction, Direction::Served, "{root:?}");
        let reason = reason_of(root);
        for expected in ["hello.v1", "hello.v2", "src/lib.rs:6", "src/lib.rs:10"] {
            assert!(
                reason.contains(expected),
                "{}: the reason lacks `{expected}`: {reason}",
                root.join_key
            );
        }
    }
}

/// The consumed side of `discrim` once `--read-build-output` found both
/// generated clients. Line 3 of each stub holds `say_hello`.
const STUB: &str = "\
// @generated
impl<T> GreeterClient<T> {
    pub async fn say_hello(&mut self) {}
}
";

const STUB_V1: &str = "target/debug/build/discrim-1111/out/hello.v1.rs";
const STUB_V2: &str = "target/debug/build/discrim-1111/out/hello.v2.rs";

fn consumed_discrim(name: &str) -> (Scratch, FakeIndex) {
    let repo = Scratch::new(name);
    repo.write("proto/hello_v1.proto", &greeter_proto("hello.v1"))
        .write("proto/hello_v2.proto", &greeter_proto("hello.v2"))
        .write(
            "src/lib.rs",
            "//! A caller of both versions.\npub struct Caller { v1: GreeterClient<()> }\n",
        )
        .write(STUB_V1, STUB)
        .write(STUB_V2, STUB);

    let mut symbols: Vec<Symbol> = Vec::new();
    for file in [STUB_V1, STUB_V2] {
        let client = Sym::impl_block(
            "GreeterClient",
            "impl GreeterClient<T>",
            file,
            repo.offset(file, "impl<T>", 0),
        )
        .build();
        symbols.push(
            Sym::method("say_hello", file, repo.offset(file, "pub async fn", 0))
                .inside(&client)
                .build(),
        );
        symbols.push(client);
    }
    (repo, FakeIndex::new(symbols))
}

/// B2 — two consumed versions, two generated stubs: each reason names both
/// packages and both stubs' `file:line`.
#[test]
fn consumed_versions_name_every_package_and_every_stub_location() {
    let (repo, index) = consumed_discrim("b2-consumed");
    let roots = repo.roots(&ProtoTonicPlugin::reading_build_output(), &index);

    let found = say_hello_roots(&roots);
    assert_eq!(found.len(), 2, "one root per version: {found:#?}");
    for root in found {
        assert_eq!(root.direction, Direction::Consumed, "{root:?}");
        let reason = reason_of(root);
        let v1 = format!("{STUB_V1}:3");
        let v2 = format!("{STUB_V2}:3");
        for expected in ["hello.v1", "hello.v2", v1.as_str(), v2.as_str()] {
            assert!(
                reason.contains(expected),
                "{}: the reason lacks `{expected}`: {reason}",
                root.join_key
            );
        }
    }
}

/// B3 — one package, many impls: tonic's `examples` after task 2064. The reason
/// states the count, the package, and every candidate's location.
#[test]
fn many_impls_in_one_package_state_count_package_and_locations() {
    let repo = Scratch::new("b3-many");
    let mut lib = String::from("//! Twenty greeters.\n");
    for n in 0..20 {
        lib.push_str(&format!(
            "impl Greeter for S{n} {{\n    fn say_hello(&self) {{}}\n}}\n"
        ));
    }
    repo.write("proto/helloworld.proto", &greeter_proto("examples"))
        .write("src/lib.rs", &lib);

    let mut symbols = Vec::new();
    for n in 0..20 {
        let header = format!("impl Greeter for S{n}");
        let block = Sym::impl_block(
            &format!("S{n}"),
            &header,
            "src/lib.rs",
            repo.offset("src/lib.rs", &format!("{header} {{"), 0),
        )
        .build();
        symbols.push(
            Sym::method(
                "say_hello",
                "src/lib.rs",
                repo.offset("src/lib.rs", "fn say_hello", n),
            )
            .inside(&block)
            .build(),
        );
        symbols.push(block);
    }
    let roots = repo.roots(&ProtoTonicPlugin::new(), &FakeIndex::new(symbols));

    let found = say_hello_roots(&roots);
    assert_eq!(found.len(), 1);
    let reason = reason_of(found[0]);
    assert!(reason.contains("20"), "the count: {reason}");
    assert!(reason.contains("examples"), "the package: {reason}");
    // Impl `n` opens on line 2 + 3n, and its `say_hello` is the line after.
    for n in 0..20 {
        let location = format!("src/lib.rs:{}", 3 + 3 * n);
        assert!(
            reason.contains(&format!("{location} ")) || reason.ends_with(&location),
            "candidate {n} at {location} is not listed: {reason}"
        );
    }
}

/// B4 — exactly one candidate binds, as it did before, to the same node.
#[test]
fn a_single_candidate_still_binds_to_the_same_node() {
    let repo = Scratch::new("b4-single");
    repo.write("proto/hello_v1.proto", &greeter_proto("hello.v1"))
        .write(
            "src/lib.rs",
            "pub struct Svc;\nimpl v1::Greeter for Svc {\n    fn say_hello(&self) {}\n}\n",
        );
    let block = Sym::impl_block(
        "Svc",
        "impl Greeter for Svc",
        "src/lib.rs",
        repo.offset("src/lib.rs", "impl v1", 0),
    )
    .build();
    let handler = Sym::method(
        "say_hello",
        "src/lib.rs",
        repo.offset("src/lib.rs", "fn say_hello", 0),
    )
    .inside(&block)
    .build();
    let expected = handler.id.clone();
    let index = FakeIndex::new(vec![handler, block]);

    let roots = repo.roots(&ProtoTonicPlugin::new(), &index);
    let found = say_hello_roots(&roots);
    assert_eq!(found.len(), 1);
    let RootBinding::Bound(node) = &found[0].binding else {
        panic!("one candidate binds: {:?}", found[0]);
    };
    assert_eq!(*node, expected);
    assert_eq!(
        index.get(&expected).map(|symbol| symbol.name.as_str()),
        Some("say_hello")
    );
}

/// B5 — a consumed stub the build output does not hold keeps its reason,
/// word for word.
#[test]
fn a_stub_missing_from_build_output_keeps_its_reason() {
    let repo = Scratch::new("b5-missing");
    repo.write("proto/hello_v1.proto", &greeter_proto("hello.v1"))
        .write(
            "src/lib.rs",
            "pub struct Caller { v1: GreeterClient<()> }\n",
        );

    let roots = repo.roots(
        &ProtoTonicPlugin::reading_build_output(),
        &FakeIndex::new(Vec::new()),
    );
    let found = say_hello_roots(&roots);
    assert_eq!(found.len(), 1);
    let expected = UnboundReason::GeneratedStubNotInBuildOutput {
        join_key: "hello.v1.Greeter/SayHello".to_owned(),
    }
    .to_string();
    assert_eq!(reason_of(found[0]), expected);
}
