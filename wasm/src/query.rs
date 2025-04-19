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

const QUERY_FILE: &str = "query.pyre";

/**
 * Dynamically parse a query and return the sql that is generated
 */
pub async fn query(
    context: &typecheck::Context,
    query_source: &str,
) -> Result<String, Vec<error::Error>> {
    // Parse query_source into a `ast::QueryList`
    match parser::parse_query(&QUERY_FILE, query_source) {
        Ok(query_list) => {
            // Typecheck and generate
            let typecheck_result =
                typecheck::check_queries(&context, &query_list, &mut context.clone());

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

#[wasm_bindgen]
pub async fn run_query(introspection: JsValue, query_source: String) -> String {
    // Get or parse the schema from cache
    let (_, context) = match cache::get_or_parse_schema(introspection) {
        Ok(result) => result,
        Err(errors) => return serde_json::to_string(&QueryError { errors }).unwrap(),
    };

    match query(&context, &query_source).await {
        Ok(sql) => serde_json::to_string(&QueryOutput { sql }).unwrap(),
        Err(errors) => serde_json::to_string(&QueryError { errors }).unwrap(),
    }
}
