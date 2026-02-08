//! Fast reverse geocoding library with enriched location data.
//!
//! Convert coordinates to detailed place information including timezone, currency, region, and more.
//!
//! # Features
//!
//! - **Simple API** - Single function call: [`lookup(lat, lon)`](lookup)
//! - **Rich Data** - Returns 16+ fields including timezone, currency, postal code, region
//! - **Fast Lookups** - Grid-based spatial indexing for sub-millisecond queries
//! - **Zero Config** - Database builds automatically on first install
//! - **Thread-Safe** - Global singleton with lazy initialization
//! - **Compact** - Efficient binary format with string interning
//!
//! # Quick Start
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
