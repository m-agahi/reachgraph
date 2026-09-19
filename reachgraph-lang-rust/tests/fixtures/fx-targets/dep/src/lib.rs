//! A callee outside every enumerated unit.

/// Called from `t`, which does not enumerate this crate as a unit.
pub fn outside() -> u32 {
    9
}
