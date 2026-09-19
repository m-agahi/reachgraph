//! The public-API snapshot renderer behind `cargo xtask public-api`.
//!
//! Plan-00 §6.1 and plan-02 §7.2.1 specify `public_api_snapshot_matches` as
//! `cargo public-api` **or equivalent**. This is the equivalent, and the reason
//! it exists rather than the tool is worth stating: `cargo public-api` builds
//! rustdoc JSON, rustdoc JSON is nightly-only, and the shared CI installs the
//! stable toolchain. A guard that cannot run in CI is not a guard.
//!
//! What it gives up against `cargo public-api`: auto-trait and blanket-impl
//! rows, and anything a macro expands to. Neither exists in
//! `reachgraph-plugin-api`, which has no macros and no dependencies.
//!
//! What it keeps, which is the whole claim of plan-02 §7.2.1's table over the
//! grep it replaced: the scope is every type, field and signature rather than
//! one trait; an added parameter is a diff whatever it is named; and a leak
//! nobody predicted still shows up, because the rendering is the whole surface.
//!
//! # Effective visibility
//!
//! Only `pub` items inside a chain of `pub` modules are rendered. A `pub` item
//! in a private module is not reachable from outside the crate, so reporting it
//! would make the snapshot churn on changes that are not API changes — and an
//! artifact nobody trusts is an artifact nobody reads.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod golden;
pub mod licenses;

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use quote::ToTokens;
use syn::{
    Attribute, Block, Field, Fields, ImplItem, Item, ItemImpl, ItemMod, TraitItem, Type, Visibility,
};

// ---------------------------------------------------------------------------
// What is rendered
// ---------------------------------------------------------------------------

/// One crate's public surface and the file the rendering is checked in as.
///
/// A single struct names both, so `--bless` and `public_api_snapshot_matches`
/// cannot disagree about which crate was rendered or which file was compared.
#[derive(Clone, Debug)]
pub struct SnapshotTarget {
    /// The package name, used in the artifact's header.
    pub crate_name: String,
    /// The crate root to start walking from.
    pub lib_rs: PathBuf,
    /// Where the rendering is checked in.
    pub snapshot: PathBuf,
}

impl SnapshotTarget {
    /// `reachgraph-plugin-api`, the contract crate ADR-0008's leaks are guarded
    /// against.
    pub fn plugin_api(workspace_root: &Path) -> Self {
        let crate_dir = workspace_root.join("reachgraph-plugin-api");
        Self {
            crate_name: "reachgraph-plugin-api".to_owned(),
            lib_rs: crate_dir.join("src/lib.rs"),
            snapshot: crate_dir.join("public-api.txt"),
        }
    }
}

/// Why a surface could not be rendered.
#[derive(Debug)]
pub enum RenderError {
    /// A source file could not be read or the snapshot could not be written.
    Io {
        /// The file involved.
        path: PathBuf,
        /// The underlying failure.
        source: io::Error,
    },
    /// A source file is not parseable Rust.
    Syntax {
        /// The file involved.
        path: PathBuf,
        /// What the parser said.
        source: syn::Error,
    },
    /// A `pub mod` was declared with no inline body and no file.
    ///
    /// An error rather than an empty section: a renderer that shrugged here
    /// would emit a smaller surface than the crate has, and the snapshot would
    /// agree with itself while the contract drifted.
    MissingModule {
        /// The file that declared it.
        declared_in: PathBuf,
        /// The module's name.
        module: String,
        /// Every path that was tried.
        looked_for: Vec<PathBuf>,
    },
    /// An item kind this renderer does not know how to render was found at a
    /// public path.
    ///
    /// Also an error rather than a skip, and for the same reason.
    UnsupportedItem {
        /// The file it was found in.
        path: PathBuf,
        /// The module path it sits at.
        module: String,
        /// What was written there.
        source_text: String,
    },
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RenderError::Io { path, source } => {
                write!(f, "{}: {source}", path.display())
            }
            RenderError::Syntax { path, source } => {
                write!(f, "{}: {source}", path.display())
            }
            RenderError::MissingModule {
                declared_in,
                module,
                looked_for,
            } => {
                let tried: Vec<String> =
                    looked_for.iter().map(|p| p.display().to_string()).collect();
                write!(
                    f,
                    "{} declares `pub mod {module};` but no file defines it. Tried: {}",
                    declared_in.display(),
                    tried.join(", ")
                )
            }
            RenderError::UnsupportedItem {
                path,
                module,
                source_text,
            } => write!(
                f,
                "{}: the public item `{source_text}` in `{module}` is a kind this renderer \
                 does not know. Teach it rather than skipping it, or the snapshot under-reports.",
                path.display()
            ),
        }
    }
}

impl std::error::Error for RenderError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            RenderError::Io { source, .. } => Some(source),
            RenderError::Syntax { source, .. } => Some(source),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// Render `target`'s public surface.
pub fn render(target: &SnapshotTarget) -> Result<String, RenderError> {
    let module = target.crate_name.replace('-', "_");
    let mut entries = Vec::new();
    let mut private_types = Vec::new();
    walk_file(&target.lib_rs, &module, &mut entries, &mut private_types)?;

    // Two passes. An `impl` block carries no visibility of its own, so whether
    // it is public is a property of the type it is on — which is only known
    // once every item has been seen.
    //
    // THE TEST IS "IS THIS TYPE PRIVATE AND LOCAL", NOT "IS IT PUBLIC AND
    // LOCAL", and the difference is the whole correctness of this filter. The
    // first version asked the second question and so dropped
    // `impl LocalTrait for String` — public surface, because a consumer gets a
    // new method on a foreign type — along with every blanket
    // `impl<T: Bound> Trait for T`, whose self type is a generic parameter and
    // is local to nothing. Silently. Defaulting to KEEP means an exotic shape
    // over-reports and shows up in the diff, which somebody reads; defaulting
    // to drop means it disappears, which nobody can.
    entries.retain(|entry| match &entry.on_type {
        Some(on_type) => !private_types.contains(on_type),
        None => true,
    });

    entries.sort_by(|a, b| {
        (&a.module, a.kind as u8, &a.name).cmp(&(&b.module, b.kind as u8, &b.name))
    });

    let mut out = String::new();
    out.push_str(&header(&target.crate_name));

    let mut current = String::new();
    for entry in &entries {
        if entry.module != current {
            out.push_str(&format!("\n## {}\n\n", entry.module));
            current.clone_from(&entry.module);
        }
        out.push_str(&entry.rendered);
        out.push('\n');
    }

    // Exactly one trailing newline. This repository's `end-of-file-fixer` hook
    // rewrites a file that ends in a blank line, so a renderer that emitted one
    // would put the hook and `public_api_snapshot_matches` in a loop: `--bless`
    // writes the artifact, the hook trims it, and the next run is red for a
    // reason that has nothing to do with the contract.
    out.truncate(out.trim_end().len());
    out.push('\n');

    Ok(out)
}

/// Render `target`'s public surface and write it to `out`.
///
/// `--bless` and `public_api_snapshot_matches` both come through here. A second
/// write path would let the snapshot be blessed through code the test never
/// runs.
pub fn emit(target: &SnapshotTarget, out: &Path) -> Result<(), RenderError> {
    let rendered = render(target)?;
    fs::write(out, rendered).map_err(|source| RenderError::Io {
        path: out.to_path_buf(),
        source,
    })
}

fn header(crate_name: &str) -> String {
    format!(
        "\
# {crate_name} — public API surface
#
# GENERATED. Regenerate with `cargo xtask public-api --bless`, which is a
# deliberate command on purpose: a snapshot that repairs itself when the test
# fails asserts nothing (plan-02 §7.2.1).
#
# `public_api_snapshot_matches` asserts this file byte for byte. A diff here is
# a change to the plugin contract — an added field, a changed signature, a
# leaked type — and it is a diff somebody has to read. The assertion makes the
# change visible; it cannot make anyone look.
#
# Doc comments and function bodies are normalised away; derives, bounds and
# signatures are not.
"
    )
}

// ---------------------------------------------------------------------------
// The walk
// ---------------------------------------------------------------------------

/// Sort rank. Types first, then the traits over them, then free functions.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Kind {
    Value = 0,
    Type = 1,
    Trait = 2,
    Function = 3,
    Reexport = 4,
    Impl = 5,
}

struct Entry {
    module: String,
    kind: Kind,
    name: String,
    /// `Some` for an `impl` block: the bare name of the type it is on.
    on_type: Option<String>,
    rendered: String,
}

fn is_public(vis: &Visibility) -> bool {
    matches!(vis, Visibility::Public(_))
}

fn walk_file(
    path: &Path,
    module: &str,
    out: &mut Vec<Entry>,
    private_types: &mut Vec<String>,
) -> Result<(), RenderError> {
    let source = fs::read_to_string(path).map_err(|source| RenderError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let parsed = syn::parse_file(&source).map_err(|source| RenderError::Syntax {
        path: path.to_path_buf(),
        source,
    })?;

    walk_items(
        parsed.items,
        path,
        &module_dir(path),
        module,
        out,
        private_types,
    )
}

/// Where a file's child modules live: `src/lib.rs` and `src/foo/mod.rs` own
/// their own directory, `src/foo.rs` owns `src/foo/`.
fn module_dir(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or(Path::new("")).to_path_buf();
    match path.file_name().and_then(|n| n.to_str()) {
        Some("lib.rs") | Some("mod.rs") | None => parent,
        Some(_) => match path.file_stem().and_then(|s| s.to_str()) {
            Some(stem) => parent.join(stem),
            None => parent,
        },
    }
}

fn walk_items(
    items: Vec<Item>,
    path: &Path,
    dir: &Path,
    module: &str,
    out: &mut Vec<Entry>,
    private_types: &mut Vec<String>,
) -> Result<(), RenderError> {
    for item in items {
        match item {
            Item::Mod(item_mod) => walk_mod(item_mod, path, dir, module, out, private_types)?,
            // Never public, and never part of a surface.
            Item::ExternCrate(_) | Item::ForeignMod(_) => {}
            other => {
                if let Some(name) = private_type_name(&other) {
                    private_types.push(name);
                }
                if let Some(entry) = entry_for(other, path, dir, module)? {
                    out.push(entry);
                }
            }
        }
    }
    Ok(())
}

fn walk_mod(
    item_mod: ItemMod,
    path: &Path,
    dir: &Path,
    module: &str,
    out: &mut Vec<Entry>,
    private_types: &mut Vec<String>,
) -> Result<(), RenderError> {
    // A private module's contents are not reachable from outside the crate, so
    // the whole subtree is skipped rather than walked and filtered.
    if !is_public(&item_mod.vis) {
        return Ok(());
    }

    let child = format!("{module}::{}", item_mod.ident);
    match item_mod.content {
        Some((_, items)) => walk_items(items, path, dir, &child, out, private_types),
        None => {
            let candidates = [
                dir.join(format!("{}.rs", item_mod.ident)),
                dir.join(item_mod.ident.to_string()).join("mod.rs"),
            ];
            match candidates.iter().find(|candidate| candidate.is_file()) {
                Some(file) => walk_file(file, &child, out, private_types),
                None => Err(RenderError::MissingModule {
                    declared_in: path.to_path_buf(),
                    module: item_mod.ident.to_string(),
                    looked_for: candidates.to_vec(),
                }),
            }
        }
    }
}

/// The name of a type this crate declares and does NOT export.
///
/// An `impl` on one of these is not reachable from outside, so it is the only
/// case the `impl` filter drops. Collected during the walk because a private
/// item is otherwise discarded before anything can ask about it.
fn private_type_name(item: &Item) -> Option<String> {
    let (vis, ident) = match item {
        Item::Struct(it) => (&it.vis, &it.ident),
        Item::Enum(it) => (&it.vis, &it.ident),
        Item::Union(it) => (&it.vis, &it.ident),
        Item::Type(it) => (&it.vis, &it.ident),
        _ => return None,
    };
    (!is_public(vis)).then(|| ident.to_string())
}

fn entry_for(
    item: Item,
    path: &Path,
    _dir: &Path,
    module: &str,
) -> Result<Option<Entry>, RenderError> {
    let (kind, name, on_type, item) = match item {
        Item::Struct(mut it) => {
            if !is_public(&it.vis) {
                return Ok(None);
            }
            let name = it.ident.to_string();
            strip_attrs(&mut it.attrs);
            strip_private_fields(&mut it.fields);
            (Kind::Type, name, None, Item::Struct(it))
        }
        Item::Enum(mut it) => {
            if !is_public(&it.vis) {
                return Ok(None);
            }
            let name = it.ident.to_string();
            strip_attrs(&mut it.attrs);
            for variant in &mut it.variants {
                strip_attrs(&mut variant.attrs);
                for field in &mut variant.fields {
                    strip_attrs(&mut field.attrs);
                }
            }
            (Kind::Type, name, None, Item::Enum(it))
        }
        Item::Union(mut it) => {
            if !is_public(&it.vis) {
                return Ok(None);
            }
            let name = it.ident.to_string();
            strip_attrs(&mut it.attrs);
            it.fields.named = it
                .fields
                .named
                .into_iter()
                .filter(|field| is_public(&field.vis))
                .map(|mut field| {
                    strip_attrs(&mut field.attrs);
                    field
                })
                .collect();
            (Kind::Type, name, None, Item::Union(it))
        }
        Item::Type(mut it) => {
            if !is_public(&it.vis) {
                return Ok(None);
            }
            let name = it.ident.to_string();
            strip_attrs(&mut it.attrs);
            (Kind::Type, name, None, Item::Type(it))
        }
        Item::Trait(mut it) => {
            if !is_public(&it.vis) {
                return Ok(None);
            }
            let name = it.ident.to_string();
            strip_attrs(&mut it.attrs);
            for trait_item in &mut it.items {
                strip_trait_item(trait_item);
            }
            (Kind::Trait, name, None, Item::Trait(it))
        }
        Item::TraitAlias(mut it) => {
            if !is_public(&it.vis) {
                return Ok(None);
            }
            let name = it.ident.to_string();
            strip_attrs(&mut it.attrs);
            (Kind::Trait, name, None, Item::TraitAlias(it))
        }
        Item::Fn(mut it) => {
            if !is_public(&it.vis) {
                return Ok(None);
            }
            let name = it.sig.ident.to_string();
            strip_attrs(&mut it.attrs);
            it.block = Box::new(empty_block());
            (Kind::Function, name, None, Item::Fn(it))
        }
        Item::Const(mut it) => {
            if !is_public(&it.vis) {
                return Ok(None);
            }
            let name = it.ident.to_string();
            strip_attrs(&mut it.attrs);
            (Kind::Value, name, None, Item::Const(it))
        }
        Item::Static(mut it) => {
            if !is_public(&it.vis) {
                return Ok(None);
            }
            let name = it.ident.to_string();
            strip_attrs(&mut it.attrs);
            (Kind::Value, name, None, Item::Static(it))
        }
        Item::Use(mut it) => {
            if !is_public(&it.vis) {
                return Ok(None);
            }
            strip_attrs(&mut it.attrs);
            let name = it.tree.to_token_stream().to_string();
            (Kind::Reexport, name, None, Item::Use(it))
        }
        Item::Impl(it) => return Ok(impl_entry(it, module)),
        other => {
            return Err(RenderError::UnsupportedItem {
                path: path.to_path_buf(),
                module: module.to_owned(),
                source_text: other.to_token_stream().to_string(),
            })
        }
    };

    Ok(Some(Entry {
        module: module.to_owned(),
        kind,
        name,
        on_type,
        rendered: unparse(item),
    }))
}

/// An `impl` block's public surface.
///
/// For an inherent block, per-item: `pub fn` is API and a bare `fn` is not. For
/// a trait block, every item is reachable through the trait, so the block is
/// rendered whole.
fn impl_entry(mut item: ItemImpl, module: &str) -> Option<Entry> {
    let on_type = base_type_name(&item.self_ty)?;
    let is_trait_impl = item.trait_.is_some();

    strip_attrs(&mut item.attrs);
    item.items.retain(|impl_item| {
        is_trait_impl
            || match impl_item {
                ImplItem::Fn(it) => is_public(&it.vis),
                ImplItem::Const(it) => is_public(&it.vis),
                ImplItem::Type(it) => is_public(&it.vis),
                _ => false,
            }
    });
    if item.items.is_empty() {
        return None;
    }
    for impl_item in &mut item.items {
        strip_impl_item(impl_item);
    }

    let name = match &item.trait_ {
        Some((_, path, _)) => format!("{} for {on_type}", path.to_token_stream()),
        None => on_type.clone(),
    };

    Some(Entry {
        module: module.to_owned(),
        kind: Kind::Impl,
        name,
        on_type: Some(on_type),
        rendered: unparse(Item::Impl(item)),
    })
}

/// The bare name of the type an `impl` is on: `Registry` for `Registry`,
/// `Vec` for `Vec<T>`. `None` for a type with no nameable head, such as a
/// tuple or a reference.
fn base_type_name(ty: &Type) -> Option<String> {
    match ty {
        Type::Path(path) => path.path.segments.last().map(|s| s.ident.to_string()),
        Type::Reference(reference) => base_type_name(&reference.elem),
        Type::Group(group) => base_type_name(&group.elem),
        Type::Paren(paren) => base_type_name(&paren.elem),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Normalisation
// ---------------------------------------------------------------------------

/// Keep the attributes that are contract; drop the ones that are commentary or
/// implementation.
///
/// `derive` is kept because `Clone` on a schema type is something a consumer
/// depends on and removing one is a breaking change. `cfg` is kept because a
/// feature gate decides whether an item exists at all. `doc` is dropped
/// because rewording a sentence must not produce a diff a reviewer has to read
/// — a snapshot that churns is a snapshot nobody reads.
fn strip_attrs(attrs: &mut Vec<Attribute>) {
    attrs.retain(|attr| {
        let path = attr.path();
        path.is_ident("derive")
            || path.is_ident("cfg")
            || path.is_ident("non_exhaustive")
            || path.is_ident("must_use")
            || path.is_ident("deprecated")
            || path.is_ident("repr")
    });
}

/// Drop fields a consumer cannot name.
///
/// A private field is not API, and rendering one would make the snapshot churn
/// whenever an implementation detail moved — which is how an artifact stops
/// being read. Enum variant fields are NOT filtered: a variant carries no
/// visibility of its own and its fields are as public as the enum.
///
/// CAVEAT on tuple structs: dropping a private field shifts the positions of
/// the public ones after it, so `pub struct T(pub u8, String, pub u8)` renders
/// as two consecutive public fields. Nothing in this workspace has that shape,
/// and a struct with any private field cannot be constructed by literal from
/// outside the crate anyway.
fn strip_private_fields(fields: &mut Fields) {
    let retained = |mut field: Field| -> Option<Field> {
        if is_public(&field.vis) {
            strip_attrs(&mut field.attrs);
            Some(field)
        } else {
            None
        }
    };

    match fields {
        Fields::Named(named) => {
            named.named = std::mem::take(&mut named.named)
                .into_iter()
                .filter_map(retained)
                .collect();
        }
        Fields::Unnamed(unnamed) => {
            unnamed.unnamed = std::mem::take(&mut unnamed.unnamed)
                .into_iter()
                .filter_map(retained)
                .collect();
        }
        Fields::Unit => {}
    }
}

fn strip_impl_item(item: &mut ImplItem) {
    match item {
        ImplItem::Fn(it) => {
            strip_attrs(&mut it.attrs);
            it.block = empty_block();
        }
        ImplItem::Const(it) => strip_attrs(&mut it.attrs),
        ImplItem::Type(it) => strip_attrs(&mut it.attrs),
        ImplItem::Macro(it) => strip_attrs(&mut it.attrs),
        _ => {}
    }
}

fn strip_trait_item(item: &mut TraitItem) {
    match item {
        TraitItem::Fn(it) => {
            strip_attrs(&mut it.attrs);
            // A provided method's body is implementation; that it HAS one is
            // contract, because it decides whether an implementor must write it.
            if it.default.is_some() {
                it.default = Some(empty_block());
            }
        }
        TraitItem::Const(it) => strip_attrs(&mut it.attrs),
        TraitItem::Type(it) => strip_attrs(&mut it.attrs),
        TraitItem::Macro(it) => strip_attrs(&mut it.attrs),
        _ => {}
    }
}

fn empty_block() -> Block {
    Block {
        brace_token: syn::token::Brace::default(),
        stmts: Vec::new(),
    }
}

fn unparse(item: Item) -> String {
    prettyplease::unparse(&syn::File {
        shebang: None,
        attrs: Vec::new(),
        items: vec![item],
    })
}
