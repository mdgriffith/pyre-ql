use crate::cache;
use pyre::ast;
use pyre::parser;
use pyre::seed;
use pyre::typecheck;
use serde::{Deserialize, Serialize};
use serde_wasm_bindgen;
use wasm_bindgen::prelude::*;

const FILEPATH: &str = "schema.pyre";

#[derive(Serialize, Deserialize)]
pub struct SeedOptions {
    pub seed: Option<u64>,
    pub default_rows_per_table: Option<usize>,
    pub table_rows: Option<std::collections::HashMap<String, usize>>,
    pub foreign_key_ratios: Option<std::collections::HashMap<(String, String), f64>>,
    pub default_foreign_key_ratio: Option<f64>,
}

#[derive(Serialize)]
pub struct SeedSql {
    pub sql: Vec<String>,
}

pub fn seed_wasm(schema_source: String, options: Option<SeedOptions>) -> Result<SeedSql, String> {
    // Parse the schema source
    let mut schema = ast::Schema::default();
    let parse_result = parser::run(FILEPATH, &schema_source, &mut schema);
    if let Err(e) = parse_result {
        return Err(format!("Failed to parse schema: {:?}", e));
    }

    // Create a Database from the parsed Schema
    let database = ast::Database {
        schemas: vec![schema.clone()],
    };

    // Typecheck the schema to get context
    let context = match typecheck::check_schema(&database) {
        Ok(ctx) => ctx,
        Err(errors) => {
            return Err(format!(
                "Failed to typecheck schema: {:?}",
                errors
                    .iter()
                    .map(|e| format!("{:?}", e))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    };

    // Convert WASM options to Rust options
    let rust_options = options.map(|opts| seed::Options {
        seed: opts.seed,
        default_rows_per_table: opts.default_rows_per_table.unwrap_or(1000),
        table_rows: opts.table_rows.unwrap_or_default(),
        foreign_key_ratios: opts.foreign_key_ratios.unwrap_or_default(),
        default_foreign_key_ratio: opts.default_foreign_key_ratio.unwrap_or(5.0),
    });

    // Generate seed SQL
    let sql_operations = seed::seed_database(&schema, &context, rust_options);

    // Convert to simple string vector
    let sql: Vec<String> = sql_operations.iter().map(|op| op.sql.clone()).collect();

    Ok(SeedSql { sql })
}
