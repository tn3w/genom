use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Copy)]
pub struct Location {
    pub latitude: f64,
    pub longitude: f64,
}

impl Location {
    pub fn new(latitude: f64, longitude: f64) -> Self {
        Self {
            latitude,
            longitude,
        }
    }

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

#[derive(Serialize, Deserialize, bincode::Encode, bincode::Decode)]
pub struct CompactPlace {
    pub city: u32,
    pub region: u32,
    pub region_code: u32,
    pub district: u32,
    pub country_code: u32,
    pub postal_code: u32,
    pub timezone: u32,
    pub lat: i32,
    pub lon: i32,
}

impl CompactPlace {
    pub fn location(&self) -> Location {
        Location {
            latitude: self.lat as f64 / 100000.0,
            longitude: self.lon as f64 / 100000.0,
        }
    }
}

#[derive(Serialize, Deserialize, bincode::Encode, bincode::Decode)]
pub struct Database {
    pub strings: Vec<String>,
    pub places: Vec<CompactPlace>,
    pub grid: rustc_hash::FxHashMap<(i16, i16), Vec<u32>>,
}
