//! The waist's test suite — plan-01 §10.
//!
//! # One binary, several modules
//!
//! Every `tests/*.rs` file is its own binary, and a shared helper module in one
//! of several binaries is unused in whichever of them does not call it. That is
//! a warning, `-D warnings` makes it a failure, and an `allow` attribute to
//! silence it would be the first suppression in the tree. One binary with one
//! helper module has no unused half.

mod support;

mod assemble;
