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
use std::collections::HashMap;
use std::sync::Arc;
use wasm_bindgen::prelude::*;

const QUERY_FILE: &str = "query.pyre";

/**
 * Dynamically parse a query and return the sql that is generated
 */
pub async fn run_query(
    context: &typecheck::Context,
    query_source: &str,
) -> Result<String, Vec<error::Error>> {
    match parser::parse_query(&QUERY_FILE, query_source) {
        Ok(query_list) => {
            let query_list: ast::QueryList = query_list;

            // Find the first query in the list
            // We're only running exactly one query in this context.
            let mut found_query = None;
            for query_def in &query_list.queries {
                match query_def {
                    ast::QueryDef::Query(query) => {
                        if found_query.is_some() {
                            // Found more than one query
                            return Err(vec![error::Error {
                                error_type: error::ErrorType::ParsingError(
                                    error::ParsingErrorDetails {
                                        expecting: error::Expecting::PyreFile,
                                    },
                                ),
                                filepath: QUERY_FILE.to_string(),
                                locations: vec![],
                            }]);
                        }
                        found_query = Some(query);
                    }
                    _ => continue,
                }
            }

            // Extract the query or return error if none found
            match found_query {
                Some(query) => {
                    let mut errors = Vec::new();
                    // Typecheck and generate
                    let query_info: typecheck::QueryInfo =
                        typecheck::check_query(context, &mut errors, &query);

                    if errors.len() > 0 {
                        return Err(errors);
                    }

                    let mut sql = String::new();
                    for field in &query.fields {
                        match field {
                            ast::TopLevelQueryField::Field(query_field) => {
                                let table = context.tables.get(&query_field.name).unwrap();
                                let prepared = pyre::generate::sql::to_string(
                                    context,
                                    query,
                                    &query_info,
                                    table,
                                    query_field,
                                );
                                for prepared in prepared {
                                    sql.push_str(&prepared.sql);
                                }
                            }
                            _ => (),
                        }
                    }
                    Ok(sql)
                }
                None => {
                    return Err(vec![error::Error {
                        error_type: error::ErrorType::ParsingError(error::ParsingErrorDetails {
                            expecting: error::Expecting::PyreFile,
                        }),
                        filepath: QUERY_FILE.to_string(),
                        locations: vec![],
                    }]);
                }
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
    let introspection = match cache::get() {
        Some(introspection) => introspection,
        None => {
            return serde_json::to_string(&error::Error {
                error_type: error::ErrorType::MigrationMissingSchema,
                filepath: "".to_string(),
                locations: vec![],
            })
            .unwrap()
        }
    };

    match &introspection.schema {
        introspect::SchemaResult::Success { context, .. } => {
            match run_query(&context, &query_source).await {
                Ok(result) => serde_json::to_string(&result).unwrap(),
                Err(errors) => serde_json::to_string(&errors).unwrap(),
            }
        }
        _ => serde_json::to_string(&error::Error {
            error_type: error::ErrorType::MigrationMissingSchema,
            filepath: "".to_string(),
            locations: vec![],
        })
        .unwrap(),
    }
}
