use crate::cache;
use log::info;
use pyre::ast;
use pyre::ast::diff;
use pyre::db::introspect;
use pyre::db::migrate;
use pyre::error;
use pyre::generate::sql::to_sql::SqlAndParams;
use pyre::parser;
use pyre::typecheck;
use serde::{Deserialize, Serialize};
use serde_wasm_bindgen;
use std::collections::HashMap;
// use std::sync::Arc;
use wasm_bindgen::prelude::*;
use web_sys::console;

const QUERY_FILE: &str = "query.pyre";

/**
 * Dynamically parse a query and return the sql that is generated
 */
pub fn query_to_sql(
    context: &typecheck::Context,
    query_source: &str,
) -> Result<Vec<SqlAndParams>, Vec<error::Error>> {
    match parser::parse_query(&QUERY_FILE, query_source) {
        Ok(query_list) => {
            let query_list: ast::QueryList = query_list;

            console::log_1(&serde_wasm_bindgen::to_value("Parsed").unwrap());

            // Find the first query in the list
            // We're only running exactly one query in this context.
            let mut found_query = None;
            for query_def in &query_list.queries {
                match query_def {
                    ast::QueryDef::Query(query) => {
                        if found_query.is_some() {
                            console::log_1(
                                &serde_wasm_bindgen::to_value("More than one query").unwrap(),
                            );
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

                    let mut sql = Vec::new();
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
                                    sql.push(SqlAndParams::Sql(prepared.sql));
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
        Err(err) => match parser::convert_parsing_error(err) {
            Some(error) => Err(vec![error]),
            None => Err(vec![error::Error {
                error_type: error::ErrorType::ParsingError(error::ParsingErrorDetails {
                    expecting: error::Expecting::PyreFile,
                }),
                filepath: QUERY_FILE.to_string(),
                locations: vec![],
            }]),
        },
    }
}

pub fn query_to_sql_wasm(query_source: String) -> Result<Vec<SqlAndParams>, Vec<String>> {
    let introspection = match cache::get() {
        Some(introspection) => introspection,
        None => {
            return Err(vec!["No schema found".to_string()]);
        }
    };

    match &introspection.schema {
        introspect::SchemaResult::Success { context, .. } => {
            match query_to_sql(&context, &query_source) {
                Ok(result) => Ok(result),
                Err(errors) => {
                    let mut formatted_errors = Vec::new();
                    for error in errors {
                        formatted_errors.push(error::format_error(&query_source, &error));
                    }
                    Err(formatted_errors)
                }
            }
        }
        _ => Err(vec!["No schema found".to_string()]),
    }
}
