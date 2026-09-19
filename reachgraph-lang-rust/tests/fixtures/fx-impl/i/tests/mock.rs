//! The test-side implementation, with the same operation name as the real one.
//!
//! MEASURED, design.md §4: `tests/service.rs:145`'s `impl TaskDbService for
//! MockDb` and `src/service/handlers.rs:22`'s real handler both define
//! `create_task`. Plan-04 §6 uses `is_test` to tell them apart, and getting it
//! wrong manufactures a phantom root.

use i::Svc;

struct Mock;

impl Svc for Mock {
    fn create(&self) -> u32 {
        3
    }
}

#[test]
fn the_mock_creates() {
    assert_eq!(Mock.create(), 3);
}
