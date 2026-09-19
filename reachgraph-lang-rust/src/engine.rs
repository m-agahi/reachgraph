//! The engine-facing half — everything that touches `ra_ap_*`.
//!
//! Nothing below returns an `ra_ap` type to a caller outside this crate.
//! `FileId`, `FilePosition`, `TextRange`, `Cancellable` and the salsa database
//! all stop here (ADR-0008 leaks 1, 2 and 5); what leaves is
//! `reachgraph-plugin-api` types and [`crate::coverage::RustCoverage`].

use std::path::{Path, PathBuf};
use std::process::Command;

use ra_ap_hir::{Crate, HasAttrs as _, Impl, Module, ModuleDef};
use ra_ap_ide::{Analysis, AnalysisHost, CallHierarchyConfig, FilePosition, RootDatabase};
use ra_ap_ide_db::ra_fixture::RaFixtureConfig;
use ra_ap_load_cargo::{load_workspace, LoadCargoConfig, ProcMacroServerChoice};
use ra_ap_paths::{AbsPathBuf, Utf8PathBuf};
use ra_ap_project_model::{
    CargoConfig, ProjectManifest, ProjectWorkspace, ProjectWorkspaceKind, TargetKind,
};
use ra_ap_syntax::{TextRange, TextSize};
use ra_ap_vfs::{Vfs, VfsPath};
use reachgraph_plugin_api::{
    Category, Edge, EdgeTarget, InferenceMode, NodeId, PluginError, Provenance, SourceRange, Span,
    Symbol, SymbolKind, Unit, UnitId,
};

use crate::classify::{classify_facts, CrateOrigin, PathFacts};
use crate::coverage::{MemberCoverage, OutDirMechanism, ProcMacroExpansion, RustCoverage};
use crate::ids::{node_id, RawParts};
use crate::kinds::{map_kind, render_impl_header, RustItem};
use crate::preflight::{CargoProbe, PreflightFacts, WorkspaceProbe};
use crate::{ENGINE, PLUGIN_ID};

/// `ra_ap` reports cancellation and the call hierarchy reports nothing; both
/// become this crate's own error at the boundary.
fn engine_error(detail: impl Into<String>) -> PluginError {
    PluginError::Engine {
        plugin: PLUGIN_ID,
        engine: ENGINE.to_owned(),
        detail: detail.into(),
    }
}

/// The call-hierarchy configuration, stated once.
///
/// `exclude_tests: false` because `is_test` is a `Symbol` field this crate
/// fills in (plan-03 §8) and plan-04 §6 uses it to decide **direction**.
/// Dropping test callers here would take that decision away from the consumer
/// and hide it in a config literal.
///
/// `disable_ra_fixture: true` because `ra_fixture` is rust-analyzer's own
/// test-harness syntax; enabling it over real source invites the engine to read
/// a doc comment as a fixture.
fn call_hierarchy_config() -> CallHierarchyConfig<'static> {
    CallHierarchyConfig {
        exclude_tests: false,
        ra_fixture: RaFixtureConfig {
            minicore: Default::default(),
            disable_ra_fixture: true,
        },
    }
}

/// What this crate knows about one workspace member target.
#[derive(Clone, Debug)]
struct UnitFacts {
    unit: Unit,
    /// The package half of the unit id, which is what `classify` compares.
    package: String,
    /// The target's crate root file, which is how a `Unit` is matched to a
    /// `hir::Crate` after the load (a `Crate` knows its root file; it does not
    /// know a cargo target).
    root_file: PathBuf,
}

/// A loaded workspace, and the whole of this crate's mutable state.
pub(crate) struct Loaded {
    /// The **workspace** root, which is not the directory the caller named.
    ///
    /// Every path in a `NodeId` and a `SourceRange` is rendered relative to
    /// this, so it has to be a property of the workspace rather than of the
    /// entry point. MEASURED as a defect first: with the caller's directory
    /// here, `discover_units(<workspace>)` and `symbols_in(<member unit>)`
    /// rendered the same file two different ways, and a call target's `raw`
    /// stopped matching the `raw` the walk had emitted for the same
    /// definition. Plan-03 §6's whole design rests on those two being
    /// byte-identical.
    root: PathBuf,
    host: AnalysisHost,
    vfs: Vfs,
    sysroot_src: Option<PathBuf>,
    units: Vec<UnitFacts>,
    coverage: RustCoverage,
}

impl Loaded {
    /// Whether a directory the caller named is inside this workspace.
    ///
    /// Cargo manifest discovery walks **up**, so any directory under the
    /// workspace root resolves the same workspace. Reloading for a member
    /// directory would be wasted work, and — before the workspace root became
    /// the anchor above — it silently re-anchored every path this crate emits.
    pub(crate) fn covers(&self, directory: &Path) -> bool {
        directory.starts_with(&self.root)
    }

    /// What the run could not see.
    pub(crate) fn coverage(&self) -> &RustCoverage {
        &self.coverage
    }

    /// Plan-03 §7's units — one per workspace member target.
    pub(crate) fn units(&self) -> Vec<Unit> {
        self.units.iter().map(|facts| facts.unit.clone()).collect()
    }
}

/// Where a `PathBuf` is rendered for a `NodeId` and a `SourceRange`.
///
/// Repository-relative inside the repository, absolute outside it. Plan-03 §6
/// says "repo-relative" while describing first-party symbols; §9 adds the
/// out-of-repository classes — a dependency's extracted library source, the
/// sysroot — which have no repository-relative spelling and must still get an
/// identity.
fn display_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Run `cargo --version` and read the output.
///
/// ADR-0003 field 5 and plan-03 §11 check 1a: **never `command -v`**. MEASURED,
/// design.md §8 and §10 — on the author's machine `command -v rust-analyzer`
/// succeeds against a `rustup` proxy that loops and is not installed. A name
/// resolving is not a capability, so the check runs the program and reads what
/// it printed.
pub(crate) fn probe_cargo() -> CargoProbe {
    probe_program("cargo")
}

/// The probe itself, with the program named.
///
/// Parameterised so the mechanism can be tested rather than only its messages.
/// A test can point it at a program that **resolves and proves nothing** —
/// which is the rustup proxy loop, reproduced rather than described — without
/// mutating `PATH` for every other test in the process.
pub fn probe_program(program: &str) -> CargoProbe {
    match Command::new(program).arg("--version").output() {
        Ok(output) if output.status.success() => {
            let line = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            if line.starts_with("cargo ") {
                CargoProbe::Responded { version: line }
            } else {
                CargoProbe::DidNotRespond {
                    detail: format!("`{program} --version` printed {line:?}, not a version line"),
                }
            }
        }
        Ok(output) => CargoProbe::DidNotRespond {
            detail: format!(
                "`{program} --version` exited with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        },
        Err(error) => CargoProbe::DidNotRespond {
            detail: error.to_string(),
        },
    }
}

/// Load the workspace at or above `root`.
///
/// # What is deliberately not configured
///
/// `load_out_dirs_from_check: false` — ADR-0001 and plan-03 §4 D-B:
/// **reachgraph runs no build, ever.** The value that would build is the one
/// value this line must never take.
///
/// `with_proc_macro_server: ProcMacroServerChoice::None` — the other two
/// variants spawn a server binary, and a proc-macro server is rust-analyzer's
/// own component rather than the target language's build toolchain, so
/// ADR-0001's carve-out does not reach it. See
/// [`crate::coverage::ProcMacroExpansion`] for why the in-process route
/// measured closed.
///
/// `CargoConfig::extra_includes` is left empty. MEASURED that setting it makes
/// a generated file VFS-resident and still does not make a call into it
/// resolve; reporting a mechanism that changes nothing observable would claim
/// coverage this crate does not have.
pub(crate) fn load(root: &Path) -> Result<Loaded, PluginError> {
    let abs_root = AbsPathBuf::assert(
        Utf8PathBuf::from_path_buf(root.to_path_buf())
            .map_err(|path| engine_error(format!("{} is not valid UTF-8", path.display())))?,
    );

    let cargo_config = CargoConfig::default();

    let manifest = ProjectManifest::discover_single(&abs_root)
        .map_err(|error| engine_error(format!("no Cargo workspace at {root:?}: {error}")))?;

    let workspace = ProjectWorkspace::load(manifest, &cargo_config, &|_| {})
        .map_err(|error| engine_error(format!("could not load {root:?}: {error}")))?;

    let sysroot_src = workspace
        .sysroot
        .rust_lib_src_root()
        .map(|it| PathBuf::from(it.as_str()));

    let (workspace_root, units, member_coverage) = enumerate_units(root, &workspace)?;

    let load_config = LoadCargoConfig {
        load_out_dirs_from_check: false,
        with_proc_macro_server: ProcMacroServerChoice::None,
        prefill_caches: false,
        num_worker_threads: 1,
        proc_macro_processes: 0,
    };

    let (db, vfs, _proc_macro_client) =
        load_workspace(workspace, &Default::default(), &load_config)
            .map_err(|error| engine_error(format!("could not load {root:?}: {error}")))?;

    Ok(Loaded {
        root: workspace_root,
        host: AnalysisHost::with_database(db),
        vfs,
        sysroot_src: sysroot_src.clone(),
        units,
        coverage: RustCoverage {
            members: member_coverage,
            rust_src_available: sysroot_src.is_some(),
            proc_macro_expansion: ProcMacroExpansion::Disabled,
            out_dir_mechanism: OutDirMechanism::Unloaded,
        },
    })
}

/// Plan-03 §7 — one `Unit` per workspace member target, and one coverage row
/// per workspace member package.
#[allow(clippy::type_complexity)]
fn enumerate_units(
    root: &Path,
    workspace: &ProjectWorkspace,
) -> Result<(PathBuf, Vec<UnitFacts>, Vec<MemberCoverage>), PluginError> {
    let ProjectWorkspaceKind::Cargo { cargo, .. } = &workspace.kind else {
        return Err(engine_error(
            "this workspace is not a Cargo workspace; reachgraph indexes Rust through cargo",
        ));
    };

    let mut units = Vec::new();
    let mut members = Vec::new();

    for package in cargo.packages() {
        let data = &cargo[package];
        if !data.is_member {
            continue;
        }

        let manifest_dir = data.manifest.parent();
        let package_id = format!("{}@{} {}", data.name, data.version, manifest_dir.as_str());

        let kinds: Vec<TargetKind> = data.targets.iter().map(|&it| cargo[it].kind).collect();
        let several = kinds
            .iter()
            .filter(|kind| !matches!(kind, TargetKind::BuildScript))
            .count()
            > 1;

        let has_build_script = kinds
            .iter()
            .any(|kind| matches!(kind, TargetKind::BuildScript));
        let out_dir_on_disk = out_dir_on_disk(root, &data.name);

        members.push(MemberCoverage {
            package: package_id.clone(),
            has_build_script,
            // MEASURED: nothing loads generated code into the crate graph, so
            // this is false whenever the member has a build script. It is not
            // hard-coded to false — a member with no build script has nothing
            // to load, and reporting "not loaded" there would read as a gap.
            out_dir_loaded: false,
            out_dir_on_disk,
        });

        for &target in &data.targets {
            let target_data = &cargo[target];
            // A build script is compiled and run by cargo, never called from
            // the crate it belongs to. Walking it would emit symbols for code
            // that is not part of the program being indexed.
            if matches!(target_data.kind, TargetKind::BuildScript) {
                continue;
            }

            let qualifier = target_qualifier(target_data.kind);
            let display_name = match (several, qualifier) {
                (true, Some(qualifier)) => format!("{} ({qualifier})", data.name),
                _ => data.name.clone(),
            };

            units.push(UnitFacts {
                unit: Unit {
                    id: UnitId(format!(
                        "{package_id}::{}::{}",
                        target_data.name,
                        qualifier.unwrap_or("lib")
                    )),
                    display_name,
                    root: PathBuf::from(manifest_dir.as_str()),
                },
                package: package_id.clone(),
                root_file: PathBuf::from(target_data.root.as_str()),
            });
        }
    }

    units.sort_by(|a, b| a.unit.id.0.cmp(&b.unit.id.0));
    members.sort_by(|a, b| a.package.cmp(&b.package));
    Ok((
        PathBuf::from(cargo.workspace_root().as_str()),
        units,
        members,
    ))
}

/// The display qualifier plan-03 §7 asks for, so two units never display
/// identically.
fn target_qualifier(kind: TargetKind) -> Option<&'static str> {
    match kind {
        TargetKind::Lib {
            is_proc_macro: false,
        } => None,
        TargetKind::Lib {
            is_proc_macro: true,
        } => Some("proc-macro"),
        TargetKind::Bin => Some("bin"),
        TargetKind::Example => Some("example"),
        TargetKind::Test => Some("test"),
        TargetKind::Bench => Some("bench"),
        TargetKind::BuildScript => Some("build"),
        TargetKind::Other => Some("other"),
    }
}

/// Whether `target/<profile>/build/<pkg>-<hash>/out/` holds at least one `.rs`
/// file for this package.
///
/// Reported **beside** `out_dir_loaded` rather than instead of it, because the
/// two differ in exactly the case plan-03 §9 says must not be softened: the
/// artifacts are on disk and reachgraph never told the crate graph to look.
fn out_dir_on_disk(root: &Path, package_name: &str) -> bool {
    let prefix = format!("{}-", package_name.replace('-', "_"));
    let alt = format!("{package_name}-");
    let Ok(profiles) = std::fs::read_dir(root.join("target")) else {
        return false;
    };
    for profile in profiles.flatten() {
        let Ok(builds) = std::fs::read_dir(profile.path().join("build")) else {
            continue;
        };
        for build in builds.flatten() {
            let name = build.file_name().to_string_lossy().into_owned();
            if !name.starts_with(&prefix) && !name.starts_with(&alt) {
                continue;
            }
            let Ok(entries) = std::fs::read_dir(build.path().join("out")) else {
                continue;
            };
            if entries
                .flatten()
                .any(|entry| entry.path().extension().is_some_and(|ext| ext == "rs"))
            {
                return true;
            }
        }
    }
    false
}

impl Loaded {
    /// Plan-03 §11's facts, gathered from a load that already happened.
    pub(crate) fn preflight_facts(&self, cargo: CargoProbe) -> PreflightFacts {
        PreflightFacts {
            cargo,
            workspace: WorkspaceProbe::Loaded,
            members_with_unindexed_generated_code: self
                .coverage
                .members_with_unindexed_generated_code()
                .into_iter()
                .map(|member| member.package.clone())
                .collect(),
            rust_src_available: self.coverage.rust_src_available,
            proc_macro_expansion: self.coverage.proc_macro_expansion,
        }
    }

    fn facts_for(&self, unit: &UnitId) -> Result<&UnitFacts, PluginError> {
        self.units
            .iter()
            .find(|facts| &facts.unit.id == unit)
            .ok_or_else(|| PluginError::UnknownUnit {
                plugin: PLUGIN_ID,
                unit: unit.clone(),
            })
    }

    /// The `hir::Crate` whose root file is this unit's target root.
    ///
    /// A `Crate` knows its root file and does not know a cargo target, so this
    /// is the join. A unit whose root file is in no crate is a real failure
    /// rather than an empty result — it means the crate graph and the cargo
    /// metadata disagree.
    fn crate_for(&self, facts: &UnitFacts) -> Result<Crate, PluginError> {
        let db = self.host.raw_database();
        let wanted = self.file_id(&facts.root_file).ok_or_else(|| {
            engine_error(format!(
                "{} is not in the VFS, so unit {} has no crate",
                facts.root_file.display(),
                facts.unit.id.0
            ))
        })?;

        Crate::all(db)
            .into_iter()
            .find(|krate| krate.root_file(db) == wanted)
            .ok_or_else(|| {
                engine_error(format!(
                    "no crate has root file {}",
                    facts.root_file.display()
                ))
            })
    }

    fn file_id(&self, path: &Path) -> Option<ra_ap_vfs::FileId> {
        let utf8 = Utf8PathBuf::from_path_buf(path.to_path_buf()).ok()?;
        let vfs_path = VfsPath::from(AbsPathBuf::assert(utf8));
        self.vfs
            .file_id(&vfs_path)
            .filter(|(_, excluded)| *excluded == ra_ap_vfs::FileExcluded::No)
            .map(|(id, _)| id)
    }

    /// The path behind a `FileId`, or `None` when the target is external in
    /// plan-01 §7.0's sense.
    ///
    /// Two fallible steps and both are real. MEASURED: `Vfs::file_path`
    /// **panics** on an unknown `FileId`, so `exists` is not a nicety; and
    /// `VfsPath::as_path` returns `None` for an in-memory path, which has no
    /// location to report.
    fn path_of(&self, file_id: ra_ap_vfs::FileId) -> Option<PathBuf> {
        if !self.vfs.exists(file_id) {
            return None;
        }
        self.vfs
            .file_path(file_id)
            .as_path()
            .map(|path| PathBuf::from(path.as_str()))
    }

    fn analysis(&self) -> Analysis {
        self.host.analysis()
    }
}

/// One located definition, reduced to the facts a [`Symbol`] needs.
struct Located {
    name: String,
    item: RustItem,
    path: PathBuf,
    /// The item's full extent.
    full_range: TextRange,
    /// The name token's start — what `FilePosition` needs for
    /// `outgoing_calls` to identify the item under it (plan-03 §6).
    def_offset: TextSize,
    doc: Option<String>,
    is_test: bool,
}

impl Loaded {
    /// Plan-03 §8 — the semantic walk.
    ///
    /// Source of truth is `ra_ap_hir`, not `Analysis::file_structure`.
    /// `file_structure` carries a ready-made `parent` relation and **no
    /// documentation field**, and documentation is not optional (ADR-0005;
    /// design.md §9 Q1 calls it the premise question).
    pub(crate) fn symbols_in(&self, unit: &UnitId) -> Result<Vec<Symbol>, PluginError> {
        let facts = self.facts_for(unit)?;
        let krate = self.crate_for(facts)?;
        let db = self.host.raw_database();
        let sema = ra_ap_hir::Semantics::new(db);

        let mut symbols = Vec::new();
        for module in krate.modules(db) {
            self.walk_module(&sema, module, facts, &mut symbols)?;
        }
        Ok(symbols)
    }

    fn walk_module(
        &self,
        sema: &ra_ap_hir::Semantics<'_, RootDatabase>,
        module: Module,
        facts: &UnitFacts,
        out: &mut Vec<Symbol>,
    ) -> Result<(), PluginError> {
        let db = sema.db;

        let module_id = self
            .located(
                sema,
                ra_ap_ide_db::defs::Definition::Module(module),
                RustItem::Module,
            )
            .map(|located| self.emit(located, facts, None, out))
            .transpose()?;

        for def in module.declarations(db) {
            // A module declaration is walked as a module in its own right by
            // `Crate::modules`, so emitting it here too would duplicate it.
            if matches!(def, ModuleDef::Module(_)) {
                continue;
            }
            let item = item_of(def);
            let definition = ra_ap_ide_db::defs::Definition::from(def);
            if let Some(located) = self.located(sema, definition, item) {
                self.emit(located, facts, module_id.clone(), out)?;
            }
        }

        // A trait's own associated functions. MEASURED as a defect first:
        // a call on a generic or through a trait object resolves to the
        // TRAIT's declaration rather than to any impl's, so a walk that
        // emitted only impl items produced edge targets with no `Symbol` —
        // breaking plan-01 §7.0's provider obligation for a target that is
        // both located AND inside an enumerated unit.
        for def in module.declarations(db) {
            let ModuleDef::Trait(tr) = def else {
                continue;
            };
            let Some(located) = self.located(
                sema,
                ra_ap_ide_db::defs::Definition::Trait(tr),
                RustItem::Trait,
            ) else {
                continue;
            };
            let trait_id = self.node_id_for(&facts.unit.id, &located.path, located.def_offset)?;
            for item in tr.items(db) {
                let ra_ap_hir::AssocItem::Function(function) = item else {
                    continue;
                };
                let definition = ra_ap_ide_db::defs::Definition::Function(function);
                if let Some(located) = self.located(sema, definition, RustItem::Method) {
                    self.emit(located, facts, Some(trait_id.clone()), out)?;
                }
            }
        }

        for imp in module.impl_defs(db) {
            let header = self.impl_header(db, imp);
            let Some(mut located) = self.located(
                sema,
                ra_ap_ide_db::defs::Definition::SelfType(imp),
                RustItem::Impl {
                    header: header.clone(),
                },
            ) else {
                continue;
            };
            // Plan-03 §8: the impl symbol's `name` is the self type's name,
            // e.g. `Task`. MEASURED that `NavigationTarget` answers `"impl"`
            // for a `SelfType` definition, which is the keyword rather than
            // the subject and would render as a wall of identical nodes.
            located.name = self_type_name(db, imp);
            let impl_id = self.emit(located, facts, module_id.clone(), out)?;

            for item in imp.items(db) {
                let ra_ap_hir::AssocItem::Function(function) = item else {
                    continue;
                };
                let definition = ra_ap_ide_db::defs::Definition::Function(function);
                if let Some(located) = self.located(sema, definition, RustItem::Method) {
                    self.emit(located, facts, Some(impl_id.clone()), out)?;
                }
            }
        }

        Ok(())
    }

    /// Plan-03 §8's impl header, and the trait half is the **declared** name.
    ///
    /// MEASURED, `/home/max/git/yadgarhq/task/src/service/handlers.rs:17,21`:
    /// the trait arrives through
    /// `use crate::pb::…::task_service_server::TaskService;` and the impl reads
    /// `impl TaskService for Task`. The declared name equals the proto service
    /// name, so plan-04 compares against that directly and never resolves a
    /// Rust import. `Trait::name` is the declared name; the use-path is not
    /// available here and is not wanted.
    fn impl_header(&self, db: &RootDatabase, imp: Impl) -> String {
        let trait_name = imp.trait_(db).map(|tr| tr.name(db).as_str().to_owned());
        render_impl_header(trait_name.as_deref(), &self_type_name(db, imp))
    }

    fn located(
        &self,
        sema: &ra_ap_hir::Semantics<'_, RootDatabase>,
        definition: ra_ap_ide_db::defs::Definition<'_>,
        item: RustItem,
    ) -> Option<Located> {
        let nav = ra_ap_ide::TryToNav::try_to_nav(&definition, sema)?.call_site;
        let path = self.path_of(nav.file_id)?;
        Some(Located {
            name: nav.name.as_str().to_owned(),
            item,
            path,
            full_range: nav.full_range,
            def_offset: nav
                .focus_range
                .map_or_else(|| nav.full_range.start(), |range| range.start()),
            doc: docs_of(sema.db, definition),
            is_test: is_test(sema.db, definition),
        })
    }

    fn emit(
        &self,
        located: Located,
        facts: &UnitFacts,
        container: Option<NodeId>,
        out: &mut Vec<Symbol>,
    ) -> Result<NodeId, PluginError> {
        let id = self.node_id_for(&facts.unit.id, &located.path, located.def_offset)?;
        let (kind, raw_kind) = map_kind(&located.item);
        out.push(Symbol {
            id: id.clone(),
            name: located.name,
            kind,
            raw_kind,
            range: SourceRange {
                file: PathBuf::from(display_path(&self.root, &located.path)),
                // Rust supplies both halves, always. MEASURED (plan-03 §3):
                // `NavigationTarget::full_range` is a plain `TextRange`, not
                // an option, so `span: None` — located in a file, offset
                // unknown — is a case this plugin never produces.
                span: Some(Span {
                    start: u32::from(located.full_range.start()),
                    end: u32::from(located.full_range.end()),
                }),
            },
            doc: located.doc,
            doc_format: reachgraph_plugin_api::DocFormat::Markdown,
            container,
            is_test: located.is_test,
        });
        Ok(id)
    }

    fn node_id_for(
        &self,
        unit: &UnitId,
        path: &Path,
        offset: TextSize,
    ) -> Result<NodeId, PluginError> {
        node_id(
            PLUGIN_ID,
            &RawParts {
                unit: unit.clone(),
                offset: u32::from(offset),
                path: display_path(&self.root, path),
            },
        )
        .map_err(|error| engine_error(error.to_string()))
    }
}

/// The self type's name, as the impl header and the impl symbol both spell it.
///
/// A non-ADT self type — `impl Trait for &str`, `impl Trait for (A, B)` — has
/// no name to take, and `"_"` says so rather than inventing one. Plan-03 §14
/// question 6 records that the string contract has no mitigation for a
/// collision; this is the same honesty one step down.
fn self_type_name(db: &RootDatabase, imp: Impl) -> String {
    imp.self_ty(db)
        .as_adt()
        .map(|adt| adt.name(db).as_str().to_owned())
        .unwrap_or_else(|| "_".to_owned())
}

/// The mapping from `hir` to plan-03 §8's table.
///
/// `RustItem::TraitAlias` has no row here on purpose: MEASURED,
/// `ra_ap_hir::ModuleDef` 0.0.352 has no `TraitAlias` variant, because trait
/// aliases are an unstable Rust feature the engine does not surface as a
/// module definition. The variant stays in the table because plan-03 §8 names
/// it and the mapping is tested without an engine; what would be dishonest is
/// a match arm claiming to produce it.
fn item_of(def: ModuleDef) -> RustItem {
    match def {
        ModuleDef::Function(_) => RustItem::Function,
        ModuleDef::Adt(ra_ap_hir::Adt::Struct(_)) => RustItem::Struct,
        ModuleDef::Adt(ra_ap_hir::Adt::Enum(_)) => RustItem::Enum,
        ModuleDef::Adt(ra_ap_hir::Adt::Union(_)) => RustItem::Union,
        ModuleDef::Trait(_) => RustItem::Trait,
        ModuleDef::TypeAlias(_) => RustItem::TypeAlias,
        ModuleDef::Static(_) => RustItem::Static,
        ModuleDef::Const(_) => RustItem::Const,
        ModuleDef::Macro(_) => RustItem::Macro,
        ModuleDef::Module(_) => RustItem::Module,
        // A kind the table has not been taught keeps the engine's own term
        // rather than being flattened to a label (plan-03 §8).
        ModuleDef::EnumVariant(_) => RustItem::Other("EnumVariant".to_owned()),
        ModuleDef::BuiltinType(it) => RustItem::Other(it.name().as_str().to_owned()),
    }
}

/// ADR-0005 and plan-03 §8 — doc text from the engine that already parsed the
/// file, never from `Analysis::hover`'s rendered markup and never from a
/// second parser.
fn docs_of(db: &RootDatabase, definition: ra_ap_ide_db::defs::Definition<'_>) -> Option<String> {
    let text = match definition {
        ra_ap_ide_db::defs::Definition::Function(it) => it.hir_docs(db),
        ra_ap_ide_db::defs::Definition::Adt(it) => it.hir_docs(db),
        ra_ap_ide_db::defs::Definition::Trait(it) => it.hir_docs(db),
        ra_ap_ide_db::defs::Definition::TypeAlias(it) => it.hir_docs(db),
        ra_ap_ide_db::defs::Definition::Static(it) => it.hir_docs(db),
        ra_ap_ide_db::defs::Definition::Const(it) => it.hir_docs(db),
        ra_ap_ide_db::defs::Definition::Macro(it) => it.hir_docs(db),
        ra_ap_ide_db::defs::Definition::Module(it) => it.hir_docs(db),
        ra_ap_ide_db::defs::Definition::SelfType(it) => it.hir_docs(db),
        ra_ap_ide_db::defs::Definition::EnumVariant(it) => it.hir_docs(db),
        _ => None,
    }?;
    let owned = text.docs().to_owned();
    // An empty doc block is an absent doc. `Some("")` would be a value that
    // claims more than the engine found.
    (!owned.trim().is_empty()).then_some(owned)
}

/// Plan-03 §8's `is_test`.
///
/// It is not decoration. Plan-04 §6 uses it to decide **direction**, and
/// getting it wrong manufactures a phantom root — MEASURED, design.md §4:
/// `tests/service.rs:145`'s `impl TaskDbService for MockDb` and
/// `src/service/handlers.rs:22`'s real handler both define `create_task`.
fn is_test(db: &RootDatabase, definition: ra_ap_ide_db::defs::Definition<'_>) -> bool {
    match definition {
        // `Function::is_test` is the engine's own answer and covers `#[test]`
        // as the engine resolves it. The other two limbs plan-03 §8 names —
        // a `tests/` or `bench` target, and an enclosing `#[cfg(test)]` — are
        // supplied by the caller, because neither is a property of the
        // function alone.
        ra_ap_ide_db::defs::Definition::Function(it) => it.is_test(db),
        _ => false,
    }
}

impl Loaded {
    /// Plan-03 §9 — every call site in a unit, as an [`Edge`].
    ///
    /// **One `Edge` per call site**, not one per callee. `CallItem::ranges` is
    /// MEASURED to be a `Vec<FileRange>`: three calls to the same function from
    /// one body are three real call sites, and collapsing them here would
    /// discard information the waist cannot recover. Deduplication is a
    /// renderer decision.
    pub(crate) fn edges_in(&self, unit: &UnitId) -> Result<Vec<Edge>, PluginError> {
        let symbols = self.symbols_in(unit)?;
        let mut edges = Vec::new();
        for symbol in &symbols {
            if !matches!(symbol.kind, SymbolKind::Function | SymbolKind::Method) {
                continue;
            }
            edges.extend(self.edges_from(&symbol.id)?);
        }
        Ok(edges)
    }

    /// Plan-03 §9 — the same emit path, entered from a node instead of a unit.
    ///
    /// # `edges_from` has no caller in this workspace, and is written anyway
    ///
    /// MEASURED during PR C: the core assembles batch-wise through `edges_in`
    /// and nothing calls `edges_from`. Plan-00 §8 question 1 defers the
    /// decision to here.
    ///
    /// **The decision taken: `edges_in` is implemented in terms of
    /// `edges_from`, not beside it.** Plan-03 §9 says the two "must not
    /// diverge" and the only way to guarantee that is to have one
    /// implementation. Writing `edges_from` as a thin unused sibling of
    /// `edges_in` would have left an untested method that drifts silently,
    /// which is the outcome an uncalled method usually has; writing it as the
    /// one that does the work means every `edges_in` test is also an
    /// `edges_from` test.
    ///
    /// The cost is real and is accepted: `edges_from` decodes its node, so
    /// `edges_in` decodes every node it just encoded. That is a string
    /// round-trip per symbol, not an engine query, and it buys the guarantee
    /// that the two entry points cannot disagree.
    pub(crate) fn edges_from(&self, node: &NodeId) -> Result<Vec<Edge>, PluginError> {
        if node.plugin != PLUGIN_ID {
            return Err(PluginError::UnknownNode {
                plugin: PLUGIN_ID,
                node: node.clone(),
            });
        }
        let parts = RawParts::decode(&node.raw).map_err(|_| PluginError::UnknownNode {
            plugin: PLUGIN_ID,
            node: node.clone(),
        })?;

        let absolute = self.absolute(&parts.path);
        let file_id = self
            .file_id(&absolute)
            .ok_or_else(|| PluginError::UnknownNode {
                plugin: PLUGIN_ID,
                node: node.clone(),
            })?;

        let analysis = self.analysis();
        let position = FilePosition {
            file_id,
            offset: TextSize::new(parts.offset),
        };
        let config = call_hierarchy_config();
        let items = analysis
            .outgoing_calls(&config, position)
            .map_err(|_| engine_error("the engine cancelled outgoing_calls"))?;

        // `Ok(None)` means "no call hierarchy at this position" and is not an
        // error (plan-03 §9). It emits no edges and is counted, never guessed
        // around.
        let Some(items) = items else {
            return Ok(Vec::new());
        };

        let mut edges = Vec::new();
        for item in items {
            let Some(target_path) = self.path_of(item.target.file_id) else {
                // The target failed the VFS lookup, so it is located nowhere
                // and is genuinely **external** in plan-01 §7.0's sense. No
                // `Symbol` is emitted for it and no edge can name it.
                continue;
            };
            let target_unit = self.unit_of(&target_path);
            let offset = item
                .target
                .focus_range
                .map_or_else(|| item.target.full_range.start(), |range| range.start());
            let target = self.node_id_for(&target_unit, &target_path, offset)?;

            for range in item.ranges {
                let Some(call_path) = self.path_of(range.file_id) else {
                    continue;
                };
                edges.push(Edge {
                    from: node.clone(),
                    to: EdgeTarget::Resolved(target.clone()),
                    call_site: Some(SourceRange {
                        file: PathBuf::from(display_path(&self.root, &call_path)),
                        span: Some(Span {
                            start: u32::from(range.range.start()),
                            end: u32::from(range.range.end()),
                        }),
                    }),
                    provenance: Provenance {
                        plugin: PLUGIN_ID,
                        engine: ENGINE.to_owned(),
                    },
                    // Always `Resolved`: `ra_ap` answered directly. `Lexical`,
                    // `TypeInferred` and `Enclosure` exist for the languages
                    // ADR-0004 says must be written. The honest consequence is
                    // that `EdgeTarget::Unresolved` is never produced either —
                    // when `ra_ap` cannot resolve a call it returns no
                    // `CallItem` at all, so there is no candidate set to
                    // report (plan-03 §9, §12, §14 question 8).
                    inference_mode: InferenceMode::Resolved,
                });
            }
        }
        Ok(edges)
    }

    /// Turn a `raw`'s path back into something the VFS can look up.
    fn absolute(&self, path: &str) -> PathBuf {
        let candidate = PathBuf::from(path);
        if candidate.is_absolute() {
            candidate
        } else {
            self.root.join(candidate)
        }
    }

    /// Which unit a path belongs to, for a node id minted mid-traversal.
    ///
    /// A call target outside every enumerated unit — a dependency's library
    /// source, the sysroot — still needs a stable identity (plan-03 §6, §9).
    /// The unit half of its raw is the unit whose root directory contains it,
    /// longest match first, and a synthesized `external` id when none does.
    fn unit_of(&self, path: &Path) -> UnitId {
        let mut best: Option<&UnitFacts> = None;
        for facts in &self.units {
            if !path.starts_with(&facts.unit.root) {
                continue;
            }
            let longer = best.is_none_or(|current| {
                facts.unit.root.as_os_str().len() > current.unit.root.as_os_str().len()
            });
            if longer {
                best = Some(facts);
            }
        }
        match best {
            Some(facts) => facts.unit.id.clone(),
            None => UnitId("external".to_owned()),
        }
    }

    /// Plan-03 §10 — the five categories, from facts the engine already holds.
    pub(crate) fn classify(&self, path: &Path, unit: &Unit) -> Category {
        let absolute = self.absolute(&path.to_string_lossy());
        let unit_package = self
            .units
            .iter()
            .find(|facts| facts.unit.id == unit.id)
            .map(|facts| facts.package.as_str())
            .unwrap_or_default();

        let origin = self
            .units
            .iter()
            .find(|facts| absolute.starts_with(&facts.unit.root))
            .map(|facts| CrateOrigin::Member {
                package: facts.package.clone(),
            })
            .unwrap_or(CrateOrigin::NotAMember);

        classify_facts(&PathFacts {
            path: &absolute,
            sysroot_src: self.sysroot_src.as_deref(),
            origin,
            unit_package,
            // Empty by measurement rather than by omission: nothing loaded an
            // out-dir, so there is no recorded one to name. The structural
            // rule in `classify` still catches the Cargo-shaped path.
            recorded_out_dirs: &[],
        })
    }
}
