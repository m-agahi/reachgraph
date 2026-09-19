//! Plan-04 §5, ADR-0007 — the version is a fact about the contract.

use reachgraph_roots_proto_tonic::version::version_of_package;

#[test]
fn version_segment_variants() {
    for package in ["yadgar.taskapi.v1", "acme.api.v1"] {
        assert_eq!(version_of_package(Some(package)), Some("v1".to_owned()));
    }
    assert_eq!(
        version_of_package(Some("acme.store.v2")),
        Some("v2".to_owned())
    );
    assert_eq!(
        version_of_package(Some("acme.api.v1beta1")),
        Some("v1beta1".to_owned())
    );
    assert_eq!(
        version_of_package(Some("acme.api.v1alpha2")),
        Some("v1alpha2".to_owned())
    );
}

/// The near-misses. Each one is a package segment that looks like a version and
/// is not one.
#[test]
fn near_misses_are_not_versions() {
    for package in [
        "acme.api.v",
        "acme.api.version1",
        "acme.api",
        "acme.api.vnext",
        "acme.api.V1",
        "acme.api.v1beta",
        "acme.api.v1beta1x",
        "acme.api.v01a",
    ] {
        assert_eq!(version_of_package(Some(package)), None, "{package}");
    }
}

/// ADR-0007, stated as an assertion a future default would fail.
///
/// `!= Some("v1")` is written out rather than implied by `== None`, so a change
/// that starts defaulting fails **by name** and the failure names the ADR.
#[test]
fn missing_version_stays_none() {
    assert_eq!(version_of_package(Some("acme.legacy")), None);
    assert_ne!(
        version_of_package(Some("acme.legacy")),
        Some("v1".to_owned()),
        "ADR-0007: a missing version is None, never defaulted to v1"
    );

    assert_eq!(version_of_package(None), None, "a file with no package");
    assert_ne!(version_of_package(None), Some("v1".to_owned()));
}

/// Only the **last** segment is the version.
#[test]
fn only_the_last_segment_counts() {
    assert_eq!(
        version_of_package(Some("acme.v1.api")),
        None,
        "a version-shaped segment in the middle is part of the package name"
    );
    assert_eq!(
        version_of_package(Some("v1")),
        Some("v1".to_owned()),
        "a one-segment package can be the version segment"
    );
}
