//! A third target. Named so its unit id sorts before the library's, which is
//! what the defect needed: units are ordered by id, and a rule that answered
//! "which unit" by directory containment took the first of several targets
//! sharing one manifest directory.

#[test]
fn caller_adds_up() {
    assert_eq!(t::caller(), 5);
}
