# Genom

Fast reverse geocoding library with enriched location data. Convert coordinates to detailed place information including timezone, currency, region, and more.

## Features

- **Simple API** - Single function call: `genom::lookup(lat, lon)`
- **Rich Data** - Returns 18+ fields including timezone, currency, postal code, region
- **Fast Lookups** - Grid-based spatial indexing for sub-millisecond queries
- **Zero Config** - Database builds automatically on first install
- **Thread-Safe** - Global singleton with lazy initialization
- **Compact** - Efficient binary format with string interning

## Installation

```toml
[dependencies]
genom = "0.1"
```

## Quick Start

```rust
use genom;

fn main() {
    // Lookup coordinates
    if let Some(place) = genom::lookup(40.7128, -74.0060) {
        println!("{}, {}", place.city, place.country_name);
        // Output: New York, United States
    }
}
```

## Returned Data

The `Place` struct contains:

```rust
pub struct Place {
    pub city: String,              // City name
    pub region: String,            // State/province name
    pub region_code: String,       // ISO region code (e.g., "NY")
    pub district: String,          // District/county
    pub country_code: String,      // ISO country code (e.g., "US")
    pub country_name: String,      // Full country name
    pub postal_code: String,       // Postal/ZIP code
    pub timezone: String,          // IANA timezone (e.g., "America/New_York")
    pub timezone_abbr: String,     // Timezone abbreviation (e.g., "EST")
    pub utc_offset: i32,           // UTC offset in seconds
    pub utc_offset_str: String,    // Formatted offset (e.g., "UTC-5")
    pub latitude: f64,             // Precise latitude
    pub longitude: f64,            // Precise longitude
    pub currency: String,          // ISO currency code (e.g., "USD")
    pub continent_code: String,    // Continent code (e.g., "NA")
    pub continent_name: String,    // Continent name
    pub is_eu: bool,               // EU member state
    pub dst_active: bool,          // Daylight saving time active
}
```

## Examples

### Basic Lookup

```rust
use genom;

let place = genom::lookup(51.5074, -0.1278)?;
println!("City: {}", place.city);
println!("Country: {}", place.country_name);
println!("Timezone: {}", place.timezone);
println!("Currency: {}", place.currency);
```

### Timezone Information

```rust
let place = genom::lookup(35.6762, 139.6503)?;
println!("Timezone: {} ({})", place.timezone, place.timezone_abbr);
println!("UTC Offset: {}", place.utc_offset_str);
println!("DST Active: {}", place.dst_active);
```

### Region Details

```rust
let place = genom::lookup(48.8566, 2.3522)?;
println!("City: {}", place.city);
println!("Region: {} ({})", place.region, place.region_code);
println!("District: {}", place.district);
println!("Postal Code: {}", place.postal_code);
```

### EU Membership Check

```rust
let place = genom::lookup(52.5200, 13.4050)?;
if place.is_eu {
    println!("{} is an EU member state", place.country_name);
}
```

## CLI Usage

```bash
# Install
cargo install genom

# Lookup coordinates
genom 40.7128 -74.0060
```

Output:
```
New York
  Region: New York
  Region Code: NY
  District: New York County
  Country: United States (US)
  Continent: North America (NA)
  Postal Code: 10007
  Timezone: America/New_York (EST) UTC-5
  UTC Offset: -18000 seconds
  DST Active: false
  Currency: USD
  EU Member: false
  Coords: 40.71427, -74.00597
```

## Data Source

Built from [GeoNames](https://www.geonames.org/) data, covering:
- 100+ countries
- Major cities and populated places
- Administrative regions
- Postal codes
- Timezone information

## Performance

- **Lookup Time**: < 1ms typical
- **Database Size**: ~50MB compressed
- **Memory Usage**: Loaded once, shared globally
- **Thread Safety**: Lock-free reads

## How It Works

1. **Build Time**: Downloads GeoNames data and builds optimized binary database
2. **Runtime**: Lazy loads database on first `lookup()` call
3. **Spatial Index**: Grid-based partitioning for O(1) coordinate lookups
4. **String Interning**: Deduplicates common strings (country names, etc.)

## License

Apache-2.0

## Links

- [Documentation](https://genom.tn3w.dev/docs)
- [Repository](https://github.com/tn3w/genom)
- [Crates.io](https://crates.io/crates/genom)
