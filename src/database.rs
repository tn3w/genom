use crate::enrichment::{enrich_place, PlaceInput};
use crate::types::{Database, Location, Place};
use std::sync::OnceLock;

static GEOCODER: OnceLock<Geocoder> = OnceLock::new();

#[cfg(not(any(doc, clippy)))]
static DATA: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/places.bin"));

#[cfg(any(doc, clippy))]
static DATA: &[u8] = &[];

pub struct Geocoder {
    db: Database,
}

impl Geocoder {
    pub fn global() -> &'static Self {
        GEOCODER.get_or_init(|| Self::new().expect("Failed to initialize geocoder"))
    }

    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let (db, _): (Database, _) = bincode::decode_from_slice(DATA, bincode::config::standard())?;
        Ok(Self { db })
    }

    pub fn lookup(&self, latitude: f64, longitude: f64) -> Option<Place> {
        let location = Location::new(latitude, longitude);
        let grid_key = self.grid_key(&location);
        let idx = self.find_nearest(&location, grid_key)?;
        Some(self.build_place(idx))
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
