#[path = "src/builder.rs"]
mod builder;

use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/builder.rs");
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
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let repo_cache = manifest.join("data");
    let cache = if repo_cache.exists() {
        repo_cache
    } else {
        let fallback = out.join("geonames-cache");
        std::fs::create_dir_all(&fallback).unwrap();
        fallback
    };
    builder::build(&cache, &bin).unwrap();
}
