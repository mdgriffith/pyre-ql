use crate::cache;
use log::info;
use pyre::ast;
use pyre::ast::diff;
use pyre::db::introspect;
use pyre::error;
use pyre::parser;
use pyre::typecheck;
use serde::{Deserialize, Serialize};
use serde_wasm_bindgen;
use std::sync::Arc;
use wasm_bindgen::prelude::*;
use web_sys::console;

const FILEPATH: &str = "schema.pyre";

/* How migrations should work:

1. Introspect the database

- Database is completely empty?
  - Run the desired migration which will auto-create
    the migration table and schema table.

- Database has no migration table or schema table

- Database failed to parse


- Parse and typecheck the new schema.





*/

#[derive(Serialize)]
pub struct MigrationSql {
    pub sql: Vec<SqlAndParams>,
    pub mark_success: SqlAndParams,
    pub mark_failure: SqlAndParams,
}

/**
 * This is a dynamic migration approach.
 * It's used in wasm, so it uses no file operations or database stuff.
 *
 *
 */
pub async fn migrate(
    name: String,
    introspection: &introspect::Introspection,
    new_schema_source: &str,
) -> Result<MigrationSql, Vec<error::Error>> {
    // First, parse and typecheck the new schema

    // Parse the schema source into a Schema
    let mut new_schema = ast::Schema::default();
    let parse_result = parser::run("schema.pyre", new_schema_source, &mut new_schema);
    if let Err(e) = parse_result {
        return match parser::convert_parsing_error(e) {
            Some(error) => Err(vec![error]),
            None => Err(vec![error::Error {
                error_type: error::ErrorType::ParsingError(error::ParsingErrorDetails {
                    expecting: error::Expecting::PyreFile,
                }),
                filepath: FILEPATH.to_string(),
                locations: vec![],
            }]),
        };
    }
    let new_schema_clone = new_schema.clone();
    // Create a Database from the parsed Schema
    let new_database = ast::Database {
        schemas: vec![new_schema],
    };

    // Typecheck the new schema
    let new_context = typecheck::check_schema(&new_database)?;

    // Get the recorded schema from introspection
    let (db_recorded_schema, _db_recorded_context) = match &introspection.schema {
        introspect::SchemaResult::FailedToParse { errors, .. } => {
            return Err(errors.clone());
        }
        introspect::SchemaResult::FailedToTypecheck { errors, .. } => {
            return Err(errors.clone());
        }
        introspect::SchemaResult::Success { schema, context } => (schema, context),
    };

    // Diff the schemas and check for errors
    let schema_diff = diff::diff_schema(&db_recorded_schema, &new_schema_clone);

    let errors = diff::to_errors(schema_diff);
    if !errors.is_empty() {
        return Err(errors);
    }

    // Generate the SQL from the diff
    let db_diff = pyre::db::diff::diff(&new_context, &new_schema_clone, &introspection);

    // Log the db_diff to console
    console::log_1(&serde_wasm_bindgen::to_value(&db_diff).unwrap());
    let diff_sql = pyre::db::diff::to_sql::to_sql(&db_diff);

    if pyre::db::diff::is_empty(&db_diff) {
        return Ok(MigrationSql {
            sql: vec![],
            mark_success: SqlAndParams::SqlWithParams {
                sql: pyre::db::migrate::INSERT_MIGRATION_SUCCESS.to_string(),
                args: vec![name.to_string(), "".to_string()],
            },
            mark_failure: SqlAndParams::SqlWithParams {
                sql: pyre::db::migrate::INSERT_MIGRATION_ERROR.to_string(),
                // They'll need to add the error message
                args: vec![name.to_string(), "".to_string()],
            },
        });
    }

    let mut sql_executed = String::new();
    let mut sql = Vec::new();
    for sql_statement in diff_sql {
        sql.push(SqlAndParams::Sql(sql_statement.clone()));
        sql_executed.push_str(&sql_statement);
        sql_executed.push_str(";\n");
    }

    match introspection.migration_state {
        introspect::MigrationState::NoMigrationTable => {
            // Create the migration table
            sql.push(SqlAndParams::Sql(
                pyre::db::migrate::CREATE_MIGRATION_TABLE.to_string(),
            ));

            // Create the schema table
            sql.push(SqlAndParams::Sql(
                pyre::db::migrate::CREATE_SCHEMA_TABLE.to_string(),
            ));
        }
        introspect::MigrationState::MigrationTable { .. } => {}
    }
    sql.push(SqlAndParams::SqlWithParams {
        sql: pyre::db::migrate::INSERT_SCHEMA.to_string(),
        args: vec![new_schema_source.to_string()],
    });

    Ok(MigrationSql {
        sql,
        mark_success: SqlAndParams::SqlWithParams {
            sql: pyre::db::migrate::INSERT_MIGRATION_SUCCESS.to_string(),
            args: vec![name.to_string(), sql_executed.clone()],
        },
        mark_failure: SqlAndParams::SqlWithParams {
            sql: pyre::db::migrate::INSERT_MIGRATION_ERROR.to_string(),
            // They'll need to add the error message
            args: vec![name.to_string(), sql_executed.clone()],
        },
    })
}

#[derive(Serialize)]
#[serde(untagged)]
pub enum SqlAndParams {
    Sql(String),
    SqlWithParams { sql: String, args: Vec<String> },
}

// #[wasm_bindgen]
pub async fn migrate_wasm(
    name: String,
    schema_source: String,
) -> Result<MigrationSql, Vec<error::Error>> {
    let introspection = match cache::get() {
        Some(introspection) => introspection,
        None => {
            return Err(vec![error::Error {
                error_type: error::ErrorType::MigrationMissingSchema,
                filepath: "".to_string(),
                locations: vec![],
            }]);
        }
    };

    migrate(name, &introspection, &schema_source).await
}
