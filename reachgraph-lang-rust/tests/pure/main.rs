//! Plan-03 §13 Tier A — every decision this crate makes, with no `ra_ap`, no
//! filesystem and no workspace anywhere in the suite.
//!
//! The absence is the point rather than a speed optimisation. A decision that
//! cannot be tested here is a decision fused to the engine, and the fusion is
//! what ADR-0008's eight leaks are each an instance of.

mod classify;
mod engine;
mod ids;
mod kinds;
mod preflight;
