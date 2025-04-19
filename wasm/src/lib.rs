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
pub fn set_schema(introspection: JsValue) -> Result<(), JsValue> {
    cache::set_schema(introspection);
    // Note, we probably want to return any errors here just in case.
    Ok(())
}

#[wasm_bindgen]
pub async fn migrate(schema_source: String) -> String {
    migrate::migrate_wasm(schema_source).await
}

#[wasm_bindgen]
pub async fn run_query(query_source: String) -> String {
    query::run_query_wasm(query_source).await
}
