//! A test that constructs the generated client.
//!
//! Plan-04 §6: the corroborating reference must be in first-party **non-test**
//! source. A repository whose only mention of `WidgetDbClient` is in a test has
//! not been shown to consume the contract in production.

use crate::pb::acme::store::v2::widget_db_client::WidgetDbClient;

#[test]
fn the_client_constructs() {
    let _ = WidgetDbClient::new(channel());
}
