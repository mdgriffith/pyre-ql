use libsql;
use std::io;
use std::path::Path;

use super::shared::{check_namespace_requirements, parse_database_schemas, Options};
use crate::db;
use pyre::ast;
use pyre::ast::diff;
use pyre::error;
use pyre::typecheck;

pub async fn migrate<'a>(
    options: &'a Options<'a>,
    database: &str,
    auth: &Option<String>,
    migration_dir: &str,
    namespace: &Option<String>,
) -> io::Result<()> {
    check_namespace_requirements(&namespace, &options);
    let namespace_migration_dir = match namespace {
        Some(ns) => Path::new(migration_dir).join(ns),
        None => Path::new(migration_dir).to_path_buf(),
    };

    // Get schema
    let paths = crate::filesystem::collect_filepaths(&options.in_dir)?;
    let all_schemas = parse_database_schemas(&paths, options.enable_color)?;

    let real_namespace = match namespace {
        Some(ns) => ns,
        None => &ast::DEFAULT_SCHEMANAME.to_string(),
    };

    // Get exactly one schema based on namespace or default
    let schema = match all_schemas
        .schemas
        .iter()
        .find(|schema| schema.namespace == *real_namespace)
    {
        Some(s) => s,
        None => {
            eprintln!("Error: No schema found for namespace '{}'", real_namespace);
            std::process::exit(1);
        }
    };

    // Typecheck schemas

    match typecheck::check_schema(&all_schemas) {
        Err(error_list) => {
            error::report_and_exit(error_list, &paths, options.enable_color);
        }
        Ok(_context) => {
            let connection_result = db::connect(&database.to_string(), auth).await;
            match connection_result {
                Ok(conn) => {
                    let migration_result =
                        db::migrate(&conn, &schema, &namespace_migration_dir).await;
                    match migration_result {
                        Ok(()) => {
                            println!("Migration finished!");
                        }
                        Err(migration_error) => {
                            println!("{}", migration_error.format_error());
                        }
                    }
                }
                Err(err) => {
                    println!("{:?}", err);
                }
            }
        }
    }
    Ok(())
}

/**
 * This is the new "dynamic" migration approach
 *
 *
 *
 */
pub async fn push<'a>(
    options: &'a Options<'a>,
    database: &str,
    auth: &Option<String>,

    namespace: &Option<String>,
) -> io::Result<()> {
    check_namespace_requirements(&namespace, &options);

    // Get schema
    let paths = crate::filesystem::collect_filepaths(&options.in_dir)?;
    let all_schemas = parse_database_schemas(&paths, options.enable_color)?;

    let real_namespace = match namespace {
        Some(ns) => ns,
        None => &ast::DEFAULT_SCHEMANAME.to_string(),
    };

    // Get exactly one schema based on namespace or default
    let current_schema = match all_schemas
        .schemas
        .iter()
        .find(|schema| schema.namespace == *real_namespace)
    {
        Some(s) => s,
        None => {
            eprintln!("Error: No schema found for namespace '{}'", real_namespace);
            std::process::exit(1);
        }
    };

    // Typecheck schemas

    match typecheck::check_schema(&all_schemas) {
        Err(error_list) => {
            error::report_and_exit(error_list, &paths, options.enable_color);
        }
        Ok(_context) => {
            let connection_result = db::connect(&database.to_string(), auth).await;
            match connection_result {
                Err(err) => {
                    println!("{:?}", err);
                }
                Ok(conn) => {
                    let introspection_result = crate::db::introspect::introspect(&conn).await;
                    match introspection_result {
                        Ok(introspection) => {
                            if let pyre::db::introspect::SchemaResult::Success {
                                schema: ref db_recorded_schema,
                                context: ref db_context,
                            } = introspection.schema
                            {
                                let schema_diff =
                                    diff::diff_schema(&current_schema, db_recorded_schema);

                                // We diff the two schemas and report errors.

                                let errors = diff::to_errors(schema_diff);
                                if !errors.is_empty() {
                                    error::report_and_exit(errors, &paths, options.enable_color);
                                }

                                // If there are no errors, we can now generate sql.

                                let db_diff = pyre::db::diff::diff(
                                    db_context,
                                    &current_schema,
                                    &introspection,
                                );

                                // Generate sql
                                let sql = pyre::db::diff::to_sql::to_sql(&db_diff);

                                match conn.connect() {
                                    Ok(connected_conn) => {
                                        match connected_conn
                                            .transaction_with_behavior(
                                                libsql::TransactionBehavior::Immediate,
                                            )
                                            .await
                                        {
                                            Ok(tx) => {
                                                let mut has_error = false;
                                                for sql_statement in sql {
                                                    match sql_statement {
                                                        pyre::generate::sql::to_sql::SqlAndParams::Sql(sql_string) => {
                                                            if let Err(e) = tx.execute(&sql_string, libsql::params_from_iter::<Vec<libsql::Value>>(vec![])).await {
                                                                eprintln!("Error executing SQL: {:?}", e);
                                                                eprintln!("SQL statement: {}", sql_string);
                                                                has_error = true;
                                                                break;
                                                            }
                                                        }
                                                        pyre::generate::sql::to_sql::SqlAndParams::SqlWithParams { sql, args } => {
                                                            if let Err(e) = tx.execute(&sql, libsql::params_from_iter(args)).await {
                                                                eprintln!("Error executing SQL: {:?}", e);
                                                                eprintln!("SQL statement: {}", sql);
                                                                has_error = true;
                                                                break;
                                                            }
                                                        }
                                                    }
                                                }

                                                if has_error {
                                                    eprintln!("Migration failed due to SQL execution errors. Database may be in an inconsistent state.");
                                                    std::process::exit(1);
                                                }

                                                if let Err(e) = tx.commit().await {
                                                    eprintln!(
                                                        "Error committing transaction: {:?}",
                                                        e
                                                    );
                                                    eprintln!("Migration failed. Database may be in an inconsistent state.");
                                                    std::process::exit(1);
                                                }
                                            }
                                            Err(e) => {
                                                eprintln!("Error creating transaction: {:?}", e);
                                                eprintln!("Migration failed. Database may be in an inconsistent state.");
                                                std::process::exit(1);
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        eprintln!("Error connecting to database: {:?}", e);
                                        eprintln!("Migration failed. Could not establish database connection.");
                                        std::process::exit(1);
                                    }
                                }
                            } else {
                                println!(
                                    "No schema found in the databasefor namespace '{}'",
                                    real_namespace
                                );
                                std::process::exit(1);
                            }
                        }
                        Err(err) => {
                            println!("{:?}", err);
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
