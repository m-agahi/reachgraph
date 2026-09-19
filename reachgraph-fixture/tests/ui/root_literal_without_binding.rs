// A `Root` literal that omits `binding` must not compile.
//
// `binding` replaced an earlier `node` plus `confidence` pair, and a separate
// `node` field would have been incoherent for an unbound root. Whether a root
// bound is the fact the artifact reports, so it cannot be left unsaid:
// ADR-0007 makes an unbound root a reported gap rather than a dropped row.
use reachgraph_plugin_api::{ContractId, Direction, Root};

fn main() {
    let _ = Root {
        contract: ContractId("acme.task".to_owned()),
        version: Some("v1".to_owned()),
        service: "TaskService".to_owned(),
        operation: "CreateTask".to_owned(),
        direction: Direction::Served,
        join_key: "acme.task.v1.TaskService/CreateTask".to_owned(),
    };
}
