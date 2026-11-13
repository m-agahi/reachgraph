//! A `#[test]` function in a `tests/` target. With the standard library
//! loaded and cfg(test) off, it resolved as cfg-gated and was not indexed.

#[test]
fn orphan_is_seven() {
    assert_eq!(t::orphan(), 7);
}
