//! A first-party call from one file into another, inside one crate.

pub mod inner;

/// Calls `inner::leaf`, which the symbol walk emits under this crate's unit.
pub fn caller() -> u32 {
    inner::leaf()
}
