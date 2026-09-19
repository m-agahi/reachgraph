//! The vendored JavaScript, compiled in — plan-05 §3 and §7.
//!
//! ADR-0001 forbids runtime downloads, and a `<script src="https://…">` is
//! one: the browser performs it instead of the binary, and it fails in exactly
//! the places ADR-0001 cares about — an air-gapped runner, an unreachable
//! host, a two-year-old archived artifact. **There is no CDN option in this
//! crate, not even behind a flag**, and `page_has_no_external_script_src`
//! turns that sentence into a build failure.
//!
//! Each bundle is `include_str!`d from `vendor/`, written to `out/vendor/`
//! byte for byte, and inlined into `overview.html` byte for byte. No
//! minification, no re-bundling, no banner stripping runs over any of them:
//! emitting them is distribution, so the licence has to travel (§7).
//!
//! # The banner is not on every bundle
//!
//! Plan-05 §8.3 writes `licence_banner_survives_emission` as though each file
//! carried a `/*! … MIT … */` banner. MEASURED 2026-09-19: only
//! `cytoscape.min.js` does. The three iVis bundles are webpack UMD output
//! beginning `(function webpackUniversalModuleDefinition(…`, with no banner at
//! all. The notice for those travels in [`LICENSES`], which is emitted beside
//! them, and [`Bundle::notice`] records which of the two routes each file
//! takes so the guard asserts the right thing per file rather than a rule that
//! is false for three quarters of the set.

/// How a bundle's licence notice reaches the artifact.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Notice {
    /// The file opens with its own licence banner, and emitting the file
    /// verbatim is what carries it.
    Banner,
    /// The file carries no banner. Its licence text is in [`LICENSES`].
    LicensesFile,
}

/// One vendored bundle: what it is, and the bytes that ship.
#[derive(Clone, Copy, Debug)]
pub struct Bundle {
    /// The file name, under `vendor/` on both sides.
    pub file: &'static str,
    /// The upstream package name.
    pub name: &'static str,
    /// The exact version vendored.
    pub version: &'static str,
    /// Its SPDX identifier.
    pub spdx: &'static str,
    /// Where the notice is.
    pub notice: Notice,
    /// The bytes, compiled in.
    pub source: &'static str,
}

impl Bundle {
    /// The path this bundle is emitted at, relative to the artifact root.
    pub fn output_path(&self) -> String {
        format!("vendor/{}", self.file)
    }
}

/// The MIT notices for the bundles that carry none of their own, plus a
/// pointer to the one that does.
pub const LICENSES: &str = include_str!("../vendor/LICENSES.txt");

/// The path `LICENSES` is emitted at.
pub const LICENSES_PATH: &str = "vendor/LICENSES.txt";

/// Every vendored bundle, **in load order**.
///
/// Order is contract, not presentation. Each is a UMD bundle that registers
/// itself on `window` and reads what the previous one registered:
/// `layout-base`, then `cose-base`, then `cytoscape-fcose`, with `cytoscape`
/// itself first. `page_references_every_vendored_bundle` asserts the page
/// carries all of them; this array is what fixes the sequence.
pub const BUNDLES: [Bundle; 5] = [
    Bundle {
        file: "cytoscape.min.js",
        name: "cytoscape",
        version: "3.34.2",
        spdx: "MIT",
        notice: Notice::Banner,
        source: include_str!("../vendor/cytoscape.min.js"),
    },
    Bundle {
        file: "layout-base.js",
        name: "layout-base",
        version: "2.0.1",
        spdx: "MIT",
        notice: Notice::LicensesFile,
        source: include_str!("../vendor/layout-base.js"),
    },
    Bundle {
        file: "cose-base.js",
        name: "cose-base",
        version: "2.2.0",
        spdx: "MIT",
        notice: Notice::LicensesFile,
        source: include_str!("../vendor/cose-base.js"),
    },
    Bundle {
        file: "cytoscape-fcose.js",
        name: "cytoscape-fcose",
        version: "2.2.0",
        spdx: "MIT",
        notice: Notice::LicensesFile,
        source: include_str!("../vendor/cytoscape-fcose.js"),
    },
    // Plan-05 §6.2's collapsible module boxes. Cytoscape draws compound
    // parents natively — which is why §2 chose it over Sigma — but it has no
    // collapse of its own. Unlike fcose this one does not self-register; the
    // presenter registers it inside a guard, so a page whose extension fails
    // still draws its graph.
    Bundle {
        file: "cytoscape-expand-collapse.js",
        name: "cytoscape-expand-collapse",
        version: "4.1.1",
        spdx: "MIT",
        notice: Notice::LicensesFile,
        source: include_str!("../vendor/cytoscape-expand-collapse.js"),
    },
];
