#[cfg(feature = "builder")]
#[path = "../../build/builder.rs"]
mod builder;

#[cfg(feature = "builder")]
#[path = "../../build/types.rs"]
mod types;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(not(feature = "builder"))]
    {
        eprintln!("Error: This binary requires the 'builder' feature.");
        eprintln!("Build with: cargo run --release --bin build-database --features builder");
        std::process::exit(1);
    }

    #[cfg(feature = "builder")]
    {
        use builder::Builder;

        let output_path = std::env::args()
            .nth(1)
            .unwrap_or_else(|| "places.bin".to_string());

        println!("Building database to: {}", output_path);

        Builder::new().build(&output_path)?;

        println!("Database built successfully!");
        Ok(())
    }
}
