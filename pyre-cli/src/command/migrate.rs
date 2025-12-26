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
    let all_schemas = parse_database_schemas(&paths)?;

    let real_namespace = match namespace {
        Some(ns) => ns,
        None => &ast::DEFAULT_SCHEMANAME.to_string(),
    };

    // Get exactly one schema based on namespace or default
    let schema = all_schemas
        .schemas
        .iter()
        .find(|schema| schema.namespace == *real_namespace)
        .ok_or_else(|| {
            eprintln!("Error: No schema found for namespace '{}'", real_namespace);
            std::process::exit(1);
        });
    let schema = schema.unwrap(); // Safe to unwrap after error handling above

    // Typecheck schemas

    match typecheck::check_schema(&all_schemas) {
        Err(error_list) => {
            error::report_and_exit(error_list, &paths);
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
                            println!("{:?}", migration_error);
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
    let all_schemas = parse_database_schemas(&paths)?;

    let real_namespace = match namespace {
        Some(ns) => ns,
        None => &ast::DEFAULT_SCHEMANAME.to_string(),
    };

    // Get exactly one schema based on namespace or default
    let current_schema = all_schemas
        .schemas
        .iter()
        .find(|schema| schema.namespace == *real_namespace)
        .ok_or_else(|| {
            eprintln!("Error: No schema found for namespace '{}'", real_namespace);
            std::process::exit(1);
        });
    let current_schema = current_schema.unwrap(); // Safe to unwrap after error handling above

    // Typecheck schemas

    match typecheck::check_schema(&all_schemas) {
        Err(error_list) => {
            error::report_and_exit(error_list, &paths);
        }
        Ok(context) => {
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
                                    error::report_and_exit(errors, &paths);
                                }

                                // If there are no errors, we can now generate sql.

                                let db_diff = pyre::db::diff::diff(
                                    db_context,
                                    &current_schema,
                                    &introspection,
                                );

                                // Generate sql
                                let sql = pyre::db::diff::to_sql::to_sql(&db_diff);

                                let conn = conn.connect().unwrap();
                                let tx = conn
                                    .transaction_with_behavior(
                                        libsql::TransactionBehavior::Immediate,
                                    )
                                    .await
                                    .unwrap();

                                for sql_statement in sql {
                                    match sql_statement {
                                        pyre::generate::sql::to_sql::SqlAndParams::Sql(sql_string) => {
                                            tx.execute(&sql_string, libsql::params_from_iter::<Vec<libsql::Value>>(vec![])).await.unwrap();
                                        }
                                        pyre::generate::sql::to_sql::SqlAndParams::SqlWithParams { sql, args } => {
                                            tx.execute(&sql, libsql::params_from_iter(args)).await.unwrap();
                                        }
                                    }
                                }

                                tx.commit().await.unwrap();
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
