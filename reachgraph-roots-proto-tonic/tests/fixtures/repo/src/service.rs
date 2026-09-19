//! First-party source: one served impl, one consumed client.
//!
//! This file is a fixture and is never compiled. It exists so the direction
//! pass has real `.rs` text to read and real paths to resolve symbols against.

use crate::pb::acme::api::v1::widgets_server::Widgets;
use crate::pb::acme::store::v2::widget_db_client::WidgetDbClient;

pub struct Svc {
    db: WidgetDbClient<Channel>,
}

impl Svc {
    pub fn new(channel: Channel) -> Self {
        Self {
            db: WidgetDbClient::new(channel),
        }
    }
}

impl Widgets for Svc {
    async fn create_widget(&self, request: CreateWidgetRequest) -> CreateWidgetResponse {
        self.db.create_widget(request).await
    }

    async fn list_widgets(&self, request: ListWidgetsRequest) -> ListWidgetsResponse {
        todo!()
    }
}
