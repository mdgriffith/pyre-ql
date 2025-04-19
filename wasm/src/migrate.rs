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
use std::sync::Arc;
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
    new_schema_source: &str,
) -> Result<String, Vec<error::Error>> {
    // First, parse and typecheck the new schema

    // Parse the schema source into a Schema
    let mut new_schema = ast::Schema::default();
    let parse_result = parser::run("schema.pyre", new_schema_source, &mut new_schema);
    if let Err(e) = parse_result {
        return Err(vec![error::Error {
            error_type: error::ErrorType::ParsingError(error::ParsingErrorDetails {
                expecting: error::Expecting::PyreFile,
            }),
            filepath: FILEPATH.to_string(),
            locations: vec![],
        }]);
    }
    let schema_clone = new_schema.clone();
    // Create a Database from the parsed Schema
    let new_database = ast::Database {
        schemas: vec![new_schema],
    };

    // Typecheck the new schema
    let new_context = typecheck::check_schema(&new_database)?;

    // Get the recorded schema from introspection
    let (db_recorded_schema, db_recorded_context) = match &introspection.schema {
        introspect::SchemaResult::FailedToParse { source, errors } => {
            return Err(errors.clone());
        }
        introspect::SchemaResult::FailedToTypecheck { errors, .. } => {
            return Err(errors.clone());
        }
        introspect::SchemaResult::Success { schema, context } => (schema, context),
    };

    // Diff the schemas and check for errors
    let schema_diff = diff::diff_schema(&db_recorded_schema, &schema_clone);

    let errors = diff::to_errors(schema_diff);
    if !errors.is_empty() {
        return Err(errors);
    }

    // Generate the SQL from the diff
    let db_diff = pyre::db::diff::diff(&new_context, &db_recorded_schema, &introspection);
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

#[derive(Serialize)]
struct MigrateError {
    errors: Vec<error::Error>,
}

#[wasm_bindgen]
pub async fn migrate_wasm(schema_source: String) -> String {
    let introspection = match cache::get() {
        Some(introspection) => introspection,
        None => {
            return serde_json::to_string(&MigrateError {
                errors: vec![error::Error {
                    error_type: error::ErrorType::MigrationMissingSchema,
                    filepath: "".to_string(),
                    locations: vec![],
                }],
            })
            .unwrap()
        }
    };

    match migrate(&introspection, &schema_source).await {
        Ok(result) => serde_json::to_string(&result).unwrap(),
        Err(errors) => serde_json::to_string(&errors).unwrap(),
    }
}
