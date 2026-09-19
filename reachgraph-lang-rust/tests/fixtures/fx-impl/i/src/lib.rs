//! A trait, its real implementation, and an inherent impl.

/// The trait both implementations share.
pub trait Svc {
    /// The operation.
    fn create(&self) -> u32;
}

/// The real implementation's self type.
pub struct Real;

impl Svc for Real {
    fn create(&self) -> u32 {
        1
    }
}

impl Real {
    /// An inherent method, so `impl Real` is a distinct container.
    pub fn helper(&self) -> u32 {
        2
    }
}
