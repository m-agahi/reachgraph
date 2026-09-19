//! ADR-0006's convenience command: a static file server over an output
//! directory.
//!
//! **No state, no API, no queries, no database, no templating, no analysis.**
//! The whole reason it exists is that `fetch()` against a `file://` origin is
//! CORS-blocked in current Chrome and Firefox, so lazily-loaded shards cannot
//! be read from a bare file open. It is a convenience, not architecture, and it
//! locks in nothing.
//!
//! **It must never grow into an application server.** ADR-0006's rationale is
//! not a taste preference: the graph cannot be computed at request time. The
//! whole-repository walk is an offline cost, so a server could only ever serve
//! data the analyse path precomputed — and a stateful one would add a store, an
//! API, a deployment and an authentication problem for private code, buying
//! nothing until cross-commit diffing exists, which is not in scope.
//!
//! # Why `ServeDir` rather than our own resolution
//!
//! "Roughly twenty lines" is only true with `ServeDir`, because `ServeDir` **is**
//! the traversal-safe path resolution plus the MIME table. `..`,
//! percent-encoded traversal and content types are library code other people
//! test. Hand-rolling them is the one place a static server earns a CVE — in a
//! tool whose entire risk surface is leaking a map of private source.
//!
//! MEASURED for plan-06 §2.1's open question: the feature adds 29 crates to a
//! 214-crate baseline. The flip threshold was ~5 MB on the stripped binary; the
//! measured delta is far below it, and `tests/cli/serve.rs` is written so a
//! later switch to `tiny_http` has to satisfy the same assertions.
//!
//! # Loopback only
//!
//! There is no `--bind` and no `--host` flag. The artifact is a structural map
//! of private source; serving it on `0.0.0.0` from a laptop on a shared network
//! or a CI runner is a disclosure with no compensating benefit. A user who
//! needs remote access forwards a port deliberately, which is their decision to
//! make and not ours to make for them.

use std::future::IntoFuture;
use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::Path;

/// A bound listener that has not started serving yet.
pub struct Bound {
    runtime: tokio::runtime::Runtime,
    listener: tokio::net::TcpListener,
    address: SocketAddr,
}

impl Bound {
    /// Where it is listening. Always loopback.
    pub fn address(&self) -> SocketAddr {
        self.address
    }

    /// Serve `directory` until the process ends.
    pub fn serve(self, directory: &Path) -> io::Result<()> {
        let files = tower_http::services::ServeDir::new(directory);
        let app = axum::Router::new().fallback_service(files);
        self.runtime
            .block_on(axum::serve(self.listener, app).into_future())
    }
}

/// Bind loopback on `port`, or on a free port when it is zero.
pub fn bind(port: u16) -> io::Result<Bound> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let listener = runtime.block_on(tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, port)))?;
    let address = listener.local_addr()?;
    Ok(Bound {
        runtime,
        listener,
        address,
    })
}
