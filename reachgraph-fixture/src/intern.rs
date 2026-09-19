//! Turning case data into the `&'static str` the contract asks for.
//!
//! [`reachgraph_plugin_api::PluginId`] holds a `&'static str` and
//! [`reachgraph_plugin_api::Detection`] holds `&'static [&'static str]`. For
//! every real plugin that is free: ADR-0002 makes plugins compile-time, so a
//! plugin's id is a literal in its own source. The fixture is the one plugin
//! whose identity is **data**, because a case declares it — the `two_plugins`
//! case exists precisely to load two documents under two different ids.
//!
//! So this crate buys `'static` from the heap, once per distinct string, and
//! never frees it. That is a leak in the literal sense and it is bounded by the
//! number of distinct ids and marker names a test run loads, which is a handful.
//!
//! **The alternative was considered and rejected**: widening `PluginId` to
//! `String` or `Cow<'static, str>` would cost `Copy` and a cheap hash on
//! [`reachgraph_plugin_api::NodeId`], which the waist hashes constantly — the
//! fixture taxing the core's hot path to spare itself thirty lines. The
//! friction here is representational rather than expressive: interning lets
//! this crate report exactly the id a case declared, and lets it lie about
//! nothing. That is the test for whether a contract change is warranted, and
//! this one does not pass it.

use std::collections::BTreeSet;
use std::sync::{Mutex, OnceLock};

/// Every string this process has interned, so a repeated load costs nothing and
/// leaks nothing further.
fn pool() -> &'static Mutex<BTreeSet<&'static str>> {
    static POOL: OnceLock<Mutex<BTreeSet<&'static str>>> = OnceLock::new();
    POOL.get_or_init(|| Mutex::new(BTreeSet::new()))
}

/// Every list this process has interned, keyed by the list itself.
fn list_pool() -> &'static Mutex<BTreeSet<&'static [&'static str]>> {
    static POOL: OnceLock<Mutex<BTreeSet<&'static [&'static str]>>> = OnceLock::new();
    POOL.get_or_init(|| Mutex::new(BTreeSet::new()))
}

/// The `&'static str` equal to `value`, allocating one the first time it is
/// asked for.
pub(crate) fn str(value: &str) -> &'static str {
    let mut pool = pool()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    if let Some(existing) = pool.get(value) {
        return existing;
    }

    let leaked: &'static str = Box::leak(value.to_owned().into_boxed_str());
    pool.insert(leaked);
    leaked
}

/// The `&'static [&'static str]` equal to `values`, interning each element and
/// then the list.
pub(crate) fn list(values: &[String]) -> &'static [&'static str] {
    let interned: Vec<&'static str> = values.iter().map(|value| str(value)).collect();

    let mut pool = list_pool()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    if let Some(existing) = pool.get(interned.as_slice()) {
        return existing;
    }

    let leaked: &'static [&'static str] = Box::leak(interned.into_boxed_slice());
    pool.insert(leaked);
    leaked
}
