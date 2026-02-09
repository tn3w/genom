<p align="center">
  <img src="https://github.com/tn3w/genom/releases/download/img/banner.png" alt="genom - Fast reverse geocoding">
</p>

<h3 align="center">Fast reverse geocoding with enriched location data</h3>

<p align="center">
  Sub-millisecond coordinate lookups returning timezone, currency, region, and 16+ other fields
</p>

<p align="center">
  <img src="https://img.shields.io/crates/v/genom?style=for-the-badge&logo=rust&logoColor=white&color=f74c00" alt="Version">
  <img src="https://img.shields.io/badge/license-Apache--2.0-blue?style=for-the-badge" alt="License">
  <img src="https://img.shields.io/docsrs/genom?style=for-the-badge&logo=rust&logoColor=white" alt="Docs">
</p>

<p align="center">
  <a href="#quick-start">🚀 Quick Start</a> •
  <a href="#features">✨ Features</a> •
  <a href="#examples">💡 Examples</a> •
  <a href="https://genom.tn3w.dev/docs">📚 Docs</a>
</p>

---

## Overview

**genom** converts latitude/longitude coordinates into detailed place information including city, region, country, timezone, currency, postal code, and more. Built for high-performance applications that need offline geocoding with rich metadata.

Whether you're enriching analytics data, adding geographic context to logs, or building location-aware APIs — genom gives you comprehensive location data with zero external API calls.

```rust
use genom;

// Single function call returns 18 fields
let place = genom::lookup(40.7128, -74.0060)?;
println!("{}, {}", place.city, place.country_name);
// Output: New York, United States

println!("Timezone: {} ({})", place.timezone, place.timezone_abbr);
// Output: Timezone: America/New_York (EST)

println!("Currency: {} | EU: {}", place.currency, place.is_eu);
// Output: Currency: USD | EU: false
```

## ✨ Features

<table>
<tr>
<td width="50%">

### ⚡ Blazing Fast

Grid-based spatial indexing delivers consistent sub-millisecond performance. Optimized binary format with string interning keeps memory usage minimal (~150 MB).

</td>
<td width="50%">

### 🎯 Rich Data

18 fields per location including timezone, currency, postal code, region, continent, EU membership, and DST status. Everything you need in one call.

</td>
</tr>
<tr>
<td width="50%">

### 🔧 Zero Setup

Database builds automatically on first install from GeoNames data. No downloads, no configuration files, no external dependencies. Just add and use.

</td>
<td width="50%">

### 🧵 Thread-Safe

Global singleton with lazy initialization. Lock-free reads mean zero contention in multi-threaded applications. Safe by default.

</td>
</tr>
</table>

### Complete Location Data

Every lookup returns comprehensive information organized into logical categories:

**Location**: `city`, `region`, `region_code`, `district`, `postal_code`

**Geography**: `country_code`, `country_name`, `continent_code`, `continent_name`, `is_eu`

**Time & Currency**: `timezone`, `timezone_abbr`, `utc_offset`, `utc_offset_str`, `dst_active`, `currency`

**Coordinates**: `latitude`, `longitude`

## 🚀 Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
genom = "1.0"
```

Or via cargo:

```bash
cargo add genom
```

Basic usage:

```rust
use genom;

fn main() {
    // Look up coordinates
    if let Some(place) = genom::lookup(48.8566, 2.3522) {
        println!("{}, {}", place.city, place.country_name);
        // Paris, France
    }
}
```

## 💡 Examples

### Basic Lookup

```rust
use genom;

// Tokyo coordinates
let place = genom::lookup(35.6762, 139.6503)?;

println!("City: {}", place.city);                    // Tokyo
println!("Country: {}", place.country_name);         // Japan
println!("Timezone: {}", place.timezone);            // Asia/Tokyo
println!("Currency: {}", place.currency);            // JPY
```

### Complete Location Context

```rust
use genom;

// London coordinates
let place = genom::lookup(51.5074, -0.1278)?;

// Location information
println!("City: {}", place.city);                    // London
println!("Region: {}", place.region);                // England
println!("Postal Code: {}", place.postal_code);      // EC1A

// Geographic details
println!("Country: {} ({})", 
    place.country_name, place.country_code);         // United Kingdom (GB)
println!("Continent: {}", place.continent_name);     // Europe
println!("EU Member: {}", place.is_eu);              // false

// Timezone information
println!("Timezone: {}", place.timezone);            // Europe/London
println!("TZ Abbr: {}", place.timezone_abbr);        // GMT or BST
println!("UTC Offset: {}", place.utc_offset_str);    // UTC+0 or UTC+1
println!("DST Active: {}", place.dst_active);        // true/false

// Economic data
println!("Currency: {}", place.currency);            // GBP
```

### Multi-threaded Usage

```rust
use std::thread;
use genom;

fn main() {
    let coordinates = vec![
        (40.7128, -74.0060),  // New York
        (51.5074, -0.1278),   // London
        (35.6762, 139.6503),  // Tokyo
    ];

    let handles: Vec<_> = coordinates
        .into_iter()
        .map(|(lat, lon)| {
            thread::spawn(move || {
                genom::lookup(lat, lon)
            })
        })
        .collect();

    for handle in handles {
        if let Some(place) = handle.join().unwrap() {
            println!("{}: {}", place.city, place.timezone);
        }
    }
}
```

### Distance Calculations

```rust
use genom::Location;

let nyc = Location::new(40.7128, -74.0060);
let la = Location::new(34.0522, -118.2437);

let distance = nyc.distance_to(&la);
println!("Distance: {:.0} km", distance);  // ~3944 km
```

### CLI Tool

```bash
# Build and run
cargo build --release
./target/release/genom 40.7128 -74.0060

# Output:
# New York
#   Region: New York
#   Country: United States (US)
#   Timezone: America/New_York (EST) UTC-5
#   Currency: USD
#   EU Member: false
```

## ⚡ Performance

- **First lookup**: ~100ms (database initialization)
- **Subsequent lookups**: <1ms (typically 0.1-0.5ms)
- **Memory usage**: ~150 MB (embedded database in memory)
- **Binary size increase**: ~150 MB (embedded database)

The database is initialized lazily on first use and cached in a static `OnceLock`, making it safe and efficient for concurrent access.

### Lookup Algorithm

1. Quantize input coordinates to grid key (0.1° resolution)
2. Search target cell and 8 neighboring cells (3×3 grid)
3. Calculate haversine distance to all candidates
4. Return nearest place with enriched metadata

This provides O(1) average-case lookup with typically 10-50 candidates to check.

## 🛠️ Build Process

On first `cargo build`, the library automatically:

1. Downloads geographic data from [GeoNames.org](https://www.geonames.org/)
2. Processes and filters place data (cities, towns, villages)
3. Merges postal code information
4. Deduplicates nearby entries
5. Builds string interning table and spatial index
6. Serializes to compact binary format

This takes 2-5 minutes depending on network speed. The database is cached in `target/` and only rebuilt when necessary.

### Skipping the Build

To skip database generation (e.g., for docs.rs or CI):

```toml
[dependencies]
genom = { version = "1.0", features = ["no-build-database"] }
```

## 🔍 Use Cases

- **Analytics**: Enrich user location data with timezone and region
- **Logging**: Add geographic context to log entries
- **APIs**: Convert coordinates to human-readable locations
- **IoT**: Offline geocoding for edge devices
- **Mobile**: Embedded geocoding without network requests
- **Privacy**: No external API calls, all data stays local

## 📊 Data Structure

The library uses a pre-built binary database embedded in your compiled binary:

- **String Interning**: Common strings (country codes, timezones) stored once
- **Fixed-Point Coordinates**: 32-bit integers instead of 64-bit floats
- **Spatial Grid Index**: World divided into 0.1° × 0.1° cells (~11km at equator)

This reduces memory footprint by approximately 70% compared to storing full structs.

## ⚠️ Limitations

- **Ocean coordinates**: Returns `None` for coordinates far from land
- **Precision**: Nearest city/town, not street-level accuracy
- **Coverage**: Limited to 100+ countries with significant population
- **Updates**: Database is static; requires rebuild for updated data
- **Size**: Adds ~150 MB to your binary

## 📄 Data Sources

All geographic data comes from [GeoNames.org](https://www.geonames.org/), which provides free geographic data under the [Creative Commons Attribution 4.0 License](https://creativecommons.org/licenses/by/4.0/).

## 📚 API Reference

### Core Functions

- `lookup(latitude: f64, longitude: f64) -> Option<Place>` - Main entry point for geocoding

### Types

- `Place` - Enriched output with 18 fields of location data
- `Location` - Coordinate pair with distance calculations
- `Geocoder` - Core geocoding engine (usually accessed via `lookup`)

See [full documentation](https://genom.tn3w.dev/docs) for detailed API reference.

## 🤝 Contributing

Contributions welcome! Please:

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Submit a pull request

## 📄 License

Copyright 2026 TN3W

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.

---

<p align="center">
  <a href="https://genom.tn3w.dev">🌐 Website</a> •
  <a href="https://genom.tn3w.dev/docs">📚 Documentation</a> •
  <a href="https://github.com/tn3w/genom">💻 GitHub</a> •
  <a href="https://crates.io/crates/genom">📦 crates.io</a>
</p>

<p align="center">
  <sub>Built with data from GeoNames.org</sub>
</p>
