use crate::enrichment::enrich_place;
use crate::types::{Database, Location, Place};
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

static GEOCODER: OnceLock<Geocoder> = OnceLock::new();

pub struct Geocoder {
    db: Database,
}

impl Geocoder {
    pub fn global() -> &'static Self {
        GEOCODER.get_or_init(|| Self::new().expect("Failed to initialize geocoder"))
    }

    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let path = Self::data_path();
        let bytes = fs::read(&path)?;
        let (db, _): (Database, _) =
            bincode::decode_from_slice(&bytes, bincode::config::standard())?;
        Ok(Self { db })
    }

    fn data_path() -> PathBuf {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        PathBuf::from(manifest_dir).join("data").join("places.bin")
    }

    pub fn lookup(&self, latitude: f64, longitude: f64) -> Option<Place> {
        self.search(Location::new(latitude, longitude))
    }

    fn search(&self, location: Location) -> Option<Place> {
        let grid_key = (
            ((location.latitude * 100000.0) as i32 / 10000) as i16,
            ((location.longitude * 100000.0) as i32 / 10000) as i16,
        );

        let (idx, _) = (-1..=1)
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
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())?;

        let place = &self.db.places[idx];
        Some(enrich_place(
            &self.db.strings[place.city as usize],
            &self.db.strings[place.region as usize],
            &self.db.strings[place.region_code as usize],
            &self.db.strings[place.district as usize],
            &self.db.strings[place.country_code as usize],
            &self.db.strings[place.postal_code as usize],
            &self.db.strings[place.timezone as usize],
            place.lat as f64 / 100000.0,
            place.lon as f64 / 100000.0,
        ))
    }
}
