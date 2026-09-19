//! A module doc, written with `//!`, on the crate root.
//!
//! It spans two paragraphs so a last-line-only reader loses the first.

/// First line of a multi-line block.
///
/// Second paragraph, which the code_graph baseline discards: MEASURED,
/// design.md §5 keeps only the last line of a `///` block, sigil attached,
/// truncated at 83 characters. The naïve assumption is that Ünicode survives
/// the round trip — 日本語 included, on purpose, because ADR-0008 leak 3 is an
/// encoding mix-up that ASCII fixtures hide.
pub fn documented() -> u32 {
    0
}

#[doc = "An attribute doc, which is the third spelling."]
pub fn attribute_documented() -> u32 {
    1
}

/// A type with a method, so a method's doc can be asserted non-empty.
pub struct Holder;

impl Holder {
    /// A method doc. MEASURED, design.md §5: 0 of 40 `Method` nodes in the
    /// code_graph baseline carry any docstring at all.
    pub fn method(&self) -> u32 {
        2
    }
}

pub fn undocumented() -> u32 {
    3
}
