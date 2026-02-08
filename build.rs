//! Build script for generating the geocoding database.
//!
//! This script runs at compile time to download and process geographic data from GeoNames,
//! creating a compact binary database that gets embedded into the compiled binary.
//!
//! # Build Process
//!
//! 1. Downloads administrative codes (admin1, admin2) from GeoNames
//! 2. Downloads place data for selected countries
//! 3. Downloads postal code data
//! 4. Merges postal codes with places
//! 5. Deduplicates entries
//! 6. Builds string interning table
//! 7. Creates spatial grid index
//! 8. Serializes to binary format
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
//! Generates `places.bin` in the cargo `OUT_DIR`, which is then embedded into the binary
//! using `include_bytes!` in the main crate.

use std::path::PathBuf;

#[path = "build/builder.rs"]
mod builder;
#[path = "build/types.rs"]
mod types;

use builder::Builder;

fn main() {
    // Skip database build if feature flag is set
    if cfg!(feature = "no-build-database") {
        return;
    }

    // Skip on docs.rs and clippy to avoid network requests
    if std::env::var("DOCS_RS").is_ok() || std::env::var("CLIPPY_ARGS").is_ok() {
        return;
    }

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let db_path = out_dir.join("places.bin");

    // Skip if database already exists
    if db_path.exists() {
        return;
    }

    Builder::new()
        .build(db_path.to_str().unwrap())
        .expect("Failed to build database");
}
