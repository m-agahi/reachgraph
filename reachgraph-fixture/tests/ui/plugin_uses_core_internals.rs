// A plugin crate reaching into the waist must not compile.
//
// Plan-00 §1: dependencies point toward `plugin-api`, and no plugin crate may
// depend on `reachgraph-core`. A plugin that CANNOT write this line cannot
// couple itself to how the graph happens to be built today, so the compiler
// enforces what review would otherwise have to enforce by vigilance.
use reachgraph_core::Whatever;

fn main() {
    let _ = Whatever;
}
