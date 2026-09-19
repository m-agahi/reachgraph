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

// An item whose doc block is PRESENT and EMPTY, which is a different case from
// having none. Written with `//` rather than `///` on purpose: a doc comment
// here would concatenate with the attribute below and the case would not be
// empty. Without this item the honest-absence guard is unexercised — MEASURED
// by mutation, returning `Some(String::new())` left the suite green.
#[doc = ""]
pub fn empty_doc() -> u32 {
    4
}

// Three spellings of an empty doc, because they are NOT equivalent and the
// difference is measured rather than assumed: the two attribute forms arrive
// from the engine as no docs at all, and only this one — a `///` block with no
// text — reaches this crate's own honest-absence guard.
///
///
pub fn whitespace_doc() -> u32 {
    5
}

#[doc = "   "]
pub fn spaces_doc() -> u32 {
    6
}
