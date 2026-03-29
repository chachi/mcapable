use std::path::PathBuf;

fn main() {
    let bridges = ["src/lib.rs"];
    for path in &bridges {
        println!("cargo:rerun-if-changed={}", path);
    }

    let manifest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("missing manifest"));
    let out_dir = manifest_dir.join("Generated");
    swift_bridge_build::parse_bridges(bridges).write_all_concatenated(out_dir, "McapableSwift");
}
