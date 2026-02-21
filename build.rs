//! Build script for downloading the geocoding database.
//!
//! This script runs at compile time to download the pre-built places.bin database
//! from GitHub releases instead of building it locally.
//!
//! # Skip Conditions
//!
//! The download is skipped when:
//! - `no-build-database` feature is enabled
//! - Building on docs.rs (`DOCS_RS` env var set)
//! - Running clippy (`CLIPPY_ARGS` env var set)
//! - Database file already exists in `OUT_DIR`
//!
//! # Output
//!
//! Downloads `places.bin` to the cargo `OUT_DIR`, which is then embedded into the binary
//! using `include_bytes!` in the main crate.

use std::path::PathBuf;

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

    eprintln!("Downloading places.bin from GitHub releases...");

    let url = "https://github.com/tn3w/genom/releases/latest/download/places.bin";

    match download_database(&url, &db_path) {
        Ok(_) => {
            eprintln!("Database downloaded successfully");
            println!("cargo:rerun-if-changed=build.rs");
        }
        Err(e) => {
            eprintln!("cargo:warning=Failed to download database: {}", e);
            eprintln!("cargo:warning=Please run: cargo run --release --bin build-database --features builder,no-build-database && ./target/release/build-database {}", db_path.display());
            std::process::exit(1);
        }
    }
}

fn download_database(url: &str, path: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let response = reqwest::blocking::get(url)?;

    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()).into());
    }

    let bytes = response.bytes()?;
    std::fs::write(path, bytes)?;

    Ok(())
}
