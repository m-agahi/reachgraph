//! Registry wiring — plan-06 §3.
//!
//! Built from Cargo features and nothing else. ADR-0008 forbids the branch this
//! file would otherwise be:
//!
//! ```ignore
//! if is_rust_project(root) { ... }      // forbidden, ADR-0008
//! ```
//!
//! Detection is plugin-declared, so adding a language is a new crate and a new
//! feature rather than an edit here or in the waist. v0.1 registers one
//! language plugin and one root provider (ADR-0008). A registry with two
//! entries costs nothing; a hardcoded branch costs a core change per language.

use reachgraph_plugin_api::{Registry, RegistryError};

#[cfg(any(feature = "lang-rust", feature = "roots-proto-tonic"))]
use reachgraph_plugin_api::Registration;

/// The analysis registry this build ships.
///
/// Fallible because [`Registry::register`] checks a plugin's declared
/// capabilities against the views handed over, in both directions. A
/// mis-wiring here is a startup error with a message rather than an index that
/// quietly has no roots.
///
/// Each feature is a function taking and returning the registry rather than a
/// `#[cfg]` block over one mutable binding. The binding shape needs an
/// `#[allow(unused_mut)]` for the no-plugin build, and this workspace has no
/// `#[allow]` in hand-written code — a lint suppression is how a real warning
/// starts being invisible.
pub fn analysis_registry() -> Result<Registry, RegistryError> {
    let registry = with_language_plugins(Registry::new())?;
    with_root_providers(registry)
}

#[cfg(feature = "lang-rust")]
fn with_language_plugins(mut registry: Registry) -> Result<Registry, RegistryError> {
    registry.register(
        Registration::of(reachgraph_lang_rust::RustPlugin::new())
            .symbols()
            .edges()
            .classifier(),
    )?;
    Ok(registry)
}

#[cfg(not(feature = "lang-rust"))]
fn with_language_plugins(registry: Registry) -> Result<Registry, RegistryError> {
    Ok(registry)
}

#[cfg(feature = "roots-proto-tonic")]
fn with_root_providers(mut registry: Registry) -> Result<Registry, RegistryError> {
    registry.register(
        Registration::of(reachgraph_roots_proto_tonic::ProtoTonicPlugin::new()).roots(),
    )?;
    Ok(registry)
}

#[cfg(not(feature = "roots-proto-tonic"))]
fn with_root_providers(registry: Registry) -> Result<Registry, RegistryError> {
    Ok(registry)
}
