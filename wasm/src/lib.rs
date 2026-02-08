use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn decompress_xz(compressed: &[u8]) -> Result<Vec<u8>, String> {
    liblzma::decode_all(compressed).map_err(|e| format!("Decompression failed: {:?}", e))
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
