//! The test double, and the whole reason `is_test` is not enough.
//!
//! MEASURED (plan-04 §1 M6): `tests/service.rs:145` is
//! `impl TaskDbService for MockDb`. Without a guard the **consumed** service
//! acquires a served signal from a mock and produces phantom roots.

use crate::pb::acme::store::v2::widget_db_server::WidgetDb;

struct MockDb;

impl WidgetDb for MockDb {
    async fn create_widget(&self, request: CreateWidgetRequest) -> CreateWidgetResponse {
        todo!()
    }
}
