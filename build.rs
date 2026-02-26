//! Build script for building the geocoding database.
//!
//! This script runs at compile time to build the places.bin database
//! from GeoNames data instead of downloading a pre-built version.
//!
//! # Skip Conditions
//!
//! The build is skipped when:
//! - `no-build-database` feature is enabled
//! - Building on docs.rs (`DOCS_RS` env var set)
//! - Running clippy (`CLIPPY_ARGS` env var set)
//! - Database file already exists in `OUT_DIR`
//!
//! # Output
//!
//! Builds `places.bin` to the cargo `OUT_DIR`, which is then embedded into the binary
//! using `include_bytes!` in the main crate.

#[path = "build/builder.rs"]
mod builder;

#[path = "build/types.rs"]
mod types;

use std::path::{Path, PathBuf};

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

    match build_database(&db_path) {
        Ok(_) => {
            eprintln!("Database built successfully");
            println!("cargo:rerun-if-changed=build.rs");
            println!("cargo:rerun-if-changed=build/builder.rs");
            println!("cargo:rerun-if-changed=build/types.rs");
        }
        Err(e) => {
            eprintln!("cargo:warning=Failed to build database: {}", e);
            std::process::exit(1);
        }
    }
}

fn build_database(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = builder::Builder::new();
    builder.build(&path.to_string_lossy())?;
    Ok(())
}
