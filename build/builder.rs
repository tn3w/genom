use rustc_hash::FxHashMap;
use std::io::{BufRead, BufReader, Read};
use std::sync::{Arc, Mutex};
use types::{CompactPlace, Database};

mod types;

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

const FEATURE_CODES: &[&str] = &[
    "PPL", "PPLA", "PPLA2", "PPLA3", "PPLA4", "PPLC", "PPLG", "PPLS",
];

#[derive(Debug)]
struct TempPlace {
    city: String,
    region: String,
    region_code: String,
    district: String,
    country_code: String,
    postal_code: String,
    timezone: String,
    lat: i32,
    lon: i32,
}

pub struct Builder {
    admin1: FxHashMap<String, String>,
    admin2: FxHashMap<String, String>,
    admin1_iso: FxHashMap<u32, String>,
}

impl Builder {
    pub fn new() -> Self {
        Self {
            admin1: FxHashMap::default(),
            admin2: FxHashMap::default(),
            admin1_iso: FxHashMap::default(),
        }
    }

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
        let db = Database {
            strings,
            places: compact_places,
            grid,
        };

        println!("Writing database...");
        let encoded = bincode::encode_to_vec(&db, bincode::config::standard())?;
        std::fs::write(output_path, &encoded)?;
        println!("Done! Database size: {} MB", encoded.len() / 1_000_000);
        Ok(())
    }

    fn download_admin_codes(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let base = "https://download.geonames.org/export/dump/";
        self.admin1 = Self::load_admin_map(&format!("{}admin1CodesASCII.txt", base))?;
        self.admin2 = Self::load_admin_map(&format!("{}admin2Codes.txt", base))?;
        Ok(())
    }

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

    fn build_grid(&self, places: &[CompactPlace]) -> FxHashMap<(i16, i16), Vec<u32>> {
        let mut grid: FxHashMap<(i16, i16), Vec<u32>> = FxHashMap::default();
        for (idx, place) in places.iter().enumerate() {
            let key = ((place.lat / 10000) as i16, (place.lon / 10000) as i16);
            grid.entry(key).or_default().push(idx as u32);
        }
        grid
    }
}

fn intern_string(s: &str, map: &mut FxHashMap<String, u32>, strings: &mut Vec<String>) -> u32 {
    *map.entry(s.to_string()).or_insert_with(|| {
        let idx = strings.len() as u32;
        strings.push(s.to_string());
        idx
    })
}

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

#[derive(Debug)]
struct PostalCode {
    country: String,
    code: String,
    district: String,
    lat: i32,
    lon: i32,
}

impl Builder {
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
                            if closest.map_or(true, |(_, d)| dist < d) {
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
