mod database;
mod enrichment;
mod types;

pub use database::Geocoder;
pub use types::{Location, Place};

pub fn lookup(latitude: f64, longitude: f64) -> Option<Place> {
    Geocoder::global().lookup(latitude, longitude)
}
