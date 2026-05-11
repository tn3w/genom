//! Fast reverse geocoding with enriched location data.
//!
//! Converts `(latitude, longitude)` into a [`Place`] with 18 fields including
//! city, region, country, postal code, timezone, currency, EU/DST status.
//!
//! ```no_run
//! if let Some(place) = genom::lookup(40.7128, -74.0060) {
//!     println!("{}, {}", place.city, place.country_name);
//! }
//! ```
//!
//! # How it works
//!
//! - Database is built once at compile time from GeoNames + Natural Earth and
//!   embedded as a compact binary blob (`geo.bin`).
//! - On first call, the blob is parsed lazily into zero-copy `&'static` slices
//!   plus two `FxHashMap` indexes (grid → city offset, country → postal section).
//! - Lookup expands a grid-cell ring until the nearest city is found, refines
//!   with the nearest postal entry for that country, then enriches with
//!   country/currency/continent/timezone metadata.
//!
//! See [`Geocoder`], [`Place`], [`Location`], [`enrichment`].

#![warn(missing_docs)]

mod database;
pub mod enrichment;
mod types;

pub use database::Geocoder;
pub use types::{Location, Place};

/// Reverse-geocode `(latitude, longitude)` using the global [`Geocoder`].
///
/// Returns `None` for coordinates with no nearby city (e.g. open ocean).
pub fn lookup(latitude: f64, longitude: f64) -> Option<Place> {
    Geocoder::global().lookup(latitude, longitude)
}
