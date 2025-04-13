use console_log;
use log::Level;
use wasm_bindgen::prelude::*;
mod migrate;

#[wasm_bindgen(start)]
pub fn start() {
    console_log::init_with_level(Level::Info).expect("error initializing log");
}

#[wasm_bindgen]
pub async fn migrate(introspection: JsValue, schema_source: String) -> String {
    migrate::migrate_wasm_direct(introspection, schema_source).await
}
