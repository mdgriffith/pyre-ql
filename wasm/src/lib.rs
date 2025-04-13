use wasm_bindgen::prelude::*;
mod migrate;

#[wasm_bindgen]
pub async fn migrate(introspection: JsValue, schema_source: String) -> String {
    migrate::migrate_wasm_direct(introspection, schema_source).await
}
