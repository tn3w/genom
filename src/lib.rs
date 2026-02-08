pub mod enrichment;
pub mod types;

#[cfg(not(feature = "wasm"))]
mod database;

#[cfg(not(feature = "wasm"))]
pub use database::Geocoder;

pub use types::{Location, Place};

pub fn lookup(latitude: f64, longitude: f64) -> Option<Place> {
    #[cfg(not(feature = "wasm"))]
    {
        Geocoder::global().lookup(latitude, longitude)
    }
    #[cfg(feature = "wasm")]
    {
        wasm::WasmGeocoder::lookup(latitude, longitude)
    }
}

#[cfg(feature = "wasm")]
pub mod wasm {
    use crate::enrichment::{enrich_place, PlaceInput};
    use crate::types::{Database, Location, Place};
    use std::sync::OnceLock;

    static GEOCODER: OnceLock<WasmGeocoder> = OnceLock::new();

    pub struct WasmGeocoder {
        db: Database,
    }

    impl WasmGeocoder {
        pub fn init(data: &[u8]) -> Result<(), String> {
            let (db, _): (Database, _) =
                bincode::decode_from_slice(data, bincode::config::standard())
                    .map_err(|e| format!("Decode failed: {}", e))?;

            GEOCODER
                .set(Self { db })
                .map_err(|_| "Already initialized".to_string())?;

            Ok(())
        }

        pub fn lookup(latitude: f64, longitude: f64) -> Option<Place> {
            let geocoder = GEOCODER.get()?;
            let location = Location::new(latitude, longitude);
            let grid_key = geocoder.grid_key(&location);
            let idx = geocoder.find_nearest(&location, grid_key)?;
            Some(geocoder.build_place(idx))
        }

        fn grid_key(&self, location: &Location) -> (i16, i16) {
            (
                ((location.latitude * 100000.0) as i32 / 10000) as i16,
                ((location.longitude * 100000.0) as i32 / 10000) as i16,
            )
        }

        fn find_nearest(&self, location: &Location, grid_key: (i16, i16)) -> Option<usize> {
            (-1..=1)
                .flat_map(|dlat| {
                    (-1..=1).filter_map(move |dlon| {
                        self.db.grid.get(&(grid_key.0 + dlat, grid_key.1 + dlon))
                    })
                })
                .flatten()
                .map(|&idx| {
                    let place = &self.db.places[idx as usize];
                    (idx as usize, location.distance_to(&place.location()))
                })
                .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
                .map(|(idx, _)| idx)
        }

        fn build_place(&self, idx: usize) -> Place {
            let place = &self.db.places[idx];
            enrich_place(PlaceInput {
                city: &self.db.strings[place.city as usize],
                region: &self.db.strings[place.region as usize],
                region_code: &self.db.strings[place.region_code as usize],
                district: &self.db.strings[place.district as usize],
                country_code: &self.db.strings[place.country_code as usize],
                postal_code: &self.db.strings[place.postal_code as usize],
                timezone: &self.db.strings[place.timezone as usize],
                latitude: place.lat as f64 / 100000.0,
                longitude: place.lon as f64 / 100000.0,
            })
        }
    }
}
