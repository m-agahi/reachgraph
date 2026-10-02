//! The argument surface — plan-06 §1.
//!
//! Hand-rolled, for the reason `xtask` is: four subcommands and ten flags do
//! not pay for an argument-parsing crate, and ADR-0001 makes every dependency
//! in the shipped binary a decision rather than a detail.
//!
//! # The renderer flags, and what they select
//!
//! `--renderer`, `--inline-threshold` and `--no-overview` are plan-06 §1.1
//! flags that select and configure an output format. They arrived with
//! plan-05, which is the first build in which there is something to select: a
//! flag that parsed and then had nothing behind it would read as a supported
//! choice, which is worse than its absence.
//!
//! `--inline-threshold` and `--no-overview` are two settings of one decision
//! and the type they parse into says so — `Overview::Never` against
//! `Overview::Under(n)`, rather than a nullable number that has to be read
//! twice. Passing both is a usage error rather than a silent precedence rule.
//!
//! # What is absent, and why it is absent rather than unimplemented
//!
//! `--contract` is absent for a sharper reason. `RootProvider::roots` takes a
//! repository root and a symbol index, and nothing else; there is no parameter
//! through which a narrowed contract list could reach a provider. Accepting the
//! flag and dropping it would silently narrow nothing while looking as though
//! it narrowed something, and ADR-0007's partial-index problem is exactly the
//! class of defect that produces.

use std::path::PathBuf;

/// What the binary was asked to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// Analyse a repository.
    Analyse(Analyse),
    /// Run every detected plugin's preflight and stop.
    Preflight {
        /// The repository to check.
        repo: PathBuf,
        /// Emit the table as structured data on stdout.
        json: bool,
        /// ADR-0009: an existing build's target directory to read
        /// build-script output from. Preflight loads the workspace, so it
        /// takes the flag too, or it would check a different load than the
        /// run it precedes.
        build_output: Option<PathBuf>,
    },
    /// List what is registered.
    Plugins,
    /// Join several repositories' artifacts on `join_key` — ADR-0010.
    Merge {
        /// Each input: an `endpoints.json`, or the directory holding one,
        /// optionally prefixed `label=` to name the repository.
        inputs: Vec<String>,
        /// Where `estate.json` and `estate.html` go.
        out: PathBuf,
        /// Overwrite those two files when they already exist.
        force: bool,
    },
    /// Serve an output directory over loopback.
    Serve {
        /// The directory to serve.
        out: PathBuf,
        /// The port, or zero for a free one.
        port: u16,
    },
    /// Print the usage text.
    Help,
    /// Print the version.
    Version,
}

/// The analyse path's options.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Analyse {
    /// The repository to analyse.
    pub repo: PathBuf,
    /// Where the artifact goes.
    pub out: PathBuf,
    /// Write into a directory this tool did not produce.
    pub force: bool,
    /// Machine-readable report on stdout.
    pub json: bool,
    /// Suppress everything but errors.
    pub quiet: bool,
    /// Which output format to emit. `None` takes the registry's first.
    pub renderer: Option<String>,
    /// ADR-0006's single-file threshold, in bytes. `None` takes the
    /// renderer's own default.
    pub inline_threshold: Option<u64>,
    /// Never emit the single-file page. A different instruction from a small
    /// threshold, and the two are mutually exclusive.
    pub no_overview: bool,
    /// ADR-0009: a cargo target directory an earlier build populated, read
    /// for build-script output (`OUT_DIR`). `None` reads none. reachgraph
    /// never runs the build itself.
    pub build_output: Option<PathBuf>,
}

impl Default for Analyse {
    /// The shape `reachgraph plugins` describes: this build's formats at their
    /// own defaults, with no repository in hand.
    fn default() -> Self {
        Self {
            repo: PathBuf::new(),
            out: PathBuf::from(DEFAULT_OUT),
            force: false,
            json: false,
            quiet: false,
            renderer: None,
            inline_threshold: None,
            no_overview: false,
            build_output: None,
        }
    }
}

/// A usage error, with the message the user sees.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsageError(pub String);

/// Plan-06 §1's command surface, verbatim.
pub const USAGE: &str = "\
usage:
  reachgraph <repo> [-o|--out <dir>] [--force] [--json] [-q|--quiet]
                    [--renderer <name>] [--inline-threshold <bytes>]
                    [--no-overview] [--read-build-output <target-dir>]
  reachgraph merge [label=]<artifact> [label=]<artifact>... [-o|--out <dir>] [--force]
  reachgraph serve <out> [--port <n>]
  reachgraph preflight <repo> [--json] [--read-build-output <target-dir>]
  reachgraph plugins
  reachgraph --version | --help

  <repo>          a repository some registered plugin claims by its marker file
  -o, --out       where the artifact goes (default: ./out)
      --force     write into a directory reachgraph did not produce
      --json      machine-readable report on stdout; progress stays on stderr
  -q, --quiet     errors only
      --renderer  output format (default: the first `reachgraph plugins` lists)
      --inline-threshold
                  emit the single-file overview.html only when the graph JSON
                  is at most this many bytes (default: 5242880). The emitted
                  file is LARGER than this by the vendored JavaScript, on
                  purpose: the budget is over graph data, not over the page
      --no-overview
                  never emit overview.html. Not the same as a threshold of 0,
                  and the two may not be combined
      --read-build-output
                  read generated code (build-script OUT_DIR) from this cargo
                  target directory, which an earlier `cargo build` populated.
                  reachgraph never runs the build; with no build output
                  there, the run fails and names the path. Needs the
                  rust-src component
  <artifact>      for merge: a per-repository endpoints.json, or the output
                  directory holding it; prefix `label=` to name the repository
                  (default: the directory's name; labels must be unique).
                  merge writes estate.json and estate.html, joining consumed
                  and served RPCs on join_key
      --port      loopback port for `serve`, or 0 for a free one (default: 0)

exit codes: 0 analysed  1 internal error  2 preflight failed
            3 no plugin detected  4 bad usage

There is no --fail-on-unreachable, and that is deliberate. A partial root set
makes live code read as unreachable, and a build that fails on it converts a
knowingly-incomplete analysis into a merge blocker. The artifact reports; a
human reads.";

const DEFAULT_OUT: &str = "out";

/// Parse the argument list, excluding the program name.
pub fn parse(args: &[String]) -> Result<Command, UsageError> {
    let Some(first) = args.first() else {
        return Err(UsageError("no repository given".to_owned()));
    };

    match first.as_str() {
        "--help" | "-h" | "help" => return Ok(Command::Help),
        "--version" | "-V" => return Ok(Command::Version),
        "plugins" => return plugins(&args[1..]),
        "preflight" => return preflight(&args[1..]),
        "serve" => return serve(&args[1..]),
        "merge" => return merge(&args[1..]),
        _ => {}
    }

    analyse(args)
}

fn plugins(rest: &[String]) -> Result<Command, UsageError> {
    match rest.first() {
        None => Ok(Command::Plugins),
        Some(unexpected) => Err(UsageError(format!(
            "plugins takes no argument: {unexpected}"
        ))),
    }
}

fn preflight(rest: &[String]) -> Result<Command, UsageError> {
    let mut repo: Option<PathBuf> = None;
    let mut json = false;
    let mut build_output: Option<PathBuf> = None;
    let mut expecting_build_output = false;

    for argument in rest {
        if expecting_build_output {
            build_output = Some(PathBuf::from(argument));
            expecting_build_output = false;
            continue;
        }
        match argument.as_str() {
            "--json" => json = true,
            "--read-build-output" => expecting_build_output = true,
            other if other.starts_with('-') => {
                return Err(UsageError(format!("unknown flag: {other}")))
            }
            other => set_once(&mut repo, other, "repository")?,
        }
    }

    if expecting_build_output {
        return Err(UsageError(
            "--read-build-output takes a directory".to_owned(),
        ));
    }

    match repo {
        Some(repo) => Ok(Command::Preflight {
            repo,
            json,
            build_output,
        }),
        None => Err(UsageError("preflight needs a repository".to_owned())),
    }
}

fn serve(rest: &[String]) -> Result<Command, UsageError> {
    let mut out: Option<PathBuf> = None;
    let mut port: u16 = 0;
    let mut expecting_port = false;

    for argument in rest {
        if expecting_port {
            port = argument
                .parse()
                .map_err(|_| UsageError(format!("--port takes a number: {argument}")))?;
            expecting_port = false;
            continue;
        }
        match argument.as_str() {
            "--port" => expecting_port = true,
            other if other.starts_with('-') => {
                return Err(UsageError(format!("unknown flag: {other}")))
            }
            other => set_once(&mut out, other, "directory")?,
        }
    }

    if expecting_port {
        return Err(UsageError("--port takes a number".to_owned()));
    }

    match out {
        Some(out) => Ok(Command::Serve { out, port }),
        None => Err(UsageError("serve needs a directory to serve".to_owned())),
    }
}

fn merge(rest: &[String]) -> Result<Command, UsageError> {
    let mut inputs: Vec<String> = Vec::new();
    let mut out: Option<PathBuf> = None;
    let mut force = false;
    let mut expecting_out = false;

    for argument in rest {
        if expecting_out {
            out = Some(PathBuf::from(argument));
            expecting_out = false;
            continue;
        }
        match argument.as_str() {
            "-o" | "--out" => expecting_out = true,
            "--force" => force = true,
            other if other.starts_with('-') => {
                return Err(UsageError(format!("unknown flag: {other}")))
            }
            other => inputs.push(other.to_owned()),
        }
    }

    if expecting_out {
        return Err(UsageError("--out takes a directory".to_owned()));
    }
    // One artifact joins with nothing. Accepting it would write an estate
    // whose every key is unmatched, which reads as a finding about the code.
    if inputs.len() < 2 {
        return Err(UsageError(
            "merge needs at least two artifacts to join".to_owned(),
        ));
    }

    Ok(Command::Merge {
        inputs,
        out: out.unwrap_or_else(|| PathBuf::from(DEFAULT_OUT)),
        force,
    })
}

/// What the next argument is a value for.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Expecting {
    Nothing,
    Out,
    Renderer,
    InlineThreshold,
    BuildOutput,
}

fn analyse(args: &[String]) -> Result<Command, UsageError> {
    let mut repo: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut force = false;
    let mut json = false;
    let mut quiet = false;
    let mut renderer: Option<String> = None;
    let mut inline_threshold: Option<u64> = None;
    let mut no_overview = false;
    let mut build_output: Option<PathBuf> = None;
    let mut expecting = Expecting::Nothing;

    for argument in args {
        match expecting {
            Expecting::Out => {
                out = Some(PathBuf::from(argument));
                expecting = Expecting::Nothing;
                continue;
            }
            Expecting::Renderer => {
                renderer = Some(argument.clone());
                expecting = Expecting::Nothing;
                continue;
            }
            Expecting::InlineThreshold => {
                inline_threshold = Some(argument.parse().map_err(|_| {
                    UsageError(format!("--inline-threshold takes a byte count: {argument}"))
                })?);
                expecting = Expecting::Nothing;
                continue;
            }
            Expecting::BuildOutput => {
                build_output = Some(PathBuf::from(argument));
                expecting = Expecting::Nothing;
                continue;
            }
            Expecting::Nothing => {}
        }

        match argument.as_str() {
            "-o" | "--out" => expecting = Expecting::Out,
            "--renderer" => expecting = Expecting::Renderer,
            "--inline-threshold" => expecting = Expecting::InlineThreshold,
            "--no-overview" => no_overview = true,
            "--read-build-output" => expecting = Expecting::BuildOutput,
            "--force" => force = true,
            "--json" => json = true,
            "-q" | "--quiet" => quiet = true,
            "-v" | "--verbose" => quiet = false,
            other if other.starts_with('-') => {
                return Err(UsageError(format!("unknown flag: {other}")))
            }
            other => set_once(&mut repo, other, "repository")?,
        }
    }

    match expecting {
        Expecting::Out => return Err(UsageError("--out takes a directory".to_owned())),
        Expecting::Renderer => return Err(UsageError("--renderer takes a name".to_owned())),
        Expecting::InlineThreshold => {
            return Err(UsageError(
                "--inline-threshold takes a byte count".to_owned(),
            ))
        }
        Expecting::BuildOutput => {
            return Err(UsageError(
                "--read-build-output takes a directory".to_owned(),
            ))
        }
        Expecting::Nothing => {}
    }

    // Two settings of one decision. Accepting both and picking one silently
    // would make the page's presence depend on a precedence rule nobody
    // stated.
    if no_overview && inline_threshold.is_some() {
        return Err(UsageError(
            "--no-overview and --inline-threshold set the same thing two ways; pass one".to_owned(),
        ));
    }

    let Some(repo) = repo else {
        return Err(UsageError("no repository given".to_owned()));
    };

    Ok(Command::Analyse(Analyse {
        repo,
        out: out.unwrap_or_else(|| PathBuf::from(DEFAULT_OUT)),
        force,
        json,
        quiet,
        renderer,
        inline_threshold,
        no_overview,
        build_output,
    }))
}

/// Plan-06 §8 question 2: a single path, and the flag shape is not reserved.
/// Cross-repository stitching is not the analyse path's job: `reachgraph merge`
/// (ADR-0010) joins artifacts that single-repository runs already wrote.
fn set_once(slot: &mut Option<PathBuf>, value: &str, what: &str) -> Result<(), UsageError> {
    if slot.is_some() {
        return Err(UsageError(format!("one {what} at a time: {value}")));
    }
    *slot = Some(PathBuf::from(value));
    Ok(())
}
