//! Writes a module into `OUT_DIR`, the way a tonic build script writes a
//! client stub. MEASURED, design.md §4: the cross-repo leaf resolved only
//! because `target/debug/build/…/out/` already existed.

use std::io::Write;

fn main() {
    let out_dir = std::env::var("OUT_DIR").expect("cargo sets OUT_DIR");
    let path = std::path::Path::new(&out_dir).join("generated.rs");
    let mut file = std::fs::File::create(path).expect("the out dir is writable");
    writeln!(file, "/// The generated leaf.").expect("write");
    writeln!(file, "pub fn generated_leaf() -> u32 {{ 11 }}").expect("write");
    println!("cargo:rerun-if-changed=build.rs");
}
