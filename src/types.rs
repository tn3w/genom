//! Core data structures for geographic information.
//!
//! This module defines the fundamental types used throughout the library:
//!
//! - [`Place`] - Enriched output with complete geographic context
//! - [`Location`] - Simple coordinate pair with distance calculations
//! - [`CompactPlace`] - Compressed storage format using string table indices
//! - [`Database`] - Complete spatial database with grid index

#![warn(missing_docs)]

use serde::{Deserialize, Serialize};

/// The enriched output type containing complete geographic context for a location.
///
/// This struct is returned by [`lookup()`](crate::lookup) and contains 18 fields
/// providing comprehensive information about a geographic location.
///
/// # Examples
///
/// ```no_run
/// # fn main() {
/// let place = genom::lookup(40.7128, -74.0060).unwrap();
///
/// println!("City: {}", place.city);
/// println!("Country: {}", place.country_name);
/// println!("Timezone: {} ({})", place.timezone, place.timezone_abbr);
/// println!("Currency: {}", place.currency);
/// println!("EU Member: {}", place.is_eu);
/// # }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, bincode::Encode, bincode::Decode)]
pub struct Place {
    /// City or locality name (e.g., "New York", "Tokyo", "Paris")
    pub city: String,
    /// State, province, or administrative region full name (e.g., "California", "Tokyo", "Île-de-France")
    pub region: String,
    /// ISO 3166-2 region code (e.g., "CA" for California, "13" for Tokyo)
    pub region_code: String,
    /// County, district, or sub-region (e.g., "Los Angeles County", "Chiyoda")
    pub district: String,
    /// ISO 3166-1 alpha-2 country code (e.g., "US", "JP", "FR")
    pub country_code: String,
    /// Full country name (e.g., "United States", "Japan", "France")
    pub country_name: String,
    /// Postal or ZIP code (e.g., "10001", "100-0001", "75001")
    pub postal_code: String,
    /// IANA timezone identifier (e.g., "America/New_York", "Asia/Tokyo", "Europe/Paris")
    pub timezone: String,
    /// Current timezone abbreviation (e.g., "EST", "JST", "CET"). Changes based on DST.
    pub timezone_abbr: String,
    /// Current UTC offset in seconds (e.g., -18000 for UTC-5, 32400 for UTC+9)
    pub utc_offset: i32,
    /// Formatted UTC offset string (e.g., "UTC-5", "UTC+9", "UTC+5:30")
    pub utc_offset_str: String,
    /// Precise latitude coordinate in decimal degrees (-90 to 90)
    pub latitude: f64,
    /// Precise longitude coordinate in decimal degrees (-180 to 180)
    pub longitude: f64,
    /// ISO 4217 currency code (e.g., "USD", "JPY", "EUR")
    pub currency: String,
    /// Two-letter continent code (e.g., "NA" for North America, "AS" for Asia, "EU" for Europe)
    pub continent_code: String,
    /// Full continent name (e.g., "North America", "Asia", "Europe")
    pub continent_name: String,
    /// Whether the location is in a European Union member state
    pub is_eu: bool,
    /// Whether daylight saving time is currently active for this location
    pub dst_active: bool,
}

/// A coordinate pair with distance calculation capabilities.
///
/// This is a simple wrapper around latitude and longitude coordinates that provides
/// utility methods for geographic calculations.
#[derive(Debug, Clone, Copy)]
pub struct Location {
    /// Latitude in decimal degrees (-90 to 90)
    pub latitude: f64,
    /// Longitude in decimal degrees (-180 to 180)
    pub longitude: f64,
}

impl Location {
    /// Constructs a new Location from coordinates.
    ///
    /// # Examples
    ///
    /// ```
    /// use genom::Location;
    ///
    /// let loc = Location::new(40.7128, -74.0060);
    /// assert_eq!(loc.latitude, 40.7128);
    /// assert_eq!(loc.longitude, -74.0060);
    /// ```
    pub fn new(latitude: f64, longitude: f64) -> Self {
        Self {
            latitude,
            longitude,
        }
    }

    /// Calculates the great-circle distance to another location using the haversine formula.
    ///
    /// Returns the distance in kilometers. This calculation assumes a spherical Earth
    /// with radius 6371 km, which provides accuracy within 0.5% for most distances.
    ///
    /// # Examples
    ///
    /// ```
    /// use genom::Location;
    ///
    /// let nyc = Location::new(40.7128, -74.0060);
    /// let la = Location::new(34.0522, -118.2437);
    ///
    /// let distance = nyc.distance_to(&la);
    /// assert!(distance > 3900.0 && distance < 4000.0); // ~3944 km
    /// ```
    pub fn distance_to(&self, other: &Location) -> f64 {
        let lat1 = self.latitude.to_radians();
        let lat2 = other.latitude.to_radians();
        let delta_lat = (other.latitude - self.latitude).to_radians();
        let delta_lon = (other.longitude - self.longitude).to_radians();

        let a = (delta_lat / 2.0).sin().powi(2)
            + lat1.cos() * lat2.cos() * (delta_lon / 2.0).sin().powi(2);
        let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());

        6371.0 * c
    }
}

/// Compressed storage format using string table indices and fixed-point coordinates.
///
/// This is the internal storage representation used in the database. All string fields
/// are stored as `u32` indices into a shared string table, and coordinates
/// are stored as `i32` fixed-point values (multiplied by 100,000).
///
/// This reduces memory footprint by approximately 70% compared to storing full
/// [`Place`] structs.
#[derive(Serialize, Deserialize, bincode::Encode, bincode::Decode)]
pub struct CompactPlace {
    /// Index into the string table for the city name
    pub city: u32,
    /// Index into the string table for the region name
    pub region: u32,
    /// Index into the string table for the region code
    pub region_code: u32,
    /// Index into the string table for the district name
    pub district: u32,
    /// Index into the string table for the country code
    pub country_code: u32,
    /// Index into the string table for the postal code
    pub postal_code: u32,
    /// Index into the string table for the timezone identifier
    pub timezone: u32,
    /// Latitude as fixed-point integer (multiply by 100,000 to get decimal degrees)
    pub lat: i32,
    /// Longitude as fixed-point integer (multiply by 100,000 to get decimal degrees)
    pub lon: i32,
}

impl CompactPlace {
    /// Converts the fixed-point coordinates to a [`Location`].
    ///
    /// Divides the integer coordinates by 100,000 to recover the original decimal degree values.
    pub fn location(&self) -> Location {
        Location {
            latitude: self.lat as f64 / 100000.0,
            longitude: self.lon as f64 / 100000.0,
        }
    }
}

/// The complete spatial database structure with string interning and grid index.
///
/// This struct contains all the data needed for geocoding operations. It uses
/// string interning to deduplicate common strings and a spatial grid index for
/// fast coordinate lookups.
///
/// # Spatial Indexing Strategy
///
/// The grid divides the world into 0.1° × 0.1° cells. For a lookup:
///
/// 1. Quantize the input coordinates to a grid key: `(lat * 100000 / 10000, lon * 100000 / 10000)`
/// 2. Search the target cell and 8 neighboring cells (3×3 grid)
/// 3. Calculate haversine distance to all candidates in these cells
/// 4. Return the nearest place
///
/// This provides O(1) average-case lookup with a small constant factor (typically 10-50 candidates to check).
#[derive(Serialize, Deserialize, bincode::Encode, bincode::Decode)]
pub struct Database {
    /// Deduplicated string table. All string fields in [`CompactPlace`]
    /// are stored as indices into this vector. Common strings like country codes and
    /// timezone names are stored only once.
    pub strings: Vec<String>,
    /// All geographic entries in compressed format. Each entry contains indices into
    /// the string table and fixed-point coordinates.
    pub places: Vec<CompactPlace>,
    /// Spatial index mapping grid cells to place indices. The world is divided into
    /// 0.1° × 0.1° cells (~11km at equator). Each cell contains a vector of indices
    /// into the `places` vector.
    ///
    /// Uses `FxHashMap` (from `rustc-hash`) for faster hashing
    /// of integer keys compared to the standard library's `HashMap`.
    pub grid: rustc_hash::FxHashMap<(i16, i16), Vec<u32>>,
}
