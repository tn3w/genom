use std::path::{Path, PathBuf};
use std::process::ExitCode;

const DEFAULT_DB: &str = "geo.bin";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(String::as_str).unwrap_or("");
    match cmd {
        "build" => run_build(args.get(2).map(Path::new).unwrap_or(Path::new(DEFAULT_DB))),
        "check" => run_check(args.get(2).map(Path::new).unwrap_or(Path::new(DEFAULT_DB))),
        "" | "-h" | "--help" => {
            usage(&args[0]);
            ExitCode::SUCCESS
        }
        _ => run_lookup(&args),
    }
}

fn usage(bin: &str) {
    eprintln!(
        "Usage:\n  {bin} <lat> <lon>       reverse-geocode\n  \
         {bin} build [path]       build geo.bin (default: ./{DEFAULT_DB})\n  \
         {bin} check [path]       exit 0 if geo.bin exists, 1 otherwise"
    );
}

fn run_build(out: &Path) -> ExitCode {
    if genom::loader::exists(out) {
        println!("exists → {}", out.display());
        return ExitCode::SUCCESS;
    }
    let cache = cache_dir(out);
    if let Err(e) = std::fs::create_dir_all(&cache) {
        eprintln!("cache: {e}");
        return ExitCode::FAILURE;
    }
    match genom::builder::build(&cache, out) {
        Ok(()) => {
            println!("built → {}", out.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("build failed: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run_check(path: &Path) -> ExitCode {
    if genom::loader::exists(path) {
        println!("ok");
        ExitCode::SUCCESS
    } else {
        println!("missing");
        ExitCode::FAILURE
    }
}

fn cache_dir(out: &Path) -> PathBuf {
    let repo = PathBuf::from("data");
    if repo.exists() {
        return repo;
    }
    out.parent()
        .unwrap_or(Path::new("."))
        .join("genom-cache")
}

fn run_lookup(args: &[String]) -> ExitCode {
    let Some(lat) = args.get(1).and_then(|s| s.parse::<f64>().ok()) else {
        usage(&args[0]);
        return ExitCode::FAILURE;
    };
    let Some(lon) = args.get(2).and_then(|s| s.parse::<f64>().ok()) else {
        usage(&args[0]);
        return ExitCode::FAILURE;
    };
    let Some(place) = genom::lookup(lat, lon) else {
        println!("No place found");
        return ExitCode::SUCCESS;
    };
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
    ExitCode::SUCCESS
}
