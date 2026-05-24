//! Embedded database parser and lookup engine.

#![warn(missing_docs)]

use std::sync::OnceLock;

use rustc_hash::FxHashMap;

use crate::enrichment::{PlaceInput, enrich_place};
use crate::types::{Location, Place};

#[cfg(not(any(doc, clippy, feature = "no-build-database")))]
static DATA: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/geo.bin"));

#[cfg(any(doc, clippy, feature = "no-build-database"))]
static DATA: &[u8] = &[];

static GEOCODER: OnceLock<Geocoder> = OnceLock::new();

const GRID_LON: i32 = 3600;
const GRID_LAT: i32 = 1800;
const GRID_SCALE: f64 = 10.0;
const PGRID_LON: i32 = 36000;
const PGRID_LAT: i32 = 18000;
const PGRID_SCALE: f64 = 100.0;

/// Reverse-geocoding engine backed by the embedded binary database.
pub struct Geocoder {
    db: Db,
}

impl Geocoder {
    /// Lazily-initialized global instance. Cheap to call repeatedly.
    pub fn global() -> &'static Self {
        GEOCODER.get_or_init(|| Self::from_bytes(DATA))
    }

    /// Construct a geocoder from a previously-built `geo.bin` byte slice.
    ///
    /// Pair with [`crate::loader::load_from_file`] for disk-backed databases.
    pub fn from_bytes(data: &'static [u8]) -> Self {
        Self { db: Db::parse(data) }
    }

    /// Reverse-geocode `(latitude, longitude)` into a fully enriched [`Place`].
    pub fn lookup(&self, latitude: f64, longitude: f64) -> Option<Place> {
        let (_, city) = self.db.nearest_city(latitude, longitude)?;
        let (postal_code, _, _, _) = self
            .db
            .nearest_postal(city.cc, latitude, longitude)
            .unwrap_or(("", 0, 0, 0));
        Some(enrich_place(PlaceInput {
            city: self.db.str_at(city.name_i),
            region: self.db.str_at(city.a1_i),
            region_code: self.db.str_at(city.a1c_i),
            district: self.db.str_at(city.a2_i),
            country_code: self.db.cc_iso(city.cc),
            postal_code,
            timezone: self.db.str_at(city.tz_i),
            latitude: city.lat as f64 / 1e6,
            longitude: city.lon as f64 / 1e6,
        }))
    }
}

struct Db {
    str_offsets: &'static [u32],
    str_body: &'static [u8],
    cc_blob: &'static [u8],
    grid: FxHashMap<u32, u32>,
    cities: &'static [u8],
    postal_by_cc: FxHashMap<u16, PostalCountry>,
}

struct PostalCountry {
    tuples: &'static [u8],
    ps_offsets: &'static [u32],
    ps_body: &'static [u8],
    cell_dir: FxHashMap<u32, u32>,
    body: &'static [u8],
}

struct CityRec {
    lat: i64,
    lon: i64,
    name_i: u32,
    a1_i: u32,
    a2_i: u32,
    a1c_i: u32,
    tz_i: u32,
    cc: u16,
}

fn u32_le(buffer: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(buffer[offset..offset + 4].try_into().unwrap())
}

fn read_varint(buffer: &[u8]) -> (u64, usize) {
    let mut value = 0u64;
    let mut shift = 0;
    let mut index = 0;
    loop {
        let byte = buffer[index];
        index += 1;
        value |= ((byte & 0x7f) as u64) << shift;
        if byte & 0x80 == 0 {
            return (value, index);
        }
        shift += 7;
    }
}

fn zigzag_decode(value: u64) -> i64 {
    ((value >> 1) as i64) ^ -((value & 1) as i64)
}

fn pcell_of(latitude: f64, longitude: f64) -> u32 {
    let la = (((latitude + 90.0) * PGRID_SCALE).floor() as i32).clamp(0, PGRID_LAT - 1);
    let lo = (((longitude + 180.0) * PGRID_SCALE).floor() as i32).clamp(0, PGRID_LON - 1);
    (la * PGRID_LON + lo) as u32
}

fn cell_of(latitude: f64, longitude: f64) -> u32 {
    let la = (((latitude + 90.0) * GRID_SCALE).floor() as i32).clamp(0, GRID_LAT - 1);
    let lo = (((longitude + 180.0) * GRID_SCALE).floor() as i32).clamp(0, GRID_LON - 1);
    (la * GRID_LON + lo) as u32
}

impl Db {
    fn parse(data: &'static [u8]) -> Self {
        if data.len() < 64 || &data[..4] != b"GEO1" {
            return Self::empty();
        }
        let off_str = u32_le(data, 8) as usize;
        let off_cc = u32_le(data, 16) as usize;
        let len_cc = u32_le(data, 20) as usize;
        let off_grid = u32_le(data, 24) as usize;
        let off_cities = u32_le(data, 32) as usize;
        let len_cities = u32_le(data, 36) as usize;
        let off_pdir = u32_le(data, 40) as usize;
        let off_postal = u32_le(data, 48) as usize;

        let str_count = u32_le(data, off_str) as usize;
        let offsets_start = off_str + 4;
        let offsets_end = offsets_start + 4 * (str_count + 1);
        let str_offsets_bytes = &data[offsets_start..offsets_end];
        let str_offsets: &[u32] = unsafe {
            std::slice::from_raw_parts(str_offsets_bytes.as_ptr() as *const u32, str_count + 1)
        };
        let str_body = &data[offsets_end..];
        let cc_blob = &data[off_cc + 4..off_cc + len_cc];

        let grid_count = u32_le(data, off_grid) as usize;
        let mut grid: FxHashMap<u32, u32> = FxHashMap::default();
        grid.reserve(grid_count);
        let mut index = off_grid + 4;
        let mut prev_cell = 0u32;
        let mut prev_offset = 0u32;
        for _ in 0..grid_count {
            let (delta_cell, k) = read_varint(&data[index..]);
            index += k;
            let (delta_offset, k) = read_varint(&data[index..]);
            index += k;
            prev_cell += delta_cell as u32;
            prev_offset += delta_offset as u32;
            grid.insert(prev_cell, prev_offset);
        }

        let pdir_count = u32_le(data, off_pdir) as usize;
        let mut postal_by_cc: FxHashMap<u16, PostalCountry> = FxHashMap::default();
        let mut pointer = off_pdir + 4;
        for _ in 0..pdir_count {
            let cc = u16::from_le_bytes([data[pointer], data[pointer + 1]]);
            let start = u32_le(data, pointer + 4) as usize;
            let end = u32_le(data, pointer + 8) as usize;
            pointer += 12;
            let section = &data[off_postal + start..off_postal + end];
            postal_by_cc.insert(cc, parse_postal_country(section));
        }

        Db {
            str_offsets,
            str_body,
            cc_blob,
            grid,
            cities: &data[off_cities..off_cities + len_cities],
            postal_by_cc,
        }
    }

    fn empty() -> Self {
        Db {
            str_offsets: &[],
            str_body: &[],
            cc_blob: &[],
            grid: FxHashMap::default(),
            cities: &[],
            postal_by_cc: FxHashMap::default(),
        }
    }

    fn str_at(&self, index: u32) -> &'static str {
        if (index as usize + 1) >= self.str_offsets.len() {
            return "";
        }
        let start = self.str_offsets[index as usize] as usize;
        let end = self.str_offsets[index as usize + 1] as usize;
        std::str::from_utf8(&self.str_body[start..end]).unwrap_or("")
    }

    fn cc_iso(&self, index: u16) -> &'static str {
        let position = (index as usize) * 2;
        if position + 2 > self.cc_blob.len() {
            return "";
        }
        std::str::from_utf8(&self.cc_blob[position..position + 2]).unwrap_or("")
    }

    fn nearest_city(&self, latitude: f64, longitude: f64) -> Option<(f64, CityRec)> {
        if self.cities.is_empty() {
            return None;
        }
        let lat_q = (latitude * 1e6) as i64;
        let lon_q = (longitude * 1e6) as i64;
        let cell = cell_of(latitude, longitude);
        let base_la = (cell / GRID_LON as u32) as i32;
        let base_lo = (cell % GRID_LON as u32) as i32;
        let mut best: Option<(i64, CityRec)> = None;
        let mut radius = 1i32;
        loop {
            for delta_la in -radius..=radius {
                for delta_lo in -radius..=radius {
                    let la = base_la + delta_la;
                    let lo = base_lo + delta_lo;
                    if !(0..GRID_LAT).contains(&la) || !(0..GRID_LON).contains(&lo) {
                        continue;
                    }
                    let cell = (la * GRID_LON + lo) as u32;
                    let Some(&offset) = self.grid.get(&cell) else {
                        continue;
                    };
                    self.scan_cell(offset, lat_q, lon_q, &mut best);
                }
            }
            if best.is_some() || radius > 200 {
                break;
            }
            radius = (radius * 2).max(radius + 1);
        }
        best.map(|(d2, rec)| ((d2 as f64).sqrt() / 1e6, rec))
    }

    fn scan_cell(&self, offset: u32, lat_q: i64, lon_q: i64, best: &mut Option<(i64, CityRec)>) {
        let mut index = offset as usize;
        let (count, k) = read_varint(&self.cities[index..]);
        index += k;
        let mut lat = 0i64;
        let mut lon = 0i64;
        for _ in 0..count {
            let (delta_lat, k) = read_varint(&self.cities[index..]);
            index += k;
            let (delta_lon, k) = read_varint(&self.cities[index..]);
            index += k;
            lat += zigzag_decode(delta_lat);
            lon += zigzag_decode(delta_lon);
            let (name_i, k) = read_varint(&self.cities[index..]);
            index += k;
            let (a1_i, k) = read_varint(&self.cities[index..]);
            index += k;
            let (a2_i, k) = read_varint(&self.cities[index..]);
            index += k;
            let (a1c_i, k) = read_varint(&self.cities[index..]);
            index += k;
            let (tz_i, k) = read_varint(&self.cities[index..]);
            index += k;
            let (cc, k) = read_varint(&self.cities[index..]);
            index += k;
            let dx = lat - lat_q;
            let dy = lon - lon_q;
            let distance_squared = dx * dx + dy * dy;
            let take = match best {
                None => true,
                Some((current, _)) => distance_squared < *current,
            };
            if take {
                *best = Some((
                    distance_squared,
                    CityRec {
                        lat,
                        lon,
                        name_i: name_i as u32,
                        a1_i: a1_i as u32,
                        a2_i: a2_i as u32,
                        a1c_i: a1c_i as u32,
                        tz_i: tz_i as u32,
                        cc: cc as u16,
                    },
                ));
            }
        }
    }

    fn nearest_postal(
        &self,
        cc: u16,
        latitude: f64,
        longitude: f64,
    ) -> Option<(&'static str, u32, u32, u32)> {
        let country = self.postal_by_cc.get(&cc)?;
        let lat_q = (latitude * 1e6) as i64;
        let lon_q = (longitude * 1e6) as i64;
        let cell = pcell_of(latitude, longitude);
        let base_la = (cell / PGRID_LON as u32) as i32;
        let base_lo = (cell % PGRID_LON as u32) as i32;
        let mut best: Option<(i64, u32, u32)> = None;
        let mut radius = 1i32;
        loop {
            for delta_la in -radius..=radius {
                for delta_lo in -radius..=radius {
                    let la = base_la + delta_la;
                    let lo = base_lo + delta_lo;
                    if !(0..PGRID_LAT).contains(&la) || !(0..PGRID_LON).contains(&lo) {
                        continue;
                    }
                    let cell = (la * PGRID_LON + lo) as u32;
                    if let Some(&offset) = country.cell_dir.get(&cell) {
                        scan_postal_cell(&country.body[offset as usize..], lat_q, lon_q, &mut best);
                    }
                }
            }
            if best.is_some() || radius > 1000 {
                break;
            }
            radius = (radius * 2).max(radius + 1);
        }
        let (_, postal_string_index, tuple_index) = best?;
        let tuple_offset = (tuple_index as usize) * 12;
        let a1 = u32_le(country.tuples, tuple_offset);
        let a2 = u32_le(country.tuples, tuple_offset + 4);
        let a1c = u32_le(country.tuples, tuple_offset + 8);
        let start = country.ps_offsets[postal_string_index as usize] as usize;
        let end = country.ps_offsets[postal_string_index as usize + 1] as usize;
        let postal_str = std::str::from_utf8(&country.ps_body[start..end]).unwrap_or("");
        Some((postal_str, a1, a2, a1c))
    }
}

fn parse_postal_country(section: &'static [u8]) -> PostalCountry {
    let mut index = 0;
    let tuple_count = u32_le(section, index) as usize;
    index += 4;
    let tuples = &section[index..index + tuple_count * 12];
    index += tuple_count * 12;
    let ps_count = u32_le(section, index) as usize;
    index += 4;
    let ps_offsets_bytes = &section[index..index + 4 * (ps_count + 1)];
    let ps_offsets: &[u32] = unsafe {
        std::slice::from_raw_parts(ps_offsets_bytes.as_ptr() as *const u32, ps_count + 1)
    };
    index += 4 * (ps_count + 1);
    let body_len = *ps_offsets.last().unwrap() as usize;
    let ps_body = &section[index..index + body_len];
    index += body_len;
    let (cell_count, k) = read_varint(&section[index..]);
    index += k;
    let mut cell_dir: FxHashMap<u32, u32> = FxHashMap::default();
    cell_dir.reserve(cell_count as usize);
    let mut prev_cell = 0u32;
    let mut prev_offset = 0u32;
    for _ in 0..cell_count {
        let (delta_cell, k) = read_varint(&section[index..]);
        index += k;
        let (delta_offset, k) = read_varint(&section[index..]);
        index += k;
        prev_cell += delta_cell as u32;
        prev_offset += delta_offset as u32;
        cell_dir.insert(prev_cell, prev_offset);
    }
    let body = &section[index..];
    PostalCountry {
        tuples,
        ps_offsets,
        ps_body,
        cell_dir,
        body,
    }
}

fn scan_postal_cell(buffer: &[u8], lat_q: i64, lon_q: i64, best: &mut Option<(i64, u32, u32)>) {
    let mut index = 0;
    let (count, k) = read_varint(&buffer[index..]);
    index += k;
    let mut lat = 0i64;
    let mut lon = 0i64;
    for _ in 0..count {
        let (delta_lat, k) = read_varint(&buffer[index..]);
        index += k;
        let (delta_lon, k) = read_varint(&buffer[index..]);
        index += k;
        lat += zigzag_decode(delta_lat);
        lon += zigzag_decode(delta_lon);
        let (postal_index, k) = read_varint(&buffer[index..]);
        index += k;
        let (tuple_index, k) = read_varint(&buffer[index..]);
        index += k;
        let dx = lat - lat_q;
        let dy = lon - lon_q;
        let distance_squared = dx * dx + dy * dy;
        if best.is_none_or(|(current, _, _)| distance_squared < current) {
            *best = Some((distance_squared, postal_index as u32, tuple_index as u32));
        }
    }
}

impl Location {
    /// Convenience: reverse-geocode this location via the global geocoder.
    pub fn lookup(&self) -> Option<Place> {
        Geocoder::global().lookup(self.latitude, self.longitude)
    }
}
