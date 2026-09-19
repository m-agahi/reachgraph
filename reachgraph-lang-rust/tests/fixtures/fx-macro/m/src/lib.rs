//! A call that crosses into generated code.

include!(concat!(env!("OUT_DIR"), "/generated.rs"));

/// The local call target, as the control: if this edge is missing too, the
/// fixture is broken rather than the mechanism.
pub fn local_leaf() -> u32 {
    13
}

/// Calls one local function and one generated function.
pub fn caller() -> u32 {
    local_leaf() + generated_leaf()
}
