//! Fast reverse geocoding library with enriched location data.
//!
//! `genom` converts latitude/longitude coordinates into detailed place information including
//! city, region, country, timezone, currency, postal code, and more. It's designed for
//! high-performance applications that need sub-millisecond geocoding with rich metadata.
//!
//! # Features
//!
//! - **Simple API** - Single function call: [`lookup(lat, lon)`](lookup)
//! - **Rich Data** - Returns 18 fields including timezone, currency, postal code, region, EU status
//! - **Fast Lookups** - Grid-based spatial indexing for sub-millisecond queries
//! - **Zero Config** - Database builds automatically on first install from GeoNames data
//! - **Thread-Safe** - Global singleton with lazy initialization, safe for concurrent access
//! - **Compact** - Efficient binary format with string interning (~20-30 MB for 100+ countries)
//! - **Offline** - No external API calls after initial build, works completely offline
//!
//! # Quick Start
//!
//! Add `genom` to your `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! genom = "0.1"
//! ```
//!
//! Basic usage:
//!
//! ```no_run
//! # fn main() {
//! // Lookup coordinates
//! if let Some(place) = genom::lookup(40.7128, -74.0060) {
//!     println!("{}, {}", place.city, place.country_name);
//!     // Output: New York, United States
//! }
//! # }
//! ```
//!
//! # Detailed Example
//!
//! ```no_run
//! # fn main() {
//! use genom;
//!
//! // Look up coordinates for Paris, France
//! if let Some(place) = genom::lookup(48.8566, 2.3522) {
//!     // Location information
//!     println!("City: {}", place.city);                    // Paris
//!     println!("Region: {}", place.region);                // Île-de-France
//!     println!("Country: {}", place.country_name);         // France
//!     println!("Country Code: {}", place.country_code);    // FR
//!     
//!     // Geographic details
//!     println!("Continent: {}", place.continent_name);     // Europe
//!     println!("Postal Code: {}", place.postal_code);      // 75001
//!     println!("Coordinates: {}, {}", place.latitude, place.longitude);
//!     
//!     // Timezone information
//!     println!("Timezone: {}", place.timezone);            // Europe/Paris
//!     println!("TZ Abbr: {}", place.timezone_abbr);        // CET or CEST
//!     println!("UTC Offset: {}", place.utc_offset_str);    // UTC+1 or UTC+2
//!     println!("DST Active: {}", place.dst_active);        // true/false
//!     
//!     // Economic/political data
//!     println!("Currency: {}", place.currency);            // EUR
//!     println!("EU Member: {}", place.is_eu);              // true
//! }
//! # }
//! ```
//!
//! # Architecture
//!
//! ## Database Structure
//!
//! The library uses a pre-built binary database that's embedded in your compiled binary:
//!
//! - **String Interning**: Common strings (country codes, timezones) stored once
//! - **Fixed-Point Coordinates**: 32-bit integers instead of 64-bit floats
//! - **Spatial Grid Index**: World divided into 0.1° × 0.1° cells (~11km at equator)
//!
//! ## Lookup Algorithm
//!
//! 1. Quantize input coordinates to grid key (0.1° resolution)
//! 2. Search target cell and 8 neighboring cells (3×3 grid)
//! 3. Calculate haversine distance to all candidates
//! 4. Return nearest place with enriched metadata
//!
//! This provides O(1) average-case lookup with typically 10-50 candidates to check.
//!
//! ## Data Enrichment
//!
//! Raw place data is enriched with:
//! - Country names from ISO codes
//! - Currency codes by country
//! - Continent information
//! - EU membership status
//! - Current timezone offset and abbreviation
//! - DST (Daylight Saving Time) status
//!
//! # Performance
//!
//! - **First lookup**: ~100ms (database initialization and decompression)
//! - **Subsequent lookups**: <1ms (typically 0.1-0.5ms)
//! - **Memory usage**: ~30-50 MB (depending on number of countries)
//! - **Binary size increase**: ~20-30 MB (embedded database)
//!
//! The database is initialized lazily on first use and cached in a static `OnceLock`,
//! making it safe and efficient for concurrent access.
//!
//! # Build Process
//!
//! On first `cargo build`, the library:
//!
//! 1. Downloads geographic data from [GeoNames.org](https://www.geonames.org/)
//! 2. Processes and filters place data (cities, towns, villages)
//! 3. Merges postal code information
//! 4. Deduplicates nearby entries
//! 5. Builds string interning table and spatial index
//! 6. Serializes to compact binary format
//!
//! This happens automatically and takes 2-5 minutes depending on network speed.
//! The database is cached in `target/` and only rebuilt when necessary.
//!
//! ## Skipping the Build
//!
//! To skip database generation (e.g., for docs.rs or CI):
//!
//! ```toml
//! [dependencies]
//! genom = { version = "0.1", features = ["no-build-database"] }
//! ```
//!
//! # Thread Safety
//!
//! All operations are thread-safe:
//!
//! - Database initialization uses `OnceLock` for safe concurrent initialization
//! - All lookups are read-only after initialization
//! - No locks needed for queries (lock-free reads)
//! - Safe to call from multiple threads simultaneously
//!
//! ```no_run
//! use std::thread;
//! use genom;
//!
//! # fn main() {
//! let handles: Vec<_> = (0..10)
//!     .map(|i| {
//!         thread::spawn(move || {
//!             let lat = 40.0 + i as f64;
//!             let lon = -74.0;
//!             genom::lookup(lat, lon)
//!         })
//!     })
//!     .collect();
//!
//! for handle in handles {
//!     handle.join().unwrap();
//! }
//! # }
//! ```
//!
//! # Data Sources
//!
//! All geographic data comes from [GeoNames.org](https://www.geonames.org/),
//! which provides free geographic data under the
//! [Creative Commons Attribution 4.0 License](https://creativecommons.org/licenses/by/4.0/).
//!
//! The library includes data for 100+ countries with significant population and data quality.
//!
//! # Limitations
//!
//! - **Ocean coordinates**: Returns `None` for coordinates far from land
//! - **Precision**: Nearest city/town, not street-level accuracy
//! - **Coverage**: Limited to countries included in the build (see `build/builder.rs`)
//! - **Updates**: Database is static; requires rebuild for updated data
//! - **Size**: Adds ~20-30 MB to your binary
//!
//! # Use Cases
//!
//! - **Analytics**: Enrich user location data with timezone and region
//! - **Logging**: Add geographic context to log entries
//! - **APIs**: Convert coordinates to human-readable locations
//! - **IoT**: Offline geocoding for edge devices
//! - **Mobile**: Embedded geocoding without network requests
//! - **Privacy**: No external API calls, all data stays local
//!
//! # Modules
//!
//! - [`types`] - Core data structures ([`Place`], [`Location`], [`Database`])
//! - [`enrichment`] - Data enrichment functions and lookup tables
//!
//! # See Also
//!
//! - [`Geocoder`] - The core geocoding engine (usually accessed via [`lookup`])
//! - [`Place`] - The enriched output structure with all location data
//! - [`Location`] - Simple coordinate pair with distance calculations

#![warn(missing_docs)]

mod database;
pub mod enrichment;
pub mod types;

pub use database::Geocoder;
pub use types::{Location, Place};

/// Performs reverse geocoding on the given coordinates, returning enriched place data if found.
///
/// This is the primary entry point for all geocoding operations. It abstracts away
/// database access, spatial indexing, and data enrichment into a single call.
///
/// # What This Function Does
///
/// - Accesses the global geocoder singleton (lazy initialization on first call)
/// - Performs grid-based spatial lookup to find nearest place
/// - Enriches raw data with timezone, currency, and regional information
/// - Returns `None` if no place found within search radius
///
/// # Thread Safety
///
/// This function is thread-safe and can be called concurrently from multiple threads.
/// The underlying database is initialized once and shared via a static `OnceLock`.
///
/// # Performance
///
/// Typical lookup time: <1ms. First call incurs database initialization overhead
/// (~100ms to decompress and load). Subsequent calls are lock-free reads.
///
/// # Examples
///
/// ```no_run
/// # fn main() {
/// // Tokyo coordinates
/// if let Some(place) = genom::lookup(35.6762, 139.6503) {
///     println!("{}, {}", place.city, place.country_name);
///     println!("Timezone: {}", place.timezone);
///     println!("Currency: {}", place.currency);
/// }
///
/// // Ocean coordinates return None
/// assert!(genom::lookup(0.0, -160.0).is_none());
/// # }
/// ```
pub fn lookup(latitude: f64, longitude: f64) -> Option<Place> {
    Geocoder::global().lookup(latitude, longitude)
}
