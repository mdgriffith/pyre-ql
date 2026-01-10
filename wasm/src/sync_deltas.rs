use crate::cache;
use pyre::db::introspect;
use pyre::sync::SessionValue as RustSessionValue;
use pyre::sync_deltas;
use serde::{Deserialize, Serialize};
use serde_wasm_bindgen;
use std::collections::{HashMap, HashSet};
use wasm_bindgen::prelude::*;

// WASM-compatible types for sync deltas
#[derive(Serialize, Deserialize)]
pub struct AffectedRowWasm {
    pub table_name: String,
    pub row: serde_json::Value,
    pub headers: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct ConnectedSessionWasm {
    pub session_id: String,
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

fn convert_session_wasm_to_rust(
    session: &ConnectedSessionWasm,
) -> (String, HashMap<String, RustSessionValue>) {
    let session_rust = session
        .fields
        .iter()
        .map(|(k, v)| (k.clone(), RustSessionValue::from((*v).clone())))
        .collect();
    (session.session_id.clone(), session_rust)
}

fn convert_affected_row_wasm_to_rust(row: &AffectedRowWasm) -> sync_deltas::AffectedRow {
    sync_deltas::AffectedRow {
        table_name: row.table_name.clone(),
        row: row.row.clone(),
        headers: row.headers.clone(),
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
pub fn calculate_sync_deltas_wasm(
    affected_rows: JsValue,
    connected_sessions: JsValue,
) -> Result<SyncDeltasResultWasm, String> {
    let introspection = match cache::get() {
        Some(introspection) => introspection,
        None => return Err("No schema found".to_string()),
    };

    let affected_rows_wasm: Vec<AffectedRowWasm> =
        serde_wasm_bindgen::from_value(affected_rows)
            .map_err(|_e| "Failed to parse affected rows".to_string())?;

    let connected_sessions_wasm: Vec<ConnectedSessionWasm> =
        serde_wasm_bindgen::from_value(connected_sessions)
            .map_err(|_e| "Failed to parse connected sessions".to_string())?;

    match &introspection.schema {
        introspect::SchemaResult::Success { context, .. } => {
            // Convert WASM types to Rust types
            let affected_rows_rust: Vec<sync_deltas::AffectedRow> = affected_rows_wasm
                .iter()
                .map(convert_affected_row_wasm_to_rust)
                .collect();

            let connected_sessions_rust: Vec<sync_deltas::ConnectedSession> =
                connected_sessions_wasm
                    .iter()
                    .map(|s| {
                        let (session_id, session_values) = convert_session_wasm_to_rust(s);
                        sync_deltas::ConnectedSession {
                            session_id,
                            session: session_values,
                        }
                    })
                    .collect();

            // Calculate deltas
            let result = sync_deltas::calculate_sync_deltas(
                &affected_rows_rust,
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

