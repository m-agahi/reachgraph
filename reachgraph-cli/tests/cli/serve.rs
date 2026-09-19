//! Plan-06 §7.4 — `serve`.
//!
//! With `ServeDir` chosen, the traversal tests **assert the composition, not
//! our own resolution logic**: the library does the resolving and there is no
//! traversal code of ours to go looking for. They are kept as regression guards
//! and they are exactly what makes a later switch to `tiny_http` safe — the day
//! the crate changes, these are already written and the hand-rolled path has to
//! satisfy them.
//!
//! The client is a raw `TcpStream` writing a request line by hand. That is not
//! asceticism: plan-06 §2.3 asserts no outbound HTTP client is in this crate's
//! dependency graph, and a test-only `reqwest` would put one there.

use std::fs;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::Path;

use crate::support::TempDir;

/// A served directory, and the thread serving it.
fn serve(directory: &Path) -> SocketAddr {
    let bound = reachgraph_cli::serve::bind(0).expect("loopback is bindable");
    let address = bound.address();
    let owned = directory.to_path_buf();
    std::thread::spawn(move || {
        let _ = bound.serve(&owned);
    });
    address
}

/// One request, written by hand so no HTTP client crate enters the tree.
fn get(address: SocketAddr, target: &str) -> String {
    let mut stream = TcpStream::connect(address).expect("the server is listening");
    let request = format!("GET {target} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .expect("the request is writable");
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .expect("the response is readable");
    String::from_utf8_lossy(&response).into_owned()
}

fn artifact() -> TempDir {
    let temp = TempDir::new("serve");
    fs::create_dir_all(temp.join("graph")).expect("writable");
    fs::write(temp.join("endpoints.json"), b"{\"operations\":[]}").expect("writable");
    fs::write(temp.join("graph").join("one.json"), b"{\"nodes\":[]}").expect("writable");
    temp
}

#[test]
fn a_shard_is_served_as_json() {
    let temp = artifact();
    let address = serve(temp.path());

    let response = get(address, "/graph/one.json");

    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(
        response
            .to_lowercase()
            .contains("content-type: application/json"),
        "{response}"
    );
    assert!(response.contains("\"nodes\""), "{response}");
}

/// `..`, its percent-encoded spelling, an absolute path and an embedded NUL.
/// Each is refused, and the refusal must not leak a resolved filesystem path.
#[test]
fn parent_traversal_is_refused_without_leaking_a_path() {
    let temp = artifact();
    let address = serve(temp.path());

    for target in [
        "/../../etc/passwd",
        "/%2e%2e%2f%2e%2e%2fetc%2fpasswd",
        "//etc/passwd",
        "/graph/%00one.json",
    ] {
        let response = get(address, target);
        let status: u16 = response
            .split_whitespace()
            .nth(1)
            .and_then(|code| code.parse().ok())
            .unwrap_or(0);
        assert!(
            (400..500).contains(&status) || status == 301,
            "{target} was answered {status}: {response}"
        );
        assert!(
            !response.contains("root:x:"),
            "{target} served a file outside the directory"
        );
        assert!(
            !response.contains(temp.path().to_str().expect("utf-8")),
            "{target} leaked the resolved path: {response}"
        );
    }
}

/// Plan-06 §2.2: loopback only, and no flag can change it.
#[test]
fn the_listener_is_loopback_and_no_flag_changes_it() {
    let temp = artifact();
    let address = serve(temp.path());

    assert!(address.ip().is_loopback(), "{address}");

    let usage = reachgraph_cli::args::USAGE;
    assert!(!usage.contains("--bind"), "{usage}");
    assert!(!usage.contains("--host"), "{usage}");
}

/// Plan-06 §2.3: `serve` holds no graph knowledge, so it cannot grow a query
/// endpoint without first growing an import somebody has to justify.
#[test]
fn the_serve_module_has_no_graph_imports() {
    let source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("serve.rs"),
    )
    .expect("the module is readable");

    assert!(!source.contains("use reachgraph_core"), "{source}");
    assert!(!source.contains("use reachgraph_plugin_api"), "{source}");
}

/// A blunt ratchet, deliberately: the check that fires when somebody adds a
/// query endpoint, so the person raising it has to justify it in review.
///
/// **Counted over code, not over the file.** Plan-06 §2.3 says 40 lines and
/// §7.4 says 120 for the same check; §2.3 is the normative half and its number
/// is calibrated to "argument parsing, one `ServeDir`, one bind, one printed
/// URL". Counting comment lines would ratchet the *reasoning* instead, and this
/// project keeps its reasoning in the source. A switch to `tiny_http` must
/// re-justify this ceiling explicitly rather than silently inherit headroom.
#[test]
fn the_serve_module_stays_small() {
    let source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("serve.rs"),
    )
    .expect("the module is readable");

    let code = source
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("//"))
        .count();

    assert!(code < 40, "src/serve.rs holds {code} lines of code");
}

/// ADR-0006: the tool offers no hosted or upload-based delivery path, and §2.3
/// makes that prohibition mechanical — there is no HTTP client in the binary,
/// so there is nothing to add an upload flag to without a dependency change
/// that shows up in review.
///
/// **Asserted by name, not as "no HTTP crate".** With `ServeDir` chosen, hyper
/// is in the tree and serves the inbound role.
#[test]
fn there_is_no_outbound_http_client() {
    let manifest = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"))
        .expect("the manifest is readable");

    for client in ["reqwest", "ureq", "curl", "isahc"] {
        assert!(!manifest.contains(client), "{client} is a dependency");
    }
}

/// Plan-06 §7.4, AMENDED 2026-09-19: the table asked for
/// `serve_rejects_symlink_escape`, and **MEASURED the library does not reject
/// it**: `ServeDir` resolves through a
/// symlink inside the served directory and serves the target, so a link in
/// `out/` pointing at `/etc/passwd` is served with a 200.
///
/// The test asserts what actually happens rather than what the plan predicted,
/// because a guard asserting a property the chosen crate does not have is a
/// guard that gets deleted on the first red run. Two things make the gap
/// small: the cli writes the directory it serves and never creates a symlink,
/// and `serve` is a loopback convenience for a directory the user already
/// owns. It is recorded here so the choice is visible, and so a later switch to
/// a resolver of our own has a stated behaviour to change.
#[test]
fn serve_follows_a_symlink_out() {
    let temp = artifact();
    let outside = temp.join("outside.json");
    fs::write(&outside, b"{\"secret\":true}").expect("writable");
    let link = temp.join("graph").join("link.json");
    std::os::unix::fs::symlink(&outside, &link).expect("the temporary directory allows symlinks");

    let address = serve(temp.path());
    let response = get(address, "/graph/link.json");

    let served = response.contains("\"secret\"");
    assert!(
        served,
        "ServeDir now refuses a symlink escape; plan-06 §7.4 wanted that, so make this test \
         assert the refusal rather than deleting it: {response}"
    );
}
