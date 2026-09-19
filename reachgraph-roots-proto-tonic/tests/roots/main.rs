//! Plan-04 §12 — every decision this crate makes, in milliseconds.
//!
//! There is no slow tier here and there is not meant to be one. The binder is
//! exercised against a hand-built `FakeIndex` rather than against
//! `reachgraph-lang-rust`, which keeps this suite off plan-03's Tier-B path
//! **and** gives `SymbolIndex` a second independent consumer: a field only a
//! real engine can produce would stop `FakeIndex` compiling (plan-04 §12).

mod names;
mod versions;
