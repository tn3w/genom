#[path = "src/builder.rs"]
mod builder;

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=src/builder.rs");
    println!("cargo:rerun-if-changed=data");
    println!("cargo:rerun-if-env-changed=DOCS_RS");

    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let database = out.join("geo.bin");
    let skip = env::var_os("CARGO_FEATURE_NO_BUILD_DATABASE").is_some()
        || env::var_os("DOCS_RS").is_some();
    if skip {
        fs::write(&database, []).unwrap();
        return;
    }
    let data = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("data");
    let sources = if data.exists() {
        data
    } else {
        out.join("sources")
    };
    builder::build(&sources, &database).unwrap();
}
