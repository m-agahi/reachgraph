//! The calling crate.

/// Calls into the sibling crate twice, from one body.
///
/// Two call sites, not one callee: plan-03 §9 emits one `Edge` per call site.
pub fn caller() -> u32 {
    b::leaf() + b::leaf()
}
