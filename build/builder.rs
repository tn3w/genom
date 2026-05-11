use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::io::{Cursor, Read};
use std::path::Path;

use rustc_hash::FxHashMap;

const URL_CITIES: &str = "https://download.geonames.org/export/dump/cities500.zip";
const URL_ADMIN1: &str = "https://download.geonames.org/export/dump/admin1CodesASCII.txt";
const URL_ADMIN2: &str = "https://download.geonames.org/export/dump/admin2Codes.txt";
const URL_COUNTRY: &str = "https://download.geonames.org/export/dump/countryInfo.txt";
const URL_POSTAL: &str = "https://download.geonames.org/export/zip/allCountries.zip";
const URL_NE_COUNTRIES: &str =
    "https://naciscdn.org/naturalearth/110m/cultural/ne_110m_admin_0_countries.zip";

pub fn build(cache: &Path, out_file: &Path) -> Result<(), Box<dyn Error>> {
    let cities_zip = fetch(cache, URL_CITIES, "cities500.zip")?;
    let admin1 = fetch_text(cache, URL_ADMIN1, "admin1.txt")?;
    let admin2 = fetch_text(cache, URL_ADMIN2, "admin2.txt")?;
    let country = fetch_text(cache, URL_COUNTRY, "countryInfo.txt")?;
    let postal_zip = fetch(cache, URL_POSTAL, "postal.zip")?;
    let ne_zip = fetch(cache, URL_NE_COUNTRIES, "ne_countries.zip")?;

    let cities_txt = unzip_first(&cities_zip, "cities500.txt")?;
    let postal_files = unzip_all_txt(&postal_zip)?;
    let (ne_shp, ne_dbf) = unzip_ne(&ne_zip)?;

    let cc_table = parse_country(&country);
    let admin1_map = parse_admin_map(&admin1);
    let admin2_map = parse_admin_map(&admin2);
    let cities = parse_cities(&cities_txt);
    let postal: Vec<PostalEntry> = postal_files.iter().flat_map(|t| parse_postal(t)).collect();
    let polys = parse_ne_polys(&ne_shp, &ne_dbf);

    let bin = build_binary(BuildInput {
        cc_table,
        admin1_map,
        admin2_map,
        cities,
        postal,
        polys,
    });
    fs::write(out_file, &bin)?;
    Ok(())
}

fn fetch(cache: &Path, url: &str, name: &str) -> Result<Vec<u8>, Box<dyn Error>> {
    let path = cache.join(name);
    if path.exists() {
        return Ok(fs::read(&path)?);
    }
    let mut buf = Vec::new();
    ureq::get(url).call()?.into_reader().read_to_end(&mut buf)?;
    fs::write(&path, &buf)?;
    Ok(buf)
}

fn fetch_text(cache: &Path, url: &str, name: &str) -> Result<String, Box<dyn Error>> {
    Ok(String::from_utf8(fetch(cache, url, name)?)?)
}

fn unzip_first(zip_bytes: &[u8], expect: &str) -> Result<String, Box<dyn Error>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(zip_bytes))?;
    let mut file = archive.by_name(expect)?;
    let mut buf = String::new();
    file.read_to_string(&mut buf)?;
    Ok(buf)
}

fn unzip_all_txt(zip_bytes: &[u8]) -> Result<Vec<String>, Box<dyn Error>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(zip_bytes))?;
    let mut out = Vec::new();
    for index in 0..archive.len() {
        let mut file = archive.by_index(index)?;
        let name = file.name().to_string();
        if !name.ends_with(".txt") || name.contains("readme") {
            continue;
        }
        let mut buf = String::new();
        if file.read_to_string(&mut buf).is_ok() {
            out.push(buf);
        }
    }
    Ok(out)
}

fn unzip_ne(zip_bytes: &[u8]) -> Result<(Vec<u8>, Vec<u8>), Box<dyn Error>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(zip_bytes))?;
    let mut shp = Vec::new();
    let mut dbf = Vec::new();
    for index in 0..archive.len() {
        let mut file = archive.by_index(index)?;
        let name = file.name().to_string();
        if name.ends_with(".shp") {
            file.read_to_end(&mut shp)?;
        } else if name.ends_with(".dbf") {
            file.read_to_end(&mut dbf)?;
        }
    }
    Ok((shp, dbf))
}

#[derive(Default)]
struct CountryRow {
    iso2: String,
}

fn parse_country(text: &str) -> Vec<CountryRow> {
    let mut rows = Vec::new();
    for line in text.lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 2 {
            continue;
        }
        rows.push(CountryRow {
            iso2: cols[0].to_string(),
        });
    }
    rows
}

fn parse_admin_map(text: &str) -> FxHashMap<String, String> {
    let mut map = FxHashMap::default();
    for line in text.lines() {
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() >= 2 {
            map.insert(cols[0].to_string(), cols[1].to_string());
        }
    }
    map
}

struct City {
    lat: f64,
    lon: f64,
    name: String,
    cc: String,
    a1: String,
    a2: String,
    tz: String,
}

fn parse_cities(text: &str) -> Vec<City> {
    let mut cities = Vec::new();
    for line in text.lines() {
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 18 {
            continue;
        }
        cities.push(City {
            lat: cols[4].parse().unwrap_or(0.0),
            lon: cols[5].parse().unwrap_or(0.0),
            name: cols[1].to_string(),
            cc: cols[8].to_string(),
            a1: cols[10].to_string(),
            a2: cols[11].to_string(),
            tz: cols[17].to_string(),
        });
    }
    cities
}

struct PostalEntry {
    lat: f64,
    lon: f64,
    cc: String,
    postal: String,
    a1: String,
    a2: String,
}

fn parse_postal(text: &str) -> Vec<PostalEntry> {
    let mut entries = Vec::new();
    for line in text.lines() {
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 12 {
            continue;
        }
        let lat: f64 = cols[9].parse().unwrap_or(0.0);
        let lon: f64 = cols[10].parse().unwrap_or(0.0);
        if lat == 0.0 && lon == 0.0 {
            continue;
        }
        entries.push(PostalEntry {
            lat,
            lon,
            cc: cols[0].to_string(),
            postal: cols[1].to_string(),
            a1: cols[4].to_string(),
            a2: cols[6].to_string(),
        });
    }
    entries
}

struct Poly {
    iso2: String,
    rings: Vec<Vec<(f64, f64)>>,
}

fn parse_ne_polys(shp: &[u8], dbf: &[u8]) -> Vec<Poly> {
    use shapefile::Shape;
    use shapefile::dbase::FieldValue;
    let Ok(shape_reader) = shapefile::ShapeReader::new(Cursor::new(shp)) else {
        return Vec::new();
    };
    let Ok(dbase_reader) = shapefile::dbase::Reader::new(Cursor::new(dbf)) else {
        return Vec::new();
    };
    let mut reader = shapefile::Reader::new(shape_reader, dbase_reader);
    let mut out = Vec::new();
    for record in reader.iter_shapes_and_records().flatten() {
        let (shape, fields) = record;
        let iso2 = match fields.get("ISO_A2_EH").or_else(|| fields.get("ISO_A2")) {
            Some(FieldValue::Character(Some(value))) => value.clone(),
            _ => continue,
        };
        if iso2.len() != 2 {
            continue;
        }
        let mut rings = Vec::new();
        if let Shape::Polygon(polygon) = shape {
            for ring in polygon.rings() {
                let points: Vec<(f64, f64)> = ring.points().iter().map(|p| (p.y, p.x)).collect();
                if !points.is_empty() {
                    rings.push(simplify(&points, 0.05));
                }
            }
        }
        if !rings.is_empty() {
            out.push(Poly { iso2, rings });
        }
    }
    out
}

fn simplify(points: &[(f64, f64)], tolerance: f64) -> Vec<(f64, f64)> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    *keep.last_mut().unwrap() = true;
    douglas_peucker(
        points,
        0,
        points.len() - 1,
        tolerance * tolerance,
        &mut keep,
    );
    points
        .iter()
        .zip(keep)
        .filter_map(|(point, k)| if k { Some(*point) } else { None })
        .collect()
}

fn douglas_peucker(
    points: &[(f64, f64)],
    a: usize,
    b: usize,
    tolerance_squared: f64,
    keep: &mut [bool],
) {
    if b <= a + 1 {
        return;
    }
    let (ax, ay) = points[a];
    let (bx, by) = points[b];
    let dx = bx - ax;
    let dy = by - ay;
    let len_squared = dx * dx + dy * dy;
    let mut best = 0.0;
    let mut idx = a;
    for (i, &(px, py)) in points.iter().enumerate().take(b).skip(a + 1) {
        let distance_squared = if len_squared == 0.0 {
            let ex = px - ax;
            let ey = py - ay;
            ex * ex + ey * ey
        } else {
            let t = (((px - ax) * dx + (py - ay) * dy) / len_squared).clamp(0.0, 1.0);
            let ex = px - (ax + t * dx);
            let ey = py - (ay + t * dy);
            ex * ex + ey * ey
        };
        if distance_squared > best {
            best = distance_squared;
            idx = i;
        }
    }
    if best > tolerance_squared {
        keep[idx] = true;
        douglas_peucker(points, a, idx, tolerance_squared, keep);
        douglas_peucker(points, idx, b, tolerance_squared, keep);
    }
}

struct BuildInput {
    cc_table: Vec<CountryRow>,
    admin1_map: FxHashMap<String, String>,
    admin2_map: FxHashMap<String, String>,
    cities: Vec<City>,
    postal: Vec<PostalEntry>,
    polys: Vec<Poly>,
}

const MAGIC: &[u8; 4] = b"GEO1";
const GRID_LON: i32 = 3600;
const GRID_LAT: i32 = 1800;
const GRID_SCALE: f64 = 10.0;
const PGRID_LON: i32 = 36000;
const PGRID_LAT: i32 = 18000;
const PGRID_SCALE: f64 = 100.0;

fn cell_of(lat: f64, lon: f64) -> usize {
    let la = (((lat + 90.0) * GRID_SCALE).floor() as i32).clamp(0, GRID_LAT - 1);
    let lo = (((lon + 180.0) * GRID_SCALE).floor() as i32).clamp(0, GRID_LON - 1);
    (la * GRID_LON + lo) as usize
}

fn pcell_of(lat: f64, lon: f64) -> u32 {
    let la = (((lat + 90.0) * PGRID_SCALE).floor() as i32).clamp(0, PGRID_LAT - 1);
    let lo = (((lon + 180.0) * PGRID_SCALE).floor() as i32).clamp(0, PGRID_LON - 1);
    (la * PGRID_LON + lo) as u32
}

#[derive(Default)]
struct Strings {
    map: FxHashMap<String, u32>,
    list: Vec<String>,
}

impl Strings {
    fn intern(&mut self, value: &str) -> u32 {
        if let Some(&index) = self.map.get(value) {
            return index;
        }
        let index = self.list.len() as u32;
        self.list.push(value.to_string());
        self.map.insert(value.to_string(), index);
        index
    }
}

fn write_varint(out: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        out.push((value as u8) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

fn zigzag(value: i64) -> u64 {
    ((value << 1) ^ (value >> 63)) as u64
}

fn build_binary(input: BuildInput) -> Vec<u8> {
    let mut strings = Strings::default();
    strings.intern("");

    let cc_list: Vec<String> = input.cc_table.iter().map(|c| c.iso2.clone()).collect();
    let cc_to_idx: FxHashMap<String, u16> = cc_list
        .iter()
        .enumerate()
        .map(|(i, c)| (c.clone(), i as u16))
        .collect();

    let mut admin1_lookup: FxHashMap<(u16, String), u32> = FxHashMap::default();
    for (key, name) in &input.admin1_map {
        let parts: Vec<&str> = key.splitn(2, '.').collect();
        if parts.len() != 2 {
            continue;
        }
        let cc = *cc_to_idx.get(parts[0]).unwrap_or(&0);
        let idx = strings.intern(name);
        admin1_lookup.insert((cc, parts[1].to_string()), idx);
    }

    let mut admin2_lookup: FxHashMap<(u16, String, String), u32> = FxHashMap::default();
    for (key, name) in &input.admin2_map {
        let parts: Vec<&str> = key.splitn(3, '.').collect();
        if parts.len() != 3 {
            continue;
        }
        let cc = *cc_to_idx.get(parts[0]).unwrap_or(&0);
        let idx = strings.intern(name);
        admin2_lookup.insert((cc, parts[1].to_string(), parts[2].to_string()), idx);
    }

    let admin1_code_idx: FxHashMap<String, u32> = input
        .admin1_map
        .keys()
        .map(|key| {
            let parts: Vec<&str> = key.splitn(2, '.').collect();
            (key.clone(), strings.intern(parts.get(1).unwrap_or(&"")))
        })
        .collect();

    let mut cities_by_cell: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (i, city) in input.cities.iter().enumerate() {
        cities_by_cell
            .entry(cell_of(city.lat, city.lon))
            .or_default()
            .push(i);
    }

    let mut cities_blob = Vec::new();
    let mut grid_entries: Vec<(u32, u32)> = Vec::new();
    for (cell, idxs) in &cities_by_cell {
        let mut sorted: Vec<usize> = idxs.clone();
        sorted.sort_by_key(|&i| {
            (
                (input.cities[i].lat * 1e6) as i64,
                (input.cities[i].lon * 1e6) as i64,
            )
        });
        let offset = cities_blob.len() as u32;
        write_varint(&mut cities_blob, sorted.len() as u64);
        let mut prev_lat = 0i64;
        let mut prev_lon = 0i64;
        for &i in &sorted {
            let city = &input.cities[i];
            let cc = *cc_to_idx.get(&city.cc).unwrap_or(&0);
            let name_idx = strings.intern(&city.name);
            let a1_idx = *admin1_lookup.get(&(cc, city.a1.clone())).unwrap_or(&0);
            let a2_idx = *admin2_lookup
                .get(&(cc, city.a1.clone(), city.a2.clone()))
                .unwrap_or(&0);
            let a1_code_idx = *admin1_code_idx
                .get(&format!("{}.{}", city.cc, city.a1))
                .unwrap_or(&0);
            let tz_idx = strings.intern(&city.tz);
            let lat = (city.lat * 1e6) as i64;
            let lon = (city.lon * 1e6) as i64;
            write_varint(&mut cities_blob, zigzag(lat - prev_lat));
            write_varint(&mut cities_blob, zigzag(lon - prev_lon));
            prev_lat = lat;
            prev_lon = lon;
            write_varint(&mut cities_blob, name_idx as u64);
            write_varint(&mut cities_blob, a1_idx as u64);
            write_varint(&mut cities_blob, a2_idx as u64);
            write_varint(&mut cities_blob, a1_code_idx as u64);
            write_varint(&mut cities_blob, tz_idx as u64);
            write_varint(&mut cities_blob, cc as u64);
        }
        grid_entries.push((*cell as u32, offset));
    }

    let mut postal_by_country: BTreeMap<u16, Vec<&PostalEntry>> = BTreeMap::new();
    for entry in &input.postal {
        let cc = *cc_to_idx.get(&entry.cc).unwrap_or(&0);
        postal_by_country.entry(cc).or_default().push(entry);
    }

    let mut postal_blob = Vec::new();
    let mut postal_dir: Vec<(u16, u32, u32)> = Vec::new();
    for (cc, list) in &postal_by_country {
        let mut admin_tuples: Vec<(u32, u32, u32)> = Vec::new();
        let mut admin_tuple_map: FxHashMap<(u32, u32, u32), u32> = FxHashMap::default();
        let mut postal_strings: Vec<&str> = Vec::new();
        let mut postal_str_map: FxHashMap<&str, u32> = FxHashMap::default();
        let mut entries: Vec<(i64, i64, u32, u32)> = Vec::with_capacity(list.len());
        for entry in list {
            let a1_idx = *admin1_lookup.get(&(*cc, entry.a1.clone())).unwrap_or(&0);
            let a2_idx = *admin2_lookup
                .get(&(*cc, entry.a1.clone(), entry.a2.clone()))
                .unwrap_or(&0);
            let a1c = *admin1_code_idx
                .get(&format!("{}.{}", entry.cc, entry.a1))
                .unwrap_or(&0);
            let key = (a1_idx, a2_idx, a1c);
            let tup_idx = *admin_tuple_map.entry(key).or_insert_with(|| {
                admin_tuples.push(key);
                (admin_tuples.len() - 1) as u32
            });
            let ps_idx = *postal_str_map
                .entry(entry.postal.as_str())
                .or_insert_with(|| {
                    postal_strings.push(entry.postal.as_str());
                    (postal_strings.len() - 1) as u32
                });
            entries.push((
                (entry.lat * 1e6) as i64,
                (entry.lon * 1e6) as i64,
                ps_idx,
                tup_idx,
            ));
        }

        entries.sort_by_key(|e| (pcell_of(e.0 as f64 / 1e6, e.1 as f64 / 1e6), e.0, e.1));

        let mut order: Vec<u32> = (0..postal_strings.len() as u32).collect();
        order.sort_by(|&a, &b| postal_strings[a as usize].cmp(postal_strings[b as usize]));
        let mut new_idx = vec![0u32; postal_strings.len()];
        for (new, &old) in order.iter().enumerate() {
            new_idx[old as usize] = new as u32;
        }
        let sorted_postals: Vec<&str> = order.iter().map(|&o| postal_strings[o as usize]).collect();

        let start = postal_blob.len() as u32;
        postal_blob.extend_from_slice(&(admin_tuples.len() as u32).to_le_bytes());
        for (a1_idx, a2_idx, a1c) in &admin_tuples {
            postal_blob.extend_from_slice(&a1_idx.to_le_bytes());
            postal_blob.extend_from_slice(&a2_idx.to_le_bytes());
            postal_blob.extend_from_slice(&a1c.to_le_bytes());
        }
        postal_blob.extend_from_slice(&(sorted_postals.len() as u32).to_le_bytes());
        let mut ps_offsets: Vec<u32> = Vec::with_capacity(sorted_postals.len() + 1);
        let mut ps_body: Vec<u8> = Vec::new();
        for s in &sorted_postals {
            ps_offsets.push(ps_body.len() as u32);
            ps_body.extend_from_slice(s.as_bytes());
        }
        ps_offsets.push(ps_body.len() as u32);
        for o in &ps_offsets {
            postal_blob.extend_from_slice(&o.to_le_bytes());
        }
        postal_blob.extend_from_slice(&ps_body);

        let mut groups: BTreeMap<u32, Vec<&(i64, i64, u32, u32)>> = BTreeMap::new();
        for entry in &entries {
            let cell = pcell_of(entry.0 as f64 / 1e6, entry.1 as f64 / 1e6);
            groups.entry(cell).or_default().push(entry);
        }
        let mut body: Vec<u8> = Vec::new();
        let mut cell_dir: Vec<(u32, u32)> = Vec::new();
        for (cell, list) in &groups {
            let offset = body.len() as u32;
            cell_dir.push((*cell, offset));
            write_varint(&mut body, list.len() as u64);
            let mut prev_lat = 0i64;
            let mut prev_lon = 0i64;
            for (lat, lon, postal_idx, tuple_idx) in list {
                write_varint(&mut body, zigzag(lat - prev_lat));
                write_varint(&mut body, zigzag(lon - prev_lon));
                prev_lat = *lat;
                prev_lon = *lon;
                write_varint(&mut body, new_idx[*postal_idx as usize] as u64);
                write_varint(&mut body, *tuple_idx as u64);
            }
        }
        write_varint(&mut postal_blob, cell_dir.len() as u64);
        let mut prev_cell = 0u32;
        let mut prev_offset = 0u32;
        for (cell, offset) in &cell_dir {
            write_varint(&mut postal_blob, (*cell - prev_cell) as u64);
            write_varint(&mut postal_blob, (*offset - prev_offset) as u64);
            prev_cell = *cell;
            prev_offset = *offset;
        }
        postal_blob.extend_from_slice(&body);
        let end = postal_blob.len() as u32;
        postal_dir.push((*cc, start, end));
    }

    let mut poly_blob = Vec::new();
    let mut poly_dir: Vec<(u16, u32, u32)> = Vec::new();
    for polygon in &input.polys {
        let cc = *cc_to_idx.get(&polygon.iso2).unwrap_or(&0);
        if cc == 0 {
            continue;
        }
        let start = poly_blob.len() as u32;
        write_varint(&mut poly_blob, polygon.rings.len() as u64);
        for ring in &polygon.rings {
            write_varint(&mut poly_blob, ring.len() as u64);
            let mut min_lat = i32::MAX;
            let mut max_lat = i32::MIN;
            let mut min_lon = i32::MAX;
            let mut max_lon = i32::MIN;
            for &(lat, lon) in ring {
                let li = (lat * 1e5) as i32;
                let lo = (lon * 1e5) as i32;
                min_lat = min_lat.min(li);
                max_lat = max_lat.max(li);
                min_lon = min_lon.min(lo);
                max_lon = max_lon.max(lo);
            }
            poly_blob.extend_from_slice(&min_lat.to_le_bytes());
            poly_blob.extend_from_slice(&max_lat.to_le_bytes());
            poly_blob.extend_from_slice(&min_lon.to_le_bytes());
            poly_blob.extend_from_slice(&max_lon.to_le_bytes());
            let mut prev_lat = 0i64;
            let mut prev_lon = 0i64;
            for &(lat, lon) in ring {
                let li = (lat * 1e5) as i64;
                let lo = (lon * 1e5) as i64;
                write_varint(&mut poly_blob, zigzag(li - prev_lat));
                write_varint(&mut poly_blob, zigzag(lo - prev_lon));
                prev_lat = li;
                prev_lon = lo;
            }
        }
        let end = poly_blob.len() as u32;
        poly_dir.push((cc, start, end));
    }

    let strings_blob = encode_strings(&strings.list);

    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&1u32.to_le_bytes());

    let mut placeholders = Vec::new();
    for _ in 0..12 {
        placeholders.push(out.len());
        out.extend_from_slice(&0u32.to_le_bytes());
    }
    while out.len() % 8 != 0 {
        out.push(0);
    }

    let off_str = out.len() as u32;
    out.extend_from_slice(&strings_blob);
    let len_str = out.len() as u32 - off_str;

    pad4(&mut out);
    let off_cc = out.len() as u32;
    out.extend_from_slice(&(cc_list.len() as u32).to_le_bytes());
    for code in &cc_list {
        let bytes = code.as_bytes();
        out.push(*bytes.first().unwrap_or(&0));
        out.push(*bytes.get(1).unwrap_or(&0));
    }
    let len_cc = out.len() as u32 - off_cc;

    pad4(&mut out);
    let off_grid = out.len() as u32;
    out.extend_from_slice(&(grid_entries.len() as u32).to_le_bytes());
    let mut prev_cell = 0u32;
    let mut prev_offset = 0u32;
    for (cell, offset) in &grid_entries {
        write_varint(&mut out, (*cell - prev_cell) as u64);
        write_varint(&mut out, (*offset - prev_offset) as u64);
        prev_cell = *cell;
        prev_offset = *offset;
    }
    let len_grid = out.len() as u32 - off_grid;

    let off_cities = out.len() as u32;
    out.extend_from_slice(&cities_blob);
    let len_cities = out.len() as u32 - off_cities;

    pad4(&mut out);
    let off_pdir = out.len() as u32;
    out.extend_from_slice(&(postal_dir.len() as u32).to_le_bytes());
    for (cc, start, end) in &postal_dir {
        out.extend_from_slice(&cc.to_le_bytes());
        out.extend_from_slice(&[0u8, 0]);
        out.extend_from_slice(&start.to_le_bytes());
        out.extend_from_slice(&end.to_le_bytes());
    }
    let len_pdir = out.len() as u32 - off_pdir;

    let off_postal = out.len() as u32;
    out.extend_from_slice(&postal_blob);
    let len_postal = out.len() as u32 - off_postal;

    pad4(&mut out);
    let off_polydir = out.len() as u32;
    out.extend_from_slice(&(poly_dir.len() as u32).to_le_bytes());
    for (cc, start, end) in &poly_dir {
        out.extend_from_slice(&cc.to_le_bytes());
        out.extend_from_slice(&[0u8, 0]);
        out.extend_from_slice(&start.to_le_bytes());
        out.extend_from_slice(&end.to_le_bytes());
    }
    let len_polydir = out.len() as u32 - off_polydir;

    let off_poly = out.len() as u32;
    out.extend_from_slice(&poly_blob);
    let len_poly = out.len() as u32 - off_poly;

    let header_values = [
        off_str, len_str, off_cc, len_cc, off_grid, len_grid, off_cities, len_cities, off_pdir,
        len_pdir, off_postal, len_postal,
    ];
    for (i, value) in header_values.iter().enumerate() {
        let position = placeholders[i];
        out[position..position + 4].copy_from_slice(&value.to_le_bytes());
    }

    out.extend_from_slice(&off_polydir.to_le_bytes());
    out.extend_from_slice(&len_polydir.to_le_bytes());
    out.extend_from_slice(&off_poly.to_le_bytes());
    out.extend_from_slice(&len_poly.to_le_bytes());

    out
}

fn pad4(out: &mut Vec<u8>) {
    while out.len() % 4 != 0 {
        out.push(0);
    }
}

fn encode_strings(list: &[String]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(list.len() as u32).to_le_bytes());
    let mut offsets: Vec<u32> = Vec::with_capacity(list.len() + 1);
    let mut body: Vec<u8> = Vec::new();
    for value in list {
        offsets.push(body.len() as u32);
        body.extend_from_slice(value.as_bytes());
    }
    offsets.push(body.len() as u32);
    for offset in &offsets {
        out.extend_from_slice(&offset.to_le_bytes());
    }
    out.extend_from_slice(&body);
    out
}
