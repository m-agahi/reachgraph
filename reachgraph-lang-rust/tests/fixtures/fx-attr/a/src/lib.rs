//! Three impls of the same shape, differing only in what could resolve.

include!(concat!(env!("OUT_DIR"), "/service.rs"));

/// The trait the bare impl implements. Ordinary first-party source.
pub trait Bare {
    /// The operation.
    fn bare(&self) -> u32;
}

/// The trait the decorated impl implements. Also first-party.
pub trait Decorated {
    /// The operation.
    fn decorated(&self) -> u32;
}

/// The self type every impl is on.
pub struct Subject;

impl Bare for Subject {
    fn bare(&self) -> u32 {
        1
    }
}

/// Control: an attribute macro above the block, trait resolvable.
#[pm::keep]
impl Decorated for Subject {
    fn decorated(&self) -> u32 {
        2
    }
}

/// The case, and the shape every tonic server has: an attribute macro above a
/// block implementing a trait that lives in build-script output reachgraph
/// does not load into the crate graph (plan-03 §9 D-D).
#[pm::keep]
impl Generated for Subject {
    fn generated(&self) -> u32 {
        3
    }
}
