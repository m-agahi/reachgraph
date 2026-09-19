//! Writes a trait into `OUT_DIR`, the way `tonic-build` writes a service
//! trait. MEASURED, `/home/max/git/yadgarhq/task`: the handler's trait arrives
//! through `use crate::pb::…::task_service_server::TaskService;`, and `pb` is
//! an `include!` of build-script output.

use std::io::Write;

fn main() {
    let out_dir = std::env::var("OUT_DIR").expect("cargo sets OUT_DIR");
    let path = std::path::Path::new(&out_dir).join("service.rs");
    let mut file = std::fs::File::create(path).expect("the out dir is writable");
    writeln!(file, "/// A trait reachgraph never loads.").expect("write");
    writeln!(file, "pub trait Generated {{").expect("write");
    writeln!(file, "    /// The operation.").expect("write");
    writeln!(file, "    fn generated(&self) -> u32;").expect("write");
    writeln!(file, "}}").expect("write");
    println!("cargo:rerun-if-changed=build.rs");
}
