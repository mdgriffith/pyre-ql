use crate::cache;
use pyre::db::introspect;
use pyre::sync;
use serde::{Deserialize, Serialize};
use serde_wasm_bindgen;
use std::collections::HashMap;
use wasm_bindgen::prelude::*;

// WASM-compatible types for sync
#[derive(Serialize, Deserialize)]
pub struct SyncCursorWasm {
    pub tables: HashMap<String, TableCursorWasm>,
}

#[derive(Serialize, Deserialize)]
pub struct TableCursorWasm {
    pub last_seen_updated_at: Option<i64>,
    pub permission_hash: String,
}

#[derive(Serialize, Deserialize)]
pub struct SyncPageResultWasm {
    pub tables: HashMap<String, TableSyncDataWasm>,
    pub has_more: bool,
}

#[derive(Serialize, Deserialize)]
pub struct TableSyncDataWasm {
    pub rows: Vec<serde_json::Value>,
    pub permission_hash: String,
    pub last_seen_updated_at: Option<i64>,
}

#[derive(Serialize, Deserialize)]
pub struct SessionWasm {
    pub fields: HashMap<String, SessionValueWasm>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum SessionValueWasm {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

impl From<SessionValueWasm> for sync::SessionValue {
    fn from(value: SessionValueWasm) -> Self {
        match value {
            SessionValueWasm::Null => sync::SessionValue::Null,
            SessionValueWasm::Integer(i) => sync::SessionValue::Integer(i),
            SessionValueWasm::Real(f) => sync::SessionValue::Real(f),
            SessionValueWasm::Text(s) => sync::SessionValue::Text(s),
            SessionValueWasm::Blob(b) => sync::SessionValue::Blob(b),
        }
    }
}

fn convert_session_wasm_to_rust(session: &SessionWasm) -> HashMap<String, sync::SessionValue> {
    session
        .fields
        .iter()
        .map(|(k, v)| (k.clone(), sync::SessionValue::from((*v).clone())))
        .collect()
}

fn convert_cursor_wasm_to_rust(cursor: &SyncCursorWasm) -> sync::SyncCursor {
    cursor
        .tables
        .iter()
        .map(|(k, v)| {
            (
                k.clone(),
                sync::TableCursor {
                    last_seen_updated_at: v.last_seen_updated_at,
                    permission_hash: v.permission_hash.clone(),
                },
            )
        })
        .collect()
}

fn convert_result_rust_to_wasm(result: sync::SyncPageResult) -> SyncPageResultWasm {
    SyncPageResultWasm {
        tables: result
            .tables
            .into_iter()
            .map(|(k, v)| {
                (
                    k,
                    TableSyncDataWasm {
                        rows: v.rows,
                        permission_hash: v.permission_hash,
                        last_seen_updated_at: v.last_seen_updated_at,
                    },
                )
            })
            .collect(),
        has_more: result.has_more,
    }
}

/// Calculate permission hash for a table
/// Returns the permission hash as a string
pub fn calculate_permission_hash_wasm(
    table_name: String,
    session: JsValue,
) -> Result<String, String> {
    let introspection = match cache::get() {
        Some(introspection) => introspection,
        None => return Err("No schema found".to_string()),
    };

    let session_wasm: SessionWasm = serde_wasm_bindgen::from_value(session)
        .map_err(|e| format!("Failed to parse session: {}", e))?;

    let session_rust = convert_session_wasm_to_rust(&session_wasm);

    match &introspection.schema {
        introspect::SchemaResult::Success { context, .. } => {
            let table = context
                .tables
                .get(&table_name)
                .ok_or_else(|| format!("Table {} not found", table_name))?;

            let permission =
                pyre::ast::get_permissions(&table.record, &pyre::ast::QueryOperation::Select);

            let hash = sync::calculate_permission_hash(&permission, &session_rust);
            Ok(hash)
        }
        _ => Err("No schema found".to_string()),
    }
}

/// Get sync page - this is a placeholder that returns the structure
/// The actual query execution will need to be implemented separately
/// since we can't execute queries directly from WASM
pub fn get_sync_page_info_wasm(
    sync_cursor: JsValue,
    session: JsValue,
    page_size: usize,
) -> Result<SyncPageResultWasm, String> {
    let introspection = match cache::get() {
        Some(introspection) => introspection,
        None => return Err("No schema found in cache".to_string()),
    };

    let cursor_wasm: SyncCursorWasm = serde_wasm_bindgen::from_value(sync_cursor)
        .map_err(|e| format!("Failed to parse sync cursor: {}", e))?;

    let session_wasm: SessionWasm = serde_wasm_bindgen::from_value(session)
        .map_err(|e| format!("Failed to parse session: {}", e))?;

    let session_rust = convert_session_wasm_to_rust(&session_wasm);
    let cursor_rust = convert_cursor_wasm_to_rust(&cursor_wasm);

    match &introspection.schema {
        introspect::SchemaResult::Success { context, .. } => {
            web_sys::console::log_1(
                &format!("Found {} tables in context", context.tables.len()).into(),
            );
            let result = sync::get_sync_page_info(&cursor_rust, context, &session_rust, page_size);
            web_sys::console::log_1(
                &format!("Sync result has {} tables", result.tables.len()).into(),
            );
            for (table_name, _) in &result.tables {
                web_sys::console::log_1(&format!("  Table: {}", table_name).into());
            }
            let wasm_result = convert_result_rust_to_wasm(result);
            web_sys::console::log_1(
                &format!("WASM result has {} tables", wasm_result.tables.len()).into(),
            );
            for (table_name, _) in &wasm_result.tables {
                web_sys::console::log_1(&format!("  WASM Table: {}", table_name).into());
            }
            Ok(wasm_result)
        }
        introspect::SchemaResult::FailedToParse { source, errors } => Err(format!(
            "Schema failed to parse: {} errors. Source length: {}",
            errors.len(),
            source.len()
        )),
        introspect::SchemaResult::FailedToTypecheck { errors, .. } => Err(format!(
            "Schema failed to typecheck: {} errors",
            errors.len()
        )),
    }
}

/// Generate sync SQL for all tables
/// Returns SQL statements for each table that needs syncing
#[derive(Serialize, Deserialize)]
pub struct SyncSqlResultWasm {
    pub tables: Vec<TableSyncSqlWasm>,
}

#[derive(Serialize, Deserialize)]
pub struct TableSyncSqlWasm {
    pub table_name: String,
    pub permission_hash: String,
    pub sql: Vec<String>,
    pub headers: Vec<String>,
}

/// Generate sync status SQL - returns a single SQL query that checks which tables need syncing
pub fn get_sync_status_sql_wasm(sync_cursor: JsValue, session: JsValue) -> Result<String, String> {
    let introspection = match cache::get() {
        Some(introspection) => introspection,
        None => return Err("No schema found".to_string()),
    };

    let cursor_wasm: SyncCursorWasm = serde_wasm_bindgen::from_value(sync_cursor)
        .map_err(|e| format!("Failed to parse sync cursor: {}", e))?;

    let session_wasm: SessionWasm = serde_wasm_bindgen::from_value(session)
        .map_err(|e| format!("Failed to parse session: {}", e))?;

    let session_rust = convert_session_wasm_to_rust(&session_wasm);
    let cursor_rust = convert_cursor_wasm_to_rust(&cursor_wasm);

    match &introspection.schema {
        introspect::SchemaResult::Success { context, .. } => {
            sync::get_sync_status_sql(&cursor_rust, context, &session_rust)
                .map_err(|e| format!("{}", e))
        }
        _ => Err("No schema found".to_string()),
    }
}

/// Parse sync status results from SQL query execution
#[derive(Serialize, Deserialize)]
pub struct SyncStatusResultWasm {
    pub tables: Vec<TableSyncStatusWasm>,
}

#[derive(Serialize, Deserialize)]
pub struct TableSyncStatusWasm {
    pub table_name: String,
    pub sync_layer: usize,
    pub needs_sync: bool,
    pub max_updated_at: Option<i64>,
    pub permission_hash: String,
}

pub fn parse_sync_status_wasm(
    sync_cursor: JsValue,
    session: JsValue,
    rows: JsValue,
) -> Result<SyncStatusResultWasm, String> {
    let introspection = match cache::get() {
        Some(introspection) => introspection,
        None => return Err("No schema found".to_string()),
    };

    let cursor_wasm: SyncCursorWasm = serde_wasm_bindgen::from_value(sync_cursor)
        .map_err(|e| format!("Failed to parse sync cursor: {}", e))?;

    let session_wasm: SessionWasm = serde_wasm_bindgen::from_value(session)
        .map_err(|e| format!("Failed to parse session: {}", e))?;

    let session_rust = convert_session_wasm_to_rust(&session_wasm);
    let cursor_rust = convert_cursor_wasm_to_rust(&cursor_wasm);

    // Parse rows from JS - expect array of objects
    let rows_vec: Vec<std::collections::HashMap<String, serde_json::Value>> =
        serde_wasm_bindgen::from_value(rows).map_err(|e| format!("Failed to parse rows: {}", e))?;

    match &introspection.schema {
        introspect::SchemaResult::Success { context, .. } => {
            let result = sync::parse_sync_status(&cursor_rust, context, &session_rust, &rows_vec)
                .map_err(|e| format!("{}", e))?;

            Ok(SyncStatusResultWasm {
                tables: result
                    .tables
                    .into_iter()
                    .map(|t| TableSyncStatusWasm {
                        table_name: t.table_name,
                        sync_layer: t.sync_layer,
                        needs_sync: t.needs_sync,
                        max_updated_at: t.max_updated_at,
                        permission_hash: t.permission_hash,
                    })
                    .collect(),
            })
        }
        _ => Err("No schema found".to_string()),
    }
}

/// Generate sync SQL for tables that need syncing
/// Takes raw sync status rows from SQL query execution and parses them internally
pub fn get_sync_sql_wasm(
    status_rows: JsValue,
    sync_cursor: JsValue,
    session: JsValue,
    page_size: usize,
) -> Result<SyncSqlResultWasm, String> {
    let introspection = match cache::get() {
        Some(introspection) => introspection,
        None => return Err("No schema found".to_string()),
    };

    let cursor_wasm: SyncCursorWasm = serde_wasm_bindgen::from_value(sync_cursor)
        .map_err(|e| format!("Failed to parse sync cursor: {}", e))?;

    let session_wasm: SessionWasm = serde_wasm_bindgen::from_value(session)
        .map_err(|e| format!("Failed to parse session: {}", e))?;

    let session_rust = convert_session_wasm_to_rust(&session_wasm);
    let cursor_rust = convert_cursor_wasm_to_rust(&cursor_wasm);

    // Parse rows from JS - expect array of objects
    let rows_vec: Vec<std::collections::HashMap<String, serde_json::Value>> =
        serde_wasm_bindgen::from_value(status_rows)
            .map_err(|e| format!("Failed to parse status rows: {}", e))?;

    match &introspection.schema {
        introspect::SchemaResult::Success { context, .. } => {
            // Parse sync status internally
            let status_rust =
                sync::parse_sync_status(&cursor_rust, context, &session_rust, &rows_vec)
                    .map_err(|e| format!("{}", e))?;

            // Generate sync SQL
            let result = sync::get_sync_sql(
                &status_rust,
                &cursor_rust,
                context,
                &session_rust,
                page_size,
            )
            .map_err(|e| format!("{}", e))?;

            Ok(SyncSqlResultWasm {
                tables: result
                    .tables
                    .into_iter()
                    .map(|t| TableSyncSqlWasm {
                        table_name: t.table_name,
                        permission_hash: t.permission_hash,
                        sql: t.sql,
                        headers: t.headers,
                    })
                    .collect(),
            })
        }
        _ => Err("No schema found".to_string()),
    }
}
