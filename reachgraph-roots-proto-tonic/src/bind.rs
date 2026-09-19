//! Handler binding — plan-04 §7, and the generated-leaf binding of §11.
//!
//! Binding is `container` plus test-context, **never the name alone**:
//! MEASURED (plan-04 §1 M6) that `create_task` exists twice in one repository,
//! once as a handler and once as a test double of a different service.

use reachgraph_plugin_api::{NodeId, Symbol, SymbolIndex, SymbolKind};

use crate::direction::{is_generated_out_path, is_test_context};
use crate::keys::join_key;
use crate::names::{camel_to_snake, impl_header_names_self_type, impl_header_names_trait};
use crate::unbound::UnboundReason;

/// One RPC, as this crate refers to it everywhere downstream of parsing.
///
/// A named type at the seam rather than three loose `&str`s: the join key is
/// spelled once, at construction, so no call site can spell it differently.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Operation {
    /// The bare service name, for display and for the impl-header comparison.
    pub service: String,
    /// The bare RPC name, for display.
    pub rpc: String,
    /// The fully-qualified key — plan-04 §3.
    pub join_key: String,
}

impl Operation {
    /// One operation of a service in a package.
    pub fn new(package: Option<&str>, service: &str, rpc: &str) -> Self {
        Self {
            service: service.to_owned(),
            rpc: rpc.to_owned(),
            join_key: join_key(package, service, rpc),
        }
    }

    /// The handler name tonic generates for this RPC.
    pub fn handler_name(&self) -> String {
        camel_to_snake(&self.rpc)
    }
}

/// A root's node, or the reason it has none.
///
/// This mirrors `RootBinding` and is not it: `RootBinding::Unbound` carries a
/// rendered `String`, and the enum here keeps the case itself so a test can
/// assert on the decision rather than on prose.
#[derive(Clone, Debug)]
pub enum Binding {
    /// The node this root's operation enters the graph at.
    Bound(NodeId),
    /// No node, and why.
    Unbound(UnboundReason),
}

impl Binding {
    /// The rendered reason, for an unbound binding.
    ///
    /// A bound binding has none, and says so rather than returning an empty
    /// string that would read as an absent reason on an unbound root.
    pub fn reason(&self) -> String {
        match self {
            Binding::Bound(node) => format!("bound to {}", node.raw),
            Binding::Unbound(reason) => reason.to_string(),
        }
    }
}

/// Find the tonic handler for a served operation.
///
/// # The filters, and why each one is there
///
/// 1. `by_name(camel_to_snake(rpc))` — plan-00 §3.4 returns a `Vec` precisely
///    because design.md §4 MEASURED that this name collides.
/// 2. a method or a function, because an impl block and a struct can share a
///    name with neither being callable;
/// 3. not test-shaped ([`is_test_context`]);
/// 4. its container is an `impl <Service> for …`. MEASURED that this is also
///    what rejects the trait's own declaration: the walk emits a trait's
///    associated functions too, and their container's `raw_kind` is `Trait`
///    rather than an impl header.
///
/// Exactly one survivor binds. Two or more is a **reported gap**, never a pick:
/// MEASURED, design.md §5, the failure mode being avoided is code_graph's 59
/// `CALLS` edges at confidence 0.55, a number that records indecision and
/// renders as though it recorded a measurement.
pub fn bind_handler(symbols: &dyn SymbolIndex, operation: &Operation) -> Binding {
    let handler = operation.handler_name();
    let candidates = symbols.by_name(&handler);
    if candidates.is_empty() {
        return Binding::Unbound(UnboundReason::NoCandidate {
            service: operation.service.clone(),
            handler,
        });
    }

    let bound = survivors(symbols, &candidates, |container| {
        impl_header_names_trait(&container.raw_kind, &operation.service)
    });

    match bound.as_slice() {
        [one] => Binding::Bound(one.id.clone()),
        [] => Binding::Unbound(UnboundReason::NoMatchingImpl {
            service: operation.service.clone(),
            handler,
            named: candidates.len(),
        }),
        many => Binding::Unbound(UnboundReason::Ambiguous {
            handler,
            count: many.len(),
        }),
    }
}

/// Find the generated client method a consumed operation leaves through.
///
/// Plan-04 §11: binding the consumed root there attaches the join key to a real
/// node, so the v0.2 cross-repository join becomes an edge between two existing
/// nodes rather than a synthesised one. The shape is the other half of plan-03
/// §8's grammar — an **inherent** `impl <Service>Client<…>` — and the file must
/// be a build script's output, which is what separates the generated leaf from
/// a first-party wrapper of the same name.
///
/// # This path is MEASURED not to fire against `reachgraph-lang-rust` today
///
/// PR D measured that generated code is not in the crate graph at all, built or
/// unbuilt (plan-03 §9 D-D), so no symbol with such a path reaches this
/// function and every consumed root takes the unbound arm. The code path is
/// written and tested anyway because the v0.2 join needs the data contract, and
/// because the day `OUT_DIR` becomes loadable this is the binding that has to
/// already be right.
pub fn bind_generated_client(symbols: &dyn SymbolIndex, operation: &Operation) -> Binding {
    let handler = operation.handler_name();
    let client_type = format!("{}Client", operation.service);
    let candidates = symbols.by_name(&handler);

    let bound: Vec<&Symbol> = survivors(symbols, &candidates, |container| {
        impl_header_names_self_type(&container.raw_kind, &client_type)
    })
    .into_iter()
    .filter(|symbol| is_generated_out_path(&symbol.range.file))
    .collect();

    match bound.as_slice() {
        [one] => Binding::Bound(one.id.clone()),
        _ => Binding::Unbound(UnboundReason::GeneratedStubNotIndexed {
            join_key: operation.join_key.clone(),
        }),
    }
}

/// The candidates that survive every filter but the container predicate, which
/// the caller supplies.
fn survivors<'a>(
    symbols: &'a dyn SymbolIndex,
    candidates: &[&'a Symbol],
    container_matches: impl Fn(&Symbol) -> bool,
) -> Vec<&'a Symbol> {
    candidates
        .iter()
        .copied()
        .filter(|symbol| matches!(symbol.kind, SymbolKind::Method | SymbolKind::Function))
        .filter(|symbol| !is_test_context(symbol))
        .filter(|symbol| match &symbol.container {
            Some(container) => symbols.get(container).is_some_and(&container_matches),
            None => false,
        })
        .collect()
}
