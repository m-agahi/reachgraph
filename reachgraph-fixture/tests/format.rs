//! Plan-02 §7.4 — the format rules, asserted rather than reviewed.
//!
//! **The artifact every test here asserts on is the parse result.** Not a
//! constructor's return, not a loaded plugin: a rule about what the format
//! accepts is a claim about `serde_json::from_str::<FixtureDoc>`, so that is
//! what is called. Each case starts from the `minimal` document and removes or
//! adds exactly one key, so the assertion is about that key and not about
//! whatever else a hand-written malformed document happened to get wrong.
//!
//! The last three are source assertions, and they are (C)-strength — they read
//! the format module's text. They exist because the corresponding positive test
//! cannot: there is no way to test that a field is absent from a format except
//! by reading the format.

use std::path::{Path, PathBuf};

use reachgraph_fixture::format::{FixtureDetection, FixtureDoc};
use reachgraph_fixture::FixturePlugin;
use reachgraph_plugin_api::{Plugin, Root, RootProvider, Symbol, SymbolIndex};
use serde_json::Value;

fn minimal_value() -> Value {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/minimal/reachgraph.fixture.json");
    let text = std::fs::read_to_string(&path).expect("the minimal case is readable");
    serde_json::from_str(&text).expect("the minimal case is JSON")
}

fn parse(value: &Value) -> Result<FixtureDoc, serde_json::Error> {
    serde_json::from_value(value.clone())
}

/// The object at `pointer`, for a test that is about to break it.
fn at<'a>(value: &'a mut Value, pointer: &str) -> &'a mut serde_json::Map<String, Value> {
    value
        .pointer_mut(pointer)
        .unwrap_or_else(|| panic!("{pointer} is in the minimal document"))
        .as_object_mut()
        .unwrap_or_else(|| panic!("{pointer} is an object"))
}

/// Remove `key` at `pointer`, asserting it was there to remove.
///
/// Without that assertion a renamed key would make every test below pass by
/// deleting nothing.
fn without(pointer: &str, key: &str) -> Value {
    let mut value = minimal_value();
    let removed = at(&mut value, pointer).remove(key);
    assert!(
        removed.is_some(),
        "{pointer} has no {key} to remove, so this test would assert nothing"
    );
    value
}

/// The unedited document parses. Every test here is a one-key edit away from
/// this, so if it did not, the edits would prove nothing.
#[test]
fn the_unedited_minimal_document_parses() {
    parse(&minimal_value()).expect("the minimal case is the baseline for every edit below");
}

// ---------------------------------------------------------------------------
// Keys that cannot be forgotten
// ---------------------------------------------------------------------------

/// `deny_unknown_fields` on every struct: a typo'd key is a parse error rather
/// than a silently ignored field.
#[test]
fn unknown_field_is_a_parse_error() {
    for pointer in [
        "",
        "/units/0",
        "/symbols/unit:app/0",
        "/edges/unit:app/0",
        "/roots/0",
    ] {
        let mut value = minimal_value();
        at(&mut value, pointer).insert("plugin_od".to_owned(), Value::from(1));

        assert!(
            parse(&value).is_err(),
            "an unknown key at {pointer:?} was accepted. A typo would then be data the \
             author thinks they wrote and the parser silently dropped."
        );
    }
}

/// ADR-0007, and plan-02 §7.1 step 8.
///
/// Asserted twice against two different artifacts: the `version_key_missing`
/// corpus case as it sits on disk, and a one-key edit of `minimal`. The first
/// is what a case author would write; the second proves the failure is about
/// the `version` key and nothing else in that file.
#[test]
fn missing_version_key_is_a_parse_error() {
    let case = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/version_key_missing/reachgraph.fixture.json");
    let text = std::fs::read_to_string(&case).expect("the case is readable");
    assert!(
        serde_json::from_str::<FixtureDoc>(&text).is_err(),
        "version_key_missing parsed. A missing version must be a parse error, because \
         ADR-0007 needs `None` to be an assertion the author made rather than an absence \
         the harness filled in."
    );

    assert!(parse(&without("/roots/0", "version")).is_err());
}

/// The same rule, one level along: the coverage record has to distinguish a
/// null version from an omitted one too.
#[test]
fn missing_coverage_version_key_is_a_parse_error() {
    assert!(parse(&without("/coverage/versions/0", "version")).is_err());
}

/// ADR-0007: `null` is sayable, absence is not — and `None` is never `"v1"`.
#[test]
fn null_version_parses_to_none_and_never_v1() {
    let mut value = minimal_value();
    at(&mut value, "/roots/0").insert("version".to_owned(), Value::Null);

    let doc = parse(&value).expect("null is a valid version");
    assert_eq!(doc.roots[0].version, None);

    // And it survives the trip through the plugin into a contract `Root`.
    let roots = roots_of(&FixturePlugin::from_doc(PathBuf::from("."), doc));
    assert_eq!(roots[0].version, None);
    assert_ne!(roots[0].version.as_deref(), Some("v1"));
}

/// Plan-00 §8 question 3: a plugin author cannot forget the enclosing
/// definition. `null` is the way to say there is none.
#[test]
fn missing_container_key_is_a_parse_error() {
    assert!(parse(&without("/symbols/unit:app/0", "container")).is_err());
}

/// The same for documentation text: absent and "there is none" are different
/// claims, and `docs/design.md` §5 MEASURED what conflating them costs.
#[test]
fn missing_doc_key_is_a_parse_error() {
    assert!(parse(&without("/symbols/unit:app/0", "doc")).is_err());
}

/// ADR-0003 field 4. An edge without provenance or inference mode is
/// unrepresentable rather than merely discouraged.
#[test]
fn missing_inference_mode_is_a_parse_error() {
    assert!(parse(&without("/edges/unit:app/0", "inference_mode")).is_err());
    assert!(parse(&without("/edges/unit:app/0", "provenance_plugin")).is_err());
}

/// Plan-00 §2: the cross-repository key is spelled by the plugin and cannot be
/// forgotten. A plugin that ships without emitting it produces roots from which
/// the key cannot be recovered afterwards.
#[test]
fn missing_join_key_is_a_parse_error() {
    assert!(parse(&without("/roots/0", "join_key")).is_err());
}

/// Plan-02 §7.4: emitted verbatim, and never a source of anything.
///
/// The document below is deliberately self-contradictory — a `join_key` that
/// says `v9` on a root whose `version` says `v1`. Both survive unchanged. A
/// plugin that parsed the key would have to pick one, and a core that parsed it
/// would be reading a framework-shaped string ADR-0003 keeps out of the waist.
#[test]
fn join_key_is_never_parsed_by_the_fixture() {
    const CONTRADICTORY: &str = "acme.task.v9.TaskService/CreateTask";

    let mut value = minimal_value();
    at(&mut value, "/roots/0").insert("join_key".to_owned(), Value::from(CONTRADICTORY));

    let doc = parse(&value).expect("a contradictory join key is still a valid string");
    let roots = roots_of(&FixturePlugin::from_doc(PathBuf::from("."), doc));

    assert_eq!(roots[0].join_key, CONTRADICTORY);
    assert_eq!(
        roots[0].version.as_deref(),
        Some("v1"),
        "the version came from the version key, not from the join key"
    );
}

/// The declaration a corpus guard checks has to be the case's own.
///
/// `fixture_detection_is_always_empty` asserts that every case declares no
/// markers. This proves the guard could fail: a case declaring one is reported
/// with it, rather than having the empty list this crate would prefer
/// substituted for it.
#[test]
fn a_declared_marker_file_is_reported_not_substituted() {
    let mut doc = parse(&minimal_value()).expect("the minimal case parses");
    doc.detection = FixtureDetection {
        marker_files: vec!["reachgraph.fixture.json".to_owned()],
        extensions: vec!["fixture".to_owned()],
    };

    let plugin = FixturePlugin::from_doc(PathBuf::from("."), doc);
    assert_eq!(plugin.detection().marker_files, ["reachgraph.fixture.json"]);
    assert_eq!(plugin.detection().extensions, ["fixture"]);
}

// ---------------------------------------------------------------------------
// Source assertions (plan-02 §7.4, mechanism (C))
// ---------------------------------------------------------------------------

fn format_source() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/format.rs");
    let source = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "{} is unreadable ({error}). A source assertion whose path does not resolve \
             passes by reading nothing.",
            path.display()
        )
    });
    assert!(
        source.len() > 1_000,
        "the format module is suspiciously short; these assertions would read almost nothing"
    );
    source
}

/// The source with every comment line removed.
///
/// Load-bearing. The format module *must* document why it has no span and no
/// confidence number, so a naive `contains("span")` over the raw text would
/// fail against the very prose that explains the rule — and the tempting fix
/// would be to delete the explanation.
fn format_code() -> String {
    format_source()
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<&str>>()
        .join("\n")
}

/// Every named field the format module declares.
fn format_field_names() -> Vec<String> {
    let names: Vec<String> = format_code()
        .lines()
        .map(str::trim)
        .filter_map(|line| line.strip_prefix("pub "))
        .filter_map(|rest| rest.split_once(':'))
        .map(|(name, _)| name.trim().to_owned())
        .filter(|name| !name.is_empty() && name.chars().all(|c| c.is_ascii_lowercase() || c == '_'))
        .collect();

    assert!(
        names.len() > 20,
        "only {} field declarations were found, so the scan is not seeing the module",
        names.len()
    );
    names
}

/// Plan-02 §2.1 and §7.4. The rule the plan says to enforce in review, enforced
/// by something that runs instead.
///
/// It does not cover the whole hazard on its own, and the source says so: an
/// `Option` field is optional to serde whether or not anybody wrote
/// `#[serde(default)]`. This assertion catches the explicit spelling; the parse
/// tests above catch the implicit one.
#[test]
fn no_serde_default_in_fixture_format() {
    assert!(
        !format_code().contains("serde(default"),
        "src/format.rs declares a serde default. Every default is a place this harness \
         invents data the case author never wrote (plan-02 §2.1)."
    );
}

/// Plan-02 §4 and §7.4: no offset, line, column, cursor or position field
/// exists in the format, and `file` is present and is not one of them.
///
/// This is the crate's reason for existing, stated as an absence. If a change
/// to the contract makes a case want an offset, this is the test that must be
/// argued with — not quietly edited.
#[test]
fn format_has_no_span_field() {
    const FORBIDDEN: [&str; 14] = [
        "span",
        "offset",
        "offsets",
        "byte_offset",
        "line",
        "lines",
        "column",
        "col",
        "position",
        "cursor",
        "start",
        "end",
        "range",
        "text_range",
    ];

    let names = format_field_names();

    for name in &names {
        assert!(
            !FORBIDDEN.contains(&name.as_str()),
            "src/format.rs declares a field named `{name}`. The fixture has no offsets, and \
             a format that can express one lets a case lie about a fact it does not have \
             (plan-02 §3.1). `position_encoding` is allowed and is not one of these: it \
             declares which units offsets WOULD be counted in, not an offset."
        );
    }

    assert!(
        names.iter().any(|name| name == "file"),
        "`file` is missing. It is the one location fact the fixture does know, and \
         discarding it was the earlier shape this one replaced."
    );

    for spelling in ["Span", "TextSize", "TextRange", "FilePosition"] {
        assert!(
            !format_code().contains(spelling),
            "src/format.rs names `{spelling}`"
        );
    }
}

/// Plan-00 §8 question 5. A root either binds or it does not.
#[test]
fn format_has_no_confidence_field() {
    assert!(
        !format_code().contains("confidence"),
        "src/format.rs mentions confidence. `docs/design.md` §5 MEASURED what a float \
         invites: 59 edges at 0.55, each with two or three candidates — indecision \
         recorded as if it were a measurement."
    );
    assert!(!format_field_names().iter().any(|name| name == "confidence"));
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

struct NoIndex;

impl SymbolIndex for NoIndex {
    fn by_name(&self, _name: &str) -> Vec<&Symbol> {
        Vec::new()
    }

    fn in_file(&self, _path: &Path) -> Vec<&Symbol> {
        Vec::new()
    }

    fn get(&self, _id: &reachgraph_plugin_api::NodeId) -> Option<&Symbol> {
        None
    }
}

fn roots_of(plugin: &FixturePlugin) -> Vec<Root> {
    plugin
        .roots(Path::new("."), &NoIndex)
        .expect("roots are declared, not computed")
}
