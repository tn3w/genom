# genom

[![Crates.io](https://img.shields.io/crates/v/genom.svg)](https://crates.io/crates/genom)
[![Docs.rs](https://docs.rs/genom/badge.svg)](https://docs.rs/genom)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Fast offline reverse geocoding with enriched location data.

```
~37 MB embedded DB
sub-µs lookup
18 fields per result
```

## Install

```toml
[dependencies]
genom = "2"
```

## Use

```rust
use genom::lookup;

let place = lookup(40.7128, -74.0060).unwrap();
println!("{}, {}", place.city, place.country_name); // New York City, United States
println!("{} ({})", place.timezone, place.timezone_abbr); // America/New_York (EDT)
println!("{} {}", place.currency, place.postal_code); // USD 10007
```

Returns `Option<Place>` with 18 fields: `city`, `region`, `region_code`,
`district`, `country_code`, `country_name`, `postal_code`, `timezone`,
`timezone_abbr`, `utc_offset`, `utc_offset_str`, `latitude`, `longitude`,
`currency`, `continent_code`, `continent_name`, `is_eu`, `dst_active`.

## How

1. **Compile-time build** (`build.rs`): downloads GeoNames `cities500`,
   `admin1/2`, `countryInfo`, `allCountries.zip` (postal), and Natural Earth
   country polygons. Outputs a single compact binary `geo.bin` (~37 MB) into
   `OUT_DIR`.
2. **Embedded**: `geo.bin` is `include_bytes!`'d into the final binary.
3. **Lazy parse**: on first lookup, `Geocoder::global()` parses headers into
   zero-copy `&'static` slices + two `FxHashMap` indexes (grid → city
   offset, country → postal section).
4. **Lookup**: expanding ring search on a 0.1° city grid → nearest city.
   Refines postal code from a 0.01° per-country grid. Enriches with
   country/currency/continent/timezone metadata via static maps.

Binary format: varint + zigzag deltas for coords, interned strings, fixed
section headers. See `build/builder.rs`.

## CLI

```bash
cargo build --release
./target/release/genom 48.8566 2.3522
```

## Build cache

`build.rs` looks for raw GeoNames + Natural Earth downloads in `data/` first,
then falls back to network fetch into `OUT_DIR/geonames-cache/`. The repo
ships with `data/` populated so CI builds without network. `data/` is
excluded from the published crate via `Cargo.toml`'s `exclude` — end users
installing from crates.io download fresh data on first build.

## Skip the build

For docs.rs / CI without network:

```toml
[dependencies]
genom = { version = "2", features = ["no-build-database"] }
```

The database becomes empty; all lookups return `None`.

## Performance

- **First lookup**: ~5 ms (lazy parse of indexes).
- **Steady state**: sub-microsecond city scan + postal scan.
- **Memory**: ~37 MB embedded blob, kept as `&'static [u8]`. Indexes add a
  few MB of hashmaps.

## Develop

```bash
cargo build --release
cargo test --release
cargo doc --no-deps --open
```

## Data sources

- [GeoNames.org](https://www.geonames.org/) — cities, admin codes, postal — CC-BY 4.0
- [Natural Earth](https://www.naturalearthdata.com/) — country polygons — public domain

## License

Apache-2.0. See [LICENSE](LICENSE).
