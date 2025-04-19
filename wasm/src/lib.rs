use console_log;
use log::Level;
use wasm_bindgen::prelude::*;
mod cache;
mod migrate;
mod query;

#[wasm_bindgen(start)]
pub fn start() {
    console_log::init_with_level(Level::Info).expect("error initializing log");
}

#[wasm_bindgen]
pub async fn migrate(introspection: JsValue, schema_source: String) -> String {
    migrate::migrate_wasm(introspection, schema_source).await
}

#[wasm_bindgen]
pub async fn run_query(introspection: JsValue, query_source: String) -> String {
    query::run_query(introspection, query_source).await
}
