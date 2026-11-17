# genom

[![Crates.io](https://img.shields.io/crates/v/genom.svg)](https://crates.io/crates/genom)
[![Docs.rs](https://docs.rs/genom/badge.svg)](https://docs.rs/genom)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Fast offline reverse geocoding. Coordinates in, nearest city out, with its region,
district, postal code, timezone and a full set of country facts.

```
38 fields per lookup
~10 µs per lookup on land
0 ms startup, nothing to parse
```

Try it in the browser: [genom.tn3w.dev](https://genom.tn3w.dev)

## Install

```toml
[dependencies]
genom = "3"
```

## Use

```rust
let place = genom::lookup(48.8566, 2.3522).unwrap();

println!("{} {}, {}", place.postal_code, place.city, place.country.name);
// 75004 Paris 04 Hôtel-de-Ville, France
println!("{} ({})", place.timezone.name, place.timezone.abbreviation);
// Europe/Paris (CET)
println!("{} {}", place.country.currency, place.country.languages[0]);
// EUR fr-FR
```

`lookup` returns `None` when no city is within about 1,100 km (open ocean).

### Fields

| Group | Fields |
| --- | --- |
| City | `city`, `city_ascii`, `geoname_id`, `feature_code`, `population`, `elevation`, `latitude`, `longitude`, `distance_km` |
| Address | `region`, `region_code`, `district`, `district_code`, `postal_code`, `locality`, `municipality` |
| `timezone` | `name`, `abbreviation`, `utc_offset`, `utc_offset_str`, `dst_active` |
| `country` | `code`, `iso3`, `iso_numeric`, `name`, `capital`, `area_km2`, `population`, `continent_code`, `continent_name`, `tld`, `currency`, `currency_name`, `phone_prefix`, `postal_code_format`, `languages`, `neighbours`, `is_eu` |

### Use a database file

Releases publish a fresh `geo.bin` daily. Load one at runtime instead of the
embedded copy:

```rust
let geocoder = genom::Geocoder::open("geo.bin")?;
let place = geocoder.lookup(40.7128, -74.0060);
```

`Geocoder::from_bytes` accepts any `Vec<u8>` or `&'static [u8]`.

## CLI

```bash
cargo install genom
genom 48.8566 2.3522            # embedded database
genom 48.8566 2.3522 geo.bin    # database file
genom build geo.bin             # build a database file
```

## Python

[`lookup.py`](lookup.py) reads the same `geo.bin` with only the standard library:

```bash
python3 lookup.py 48.8566 2.3522 geo.bin
```

## How it works

1. **Build** (`build.rs`): reads GeoNames `cities500`, `admin1`/`admin2`,
   `countryInfo` and all postal codes from `data/`, or downloads them, and writes
   `geo.bin` (~70 MB) into `OUT_DIR`.
2. **Embed**: `geo.bin` is `include_bytes!`'d into your binary.
3. **Lookup**: rings of 0.1° grid cells are scanned outward until no unscanned
   cell can hold a closer city. Distances scale longitude by `cos(latitude)` and
   wrap at ±180°, so the result is the true nearest city everywhere. The postal
   code is the nearest one of the same country within ~30 km.

The file is read in place: a sorted cell directory is binary-searched directly,
records are varint/zigzag-delta encoded, and all text is interned once.

## Skip the build

For CI or docs builds without the data:

```toml
genom = { version = "3", default-features = false, features = ["no-build-database"] }
```

Lookups then return `None`; `Geocoder::open` still works.

## Develop

```bash
cargo test --release
cargo clippy --all-targets -- -D warnings
```

## Data sources

[GeoNames](https://www.geonames.org/), CC BY 4.0: cities, admin divisions,
countries and postal codes.

## License

Apache-2.0. See [LICENSE](LICENSE).
