use std::path::PathBuf;

#[path = "build/builder.rs"]
mod builder;
#[path = "build/types.rs"]
mod types;

use builder::Builder;

fn main() {
    let out_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let data_dir = out_dir.join("data");
    let db_path = data_dir.join("places.bin");

    if db_path.exists() {
        return;
    }

    std::fs::create_dir_all(&data_dir).expect("Failed to create data directory");

    Builder::new()
        .build(db_path.to_str().unwrap())
        .expect("Failed to build database");
}
