fn main() {
    let geo = genom_golf::Geo::open("geo.bin").unwrap();
    for (la, lo) in &[(40.7128, -74.0060), (48.8566, 2.3522), (35.6762, 139.6503)] {
        let p = geo.lookup(*la, *lo).expect("lookup");
        println!("{} {} {} {}", p.city, p.country_code, p.postal_code, p.timezone);
    }
}
