//! Database builder that downloads and processes GeoNames data.
//!
//! This module handles the entire database construction pipeline:
//!
//! 1. **Download Phase**: Fetches data from GeoNames.org
//!    - Administrative codes (admin1CodesASCII.txt, admin2Codes.txt)
//!    - Alternate names for ISO codes (alternateNamesV2.zip)
//!    - Place data for each country (e.g., US.zip, FR.zip)
//!    - Postal code data for each country
//!
//! 2. **Processing Phase**: Transforms raw data
//!    - Filters places by feature codes (cities, towns, villages)
//!    - Merges postal codes with nearest places
//!    - Deduplicates entries based on proximity
//!
//! 3. **Optimization Phase**: Reduces memory footprint
//!    - String interning to deduplicate common strings
//!    - Fixed-point coordinate encoding (5 decimal places)
//!    - Spatial grid indexing for fast lookups
//!
//! 4. **Serialization Phase**: Writes binary database
//!    - Uses varint encoding for compact binary format
//!    - Typical output size: 20-30 MB for 100+ countries
//!
//! # Data Sources
//!
//! All data is downloaded from [GeoNames.org](https://download.geonames.org/export/dump/)
//! which provides free geographic data under Creative Commons Attribution 4.0 license.

use rustc_hash::FxHashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::sync::{Arc, Mutex};
use types::CompactPlace;

use crate::types;

/// Countries to include in the database.
///
/// This list focuses on countries with significant population and data quality.
/// Adding more countries increases build time and database size proportionally.
const COUNTRIES: &[&str] = &[
    "AD", "AE", "AI", "AL", "AR", "AS", "AT", "AU", "AX", "AZ", "BD", "BE", "BG", "BM", "BR", "BY",
    "CA", "CC", "CH", "CL", "CN", "CO", "CR", "CX", "CY", "CZ", "DE", "DK", "DO", "DZ", "EC", "EE",
    "ES", "FI", "FK", "FM", "FO", "FR", "GB", "GF", "GG", "GI", "GL", "GP", "GS", "GT", "GU", "HK",
    "HM", "HN", "HR", "HT", "HU", "ID", "IE", "IM", "IN", "IO", "IS", "IT", "JE", "JP", "KE", "KR",
    "LI", "LK", "LT", "LU", "LV", "MA", "MC", "MD", "MH", "MK", "MO", "MP", "MQ", "MT", "MW", "MX",
    "MY", "NC", "NF", "NL", "NO", "NR", "NU", "NZ", "PA", "PE", "PF", "PH", "PK", "PL", "PM", "PN",
    "PR", "PT", "PW", "RE", "RO", "RS", "RU", "SE", "SG", "SI", "SJ", "SK", "SM", "TC", "TH", "TR",
    "UA", "US", "UY", "VA", "VI", "WF", "WS", "YT", "ZA",
];

/// GeoNames feature codes for populated places.
///
/// These codes identify different types of settlements:
/// - PPL: populated place (generic)
/// - PPLA: seat of a first-order administrative division
/// - PPLA2: seat of a second-order administrative division
/// - PPLA3: seat of a third-order administrative division
/// - PPLA4: seat of a fourth-order administrative division
/// - PPLC: capital of a political entity
/// - PPLG: seat of government of a political entity
/// - PPLS: populated places (generic)
const FEATURE_CODES: &[&str] = &[
    "PPL", "PPLA", "PPLA2", "PPLA3", "PPLA4", "PPLC", "PPLG", "PPLS",
];

/// Temporary place structure used during database construction.
///
/// This struct holds raw place data before string interning and final serialization.
/// Coordinates are stored as fixed-point integers (multiplied by 100,000) to maintain
/// precision while using less memory than f64.
#[derive(Debug)]
struct TempPlace {
    /// City or locality name
    city: String,
    /// State/province name
    region: String,
    /// ISO 3166-2 region code
    region_code: String,
    /// County/district name
    district: String,
    /// ISO 3166-1 alpha-2 country code
    country_code: String,
    /// Postal/ZIP code
    postal_code: String,
    /// IANA timezone identifier
    timezone: String,
    /// Latitude as fixed-point integer (degrees * 100,000)
    lat: i32,
    /// Longitude as fixed-point integer (degrees * 100,000)
    lon: i32,
}

/// Database builder that orchestrates the entire construction process.
///
/// The builder maintains state for administrative code lookups and coordinates
/// the parallel download and processing of geographic data.
pub struct Builder {
    /// Maps admin1 codes to region names (e.g., "US.CA" -> "California")
    admin1: FxHashMap<String, String>,
    /// Maps admin2 codes to district names (e.g., "US.CA.037" -> "Los Angeles County")
    admin2: FxHashMap<String, String>,
    /// Maps GeoNames IDs to ISO region codes for admin1 divisions
    admin1_iso: FxHashMap<u32, String>,
}

impl Builder {
    /// Creates a new database builder with empty lookup tables.
    pub fn new() -> Self {
        Self {
            admin1: FxHashMap::default(),
            admin2: FxHashMap::default(),
            admin1_iso: FxHashMap::default(),
        }
    }

    /// Builds the complete database and writes it to the specified path.
    ///
    /// # Process
    ///
    /// 1. Downloads administrative codes from GeoNames
    /// 2. Downloads place data for all countries in parallel
    /// 3. Downloads postal code data in parallel
    /// 4. Merges postal codes with nearest places
    /// 5. Deduplicates places within ~1km radius
    /// 6. Interns strings to reduce memory usage
    /// 7. Builds spatial grid index
    /// 8. Serializes to binary format with varint encoding
    ///
    /// # Arguments
    ///
    /// * `output_path` - Path where the binary database will be written
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Network requests fail
    /// - Downloaded data is malformed
    /// - File system operations fail
    /// - Serialization fails
    ///
    /// # Performance
    ///
    /// Typical build time: 2-5 minutes depending on network speed.
    /// Uses parallel downloads to minimize wall-clock time.
    pub(crate) fn build(&mut self, output_path: &str) -> Result<(), Box<dyn std::error::Error>> {
        println!("Downloading admin codes...");
        self.download_admin_codes()?;
        self.download_admin_iso_codes()?;

        println!("Downloading places...");
        let mut places = self.download_places()?;

        println!("Downloading postal codes...");
        self.merge_postal_codes(&mut places, self.download_postal_codes()?);

        println!("Deduplicating {} places...", places.len());
        let places = self.deduplicate_places(places);

        println!("Building database for {} places...", places.len());
        let (strings, compact_places) = self.intern_strings(places);
        let grid = self.build_grid(&compact_places);

        println!("Writing database...");
        let mut out = BufWriter::new(File::create(output_path)?);
        
        out.write_all(&(strings.len() as u64).to_le_bytes())?;
        for s in &strings {
            let bytes = s.as_bytes();
            write_varint(&mut out, bytes.len() as u64)?;
            out.write_all(bytes)?;
        }

        out.write_all(&(compact_places.len() as u64).to_le_bytes())?;
        for place in &compact_places {
            out.write_all(&place.city.to_le_bytes())?;
            out.write_all(&place.region.to_le_bytes())?;
            out.write_all(&place.region_code.to_le_bytes())?;
            out.write_all(&place.district.to_le_bytes())?;
            out.write_all(&place.country_code.to_le_bytes())?;
            out.write_all(&place.postal_code.to_le_bytes())?;
            out.write_all(&place.timezone.to_le_bytes())?;
            out.write_all(&place.lat.to_le_bytes())?;
            out.write_all(&place.lon.to_le_bytes())?;
        }

        out.write_all(&(grid.len() as u64).to_le_bytes())?;
        for ((lat, lon), indices) in &grid {
            out.write_all(&lat.to_le_bytes())?;
            out.write_all(&lon.to_le_bytes())?;
            out.write_all(&(indices.len() as u64).to_le_bytes())?;
            for idx in indices {
                out.write_all(&idx.to_le_bytes())?;
            }
        }

        out.flush()?;
        let size = std::fs::metadata(output_path)?.len();
        println!("Done! Database size: {} MB", size / 1_000_000);
        Ok(())
    }

    /// Downloads administrative code mappings from GeoNames.
    ///
    /// Fetches admin1 (states/provinces) and admin2 (counties/districts) codes
    /// which are used to resolve region names from codes in place data.
    fn download_admin_codes(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let base = "https://download.geonames.org/export/dump/";
        self.admin1 = Self::load_admin_map(&format!("{}admin1CodesASCII.txt", base))?;
        self.admin2 = Self::load_admin_map(&format!("{}admin2Codes.txt", base))?;
        Ok(())
    }

    /// Downloads ISO region codes from alternate names database.
    ///
    /// Maps GeoNames admin1 IDs to their ISO 3166-2 region codes
    /// (e.g., "CA" for California instead of just the numeric code).
    fn download_admin_iso_codes(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let url = "https://download.geonames.org/export/dump/alternateNamesV2.zip";
        let bytes = reqwest::blocking::get(url)?.bytes()?;
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
        let mut content = String::new();
        archive
            .by_name("alternateNamesV2.txt")?
            .read_to_string(&mut content)?;

        for line in content.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 4 && parts[2] == "abbr" {
                if let Ok(id) = parts[1].parse::<u32>() {
                    self.admin1_iso.insert(id, parts[3].to_string());
                }
            }
        }
        Ok(())
    }

    /// Loads an administrative code mapping from a GeoNames URL.
    ///
    /// Parses tab-separated files containing admin codes and names.
    /// Also stores GeoNames IDs with ":gid" suffix for later ISO code lookup.
    fn load_admin_map(url: &str) -> Result<FxHashMap<String, String>, Box<dyn std::error::Error>> {
        let response = reqwest::blocking::get(url)?;
        let reader = BufReader::new(response);
        let mut map = FxHashMap::default();

        for line in reader.lines() {
            let line = line?;
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 4 {
                map.insert(parts[0].to_string(), parts[1].to_string());
                map.insert(parts[0].to_string() + ":gid", parts[3].to_string());
            }
        }
        Ok(map)
    }

    /// Downloads place data for all countries in parallel.
    ///
    /// Spawns a thread for each country to download and parse place data concurrently.
    /// Filters places to only include populated places (cities, towns, villages).
    ///
    /// # Returns
    ///
    /// A vector of all places from all countries combined.
    fn download_places(&self) -> Result<Vec<TempPlace>, Box<dyn std::error::Error>> {
        let places = Arc::new(Mutex::new(Vec::new()));
        let (admin1, admin2, admin1_iso) = (
            Arc::new(self.admin1.clone()),
            Arc::new(self.admin2.clone()),
            Arc::new(self.admin1_iso.clone()),
        );

        std::thread::scope(|scope| {
            for country in COUNTRIES {
                let (places, admin1, admin2, admin1_iso) = (
                    Arc::clone(&places),
                    Arc::clone(&admin1),
                    Arc::clone(&admin2),
                    Arc::clone(&admin1_iso),
                );

                scope.spawn(move || {
                    if let Ok(data) = download_country(country, &admin1, &admin2, &admin1_iso) {
                        places.lock().unwrap().extend(data);
                    }
                });
            }
        });

        Ok(Arc::try_unwrap(places).unwrap().into_inner().unwrap())
    }

    /// Deduplicates places that are very close to each other.
    ///
    /// # Strategy
    ///
    /// 1. Sorts places by city name length (longer names preferred)
    /// 2. Sorts by postal code presence (places with postal codes preferred)
    /// 3. Keeps only one place per ~1km grid cell (lat/lon rounded to 3 decimals)
    ///
    /// This removes duplicate entries for the same location while keeping
    /// the most complete data.
    fn deduplicate_places(&self, mut places: Vec<TempPlace>) -> Vec<TempPlace> {
        places.sort_by(|a, b| {
            b.city
                .len()
                .cmp(&a.city.len())
                .then_with(|| a.postal_code.is_empty().cmp(&b.postal_code.is_empty()))
        });

        let mut seen = FxHashMap::default();
        places.retain(|p| seen.insert((p.lat / 1000, p.lon / 1000), ()).is_none());
        places
    }

    /// Converts places to compact format using string interning.
    ///
    /// # String Interning
    ///
    /// Common strings (country codes, timezones, etc.) are stored once in a string table.
    /// Each place stores only a u32 index into this table instead of the full string.
    ///
    /// This reduces memory usage by ~60% since many strings are repeated across places.
    ///
    /// # Returns
    ///
    /// A tuple of (string_table, compact_places) where compact_places reference
    /// strings by index.
    fn intern_strings(&self, places: Vec<TempPlace>) -> (Vec<String>, Vec<CompactPlace>) {
        let mut string_map: FxHashMap<String, u32> = FxHashMap::default();
        let mut strings = Vec::new();

        let mut intern = |s: &str| intern_string(s, &mut string_map, &mut strings);

        let compact_places = places
            .into_iter()
            .map(|p| CompactPlace {
                city: intern(&p.city),
                region: intern(&p.region),
                region_code: intern(&p.region_code),
                district: intern(&p.district),
                country_code: intern(&p.country_code),
                postal_code: intern(&p.postal_code),
                timezone: intern(&p.timezone),
                lat: p.lat,
                lon: p.lon,
            })
            .collect();

        (strings, compact_places)
    }

    /// Builds a spatial grid index for fast coordinate lookups.
    ///
    /// # Grid Structure
    ///
    /// - Divides world into 0.1° × 0.1° cells (~11km at equator)
    /// - Each cell contains indices of places within that cell
    /// - Grid key is (lat/10000, lon/10000) as i16
    ///
    /// # Lookup Strategy
    ///
    /// To find nearest place:
    /// 1. Calculate grid key for query coordinates
    /// 2. Check target cell and 8 neighbors (3×3 grid)
    /// 3. Calculate distance to all candidates
    /// 4. Return nearest
    ///
    /// This provides O(1) average-case lookup with small constant factor.
    fn build_grid(&self, places: &[CompactPlace]) -> FxHashMap<(i16, i16), Vec<u32>> {
        let mut grid: FxHashMap<(i16, i16), Vec<u32>> = FxHashMap::default();
        for (idx, place) in places.iter().enumerate() {
            let key = ((place.lat / 10000) as i16, (place.lon / 10000) as i16);
            grid.entry(key).or_default().push(idx as u32);
        }
        grid
    }
}

fn write_varint(out: &mut BufWriter<File>, mut value: u64) -> std::io::Result<()> {
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.write_all(&[byte])?;
        if value == 0 {
            break;
        }
    }
    Ok(())
}

fn intern_string(s: &str, map: &mut FxHashMap<String, u32>, strings: &mut Vec<String>) -> u32 {
    *map.entry(s.to_string()).or_insert_with(|| {
        let idx = strings.len() as u32;
        strings.push(s.to_string());
        idx
    })
}

/// Downloads and parses place data for a single country.
///
/// # Arguments
///
/// * `country` - ISO 3166-1 alpha-2 country code (e.g., "US", "FR")
/// * `admin1` - Admin1 code lookup table
/// * `admin2` - Admin2 code lookup table
/// * `admin1_iso` - GeoNames ID to ISO code mapping
///
/// # Returns
///
/// Vector of places filtered to only include populated places with valid coordinates.
fn download_country(
    country: &str,
    admin1: &FxHashMap<String, String>,
    admin2: &FxHashMap<String, String>,
    admin1_iso: &FxHashMap<u32, String>,
) -> Result<Vec<TempPlace>, Box<dyn std::error::Error>> {
    let url = format!("https://download.geonames.org/export/dump/{}.zip", country);
    let bytes = reqwest::blocking::get(&url)?.bytes()?;
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
    let mut content = String::new();
    archive
        .by_name(&format!("{}.txt", country))?
        .read_to_string(&mut content)?;

    let places = content
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() < 18 || !FEATURE_CODES.contains(&parts[7]) {
                return None;
            }

            let lat = parts[4].parse::<f64>().ok()?;
            let lon = parts[5].parse::<f64>().ok()?;
            let admin1_code = parts[10];
            let admin1_key = format!("{}.{}", country, admin1_code);

            let region = admin1.get(&admin1_key).map(|s| s.as_str()).unwrap_or("");
            let district = admin2
                .get(&format!("{}.{}.{}", country, admin1_code, parts[11]))
                .map(|s| s.as_str())
                .unwrap_or("");

            let region_code = if admin1_code == "00" || admin1_code.is_empty() {
                String::new()
            } else {
                admin1
                    .get(&format!("{}:gid", admin1_key))
                    .and_then(|gid| gid.parse::<u32>().ok())
                    .and_then(|gid| admin1_iso.get(&gid))
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| admin1_code.to_string())
            };

            Some(TempPlace {
                city: parts[2].to_string(),
                region: region.to_string(),
                region_code,
                district: district.to_string(),
                country_code: country.to_string(),
                postal_code: String::new(),
                timezone: parts.get(17).unwrap_or(&"").to_string(),
                lat: (lat * 100000.0) as i32,
                lon: (lon * 100000.0) as i32,
            })
        })
        .collect();

    Ok(places)
}

/// Postal code data structure used during database construction.
#[derive(Debug)]
struct PostalCode {
    /// ISO country code
    country: String,
    /// Postal/ZIP code
    code: String,
    /// District/county name
    district: String,
    /// Latitude as fixed-point integer (degrees * 100,000)
    lat: i32,
    /// Longitude as fixed-point integer (degrees * 100,000)
    lon: i32,
}

impl Builder {
    /// Downloads postal code data for all countries in parallel.
    ///
    /// Postal codes provide more precise location data and district names
    /// that may be missing from the main place database.
    fn download_postal_codes(&self) -> Result<Vec<PostalCode>, Box<dyn std::error::Error>> {
        let codes = Arc::new(Mutex::new(Vec::new()));

        std::thread::scope(|scope| {
            for country in COUNTRIES {
                let codes = Arc::clone(&codes);
                scope.spawn(move || {
                    if let Ok(data) = download_postal_codes_for_country(country) {
                        codes.lock().unwrap().extend(data);
                    }
                });
            }
        });

        Ok(Arc::try_unwrap(codes).unwrap().into_inner().unwrap())
    }

    /// Merges postal code data with places by finding nearest postal code.
    ///
    /// # Strategy
    ///
    /// For each place:
    /// 1. Find all postal codes in the same and neighboring grid cells
    /// 2. Filter to same country
    /// 3. Calculate squared distance to each postal code
    /// 4. Assign postal code from nearest match
    /// 5. If place has no district, use postal code's district
    ///
    /// This enriches places with postal codes and fills in missing district names.
    fn merge_postal_codes(&self, places: &mut [TempPlace], postal_codes: Vec<PostalCode>) {
        let mut postal_grid: FxHashMap<(i16, i16), Vec<PostalCode>> = FxHashMap::default();
        for postal in postal_codes {
            let key = ((postal.lat / 10000) as i16, (postal.lon / 10000) as i16);
            postal_grid.entry(key).or_default().push(postal);
        }

        for place in places.iter_mut() {
            let grid_key = ((place.lat / 10000) as i16, (place.lon / 10000) as i16);
            let mut closest: Option<(&PostalCode, f64)> = None;

            for dlat in -1..=1 {
                for dlon in -1..=1 {
                    let key = (grid_key.0 + dlat, grid_key.1 + dlon);
                    if let Some(postals) = postal_grid.get(&key) {
                        for postal in postals.iter().filter(|p| p.country == place.country_code) {
                            let dist = {
                                let dlat = (place.lat - postal.lat) as f64;
                                let dlon = (place.lon - postal.lon) as f64;
                                dlat * dlat + dlon * dlon
                            };
                            if closest.is_none_or(|(_, d)| dist < d) {
                                closest = Some((postal, dist));
                            }
                        }
                    }
                }
            }

            if let Some((postal, _)) = closest {
                place.postal_code = postal.code.clone();
                if place.district.is_empty() {
                    place.district = postal.district.clone();
                }
            }
        }
    }
}

/// Downloads postal code data for a single country.
///
/// # Arguments
///
/// * `country` - ISO 3166-1 alpha-2 country code
///
/// # Returns
///
/// Vector of postal codes with coordinates. Returns empty vector if country
/// has no postal code data available.
///
/// # Note
///
/// Some countries don't have postal code data on GeoNames. The function
/// gracefully handles this by returning an empty vector.
fn download_postal_codes_for_country(
    country: &str,
) -> Result<Vec<PostalCode>, Box<dyn std::error::Error>> {
    let url = format!("https://download.geonames.org/export/zip/{}.zip", country);
    let bytes = reqwest::blocking::get(&url)?.bytes()?;

    if bytes.len() < 100 {
        return Ok(Vec::new());
    }

    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
    let mut content = String::new();
    archive
        .by_name(&format!("{}.txt", country))?
        .read_to_string(&mut content)?;

    let codes = content
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() < 11 {
                return None;
            }

            let lat = parts[9].parse::<f64>().ok()?;
            let lon = parts[10].parse::<f64>().ok()?;

            Some(PostalCode {
                country: parts[0].to_string(),
                code: parts[1].to_string(),
                district: parts.get(5).unwrap_or(&"").to_string(),
                lat: (lat * 100000.0) as i32,
                lon: (lon * 100000.0) as i32,
            })
        })
        .collect();

    Ok(codes)
}
