fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 3 {
        eprintln!("Usage: {} <lat> <lon>", args[0]);
        return Ok(());
    }

    let lat: f64 = args[1].parse()?;
    let lon: f64 = args[2].parse()?;

    if let Some(place) = genom::lookup(lat, lon) {
        println!("{}", place.city);
        println!("  Region: {}", place.region);
        println!("  Region Code: {}", place.region_code);
        println!("  District: {}", place.district);
        println!("  Country: {} ({})", place.country_name, place.country_code);
        println!(
            "  Continent: {} ({})",
            place.continent_name, place.continent_code
        );
        println!("  Postal Code: {}", place.postal_code);
        println!(
            "  Timezone: {} ({}) {}",
            place.timezone, place.timezone_abbr, place.utc_offset_str
        );
        println!("  UTC Offset: {} seconds", place.utc_offset);
        println!("  DST Active: {}", place.dst_active);
        println!("  Currency: {}", place.currency);
        println!("  EU Member: {}", place.is_eu);
        println!("  Coords: {}, {}", place.latitude, place.longitude);
    } else {
        println!("No place found");
    }

    Ok(())
}
