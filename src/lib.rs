//! Fast offline reverse geocoding with enriched location data.
//!
//! ```no_run
//! if let Some(place) = genom::lookup(40.7128, -74.0060) {
//!     println!("{}, {}", place.city, place.country.name);
//! }
//! ```

use std::borrow::Cow;
use std::cmp::Ordering;
use std::path::Path;
use std::sync::OnceLock;
use std::{fs, io};

use chrono::{Offset, Utc};
use chrono_tz::{OffsetComponents, OffsetName, Tz};

const MAGIC: &[u8; 4] = b"GEO2";
const HEADER_SIZE: usize = 68;
const MAX_FIELDS: usize = 12;
const EARTH_RADIUS_KM: f64 = 6371.0;

static DATABASE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/geo.bin"));

struct Layer {
    header: usize,
    max_degrees: f64,
}

const CITIES: Layer = Layer {
    header: 7,
    max_degrees: 10.0,
};
const POSTAL: Layer = Layer {
    header: 12,
    max_degrees: 0.3,
};

/// A reverse-geocoded location: the nearest city with its surroundings.
#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    pub city: String,
    pub city_ascii: String,
    pub geoname_id: u32,
    pub feature_code: String,
    pub population: u32,
    pub elevation: Option<i32>,
    pub latitude: f64,
    pub longitude: f64,
    pub distance_km: f64,
    pub region: String,
    pub region_code: String,
    pub district: String,
    pub district_code: String,
    pub postal_code: String,
    pub locality: String,
    pub municipality: String,
    pub timezone: Timezone,
    pub country: Country,
}

/// Current state of a place's IANA timezone.
#[derive(Debug, Clone, PartialEq)]
pub struct Timezone {
    pub name: String,
    pub abbreviation: String,
    pub utc_offset: i32,
    pub utc_offset_str: String,
    pub dst_active: bool,
}

/// Country facts from GeoNames.
#[derive(Debug, Clone, PartialEq)]
pub struct Country {
    pub code: String,
    pub iso3: String,
    pub iso_numeric: String,
    pub name: String,
    pub capital: String,
    pub area_km2: f64,
    pub population: u64,
    pub continent_code: String,
    pub continent_name: String,
    pub tld: String,
    pub currency: String,
    pub currency_name: String,
    pub phone_prefix: String,
    pub postal_code_format: String,
    pub languages: Vec<String>,
    pub neighbours: Vec<String>,
    pub is_eu: bool,
}

#[derive(Clone, Copy)]
struct Record {
    latitude: f64,
    longitude: f64,
    fields: [u32; MAX_FIELDS],
}

struct Grid {
    cells_per_degree: f64,
    rows: i32,
    columns: i32,
}

struct Reader<'a> {
    data: &'a [u8],
    position: usize,
}

impl Reader<'_> {
    fn varint(&mut self) -> u32 {
        let mut value = 0;
        for shift in (0..35).step_by(7) {
            let byte = self.data[self.position];
            self.position += 1;
            value |= u32::from(byte & 0x7f) << shift;
            if byte < 0x80 {
                break;
            }
        }
        value
    }

    fn signed(&mut self) -> i32 {
        unzigzag(self.varint())
    }
}

/// Reverse geocoder over a `geo.bin` database.
pub struct Geocoder {
    data: Cow<'static, [u8]>,
}

impl Geocoder {
    /// The database embedded at compile time, or `None` if it was not built.
    pub fn global() -> Option<&'static Self> {
        static GLOBAL: OnceLock<Option<Geocoder>> = OnceLock::new();
        GLOBAL.get_or_init(|| Self::from_bytes(DATABASE)).as_ref()
    }

    /// Wraps `geo.bin` bytes, or `None` if they are not a database.
    pub fn from_bytes(data: impl Into<Cow<'static, [u8]>>) -> Option<Self> {
        let data = data.into();
        let valid = data.len() >= HEADER_SIZE && data.starts_with(MAGIC);
        valid.then_some(Self { data })
    }

    /// Reads a `geo.bin` file from disk.
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        Self::from_bytes(fs::read(path)?).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "not a geo.bin file")
        })
    }

    /// Finds the nearest city to the coordinates, `None` if nothing is in range.
    pub fn lookup(&self, latitude: f64, longitude: f64) -> Option<Place> {
        let city = self.nearest(&CITIES, latitude, longitude, |_| true)?;
        let [
            name,
            ascii,
            region,
            region_code,
            district,
            district_code,
            ..,
        ] = city.fields;
        let [
            timezone,
            country,
            feature_code,
            population,
            elevation,
            geoname_id,
        ] = city.fields[6..].try_into().unwrap();

        let has_postal_codes = self.country_field(country, 17) > 0;
        let postal = has_postal_codes
            .then(|| {
                self.nearest(&POSTAL, latitude, longitude, |fields| fields[1] == country)
            })
            .flatten()
            .map_or([0; MAX_FIELDS], |record| record.fields);

        Some(Place {
            city: self.text(name),
            city_ascii: self.text(ascii),
            geoname_id,
            feature_code: self.text(feature_code),
            population,
            elevation: elevation.checked_sub(1).map(unzigzag),
            latitude: city.latitude,
            longitude: city.longitude,
            distance_km: haversine(latitude, longitude, city.latitude, city.longitude),
            region: self.text(region),
            region_code: self.text(region_code),
            district: self.text(district),
            district_code: self.text(district_code),
            postal_code: self.text(postal[0]),
            locality: self.text(postal[2]),
            municipality: self.text(postal[3]),
            timezone: timezone_details(&self.text(timezone)),
            country: self.country(country),
        })
    }

    fn country(&self, id: u32) -> Country {
        let text = |column| self.text(self.country_field(id, column));
        let list = |column| {
            let text = text(column);
            text.split(',')
                .filter(|item| !item.is_empty())
                .map(str::to_string)
                .collect()
        };
        Country {
            code: text(0),
            iso3: text(1),
            iso_numeric: text(2),
            name: text(3),
            capital: text(4),
            area_km2: text(5).parse().unwrap_or(0.0),
            population: text(6).parse().unwrap_or(0),
            continent_code: text(7),
            continent_name: text(8),
            tld: text(9),
            currency: text(10),
            currency_name: text(11),
            phone_prefix: text(12),
            postal_code_format: text(13),
            languages: list(14),
            neighbours: list(15),
            is_eu: self.country_field(id, 16) == 1,
        }
    }

    fn word(&self, position: usize) -> u32 {
        u32::from_le_bytes(self.data[position..position + 4].try_into().unwrap())
    }

    fn header(&self, index: usize) -> usize {
        self.word(4 * index) as usize
    }

    fn country_field(&self, id: u32, column: usize) -> u32 {
        let row = id as usize * self.header(5);
        self.word(self.header(6) + 4 * (row + column))
    }

    fn text(&self, index: u32) -> String {
        let index = index as usize;
        if index >= self.header(1) {
            return String::new();
        }
        let offsets = self.header(2) + 4 * index;
        let body = self.header(3);
        let start = body + self.word(offsets) as usize;
        let end = body + self.word(offsets + 4) as usize;
        String::from_utf8_lossy(&self.data[start..end]).into_owned()
    }

    fn grid(&self, layer: &Layer) -> Grid {
        let cells_per_degree = self.header(layer.header + 3) as i32;
        Grid {
            cells_per_degree: cells_per_degree as f64,
            rows: 180 * cells_per_degree,
            columns: 360 * cells_per_degree,
        }
    }

    fn cell(&self, layer: &Layer, grid: &Grid, row: i32, column: i32) -> Option<usize> {
        if !(0..grid.rows).contains(&row) {
            return None;
        }
        let cell = (row * grid.columns + column.rem_euclid(grid.columns)) as u32;
        let directory = self.header(layer.header + 1);
        let (mut low, mut high) = (0, self.header(layer.header));
        while low < high {
            let middle = (low + high) / 2;
            let entry = directory + 8 * middle;
            match self.word(entry).cmp(&cell) {
                Ordering::Less => low = middle + 1,
                Ordering::Greater => high = middle,
                Ordering::Equal => {
                    let body = self.header(layer.header + 2);
                    return Some(body + self.word(entry + 4) as usize);
                }
            }
        }
        None
    }

    fn records(&self, layer: &Layer, position: usize) -> impl Iterator<Item = Record> {
        let field_count = self.header(layer.header + 4);
        let mut reader = Reader {
            data: &self.data,
            position,
        };
        let count = reader.varint();
        let (mut latitude, mut longitude) = (0, 0);
        (0..count).map(move |_| {
            latitude += reader.signed();
            longitude += reader.signed();
            let mut fields = [0; MAX_FIELDS];
            for index in 0..field_count {
                let value = reader.varint();
                if let Some(field) = fields.get_mut(index) {
                    *field = value;
                }
            }
            Record {
                latitude: latitude as f64 / 1e6,
                longitude: longitude as f64 / 1e6,
                fields,
            }
        })
    }

    fn nearest(
        &self,
        layer: &Layer,
        latitude: f64,
        longitude: f64,
        accept: impl Fn(&[u32; MAX_FIELDS]) -> bool,
    ) -> Option<Record> {
        let grid = self.grid(layer);
        let row_position = (latitude + 90.0) * grid.cells_per_degree;
        let column_position = (longitude + 180.0) * grid.cells_per_degree;
        let (row, column) = (row_position as i32, column_position as i32);
        let edge = |position: f64| position.fract().min(1.0 - position.fract());
        let longitude_scale = latitude.to_radians().cos();
        let row_limit = (layer.max_degrees * grid.cells_per_degree).ceil() as i32;
        let max_radius = (row_limit as f64 / longitude_scale.max(0.2)) as i32;
        let mut closest = layer.max_degrees.powi(2);
        let mut best = None;

        for radius in 0..=max_radius {
            let scanned = (radius - 1) as f64;
            let row_reach = scanned + edge(row_position);
            let column_reach = (scanned + edge(column_position)) * longitude_scale;
            let reach = row_reach.min(column_reach) / grid.cells_per_degree;
            if radius > 0 && reach * reach > closest {
                break;
            }
            let candidates = ring(radius)
                .into_iter()
                .filter(|(row_step, _)| row_step.abs() <= row_limit)
                .filter_map(|(row_step, column_step)| {
                    self.cell(layer, &grid, row + row_step, column + column_step)
                })
                .flat_map(|position| self.records(layer, position))
                .filter(|record| accept(&record.fields));
            for record in candidates {
                let latitude_delta = record.latitude - latitude;
                let longitude_delta =
                    wrap_degrees(record.longitude - longitude) * longitude_scale;
                let distance = latitude_delta.powi(2) + longitude_delta.powi(2);
                if distance < closest {
                    (closest, best) = (distance, Some(record));
                }
            }
        }
        best
    }
}

fn unzigzag(value: u32) -> i32 {
    (value >> 1) as i32 ^ -((value & 1) as i32)
}

fn wrap_degrees(degrees: f64) -> f64 {
    (degrees + 540.0) % 360.0 - 180.0
}

fn haversine(
    latitude: f64,
    longitude: f64,
    other_latitude: f64,
    other_longitude: f64,
) -> f64 {
    let latitude_delta = (other_latitude - latitude).to_radians() / 2.0;
    let longitude_delta = wrap_degrees(other_longitude - longitude).to_radians() / 2.0;
    let chord = latitude_delta.sin().powi(2)
        + latitude.to_radians().cos()
            * other_latitude.to_radians().cos()
            * longitude_delta.sin().powi(2);
    2.0 * EARTH_RADIUS_KM * chord.sqrt().asin()
}

fn ring(radius: i32) -> Vec<(i32, i32)> {
    if radius == 0 {
        return vec![(0, 0)];
    }
    let horizontal =
        (-radius..=radius).flat_map(|step| [(-radius, step), (radius, step)]);
    let vertical =
        (1 - radius..radius).flat_map(|step| [(step, -radius), (step, radius)]);
    horizontal.chain(vertical).collect()
}

fn timezone_details(name: &str) -> Timezone {
    let Ok(zone) = name.parse::<Tz>() else {
        return Timezone {
            name: name.to_string(),
            abbreviation: String::new(),
            utc_offset: 0,
            utc_offset_str: format_offset(0),
            dst_active: false,
        };
    };
    let now = Utc::now().with_timezone(&zone);
    let offset = now.offset();
    let utc_offset = offset.fix().local_minus_utc();
    Timezone {
        name: name.to_string(),
        abbreviation: offset
            .abbreviation()
            .filter(|abbreviation| !abbreviation.is_empty())
            .map_or_else(|| now.format("%z").to_string(), str::to_string),
        utc_offset,
        utc_offset_str: format_offset(utc_offset),
        dst_active: !offset.dst_offset().is_zero(),
    }
}

fn format_offset(seconds: i32) -> String {
    let sign = if seconds < 0 { '-' } else { '+' };
    let (hours, minutes) = (seconds.abs() / 3600, seconds.abs() % 3600 / 60);
    match minutes {
        0 => format!("UTC{sign}{hours}"),
        _ => format!("UTC{sign}{hours}:{minutes:02}"),
    }
}

/// Reverse-geocodes with the embedded database.
pub fn lookup(latitude: f64, longitude: f64) -> Option<Place> {
    Geocoder::global()?.lookup(latitude, longitude)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place(latitude: f64, longitude: f64) -> Option<Place> {
        Geocoder::global().and_then(|geocoder| geocoder.lookup(latitude, longitude))
    }

    #[test]
    fn formats_offsets() {
        assert_eq!(format_offset(3600), "UTC+1");
        assert_eq!(format_offset(-12600), "UTC-3:30");
        assert_eq!(format_offset(19800), "UTC+5:30");
    }

    #[test]
    fn rings_cover_the_perimeter() {
        assert_eq!(ring(0), vec![(0, 0)]);
        assert_eq!(ring(1).len(), 8);
        assert_eq!(ring(3).len(), 24);
    }

    #[test]
    fn measures_distance() {
        let distance = haversine(48.8566, 2.3522, 51.5074, -0.1278);
        assert!((distance - 343.5).abs() < 1.0);
        assert!(haversine(0.0, 179.9, 0.0, -179.9) < 23.0);
    }

    #[test]
    fn finds_major_cities() {
        let Some(new_york) = place(40.7128, -74.0060) else {
            return;
        };
        assert_eq!(new_york.country.code, "US");
        assert_eq!(new_york.country.name, "United States");
        assert_eq!(new_york.country.iso3, "USA");
        assert_eq!(new_york.country.currency, "USD");
        assert_eq!(new_york.country.continent_name, "North America");
        assert!(new_york.country.neighbours.contains(&"CA".to_string()));
        assert_eq!(new_york.timezone.name, "America/New_York");
        assert!(new_york.population > 1_000_000);
        assert!(!new_york.country.is_eu);

        let paris = place(48.8566, 2.3522).unwrap();
        assert_eq!(paris.country.code, "FR");
        assert_eq!(paris.country.capital, "Paris");
        assert!(paris.postal_code.starts_with("75"));
        assert!(paris.country.is_eu);
        assert!(paris.distance_km < 2.0);

        let tokyo = place(35.6762, 139.6503).unwrap();
        assert_eq!(tokyo.country.code, "JP");
    }

    #[test]
    fn wraps_the_antimeridian() {
        let Some(fiji) = place(-16.8, 179.99) else {
            return;
        };
        assert_eq!(fiji.country.code, "FJ");
    }
}
