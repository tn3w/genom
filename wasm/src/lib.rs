use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn decompress_xz(compressed: &[u8]) -> Result<Vec<u8>, String> {
    let mut decompressed = Vec::new();
    lzma_rs::xz_decompress(&mut &compressed[..], &mut decompressed)
        .map_err(|e| format!("Decompression failed: {}", e))?;
    Ok(decompressed)
}

#[wasm_bindgen]
pub fn init_geocoder(data: &[u8]) -> Result<(), String> {
    genom::wasm::WasmGeocoder::init(data)
}

#[wasm_bindgen]
pub fn lookup(latitude: f64, longitude: f64) -> JsValue {
    match genom::wasm::WasmGeocoder::lookup(latitude, longitude) {
        Some(place) => serde_wasm_bindgen::to_value(&place).unwrap_or(JsValue::NULL),
        None => JsValue::NULL,
    }
}
