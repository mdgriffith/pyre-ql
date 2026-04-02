use crate::cache;
use crate::sync_deltas::{
    convert_table_group_rust_to_wasm, convert_table_group_wasm_to_rust, AffectedRowTableGroupWasm,
};
use pyre::db::introspect;
use pyre::sync_shape;
use serde_wasm_bindgen;
use wasm_bindgen::prelude::*;

pub fn reshape_sync_table_groups_wasm(
    table_groups: JsValue,
) -> Result<Vec<AffectedRowTableGroupWasm>, String> {
    let introspection = match cache::get() {
        Some(introspection) => introspection,
        None => return Err("No schema found".to_string()),
    };

    let table_groups_wasm: Vec<AffectedRowTableGroupWasm> =
        serde_wasm_bindgen::from_value(table_groups)
            .map_err(|_e| "Failed to parse sync table groups".to_string())?;

    match &introspection.schema {
        introspect::SchemaResult::Success { context, .. } => Ok(sync_shape::reshape_table_groups(
            &table_groups_wasm
                .iter()
                .map(convert_table_group_wasm_to_rust)
                .collect::<Vec<_>>(),
            context,
        )
        .into_iter()
        .map(convert_table_group_rust_to_wasm)
        .collect()),
        _ => Err("No schema found".to_string()),
    }
}
