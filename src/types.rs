//! Core data structures returned to library users.

#![warn(missing_docs)]

/// Enriched location data returned by [`lookup`](crate::lookup).
#[derive(Debug, Clone)]
pub struct Place {
    /// City or locality name
    pub city: String,
    /// Region / state / province full name
    pub region: String,
    /// ISO 3166-2 region code
    pub region_code: String,
    /// District / county
    pub district: String,
    /// ISO 3166-1 alpha-2 country code
    pub country_code: String,
    /// Full country name
    pub country_name: String,
    /// Postal / ZIP code
    pub postal_code: String,
    /// IANA timezone id
    pub timezone: String,
    /// Current timezone abbreviation
    pub timezone_abbr: String,
    /// Current UTC offset in seconds
    pub utc_offset: i32,
    /// Formatted UTC offset (e.g. `UTC+1`)
    pub utc_offset_str: String,
    /// Latitude in decimal degrees
    pub latitude: f64,
    /// Longitude in decimal degrees
    pub longitude: f64,
    /// ISO 4217 currency code
    pub currency: String,
    /// Two-letter continent code
    pub continent_code: String,
    /// Full continent name
    pub continent_name: String,
    /// EU member state
    pub is_eu: bool,
    /// DST currently active
    pub dst_active: bool,
}

/// A `(latitude, longitude)` pair with distance calculation.
#[derive(Debug, Clone, Copy)]
pub struct Location {
    /// Latitude in decimal degrees
    pub latitude: f64,
    /// Longitude in decimal degrees
    pub longitude: f64,
}

impl Location {
    /// Construct a new `Location`.
    pub fn new(latitude: f64, longitude: f64) -> Self {
        Self {
            latitude,
            longitude,
        }
    }

    /// Great-circle distance in kilometers (haversine, sphere r=6371).
    pub fn distance_to(&self, other: &Location) -> f64 {
        let lat1 = self.latitude.to_radians();
        let lat2 = other.latitude.to_radians();
        let delta_lat = (other.latitude - self.latitude).to_radians();
        let delta_lon = (other.longitude - self.longitude).to_radians();
        let a = (delta_lat / 2.0).sin().powi(2)
            + lat1.cos() * lat2.cos() * (delta_lon / 2.0).sin().powi(2);
        6371.0 * 2.0 * a.sqrt().atan2((1.0 - a).sqrt())
    }
}
