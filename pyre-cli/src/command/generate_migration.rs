use chrono;
use colored::*;
use std::fs;
use std::io;
use std::path::Path;

use super::shared::{check_namespace_requirements, parse_database_schemas, Options};
use crate::db;
use pyre::ast;
use pyre::db::introspect::MigrationState;
use pyre::error;
use pyre::filesystem;
use pyre::typecheck;

pub async fn generate_migration<'a>(
    options: &'a Options<'a>,
    name: &str,
    db: &str,
    auth: &Option<String>,
    migration_dir: &Path,
    namespace: &Option<String>,
) -> io::Result<()> {
    check_namespace_requirements(&namespace, &options);

    let target_namespace = namespace
        .clone()
        .unwrap_or_else(|| ast::DEFAULT_SCHEMANAME.to_string());

    let target_namespace_dir = match namespace {
        None => migration_dir,
        Some(name) => &migration_dir.join(&name),
    };

    let connection_result = db::connect(&db.to_string(), auth).await;
    match connection_result {
        Err(e) => {
            println!("Failed to connect to database: {:?}", e);
        }
        Ok(conn) => {
            let introspection_result = db::introspect::introspect(&conn).await;
            match introspection_result {
                Ok(introspection) => {
                    let existing_migrations =
                        db::read_migration_items(target_namespace_dir).unwrap_or(vec![]);

                    let not_applied: Vec<String> = existing_migrations
                        .iter()
                        .filter(|migration| match &introspection.migration_state {
                            MigrationState::NoMigrationTable => true,
                            MigrationState::MigrationTable { migrations } => {
                                !migrations.iter().any(|m| m.name == **migration)
                            }
                        })
                        .map(|m| m.yellow().to_string())
                        .collect();

                    if !not_applied.is_empty() {
                        println!(
                            "\nIt looks like some migrations have not been applied:\n\n    {}",
                            not_applied.join("\n   ")
                        );
                        println!("\nRun `pyre migrate` to apply these migrations before generating a new one.");
                        return Ok(());
                    }

                    let paths = crate::filesystem::collect_filepaths(&options.in_dir)?;
                    let current_db = parse_database_schemas(&paths)?;

                    match typecheck::check_schema(&current_db) {
                        Ok(context) => {
                            let current_schema = current_db
                                .schemas
                                .iter()
                                .find(|s| s.namespace == target_namespace)
                                .expect("Schema not found");

                            let db_diff =
                                pyre::db::diff::diff(&context, &current_schema, &introspection);

                            println!("DB Diff: {:#?}", db_diff);

                            crate::filesystem::create_dir_if_not_exists(migration_dir)?;

                            let current_date = chrono::Utc::now().format("%Y%m%d%H%M").to_string();
                            let migration_folder =
                                target_namespace_dir.join(format!("{}_{}", current_date, name));
                            crate::filesystem::create_dir_if_not_exists(&migration_folder)?;

                            let migration_file = migration_folder.join("migration.sql");
                            let diff_file = migration_folder.join("schema.diff");

                            let sql = pyre::db::diff::to_sql::to_sql(&db_diff);

                            let mut all_sql_as_string = String::new();
                            for sql_statement in sql {
                                match sql_statement {
                                    pyre::generate::sql::to_sql::SqlAndParams::Sql(sql_string) => {
                                        all_sql_as_string.push_str(&sql_string.clone());
                                        all_sql_as_string.push_str(";\n");
                                    }
                                    pyre::generate::sql::to_sql::SqlAndParams::SqlWithParams {
                                        sql,
                                        args,
                                    } => {
                                        all_sql_as_string.push_str(&sql);
                                        // for arg in args {
                                        //     all_sql_as_string.push_str(&arg);
                                        // }
                                        all_sql_as_string.push_str(";\n");
                                    }
                                }
                            }
                            fs::write(&migration_file, all_sql_as_string)?;
                            let json_diff = serde_json::to_string(&db_diff)?;
                            fs::write(&diff_file, json_diff)?;
                        }
                        Err(error_list) => {
                            for err in error_list {
                                let schema_source =
                                    filesystem::get_schema_source(&err.filepath, &paths)
                                        .unwrap_or("");

                                let formatted_error = error::format_error(&schema_source, &err);
                                eprintln!("{}", &formatted_error);
                            }
                            std::process::exit(1);
                        }
                    }
                }
                Err(err) => {
                    println!("Failed to introspect database: {:?}", err);
                }
            }
        }
    }
    Ok(())
}
