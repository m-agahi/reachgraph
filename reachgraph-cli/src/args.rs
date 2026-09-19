//! The argument surface — plan-06 §1.
//!
//! Hand-rolled, for the reason `xtask` is: four subcommands and seven flags do
//! not pay for an argument-parsing crate, and ADR-0001 makes every dependency
//! in the shipped binary a decision rather than a detail.
//!
//! # What is absent, and why it is absent rather than unimplemented
//!
//! `--renderer`, `--inline-threshold` and `--no-overview` are plan-06 §1.1
//! flags that select and configure a renderer. **No renderer crate exists**:
//! plan-05 has not landed, `Renderer` is not in the contract, and the artifact
//! today is the waist's own JSON. A flag that parsed and then had nothing to
//! select would be worse than its absence — it would read as a supported
//! choice.
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
    },
    /// List what is registered.
    Plugins,
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
}

/// A usage error, with the message the user sees.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsageError(pub String);

/// Plan-06 §1's command surface, verbatim.
pub const USAGE: &str = "\
usage:
  reachgraph <repo> [-o|--out <dir>] [--force] [--json] [-q|--quiet]
  reachgraph serve <out> [--port <n>]
  reachgraph preflight <repo> [--json]
  reachgraph plugins
  reachgraph --version | --help

  <repo>          a repository some registered plugin claims by its marker file
  -o, --out       where the artifact goes (default: ./out)
      --force     write into a directory reachgraph did not produce
      --json      machine-readable report on stdout; progress stays on stderr
  -q, --quiet     errors only
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

    for argument in rest {
        match argument.as_str() {
            "--json" => json = true,
            other if other.starts_with('-') => {
                return Err(UsageError(format!("unknown flag: {other}")))
            }
            other => set_once(&mut repo, other, "repository")?,
        }
    }

    match repo {
        Some(repo) => Ok(Command::Preflight { repo, json }),
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

fn analyse(args: &[String]) -> Result<Command, UsageError> {
    let mut repo: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut force = false;
    let mut json = false;
    let mut quiet = false;
    let mut expecting_out = false;

    for argument in args {
        if expecting_out {
            out = Some(PathBuf::from(argument));
            expecting_out = false;
            continue;
        }
        match argument.as_str() {
            "-o" | "--out" => expecting_out = true,
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

    if expecting_out {
        return Err(UsageError("--out takes a directory".to_owned()));
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
    }))
}

/// Plan-06 §8 question 2: a single path, and the flag shape is not reserved.
/// Cross-repository stitching is out of v0.1 (ADR-0008), and accepting several
/// paths now would invite a half-implementation of it.
fn set_once(slot: &mut Option<PathBuf>, value: &str, what: &str) -> Result<(), UsageError> {
    if slot.is_some() {
        return Err(UsageError(format!("one {what} at a time: {value}")));
    }
    *slot = Some(PathBuf::from(value));
    Ok(())
}
