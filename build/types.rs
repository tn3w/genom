//! Type definitions used during database construction.
//!
//! This module contains duplicate definitions of the main crate's types,
//! used during the build process. These are kept separate to avoid circular
//! dependencies between the build script and the main crate.
//!
//! The types here mirror those in `src/types.rs` but are only used during
//! the database build phase.

use serde::{Deserialize, Serialize};

/// Enriched place data structure (build-time version).
///
/// This mirrors the `Place` struct in the main crate but is used only
/// during database construction. The `#[allow(dead_code)]` attribute
/// is used because not all fields are accessed during the build process.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, bincode::Encode, bincode::Decode)]
pub struct Place {
    pub city: String,
    pub region: String,
    pub region_code: String,
    pub district: String,
    pub country_code: String,
    pub country_name: String,
    pub postal_code: String,
    pub timezone: String,
    pub timezone_abbr: String,
    pub utc_offset: i32,
    pub utc_offset_str: String,
    pub latitude: f64,
    pub longitude: f64,
    pub currency: String,
    pub continent_code: String,
    pub continent_name: String,
    pub is_eu: bool,
    pub dst_active: bool,
}

/// Coordinate pair with distance calculations (build-time version).
///
/// Mirrors the `Location` struct from the main crate.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub struct Location {
    pub latitude: f64,
    pub longitude: f64,
}

impl Location {
    /// Creates a new Location from coordinates.
    #[allow(dead_code)]
    pub fn new(latitude: f64, longitude: f64) -> Self {
        Self {
            latitude,
            longitude,
        }
    }

    /// Calculates great-circle distance using haversine formula.
    ///
    /// Returns distance in kilometers.
    #[allow(dead_code)]
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

/// Compact place representation using string table indices (build-time version).
///
/// This is the serialized format stored in the binary database.
/// All string fields are replaced with u32 indices into a shared string table,
/// and coordinates are stored as fixed-point i32 values.
#[derive(Serialize, Deserialize, bincode::Encode, bincode::Decode)]
pub(crate) struct CompactPlace {
    /// Index into string table for city name
    pub city: u32,
    /// Index into string table for region name
    pub region: u32,
    /// Index into string table for region code
    pub region_code: u32,
    /// Index into string table for district name
    pub district: u32,
    /// Index into string table for country code
    pub country_code: u32,
    /// Index into string table for postal code
    pub postal_code: u32,
    /// Index into string table for timezone
    pub timezone: u32,
    /// Latitude as fixed-point integer (degrees * 100,000)
    pub lat: i32,
    /// Longitude as fixed-point integer (degrees * 100,000)
    pub lon: i32,
}

impl CompactPlace {
    /// Converts fixed-point coordinates back to a Location.
    #[allow(dead_code)]
    pub fn location(&self) -> Location {
        Location {
            latitude: self.lat as f64 / 100000.0,
            longitude: self.lon as f64 / 100000.0,
        }
    }
}

/// Complete database structure with string table and spatial index (build-time version).
///
/// This is the top-level structure that gets serialized to the binary database file.
#[allow(dead_code)]
#[derive(Serialize, Deserialize, bincode::Encode, bincode::Decode)]
pub(crate) struct Database {
    /// Deduplicated string table
    pub strings: Vec<String>,
    /// All places in compact format
    pub places: Vec<CompactPlace>,
    /// Spatial grid index: (lat_key, lon_key) -> [place_indices]
    pub grid: rustc_hash::FxHashMap<(i16, i16), Vec<u32>>,
}
