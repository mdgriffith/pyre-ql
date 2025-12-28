use console_log;
use js_sys;
use log::Level;
use serde::Serialize;
use wasm_bindgen::prelude::*;
mod cache;
mod migrate;
mod query;
mod sync;

#[wasm_bindgen(start)]
pub fn start() {
    console_log::init_with_level(Level::Info).expect("error initializing log");
}

#[wasm_bindgen]
pub fn set_schema(introspection: JsValue) -> Result<(), JsValue> {
    cache::set_schema(introspection);
    Ok(())
}

#[wasm_bindgen]
pub fn migrate(name: String, schema_source: String) -> JsValue {
    let result = migrate::migrate_wasm(name, schema_source);
    serde_wasm_bindgen::to_value(&result).unwrap()
}

#[wasm_bindgen]
pub fn query_to_sql(query_source: String) -> JsValue {
    let result = query::query_to_sql_wasm(query_source);
    serde_wasm_bindgen::to_value(&result).unwrap()
}

#[wasm_bindgen]
pub fn sql_is_initialized() -> String {
    pyre::db::introspect::IS_INITIALIZED.to_string()
}

#[wasm_bindgen]
pub fn sql_introspect() -> String {
    pyre::db::introspect::INTROSPECT_SQL.to_string()
}

#[wasm_bindgen]
pub fn sql_introspect_uninitialized() -> String {
    pyre::db::introspect::INTROSPECT_UNINITIALIZED_SQL.to_string()
}

#[wasm_bindgen]
pub fn calculate_permission_hash(table_name: String, session: JsValue) -> JsValue {
    let result = sync::calculate_permission_hash_wasm(table_name, session);
    match result {
        Ok(hash) => serde_wasm_bindgen::to_value(&hash).unwrap(),
        Err(e) => serde_wasm_bindgen::to_value(&format!("Error: {}", e)).unwrap(),
    }
}

#[wasm_bindgen]
pub fn get_sync_page_info(sync_cursor: JsValue, session: JsValue, page_size: usize) -> JsValue {
    let result = sync::get_sync_page_info_wasm(sync_cursor, session, page_size);
    match result {
        Ok(info) => {
            web_sys::console::log_1(
                &format!("Serializing WASM result with {} tables", info.tables.len()).into(),
            );
            // Serialize to JSON string first, then parse it back to JsValue
            // This works around serde_wasm_bindgen HashMap serialization issues
            let json_str = serde_json::to_string(&info).unwrap();
            web_sys::console::log_1(&format!("JSON serialization: {}", json_str).into());
            js_sys::JSON::parse(&json_str).unwrap()
        }
        Err(e) => serde_wasm_bindgen::to_value(&format!("Error: {}", e)).unwrap(),
    }
}

#[wasm_bindgen]
pub fn get_sync_status_sql(sync_cursor: JsValue, session: JsValue) -> JsValue {
    let result = sync::get_sync_status_sql_wasm(sync_cursor, session);
    match result {
        Ok(sql) => serde_wasm_bindgen::to_value(&sql).unwrap(),
        Err(e) => serde_wasm_bindgen::to_value(&format!("Error: {}", e)).unwrap(),
    }
}

#[wasm_bindgen]
pub fn get_sync_sql(
    status_rows: JsValue,
    sync_cursor: JsValue,
    session: JsValue,
    page_size: usize,
) -> JsValue {
    let result = sync::get_sync_sql_wasm(status_rows, sync_cursor, session, page_size);
    match result {
        Ok(sql_result) => {
            // Serialize to JSON string first, then parse it back to JsValue
            // This works around serde_wasm_bindgen HashMap serialization issues
            let json_str = serde_json::to_string(&sql_result).unwrap();
            js_sys::JSON::parse(&json_str).unwrap()
        }
        Err(e) => serde_wasm_bindgen::to_value(&format!("Error: {}", e)).unwrap(),
    }
}
