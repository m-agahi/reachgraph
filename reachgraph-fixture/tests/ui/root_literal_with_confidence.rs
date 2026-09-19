// A `Root` literal that sets `confidence` must not compile.
//
// The field is GONE, not deprecated. `docs/design.md` §5 MEASURED what one
// invites: `code_graph` emits 59 `CALLS` edges at confidence 0.55, each with
// two or three candidate targets — a number that records indecision and then
// renders as if it were a measurement. A root either binds to a handler or it
// does not, and `RootBinding::Unbound` carries the reason.
use reachgraph_plugin_api::{ContractId, Direction, NodeId, PluginId, Root, RootBinding};

fn main() {
    let _ = Root {
        contract: ContractId("acme.task".to_owned()),
        version: Some("v1".to_owned()),
        service: "TaskService".to_owned(),
        operation: "CreateTask".to_owned(),
        direction: Direction::Served,
        join_key: "acme.task.v1.TaskService/CreateTask".to_owned(),
        binding: RootBinding::Bound(NodeId {
            plugin: PluginId("fixture"),
            raw: "fn:handlers/create_task".to_owned(),
        }),
        confidence: 0.55,
    };
}
