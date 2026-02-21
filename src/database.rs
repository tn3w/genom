//! Database management and geocoding engine.
//!
//! This module contains the core [`Geocoder`] struct that manages the spatial database
//! and performs coordinate lookups.

#![warn(missing_docs)]

use crate::enrichment::{enrich_place, PlaceInput};
use crate::types::{Database, Location, Place};
use std::sync::OnceLock;

static GEOCODER: OnceLock<Geocoder> = OnceLock::new();

#[cfg(not(any(doc, clippy, feature = "no-build-database")))]
static DATA: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/places.bin"));

#[cfg(any(doc, clippy, feature = "no-build-database"))]
static DATA: &[u8] = &[];

/// The core geocoding engine. Manages the spatial database and performs coordinate lookups.
///
/// # Conceptual Role
///
/// `Geocoder` is the transport layer for all geographic queries. It handles:
///
/// - Database initialization and decompression
/// - Grid-based spatial indexing for O(1) lookups
/// - Nearest-neighbor search across grid cells
/// - String table resolution for compact storage
///
/// # What This Type Does NOT Do
///
/// - Data enrichment (handled by [`enrichment`](crate::enrichment) module)
/// - Distance calculations (delegated to [`Location`] type)
/// - Thread synchronization (uses `OnceLock` for initialization)
///
/// # Invariants
///
/// - After construction, the database is fully loaded and valid
/// - Grid keys are consistent with coordinate quantization
/// - String indices in [`CompactPlace`](crate::types::CompactPlace) are valid into strings vector
///
/// # Thread Safety
///
/// `Geocoder` is `Send` but not `Sync`. However, the global instance
/// accessed via [`Geocoder::global()`] is safe to use from multiple threads
/// because all operations are read-only after initialization.
pub struct Geocoder {
    db: Database,
}

impl Geocoder {
    /// Returns a reference to the global geocoder singleton.
    ///
    /// # Initialization
    ///
    /// First call initializes the database by decompressing the embedded binary data.
    /// Subsequent calls return the cached instance. Initialization is thread-safe via `OnceLock`.
    ///
    /// # Panics
    ///
    /// Panics if database initialization fails (corrupted data, out of memory).
    /// This is intentional - the library cannot function without a valid database.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn main() {
    /// use genom::Geocoder;
    ///
    /// let geocoder = Geocoder::global();
    /// let place = geocoder.lookup(51.5074, -0.1278);
    /// # }
    /// ```
    pub fn global() -> &'static Self {
        GEOCODER.get_or_init(|| Self::new().expect("Failed to initialize geocoder"))
    }

    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let db = Self::load_database(DATA)?;
        Ok(Self { db })
    }

    fn load_database(data: &[u8]) -> Result<Database, Box<dyn std::error::Error>> {
        let mut cursor = std::io::Cursor::new(data);
        use std::io::Read;

        let mut buf8 = [0u8; 8];
        let mut buf4 = [0u8; 4];
        let mut buf2 = [0u8; 2];

        cursor.read_exact(&mut buf8)?;
        let str_count = u64::from_le_bytes(buf8) as usize;
        let mut strings = Vec::with_capacity(str_count);
        for _ in 0..str_count {
            let str_len = Self::read_varint(&mut cursor)? as usize;
            let mut str_buf = vec![0u8; str_len];
            cursor.read_exact(&mut str_buf)?;
            strings.push(String::from_utf8(str_buf)?);
        }

        cursor.read_exact(&mut buf8)?;
        let place_count = u64::from_le_bytes(buf8) as usize;
        let mut places = Vec::with_capacity(place_count);
        for _ in 0..place_count {
            cursor.read_exact(&mut buf4)?;
            let city = u32::from_le_bytes(buf4);
            cursor.read_exact(&mut buf4)?;
            let region = u32::from_le_bytes(buf4);
            cursor.read_exact(&mut buf4)?;
            let region_code = u32::from_le_bytes(buf4);
            cursor.read_exact(&mut buf4)?;
            let district = u32::from_le_bytes(buf4);
            cursor.read_exact(&mut buf4)?;
            let country_code = u32::from_le_bytes(buf4);
            cursor.read_exact(&mut buf4)?;
            let postal_code = u32::from_le_bytes(buf4);
            cursor.read_exact(&mut buf4)?;
            let timezone = u32::from_le_bytes(buf4);
            cursor.read_exact(&mut buf4)?;
            let lat = i32::from_le_bytes(buf4);
            cursor.read_exact(&mut buf4)?;
            let lon = i32::from_le_bytes(buf4);
            places.push(crate::types::CompactPlace {
                city,
                region,
                region_code,
                district,
                country_code,
                postal_code,
                timezone,
                lat,
                lon,
            });
        }

        cursor.read_exact(&mut buf8)?;
        let grid_count = u64::from_le_bytes(buf8) as usize;
        let mut grid = rustc_hash::FxHashMap::default();
        for _ in 0..grid_count {
            cursor.read_exact(&mut buf2)?;
            let key_lat = i16::from_le_bytes(buf2);
            cursor.read_exact(&mut buf2)?;
            let key_lon = i16::from_le_bytes(buf2);
            cursor.read_exact(&mut buf8)?;
            let vec_len = u64::from_le_bytes(buf8) as usize;
            let mut indices = Vec::with_capacity(vec_len);
            for _ in 0..vec_len {
                cursor.read_exact(&mut buf4)?;
                indices.push(u32::from_le_bytes(buf4));
            }
            grid.insert((key_lat, key_lon), indices);
        }

        Ok(Database {
            strings,
            places,
            grid,
        })
    }

    fn read_varint(cursor: &mut std::io::Cursor<&[u8]>) -> Result<u64, Box<dyn std::error::Error>> {
        use std::io::Read;
        let mut result = 0u64;
        let mut shift = 0;
        loop {
            let mut byte = [0u8; 1];
            cursor.read_exact(&mut byte)?;
            result |= ((byte[0] & 0x7F) as u64) << shift;
            if (byte[0] & 0x80) == 0 {
                break;
            }
            shift += 7;
        }
        Ok(result)
    }

    /// Finds the nearest place to the given coordinates.
    ///
    /// # Algorithm
    ///
    /// 1. Quantize coordinates to grid key (0.1° resolution)
    /// 2. Search target cell and 8 neighboring cells
    /// 3. Calculate haversine distance to all candidates
    /// 4. Return nearest place, enriched with metadata
    ///
    /// # Returns
    ///
    /// `Some(Place)` if a location is found within search radius, `None` otherwise.
    /// Ocean coordinates typically return `None` unless near coastal cities.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn main() {
    /// use genom::Geocoder;
    ///
    /// let geocoder = Geocoder::global();
    ///
    /// // Paris, France
    /// let place = geocoder.lookup(48.8566, 2.3522).unwrap();
    /// assert_eq!(place.city, "Paris");
    /// assert_eq!(place.country_code, "FR");
    /// # }
    /// ```
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
