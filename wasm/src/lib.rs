use wasm_bindgen::prelude::*;
mod migrate;

#[wasm_bindgen]
pub async fn migrate(input: String) -> String {
    migrate::migrate_wasm(input).await
}
