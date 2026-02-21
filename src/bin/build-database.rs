use std::path::PathBuf;

#[path = "../../build/builder.rs"]
mod builder;
#[path = "../../build/types.rs"]
mod types;

use builder::Builder;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "places.bin".to_string());

    println!("Building database to: {}", output_path);
    
    Builder::new().build(&output_path)?;
    
    println!("Database built successfully!");
    Ok(())
}
