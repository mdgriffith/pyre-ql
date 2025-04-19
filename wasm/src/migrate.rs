use crate::cache;
use log::info;
use pyre::ast;
use pyre::ast::diff;
use pyre::db::introspect;
use pyre::db::migrate;
use pyre::error;
use pyre::parser;
use pyre::typecheck;
use serde::{Deserialize, Serialize};
use serde_wasm_bindgen;
use wasm_bindgen::prelude::*;

const FILEPATH: &str = "schema.pyre";

/**
 * This is a dynamic migration approach.
 * It's used in wasm, so it uses no file operations or database stuff.
 *
 *
 */
pub async fn migrate(
    introspection: &introspect::Introspection,
    schema_source: &str,
) -> Result<String, Vec<error::Error>> {
    // Parse the schema source into a Schema
    let mut schema = ast::Schema::default();
    let parse_result = parser::run("schema.pyre", schema_source, &mut schema);
    if let Err(e) = parse_result {
        return Err(vec![error::Error {
            error_type: error::ErrorType::ParsingError(error::ParsingErrorDetails {
                expecting: error::Expecting::PyreFile,
            }),
            filepath: FILEPATH.to_string(),
            locations: vec![],
        }]);
    }
    let schema_clone = schema.clone();
    // Create a Database from the parsed Schema
    let new_database = ast::Database {
        schemas: vec![schema],
    };

    // Typecheck the new schema
    let context = typecheck::check_schema(&new_database)?;

    // Get the recorded schema from introspection
    let db_recorded_schema = introspection
        .schema
        .as_ref()
        .map_or_else(|| ast::Schema::default(), |schema| schema.clone());

    // info!("Schema: {:#?}", schema_clone);
    // info!("Recorded schema: {:#?}", db_recorded_schema);
    // Diff the schemas and check for errors
    let schema_diff = diff::diff_schema(&db_recorded_schema, &schema_clone);

    let errors = diff::to_errors(schema_diff);
    if !errors.is_empty() {
        return Err(errors);
    }

    // Generate the SQL from the diff
    let db_diff = pyre::db::diff::diff(&context, &schema_clone, introspection);
    let mut sql = pyre::db::diff::to_sql::to_sql(&db_diff);

    match introspection.migration_state {
        introspect::MigrationState::NoMigrationTable => {
            // Create the migration table
            sql.push_str("\n");
            sql.push_str(pyre::db::migrate::CREATE_MIGRATION_TABLE);

            // Create the schema table
            sql.push_str("\n");
            sql.push_str(pyre::db::migrate::CREATE_SCHEMA_TABLE);
        }
        introspect::MigrationState::MigrationTable { .. } => {}
    }

    // Insert the migration
    sql.push_str("\n");
    sql.push_str(pyre::db::migrate::INSERT_MIGRATION);

    Ok(sql)
}

#[derive(Deserialize, Serialize)]
struct MigrateInput {
    introspection: introspect::Introspection,
    schema_source: String,
}

#[derive(Serialize)]
struct MigrateOutput {
    sql: String,
}

#[derive(Serialize)]
struct MigrateError {
    errors: Vec<error::Error>,
}

#[wasm_bindgen]
pub async fn migrate_wasm(introspection: JsValue, schema_source: String) -> String {
    // Get or parse the schema from cache
    let (_, context) = match cache::get_or_parse_schema(introspection.clone()) {
        Ok(result) => result,
        Err(errors) => return serde_json::to_string(&MigrateError { errors }).unwrap(),
    };

    let introspection: introspect::Introspection =
        serde_wasm_bindgen::from_value(introspection).unwrap();
    match migrate(&introspection, &schema_source).await {
        Ok(sql) => serde_json::to_string(&MigrateOutput { sql }).unwrap(),
        Err(errors) => serde_json::to_string(&MigrateError { errors }).unwrap(),
    }
}
