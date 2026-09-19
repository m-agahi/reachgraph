//! A `.proto` file, reduced to the three facts a root needs — plan-04 §4.
//!
//! Roots need `package`, `service` and `method` and nothing else. Message
//! types, field numbers and the import closure are all irrelevant to root
//! identity, which is what lets this crate parse one file at a time and never
//! resolve an import — so the include-path problem, and with it the `protoc`
//! prerequisite ADR-0001 forbids, never arises.

use std::fmt;
use std::path::{Path, PathBuf};

use reachgraph_plugin_api::ContractId;

use crate::version::version_of_package;

/// One `.proto` file's contribution to the root set.
#[derive(Clone, Debug)]
pub struct ProtoContract {
    /// The repo-relative path of the file, which is what `Coverage` names.
    ///
    /// Plan-04 §5: the contract is the **file**, not the package. Two files may
    /// share a package, and a path is what a user passes, excludes or forgets.
    pub contract: ContractId,
    /// The `package` declaration, verbatim, or `None` when there is not one.
    pub package: Option<String>,
    /// ADR-0007's version, derived from [`ProtoContract::package`].
    pub version: Option<String>,
    /// Every service declared in this file, in declaration order.
    pub services: Vec<ProtoService>,
}

/// One `service` declaration.
#[derive(Clone, Debug)]
pub struct ProtoService {
    /// The bare service name, as declared.
    pub name: String,
    /// Every RPC name, as declared, in declaration order.
    ///
    /// Streaming RPCs are included and are not distinguished: a streaming
    /// endpoint is still an endpoint, so no root is lost (plan-04 §13
    /// question 7).
    pub rpcs: Vec<String>,
}

/// A discovered `.proto` file, named both ways.
#[derive(Clone, Debug)]
pub struct ContractFile {
    /// The repo-relative, `/`-separated identity.
    pub contract: ContractId,
    /// The absolute path the file is read from.
    pub path: PathBuf,
}

/// A `.proto` file that could not be read as one.
///
/// Plan-04 §10 wrote that `Coverage` had no slot for "found but unreadable",
/// and that inventing one **by omission** is the partial-index bug ADR-0007
/// exists to prevent. Both halves were right; the conclusion drawn from them —
/// fail the run — was not the only way to keep them. ADR-0743 added the slot
/// explicitly, so the file is named rather than omitted.
///
/// This type is unchanged and still an error: `parse` reports a file it could
/// not read, and [`crate::plugin`] decides what that costs. The boundary moved
/// to the caller, not into here.
#[derive(Debug)]
pub struct ProtoParseError {
    contract: ContractId,
    detail: String,
}

impl ProtoParseError {
    /// The file that did not parse.
    pub fn contract(&self) -> &ContractId {
        &self.contract
    }

    /// The parser's own message, for [`reachgraph_plugin_api::PluginError::Parse`].
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for ProtoParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} could not be read as a protobuf contract by {}: {}",
            self.contract.0,
            crate::PARSER,
            self.detail
        )
    }
}

impl std::error::Error for ProtoParseError {}

/// Read one `.proto` file's services.
///
/// MEASURED (plan-04 §4): `protox_parse::parse` looks at the syntax of the file
/// only. It neither reads imported files nor rejects a file whose imports
/// cannot be resolved — imports are recorded as dependency names and never
/// followed — so every service declaration in a repository is readable without
/// a single include path.
pub fn parse(contract: &ContractId, source: &str) -> Result<ProtoContract, ProtoParseError> {
    let descriptor = protox_parse::parse(&contract.0, source).map_err(|error| ProtoParseError {
        contract: contract.clone(),
        detail: error.to_string(),
    })?;

    // `FileDescriptorProto` spells an absent package as an empty `Option`, and
    // an empty string is an absent package too — both are "the file declares
    // none", which is `None` rather than `Some("")`.
    let package = descriptor
        .package
        .filter(|package| !package.is_empty())
        .map(|package| package.to_string());

    let services = descriptor
        .service
        .into_iter()
        .map(|service| ProtoService {
            name: service.name().to_owned(),
            rpcs: service
                .method
                .iter()
                .map(|method| method.name().to_owned())
                .collect(),
        })
        .collect();

    Ok(ProtoContract {
        contract: contract.clone(),
        version: version_of_package(package.as_deref()),
        package,
        services,
    })
}

/// Every `.proto` file under `repo_root`, sorted by contract id.
///
/// # The two exclusions
///
/// `target/` is build output rather than a contract: a `.proto` copied there by
/// a build script is the same contract twice, and counting it twice would put a
/// second `ContractId` in `Coverage` for one file a user can point at. A
/// directory whose name starts with `.` is VCS or tooling state for the same
/// reason.
pub fn discover(repo_root: &Path) -> std::io::Result<Vec<ContractFile>> {
    let mut found = Vec::new();
    walk(repo_root, repo_root, &mut found)?;
    found.sort_by(|left, right| left.contract.0.cmp(&right.contract.0));
    Ok(found)
}

fn walk(repo_root: &Path, directory: &Path, out: &mut Vec<ContractFile>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();

        if entry.file_type()?.is_dir() {
            if name == "target" || name.starts_with('.') {
                continue;
            }
            walk(repo_root, &path, out)?;
            continue;
        }

        if path.extension().is_some_and(|ext| ext == "proto") {
            out.push(ContractFile {
                contract: ContractId(relative_id(repo_root, &path)),
                path,
            });
        }
    }
    Ok(())
}

/// The repo-relative, `/`-separated spelling of a discovered path.
///
/// A path outside `repo_root` cannot happen here — the walk starts at the root
/// — and if it ever did, the full path is a truthful id rather than a panic.
fn relative_id(repo_root: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(repo_root).unwrap_or(path);
    relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}
