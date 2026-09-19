//! A call dispatched through a generic parameter.

/// The trait the generic call goes through.
pub trait Op {
    /// The generic call's target.
    fn run(&self) -> u32;
}

/// The only implementor.
pub struct Only;

impl Op for Only {
    fn run(&self) -> u32 {
        5
    }
}

/// Calls `run` through a generic parameter rather than a concrete type.
pub fn through_generic<T: Op>(value: &T) -> u32 {
    value.run()
}

/// Calls `run` on the concrete type, as the control.
pub fn through_concrete(value: &Only) -> u32 {
    value.run()
}
