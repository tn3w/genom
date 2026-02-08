use std::path::PathBuf;

#[path = "build/builder.rs"]
mod builder;
#[path = "build/types.rs"]
mod types;

use builder::Builder;

fn main() {
    if cfg!(feature = "no-build-database") {
        return;
    }

    if std::env::var("DOCS_RS").is_ok() || std::env::var("CLIPPY_ARGS").is_ok() {
        return;
    }

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let db_path = out_dir.join("places.bin");

    if db_path.exists() {
        return;
    }

    Builder::new()
        .build(db_path.to_str().unwrap())
        .expect("Failed to build database");
}
