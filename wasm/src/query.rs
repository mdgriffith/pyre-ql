use crate::cache;
use log::info;
use pyre::ast;
use pyre::ast::diff;
use pyre::db::introspect;
use pyre::db::migrate;
use pyre::error;
use pyre::parser;
use pyre::query::run_query;
use pyre::typecheck;
use serde::{Deserialize, Serialize};
use serde_wasm_bindgen;
use std::sync::Arc;
use wasm_bindgen::prelude::*;

const QUERY_FILE: &str = "query.pyre";

/**
 * Dynamically parse a query and return the sql that is generated
 */
pub async fn query(
    context: &Arc<typecheck::Context>,
    query_source: &str,
) -> Result<String, Vec<error::Error>> {
    // Parse query_source into a `ast::QueryList`
    match parser::parse_query(&QUERY_FILE, query_source) {
        Ok(query_list) => {
            // Typecheck and generate
            let typecheck_result =
                typecheck::check_queries(context, &query_list, &mut context.clone());

            match typecheck_result {
                Ok(all_query_info) => {
                    // TODO: Implement query generation
                    Ok("".to_string())
                }
                Err(error_list) => Err(error_list),
            }
        }
        Err(err) => Err(vec![error::Error {
            error_type: error::ErrorType::ParsingError(error::ParsingErrorDetails {
                expecting: error::Expecting::PyreFile,
            }),
            filepath: QUERY_FILE.to_string(),
            locations: vec![],
        }]),
    }
}

#[derive(Serialize)]
struct QueryOutput {
    sql: String,
}

#[derive(Serialize)]
struct QueryError {
    errors: Vec<error::Error>,
}

pub async fn run_query_wasm(query_source: String) -> String {
    let schema = match cache::get_schema() {
        Ok(schema) => schema,
        Err(errors) => return serde_json::to_string(&errors).unwrap(),
    };

    match run_query(&schema, &query_source) {
        Ok(result) => serde_json::to_string(&result).unwrap(),
        Err(errors) => serde_json::to_string(&errors).unwrap(),
    }
}
