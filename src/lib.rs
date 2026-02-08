pub mod enrichment;
pub mod types;
mod database;

pub use database::Geocoder;
pub use types::{Location, Place};

pub fn lookup(latitude: f64, longitude: f64) -> Option<Place> {
    Geocoder::global().lookup(latitude, longitude)
}
