#[path = "builder.rs"]
mod builder;

use std::env;
use std::path::Path;
use std::process::ExitCode;

use genom::Geocoder;

const USAGE: &str = "Usage:
  genom <latitude> <longitude> [geo.bin]   reverse-geocode
  genom build [geo.bin]                    build the database";

fn main() -> ExitCode {
    let arguments: Vec<String> = env::args().skip(1).collect();
    let arguments: Vec<&str> = arguments.iter().map(String::as_str).collect();
    let result = match arguments.as_slice() {
        ["build"] => build("geo.bin"),
        ["build", output] => build(output),
        [latitude, longitude] => lookup(latitude, longitude, None),
        [latitude, longitude, database] => lookup(latitude, longitude, Some(database)),
        _ => Err(USAGE.to_string()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn build(output: &str) -> Result<(), String> {
    let output = Path::new(output);
    let sources = match Path::new("data").exists() {
        true => Path::new("data").to_path_buf(),
        false => output.with_file_name("genom-sources"),
    };
    builder::build(&sources, output).map_err(|error| format!("build failed: {error}"))?;
    println!("built {}", output.display());
    Ok(())
}

fn lookup(latitude: &str, longitude: &str, database: Option<&str>) -> Result<(), String> {
    let (Ok(latitude), Ok(longitude)) = (latitude.parse(), longitude.parse()) else {
        return Err(USAGE.to_string());
    };
    let place = match database {
        Some(path) => Geocoder::open(path)
            .map_err(|error| format!("{path}: {error}"))?
            .lookup(latitude, longitude),
        None => genom::lookup(latitude, longitude),
    };
    match place {
        Some(place) => println!("{place:#?}"),
        None => println!("no place found"),
    }
    Ok(())
}
