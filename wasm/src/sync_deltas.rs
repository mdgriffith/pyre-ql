use crate::cache;
use pyre::db::introspect;
use pyre::sync::SessionValue as RustSessionValue;
use pyre::sync_deltas;
use serde::{Deserialize, Serialize};
use serde_wasm_bindgen;
use std::collections::{HashMap, HashSet};
use wasm_bindgen::prelude::*;

// WASM-compatible types for sync deltas
// Grouped format matching SQL output: one entry per table with multiple rows
#[derive(Serialize, Deserialize)]
pub struct AffectedRowTableGroupWasm {
    pub table_name: String,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<serde_json::Value>>, // Array of row arrays, each row array has values matching headers order
}

// Flat format for result output (one entry per row)
#[derive(Serialize, Deserialize)]
pub struct AffectedRowWasm {
    pub table_name: String,
    pub row: serde_json::Value,
    pub headers: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct SessionDataWasm {
    pub session: HashMap<String, SessionValueWasm>,
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

impl From<SessionValueWasm> for RustSessionValue {
    fn from(value: SessionValueWasm) -> Self {
        match value {
            SessionValueWasm::Null => RustSessionValue::Null,
            SessionValueWasm::Integer(i) => RustSessionValue::Integer(i),
            SessionValueWasm::Real(f) => RustSessionValue::Real(f),
            SessionValueWasm::Text(s) => RustSessionValue::Text(s),
            SessionValueWasm::Blob(b) => RustSessionValue::Blob(b),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct AffectedRowGroupWasm {
    pub session_ids: HashSet<String>,
    pub affected_row_indices: Vec<usize>,
}

#[derive(Serialize, Deserialize)]
pub struct SyncDeltasResultWasm {
    pub all_affected_rows: Vec<AffectedRowWasm>,
    pub groups: Vec<AffectedRowGroupWasm>,
}

fn convert_table_group_wasm_to_rust(
    group: &AffectedRowTableGroupWasm,
) -> sync_deltas::AffectedRowTableGroup {
    sync_deltas::AffectedRowTableGroup {
        table_name: group.table_name.clone(),
        headers: group.headers.clone(),
        rows: group.rows.clone(),
    }
}

fn convert_result_rust_to_wasm(result: sync_deltas::SyncDeltasResult) -> SyncDeltasResultWasm {
    SyncDeltasResultWasm {
        all_affected_rows: result
            .all_affected_rows
            .into_iter()
            .map(|row| AffectedRowWasm {
                table_name: row.table_name,
                row: row.row,
                headers: row.headers,
            })
            .collect(),
        groups: result
            .groups
            .into_iter()
            .map(|group| AffectedRowGroupWasm {
                session_ids: group.session_ids,
                affected_row_indices: group.affected_row_indices,
            })
            .collect(),
    }
}

/// Calculate sync deltas for connected sessions based on affected rows from a mutation
/// Accepts grouped format directly from SQL (no transformation needed)
pub fn calculate_sync_deltas_wasm(
    affected_row_groups: JsValue,
    connected_sessions: JsValue,
) -> Result<SyncDeltasResultWasm, String> {
    let introspection = match cache::get() {
        Some(introspection) => introspection,
        None => return Err("No schema found".to_string()),
    };

    // Parse grouped format directly from JavaScript
    let affected_row_groups_wasm: Vec<AffectedRowTableGroupWasm> =
        serde_wasm_bindgen::from_value(affected_row_groups)
            .map_err(|_e| "Failed to parse affected row groups".to_string())?;

    // Accept Map as HashMap<String, SessionDataWasm> where session_id comes from the map key
    let connected_sessions_map: HashMap<String, SessionDataWasm> =
        serde_wasm_bindgen::from_value(connected_sessions)
            .map_err(|_e| "Failed to parse connected sessions".to_string())?;

    match &introspection.schema {
        introspect::SchemaResult::Success { context, .. } => {
            // Convert WASM types to Rust types (grouped format)
            let affected_row_groups_rust: Vec<sync_deltas::AffectedRowTableGroup> =
                affected_row_groups_wasm
                    .iter()
                    .map(convert_table_group_wasm_to_rust)
                    .collect();

            // Convert HashMap<String, SessionDataWasm> directly to HashMap<String, HashMap<String, SessionValue>>
            // Single pass conversion - extract session_id from map key, convert SessionValueWasm to SessionValue
            let connected_sessions_rust: HashMap<String, HashMap<String, RustSessionValue>> =
                connected_sessions_map
                    .into_iter()
                    .map(|(session_id, data)| {
                        let session: HashMap<String, RustSessionValue> = data
                            .session
                            .into_iter()
                            .map(|(k, v)| (k, RustSessionValue::from(v)))
                            .collect();
                        (session_id, session)
                    })
                    .collect();

            // Calculate deltas (now accepts grouped format directly)
            let result = sync_deltas::calculate_sync_deltas(
                &affected_row_groups_rust,
                &connected_sessions_rust,
                context,
            )
            .map_err(|e| match e {
                sync_deltas::SyncDeltasError::TableNotFound(table) => {
                    "Table not found: ".to_string() + &table
                }
                sync_deltas::SyncDeltasError::InvalidRowData(msg) => {
                    "Invalid row data: ".to_string() + &msg
                }
            })?;

            Ok(convert_result_rust_to_wasm(result))
        }
        _ => Err("No schema found".to_string()),
    }
}
