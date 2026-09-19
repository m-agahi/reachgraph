//! `cargo xtask third-party-licenses` — plan-07 §6.2's generated attribution.
//!
//! The binary links its dependencies statically and the artifact writes the
//! vendored JavaScript out, so a release redistributes both. MIT and Apache-2.0
//! each require the notice to travel with the redistribution, and
//! `THIRD-PARTY-LICENSES.md` is where it travels.
//!
//! # Two halves, one file, because only one tool can see one of them
//!
//! `cargo-about` walks the crate graph and reads each crate's own licence
//! files. It cannot see the five UMD bundles in
//! `reachgraph-render-html/vendor/`, because they are not crates — ADR-0001
//! names this exact residual: removing npm from the chain also removed npm's
//! tooling. So the JavaScript half is generated here from `VENDOR.toml`, which
//! is the file an upstream bump edits, and
//! `xtask/tests/packaging.rs::every_vendored_bundle_is_attributed` asserts that
//! every bundle in that table reached the notice.
//!
//! # Why this shells out
//!
//! ADR-0001 forbids subprocesses in the **artifact**. `xtask` is a dependency
//! of nothing shipped, and the crate's own manifest says so. `cargo-about` has
//! to be on `PATH`; a missing tool fails loudly here rather than producing a
//! shorter notice, which is the same posture the `language: system` pre-commit
//! hooks take.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

/// The vendored-JavaScript table, `reachgraph-render-html/vendor/VENDOR.toml`.
#[derive(Debug, Deserialize)]
struct VendoredJs {
    bundle: Vec<Bundle>,
}

/// One UMD bundle, as `VENDOR.toml` records it.
#[derive(Debug, Deserialize)]
struct Bundle {
    name: String,
    version: String,
    spdx: String,
    source: String,
}

/// Anything that stops the notice being produced.
#[derive(Debug)]
pub enum Error {
    /// A file the generator reads is missing or unreadable.
    Read(PathBuf, io::Error),
    /// `cargo-about` could not be started at all.
    ToolMissing(io::Error),
    /// `cargo-about` ran and failed.
    ToolFailed(String),
    /// `VENDOR.toml` did not parse.
    Table(String),
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(path, error) => write!(formatter, "{}: {error}", path.display()),
            Self::ToolMissing(error) => write!(
                formatter,
                "cargo-about could not be run ({error}). It is not installed by this task — \
                 `nix shell nixpkgs#cargo-about` or `cargo install cargo-about`."
            ),
            Self::ToolFailed(detail) => write!(formatter, "cargo about generate failed:\n{detail}"),
            Self::Table(detail) => write!(formatter, "VENDOR.toml does not parse: {detail}"),
        }
    }
}

impl std::error::Error for Error {}

/// Where the generated notice lives.
pub fn notice_path(root: &Path) -> PathBuf {
    root.join("THIRD-PARTY-LICENSES.md")
}

fn read(path: &Path) -> Result<String, Error> {
    std::fs::read_to_string(path).map_err(|error| Error::Read(path.to_path_buf(), error))
}

/// Render the whole notice: the crate half from `cargo-about`, then the
/// JavaScript half from `VENDOR.toml` and the vendored `LICENSES.txt`.
pub fn render(root: &Path) -> Result<String, Error> {
    let mut rendered = crates_section(root)?;

    if !rendered.ends_with('\n') {
        rendered.push('\n');
    }
    rendered.push_str(&javascript_section(root)?);

    Ok(hook_clean(&rendered))
}

/// Emit what the whitespace hooks would leave alone.
///
/// MEASURED 2026-09-19: with the file unrestricted, `trailing-whitespace`
/// removed 119 lines' worth of trailing spaces from the reproduced Apache-2.0
/// text. That is not cosmetic — it makes `cargo xtask third-party-licenses`
/// report the committed notice as out of date for ever, because the generator
/// and the hook disagree about the same bytes, and plan-07 §7.3's
/// regenerates-identically check is then unpassable.
///
/// ADR-0738 solved the same collision for the vendored JavaScript by excluding
/// the files from the hooks. That is the wrong shape here, and the difference
/// is what is being protected: there the guard is a sha256 of what UPSTREAM
/// published, so a single changed byte is the failure. Here the guard is that
/// regeneration is reproducible, and the notice's obligation is that the WORDS
/// travel. A trailing space is not a word. Normalising here keeps four hooks
/// unrestricted and costs the notice nothing.
///
/// Whitespace only, and only at the ends: no line is joined, split, reflowed or
/// re-indented, and no character inside a line is touched.
fn hook_clean(text: &str) -> String {
    let mut cleaned: String = text
        .replace("\r\n", "\n")
        .lines()
        .map(|line| line.trim_end_matches([' ', '\t']))
        .collect::<Vec<_>>()
        .join("\n");
    cleaned.push('\n');
    cleaned
}

/// `cargo about generate`, run against this workspace's `about.toml`.
fn crates_section(root: &Path) -> Result<String, Error> {
    let output = Command::new("cargo")
        .arg("about")
        .arg("generate")
        .arg("--config")
        .arg(root.join("about.toml"))
        .arg("--manifest-path")
        .arg(root.join("reachgraph-cli/Cargo.toml"))
        .arg("--locked")
        .arg(root.join("about.hbs"))
        .output()
        .map_err(Error::ToolMissing)?;

    if !output.status.success() {
        return Err(Error::ToolFailed(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// The half `cargo-about` cannot see.
///
/// The per-bundle rows come from `VENDOR.toml` so that adding a bundle there
/// and forgetting it here is impossible; the licence texts come from the same
/// `LICENSES.txt` the artifact itself carries, so the notice a user reads in
/// `out/vendor/` and the notice in the wheel are the same words.
fn javascript_section(root: &Path) -> Result<String, Error> {
    let vendor = root.join("reachgraph-render-html/vendor");
    let table: VendoredJs = toml::from_str(&read(&vendor.join("VENDOR.toml"))?)
        .map_err(|error| Error::Table(error.to_string()))?;
    let texts = read(&vendor.join("LICENSES.txt"))?;

    let mut section = String::new();
    section.push_str(
        "\n## JavaScript bundled into the binary and written into every artifact\n\n\
         `reachgraph-render-html` compiles these in with `include_str!` and writes them to\n\
         `out/vendor/` byte for byte. Emitting them is distribution, so the notice travels twice:\n\
         here, and in the artifact's own `vendor/LICENSES.txt`. No minification or\n\
         banner-stripping pass runs over them — for `cytoscape.min.js` the `/*! … */` banner **is**\n\
         the notice.\n\n\
         `cargo-about` cannot see these: they are not crates, and ADR-0001 names that residual.\n\
         They are generated here from `reachgraph-render-html/vendor/VENDOR.toml`.\n\n",
    );

    section.push_str("| bundle | version | SPDX | obtained from |\n");
    section.push_str("| ------ | ------- | ---- | ------------- |\n");
    for bundle in &table.bundle {
        section.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            bundle.name, bundle.version, bundle.spdx, bundle.source
        ));
    }

    section.push_str(
        "\n<details><summary>Licence texts, as the artifact carries them</summary>\n\n<pre>\n",
    );
    section.push_str(&escape_html(&texts));
    section.push_str("</pre>\n\n</details>\n");

    Ok(section)
}

/// The licence texts go inside a `<pre>`, so the three characters that would
/// close it early are escaped. Nothing else is touched: the texts are somebody
/// else's words and this file reproduces them.
fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
