#[path = "build/builder.rs"]
mod builder;

use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=build/builder.rs");
    if cfg!(feature = "no-build-database") {
        return;
    }
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let bin = out.join("geo.bin");
    if std::env::var("DOCS_RS").is_ok() || std::env::var("CLIPPY_ARGS").is_ok() {
        std::fs::write(&bin, []).ok();
        return;
    }
    if bin.exists() {
        return;
    }
    let cache = out.join("geonames-cache");
    std::fs::create_dir_all(&cache).unwrap();
    builder::build(&cache, &bin).unwrap();
}
