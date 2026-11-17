use std::collections::{BTreeMap, HashMap};
use std::error::Error;
use std::fs;
use std::io::{Cursor, Read};
use std::path::Path;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

const DUMP_URL: &str = "https://download.geonames.org/export";
const MAGIC: &[u8; 4] = b"GEO2";
const HEADER_WORDS: usize = 17;
const CITY_CELLS_PER_DEGREE: u32 = 10;
const POSTAL_CELLS_PER_DEGREE: u32 = 10;

const EUROPEAN_UNION: [&str; 27] = [
    "AT", "BE", "BG", "HR", "CY", "CZ", "DK", "EE", "FI", "FR", "DE", "GR", "HU", "IE",
    "IT", "LV", "LT", "LU", "MT", "NL", "PL", "PT", "RO", "SK", "SI", "ES", "SE",
];

struct Point {
    latitude: i32,
    longitude: i32,
    fields: Vec<u32>,
}

struct Country {
    id: u32,
    row: Vec<u32>,
    postal_codes: u32,
}

#[derive(Default)]
struct Strings {
    ids: HashMap<String, u32>,
    list: Vec<String>,
}

impl Strings {
    fn intern(&mut self, text: &str) -> u32 {
        if let Some(&id) = self.ids.get(text) {
            return id;
        }
        let id = self.list.len() as u32;
        self.ids.insert(text.to_string(), id);
        self.list.push(text.to_string());
        id
    }
}

pub fn build(sources: &Path, output: &Path) -> Result<()> {
    fs::create_dir_all(sources)?;
    let countries = fetch_text(sources, "countryInfo.txt", "dump/countryInfo.txt")?;
    let admin1 = fetch_text(sources, "admin1.txt", "dump/admin1CodesASCII.txt")?;
    let admin2 = fetch_text(sources, "admin2.txt", "dump/admin2Codes.txt")?;
    let cities = fetch(sources, "cities500.zip", "dump/cities500.zip")?;
    let postal = fetch(sources, "postal.zip", "zip/allCountries.zip")?;

    let mut strings = Strings::default();
    strings.intern("");
    let mut countries = parse_countries(&countries, &mut strings);
    let admin_names: HashMap<&str, &str> = admin1
        .lines()
        .chain(admin2.lines())
        .filter_map(|line| {
            let mut columns = line.split('\t');
            Some((columns.next()?, columns.next()?))
        })
        .collect();

    let cities = unzip(&cities, "cities500.txt")?;
    let cities = parse_cities(&cities, &admin_names, &countries, &mut strings);
    let postal = unzip(&postal, "allCountries.txt")?;
    let postal = parse_postal(&postal, &mut countries, &mut strings);

    fs::write(output, encode(strings, countries, cities, postal))?;
    Ok(())
}

fn fetch(sources: &Path, name: &str, url_path: &str) -> Result<Vec<u8>> {
    let path = sources.join(name);
    if path.exists() {
        return Ok(fs::read(path)?);
    }
    let mut bytes = Vec::new();
    ureq::get(format!("{DUMP_URL}/{url_path}"))
        .call()?
        .into_body()
        .into_reader()
        .read_to_end(&mut bytes)?;
    fs::write(path, &bytes)?;
    Ok(bytes)
}

fn fetch_text(sources: &Path, name: &str, url_path: &str) -> Result<String> {
    Ok(String::from_utf8(fetch(sources, name, url_path)?)?)
}

fn unzip(archive: &[u8], name: &str) -> Result<String> {
    let mut text = String::new();
    zip::ZipArchive::new(Cursor::new(archive))?
        .by_name(name)?
        .read_to_string(&mut text)?;
    Ok(text)
}

fn continent_name(code: &str) -> &'static str {
    match code {
        "AF" => "Africa",
        "AN" => "Antarctica",
        "AS" => "Asia",
        "EU" => "Europe",
        "NA" => "North America",
        "OC" => "Oceania",
        "SA" => "South America",
        _ => "",
    }
}

fn parse_countries(text: &str, strings: &mut Strings) -> HashMap<String, Country> {
    let mut countries = HashMap::new();
    for line in text.lines().filter(|line| !line.starts_with('#')) {
        let columns: Vec<&str> = line.split('\t').collect();
        if columns.len() < 11 {
            continue;
        }
        let column = |index: usize| columns.get(index).copied().unwrap_or("");
        let texts = [
            column(0),
            column(1),
            column(2),
            column(4),
            column(5),
            column(6),
            column(7),
            column(8),
            continent_name(column(8)),
            column(9),
            column(10),
            column(11),
            column(12),
            column(13),
            column(15),
            column(17),
        ];
        let mut row = texts.map(|text| strings.intern(text)).to_vec();
        row.push(EUROPEAN_UNION.contains(&column(0)) as u32);
        let id = countries.len() as u32;
        let country = Country {
            id,
            row,
            postal_codes: 0,
        };
        countries.insert(column(0).to_string(), country);
    }
    countries
}

fn microdegrees(text: &str) -> Option<i32> {
    let degrees: f64 = text.parse().ok()?;
    Some((degrees * 1e6).round() as i32)
}

fn zigzag(value: i32) -> u32 {
    ((value << 1) ^ (value >> 31)) as u32
}

fn elevation(columns: &[&str]) -> u32 {
    let measured = columns[15].parse().ok();
    let modelled = columns[16]
        .parse()
        .ok()
        .filter(|&value: &i32| value != -9999);
    measured.or(modelled).map_or(0, |value| zigzag(value) + 1)
}

fn parse_cities(
    text: &str,
    admin_names: &HashMap<&str, &str>,
    countries: &HashMap<String, Country>,
    strings: &mut Strings,
) -> Vec<Point> {
    let mut points = Vec::new();
    for line in text.lines() {
        let columns: Vec<&str> = line.split('\t').collect();
        if columns.len() < 18 {
            continue;
        }
        let (country, admin1, admin2) = (columns[8], columns[10], columns[11]);
        let Some(country) = countries.get(country) else {
            continue;
        };
        let (Some(latitude), Some(longitude)) =
            (microdegrees(columns[4]), microdegrees(columns[5]))
        else {
            continue;
        };
        let region_key = format!("{}.{admin1}", columns[8]);
        let district_key = format!("{region_key}.{admin2}");
        let name_of = |key: &str| admin_names.get(key).copied().unwrap_or("");
        let fields = vec![
            strings.intern(columns[1]),
            strings.intern(columns[2]),
            strings.intern(name_of(&region_key)),
            strings.intern(admin1),
            strings.intern(name_of(&district_key)),
            strings.intern(admin2),
            strings.intern(columns[17]),
            country.id,
            strings.intern(columns[7]),
            columns[14].parse().unwrap_or(0),
            elevation(&columns),
            columns[0].parse().unwrap_or(0),
        ];
        points.push(Point {
            latitude,
            longitude,
            fields,
        });
    }
    points
}

fn parse_postal(
    text: &str,
    countries: &mut HashMap<String, Country>,
    strings: &mut Strings,
) -> Vec<Point> {
    let mut points = Vec::new();
    for line in text.lines() {
        let columns: Vec<&str> = line.split('\t').collect();
        if columns.len() < 11 {
            continue;
        }
        let Some(country) = countries.get_mut(columns[0]) else {
            continue;
        };
        let (Some(latitude), Some(longitude)) =
            (microdegrees(columns[9]), microdegrees(columns[10]))
        else {
            continue;
        };
        country.postal_codes += 1;
        let fields = vec![
            strings.intern(columns[1]),
            country.id,
            strings.intern(columns[2]),
            strings.intern(columns[7]),
        ];
        points.push(Point {
            latitude,
            longitude,
            fields,
        });
    }
    points
}

fn cell_of(point: &Point, cells_per_degree: u32) -> u32 {
    let scale = cells_per_degree as f64 / 1e6;
    let rows = 180 * cells_per_degree;
    let columns = 360 * cells_per_degree;
    let row = ((point.latitude as f64 + 90e6) * scale) as u32;
    let column = ((point.longitude as f64 + 180e6) * scale) as u32;
    row.min(rows - 1) * columns + column.min(columns - 1)
}

fn write_varint(output: &mut Vec<u8>, mut value: u32) {
    while value >= 0x80 {
        output.push(value as u8 | 0x80);
        value >>= 7;
    }
    output.push(value as u8);
}

fn encode_layer(points: Vec<Point>, cells_per_degree: u32) -> (Vec<u32>, Vec<u8>) {
    let mut cells: BTreeMap<u32, Vec<Point>> = BTreeMap::new();
    for point in points {
        cells
            .entry(cell_of(&point, cells_per_degree))
            .or_default()
            .push(point);
    }
    let mut directory = Vec::new();
    let mut body = Vec::new();
    for (cell, mut points) in cells {
        points.sort_by_key(|point| (point.latitude, point.longitude));
        directory.extend([cell, body.len() as u32]);
        write_varint(&mut body, points.len() as u32);
        let (mut latitude, mut longitude) = (0, 0);
        for point in points {
            write_varint(&mut body, zigzag(point.latitude - latitude));
            write_varint(&mut body, zigzag(point.longitude - longitude));
            (latitude, longitude) = (point.latitude, point.longitude);
            for field in point.fields {
                write_varint(&mut body, field);
            }
        }
    }
    (directory, body)
}

fn append(output: &mut Vec<u8>, bytes: &[u8]) -> u32 {
    output.resize(output.len().next_multiple_of(4), 0);
    let position = output.len() as u32;
    output.extend_from_slice(bytes);
    position
}

fn words(values: &[u32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

fn encode(
    strings: Strings,
    countries: HashMap<String, Country>,
    cities: Vec<Point>,
    postal: Vec<Point>,
) -> Vec<u8> {
    let mut offsets = vec![0];
    let mut text = Vec::new();
    for string in &strings.list {
        text.extend_from_slice(string.as_bytes());
        offsets.push(text.len() as u32);
    }

    let mut countries: Vec<Country> = countries.into_values().collect();
    countries.sort_by_key(|country| country.id);
    let rows: Vec<Vec<u32>> = countries
        .into_iter()
        .map(|country| [country.row, vec![country.postal_codes]].concat())
        .collect();

    let mut output = vec![0; 4 * HEADER_WORDS];
    let mut header = vec![u32::from_le_bytes(*MAGIC), strings.list.len() as u32];
    header.push(append(&mut output, &words(&offsets)));
    header.push(append(&mut output, &text));
    header.extend([rows.len() as u32, rows[0].len() as u32]);
    header.push(append(&mut output, &words(&rows.concat())));
    let layers = [
        (cities, CITY_CELLS_PER_DEGREE),
        (postal, POSTAL_CELLS_PER_DEGREE),
    ];
    for (points, cells_per_degree) in layers {
        let field_count = points.first().map_or(0, |point| point.fields.len());
        let (directory, body) = encode_layer(points, cells_per_degree);
        header.push((directory.len() / 2) as u32);
        header.push(append(&mut output, &words(&directory)));
        header.push(append(&mut output, &body));
        header.extend([cells_per_degree, field_count as u32]);
    }
    output[..4 * HEADER_WORDS].copy_from_slice(&words(&header));
    output
}
