//! ADR-0007's version, for gRPC — plan-04 §5.
//!
//! Per-contract knowledge, which is exactly why it lives in a plugin and never
//! in the waist. For gRPC the version is **structural**: it is a segment of the
//! proto package, and the generated path carries it mechanically.

/// The endpoint version a proto package declares, if it declares one.
///
/// ```text
/// version = last dot-segment of `package` matching  ^v[0-9]+([a-z]+[0-9]+)?$
/// ```
///
/// MEASURED: `yadgar.taskapi.v1` → `Some("v1")`. The pattern is Google's API
/// versioning convention, so `v2`, `v1beta1` and `v1alpha2` match while `v`,
/// `version1`, `api` and `vnext` do not.
///
/// # `None` is an assertion
///
/// ADR-0007 is explicit and plan-04 §5 restates the reason: `"v1"` would
/// fabricate a distinction the contract does not make, and would make two
/// genuinely unversioned APIs look like the same version of one API. `None` is
/// a true statement about the contract, so a package with no version segment
/// and a file with no package at all both produce it.
pub fn version_of_package(package: Option<&str>) -> Option<String> {
    let last = package?.rsplit('.').next()?;
    is_version_segment(last).then(|| last.to_owned())
}

/// `^v[0-9]+([a-z]+[0-9]+)?$`, written out rather than pulled in.
///
/// A regex crate for one anchored pattern would be a dependency ADR-0001 has to
/// justify, and the pattern is four states.
fn is_version_segment(segment: &str) -> bool {
    let Some(rest) = segment.strip_prefix('v') else {
        return false;
    };
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return false;
    }
    let rest = &rest[digits.len()..];
    if rest.is_empty() {
        return true;
    }

    // The optional stability suffix: letters then digits, both non-empty.
    let letters: String = rest
        .chars()
        .take_while(|c| c.is_ascii_lowercase())
        .collect();
    if letters.is_empty() {
        return false;
    }
    let tail = &rest[letters.len()..];
    !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit())
}
